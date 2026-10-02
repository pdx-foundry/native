//! Normalize loader paths without losing the pairing between conditions and outcomes.
use crate::engine::analysis::fields::{
    ConcreteReader, Condition, PathOutcome, ReaderJoin, RegistryFieldResult, RootField, TokenPath,
    Value,
};
use crate::engine::analysis::readers;
use crate::engine::analysis::references::{
    self, Lookup, ReferenceFacts, initialization::InitializationLookup,
};
use crate::{
    EmptyKey, Field, FieldCondition, FieldDomain, FieldMembers, FieldReadAlternative,
    FieldReadOutcome, FieldReference, FieldShape, KeyMatch, LookupStage, MissingResult, Reader,
    ReaderId, ReaderKind, ReferenceLookup, ReferenceTarget, RepeatBehavior, ValueShape,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub(super) fn reader(joins: &[ReaderJoin]) -> Reader {
    let classification = readers::classify(joins);
    Reader {
        numeric: crate::GrammarProperty::Unresolved,
        scoped_operand: crate::GrammarProperty::Unresolved,
        id: classification.callee.map(ReaderId::from_callee),
        kind: classification.kind,
        family: classification.family,
    }
}

/// One identity for a concrete read/member pair across field and command answers.
pub(super) fn concrete_reader_id(read: &str, member: &str) -> ReaderId {
    let digest = Sha256::digest(format!("{read}:{member}").as_bytes());
    ReaderId(format!("{digest:x}")[..16].into())
}

fn field_reader(joins: &[ReaderJoin], persistent: &BTreeMap<i64, ConcreteReader>) -> Reader {
    let mut alternatives = Vec::new();
    for join in joins {
        let mut selected = reader(std::slice::from_ref(join));
        if matches!(join, ReaderJoin::Joined { callee, .. } if callee == "CReader::Read(CPersistent&)" )
        {
            selected.id = None;
            if let Some(concrete) =
                readers::destination(join).and_then(|offset| persistent.get(&offset))
            {
                selected.id = Some(concrete_reader_id(&concrete.read, &concrete.member));
                selected.family = concrete.family;
            }
        }
        alternatives.push(selected);
    }
    let mut joined = reader(joins);
    if let Some(first) = alternatives.first() {
        joined.id = alternatives
            .iter()
            .all(|reader| reader.id == first.id)
            .then(|| first.id.clone())
            .flatten();
        joined.family = if alternatives
            .iter()
            .all(|reader| reader.family == first.family)
        {
            first.family
        } else {
            crate::BlockFamily::Unknown
        };
    }
    joined
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
            ReaderKind::Boolean
                | ReaderKind::Integer
                | ReaderKind::FixedPoint
                | ReaderKind::Float
                | ReaderKind::String
        );
    FieldShape {
        value,
        repeat: if let ReaderJoin::Stored { repeat, .. } = join {
            *repeat
        } else if replaces {
            RepeatBehavior::Replace
        } else {
            RepeatBehavior::Unknown
        },
    }
}

