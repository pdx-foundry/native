//! Normalize loader paths without losing the pairing between conditions and outcomes.
use crate::engine::analysis::fields::{
    Condition, PathOutcome, ReaderJoin, RegistryFieldResult, RootField, Value,
};
use crate::engine::analysis::readers;
use crate::{
    Field, FieldCondition, FieldDefault, FieldDomain, FieldMembers, FieldReadAlternative,
    FieldReadOutcome, FieldShape, Reader, ReaderId, ReaderKind, RepeatBehavior, ValueShape,
};
use sha2::{Digest, Sha256};

pub(super) fn reader(joins: &[ReaderJoin]) -> Reader {
    let classification = readers::classify(joins);
    Reader {
        id: classification.callee.map(|callee| {
            let digest = Sha256::digest(callee.as_bytes());
            ReaderId(format!("{digest:x}")[..16].to_owned())
        }),
        kind: classification.kind,
    }
}

fn shape(join: &ReaderJoin) -> FieldShape {
    let classification = readers::classify(std::slice::from_ref(join));
    let value = match classification.kind {
        ReaderKind::Unknown => ValueShape::Unknown,
        ReaderKind::Block => ValueShape::Block,
        _ => ValueShape::Scalar,
    };
    // These primitive readers assign one destination. A block or deferred reference can
    // retain earlier state; a call with an unexamined continuation proves no final storage.
    let replaces = matches!(join, ReaderJoin::Joined { tail: true, .. })
        && matches!(
            classification.kind,
            ReaderKind::Boolean | ReaderKind::Integer | ReaderKind::FixedPoint | ReaderKind::String
        );
    FieldShape {
        value,
        repeat: if replaces {
            RepeatBehavior::Replace
        } else {
            RepeatBehavior::Unknown
        },
    }
}

pub(super) fn field(field: &RootField, result: &RegistryFieldResult) -> Field {
    let collection = result
        .collections
        .iter()
        .find(|collection| collection.token == field.token);
    let mut normalized = match collection {
        Some(collection) => collection_field(field, collection),
        None => ordinary_field(field, result),
    };
    if let FieldMembers::Fields(children) = &mut normalized.members {
        for child in children {
            for alternative in &mut child.read {
                prefix_condition(&mut alternative.condition, &field.name);
            }
        }
    }
    attach_uses(
        &mut normalized,
        std::slice::from_ref(&field.name),
        &result.uses,
    );
    normalized
}
fn ordinary_field(field: &RootField, result: &RegistryFieldResult) -> Field {
    let paths = result
        .paths
        .iter()
        .filter(|path| path.domain[0] <= field.token && field.token <= path.domain[1]);
    let mut alternatives: Vec<_> = paths
        .map(|path| (path.conditions.clone(), path.outcome.clone()))
        .collect();
    collapse_equivalent_branches(&mut alternatives);
    let read: Vec<_> = alternatives
        .iter()
        .map(|(conditions, outcome)| {
            let outcome = match outcome {
                PathOutcome::Reader(join @ ReaderJoin::Joined { .. }) => FieldReadOutcome::Read {
                    reader: reader(std::slice::from_ref(join)),
                    shape: shape(join),
                },
                PathOutcome::Rejected => FieldReadOutcome::Rejected,
                PathOutcome::Reader(ReaderJoin::Missing(_)) | PathOutcome::Gap(_) => {
                    FieldReadOutcome::Unresolved
                }
            };
            FieldReadAlternative {
                condition: condition(conditions, result),
                outcome,
            }
        })
        .collect();
    let unknown = FieldShape {
        value: ValueShape::Unknown,
        repeat: RepeatBehavior::Unknown,
    };
    let shapes: Vec<_> = read
        .iter()
        .filter_map(|alternative| match alternative.outcome {
            FieldReadOutcome::Read { shape, .. } => Some(shape),
            FieldReadOutcome::Rejected => None,
            FieldReadOutcome::Unresolved => Some(unknown),
        })
        .collect();
    let first = shapes.first().copied().unwrap_or(unknown);
    let shape = FieldShape {
        value: if shapes.iter().all(|shape| shape.value == first.value) {
            first.value
        } else {
            ValueShape::Unknown
        },
        repeat: if shapes.iter().all(|shape| shape.repeat == first.repeat) {
            first.repeat
        } else {
            RepeatBehavior::Unknown
        },
    };
    Field {
        name: field.name.clone(),
        reader: reader(&field.readers),
        shape,
        read,
        members: if shape.value == ValueShape::Scalar {
            FieldMembers::None
        } else {
            FieldMembers::Unresolved
        },
        domain: FieldDomain::Unknown,
        default: FieldDefault::Unknown,
        uses: Vec::new(),
    }
}

