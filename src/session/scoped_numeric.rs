//! Attach one shared proof and a constructor-selected subtype to each public reader occurrence.
use crate::engine::analysis::{
    fields::{PathOutcome, ReaderJoin, RegistryFieldResult, RootField, TokenPath},
    grammar::GrammarResult,
    numeric::NumericFacts,
    readers,
    scoped_numeric::{Facts, Subtype},
    stop::Unresolved,
};
use crate::{
    CommandForm, CommandGrammar, Field, FieldMembers, FieldReadOutcome, Gap, GapKind, GapSubject,
    GrammarProperty, Reader, ReaderKind, ScopedOperand, ScopedOperandForm, ScopedReferenceKind,
};
use std::collections::BTreeMap;

/// The paths and destination address points that a field's scoped readers are read through.
pub(super) struct FieldEvidence<'a> {
    pub paths: &'a [TokenPath],
    pub points: &'a BTreeMap<i64, u64>,
    pub facts: &'a Facts,
    pub numeric: &'a NumericFacts,
}

pub(super) fn fields(
    values: &mut [Field],
    result: &RegistryFieldResult,
    facts: &Facts,
    numeric: &NumericFacts,
    gaps: &mut Vec<Gap>,
) {
    fields_at_path(values, result, facts, numeric, &[], gaps);
}

fn fields_at_path(
    values: &mut [Field],
    result: &RegistryFieldResult,
    facts: &Facts,
    numeric: &NumericFacts,
    parent: &[String],
    gaps: &mut Vec<Gap>,
) {
    for (value, root) in values.iter_mut().zip(&result.fields) {
        let mut path = parent.to_vec();
        path.push(value.name.clone());
        let subject = field_subject(&path);
        attach_field(
            value,
            root,
            FieldEvidence {
                paths: &result.paths,
                points: &result.scoped_destinations,
                facts,
                numeric,
            },
            &subject,
            gaps,
        );
        if let FieldMembers::Fields(children) = &mut value.members
            && let Some(collection) = result
                .collections
                .iter()
                .find(|collection| collection.token == root.token)
        {
            fields_at_path(children, &collection.fields, facts, numeric, &path, gaps);
        }
    }
}

fn field_subject(path: &[String]) -> GapSubject {
    if path.len() == 1 {
        GapSubject::field(&path[0])
    } else {
        GapSubject::key_path(path.to_vec())
    }
}

pub(super) fn attach_field(
    value: &mut Field,
    root: &RootField,
    evidence: FieldEvidence<'_>,
    subject: &GapSubject,
    gaps: &mut Vec<Gap>,
) {
    for join in &root.readers {
        if let ReaderJoin::Joined {
            callee, arguments, ..
        } = join
            && callee == "CVariableValue::Assign(CToken const&, EScopeType, CString const&)"
            && !matches!(
                arguments.get("x3"),
                Some(crate::engine::analysis::fields::Value::Owner(_))
            )
        {
            gap(
                gaps,
                GapKind::UnresolvedStorage,
                subject,
                "Assignment source-location provenance is not established.",
            );
        }
    }
    let point = joined_point(&root.readers, evidence.points);
    attach(
        &mut value.reader,
        point,
        evidence.facts,
        evidence.numeric,
        subject,
        gaps,
    );
    for (alternative, (_, outcome)) in value
        .read
        .iter_mut()
        .zip(super::fields::read_alternatives(root, evidence.paths))
    {
        if let (FieldReadOutcome::Read { reader, .. }, PathOutcome::Reader(join)) =
            (&mut alternative.outcome, outcome)
        {
            attach(
                reader,
                readers::destination(&join)
                    .and_then(|offset| evidence.points.get(&offset).copied()),
                evidence.facts,
                evidence.numeric,
                subject,
                gaps,
            );
        }
    }
}