pub(super) fn field(
    field: &RootField,
    result: &RegistryFieldResult,
    references: &ReferenceFacts,
) -> Field {
    let collection = result
        .collections
        .iter()
        .find(|collection| collection.token == field.token);
    let mut normalized = match collection {
        Some(collection) => collection_field(field, collection, references),
        None => ordinary_field(
            field,
            &result.fields,
            &result.paths,
            &result.persistent,
            references,
        ),
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
/// Normalize command children from their dispatch ledger, without registry storage or uses.
/// `initialization` is the receiver's initialization lookup, which joins the child keys that
/// store its key string; each joined read alternative keeps its own condition.
pub(super) fn grammar_fields(
    fields: &[RootField],
    paths: &[TokenPath],
    references: &ReferenceFacts,
    initialization: Option<&InitializationLookup>,
) -> Vec<Field> {
    let persistent = BTreeMap::new();
    fields
        .iter()
        .map(|field| {
            let mut normalized = ordinary_field(field, fields, paths, &persistent, references);
            if let Some(initialization) = initialization {
                for lookup in initialization_lookups(field, fields, paths, initialization) {
                    normalized.reference =
                        with_lookup(std::mem::take(&mut normalized.reference), lookup);
                }
            }

            normalized
        })
        .collect()
}

/// One lookup for each read alternative of `field` that stores the initialization key.
fn initialization_lookups(
    field: &RootField,
    fields: &[RootField],
    paths: &[TokenPath],
    initialization: &InitializationLookup,
) -> Vec<ReferenceLookup> {
    read_alternatives(field, paths)
        .iter()
        .filter(|(_, outcome)| {
            matches!(outcome, PathOutcome::Reader(join) if stores_key(join, initialization.key_offset))
        })
        .map(|(conditions, _)| {
            reference_lookup(
                condition(conditions, fields),
                initialization.directory.clone(),
                Some(&initialization.lookup),
            )
        })
        .collect()
}

/// Whether a read alternative of `field` stores a string at the owner offset whose text the
/// initialization lookup reads.
pub(super) fn stores_initialization_key(
    field: &RootField,
    initialization: &InitializationLookup,
) -> bool {
    field
        .readers
        .iter()
        .any(|join| stores_key(join, initialization.key_offset))
}

/// Whether `join` is a string read that stores the initialization key. The read must be the
/// path's tail call: an unexamined continuation may overwrite the key before `PostInit()`.
pub(super) fn stores_key(join: &ReaderJoin, destination: i64) -> bool {
    matches!(join, ReaderJoin::Joined { tail: true, .. })
        && readers::classify(std::slice::from_ref(join)).kind == ReaderKind::String
        && readers::destination(join) == Some(destination)
}

fn with_lookup(reference: FieldReference, lookup: ReferenceLookup) -> FieldReference {
    match reference {
        FieldReference::Lookups(mut lookups) => {
            lookups.push(lookup);
            FieldReference::Lookups(lookups)
        }
        _ => FieldReference::Lookups(vec![lookup]),
    }
}

fn ordinary_field(
    field: &RootField,
    fields: &[RootField],
    paths: &[TokenPath],
    persistent: &BTreeMap<i64, ConcreteReader>,
    references: &ReferenceFacts,
) -> Field {
    let alternatives = read_alternatives(field, paths);
    let lookups: Vec<ReferenceLookup> = alternatives
        .iter()
        .filter_map(|(conditions, outcome)| {
            let PathOutcome::Reader(ReaderJoin::Joined { callee, .. }) = outcome else {
                return None;
            };
            references::reader(callee)?;
            let fact = references.readers.get(callee);
            Some(reference_lookup(
                condition(conditions, fields),
                fact.and_then(|fact| fact.directory.clone()),
                fact.and_then(|fact| fact.lookup.as_ref().ok()),
            ))
        })
        .collect();
    let read: Vec<_> = alternatives
        .iter()
        .map(|(conditions, outcome)| {
            let outcome = match outcome {
                PathOutcome::Reader(
                    join @ (ReaderJoin::Joined { .. } | ReaderJoin::Stored { .. }),
                ) => FieldReadOutcome::Read {
                    reader: field_reader(std::slice::from_ref(join), persistent),
                    shape: shape(join),
                },
                PathOutcome::Rejected => FieldReadOutcome::Rejected,
                PathOutcome::Reader(ReaderJoin::Missing(_)) | PathOutcome::Gap(_) => {
                    FieldReadOutcome::Unresolved
                }
            };
            FieldReadAlternative {
                condition: condition(conditions, fields),
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
    let mut reader = field_reader(&field.readers, persistent);
    if read.iter().any(|alternative| match &alternative.outcome {
        FieldReadOutcome::Read {
            reader: alternative,
            ..
        } => alternative.family != reader.family,
        FieldReadOutcome::Unresolved => true,
        FieldReadOutcome::Rejected => false,
    }) {
        reader.family = crate::BlockFamily::Unknown;
    }
    Field {
        name: field.name.clone(),
        reader,
        shape,
        read,
        members: if shape.value == ValueShape::Scalar {
            FieldMembers::None
        } else {
            FieldMembers::Unresolved
        },
        domain: FieldDomain::Unknown,

        uses: Vec::new(),
        reference: if lookups.is_empty() {
            FieldReference::NotEstablished
        } else {
            FieldReference::Lookups(lookups)
        },
    }
}

/// The token paths that read `field`, as condition and outcome pairs with equivalent branches
/// collapsed.
pub(super) fn read_alternatives(
    field: &RootField,
    paths: &[TokenPath],
) -> Vec<(Vec<Condition>, PathOutcome)> {
    let mut alternatives: Vec<_> = paths
        .iter()
        .filter(|path| path.domain[0] <= field.token && field.token <= path.domain[1])
        .map(|path| (path.conditions.clone(), path.outcome.clone()))
        .collect();
    collapse_equivalent_branches(&mut alternatives);

    alternatives
}

/// The public lookup of one reference read. Each fact that the method did not establish stays
/// unresolved; the field's gap names it.
pub(super) fn reference_lookup(
    condition: FieldCondition,
    directory: Option<String>,
    lookup: Option<&Lookup>,
) -> ReferenceLookup {
    let target = match directory {
        Some(name) => ReferenceTarget::Registry { name },
        None => ReferenceTarget::Unresolved,
    };
    let Some(lookup) = lookup else {
        return ReferenceLookup {
            condition,
            target,
            stage: LookupStage::Unresolved,
            key_match: KeyMatch::Unresolved,
            empty_key: EmptyKey::Unresolved,
            on_missing: MissingResult::Unresolved,
        };
    };

    ReferenceLookup {
        condition,
        target,
        stage: match lookup.stage {
            references::Stage::WhileReading => LookupStage::WhileReading,
            references::Stage::Deferred => LookupStage::Deferred,
            references::Stage::OwnerInitialization => LookupStage::OwnerInitialization,
        },
        key_match: match lookup.key_match {
            Some(references::KeyMatch::Equal) => KeyMatch::Equal,
            Some(references::KeyMatch::FirstEqual) => KeyMatch::FirstEqual,
            None => KeyMatch::Unresolved,
        },
        empty_key: match lookup.empty_key_looked_up {
            Some(true) => EmptyKey::LookedUp,
            Some(false) => EmptyKey::NotLookedUp,
            None => EmptyKey::Unresolved,
        },
        on_missing: match lookup.missing_yields_null {
            Some(true) => MissingResult::NullObject,
            _ => MissingResult::Unresolved,
        },
    }
}

/// Why a field's reference lookups are not fully established, when a reference reader reads it.
pub(super) fn reference_gap(field: &RootField, references: &ReferenceFacts) -> Option<String> {
    readers_reference_gap(&field.readers, references)
}

/// Why the lookups of these reader joins are not fully established.
pub(super) fn readers_reference_gap(
    readers: &[ReaderJoin],
    references: &ReferenceFacts,
) -> Option<String> {
    let mut missing = std::collections::BTreeSet::new();
    for join in readers {
        let ReaderJoin::Joined { callee, .. } = join else {
            continue;
        };
        if references::reader(callee).is_none() {
            continue;
        }
        match references.readers.get(callee) {
            None => {
                missing.insert("the reader was not analyzed");
            }
            Some(fact) => {
                if fact.directory.is_none() {
                    missing.insert(NO_DIRECTORY);
                }
                match &fact.lookup {
                    Err(stop) if stop.reason == "reference-list-form" => {
                        missing.insert(
                            "the reader reads a list of keys, whose lookups are not established",
                        );
                    }
                    Err(_) => {
                        missing.insert("no qualified lookup shape matched the reader");
                    }
                    Ok(lookup) if lookup.key_match.is_none() => {
                        missing.insert(UNQUALIFIED_SEARCH);
                    }
                    Ok(_) => {}
                }
            }
        }
    }

    unestablished(missing)
}

/// Why a joined owner-initialization lookup is not fully established.
pub(super) fn initialization_gap(initialization: &InitializationLookup) -> Option<String> {
    let mut missing = std::collections::BTreeSet::new();
    if initialization.directory.is_none() {
        missing.insert(NO_DIRECTORY);
    }
    if initialization.lookup.key_match.is_none() {
        missing.insert(UNQUALIFIED_SEARCH);
    }

    unestablished(missing)
}

const NO_DIRECTORY: &str = "the searched database has no established content directory";
const UNQUALIFIED_SEARCH: &str = "the map search that compares keys is not qualified";

fn unestablished(missing: std::collections::BTreeSet<&str>) -> Option<String> {
    if missing.is_empty() {
        return None;
    }
    let reasons: Vec<&str> = missing.into_iter().collect();

    Some(format!(
        "The field's reference lookup is not fully established: {}.",
        reasons.join("; ")
    ))
}

fn collection_field(
    field: &RootField,
    collection: &crate::engine::analysis::fields::CollectionField,
    references: &ReferenceFacts,
) -> Field {
    let reader = Reader {
        numeric: crate::GrammarProperty::Unresolved,
        scoped_operand: crate::GrammarProperty::Unresolved,
        id: Some(concrete_reader_id(
            "CPersistent::Read(CReader&)",
            &format!("{}::ReadMember(CReader&, int)", collection.class),
        )),
        kind: ReaderKind::Block,
        family: crate::BlockFamily::Unknown,
    };
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
                .map(|child| self::field(child, &collection.fields, references))
                .collect(),
        ),
        domain: FieldDomain::Unknown,

        uses: Vec::new(),
        reference: FieldReference::NotEstablished,
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

fn condition(conditions: &[Condition], fields: &[RootField]) -> FieldCondition {
    let mut terms: Vec<_> = conditions
        .iter()
        .map(|condition| {
            let field = condition
                .value
                .as_ref()
                .and_then(|value| tested_field(value, fields));
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
fn tested_field<'a>(value: &Value, fields: &'a [RootField]) -> Option<&'a RootField> {
    let Value::Load(base, width) = value else {
        return None;
    };
    let Value::Owner(offset) = base.as_ref() else {
        return None;
    };
    let mut matches = fields.iter().filter(|field| {
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

    #[test]
    fn opposite_read_conditions_keep_their_own_reference_targets() {
        use crate::engine::analysis::fields::TokenPath;
        use crate::engine::analysis::references::{Lookup, ReaderLookup, Stage};

        let deferred = |database: &str| {
            format!(
                "void NParserUtil::ReadKeyReferenceDeferred<{database}>(CGlobalDeferredDatabaseObject const&, CReader&, {database}::ValueType const**)"
            )
        };
        let join = |callee: String| {
            PathOutcome::Reader(ReaderJoin::Joined {
                callee,
                arguments: [
                    ("x0".into(), Value::Owner(0)),
                    ("x1".into(), Value::Reader(0)),
                    ("x2".into(), Value::Owner(0x40)),
                ]
                .into(),
                tail: true,
            })
        };
        let path = |zero: bool, outcome: PathOutcome| TokenPath {
            domain: [7, 7],
            conditions: vec![test(zero)],
            instructions: vec![],
            terminal: 0,
            outcome,
        };
        let paths = vec![
            path(true, join(deferred("CShipDatabase"))),
            path(false, join(deferred("CArmyDatabase"))),
        ];
        let flag = RootField {
            name: "flag".into(),
            token: 3,
            constructor: 0,
            paths: vec![],
            readers: vec![match read(8) {
                PathOutcome::Reader(join) => ReaderJoin::Joined {
                    callee: "CReader::Read(bool&)".into(),
                    arguments: match join {
                        ReaderJoin::Joined { arguments, .. } => arguments,
                        ReaderJoin::Missing(_) | ReaderJoin::Stored { .. } => unreachable!(),
                    },
                    tail: true,
                },
                _ => unreachable!(),
            }],
        };
        let target = RootField {
            name: "target".into(),
            token: 7,
            constructor: 0,
            paths: vec![0, 1],
            readers: vec![],
        };
        let lookup = |database: &str, directory: &str| ReaderLookup {
            database: database.into(),
            directory: Some(directory.into()),
            lookup: Ok(Lookup {
                stage: Stage::Deferred,
                key_match: None,
                empty_key_looked_up: Some(true),
                missing_yields_null: Some(true),
            }),
        };
        let references = ReferenceFacts {
            readers: BTreeMap::from([
                (
                    deferred("CShipDatabase"),
                    lookup("CShipDatabase", "common/ships"),
                ),
                (
                    deferred("CArmyDatabase"),
                    lookup("CArmyDatabase", "common/armies"),
                ),
            ]),
            initializers: BTreeMap::new(),
        };
        let fields = [flag, target];
        let field = ordinary_field(&fields[1], &fields, &paths, &BTreeMap::new(), &references);

        let FieldReference::Lookups(lookups) = field.reference else {
            panic!("{:?}", field.reference);
        };
        let targets: Vec<_> = lookups
            .iter()
            .map(|lookup| (lookup.condition.clone(), lookup.target.clone()))
            .collect();
        assert_eq!(
            targets,
            [
                (
                    FieldCondition::FieldZero {
                        path: vec!["flag".into()],
                        zero: true
                    },
                    ReferenceTarget::Registry {
                        name: "common/ships".into()
                    }
                ),
                (
                    FieldCondition::FieldZero {
                        path: vec!["flag".into()],
                        zero: false
                    },
                    ReferenceTarget::Registry {
                        name: "common/armies".into()
                    }
                ),
            ]
        );
        assert!(
            lookups
                .iter()
                .all(|lookup| lookup.key_match == KeyMatch::Unresolved)
        );
    }

    fn string_read(at: i64, tail: bool) -> ReaderJoin {
        ReaderJoin::Joined {
            callee: "CReader::Read(CString&, bool)".into(),
            arguments: [
                ("x0".into(), Value::Reader(0)),
                ("x1".into(), Value::Owner(at)),
                ("x2".into(), Value::Constant(0)),
            ]
            .into(),
            tail,
        }
    }

    fn initialization(key_offset: i64) -> InitializationLookup {
        use crate::engine::analysis::references::Stage;

        InitializationLookup {
            database: "CShipSizeDatabase".into(),
            directory: Some("common/ship_sizes".into()),
            key_offset,
            item_offset: key_offset + 0x28,
            lookup: Lookup {
                stage: Stage::OwnerInitialization,
                key_match: Some(references::KeyMatch::Equal),
                empty_key_looked_up: Some(false),
                missing_yields_null: Some(true),
            },
        }
    }

    fn string_field(readers: Vec<ReaderJoin>) -> RootField {
        RootField {
            name: "size".into(),
            token: 7,
            constructor: 0,
            paths: (0..readers.len()).collect(),
            readers,
        }
    }

    #[test]
    fn only_a_tail_string_read_stores_the_initialization_key() {
        let lookup = initialization(0xa8);

        let continued = string_field(vec![string_read(0xa8, false)]);
        assert!(!stores_initialization_key(&continued, &lookup));

        let tail = string_field(vec![string_read(0xa8, true)]);
        assert!(stores_initialization_key(&tail, &lookup));
    }

    #[test]
    fn an_initialization_lookup_keeps_the_condition_of_the_alternative_that_stores_the_key() {
        let flag = RootField {
            name: "flag".into(),
            token: 3,
            constructor: 0,
            paths: vec![],
            readers: vec![ReaderJoin::Joined {
                callee: "CReader::Read(bool&)".into(),
                arguments: [
                    ("x0".into(), Value::Reader(0)),
                    ("x1".into(), Value::Owner(8)),
                ]
                .into(),
                tail: true,
            }],
        };
        let size = string_field(vec![string_read(0xa8, true), string_read(0xb0, true)]);
        let path = |zero: bool, join: ReaderJoin| TokenPath {
            domain: [7, 7],
            conditions: vec![test(zero)],
            instructions: vec![],
            terminal: 0,
            outcome: PathOutcome::Reader(join),
        };
        let paths = [
            path(true, string_read(0xa8, true)),
            path(false, string_read(0xb0, true)),
        ];
        let references = ReferenceFacts {
            readers: BTreeMap::new(),
            initializers: BTreeMap::new(),
        };

        let fields = grammar_fields(
            &[flag, size],
            &paths,
            &references,
            Some(&initialization(0xa8)),
        );

        let FieldReference::Lookups(lookups) = &fields[1].reference else {
            panic!("{:?}", fields[1].reference);
        };
        let conditions: Vec<_> = lookups.iter().map(|lookup| &lookup.condition).collect();
        assert_eq!(
            conditions,
            [&FieldCondition::FieldZero {
                path: vec!["flag".into()],
                zero: true
            }]
        );
        assert_eq!(lookups[0].stage, LookupStage::OwnerInitialization);
    }
}
