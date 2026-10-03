//! Dynamic-name namespaces from the stores that flag commands reach.
use std::collections::{BTreeMap, BTreeSet};

use sha2::{Digest, Sha256};

use super::Native;
use super::questions::{error, scope_references};
use crate::engine::analysis::declarations::ScopeType;
use crate::engine::analysis::dynamic_names::{
    self, CommandNames, NameOutcome, Role, routes::Route,
};
use crate::{
    Answer, Basis, BuildId, CommandReference, Completeness, DynamicNameForm, DynamicNameKind,
    DynamicNamespace, DynamicNamespaceId, Error, Gap, GapKind, GapSubject, NamespaceOwner,
    Operation, Source,
};

impl Native {
    /// Return the stores of dynamic names, each with the commands that define, remove and read
    /// names in it.
    ///
    /// The search covers integer flags: every effect and trigger whose assign or member reader
    /// stores an interned flag index. Two commands share a namespace only when both reach the
    /// same store. Saved event targets and variables are outside it.
    pub fn dynamic_names(&self) -> Result<Answer<Vec<DynamicNamespace>>, Error> {
        self.answer("dynamic_names", None, || {
            let commands = self.dynamic_name_commands()?;
            Ok(normalize(&commands, self.build()))
        })
    }

    /// The method's result for every registered effect and trigger.
    pub(crate) fn dynamic_name_commands(&self) -> Result<Vec<CommandNames>, Error> {
        let operation = Operation::DynamicNames;
        let input = self
            .declaration_analysis(operation)?
            .dynamic_name_input()
            .map_err(|failure| error(operation, failure))?;

        Ok(dynamic_names::analyze(&input))
    }
}

/// One store: its owner and its normalized route. Addresses stay inside the key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum StoreKey {
    Global {
        address: u64,
        offset: u64,
    },
    Scope {
        bit: usize,
        name: String,
        terminal: u64,
        offset: u64,
    },
}

impl StoreKey {
    fn new(route: Route, scope: &ScopeType) -> Self {
        match route {
            Route::Global { address, offset } => Self::Global { address, offset },
            Route::Scope { terminal, offset } => Self::Scope {
                bit: scope.bit,
                name: scope.name.clone(),
                terminal,
                offset,
            },
        }
    }

    fn id(&self) -> DynamicNamespaceId {
        let text = match self {
            Self::Global { address, offset } => {
                format!("dynamic-names/global/{address:x}/{offset:x}")
            }
            Self::Scope {
                bit,
                terminal,
                offset,
                ..
            } => format!("dynamic-names/scope/{bit}/{terminal:x}/{offset:x}"),
        };
        let digest = Sha256::digest(text.as_bytes());

        DynamicNamespaceId(format!("{digest:x}")[..16].to_owned())
    }

    fn owner(&self) -> NamespaceOwner {
        match self {
            Self::Global { .. } => NamespaceOwner::Global,
            Self::Scope { bit, name, .. } => {
                let scope = ScopeType {
                    bit: *bit,
                    name: name.clone(),
                };
                let [reference] = scope_references(std::slice::from_ref(&scope))
                    .try_into()
                    .expect("one scope gives one reference");
                NamespaceOwner::Scope(reference)
            }
        }
    }
}

/// The commands of one store by role, and the commands of each form.
#[derive(Default)]
struct Members {
    roles: BTreeMap<Role, Vec<CommandReference>>,
    /// Each form with its commands as `kind name`.
    forms: BTreeMap<DynamicNameForm, BTreeSet<String>>,
}

