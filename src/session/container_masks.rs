//! Give each registry field the categories that its modifier container accepts.
//!
//! A modifier field's container is built at each persistent destination that the field reads;
//! a clause field's container is the object that its collection inserts. The categories are
//! established only when every read alternative reaches a container with one agreed mask. A key of
//! a clause that reads into the clause's own container is `Enclosing`. Any other container
//! category stays unresolved, with a gap that names the field.
use crate::engine::analysis::{
    fields::{PathOutcome, ReaderJoin, RegistryFieldResult, RootField},
    modifier_blocks::triggered::TriggeredFacts,
    modifiers::{CategoryNames, single_categories},
    readers,
};
use crate::{
    AcceptedCategories, BlockFamily, Field, FieldMembers, Gap, GapKind, GapSubject, GrammarProperty,
};
use std::collections::BTreeSet;

pub(super) fn attach(
    values: &mut [Field],
    result: &RegistryFieldResult,
    clauses: &TriggeredFacts,
    categories: &CategoryNames,
    gaps: &mut Vec<Gap>,
) {
    for field in values.iter_mut() {
        let Some(root) = result.fields.iter().find(|root| root.name == field.name) else {
            continue;
        };
        let mask = match field.reader.family {
            BlockFamily::Modifier => member_mask(root, result),
            BlockFamily::TriggeredModifier => {
                enclose_clause_keys(field, root, result, clauses, gaps);
                clause_mask(root, result)
            }
            _ => continue,
        };
        let path = [field.name.clone()];

        field.accepted_categories = accepted(mask, categories, &path, gaps);
    }

    for field in values.iter() {
        state_unresolved(field, &[], gaps);
    }
}

/// The categories of `mask`, or `Unresolved` with a gap at `path`.
fn accepted(
    mask: Result<u64, &'static str>,
    categories: &CategoryNames,
    path: &[String],
    gaps: &mut Vec<Gap>,
) -> AcceptedCategories {
    let names = mask.and_then(|mask| {
        single_categories(categories, mask).map_err(|unresolved| unresolved.reason)
    });

    match names {
        Ok(names) => AcceptedCategories::Listed(names),
        Err(reason) => {
            push(
                gaps,
                path,
                format!("The modifier container's categories are not established: {reason}."),
            );
            AcceptedCategories::Unresolved
        }
    }
}

/// The one mask of the containers at every destination of every read alternative of a member
/// field.
fn member_mask(root: &RootField, result: &RegistryFieldResult) -> Result<u64, &'static str> {
    let mut masks = BTreeSet::new();

    for (_, outcome) in super::fields::read_alternatives(root, &result.paths) {
        let join = match outcome {
            PathOutcome::Reader(join @ ReaderJoin::Joined { .. }) => join,
            PathOutcome::Rejected => continue,
            _ => return Err("reader-join"),
        };
        let offset = readers::destination(&join).ok_or("container-destination")?;
        let mask = result
            .container_masks
            .get(&offset)
            .ok_or("container-unreached")?
            .as_ref()
            .map_err(|unresolved| unresolved.reason)?;

        masks.insert(*mask);
    }

    match (masks.first(), masks.len()) {
        (Some(&mask), 1) => Ok(mask),
        (_, 0) => Err("container-unreached"),
        _ => Err("container-destinations-disagree"),
    }
}

/// The mask of the object that a clause field's collection inserts.
fn clause_mask(root: &RootField, result: &RegistryFieldResult) -> Result<u64, &'static str> {
    let collection = result
        .collections
        .iter()
        .find(|collection| collection.token == root.token)
        .ok_or("container-unreached")?;

    match &collection.container_mask {
        Some(Ok(mask)) => Ok(*mask),
        Some(Err(unresolved)) => Err(unresolved.reason),
        None => Err("container-constructor"),
    }
}

