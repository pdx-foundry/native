//! Static callback questions: the on_actions and game rules that the engine calls, with the
//! scopes that their call sites supply.
use std::collections::BTreeMap;

use super::Native;
use super::language::gap;
use super::questions::{error, scope_id};
use crate::answer::{
    Answer, Basis, BuildId, Completeness, EntryContext, EntryScope, Error, GameRule, Gap, GapKind,
    OnAction, Operation, RuleKind, ScopeReference, Source,
};
use crate::engine::analysis::callbacks::{
    self, CallbacksResult, Context, Family, Findings, METHOD, RuleFamily, Slot,
};
use crate::engine::analysis::declarations::ScopeType;

impl Native {
    /// The on_actions that the engine fires by name, each with the scopes that its call sites
    /// supply for `this`, `root` and the `from` chain.
    ///
    /// Each [`EntryContext`] is what one or more call sites pass; a name that different sites
    /// fire with different scopes has several. A name whose call sites could not be followed has
    /// no entries and a gap. On_actions that script content defines and fires are outside this
    /// answer, as is what the event system does with a scope after the call.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// use pdx_native::{EntryScope, Native};
    ///
    /// let native = Native::open("/path/to/Stellaris")?;
    /// let on_actions = native.on_actions()?.value;
    /// let scopes = native.scopes()?.value;
    /// for on_action in on_actions.iter().filter(|on_action| on_action.name == "on_game_start") {
    ///     for entry in &on_action.entries {
    ///         if let EntryScope::Scope(reference) = &entry.this {
    ///             let declared = scopes.types.iter().find(|scope| scope.id == reference.id);
    ///             println!("this = {:?}", declared.map(|scope| &scope.name));
    ///         }
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn on_actions(&self) -> Result<Answer<Vec<OnAction>>, Error> {
        self.answer("on_actions", None, || {
            let (callbacks, scope_names) =
                self.callbacks(Operation::OnActions, Family::OnAction)?;
            Ok(normalized_on_actions(
                &callbacks,
                &scope_names,
                self.build(),
            ))
        })
    }

    /// The game rules that the engine evaluates, each with the scopes that its call sites supply
    /// for `this`, `root` and the `from` chain.
    ///
    /// Rule names come from the engine's rule declarations. Each [`EntryContext`] is what one
    /// or more call sites pass. A declared rule with no followed call site has no entries and a
    /// gap.
    pub fn game_rules(&self) -> Result<Answer<Vec<GameRule>>, Error> {
        self.answer("game_rules", None, || {
            let (callbacks, scope_names) =
                self.callbacks(Operation::GameRules, Family::GameRule)?;
            Ok(normalized_game_rules(
                &callbacks,
                &scope_names,
                self.build(),
            ))
        })
    }

    fn callbacks(
        &self,
        operation: Operation,
        family: Family,
    ) -> Result<(CallbacksResult, Option<Vec<String>>), Error> {
        let input = self
            .declaration_analysis(operation)?
            .callbacks_input()
            .map_err(|failure| error(operation, failure))?;
        let result =
            callbacks::analyze(&input, family).map_err(|error| Error::Method(error.to_string()))?;
        Ok((result, input.scope_names))
    }
}

pub(crate) fn normalized_on_actions(
    result: &CallbacksResult,
    scope_names: &Option<Vec<String>>,
    build: BuildId,
) -> Answer<Vec<OnAction>> {
    let mut gaps = Vec::new();
    let scopes = Scopes::new(scope_names, &mut gaps);
    let value = result
        .on_actions
        .iter()
        .map(|(name, findings)| OnAction {
            name: name.clone(),
            entries: entries(name, findings, &scopes, &mut gaps),
        })
        .collect();

    unnamed_gaps(result, Family::OnAction, &mut gaps);
    if result.script_fired_sites > 0 {
        gaps.push(gap(
            GapKind::OutsideMethod,
            None,
            "On_actions that script content defines and fires with fire_on_action are outside this answer.",
        ));
    }
    gaps.push(gap(
        GapKind::OutsideMethod,
        None,
        "The search covers every direct call that fires an on_action, and the checked forwarders. What the event system does with the scope after the call is outside it.",
    ));
    static_answer(value, gaps, build)
}