/// Each scoped destination whose vtable point the factory state does not establish, by the
/// child key that reads it, with the trace of the unknown bytes while tracing causes. The
/// command's value forms are under `(value)`.
pub(super) fn state_stops(result: &GrammarResult) -> Vec<(String, Unresolved)> {
    let points = &result.scoped_destinations;
    let fields = result.fields.fields.iter().map(|root| {
        let offsets = root
            .readers
            .iter()
            .filter(|join| is_scoped(join))
            .filter_map(readers::destination);
        (root.name.clone(), offsets.collect::<Vec<_>>())
    });
    let forms = result.forms.iter().map(|forms| {
        let offsets = forms
            .alternatives
            .iter()
            .filter(|alternative| alternative.accepted)
            .filter(|alternative| alternative.value.reader.as_ref().is_some_and(is_scoped))
            .filter_map(|alternative| alternative.value.destination)
            .map(|offset| offset as i64);
        ("(value)".to_string(), offsets.collect())
    });

    fields
        .chain(forms)
        .filter_map(|(key, offsets)| {
            let missing: Vec<_> = offsets
                .into_iter()
                .filter(|offset| !points.contains_key(offset))
                .collect();
            if missing.is_empty() {
                return None;
            }
            let mut traces = missing
                .iter()
                .filter_map(|&offset| result.state_trace(offset, 8));
            let mut trace = traces.next();
            if let Some(trace) = &mut trace {
                for other in traces {
                    trace.merge(&other);
                }
            }

            Some((
                key,
                Unresolved::new("scoped-destination-state").traced(trace),
            ))
        })
        .collect()
}

fn is_scoped(join: &ReaderJoin) -> bool {
    matches!(join, ReaderJoin::Joined { callee, .. }
        if readers::classify_callee(callee) == ReaderKind::ScopedNumeric)
}

fn joined_point(joins: &[ReaderJoin], points: &BTreeMap<i64, u64>) -> Option<u64> {
    let mut selected = None;
    for join in joins {
        let point = readers::destination(join).and_then(|offset| points.get(&offset).copied())?;
        if selected.is_some_and(|previous| previous != point) {
            return None;
        }
        selected = Some(point);
    }
    selected
}

pub(super) fn grammar(
    value: &mut CommandGrammar,
    result: &GrammarResult,
    facts: &Facts,
    numeric: &NumericFacts,
    name: &str,
    gaps: &mut Vec<Gap>,
) {
    let subject = GapSubject::answer_item(name);
    if value.reader.kind == ReaderKind::ScopedNumeric {
        attach(
            &mut value.reader,
            result.scoped_destinations.get(&0).copied(),
            facts,
            numeric,
            &subject,
            gaps,
        );
    }
    if let GrammarProperty::Known(keys) | GrammarProperty::Partial(keys) = &mut value.fixed_keys {
        grammar_fields(keys, result, facts, numeric, &[], gaps);
    }
    if let (GrammarProperty::Known(forms) | GrammarProperty::Partial(forms), Some(internal)) =
        (&mut value.forms, &result.forms)
    {
        for (form, alternative) in forms
            .iter_mut()
            .filter_map(|form| match form {
                CommandForm::Value(value) => Some(value),
                _ => None,
            })
            .zip(
                internal
                    .alternatives
                    .iter()
                    .filter(|alternative| alternative.accepted),
            )
        {
            let point = alternative
                .value
                .destination
                .and_then(|offset| result.scoped_destinations.get(&(offset as i64)).copied());
            attach(&mut form.reader, point, facts, numeric, &subject, gaps);
        }
    }
    if let (
        GrammarProperty::Known(Some(child)) | GrammarProperty::Partial(Some(child)),
        Some(internal),
    ) = (&mut value.numeric_keys, &result.numeric)
    {
        grammar(child, internal, facts, numeric, name, gaps);
    }
}

fn grammar_fields(
    values: &mut [Field],
    result: &GrammarResult,
    facts: &Facts,
    numeric: &NumericFacts,
    parent: &[String],
    gaps: &mut Vec<Gap>,
) {
    for (value, root) in values.iter_mut().zip(&result.fields.fields) {
        let mut path = parent.to_vec();
        path.push(value.name.clone());
        let subject = field_subject(&path);
        attach_field(
            value,
            root,
            FieldEvidence {
                paths: &result.fields.paths,
                points: &result.scoped_destinations,
                facts,
                numeric,
            },
            &subject,
            gaps,
        );
        if let FieldMembers::Fields(children) = &mut value.members
            && let Some(child) = result.nested.get(&value.name)
        {
            grammar_fields(children, child, facts, numeric, &path, gaps);
        }
    }
}

