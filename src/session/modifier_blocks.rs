//! Attach shared modifier grammar only where the field constructor proved its address point.
use crate::engine::analysis::{
    fields::RegistryFieldResult,
    modifier_blocks::{Entry, ModifierBlockFacts, Variant},
    readers,
    references::ReferenceFacts,
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
        match facts.points.get(point) {
            Some(Ok(variant)) => {
                field.members = FieldMembers::ModifierBlock(normalize(variant, references));
                for (key, stop) in &variant.stops {
                    let mut path = vec![field.name.clone()];
                    path.extend(key.iter().cloned());
                    gaps.push(Gap {
                        kind: if key.is_some() {
                            GapKind::UnresolvedReader
                        } else {
                            GapKind::UnresolvedPath
                        },
                        subject: Some(if key.is_some() {
                            GapSubject::key_path(path)
                        } else {
                            GapSubject::field(&field.name)
                        }),
                        detail: format!("The modifier block analysis stopped at {}.", stop.reason),
                    });
                }
            }
            failure => {
                gaps.push(Gap {
                    kind: GapKind::UnresolvedReader,
                    subject: Some(GapSubject::field(&field.name)),
                    detail: format!(
                        "The modifier block analysis stopped at {}.",
                        match failure {
                            Some(Err(stop)) => stop.reason,
                            _ => "modifier-block-point",
                        }
                    ),
                });
            }
        }
    }
}

fn normalize(variant: &Variant, references: &ReferenceFacts) -> ModifierBlock {
    let fields = super::fields::grammar_fields(&variant.fields, &variant.paths, references, None);
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
