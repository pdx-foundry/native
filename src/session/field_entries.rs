//! The entry contexts of registry field blocks: the scopes that the owner's direct evaluation
//! calls supply to each stored trigger or effect block.
use std::collections::BTreeSet;

use super::callbacks::{Scopes, entries};
use super::language::gap_for_subject;
use crate::answer::{BlockFamily, Field, Gap, GapKind, GapSubject};
use crate::binding::BlockFacts;
use crate::engine::analysis::callbacks::Findings;
use crate::engine::analysis::callbacks::blocks::Block;
use crate::engine::analysis::fields::RegistryFieldResult;
use crate::engine::analysis::readers;

/// What the search for entry contexts leaves out.
const LIMIT: &str = "Entry contexts cover the root trigger and effect blocks that the owner's \
    own methods evaluate by direct call. Virtual calls, evaluations outside the owner's methods, \
    nested blocks, weights, script values and modifier blocks are outside this method.";

/// Give each root trigger or effect block of `owner`'s registry its entry contexts, and record a
/// gap for each block whose contexts are missing or incomplete.
pub(super) fn attach(
    fields: &mut [Field],
    result: &RegistryFieldResult,
    registry: &str,
    owner: &str,
    facts: &BlockFacts,
    gaps: &mut Vec<Gap>,
) {
    let scopes = Scopes::new(&facts.scope_names, gaps);
    for (field, root) in fields.iter_mut().zip(&result.fields) {
        if !matches!(
            field.reader.family,
            BlockFamily::Trigger | BlockFamily::Effect
        ) {
            continue;
        }

        let subject = GapSubject::field(&field.name);
        let destinations: BTreeSet<i64> = root
            .readers
            .iter()
            .filter_map(readers::destination)
            .collect();
        if destinations.is_empty() {
            gaps.push(gap_for_subject(
                GapKind::UnresolvedPath,
                Some(subject),
                "the block's storage is not established, so no evaluation joins it",
            ));
            continue;
        }

        let findings = merged(facts, owner, &destinations);
        if findings.contexts.is_empty() && findings.unresolved.is_empty() {
            gaps.push(gap_for_subject(
                GapKind::UnresolvedPath,
                Some(subject),
                "no direct call in the owner's methods evaluates this block",
            ));
            continue;
        }
        field.entry_contexts = entries(&subject, &findings, &scopes, gaps);
    }

    if let Some(count) = facts.entries.unattributed.get(owner) {
        gaps.push(gap_for_subject(
            GapKind::UnresolvedPath,
            Some(GapSubject::registry(registry)),
            format!(
                "{count} direct evaluation calls in the owner's methods evaluate a block that \
                 the method cannot name"
            ),
        ));
    }
    gaps.push(gap_for_subject(GapKind::OutsideMethod, None, LIMIT));
}