pub(super) fn attach(
    reader: &mut Reader,
    point: Option<u64>,
    facts: &Facts,
    numeric: &NumericFacts,
    subject: &GapSubject,
    gaps: &mut Vec<Gap>,
) {
    if reader.kind != ReaderKind::ScopedNumeric {
        return;
    }
    let Some(point) = point else {
        gap(
            gaps,
            GapKind::UnresolvedStorage,
            subject,
            "Scoped destination vtable is not established.",
        );
        return;
    };
    let Some(Ok(subtype)) = facts.subtypes.get(&point) else {
        gap(
            gaps,
            GapKind::UnresolvedStorage,
            subject,
            "Scoped destination subtype is not established.",
        );
        return;
    };
    let literal = match subtype {
        Subtype::Base => false,
        Subtype::Numeric {
            token_reader,
            literal,
        } => {
            let Some(conversion) = numeric.token_readers.get(token_reader) else {
                gap(
                    gaps,
                    GapKind::NumericConversion,
                    subject,
                    "Scoped literal token conversion is not established.",
                );
                return;
            };
            reader.numeric = conversion.conversion.clone();
            super::numeric::push_gaps(gaps, subject.clone(), &super::numeric::limits(conversion));
            if facts
                .shared
                .selection
                .as_ref()
                .is_ok_and(|layout| *literal != layout.literal)
            {
                gap(
                    gaps,
                    GapKind::UnresolvedStorage,
                    subject,
                    "Scoped literal offset disagrees with GetValue.",
                );
                return;
            }
            true
        }
    };
    if !literal {
        reader.numeric = GrammarProperty::Known(None);
    }
    let forms = match &facts.shared.forms {
        Ok(forms) => {
            let mut values = Vec::new();
            if literal {
                values.push(ScopedOperandForm::Literal);
            }
            for (prefix, kind) in forms.prefixes.iter().zip([
                ScopedReferenceKind::Trigger,
                ScopedReferenceKind::Modifier,
                ScopedReferenceKind::ScriptValue,
            ]) {
                values.push(ScopedOperandForm::Prefixed {
                    kind,
                    prefix: format!("{prefix}{}", forms.separator),
                });
            }
            values.push(ScopedOperandForm::Variable);
            GrammarProperty::Partial(values)
        }
        Err(_) => {
            gap(
                gaps,
                GapKind::ReaderSemantics,
                subject,
                "Scoped operand forms are not established.",
            );
            GrammarProperty::Unresolved
        }
    };
    reader.scoped_operand = GrammarProperty::Known(Some(ScopedOperand { forms }));
    gap(
        gaps,
        GapKind::OutsideMethod,
        subject,
        "Qualified scope and parameter grammar, reference lookup outcomes, and evaluated values remain outside this operand method.",
    );
}

