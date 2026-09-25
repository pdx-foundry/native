//! Static localization question: contexts, their commands and links, and the scope join.
use std::collections::{BTreeMap, BTreeSet};

use sha2::{Digest, Sha256};

use super::Native;
use super::language::{declared, gap, gap_for_subject};
use super::questions::{error, scope_id};
use crate::answer::{
    Answer, BuildId, ContextScopes, Error, Gap, GapKind, GapSubject, LocalizationCommand,
    LocalizationContext, LocalizationContextId, LocalizationContextReference,
    LocalizationDeclarations, LocalizationLink, LocalizationOutput, Operation, ScopeReference,
};
use crate::engine::analysis::localization::{self, Join, LocalizationResult, METHOD, Output};
use crate::engine::analysis::stop::Unresolved;

impl Native {
    /// Read the localization ("localisation") language from the engine's text tables: the
    /// contexts of bracket commands such as `[Root.GetName]`, each context's commands and links,
    /// each link's output, and the scope types that select each context.
    ///
    /// The answer holds what the engine declares. Unscoped command forms, variable and date-flag
    /// lookups, and scripted localization that content defines are outside this method, as are
    /// arguments, formatting and whether a command gives useful text at run time.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// use pdx_native::{ContextScopes, Native};
    ///
    /// let native = Native::open("/path/to/Stellaris")?;
    /// let localization = native.localization_declarations()?.value;
    /// let get_name = localization.commands.iter().find(|command| command.name == "GetName");
    /// for reference in get_name.map(|command| &command.contexts).into_iter().flatten() {
    ///     let context = localization.contexts.iter().find(|context| context.id == reference.id);
    ///     if let Some(ContextScopes::Joined(scopes)) = context.map(|context| &context.scopes) {
    ///         println!("{}: {} scope types", reference.name, scopes.len());
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn localization_declarations(&self) -> Result<Answer<LocalizationDeclarations>, Error> {
        self.answer("localization_declarations", None, || {
            let operation = Operation::LocalizationDeclarations;
            let input = self
                .declaration_analysis(operation)?
                .localization_input()
                .map_err(|failure| error(operation, failure))?;
            let result =
                localization::analyze(&input).map_err(|error| Error::Method(error.to_string()))?;
            Ok(normalized_localization(&result, self.build()))
        })
    }
}

/// The public identity of a localization context. It hides the engine's context value.
fn context_id(value: u64) -> LocalizationContextId {
    let digest = Sha256::digest(format!("localization-context/{value}").as_bytes());
    LocalizationContextId(format!("{digest:x}")[..16].to_owned())
}

/// A link's output with context values, ordered so that rows group by name and output.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Target {
    Contexts(Vec<u64>),
    Various,
    Unchanged,
    Unresolved,
}

pub(crate) fn normalized_localization(
    result: &LocalizationResult,
    build: BuildId,
) -> Answer<LocalizationDeclarations> {
    let mut gaps = Vec::new();
    let names: BTreeMap<u64, String> = result
        .contexts
        .iter()
        .map(|context| (context.value, context.name.clone().unwrap_or_default()))
        .collect();
    let reference = |value: u64| LocalizationContextReference {
        id: context_id(value),
        name: names.get(&value).cloned().unwrap_or_default(),
    };

    for context in &result.contexts {
        if context.name.is_none() {
            gaps.push(gap_for_subject(
                GapKind::UnreadableInput,
                Some(GapSubject::LocalizationContext {
                    id: context_id(context.value),
                    name: String::new(),
                }),
                "the name of this context could not be read",
            ));
        }
    }

    let contexts = contexts(result, &names, &mut gaps);
    let commands = commands(result, &reference, &mut gaps);
    let links = links(result, &reference, &mut gaps);

    gaps.push(gap(
        GapKind::OutsideMethod,
        None,
        "Unscoped command forms such as GetDate, and variable and date-flag lookups, are resolved outside the context tables.",
    ));
    gaps.push(gap(
        GapKind::OutsideMethod,
        None,
        "Scripted localization that content defines is outside this method.",
    ));

    declared(
        LocalizationDeclarations {
            contexts,
            commands,
            links,
        },
        gaps,
        build,
        METHOD,
    )
}

/// Each context with the scope types that select it. With any scope type unresolved, no context
/// can be `Joined` or `Missing`.
fn contexts(
    result: &LocalizationResult,
    names: &BTreeMap<u64, String>,
    gaps: &mut Vec<Gap>,
) -> Vec<LocalizationContext> {
    let mut selecting = BTreeMap::<u64, Vec<ScopeReference>>::new();
    let mut unresolved = result.scope_table_missing;
    if result.scope_table_missing {
        gaps.push(gap(
            GapKind::UnreadableInput,
            None,
            "scope name table not found",
        ));
    }

    for (scope, join) in &result.joins {
        match join {
            Join::Context(value) => {
                selecting.entry(*value).or_default().push(ScopeReference {
                    id: scope_id(scope),
                    name: scope.name.clone(),
                });
            }
            Join::NoContext => {}
            Join::Unresolved(Unresolved { reason, .. }) => {
                unresolved = true;
                gaps.push(gap_for_subject(
                    GapKind::UnresolvedPath,
                    Some(GapSubject::ScopeType {
                        id: scope_id(scope),
                        name: scope.name.clone(),
                    }),
                    format!("the scope-object setter could not be followed ({reason})"),
                ));
            }
        }
    }

    let mut contexts: Vec<_> = names
        .iter()
        .map(|(value, name)| {
            let mut scopes = selecting.remove(value).unwrap_or_default();
            scopes.sort_by(|left, right| (&left.name, &left.id).cmp(&(&right.name, &right.id)));
            let scopes = match (unresolved, scopes.is_empty()) {
                (true, _) => ContextScopes::Partial(scopes),
                (false, true) => ContextScopes::Missing,
                (false, false) => ContextScopes::Joined(scopes),
            };
            LocalizationContext {
                id: context_id(*value),
                name: name.clone(),
                scopes,
            }
        })
        .collect();
    contexts.sort_by(|left, right| (&left.name, &left.id).cmp(&(&right.name, &right.id)));
    contexts
}