/// The findings of every block that `owner` stores at one of `destinations`.
fn merged(facts: &BlockFacts, owner: &str, destinations: &BTreeSet<i64>) -> Findings {
    let mut findings = Findings::default();
    for &offset in destinations {
        let block = Block {
            owner: owner.into(),
            offset,
        };
        if let Some(found) = facts.entries.blocks.get(&block) {
            findings.contexts.extend(found.contexts.iter().cloned());
            findings.unresolved.extend(&found.unresolved);
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use serde_json::json;

    use super::*;
    use crate::answer::{EntryContext, EntryScope};
    use crate::engine::analysis::callbacks::blocks::BlockEntries;
    use crate::engine::analysis::callbacks::{Context, Slot};
    use crate::engine::analysis::fields::{ReaderJoin, RootField, Value};

    const OWNER: &str = "COwner";

    fn field(name: &str, family: &str) -> Field {
        serde_json::from_value(json!({
            "name": name,
            "reader": { "id": null, "kind": "Block", "family": family,
                "numeric": "Unresolved", "scoped_operand": "Unresolved" },
            "shape": { "value": "Unknown", "repeat": "Unknown" }, "read": [],
            "members": "Unresolved", "domain": "Unknown", "uses": [],
            "reference": "NotEstablished", "entry_contexts": [], "read_scope": "Unresolved",
            "accepted_categories": "NotApplicable"
        }))
        .unwrap()
    }

    /// A root trigger field stored at `this + destination`, or at no established destination.
    fn root(name: &str, destination: Option<i64>) -> RootField {
        let stored = destination.map_or(Value::Constant(0), Value::Owner);
        RootField {
            name: name.into(),
            token: 7,
            constructor: 0,
            paths: vec![0],
            readers: vec![ReaderJoin::Joined {
                callee: "void NParserUtil::ReadTrigger<CRootTrigger>(CReader&, CRootTrigger&, EScopeType)".into(),
                arguments: [("x0".into(), Value::Reader(0)), ("x1".into(), stored)].into(),
                tail: false,
            }],
        }
    }

    fn result(fields: Vec<RootField>) -> RegistryFieldResult {
        RegistryFieldResult {
            uses: vec![],
            persistent: BTreeMap::new(),
            persistent_points: BTreeMap::new(),
            scoped_destinations: BTreeMap::new(),
            stored_words: BTreeMap::new(),
            container_masks: Default::default(),
            collections: vec![],
            fields,
            paths: vec![],
            gaps: vec![],
            partition_accounted: true,
        }
    }

    /// A country scope, bit 2, whose links point back to itself.
    fn country() -> Context {
        Context {
            this: Slot::Scope(2),
            root: Slot::SelfLink,
            from: vec![Slot::SelfLink],
            prev: vec![Slot::SelfLink],
        }
    }

    fn facts(blocks: &[(i64, Findings)], unattributed: usize) -> BlockFacts {
        BlockFacts {
            entries: BlockEntries {
                blocks: blocks
                    .iter()
                    .map(|(offset, findings)| {
                        let block = Block {
                            owner: OWNER.into(),
                            offset: *offset,
                        };
                        (block, findings.clone())
                    })
                    .collect(),
                unattributed: (unattributed > 0)
                    .then(|| (OWNER.to_string(), unattributed))
                    .into_iter()
                    .collect(),
            },
            scope_names: Some(vec!["none".into(), "planet".into(), "country".into()]),
        }
    }

    fn found(contexts: &[Context]) -> Findings {
        Findings {
            contexts: contexts.iter().cloned().collect(),
            unresolved: BTreeSet::new(),
        }
    }

    fn field_gaps<'a>(gaps: &'a [Gap], name: &str) -> Vec<&'a str> {
        gaps.iter()
            .filter(|gap| gap.subject == Some(GapSubject::field(name)))
            .map(|gap| gap.detail.as_str())
            .collect()
    }

    fn attached(fields: &mut [Field], roots: Vec<RootField>, facts: &BlockFacts) -> Vec<Gap> {
        let mut gaps = Vec::new();
        attach(
            fields,
            &result(roots),
            "common/owners",
            OWNER,
            facts,
            &mut gaps,
        );
        gaps
    }

    #[test]
    fn a_block_takes_the_contexts_that_its_evaluations_receive() {
        let mut fields = [field("potential", "Trigger")];
        let gaps = attached(
            &mut fields,
            vec![root("potential", Some(0x40))],
            &facts(&[(0x40, found(&[country()]))], 0),
        );

        assert_eq!(fields[0].entry_contexts.len(), 1);
        let EntryContext { this, root, .. } = &fields[0].entry_contexts[0];
        assert!(matches!(this, EntryScope::Scope(scope) if scope.name == "country"));
        assert_eq!(*root, EntryScope::SelfLink);
        assert!(field_gaps(&gaps, "potential").is_empty());
        assert!(gaps.iter().any(|gap| gap.kind == GapKind::OutsideMethod
            && gap.subject.is_none()
            && gap.detail.contains("Virtual calls")));
    }

    #[test]
    fn a_block_that_no_call_evaluates_has_a_gap() {
        let mut fields = [field("potential", "Trigger")];
        let gaps = attached(
            &mut fields,
            vec![root("potential", Some(0x40))],
            &facts(&[(0x48, found(&[country()]))], 0),
        );

        assert!(fields[0].entry_contexts.is_empty());
        assert_eq!(
            field_gaps(&gaps, "potential"),
            ["no direct call in the owner's methods evaluates this block"]
        );
    }

    #[test]
    fn a_block_with_no_established_storage_has_a_gap() {
        let mut fields = [field("potential", "Trigger")];
        let gaps = attached(
            &mut fields,
            vec![root("potential", None)],
            &facts(&[(0x40, found(&[country()]))], 0),
        );

        assert!(fields[0].entry_contexts.is_empty());
        assert_eq!(
            field_gaps(&gaps, "potential"),
            ["the block's storage is not established, so no evaluation joins it"]
        );
    }

    #[test]
    fn an_unresolved_reason_and_an_unreadable_context_are_gaps_beside_the_found_contexts() {
        let unreadable = Context {
            this: Slot::Unresolved,
            root: Slot::Unresolved,
            from: vec![Slot::Unresolved],
            prev: vec![Slot::Unresolved],
        };
        let findings = Findings {
            contexts: BTreeSet::from([country(), unreadable]),
            unresolved: BTreeSet::from(["no-caller"]),
        };
        let mut fields = [field("potential", "Trigger")];
        let gaps = attached(
            &mut fields,
            vec![root("potential", Some(0x40))],
            &facts(&[(0x40, findings)], 0),
        );

        assert_eq!(fields[0].entry_contexts.len(), 2);
        let details = field_gaps(&gaps, "potential");
        assert!(details.iter().any(|detail| detail.ends_with("(no-caller)")));
        assert!(details.contains(&"some entry scopes of a call site could not be established"));
    }

    #[test]
    fn other_fields_keep_no_contexts_and_no_gap() {
        let mut fields = [field("modifier", "Modifier")];
        let gaps = attached(
            &mut fields,
            vec![root("modifier", Some(0x40))],
            &facts(&[(0x40, found(&[country()]))], 0),
        );

        assert!(fields[0].entry_contexts.is_empty());
        assert!(field_gaps(&gaps, "modifier").is_empty());
    }

    #[test]
    fn evaluations_that_name_no_block_are_a_registry_gap() {
        let mut fields = [field("potential", "Trigger")];
        let gaps = attached(
            &mut fields,
            vec![root("potential", Some(0x40))],
            &facts(&[(0x40, found(&[country()]))], 2),
        );

        assert_eq!(fields[0].entry_contexts.len(), 1);
        assert!(gaps.iter().any(|gap| gap.kind == GapKind::UnresolvedPath
            && gap.subject == Some(GapSubject::registry("common/owners"))
            && gap.detail.starts_with("2 direct evaluation calls")));
    }
}