fn collection_field(
    field: &RootField,
    collection: &crate::engine::analysis::fields::CollectionField,
) -> Field {
    let reader = reader(&[ReaderJoin::Joined {
        callee: "CPersistent::Read(CReader&)".into(),
        arguments: Default::default(),
        tail: false,
    }]);
    let shape = FieldShape {
        value: ValueShape::Block,
        repeat: RepeatBehavior::Accumulate,
    };
    Field {
        name: field.name.clone(),
        read: vec![FieldReadAlternative {
            condition: FieldCondition::Always,
            outcome: FieldReadOutcome::Read {
                reader: reader.clone(),
                shape,
            },
        }],
        reader,
        shape,
        members: FieldMembers::Fields(
            collection
                .fields
                .fields
                .iter()
                .map(|child| self::field(child, &collection.fields))
                .collect(),
        ),
        domain: FieldDomain::Unknown,
        default: FieldDefault::Unknown,
        uses: Vec::new(),
    }
}

fn prefix_condition(condition: &mut FieldCondition, parent: &str) {
    match condition {
        FieldCondition::FieldZero { path, .. } => path.insert(0, parent.into()),
        FieldCondition::All(terms) => {
            for term in terms {
                prefix_condition(term, parent);
            }
        }
        _ => {}
    }
}

fn attach_uses(
    field: &mut Field,
    path: &[String],
    uses: &[crate::engine::analysis::fields::StorageSelection],
) {
    field.uses = uses
        .iter()
        .filter(|selection| selection.field == path)
        .map(|selection| {
            let digest = Sha256::digest(selection.method.as_bytes());
            crate::FieldUse {
                id: crate::FieldUseId(format!("{digest:x}")[..16].into()),
                condition: FieldCondition::All(vec![
                    FieldCondition::Unresolved,
                    FieldCondition::FieldZero {
                        path: selection.tested.clone(),
                        zero: selection.zero,
                    },
                ]),
            }
        })
        .collect();
    if let FieldMembers::Fields(children) = &mut field.members {
        for child in children {
            let mut path = path.to_vec();
            path.push(child.name.clone());
            attach_uses(child, &path, uses);
        }
    }
}

fn same_outcome(left: &PathOutcome, right: &PathOutcome) -> bool {
    match (left, right) {
        (
            PathOutcome::Reader(ReaderJoin::Joined {
                callee,
                arguments,
                tail,
            }),
            PathOutcome::Reader(ReaderJoin::Joined {
                callee: other,
                arguments: other_arguments,
                tail: other_tail,
            }),
        ) if callee == other && tail == other_tail => match readers::call_arguments(callee) {
            Some(registers) => registers.iter().all(|register| {
                arguments
                    .get(*register)
                    .is_some_and(|value| Some(value) == other_arguments.get(*register))
            }),
            None => left == right,
        },
        _ => left == right,
    }
}

/// Complementary tests may cancel only when the complete path outcomes agree, including
/// destination and arguments. A missing reader or stopped path never proves an always-read rule.
fn collapse_equivalent_branches(alternatives: &mut Vec<(Vec<Condition>, PathOutcome)>) {
    for (conditions, _) in alternatives.iter_mut() {
        conditions.retain(|condition| !matches!(condition.value, Some(Value::Constant(_))));
    }
    loop {
        let mut merged = None;
        'pairs: for left in 0..alternatives.len() {
            let (conditions, outcome) = &alternatives[left];
            if !matches!(
                outcome,
                PathOutcome::Reader(ReaderJoin::Joined { .. }) | PathOutcome::Rejected
            ) {
                continue;
            }
            for (right, (other, other_outcome)) in alternatives.iter().enumerate().skip(left + 1) {
                if !same_outcome(outcome, other_outcome) || conditions.len() != other.len() {
                    continue;
                }
                let differences: Vec<_> = conditions
                    .iter()
                    .zip(other)
                    .enumerate()
                    .filter(|(_, (a, b))| a != b)
                    .collect();
                if let [(index, (a, b))] = differences.as_slice()
                    && a.value.is_some()
                    && a.value == b.value
                    && a.at == b.at
                    && a.zero != b.zero
                {
                    merged = Some((left, right, *index));
                    break 'pairs;
                }
            }
        }
        let Some((left, right, term)) = merged else {
            break;
        };
        alternatives[left].0.remove(term);
        alternatives.remove(right);
    }
}