pub(crate) fn normalized_game_rules(
    result: &CallbacksResult,
    scope_names: &Option<Vec<String>>,
    build: BuildId,
) -> Answer<Vec<GameRule>> {
    let mut gaps = Vec::new();
    let scopes = Scopes::new(scope_names, &mut gaps);
    if let Some(reason) = result.rule_tables_missing {
        gaps.push(gap(
            GapKind::UnreadableInput,
            None,
            format!("the rule declarations could not be read ({reason})"),
        ));
    }
    let value = result
        .rules
        .iter()
        .map(|((name, family), findings)| GameRule {
            name: name.clone(),
            kind: match family {
                RuleFamily::Scripted => RuleKind::Scripted,
                RuleFamily::Weighted => RuleKind::Weighted,
            },
            entries: entries(name, findings, &scopes, &mut gaps),
        })
        .collect();

    unnamed_gaps(result, Family::GameRule, &mut gaps);
    gaps.push(gap(
        GapKind::OutsideMethod,
        None,
        "The search covers every direct call that evaluates a rule, and the checked forwarders. How the rule's result is used is outside it.",
    ));
    static_answer(value, gaps, build)
}

fn static_answer<T>(value: Vec<T>, gaps: Vec<Gap>, build: BuildId) -> Answer<Vec<T>> {
    let completeness = if gaps.iter().all(|gap| gap.kind == GapKind::OutsideMethod) {
        Completeness::Complete
    } else {
        Completeness::Partial
    };
    Answer {
        value,
        completeness,
        gaps,
        source: Source::new(build, METHOD, Basis::StaticAnalysis),
    }
}

/// The scope names by bit, when the table could be read.
struct Scopes<'a>(Option<&'a [String]>);

impl<'a> Scopes<'a> {
    fn new(names: &'a Option<Vec<String>>, gaps: &mut Vec<Gap>) -> Self {
        if names.is_none() {
            gaps.push(gap(
                GapKind::UnreadableInput,
                None,
                "scope name table not found",
            ));
        }
        Self(names.as_deref())
    }

    fn reference(&self, bit: u32) -> Option<ScopeReference> {
        let name = self.0?.get(bit as usize).filter(|name| !name.is_empty())?;
        let scope = ScopeType {
            bit: bit as usize,
            name: name.clone(),
        };
        Some(ScopeReference {
            id: scope_id(&scope),
            name: scope.name,
        })
    }
}

/// The public entries of one name, and a gap for each reason that some site gave no context,
/// and for a context with a scope that could not be established.
fn entries(
    name: &str,
    findings: &Findings,
    scopes: &Scopes<'_>,
    gaps: &mut Vec<Gap>,
) -> Vec<EntryContext> {
    let mut unreadable = false;
    let mut slot = |slot: Slot| match slot {
        Slot::Scope(bit) => scopes.reference(bit).map_or_else(
            || {
                unreadable = true;
                EntryScope::Unresolved
            },
            EntryScope::Scope,
        ),
        Slot::NotSet => EntryScope::NotSet,
        Slot::SelfLink => EntryScope::SelfLink,
        Slot::Unresolved => EntryScope::Unresolved,
    };
    let entries: Vec<EntryContext> = findings
        .contexts
        .iter()
        .map(|Context { this, root, from }| EntryContext {
            this: slot(*this),
            root: slot(*root),
            from: from.iter().map(|from| slot(*from)).collect(),
        })
        .collect();

    for reason in &findings.unresolved {
        gaps.push(gap(GapKind::UnresolvedPath, Some(name), describe(reason)));
    }
    let incomplete = findings.contexts.iter().any(|context| {
        std::iter::once(&context.this)
            .chain([&context.root])
            .chain(&context.from)
            .any(|slot| *slot == Slot::Unresolved)
    });
    if incomplete {
        gaps.push(gap(
            GapKind::UnresolvedPath,
            Some(name),
            "some entry scopes of a call site could not be established",
        ));
    }
    if unreadable {
        gaps.push(gap(
            GapKind::UnreadableInput,
            Some(name),
            "a scope type has no name in the engine's scope table",
        ));
    }
    if entries.is_empty() && findings.unresolved.is_empty() {
        gaps.push(gap(
            GapKind::UnresolvedPath,
            Some(name),
            "no call site of this name was followed",
        ));
    }
    entries
}

/// One gap for each reason that some call sites could not be named, with their number.
fn unnamed_gaps(result: &CallbacksResult, family: Family, gaps: &mut Vec<Gap>) {
    let mut counts = BTreeMap::<&str, usize>::new();
    for unnamed in result
        .unnamed
        .iter()
        .filter(|unnamed| unnamed.family == family)
    {
        *counts.entry(unnamed.reason).or_default() += 1;
    }
    for (reason, count) in counts {
        gaps.push(gap(
            GapKind::UnnamedDeclaration,
            None,
            format!(
                "{count} call sites could not be named: {}",
                describe(reason)
            ),
        ));
    }
}

