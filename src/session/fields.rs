//! Normalize loader paths without losing the pairing between conditions and outcomes.
use crate::engine::analysis::fields::{
    ConcreteReader, Condition, PathOutcome, ReaderJoin, RegistryFieldResult, RootField, TokenPath,
    Value,
};
use crate::engine::analysis::readers;
use crate::engine::analysis::references::{
    self, Lookup, Missing, ReferenceFacts, initialization::InitializationLookup,
};
use crate::{
    AcceptedCategories, BlockFamily, EmptyKey, Field, FieldCondition, FieldDomain, FieldMembers,
    FieldReadAlternative, FieldReadOutcome, FieldReference, FieldShape, KeyMatch, LookupStage,
    MissingResult, Reader, ReaderId, ReaderKind, ReferenceLookup, ReferenceTarget, RepeatBehavior,
    ValueShape,
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

/// The identity of a persistent reader. A bound delegate tells apart readers that share their
/// read and member.
pub(super) fn persistent_reader_id(reader: &ConcreteReader) -> ReaderId {
    match &reader.delegate {
        None => concrete_reader_id(&reader.read, &reader.member),
        Some(delegate) => {
            concrete_reader_id(&reader.read, &format!("{}:{delegate}", reader.member))
        }
    }
}

/// Fields of a block whose persistent destinations, by owner offset, hold these readers.
pub(super) fn embedded_fields(
    fields: &[RootField],
    paths: &[TokenPath],
    references: &ReferenceFacts,
    persistent: &BTreeMap<i64, ConcreteReader>,
) -> Vec<Field> {
    fields
        .iter()
        .map(|field| ordinary_field(field, fields, paths, persistent, references))
        .collect()
}

/// The reader of one path. A persistent read takes the concrete identity of its destination, or
/// none when the destination has no known reader.
pub(super) fn alternative_reader(
    join: &ReaderJoin,
    persistent: &BTreeMap<i64, ConcreteReader>,
) -> Reader {
    let mut selected = reader(std::slice::from_ref(join));
    if matches!(join, ReaderJoin::Joined { callee, .. } if callee == "CReader::Read(CPersistent&)" )
    {
        selected.id = None;
        if let Some(concrete) =
            readers::destination(join).and_then(|offset| persistent.get(&offset))
        {
            selected.id = Some(persistent_reader_id(concrete));
            selected.family = concrete.family;
        }
    }
    selected
}

fn field_reader(joins: &[ReaderJoin], persistent: &BTreeMap<i64, ConcreteReader>) -> Reader {
    let alternatives: Vec<_> = joins
        .iter()
        .map(|join| alternative_reader(join, persistent))
        .collect();
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
            BlockFamily::Unknown
        };
    }
    joined
}

fn shape(join: &ReaderJoin, reader: &Reader, references: &ReferenceFacts) -> FieldShape {
    let value = match reader.kind {
        ReaderKind::Unknown => ValueShape::Unknown,
        ReaderKind::Block => ValueShape::Block,
        _ => ValueShape::Scalar,
    };

    FieldShape {
        value,
        repeat: repeat(join, reader, references),
    }
}

/// How a second occurrence read by `join` affects storage. Each rule is a stated fact about one
/// shared reader on M452; the method reads no reader body for it. A call with an unexamined
/// continuation proves no final storage, so only a tail call earns a rule.
fn repeat(join: &ReaderJoin, reader: &Reader, references: &ReferenceFacts) -> RepeatBehavior {
    let callee = match join {
        ReaderJoin::Stored { repeat, .. } => return *repeat,
        ReaderJoin::Joined {
            callee, tail: true, ..
        } => callee,
        _ => return RepeatBehavior::Unknown,
    };

    match reader.kind {
        ReaderKind::Boolean
        | ReaderKind::Integer
        | ReaderKind::FixedPoint
        | ReaderKind::Float
        | ReaderKind::String => RepeatBehavior::Replace,
        ReaderKind::Block if readers::clears_before_reading(callee) => RepeatBehavior::Replace,
        ReaderKind::Reference if stores_last_deferred_key(callee, references) => {
            RepeatBehavior::Replace
        }
        // A literal replaces the literal slot; variable, trigger and script-value slots stay.
        ReaderKind::ScopedNumeric if callee == "CVariableValue::Read(CReader&, EScopeType)" => {
            RepeatBehavior::Merges
        }
        // The persistent read of a weight or modifier block keeps the object: a weight block
        // keeps its entries and a modifier block its fixed keys.
        ReaderKind::Block
            if matches!(reader.family, BlockFamily::Weight | BlockFamily::Modifier) =>
        {
            RepeatBehavior::Merges
        }
        _ => RepeatBehavior::Unknown,
    }
}

