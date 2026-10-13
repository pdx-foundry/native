//! Attach shared modifier grammar only where the field constructor proved its address point.
use crate::engine::analysis::{
    fields::RegistryFieldResult,
    modifier_blocks::{Entry, ModifierBlockFacts, Variant},
    readers,
    references::ReferenceFacts,
    stop::Unresolved,
};
use crate::{
    AcceptedCategories, BlockFamily, Field, FieldMembers, FieldReadOutcome, Gap, GapKind,
    GapSubject, GrammarProperty, ModifierBlock, ModifierEntry, ReaderKind,
};
use std::collections::BTreeSet;

pub(super) fn attach(
    values: &mut [Field],
    result: &RegistryFieldResult,
    facts: &ModifierBlockFacts,
    references: &ReferenceFacts,
    scope_names: Option<&[String]>,
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
            scope_names,
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
    scope_names: Option<&[String]>,
    path: &[String],
    gaps: &mut Vec<Gap>,
) -> Option<ModifierBlock> {
    let variant = match found {
        Some(Ok(variant)) => variant,
        failure => {
            gaps.push(Gap {
                kind: GapKind::UnresolvedReader,
                subject: Some(GapSubject::path(path)),
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
            subject: Some(GapSubject::path(&stopped)),
            detail: format!("The modifier block analysis stopped at {}.", stop.reason),
        });
    }

    normalize(variant, references, scope_names, path, gaps)
}

/// The block grammar of the field at `path`. Fixed-key conditions name field paths from its last
/// name, and each fixed key's read scope is normalized with a gap where its reader leaves it open.
fn normalize(
    variant: &Variant,
    references: &ReferenceFacts,
    scope_names: Option<&[String]>,
    path: &[String],
    gaps: &mut Vec<Gap>,
) -> Option<ModifierBlock> {
    let parent = path.last()?;
    let mut fields =
        super::fields::grammar_fields(&variant.fields, &variant.paths, references, None);
    any_keys(&mut fields, variant, path, gaps);
    super::read_scope::block_keys(
        &mut fields,
        &variant.fields,
        &variant.paths,
        scope_names,
        path,
        gaps,
    );
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
    Some(ModifierBlock {
        fixed_keys,
        entries,
    })
}

/// Give each field that the variant reads as a parameter list its `AnyKey` members. Such a block
/// reads no modifier entries and has no command family. When the written forms are not
/// established, a gap at the field says why.
fn any_keys(fields: &mut [Field], variant: &Variant, path: &[String], gaps: &mut Vec<Gap>) {
    for root in &variant.fields {
        let Some(forms) = variant.any_keys.get(&root.token) else {
            continue;
        };
        let Some(field) = fields.iter_mut().find(|field| field.name == root.name) else {
            continue;
        };

        let value_forms = match forms {
            Ok(forms) => GrammarProperty::Known(forms.clone()),
            Err(stop) => {
                let mut subject = path.to_vec();
                subject.push(root.name.clone());
                gaps.push(Gap {
                    kind: GapKind::ReaderSemantics,
                    subject: Some(GapSubject::path(&subject)),
                    detail: format!(
                        "The written value forms rest on the text lexer boundary, which stopped at {}.",
                        stop.reason
                    ),
                });
                GrammarProperty::Unresolved
            }
        };
        field.members = FieldMembers::AnyKey { value_forms };
        field.reader.family = BlockFamily::NotApplicable;
        for alternative in &mut field.read {
            if let FieldReadOutcome::Read { reader, .. } = &mut alternative.outcome {
                reader.family = BlockFamily::NotApplicable;
            }
        }
        field.accepted_categories = AcceptedCategories::NotApplicable;
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
            any_keys: Default::default(),
        };
        let references = ReferenceFacts::default();

        let block = normalize(
            &variant,
            &references,
            None,
            &["modifier".into()],
            &mut Vec::new(),
        )
        .unwrap();

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

    #[test]
    fn a_scalar_fixed_key_reads_in_no_scope_and_an_unknown_reader_keeps_a_gap() {
        let key = |name: &str, token: i64, callee: &str| RootField {
            name: name.into(),
            token,
            constructor: 0,
            paths: vec![],
            readers: vec![ReaderJoin::Joined {
                callee: callee.into(),
                arguments: Default::default(),
                tail: true,
            }],
        };
        let variant = Variant {
            fields: vec![
                key("label", 1, "CReader::Read(CString&, bool)"),
                key("count", 2, "CReader::Read(int&)"),
                key("mystery", 3, "Mystery::Read(CReader&)"),
            ],
            paths: vec![],
            fixed_complete: true,
            entries: GrammarProperty::Known(vec![]),
            stops: vec![],
            any_keys: Default::default(),
        };
        let mut gaps = Vec::new();

        let block = normalize(
            &variant,
            &ReferenceFacts::default(),
            None,
            &["modifier".into()],
            &mut gaps,
        )
        .unwrap();

        let GrammarProperty::Known(keys) = &block.fixed_keys else {
            panic!("{:?}", block.fixed_keys);
        };
        assert_eq!(keys[0].read_scope, GrammarProperty::Known(vec![]));
        assert_eq!(keys[1].read_scope, GrammarProperty::Known(vec![]));
        assert_eq!(keys[2].read_scope, GrammarProperty::Unresolved);
        assert_eq!(gaps.len(), 1, "{gaps:?}");
        assert_eq!(
            gaps[0].subject,
            Some(GapSubject::key_path(vec![
                "modifier".into(),
                "mystery".into()
            ]))
        );
    }

    #[test]
    fn a_parameter_list_key_takes_any_key_and_reads_in_no_scope() {
        let read = ReaderJoin::Stored {
            callee: "CSerializer::ReadMember(CReader&, int)".into(),
            kind: ReaderKind::Block,
            destination: None,
            repeat: crate::RepeatBehavior::Accumulate,
        };
        let list = |name: &str, token: i64, path: usize| RootField {
            name: name.into(),
            token,
            constructor: 0,
            paths: vec![path],
            readers: vec![read.clone()],
        };
        let path = |token: i64| TokenPath {
            domain: [token, token],
            conditions: vec![],
            instructions: vec![],
            terminal: 0,
            outcome: PathOutcome::Reader(read.clone()),
        };
        let variant = Variant {
            fields: vec![list("parameters", 4, 0), list("unlexed", 5, 1)],
            paths: vec![path(4), path(5)],
            fixed_complete: true,
            entries: GrammarProperty::Known(vec![]),
            stops: vec![],
            any_keys: [
                (4, Ok(vec![crate::ScalarForm::Number])),
                (5, Err(Unresolved::new("numeric-lexer-shape"))),
            ]
            .into(),
        };
        let mut gaps = Vec::new();

        let block = normalize(
            &variant,
            &ReferenceFacts::default(),
            None,
            &["modifier".into()],
            &mut gaps,
        )
        .unwrap();

        let GrammarProperty::Known(keys) = &block.fixed_keys else {
            panic!("{:?}", block.fixed_keys);
        };
        assert_eq!(
            keys[0].members,
            FieldMembers::AnyKey {
                value_forms: GrammarProperty::Known(vec![crate::ScalarForm::Number]),
            }
        );
        assert_eq!(keys[0].read_scope, GrammarProperty::Known(vec![]));
        assert_eq!(keys[0].reader.family, BlockFamily::NotApplicable);
        assert!(matches!(
            &keys[0].read[0].outcome,
            FieldReadOutcome::Read { reader, .. } if reader.family == BlockFamily::NotApplicable
        ));
        assert_eq!(
            keys[1].members,
            FieldMembers::AnyKey {
                value_forms: GrammarProperty::Unresolved,
            }
        );
        assert_eq!(gaps.len(), 1, "{gaps:?}");
        assert_eq!(gaps[0].kind, GapKind::ReaderSemantics);
        assert_eq!(
            gaps[0].subject,
            Some(GapSubject::key_path(vec![
                "modifier".into(),
                "unlexed".into()
            ]))
        );
    }
}