fn normalize(commands: &[CommandNames], build: BuildId) -> Answer<Vec<DynamicNamespace>> {
    let mut stores = BTreeMap::<StoreKey, Members>::new();
    let mut gaps = Vec::new();
    for command in commands {
        let flag = match &command.outcome {
            NameOutcome::NotFlag => continue,
            NameOutcome::NotExamined(stop) => {
                let detail = format!(
                    "was not examined: its command object was not established ({})",
                    stop.reason
                );
                gaps.push(command_gap(command, GapKind::UnresolvedReader, &detail));
                continue;
            }
            NameOutcome::Unresolved(stop) => {
                let detail = obstacle(stop.reason);
                gaps.push(command_gap(command, GapKind::UnresolvedReader, &detail));
                continue;
            }
            NameOutcome::Flag(flag) => flag,
        };
        for stop in &flag.stops {
            let detail = obstacle(stop.reason);
            gaps.push(command_gap(command, GapKind::ReaderSemantics, &detail));
        }
        for role_use in &flag.uses {
            let route = match &role_use.route {
                Ok(route) => *route,
                Err(stop) => {
                    let detail = format!(
                        "reaches a {} store whose route was not established ({})",
                        role_use.scope.name, stop.reason
                    );
                    gaps.push(command_gap(command, GapKind::UnresolvedStorage, &detail));
                    continue;
                }
            };
            let members = stores
                .entry(StoreKey::new(route, &role_use.scope))
                .or_default();
            members
                .roles
                .entry(role_use.role)
                .or_default()
                .push(CommandReference {
                    kind: command.kind,
                    name: command.name.clone(),
                });
            members.forms.entry(flag.form).or_default().insert(format!(
                "{} {}",
                command.kind.subject(),
                command.name
            ));
        }
    }
    gaps.dedup();

    let mut value = Vec::new();
    for (key, members) in &stores {
        let namespace = namespace(key, members);
        gaps.extend(form_gap(&namespace, members));
        value.push(namespace);
    }
    value.sort_by(|left, right| {
        owner_order(&left.owner)
            .cmp(&owner_order(&right.owner))
            .then_with(|| left.id.cmp(&right.id))
    });
    gaps.extend(outside_method());

    Answer {
        value,
        completeness: Completeness::from_gaps(&gaps),
        gaps,
        source: Source::new(build, dynamic_names::METHOD, Basis::StaticAnalysis),
    }
}

fn namespace(key: &StoreKey, members: &Members) -> DynamicNamespace {
    let commands = |role| {
        let mut commands = members.roles.get(&role).cloned().unwrap_or_default();
        commands.sort_by(|left, right| {
            (left.kind.subject(), &left.name).cmp(&(right.kind.subject(), &right.name))
        });
        commands.dedup();
        commands
    };
    let dynamic_form = match members.forms.keys().collect::<Vec<_>>()[..] {
        [&form] => form,
        _ => DynamicNameForm::Unresolved,
    };

    DynamicNamespace {
        id: key.id(),
        kind: DynamicNameKind::IntegerFlag,
        owner: key.owner(),
        defined_by: commands(Role::Defines),
        removed_by: commands(Role::Removes),
        read_by: commands(Role::Reads),
        dynamic_form,
    }
}

/// A gap when the commands of `namespace` disagree on whether a name accepts `name@target`.
fn form_gap(namespace: &DynamicNamespace, members: &Members) -> Option<Gap> {
    if members.forms.len() < 2 {
        return None;
    }
    let groups: Vec<String> = members
        .forms
        .iter()
        .map(|(form, commands)| {
            let form = match form {
                DynamicNameForm::TargetSuffix => "accepted by",
                DynamicNameForm::NotAccepted => "refused by",
                DynamicNameForm::Unresolved => "not established for",
            };
            let commands: Vec<&str> = commands.iter().map(String::as_str).collect();
            format!("{form} {}", commands.join(", "))
        })
        .collect();

    Some(Gap {
        kind: GapKind::ReaderSemantics,
        subject: Some(GapSubject::answer_item(namespace.id.0.clone())),
        detail: format!(
            "The namespace's commands disagree on name@target: {}.",
            groups.join("; ")
        ),
    })
}

/// Global stores first, then scope stores by scope name.
fn owner_order(owner: &NamespaceOwner) -> (u8, &str) {
    match owner {
        NamespaceOwner::Global => (0, ""),
        NamespaceOwner::Scope(scope) => (1, scope.name.as_str()),
    }
}

/// A gap about `command`; `detail` completes a sentence whose subject is the command.
fn command_gap(command: &CommandNames, kind: GapKind, detail: &str) -> Gap {
    Gap {
        kind,
        subject: Some(GapSubject::answer_item(command.name.clone())),
        detail: format!("{} {detail}", command.kind.subject()),
    }
}