/// Whether `callee` registers one deferred key whose resolver lambda writes the destination on
/// a hit and on a miss. The resolver runs registrations in order, so the last occurrence's item
/// is the one stored.
fn stores_last_deferred_key(callee: &str, references: &ReferenceFacts) -> bool {
    let deferred = references::reader(callee)
        .is_some_and(|reader| reader.form == references::ReaderForm::Deferred);
    let writes_every_resolution = references.readers.get(callee).is_some_and(|fact| {
        matches!(
            &fact.lookup,
            Ok(Lookup {
                stage: references::Stage::Deferred,
                on_missing: Some(Missing::NullObject),
                ..
            })
        )
    });

    deferred && writes_every_resolution
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
            prefix_conditions(child, &field.name);
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
                ) => {
                    let reader = field_reader(std::slice::from_ref(join), persistent);
                    FieldReadOutcome::Read {
                        shape: shape(join, &reader, references),
                        reader,
                    }
                }
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
    // `field.readers` holds only the field's own token paths; a wider path that also reads
    // the token must not leave a shared reader claim that it contradicts.
    let mut reader = field_reader(&field.readers, persistent);
    for alternative in &read {
        match &alternative.outcome {
            FieldReadOutcome::Read {
                reader: alternative,
                ..
            } => {
                if alternative.id != reader.id {
                    reader.id = None;
                }
                if alternative.kind != reader.kind {
                    reader.kind = ReaderKind::Unknown;
                }
                if alternative.family != reader.family {
                    reader.family = BlockFamily::Unknown;
                }
            }
            FieldReadOutcome::Unresolved => {
                reader.id = None;
                reader.kind = ReaderKind::Unknown;
                reader.family = BlockFamily::Unknown;
            }
            FieldReadOutcome::Rejected => {}
        }
    }
    let accepted_categories = accepted_categories(reader.family);
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
        entry_contexts: Vec::new(),
        read_scope: crate::GrammarProperty::Unresolved,
        accepted_categories,
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
        on_missing: match lookup.on_missing {
            Some(Missing::NullObject) => MissingResult::NullObject,
            Some(Missing::ScriptedTriggerPlaceholder) => MissingResult::ScriptedTriggerPlaceholder,
            None => MissingResult::Unresolved,
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
    let (id, family, members) = match &collection.reader {
        Some(bound) => (
            persistent_reader_id(&bound.reader),
            bound.reader.family,
            FieldMembers::Unresolved,
        ),
        None => (
            concrete_reader_id(
                "CPersistent::Read(CReader&)",
                &format!("{}::ReadMember(CReader&, int)", collection.class),
            ),
            BlockFamily::Unknown,
            FieldMembers::Fields(
                collection
                    .fields
                    .fields
                    .iter()
                    .map(|child| self::field(child, &collection.fields, references))
                    .collect(),
            ),
        ),
    };
    let reader = Reader {
        numeric: crate::GrammarProperty::Unresolved,
        scoped_operand: crate::GrammarProperty::Unresolved,
        id: Some(id),
        kind: ReaderKind::Block,
        family,
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
        members,
        domain: FieldDomain::Unknown,

        uses: Vec::new(),
        entry_contexts: Vec::new(),
        read_scope: crate::GrammarProperty::Unresolved,
        accepted_categories: match (&collection.reader, &collection.container_mask) {
            (None, None) => AcceptedCategories::NotApplicable,
            _ => accepted_categories(family),
        },
        reference: FieldReference::NotEstablished,
    }
}