fn commands(
    result: &LocalizationResult,
    reference: &impl Fn(u64) -> LocalizationContextReference,
    gaps: &mut Vec<Gap>,
) -> Vec<LocalizationCommand> {
    let mut declaring = BTreeMap::<String, BTreeSet<u64>>::new();
    for context in &result.contexts {
        match &context.commands {
            Ok(commands) => {
                for command in commands {
                    declaring
                        .entry(command.clone())
                        .or_default()
                        .insert(context.value);
                }
            }
            Err(reason) => gaps.push(gap_for_subject(
                GapKind::UnreadableInput,
                Some(GapSubject::LocalizationContext {
                    id: context_id(context.value),
                    name: reference(context.value).name,
                }),
                format!("the command rows of this context could not be read ({reason})"),
            )),
        }
    }

    declaring
        .into_iter()
        .map(|(name, values)| LocalizationCommand {
            name,
            contexts: sorted_references(values, reference),
        })
        .collect()
}

fn links(
    result: &LocalizationResult,
    reference: &impl Fn(u64) -> LocalizationContextReference,
    gaps: &mut Vec<Gap>,
) -> Vec<LocalizationLink> {
    let mut declaring = BTreeMap::<(String, Target), BTreeSet<u64>>::new();
    for context in &result.contexts {
        let links = match &context.links {
            Ok(links) => links,
            Err(reason) => {
                gaps.push(gap_for_subject(
                    GapKind::UnreadableInput,
                    Some(GapSubject::LocalizationContext {
                        id: context_id(context.value),
                        name: reference(context.value).name,
                    }),
                    format!("the link rows of this context could not be read ({reason})"),
                ));
                continue;
            }
        };

        for (name, output) in links {
            let target = match output {
                Output::Contexts(values) => Target::Contexts(values.iter().copied().collect()),
                Output::Various => Target::Various,
                Output::Unchanged => Target::Unchanged,
                Output::Unresolved(Unresolved { reason, .. }) => {
                    gaps.push(gap_for_subject(
                        GapKind::UnresolvedPath,
                        Some(GapSubject::LocalizationLink { name: name.clone() }),
                        format!(
                            "from context {}: the link function could not be followed ({reason})",
                            reference(context.value).name
                        ),
                    ));
                    Target::Unresolved
                }
            };
            declaring
                .entry((name.clone(), target))
                .or_default()
                .insert(context.value);
        }
    }

    declaring
        .into_iter()
        .map(|((name, target), values)| LocalizationLink {
            name,
            input_contexts: sorted_references(values, reference),
            output: match target {
                Target::Contexts(values) => {
                    LocalizationOutput::Listed(sorted_references(values, reference))
                }
                Target::Various => LocalizationOutput::Various,
                Target::Unchanged => LocalizationOutput::Unchanged,
                Target::Unresolved => LocalizationOutput::Unresolved,
            },
        })
        .collect()
}

fn sorted_references(
    values: impl IntoIterator<Item = u64>,
    reference: &impl Fn(u64) -> LocalizationContextReference,
) -> Vec<LocalizationContextReference> {
    let mut references: Vec<_> = values.into_iter().map(reference).collect();
    references.sort_by(|left, right| (&left.name, &left.id).cmp(&(&right.name, &right.id)));
    references
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::declarations::ScopeType;
    use crate::engine::analysis::localization::Context;

    #[test]
    fn gaps_identify_link_context_and_scope_even_when_names_collide() {
        let scope = ScopeType {
            bit: 1,
            name: "Planet".into(),
        };
        let result = LocalizationResult {
            contexts: vec![Context {
                value: 7,
                name: Some("Planet".into()),
                commands: Err("rows"),
                links: Ok(vec![(
                    "Planet".into(),
                    Output::Unresolved(Unresolved::new("output")),
                )]),
            }],
            joins: vec![(scope.clone(), Join::Unresolved(Unresolved::new("setter")))],
            scope_table_missing: false,
        };

        let answer = normalized_localization(&result, BuildId("test".into()));
        let subjects: Vec<_> = answer
            .gaps
            .iter()
            .filter_map(|gap| gap.subject.clone())
            .collect();

        assert!(subjects.contains(&GapSubject::LocalizationContext {
            id: context_id(7),
            name: "Planet".into(),
        }));
        assert!(subjects.contains(&GapSubject::LocalizationLink {
            name: "Planet".into(),
        }));
        assert!(subjects.contains(&GapSubject::ScopeType {
            id: scope_id(&scope),
            name: "Planet".into(),
        }));
    }
}