/// Mark each modifier key of a clause that reads into the clause's own container as
/// `Enclosing`; another modifier key keeps a gap.
fn enclose_clause_keys(
    field: &mut Field,
    root: &RootField,
    result: &RegistryFieldResult,
    clauses: &TriggeredFacts,
    gaps: &mut Vec<Gap>,
) {
    let FieldMembers::TriggeredModifier(block) = &mut field.members else {
        return;
    };
    let (GrammarProperty::Known(keys) | GrammarProperty::Partial(keys)) = &mut block.fixed_keys
    else {
        return;
    };
    let clause = result
        .collections
        .iter()
        .find(|collection| collection.token == root.token)
        .and_then(|collection| collection.reader.as_ref())
        .and_then(|bound| clauses.points.get(&bound.point)?.as_ref().ok());

    for key in keys
        .iter_mut()
        .filter(|key| key.reader.family == BlockFamily::Modifier)
    {
        let destinations: Option<BTreeSet<_>> = clause.and_then(|clause| {
            let key_root = clause.fields.iter().find(|other| other.name == key.name)?;
            key_root.readers.iter().map(readers::destination).collect()
        });
        let other_keys = clause.and_then(|clause| clause.other_keys.as_ref().ok());

        if let (Some(destinations), Some(other_keys)) = (destinations, other_keys)
            && destinations == BTreeSet::from([*other_keys])
        {
            key.accepted_categories = AcceptedCategories::Enclosing;
        } else {
            push(
                gaps,
                &[field.name.clone(), key.name.clone()],
                "The key is not established to read into the clause's own modifier container."
                    .into(),
            );
        }
    }
}

/// Give a gap to each unresolved container of `field` and its members that no gap names yet.
fn state_unresolved(field: &Field, parent: &[String], gaps: &mut Vec<Gap>) {
    let mut path = parent.to_vec();
    path.push(field.name.clone());

    if field.accepted_categories == AcceptedCategories::Unresolved && !named(gaps, &path) {
        push(
            gaps,
            &path,
            "Whether the field reads a modifier container, and its categories, are not established."
                .into(),
        );
    }

    let members = match &field.members {
        FieldMembers::Fields(members) => members,
        FieldMembers::ModifierBlock(block) => match &block.fixed_keys {
            GrammarProperty::Known(members) | GrammarProperty::Partial(members) => members,
            GrammarProperty::Unresolved => return,
        },
        FieldMembers::TriggeredModifier(block) => match &block.fixed_keys {
            GrammarProperty::Known(members) | GrammarProperty::Partial(members) => members,
            GrammarProperty::Unresolved => return,
        },
        _ => return,
    };

    for member in members {
        state_unresolved(member, &path, gaps);
    }
}

/// Whether a gap names `path` or a field that encloses it.
fn named(gaps: &[Gap], path: &[String]) -> bool {
    (1..=path.len()).any(|length| {
        let subject = subject(&path[..length]);
        gaps.iter()
            .any(|gap| gap.subject.as_ref() == Some(&subject))
    })
}

fn subject(path: &[String]) -> GapSubject {
    match path {
        [name] => GapSubject::field(name),
        _ => GapSubject::key_path(path.to_vec()),
    }
}