/// A field's container categories before the container pass: a modifier reader reads a
/// container, an unknown reader may, and an established reader of another family does not.
fn accepted_categories(family: BlockFamily) -> AcceptedCategories {
    match family {
        BlockFamily::Modifier | BlockFamily::TriggeredModifier | BlockFamily::Unknown => {
            AcceptedCategories::Unresolved
        }
        _ => AcceptedCategories::NotApplicable,
    }
}

/// Prefix every field path that a condition of `field` or its members tests with `parent`, the
/// block field that encloses `field`.
pub(super) fn prefix_conditions(field: &mut Field, parent: &str) {
    for alternative in &mut field.read {
        prefix_condition(&mut alternative.condition, parent);
    }

    if let FieldReference::Lookups(lookups) = &mut field.reference {
        for lookup in lookups {
            prefix_condition(&mut lookup.condition, parent);
        }
    }

    let members = match &mut field.members {
        FieldMembers::Fields(members) => members,
        FieldMembers::ModifierBlock(crate::ModifierBlock {
            fixed_keys:
                crate::GrammarProperty::Known(members) | crate::GrammarProperty::Partial(members),
            ..
        }) => members,
        _ => return,
    };
    for member in members {
        prefix_conditions(member, parent);
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
    fn a_bound_delegate_tells_apart_readers_that_share_read_and_member() {
        let reader = |delegate: Option<&str>| ConcreteReader {
            read: "CPersistent::Read(CReader&)".into(),
            member: "Base::ReadMember(CReader&, int)".into(),
            family: BlockFamily::TriggeredModifier,
            delegate: delegate.map(Into::into),
        };
        let undelegated = persistent_reader_id(&reader(None));

        assert_eq!(
            undelegated,
            concrete_reader_id(
                "CPersistent::Read(CReader&)",
                "Base::ReadMember(CReader&, int)"
            )
        );
        assert_ne!(
            persistent_reader_id(&reader(Some("Base::ReadModifier"))),
            undelegated
        );
        assert_ne!(
            persistent_reader_id(&reader(Some("Base::ReadModifier"))),
            persistent_reader_id(&reader(Some("Derived::ReadModifier")))
        );
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
    fn each_reader_rule_states_repeat_only_for_its_own_tail_read() {
        use crate::engine::analysis::references::{Lookup, ReaderLookup, Stage};
        use crate::engine::analysis::stop::Unresolved;
        const TRIGGER: &str =
            "void NParserUtil::ReadTrigger<CRootTrigger>(CReader&, CRootTrigger&, EScopeType)";
        const MODIFIER: &str = "void NParserUtil::ReadKeyReferenceDeferred<CStaticModifierDatabase>(CGlobalDeferredDatabaseObject const&, CReader&, CStaticModifierDatabase::ValueType const**)";
        const SHIP: &str = "void NParserUtil::ReadKeyReferenceDeferred<CShipDatabase>(CGlobalDeferredDatabaseObject const&, CReader&, CShipDatabase::ValueType const**)";
        let references = ReferenceFacts {
            readers: BTreeMap::from([
                (
                    MODIFIER.to_owned(),
                    ReaderLookup {
                        database: "CStaticModifierDatabase".into(),
                        directory: None,
                        lookup: Ok(Lookup {
                            stage: Stage::Deferred,
                            key_match: None,
                            empty_key_looked_up: Some(true),
                            on_missing: Some(Missing::NullObject),
                        }),
                    },
                ),
                (
                    SHIP.to_owned(),
                    ReaderLookup {
                        database: "CShipDatabase".into(),
                        directory: None,
                        lookup: Err(Unresolved::new("reference-lambda-shape")),
                    },
                ),
            ]),
            ..Default::default()
        };
        let repeat_of = |callee: &str, tail: bool, family: BlockFamily| {
            let join = ReaderJoin::Joined {
                callee: callee.into(),
                arguments: Default::default(),
                tail,
            };
            let mut reader = reader(std::slice::from_ref(&join));
            reader.family = family;

            shape(&join, &reader, &references).repeat
        };
        let persistent = "CReader::Read(CPersistent&)";
        let scoped = "CVariableValue::Read(CReader&, EScopeType)";
        let cases = [
            (
                "CReader::Read(int&)",
                true,
                BlockFamily::NotApplicable,
                RepeatBehavior::Replace,
            ),
            (
                "CReader::Read(int&)",
                false,
                BlockFamily::NotApplicable,
                RepeatBehavior::Unknown,
            ),
            (TRIGGER, true, BlockFamily::Trigger, RepeatBehavior::Replace),
            (
                TRIGGER,
                false,
                BlockFamily::Trigger,
                RepeatBehavior::Unknown,
            ),
            (
                "CTrigger::Read(CReader&, EScopeType)",
                true,
                BlockFamily::Trigger,
                RepeatBehavior::Unknown,
            ),
            (
                scoped,
                true,
                BlockFamily::NotApplicable,
                RepeatBehavior::Merges,
            ),
            (
                MODIFIER,
                true,
                BlockFamily::NotApplicable,
                RepeatBehavior::Replace,
            ),
            (
                SHIP,
                true,
                BlockFamily::NotApplicable,
                RepeatBehavior::Unknown,
            ),
            (
                persistent,
                true,
                BlockFamily::Weight,
                RepeatBehavior::Merges,
            ),
            (
                persistent,
                true,
                BlockFamily::Modifier,
                RepeatBehavior::Merges,
            ),
            (
                persistent,
                true,
                BlockFamily::Unknown,
                RepeatBehavior::Unknown,
            ),
        ];
        for (callee, tail, family, expected) in cases {
            assert_eq!(
                repeat_of(callee, tail, family),
                expected,
                "{callee} tail={tail} {family:?}"
            );
        }
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
                on_missing: Some(Missing::NullObject),
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
            ..Default::default()
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
                on_missing: Some(Missing::NullObject),
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
        let references = ReferenceFacts::default();

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

    fn path(domain: [i64; 2], conditions: Vec<Condition>, outcome: PathOutcome) -> TokenPath {
        TokenPath {
            domain,
            conditions,
            instructions: vec![],
            terminal: 0,
            outcome,
        }
    }

    #[test]
    fn a_wide_unresolved_path_leaves_no_shared_reader_claim() {
        use crate::engine::analysis::stop::Unresolved;

        let PathOutcome::Reader(join) = read(12) else {
            unreachable!()
        };
        let field = RootField {
            name: "count".into(),
            token: 7,
            constructor: 0,
            paths: vec![0],
            readers: vec![join],
        };
        let fields = [field.clone()];
        let references = ReferenceFacts::default();
        let normalize = |wide: PathOutcome| {
            let paths = [path([7, 7], vec![], read(12)), path([6, 8], vec![], wide)];
            ordinary_field(&field, &fields, &paths, &BTreeMap::new(), &references)
        };

        let unresolved = normalize(PathOutcome::Gap(Unresolved::new("wide-path")));
        assert_eq!(unresolved.reader.id, None);
        assert_eq!(unresolved.reader.kind, ReaderKind::Unknown);
        let FieldReadOutcome::Read { reader: known, .. } = &unresolved.read[0].outcome else {
            panic!("{:?}", unresolved.read[0]);
        };
        assert_eq!(known.id, Some(ReaderId::from_callee("CReader::Read(int&)")));
        assert_eq!(known.kind, ReaderKind::Integer);
        assert_eq!(unresolved.read[1].outcome, FieldReadOutcome::Unresolved);

        let boolean = normalize(PathOutcome::Reader(ReaderJoin::Joined {
            callee: "CReader::Read(bool&)".into(),
            arguments: [("x1".into(), Value::Owner(12))].into(),
            tail: true,
        }));
        assert_eq!(boolean.reader.id, None);
        assert_eq!(boolean.reader.kind, ReaderKind::Unknown);

        let rejected = normalize(PathOutcome::Rejected);
        assert_eq!(rejected.reader, known.clone());
        assert_eq!(rejected.read[1].outcome, FieldReadOutcome::Rejected);
    }

    #[test]
    fn a_collection_child_reference_names_the_same_condition_path_as_its_read() {
        use crate::engine::analysis::fields::CollectionField;
        use crate::engine::analysis::references::{Lookup, ReaderLookup, Stage};

        let deferred = "void NParserUtil::ReadKeyReferenceDeferred<CShipDatabase>(CGlobalDeferredDatabaseObject const&, CReader&, CShipDatabase::ValueType const**)";
        let target = PathOutcome::Reader(ReaderJoin::Joined {
            callee: deferred.into(),
            arguments: [
                ("x0".into(), Value::Owner(0)),
                ("x1".into(), Value::Reader(0)),
                ("x2".into(), Value::Owner(0x40)),
            ]
            .into(),
            tail: true,
        });
        let PathOutcome::Reader(target_join) = target.clone() else {
            unreachable!()
        };
        let flag = RootField {
            name: "flag".into(),
            token: 3,
            constructor: 0,
            paths: vec![],
            readers: vec![ReaderJoin::Joined {
                callee: "CReader::Read(bool&)".into(),
                arguments: [("x1".into(), Value::Owner(8))].into(),
                tail: true,
            }],
        };
        let target_field = RootField {
            name: "target".into(),
            token: 7,
            constructor: 0,
            paths: vec![0],
            readers: vec![target_join],
        };
        let result = |fields, paths, collections| RegistryFieldResult {
            persistent: BTreeMap::new(),
            persistent_points: BTreeMap::new(),
            scoped_destinations: BTreeMap::new(),
            stored_words: BTreeMap::new(),
            container_masks: Default::default(),
            uses: vec![],
            collections,
            fields,
            paths,
            gaps: vec![],
            partition_accounted: true,
        };
        let child = result(
            vec![flag, target_field],
            vec![path([7, 7], vec![test(true)], target)],
            vec![],
        );
        let parent = RootField {
            name: "entry".into(),
            token: 5,
            constructor: 0,
            paths: vec![],
            readers: vec![],
        };
        let registry = result(
            vec![parent.clone()],
            vec![],
            vec![CollectionField {
                token: 5,
                offset: 0x20,
                data_offset: None,
                count_offset: None,
                container_mask: None,
                class: "CEntry".into(),
                reader: None,
                fields: Box::new(child),
            }],
        );
        let references = ReferenceFacts {
            readers: BTreeMap::from([(
                deferred.to_owned(),
                ReaderLookup {
                    database: "CShipDatabase".into(),
                    directory: Some("common/ships".into()),
                    lookup: Ok(Lookup {
                        stage: Stage::Deferred,
                        key_match: None,
                        empty_key_looked_up: Some(true),
                        on_missing: Some(Missing::NullObject),
                    }),
                },
            )]),
            initializers: BTreeMap::new(),
            ..Default::default()
        };

        let mut normalized = field(&parent, &registry, &references);
        let mut scope_gaps = Vec::new();
        super::super::read_scope::registry_fields(
            std::slice::from_mut(&mut normalized),
            &registry,
            None,
            &[],
            &mut scope_gaps,
        );

        let FieldMembers::Fields(children) = &normalized.members else {
            panic!("{:?}", normalized.members);
        };
        assert_eq!(
            children[0].read_scope,
            crate::GrammarProperty::Known(vec![])
        );
        assert!(
            scope_gaps
                .iter()
                .any(|gap| gap.subject == Some(crate::GapSubject::field("entry")))
        );
        let target = &children[1];
        let FieldReference::Lookups(lookups) = &target.reference else {
            panic!("{:?}", target.reference);
        };
        let expected = FieldCondition::FieldZero {
            path: vec!["entry".into(), "flag".into()],
            zero: true,
        };
        assert_eq!(target.read[0].condition, expected);
        assert_eq!(lookups[0].condition, expected);
    }
}