fn condition(conditions: &[Condition], result: &RegistryFieldResult) -> FieldCondition {
    let mut terms: Vec<_> = conditions
        .iter()
        .map(|condition| {
            let field = condition
                .value
                .as_ref()
                .and_then(|value| tested_field(value, result));
            match field {
                Some(field) => FieldCondition::FieldZero {
                    path: vec![field.name.clone()],
                    zero: condition.zero,
                },
                None => FieldCondition::Unresolved,
            }
        })
        .collect();
    match terms.len() {
        0 => FieldCondition::Always,
        1 => terms.remove(0),
        _ => FieldCondition::All(terms),
    }
}

/// Join only a scalar load of exactly the width and location written by one named field.
fn tested_field<'a>(value: &Value, result: &'a RegistryFieldResult) -> Option<&'a RootField> {
    let Value::Load(base, width) = value else {
        return None;
    };
    let Value::Owner(offset) = base.as_ref() else {
        return None;
    };
    let mut matches = result.fields.iter().filter(|field| {
        !field.readers.is_empty()
            && field.readers.iter().all(|join| {
                let ReaderJoin::Joined {
                    callee, arguments, ..
                } = join
                else {
                    return false;
                };
                let scalar_width = match callee.as_str() {
                    "CReader::Read(bool&)"
                    | "CReader::Read(signed char&)"
                    | "CReader::Read(unsigned char&)" => 1,
                    "CReader::Read(short&)" | "CReader::Read(unsigned short&)" => 2,
                    "CReader::Read(int&)" | "CReader::Read(unsigned int&)" => 4,
                    "CReader::Read(long long&)" | "CReader::Read(unsigned long long&)" => 8,
                    _ => return false,
                };
                *width == scalar_width && arguments.get("x1") == Some(&Value::Owner(*offset))
            })
    });
    let found = matches.next()?;
    matches.next().is_none().then_some(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn read(at: i64) -> PathOutcome {
        PathOutcome::Reader(ReaderJoin::Joined {
            callee: "CReader::Read(int&)".into(),
            arguments: [
                ("x0".into(), Value::Reader(0)),
                ("x1".into(), Value::Owner(at)),
            ]
            .into(),
            tail: true,
        })
    }
    fn test(zero: bool) -> Condition {
        Condition {
            at: 4,
            value: Some(Value::Load(Box::new(Value::Owner(8)), 1)),
            zero,
        }
    }
    #[test]
    fn presence_tests_collapse_only_for_equal_outcomes_and_destinations() {
        let mut same = vec![(vec![test(true)], read(12)), (vec![test(false)], read(12))];
        if let PathOutcome::Reader(ReaderJoin::Joined { arguments, .. }) = &mut same[1].1 {
            arguments.insert("x8".into(), Value::Constant(1)); // scratch presence byte
        }
        collapse_equivalent_branches(&mut same);
        assert_eq!(same, vec![(vec![], read(12))]);
        let mut different = vec![(vec![test(true)], read(12)), (vec![test(false)], read(16))];
        collapse_equivalent_branches(&mut different);
        assert_eq!(different.len(), 2);
        different[1].1 = PathOutcome::Rejected;
        collapse_equivalent_branches(&mut different);
        assert_eq!(different.len(), 2);
    }
    #[test]
    fn unknown_tests_never_cancel_to_an_unconditional_read() {
        let mut yes = test(true);
        yes.value = None;
        let mut no = test(false);
        no.value = None;
        let mut alternatives = vec![(vec![yes], read(12)), (vec![no], read(12))];
        collapse_equivalent_branches(&mut alternatives);
        assert_eq!(alternatives.len(), 2);
        assert!(
            alternatives
                .iter()
                .all(|(conditions, _)| !conditions.is_empty())
        );
    }
    #[test]
    fn block_and_unexamined_continuations_do_not_claim_replacement() {
        let join = ReaderJoin::Joined {
            callee: "CReader::Read(CPersistent&)".into(),
            arguments: Default::default(),
            tail: true,
        };
        assert_eq!(shape(&join).repeat, RepeatBehavior::Unknown);
        let join = ReaderJoin::Joined {
            callee: "CReader::Read(int&)".into(),
            arguments: Default::default(),
            tail: false,
        };
        assert_eq!(shape(&join).repeat, RepeatBehavior::Unknown);
    }
}
