//! Apply one cached conversion fact to each occurrence of a shared reader.
use crate::engine::analysis::numeric::{FLOAT_BOUND_REPRESENTATION_GAP, NumericFacts};
use crate::{
    CommandForm, CommandGrammar, Field, FieldMembers, FieldReadOutcome, Gap, GapKind, GapSubject,
    GrammarProperty, Reader, ReaderId, ReaderKind,
};

#[derive(Default)]
struct ReaderLimits {
    incomplete: bool,
    unrepresentable_bounds: bool,
}

impl ReaderLimits {
    fn include(&mut self, other: Self) {
        self.incomplete |= other.incomplete;
        self.unrepresentable_bounds |= other.unrepresentable_bounds;
    }
}

/// Attach conversion facts and retain the obstructions that need public gaps.
fn attach_reader_facts(reader: &mut Reader, facts: &NumericFacts) -> ReaderLimits {
    reader.scoped_operand =
        if matches!(reader.kind, ReaderKind::Unknown | ReaderKind::ScopedNumeric) {
            GrammarProperty::Unresolved
        } else {
            GrammarProperty::Known(None)
        };
    if let Some(fact) = facts.readers.iter().find_map(|(callee, fact)| {
        (reader.id.as_ref() == Some(&ReaderId::from_callee(callee))).then_some(fact)
    }) {
        reader.numeric = fact.conversion.clone();
        if let GrammarProperty::Known(Some(conversion)) | GrammarProperty::Partial(Some(conversion)) =
            &reader.numeric
            && conversion.representation
                == GrammarProperty::Known(crate::NumericRepresentation::BinaryFloat)
        {
            reader.kind = ReaderKind::Float;
        }
        return ReaderLimits {
            incomplete: !fact.gaps.is_empty(),
            unrepresentable_bounds: fact
                .gaps
                .iter()
                .any(|gap| gap.reason == FLOAT_BOUND_REPRESENTATION_GAP),
        };
    }
    reader.numeric = match reader.kind {
        ReaderKind::Boolean
        | ReaderKind::String
        | ReaderKind::Reference
        | ReaderKind::Target
        | ReaderKind::Keyword
        | ReaderKind::Block => GrammarProperty::Known(None),
        _ => GrammarProperty::Unresolved,
    };
    ReaderLimits {
        incomplete: matches!(
            reader.kind,
            ReaderKind::Integer | ReaderKind::FixedPoint | ReaderKind::Float
        ),
        ..ReaderLimits::default()
    }
}