fn gap(gaps: &mut Vec<Gap>, kind: GapKind, subject: &GapSubject, detail: &str) {
    let gap = Gap {
        kind,
        subject: Some(subject.clone()),
        detail: detail.into(),
    };
    if !gaps.contains(&gap) {
        gaps.push(gap);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NumericConversion;
    use crate::engine::analysis::{
        numeric::NumericReader,
        scoped_numeric::{Forms, Layout, Shared},
        stop::Unresolved,
    };

    fn reader() -> Reader {
        Reader {
            numeric: GrammarProperty::Unresolved,
            scoped_operand: GrammarProperty::Unresolved,
            id: Some(crate::ReaderId::from_callee(
                "CVariableValue::Read(CReader&, EScopeType)",
            )),
            kind: ReaderKind::ScopedNumeric,
            family: crate::BlockFamily::NotApplicable,
        }
    }

    fn facts(subtype: Subtype) -> Facts {
        Facts {
            shared: Shared {
                forms: Ok(Forms {
                    prefixes: ["trigger".into(), "modifier".into(), "value".into()],
                    separator: ":".into(),
                }),

                selection: Ok(Layout {
                    literal: 0x200,
                    location: 0x1d8,
                    trigger: 0x1a0,
                    script_value: 0x1a8,
                    modifier: 0x198,
                    modifier_unset: 0x7fffffff,
                    variable: 0x1b0,
                }),
            },
            subtypes: [(0x8000, Ok(subtype))].into(),
        }
    }

    #[test]
    fn attached_literal_is_conditional_and_preserves_its_shared_identity() {
        let mut reader = reader();
        let conversion = GrammarProperty::Partial(Some(NumericConversion {
            accepted_range: GrammarProperty::Known(Box::new(crate::NumericRange {
                minimum: GrammarProperty::Known(crate::NumericBound::Signed(-2147483648)),
                maximum: GrammarProperty::Known(crate::NumericBound::Signed(2147483647)),
            })),
            ..NumericConversion::default()
        }));
        let numeric = NumericFacts {
            token_readers: [(
                "CToken::ReadValue(int&) const".into(),
                NumericReader {
                    boundary: Vec::new(),
                    conversion: conversion.clone(),
                    gaps: vec![Unresolved::new("numeric-overflow")],
                },
            )]
            .into(),
            ..NumericFacts::default()
        };
        let facts = facts(Subtype::Numeric {
            token_reader: "CToken::ReadValue(int&) const".into(),
            literal: 0x200,
        });
        let identity = reader.id.clone();
        let mut gaps = Vec::new();
        attach(
            &mut reader,
            Some(0x8000),
            &facts,
            &numeric,
            &GapSubject::field("cost"),
            &mut gaps,
        );
        assert_eq!(reader.id, identity);
        assert_eq!(reader.numeric, conversion);
        let GrammarProperty::Known(Some(operand)) = reader.scoped_operand else {
            panic!("missing operand");
        };
        assert!(
            matches!(operand.forms, GrammarProperty::Partial(ref forms) if forms.contains(&ScopedOperandForm::Literal))
        );
        assert!(
            gaps.iter()
                .any(|gap| gap.kind == GapKind::NumericConversion)
        );
        assert!(!gaps.iter().any(|gap| gap.kind == GapKind::ReaderSemantics));
    }

    #[test]
    fn a_scoped_literal_inherits_the_numeric_classification() {
        let literal = |signedness| {
            GrammarProperty::Partial(Some(NumericConversion {
                representation: GrammarProperty::Known(crate::NumericRepresentation::Integer),
                width_bits: GrammarProperty::Known(32),
                signedness,
                scale: GrammarProperty::Known(Some(1)),
                literal_syntax: GrammarProperty::Partial(Vec::new()),
                accepted_range: GrammarProperty::Known(Box::new(crate::NumericRange {
                    minimum: GrammarProperty::Known(crate::NumericBound::Signed(-2147483648)),
                    maximum: GrammarProperty::Known(crate::NumericBound::Signed(2147483647)),
                })),
            }))
        };
        let facts = facts(Subtype::Numeric {
            token_reader: "CToken::ReadValue(int&) const".into(),
            literal: 0x200,
        });
        for (signedness, typed) in [
            (
                GrammarProperty::Known(crate::NumericSignedness::Signed),
                false,
            ),
            (GrammarProperty::Unresolved, true),
        ] {
            let numeric = NumericFacts {
                token_readers: [(
                    "CToken::ReadValue(int&) const".into(),
                    NumericReader {
                        conversion: literal(signedness),
                        gaps: Vec::new(),
                        boundary: vec![Unresolved::new("numeric-overflow")],
                    },
                )]
                .into(),
                ..NumericFacts::default()
            };
            let mut gaps = Vec::new();
            attach(
                &mut reader(),
                Some(0x8000),
                &facts,
                &numeric,
                &GapSubject::field("cost"),
                &mut gaps,
            );

            assert_eq!(
                gaps.iter()
                    .any(|gap| gap.kind == GapKind::NumericConversion),
                typed
            );
            assert!(
                gaps.iter().any(|gap| gap.kind == GapKind::OutsideMethod
                    && gap.detail.contains("platform scanner"))
            );
        }
    }

    #[test]
    fn missing_subtype_and_base_subtype_do_not_claim_literal_storage() {
        let facts = facts(Subtype::Base);
        let mut missing = reader();
        let mut gaps = Vec::new();
        attach(
            &mut missing,
            None,
            &facts,
            &NumericFacts::default(),
            &GapSubject::field("cost"),
            &mut gaps,
        );
        assert_eq!(missing.scoped_operand, GrammarProperty::Unresolved);
        assert!(
            gaps.iter()
                .any(|gap| gap.kind == GapKind::UnresolvedStorage)
        );

        let mut base = reader();
        let mut gaps = Vec::new();
        attach(
            &mut base,
            Some(0x8000),
            &facts,
            &NumericFacts::default(),
            &GapSubject::field("cost"),
            &mut gaps,
        );
        assert_eq!(base.numeric, GrammarProperty::Known(None));
        let GrammarProperty::Known(Some(operand)) = base.scoped_operand else {
            panic!("missing base operand");
        };
        assert!(!gaps.iter().any(|gap| gap.kind == GapKind::ReaderSemantics));
        assert!(
            matches!(operand.forms, GrammarProperty::Partial(ref forms) if !forms.contains(&ScopedOperandForm::Literal))
        );
    }
}
