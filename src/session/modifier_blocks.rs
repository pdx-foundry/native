//! Attach shared modifier grammar only where the field constructor proved its address point.
use crate::engine::analysis::{
    fields::RegistryFieldResult,
    modifier_blocks::{Entry, ModifierBlockFacts, Variant},
    readers,
    references::ReferenceFacts,
    stop::Unresolved,
};
use crate::{
    BlockFamily, Field, FieldMembers, Gap, GapKind, GapSubject, GrammarProperty, ModifierBlock,
    ModifierEntry, ReaderKind,
};
use std::collections::BTreeSet;

pub(super) fn attach(
    values: &mut [Field],
    result: &RegistryFieldResult,
    facts: &ModifierBlockFacts,
    references: &ReferenceFacts,
    gaps: &mut Vec<Gap>,
) {
    for field in values
        .iter_mut()
        .filter(|field| field.reader.family == BlockFamily::Modifier)
    {
        let Some(root) = result.fields.iter().find(|root| root.name == field.name) else {
            continue;
        };
        let points: BTreeSet<_> = root
            .readers
            .iter()
            .filter_map(|join| {
                readers::destination(join).and_then(|offset| result.persistent_points.get(&offset))
            })
            .collect();
        if points.len() != 1 {
            gaps.push(Gap {
                kind: GapKind::UnresolvedReader,
                subject: Some(GapSubject::field(&field.name)),
                detail: "The modifier block has no unique constructor-proven address point.".into(),
            });
            continue;
        }
        let point = *points.first().unwrap();
        if let Some(block) = block(
            facts.points.get(point),
            references,
            std::slice::from_ref(&field.name),
            gaps,
        ) {
            field.members = FieldMembers::ModifierBlock(block);
        }
    }
}

/// The modifier block at `path` from its analysis result, with a gap for each stop. A missing
/// or stopped analysis gives `None` and a gap at `path`.
pub(super) fn block(
    found: Option<&Result<Variant, Unresolved>>,
    references: &ReferenceFacts,
    path: &[String],
    gaps: &mut Vec<Gap>,
) -> Option<ModifierBlock> {
    let subject = |path: Vec<String>| {
        if path.len() == 1 {
            GapSubject::field(&path[0])
        } else {
            GapSubject::key_path(path)
        }
    };
    let variant = match found {
        Some(Ok(variant)) => variant,
        failure => {
            gaps.push(Gap {
                kind: GapKind::UnresolvedReader,
                subject: Some(subject(path.to_vec())),
                detail: format!(
                    "The modifier block analysis stopped at {}.",
                    match failure {
                        Some(Err(stop)) => stop.reason,
                        _ => "modifier-block-point",
                    }
                ),
            });
            return None;
        }
    };

    for (key, stop) in &variant.stops {
        let mut stopped = path.to_vec();
        stopped.extend(key.iter().cloned());
        gaps.push(Gap {
            kind: if key.is_some() {
                GapKind::UnresolvedReader
            } else {
                GapKind::UnresolvedPath
            },
            subject: Some(subject(stopped)),
            detail: format!("The modifier block analysis stopped at {}.", stop.reason),
        });
    }

    Some(normalize(variant, references, path.last()?))
}

/// The block grammar of the field `parent`. Fixed-key conditions name field paths from `parent`.
fn normalize(variant: &Variant, references: &ReferenceFacts, parent: &str) -> ModifierBlock {
    let mut fields =
        super::fields::grammar_fields(&variant.fields, &variant.paths, references, None);
    for field in &mut fields {
        super::fields::prefix_conditions(field, parent);
    }

    let fixed_keys = if variant.fixed_complete {
        GrammarProperty::Known(fields)
    } else {
        GrammarProperty::Partial(fields)
    };
    let normalize_entries = |entries: &[Entry]| {
        entries
            .iter()
            .map(|entry| match entry {
                Entry::Numeric(callee) => ModifierEntry::Numeric {
                    value: super::fields::reader(&[
                        crate::engine::analysis::fields::ReaderJoin::Joined {
                            callee: callee.clone(),
                            arguments: Default::default(),
                            tail: false,
                        },
                    ]),
                },
                Entry::Reference(target) => ModifierEntry::Reference {
                    target: target.clone(),
                    value: ReaderKind::FixedPoint,
                },
            })
            .collect()
    };
    let entries = match &variant.entries {
        GrammarProperty::Known(entries) => GrammarProperty::Known(normalize_entries(entries)),
        GrammarProperty::Partial(entries) => GrammarProperty::Partial(normalize_entries(entries)),
        GrammarProperty::Unresolved => GrammarProperty::Unresolved,
    };
    ModifierBlock {
        fixed_keys,
        entries,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FieldCondition;
    use crate::engine::analysis::fields::{
        Condition, PathOutcome, ReaderJoin, RootField, TokenPath, Value,
    };

    #[test]
    fn a_fixed_key_condition_names_the_path_from_the_block_field() {
        let read = |callee: &str, at: i64| ReaderJoin::Joined {
            callee: callee.into(),
            arguments: [("x1".into(), Value::Owner(at))].into(),
            tail: true,
        };
        let variant = Variant {
            fields: vec![
                RootField {
                    name: "flag".into(),
                    token: 3,
                    constructor: 0,
                    paths: vec![],
                    readers: vec![read("CReader::Read(bool&)", 8)],
                },
                RootField {
                    name: "amount".into(),
                    token: 7,
                    constructor: 0,
                    paths: vec![0],
                    readers: vec![read("CReader::Read(int&)", 12)],
                },
            ],
            paths: vec![TokenPath {
                domain: [7, 7],
                conditions: vec![Condition {
                    at: 4,
                    value: Some(Value::Load(Box::new(Value::Owner(8)), 1)),
                    zero: false,
                }],
                instructions: vec![],
                terminal: 0,
                outcome: PathOutcome::Reader(read("CReader::Read(int&)", 12)),
            }],
            fixed_complete: true,
            entries: GrammarProperty::Known(vec![]),
            stops: vec![],
        };
        let references = ReferenceFacts::default();

        let block = normalize(&variant, &references, "modifier");

        let GrammarProperty::Known(keys) = &block.fixed_keys else {
            panic!("{:?}", block.fixed_keys);
        };
        assert_eq!(
            keys[1].read[0].condition,
            FieldCondition::FieldZero {
                path: vec!["modifier".into(), "flag".into()],
                zero: false,
            }
        );
    }
}