pub(super) fn fields(
    values: &mut [Field],
    facts: &NumericFacts,
    prefix: &[String],
    gaps: &mut Vec<Gap>,
) {
    for field in values {
        let mut path = prefix.to_vec();
        path.push(field.name.clone());
        let mut limits = attach_reader_facts(&mut field.reader, facts);
        for alternative in &mut field.read {
            if let FieldReadOutcome::Read { reader: value, .. } = &mut alternative.outcome {
                limits.include(attach_reader_facts(value, facts));
            }
        }
        if limits.incomplete {
            let subject = if path.len() == 1 {
                GapSubject::Field {
                    name: path[0].clone(),
                }
            } else {
                GapSubject::KeyPath { path: path.clone() }
            };
            gap(gaps, subject, limits);
        }
        match &mut field.members {
            FieldMembers::Fields(children) => fields(children, facts, &path, gaps),
            FieldMembers::ModifierBlock(block) => {
                if let GrammarProperty::Known(keys) | GrammarProperty::Partial(keys) =
                    &mut block.fixed_keys
                {
                    fields(keys, facts, &path, gaps);
                }
                if let GrammarProperty::Known(entries) | GrammarProperty::Partial(entries) =
                    &mut block.entries
                {
                    for entry in entries {
                        if let crate::ModifierEntry::Numeric { value } = entry {
                            let limits = attach_reader_facts(value, facts);
                            if limits.incomplete {
                                gap(
                                    gaps,
                                    if path.len() == 1 {
                                        GapSubject::field(&field.name)
                                    } else {
                                        GapSubject::key_path(path.clone())
                                    },
                                    limits,
                                );
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Attach the numeric facts of one reader that no field carries, with its gap at `subject`.
pub(super) fn reader(
    reader: &mut Reader,
    facts: &NumericFacts,
    subject: GapSubject,
    gaps: &mut Vec<Gap>,
) {
    let limits = attach_reader_facts(reader, facts);
    if limits.incomplete {
        gap(gaps, subject, limits);
    }
}

pub(super) fn grammar(
    value: &mut CommandGrammar,
    name: &str,
    facts: &NumericFacts,
    gaps: &mut Vec<Gap>,
) {
    let mut limits = attach_reader_facts(&mut value.reader, facts);
    if let GrammarProperty::Known(forms) | GrammarProperty::Partial(forms) = &mut value.forms {
        for form in forms {
            if let CommandForm::Value(value) = form {
                limits.include(attach_reader_facts(&mut value.reader, facts));
            }
        }
    }
    if limits.incomplete {
        gap(gaps, GapSubject::answer_item(name), limits);
    }
    if let GrammarProperty::Known(keys) | GrammarProperty::Partial(keys) = &mut value.fixed_keys {
        fields(keys, facts, &[], gaps);
    }
    if let GrammarProperty::Known(rules) | GrammarProperty::Partial(rules) = &mut value.ordering {
        for rule in rules {
            if let crate::ChildOrderOutcome::Read(selected) = &mut rule.outcome {
                let limits = attach_reader_facts(selected, facts);
                if limits.incomplete {
                    gap(gaps, GapSubject::field(&rule.child), limits);
                }
            }
        }
    }
    let child_was_known = matches!(value.numeric_keys, GrammarProperty::Known(_));
    if let GrammarProperty::Known(Some(child)) | GrammarProperty::Partial(Some(child)) =
        &mut value.numeric_keys
    {
        let mut child_gaps = Vec::new();
        grammar(child, name, facts, &mut child_gaps);
        if !child_gaps.is_empty()
            && child_was_known
            && let GrammarProperty::Known(child) = std::mem::take(&mut value.numeric_keys)
        {
            value.numeric_keys = GrammarProperty::Partial(child);
        }
        for mut child_gap in child_gaps {
            child_gap.subject = Some(GapSubject::answer_item(name));
            if !gaps.contains(&child_gap) {
                gaps.push(child_gap);
            }
        }
    }
}

fn gap(gaps: &mut Vec<Gap>, subject: GapSubject, limits: ReaderLimits) {
    let gap = Gap {
        kind: GapKind::NumericConversion, subject: Some(subject.clone()),
        detail: "Numeric conversion is incomplete: lexical boundaries, trailing text, accepted range, overflow and library-dependent behavior are not fully established.".into(),
    };
    if !gaps.contains(&gap) {
        gaps.push(gap);
    }
    if limits.unrepresentable_bounds {
        let gap = Gap {
            kind: GapKind::NumericConversion,
            subject: Some(subject),
            detail: "Exact binary32 range endpoints cannot be represented by NumericBound.".into(),
        };
        if !gaps.contains(&gap) {
            gaps.push(gap);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::numeric::NumericReader;
    use crate::engine::analysis::stop::Unresolved;
    use crate::{
        CommandValue, FieldCondition, FieldDomain, FieldReadAlternative, FieldReference,
        FieldShape, NumericConversion, RepeatBehavior, ValueShape,
    };

    fn numeric_field(reader: Reader) -> Field {
        let shape = FieldShape {
            value: ValueShape::Scalar,
            repeat: RepeatBehavior::Replace,
        };
        Field {
            name: "amount".into(),
            reader: reader.clone(),
            shape,
            read: vec![FieldReadAlternative {
                condition: FieldCondition::Always,
                outcome: FieldReadOutcome::Read { reader, shape },
            }],
            members: FieldMembers::None,
            domain: FieldDomain::Unknown,

            uses: Vec::new(),
            entry_contexts: Vec::new(),
            read_scope: crate::GrammarProperty::Unresolved,
            reference: FieldReference::NotEstablished,
        }
    }

    #[test]
    fn fields_alternatives_and_command_values_reuse_the_same_fact_without_changing_ids() {
        for (callee, representation, bound_gap) in [
            (
                "CReader::Read(int&)",
                crate::NumericRepresentation::Integer,
                false,
            ),
            (
                "CReader::Read(float&)",
                crate::NumericRepresentation::BinaryFloat,
                true,
            ),
            (
                "CReader::Read(float&)",
                crate::NumericRepresentation::BinaryFloat,
                false,
            ),
        ] {
            let id = Some(ReaderId::from_callee(callee));
            let shared = Reader {
                id: id.clone(),
                kind: ReaderKind::Integer,
                family: crate::BlockFamily::NotApplicable,
                numeric: GrammarProperty::Unresolved,
                scoped_operand: crate::GrammarProperty::Unresolved,
            };
            let conversion = GrammarProperty::Partial(Some(NumericConversion {
                representation: GrammarProperty::Known(representation),
                width_bits: GrammarProperty::Known(32),
                ..NumericConversion::default()
            }));
            let facts = NumericFacts {
                token_readers: Default::default(),
                modifier_entry: Err(Unresolved::new("modifier-numeric-not-analyzed")),
                readers: [(
                    callee.into(),
                    NumericReader {
                        conversion: conversion.clone(),
                        gaps: if bound_gap {
                            vec![Unresolved::new(FLOAT_BOUND_REPRESENTATION_GAP)]
                        } else {
                            vec![Unresolved::new("numeric-external-library-conversion")]
                        },
                    },
                )]
                .into(),
            };
            let mut registry = vec![numeric_field(shared.clone())];
            let mut gaps = Vec::new();
            fields(&mut registry, &facts, &[], &mut gaps);
            let mut command = CommandGrammar {
                reader: shared.clone(),
                forms: GrammarProperty::Known(vec![CommandForm::Value(CommandValue {
                    reader: shared,
                    reference: FieldReference::NotEstablished,
                })]),
                fixed_keys: GrammarProperty::Known(vec![numeric_field(registry[0].reader.clone())]),
                targets: GrammarProperty::Unresolved,
                child_families: GrammarProperty::Unresolved,
                child_scopes: GrammarProperty::Unresolved,
                numeric_keys: GrammarProperty::Unresolved,
                ordering: GrammarProperty::Known(vec![crate::ChildOrderRule {
                    child: "amount".into(),
                    conditions: vec![crate::ChildOrderCondition::First(true)],
                    outcome: crate::ChildOrderOutcome::Read(registry[0].reader.clone()),
                }]),
                durations: GrammarProperty::Unresolved,
            };
            let mut nested = command.clone();
            nested.numeric_keys = GrammarProperty::Known(Some(Box::new(command.clone())));
            grammar(&mut nested, "nested", &facts, &mut Vec::new());
            assert!(matches!(
                nested.numeric_keys,
                GrammarProperty::Partial(Some(_))
            ));
            grammar(&mut command, "command", &facts, &mut gaps);
            let FieldReadOutcome::Read {
                reader: alternative,
                ..
            } = &registry[0].read[0].outcome
            else {
                panic!()
            };
            let GrammarProperty::Known(forms) = command.forms else {
                panic!()
            };
            let CommandForm::Value(value) = &forms[0] else {
                panic!()
            };
            let GrammarProperty::Known(ordering) = &command.ordering else {
                panic!()
            };
            let crate::ChildOrderOutcome::Read(ordered) = &ordering[0].outcome else {
                panic!()
            };
            for reader in [
                &registry[0].reader,
                alternative,
                &command.reader,
                &value.reader,
                ordered,
            ] {
                assert_eq!(reader.id, id);
                assert_eq!(reader.numeric, conversion);
            }
            assert!(
                gaps.iter()
                    .all(|gap| gap.kind == GapKind::NumericConversion)
            );
            assert_eq!(
                gaps.iter().any(|gap| gap.detail.contains("NumericBound")),
                bound_gap
            );
        }
    }
    #[test]
    fn unknown_reader_keeps_scoped_operand_unresolved() {
        let mut reader = Reader {
            id: None,
            kind: ReaderKind::Unknown,
            family: crate::BlockFamily::Unknown,
            numeric: GrammarProperty::Unresolved,
            scoped_operand: GrammarProperty::Unresolved,
        };
        attach_reader_facts(&mut reader, &NumericFacts::default());
        assert_eq!(reader.scoped_operand, GrammarProperty::Unresolved);
    }

    #[test]
    fn a_keyword_reader_and_its_alternatives_have_no_numeric_conversion() {
        let reader = Reader {
            id: None,
            kind: ReaderKind::Keyword,
            family: crate::BlockFamily::NotApplicable,
            numeric: GrammarProperty::Unresolved,
            scoped_operand: GrammarProperty::Unresolved,
        };
        let mut values = [numeric_field(reader)];
        let mut gaps = Vec::new();

        fields(&mut values, &NumericFacts::default(), &[], &mut gaps);

        assert_eq!(values[0].reader.numeric, GrammarProperty::Known(None));
        assert!(matches!(
            &values[0].read[0].outcome,
            FieldReadOutcome::Read { reader, .. } if reader.numeric == GrammarProperty::Known(None)
        ));
        assert!(gaps.is_empty(), "{gaps:?}");
    }

    #[test]
    fn a_modifier_block_does_not_inherit_the_numeric_entry_conversion() {
        let mut block = Reader {
            id: Some(ReaderId::from_callee("modifier-block")),
            kind: ReaderKind::Block,
            family: crate::BlockFamily::Modifier,
            numeric: GrammarProperty::Unresolved,
            scoped_operand: crate::GrammarProperty::Unresolved,
        };
        let callee = "CReader::Read(CFixedPoint&)";
        let facts = NumericFacts {
            token_readers: Default::default(),
            readers: [(
                callee.into(),
                NumericReader {
                    conversion: GrammarProperty::Partial(Some(NumericConversion::default())),
                    gaps: Vec::new(),
                },
            )]
            .into(),
            modifier_entry: Ok(crate::engine::analysis::numeric::ModifierNumericEntry {
                shared_callee: callee.into(),
                reader_id: ReaderId::from_callee(callee),
                storage_width_bits: 64,
            }),
        };
        assert!(!attach_reader_facts(&mut block, &facts).incomplete);
        assert_eq!(block.numeric, GrammarProperty::Known(None));
    }

    #[test]
    fn dynamic_numeric_child_gaps_use_the_parent_subject_without_hiding_parent_fields() {
        let number = Reader {
            id: Some(ReaderId::from_callee("number")),
            kind: ReaderKind::Integer,
            family: crate::BlockFamily::NotApplicable,
            numeric: GrammarProperty::Unresolved,
            scoped_operand: crate::GrammarProperty::Unresolved,
        };
        let block = Reader {
            id: None,
            kind: ReaderKind::Block,
            family: crate::BlockFamily::Effect,
            numeric: GrammarProperty::Unresolved,
            scoped_operand: crate::GrammarProperty::Unresolved,
        };
        let parent = CommandGrammar {
            reader: block,
            forms: GrammarProperty::Known(vec![CommandForm::Block]),
            targets: GrammarProperty::Known(vec![]),
            child_scopes: GrammarProperty::Unresolved,
            child_families: GrammarProperty::Known(vec![]),
            fixed_keys: GrammarProperty::Known(vec![numeric_field(number.clone())]),
            numeric_keys: GrammarProperty::Known(None),
            ordering: GrammarProperty::Known(vec![]),
            durations: GrammarProperty::Unresolved,
        };
        let mut nested_field = numeric_field(number.clone());
        nested_field.name = "nested".into();
        nested_field.members = FieldMembers::Fields(vec![numeric_field(number.clone())]);
        let mut child = parent.clone();
        child.fixed_keys =
            GrammarProperty::Known(vec![numeric_field(number.clone()), nested_field]);
        child.ordering = GrammarProperty::Known(vec![crate::ChildOrderRule {
            child: "ordered".into(),
            conditions: vec![crate::ChildOrderCondition::First(true)],
            outcome: crate::ChildOrderOutcome::Read(number),
        }]);
        for numeric_keys in [
            GrammarProperty::Known(Some(Box::new(child.clone()))),
            GrammarProperty::Partial(Some(Box::new(child))),
        ] {
            let mut value = parent.clone();
            value.numeric_keys = numeric_keys;
            let mut gaps = Vec::new();
            grammar(&mut value, "parent", &NumericFacts::default(), &mut gaps);
            assert_eq!(gaps.len(), 2);
            assert!(
                gaps.iter()
                    .all(|gap| gap.kind == GapKind::NumericConversion)
            );
            assert_eq!(gaps[0].subject, Some(GapSubject::field("amount")));
            assert_eq!(gaps[1].subject, Some(GapSubject::answer_item("parent")));
            assert!(matches!(
                value.numeric_keys,
                GrammarProperty::Partial(Some(_))
            ));
        }
    }
    #[test]
    fn modifier_children_and_entries_use_the_shared_normalization_pass() {
        use crate::engine::analysis::fields::ReaderJoin;
        use crate::{ModifierBlock, ModifierEntry};
        let reader = |callee: &str| {
            super::super::fields::reader(&[ReaderJoin::Joined {
                callee: callee.into(),
                arguments: Default::default(),
                tail: false,
            }])
        };
        let conversion = GrammarProperty::Partial(Some(NumericConversion::default()));
        let facts = NumericFacts {
            readers: ["CReader::Read(int&)", "CReader::Read(CFixedPoint&)"]
                .into_iter()
                .map(|callee| {
                    (
                        callee.into(),
                        NumericReader {
                            conversion: conversion.clone(),
                            gaps: vec![Unresolved::new("numeric-overflow")],
                        },
                    )
                })
                .collect(),
            ..NumericFacts::default()
        };
        let integer = numeric_field(reader("CReader::Read(int&)"));
        let mut children: Vec<_> = ["icon", "custom_tooltip", "description"]
            .into_iter()
            .map(|name| {
                let mut field = numeric_field(reader("CReader::Read(CString&, bool)"));
                field.name = name.into();
                field
            })
            .collect();
        children.push(integer.clone());
        let mut parent = numeric_field(reader("CReader::Read(CPersistent&)"));
        parent.name = "modifier".into();
        parent.members = FieldMembers::ModifierBlock(ModifierBlock {
            fixed_keys: GrammarProperty::Partial(children),
            entries: GrammarProperty::Known(vec![ModifierEntry::Numeric {
                value: reader("CReader::Read(CFixedPoint&)"),
            }]),
        });
        let mut gaps = vec![];
        fields(std::slice::from_mut(&mut parent), &facts, &[], &mut gaps);
        let mut outside = vec![integer];
        fields(&mut outside, &facts, &[], &mut vec![]);
        assert_eq!(parent.reader.numeric, GrammarProperty::Known(None));
        let FieldMembers::ModifierBlock(block) = &parent.members else {
            panic!()
        };
        let GrammarProperty::Partial(children) = &block.fixed_keys else {
            panic!()
        };
        for child in &children[..3] {
            assert_eq!(child.reader.numeric, GrammarProperty::Known(None));
            assert_eq!(child.reader.scoped_operand, GrammarProperty::Known(None));
            assert_eq!(child.reference, FieldReference::NotEstablished);
        }
        assert_eq!(children[3], outside[0]);
        assert!(gaps.iter().any(|gap| gap.kind == GapKind::NumericConversion
            && gap.subject == Some(GapSubject::field("modifier"))));
        assert_eq!(
            crate::Completeness::from_gaps(&gaps),
            crate::Completeness::Partial
        );
        let GrammarProperty::Known(entries) = &block.entries else {
            panic!()
        };
        let ModifierEntry::Numeric { value } = &entries[0] else {
            panic!()
        };
        assert_eq!(value.numeric, conversion);
        assert_eq!(value.scoped_operand, GrammarProperty::Known(None));
    }
}