fn push(gaps: &mut Vec<Gap>, path: &[String], detail: String) {
    let gap = Gap {
        kind: GapKind::UnresolvedPath,
        subject: Some(subject(path)),
        detail,
    };

    if !gaps.contains(&gap) {
        gaps.push(gap);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::fields::{ConcreteReader, TokenPath, Value};
    use crate::engine::analysis::references::ReferenceFacts;
    use crate::engine::analysis::stop::Unresolved;
    use std::collections::BTreeMap;

    fn read(destination: i64) -> ReaderJoin {
        ReaderJoin::Joined {
            callee: "CReader::Read(CPersistent&)".into(),
            arguments: [("x1".into(), Value::Owner(destination))].into(),
            tail: true,
        }
    }

    /// A modifier field that reads each of `destinations` on its own path, whose containers have
    /// `masks`.
    fn result(
        destinations: &[i64],
        masks: BTreeMap<i64, Result<u64, Unresolved>>,
    ) -> RegistryFieldResult {
        let modifier = ConcreteReader {
            read: "CPersistent::Read(CReader&)".into(),
            member: "CStaticModifier::ReadMember(CReader&, int)".into(),
            family: BlockFamily::Modifier,
            delegate: None,
        };
        let paths = destinations
            .iter()
            .zip(0..)
            .map(|(&destination, wider)| TokenPath {
                domain: [7 - wider, 7 + wider],
                conditions: vec![],
                instructions: vec![],
                terminal: 0,
                outcome: PathOutcome::Reader(read(destination)),
            })
            .collect::<Vec<_>>();

        RegistryFieldResult {
            persistent: destinations
                .iter()
                .map(|&destination| (destination, modifier.clone()))
                .collect(),
            persistent_points: Default::default(),
            scoped_destinations: Default::default(),
            stored_words: Default::default(),
            container_masks: masks,
            uses: vec![],
            collections: vec![],
            fields: vec![RootField {
                name: "modifier".into(),
                token: 7,
                constructor: 0,
                paths: (0..paths.len()).collect(),
                readers: destinations
                    .iter()
                    .map(|&destination| read(destination))
                    .collect(),
            }],
            paths,
            gaps: vec![],
            partition_accounted: true,
        }
    }

    fn accepted(result: &RegistryFieldResult) -> (AcceptedCategories, Vec<Gap>) {
        let categories = BTreeMap::from([
            (0x1, Ok(Some("Pops".to_owned()))),
            (0x2, Ok(Some("Fleets".to_owned()))),
        ]);
        let mut values =
            super::super::questions::normalized_fields(result, &ReferenceFacts::default());
        let mut gaps = vec![];

        attach(
            &mut values,
            result,
            &TriggeredFacts::default(),
            &categories,
            &mut gaps,
        );

        (values[0].accepted_categories.clone(), gaps)
    }

    fn container_gap(gaps: &[Gap], reason: &str) -> bool {
        gaps.iter().any(|gap| {
            gap.kind == GapKind::UnresolvedPath
                && gap.subject == Some(GapSubject::field("modifier"))
                && gap.detail.contains(reason)
        })
    }

    #[test]
    fn a_field_whose_destinations_agree_lists_the_categories_of_their_mask() {
        let masks = BTreeMap::from([(0x40, Ok(0x3)), (0x80, Ok(0x3))]);

        let (categories, gaps) = accepted(&result(&[0x40, 0x80], masks));

        assert_eq!(
            categories,
            AcceptedCategories::Listed(vec!["Pops".into(), "Fleets".into()])
        );
        assert!(gaps.is_empty());
    }

    #[test]
    fn destinations_with_different_masks_are_unresolved_with_a_gap() {
        let masks = BTreeMap::from([(0x40, Ok(0x1)), (0x80, Ok(0x2))]);

        let (categories, gaps) = accepted(&result(&[0x40, 0x80], masks));

        assert_eq!(categories, AcceptedCategories::Unresolved);
        assert!(container_gap(&gaps, "container-destinations-disagree"));
    }

    #[test]
    fn an_unresolved_or_unreached_container_is_unresolved_not_every_category() {
        let unknown = BTreeMap::from([(0x40, Err(Unresolved::new("category-argument")))]);

        let (unknown_categories, unknown_gaps) = accepted(&result(&[0x40], unknown));
        let (unreached_categories, unreached_gaps) = accepted(&result(&[0x40], BTreeMap::new()));

        assert_eq!(unknown_categories, AcceptedCategories::Unresolved);
        assert!(container_gap(&unknown_gaps, "category-argument"));
        assert_eq!(unreached_categories, AcceptedCategories::Unresolved);
        assert!(container_gap(&unreached_gaps, "container-unreached"));
    }
}