/// What a method reason means for the command, in public words.
fn obstacle(reason: &str) -> String {
    let text = match reason {
        "index-store" => {
            "calls the flag name reader or interner, but no stored flag index was established"
        }
        "index-stores" => "stores its interned flag index at more than one place",
        "assign-slot" => {
            "has no assign reader, so whether it stores a flag name was not established"
        }
        "reader-code" => {
            "has an assign or member reader whose code was not read, so whether it stores a flag name was not established"
        }
        "no-role" => "stores a flag name, but no define, remove or read role was established",
        "role-slot" | "role-code" => "has no readable execute or evaluate code",
        "role-store" => "passes a flag store that did not come from its flag accessor",
        "role-flag" => "passes a flag that did not load from its stored flag index",
        "read-index" => "tests a flag index other than the one that it stores",
        "accessor-slot" => "has no known flag accessor in the slot that its test calls",
        "dynamic-form" => "was not established to accept or refuse name@target",
        "scope-set" => "has no established scope set, so no store was followed",
        reason => return format!("was not followed to its flag store ({reason})"),
    };

    text.to_owned()
}

fn outside_method() -> [Gap; 3] {
    let gap = |detail: &str| Gap {
        kind: GapKind::OutsideMethod,
        subject: None,
        detail: detail.into(),
    };

    [
        gap(
            "The search covers every effect and trigger whose assign or member reader stores an interned flag index. Flags that the engine sets without such a command are outside it.",
        ),
        gap(
            "Saved event targets are outside the method: the engine interns their names in the flag table, but keeps them with the saved event targets of a scope or of the game state, not in a flag store.",
        ),
        gap(
            "Variables are outside the method: the engine keeps them in a table keyed by the name's text, which does not intern names.",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DeclarationKind;
    use crate::engine::analysis::dynamic_names::{FlagCommand, RoleUse};
    use crate::engine::analysis::stop::Unresolved;

    const TERMINAL: u64 = 0x3c00;
    const GLOBAL: Route = Route::Global {
        address: 0x60_0450,
        offset: 0x478,
    };

    fn scope(bit: usize, name: &str) -> ScopeType {
        ScopeType {
            bit,
            name: name.into(),
        }
    }

    fn flag_command(
        kind: DeclarationKind,
        name: &str,
        role: Role,
        routes: Vec<(ScopeType, Result<Route, Unresolved>)>,
    ) -> CommandNames {
        CommandNames {
            kind,
            name: name.into(),
            outcome: NameOutcome::Flag(FlagCommand {
                form: DynamicNameForm::TargetSuffix,
                uses: routes
                    .into_iter()
                    .map(|(scope, route)| RoleUse { role, scope, route })
                    .collect(),
                stops: vec![],
            }),
        }
    }

    fn names(commands: &[CommandReference]) -> Vec<&str> {
        commands
            .iter()
            .map(|command| command.name.as_str())
            .collect()
    }

    #[test]
    fn global_stores_reached_from_different_scopes_are_one_namespace() {
        let commands = [
            flag_command(
                DeclarationKind::Effect,
                "set_global",
                Role::Defines,
                vec![(scope(2, "country"), Ok(GLOBAL))],
            ),
            flag_command(
                DeclarationKind::Trigger,
                "has_global",
                Role::Reads,
                vec![
                    (scope(3, "planet"), Ok(GLOBAL)),
                    (scope(4, "ship"), Ok(GLOBAL)),
                ],
            ),
        ];
        let answer = normalize(&commands, BuildId("build".into()));

        let [global] = answer.value.as_slice() else {
            panic!("one namespace: {:?}", answer.value);
        };
        assert_eq!(global.owner, NamespaceOwner::Global);
        assert_eq!(names(&global.defined_by), ["set_global"]);
        assert_eq!(names(&global.read_by), ["has_global"]);
        assert_eq!(global.dynamic_form, DynamicNameForm::TargetSuffix);
    }

    #[test]
    fn unequal_scope_sets_share_only_their_common_scope() {
        let route = Route::Scope {
            terminal: TERMINAL,
            offset: 0,
        };
        let commands = [
            flag_command(
                DeclarationKind::Effect,
                "set_star",
                Role::Defines,
                vec![(scope(7, "galactic_object"), Ok(route))],
            ),
            flag_command(
                DeclarationKind::Trigger,
                "has_star",
                Role::Reads,
                vec![
                    (scope(7, "galactic_object"), Ok(route)),
                    (scope(27, "other"), Ok(route)),
                    (scope(9, "planet"), Err(Unresolved::new("accessor-routes"))),
                ],
            ),
        ];
        let answer = normalize(&commands, BuildId("build".into()));

        let owners: Vec<_> = answer
            .value
            .iter()
            .map(|namespace| match &namespace.owner {
                NamespaceOwner::Scope(scope) => scope.name.as_str(),
                NamespaceOwner::Global => "global",
            })
            .collect();
        assert_eq!(owners, ["galactic_object", "other"]);
        assert_eq!(names(&answer.value[0].defined_by), ["set_star"]);
        assert_eq!(names(&answer.value[0].read_by), ["has_star"]);
        assert_eq!(names(&answer.value[1].defined_by), Vec::<&str>::new());
        assert_eq!(names(&answer.value[1].read_by), ["has_star"]);
        assert_ne!(answer.value[0].id, answer.value[1].id);
        assert!(answer.gaps.iter().any(|gap| {
            gap.kind == GapKind::UnresolvedStorage
                && gap.subject == Some(GapSubject::answer_item("has_star"))
        }));
        assert_eq!(answer.completeness, Completeness::Partial);
    }

    #[test]
    fn commands_that_disagree_on_the_form_leave_a_gap_for_their_namespace() {
        let mut refusing = flag_command(
            DeclarationKind::Trigger,
            "has_global",
            Role::Reads,
            vec![(scope(2, "country"), Ok(GLOBAL))],
        );
        if let NameOutcome::Flag(flag) = &mut refusing.outcome {
            flag.form = DynamicNameForm::NotAccepted;
        }
        let commands = [
            flag_command(
                DeclarationKind::Effect,
                "set_global",
                Role::Defines,
                vec![(scope(2, "country"), Ok(GLOBAL))],
            ),
            refusing,
        ];
        let answer = normalize(&commands, BuildId("build".into()));

        let [global] = answer.value.as_slice() else {
            panic!("one namespace: {:?}", answer.value);
        };
        assert_eq!(global.dynamic_form, DynamicNameForm::Unresolved);
        let gap = answer
            .gaps
            .iter()
            .find(|gap| gap.subject == Some(GapSubject::answer_item(global.id.0.clone())))
            .expect("a gap for the namespace");
        assert_eq!(gap.kind, GapKind::ReaderSemantics);
        assert!(gap.detail.contains("effect set_global"));
        assert!(gap.detail.contains("trigger has_global"));
        assert_eq!(answer.completeness, Completeness::Partial);
    }

    #[test]
    fn an_unread_assign_reader_leaves_a_gap_for_its_command() {
        let mut partial = flag_command(
            DeclarationKind::Effect,
            "set_timed",
            Role::Defines,
            vec![(scope(2, "country"), Ok(GLOBAL))],
        );
        if let NameOutcome::Flag(flag) = &mut partial.outcome {
            flag.stops = vec![Unresolved::new("reader-code")];
        }
        let commands = [
            CommandNames {
                kind: DeclarationKind::Effect,
                name: "set_alone".into(),
                outcome: NameOutcome::Unresolved(Unresolved::new("assign-slot")),
            },
            partial,
        ];
        let answer = normalize(&commands, BuildId("build".into()));

        let gap = |name: &str| {
            answer
                .gaps
                .iter()
                .find(|gap| gap.subject == Some(GapSubject::answer_item(name)))
                .unwrap_or_else(|| panic!("a gap for {name}"))
        };
        assert_eq!(gap("set_alone").kind, GapKind::UnresolvedReader);
        assert!(gap("set_alone").detail.contains("no assign reader"));
        assert_eq!(gap("set_timed").kind, GapKind::ReaderSemantics);
        assert!(gap("set_timed").detail.contains("code was not read"));
        assert_eq!(answer.completeness, Completeness::Partial);
    }
}