/// A reader's description of a method reason. The reason code stays at the end.
fn describe(reason: &str) -> String {
    let text = match reason {
        "path-limit" => "a call site has more paths than the method follows",
        "step-limit" => "a path to a call site is longer than the method follows",
        "loop-limit" => {
            "a path to a call site goes around a loop more often than the method follows"
        }
        "left-the-site" => "a path to a call site could not be followed to it",
        "site-not-reached" => "no path from the function entry reaches a call site",
        "context-not-attributed" => {
            "a call site passes one of several names, and a path did not show which"
        }
        "scope-built-by-callee" => "a call site passes a command that builds its own scope",
        "looked-up-only" => "the engine looks up this list, but no followed call site fires it",
        "cached-list-not-fired" => {
            "the engine caches this list, but no followed call site fires it"
        }
        "no-site" => "the engine declares this rule, but no followed call site evaluates it",
        "name-not-a-literal" => "the name is not a text literal, such as a name built at run time",
        "name-not-proved" => "no path proves which name the call site passes",
        "list-unknown" | "cached-list-unknown" => "the list that a call site fires is not known",
        "rule-outside-the-rule-set" => "a rule object outside the engine's rule set",
        "rule-unknown" | "rule-offset" | "rule-not-constant" => "the rule object is not known",
        "rule-not-declared" => "a rule object that the rule declarations do not name",
        "forwarder-not-verified" => "a forwarder did not pass its caller's argument as expected",
        "site-not-decoded" => "the function that holds a call site could not be decoded",
        _ => "a path to a call site could not be followed",
    };
    format!("{text} ({reason})")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::engine::analysis::callbacks::Unnamed;

    fn build() -> BuildId {
        BuildId("test".into())
    }

    fn names() -> Option<Vec<String>> {
        Some(vec!["".into(), "".into(), "country".into()])
    }

    fn result(findings: Findings) -> CallbacksResult {
        let mut result = CallbacksResult::default();
        result.on_actions.insert("on_test".into(), findings);
        result
    }

    #[test]
    fn a_followed_context_with_named_scopes_is_complete() {
        let findings = Findings {
            contexts: BTreeSet::from([Context {
                this: Slot::Scope(2),
                root: Slot::SelfLink,
                from: vec![Slot::SelfLink],
            }]),
            unresolved: BTreeSet::new(),
        };
        let answer = normalized_on_actions(&result(findings), &names(), build());

        assert_eq!(answer.completeness, Completeness::Complete);
        assert_eq!(answer.source.basis, Basis::StaticAnalysis);
        let entry = &answer.value[0].entries[0];
        assert!(matches!(&entry.this, EntryScope::Scope(scope) if scope.name == "country"));
        assert_eq!(entry.from, [EntryScope::SelfLink]);
    }

    #[test]
    fn a_name_without_entries_has_a_gap_and_the_answer_is_partial() {
        let answer = normalized_on_actions(&result(Findings::default()), &names(), build());

        assert!(answer.value[0].entries.is_empty());
        assert_eq!(answer.completeness, Completeness::Partial);
        assert!(
            answer
                .gaps
                .iter()
                .any(|gap| gap.subject.as_ref().map(|subject| subject.name()) == Some("on_test"))
        );
    }

    #[test]
    fn a_scope_bit_without_a_name_is_unresolved_with_a_gap() {
        let findings = Findings {
            contexts: BTreeSet::from([Context {
                this: Slot::Scope(1),
                root: Slot::SelfLink,
                from: vec![Slot::SelfLink],
            }]),
            unresolved: BTreeSet::new(),
        };
        let answer = normalized_on_actions(&result(findings), &names(), build());

        assert_eq!(answer.value[0].entries[0].this, EntryScope::Unresolved);
        assert!(
            answer
                .gaps
                .iter()
                .any(|gap| gap.kind == GapKind::UnreadableInput
                    && gap.subject.as_ref().map(|subject| subject.name()) == Some("on_test"))
        );
    }

    #[test]
    fn unnamed_sites_are_counted_by_reason_and_family() {
        let mut result = CallbacksResult::default();
        for (family, reason) in [
            (Family::OnAction, "name-not-a-literal"),
            (Family::OnAction, "name-not-a-literal"),
            (Family::GameRule, "rule-unknown"),
        ] {
            result.unnamed.push(Unnamed { family, reason });
        }
        let answer = normalized_on_actions(&result, &names(), build());

        let unnamed: Vec<_> = answer
            .gaps
            .iter()
            .filter(|gap| gap.kind == GapKind::UnnamedDeclaration)
            .map(|gap| gap.detail.as_str())
            .collect();
        assert_eq!(unnamed.len(), 1);
        assert!(unnamed[0].starts_with("2 call sites"));
    }
}
