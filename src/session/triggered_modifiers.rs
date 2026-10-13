//! Attach triggered modifier clause grammar to fields whose collected object has a clause reader.
use super::push_once;
use crate::engine::analysis::{
    fields::{
        CollectionField, ConcreteReader, PathOutcome, ReaderJoin, RegistryFieldResult, RootField,
        Value,
    },
    modifier_blocks::{
        ModifierBlockFacts,
        triggered::{Clause, TriggeredFacts},
    },
    numeric::NumericFacts,
    readers,
    references::ReferenceFacts,
    scoped_numeric::Facts as ScopedFacts,
};
use crate::{
    BlockFamily, Field, FieldMembers, Gap, GapKind, GapSubject, GrammarProperty, ModifierMembers,
    ReadScope, Reader, ReaderKind, TriggeredModifierBlock,
};
use std::collections::BTreeMap;

/// The executable-wide facts that a clause's keys join.
pub(super) struct ClauseFacts<'a> {
    pub clauses: &'a TriggeredFacts,
    pub modifiers: &'a ModifierBlockFacts,
    pub references: &'a ReferenceFacts,
    pub numeric: &'a NumericFacts,
    pub scoped: &'a ScopedFacts,
    pub scope_names: Option<&'a [String]>,
}

pub(super) fn attach(
    values: &mut [Field],
    result: &RegistryFieldResult,
    facts: &ClauseFacts<'_>,
    gaps: &mut Vec<Gap>,
) {
    for field in values
        .iter_mut()
        .filter(|field| field.reader.family == BlockFamily::TriggeredModifier)
    {
        let collection = clause_collection(result, &field.name);
        let point = collection
            .and_then(|collection| collection.reader.as_ref())
            .map(|bound| bound.point);

        match point.and_then(|point| facts.clauses.points.get(&point)) {
            Some(Ok(clause)) => {
                let scopes = collection.map_or(&[][..], |collection| {
                    collection.construction_scopes.as_slice()
                });
                let clause = in_construction_scope(clause);
                let block = normalize(&clause, facts, &field.name, gaps);

                field.members = FieldMembers::TriggeredModifier(Box::new(block));
                field.read_scope = read_scope(scopes, facts.scope_names, &field.name, gaps);
            }
            failure => push(
                gaps,
                GapKind::UnresolvedReader,
                GapSubject::field(&field.name),
                format!(
                    "The triggered modifier analysis stopped at {}.",
                    match failure {
                        Some(Err(stop)) => stop.reason,
                        _ => "triggered-modifier-point",
                    }
                ),
            ),
        }
    }
}

/// The collection that inserts the field's clause objects.
fn clause_collection<'a>(
    result: &'a RegistryFieldResult,
    name: &str,
) -> Option<&'a CollectionField> {
    let root = result.fields.iter().find(|root| root.name == name)?;

    result
        .collections
        .iter()
        .find(|collection| collection.token == root.token)
}

/// The scopes that a clause reads its keys in: the scopes its constructor receives, which the
/// constructor stores in the clause's scope word (M452: `CTriggeredModifierBase` stores its
/// `EScopeType` argument at `+0x3e8`, or `+0x428` for the custom-description variant).
fn read_scope(
    scopes: &[Option<i64>],
    names: Option<&[String]>,
    field: &str,
    gaps: &mut Vec<Gap>,
) -> GrammarProperty<Vec<ReadScope>> {
    let subject = GapSubject::field(field);
    if scopes.is_empty() {
        push(
            gaps,
            GapKind::UnresolvedPath,
            subject,
            "read-scope: construction-scope".into(),
        );
        return GrammarProperty::Unresolved;
    }
    let arguments = scopes
        .iter()
        .map(|scope| scope.map(Value::Constant))
        .collect();

    super::read_scope::normalize_scopes(arguments, names, subject, gaps)
}

/// The clause with the scope that its keys load from the clause object replaced by the clause's
/// own read scope, when every such key loads the same word. That word is the scope that the
/// constructor stores, so equal clause readers keep equal key grammar under different owners.
fn in_construction_scope(clause: &Clause) -> Clause {
    let mut clause = clause.clone();
    let joins = clause
        .fields
        .iter_mut()
        .flat_map(|root| root.readers.iter_mut())
        .chain(
            clause
                .paths
                .iter_mut()
                .filter_map(|path| match &mut path.outcome {
                    PathOutcome::Reader(join) => Some(join),
                    _ => None,
                }),
        );
    let stored: Vec<_> = joins.filter_map(stored_scope).collect();
    let mut words = stored.iter().map(|argument| (**argument).clone());
    let Some(word) = words.next() else {
        return clause;
    };
    if words.any(|other| other != word) {
        return clause;
    }
    for argument in stored {
        *argument = Value::EnclosingScope;
    }

    clause
}

/// The scope argument of a join that loads its scope from the clause object.
fn stored_scope(join: &mut ReaderJoin) -> Option<&mut Value> {
    let ReaderJoin::Joined {
        callee, arguments, ..
    } = join
    else {
        return None;
    };
    let register = readers::scope_register(callee)?;
    let argument = arguments.get_mut(register)?;

    matches!(argument, Value::Load(base, _) if matches!(**base, Value::Owner(_)))
        .then_some(argument)
}

fn normalize(
    clause: &Clause,
    facts: &ClauseFacts<'_>,
    parent: &str,
    gaps: &mut Vec<Gap>,
) -> TriggeredModifierBlock {
    let path = vec![parent.to_owned()];
    let embedded_readers: BTreeMap<i64, ConcreteReader> = clause
        .embedded
        .iter()
        .filter_map(|(&offset, (_, reader))| Some((offset, reader.clone()?)))
        .collect();
    let embedded_points: BTreeMap<i64, u64> = clause
        .embedded
        .iter()
        .map(|(&offset, &(point, _))| (offset, point))
        .collect();
    let mut keys = super::fields::embedded_fields(
        &clause.fields,
        &clause.paths,
        facts.references,
        &embedded_readers,
    );

    for (key, root) in keys.iter_mut().zip(&clause.fields) {
        let Some(point) = embedded_modifier(root, clause) else {
            continue;
        };
        let key_path = [parent.to_owned(), key.name.clone()];

        key.members = match super::modifier_blocks::block(
            facts.modifiers.points.get(&point),
            facts.references,
            facts.scope_names,
            &key_path,
            gaps,
        ) {
            Some(block) => FieldMembers::ModifierBlock(block),
            None => FieldMembers::Unresolved,
        };
    }

    super::numeric::fields(&mut keys, facts.numeric, &path, gaps);

    for (key, root) in keys.iter_mut().zip(&clause.fields) {
        let subject = GapSubject::key_path(vec![parent.to_owned(), key.name.clone()]);
        let evidence = super::scoped_numeric::FieldEvidence {
            paths: &clause.paths,
            points: &embedded_points,
            facts: facts.scoped,
            numeric: facts.numeric,
        };

        super::scoped_numeric::attach_field(key, root, evidence, &subject, gaps);
    }

    super::read_scope::block_keys(
        &mut keys,
        &clause.fields,
        &clause.paths,
        facts.scope_names,
        &path,
        gaps,
    );

    for key in &mut keys {
        super::fields::prefix_conditions(key, parent);
    }

    for (key, stop) in &clause.stops {
        let (kind, subject) = match key {
            Some(key) => (
                GapKind::UnresolvedReader,
                GapSubject::key_path(vec![parent.to_owned(), key.clone()]),
            ),
            None => (GapKind::UnresolvedPath, GapSubject::field(parent)),
        };

        push(
            gaps,
            kind,
            subject,
            format!(
                "The triggered modifier analysis stopped at {}.",
                stop.reason
            ),
        );
    }

    TriggeredModifierBlock {
        fixed_keys: if clause.fixed_complete {
            GrammarProperty::Known(keys)
        } else {
            GrammarProperty::Partial(keys)
        },
        other_keys: other_keys(clause, facts, parent, gaps),
    }
}

/// The address point of the modifier block that a key reads into the clause object.
fn embedded_modifier(root: &RootField, clause: &Clause) -> Option<u64> {
    let mut points = root.readers.iter().filter_map(|join| {
        let ReaderJoin::Joined { callee, .. } = join else {
            return None;
        };
        let offset = readers::destination(join)?;
        let (point, reader) = clause.embedded.get(&offset)?;

        (callee == readers::PERSISTENT_READ && reader.as_ref()?.family == BlockFamily::Modifier)
            .then_some(*point)
    });
    let point = points.next()?;

    points.all(|other| other == point).then_some(point)
}

fn other_keys(
    clause: &Clause,
    facts: &ClauseFacts<'_>,
    parent: &str,
    gaps: &mut Vec<Gap>,
) -> GrammarProperty<ModifierMembers> {
    let embedded = clause
        .other_keys
        .as_ref()
        .map_err(|stop| stop.reason)
        .and_then(|offset| match clause.embedded.get(offset) {
            Some((point, Some(reader))) if reader.family == BlockFamily::Modifier => {
                Ok((*point, reader))
            }
            _ => Err("triggered-embedded-modifier"),
        });
    let (point, reader) = match embedded {
        Ok(embedded) => embedded,
        Err(reason) => {
            push(
                gaps,
                GapKind::UnresolvedReader,
                GapSubject::field(parent),
                format!(
                    "The reader of keys that the triggered modifier does not name is not \
                     established: {reason}."
                ),
            );
            return GrammarProperty::Unresolved;
        }
    };
    // The block's own paths would name keys of the clause, such as the clause's `key`, so its
    // gaps are stated on the field.
    let path = [parent.to_owned()];
    let mut block_gaps = Vec::new();
    let mut block = super::modifier_blocks::block(
        facts.modifiers.points.get(&point),
        facts.references,
        facts.scope_names,
        &path,
        &mut block_gaps,
    );

    if let Some(block) = &mut block {
        super::numeric::modifier_block(block, facts.numeric, &path, &mut block_gaps);
    }

    for gap in block_gaps {
        let key = match &gap.subject {
            Some(GapSubject::KeyPath { path }) => path.get(1..).map(|key| key.join(".")),
            _ => None,
        };
        let detail = match key {
            Some(key) => format!(
                "Keys that the triggered modifier does not name, at `{key}`: {}",
                gap.detail
            ),
            None => format!(
                "Keys that the triggered modifier does not name: {}",
                gap.detail
            ),
        };

        push(gaps, gap.kind, GapSubject::field(parent), detail);
    }

    let Some(block) = block else {
        return GrammarProperty::Unresolved;
    };

    GrammarProperty::Known(ModifierMembers {
        reader: Reader {
            numeric: GrammarProperty::Known(None),
            scoped_operand: GrammarProperty::Known(None),
            id: Some(super::fields::persistent_reader_id(reader)),
            kind: ReaderKind::Block,
            family: BlockFamily::Modifier,
        },
        block,
    })
}

fn push(gaps: &mut Vec<Gap>, kind: GapKind, subject: GapSubject, detail: String) {
    let gap = Gap {
        kind,
        subject: Some(subject),
        detail,
    };

    push_once(gaps, gap);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::stop::Unresolved;

    const POTENTIAL: &str = "CTrigger::Read(CReader&, EScopeType)";

    /// A clause whose `potential` and `limit` keys load their scope from `potential` and `limit`.
    fn clause(potential: Value, limit: Value) -> Clause {
        let key = |name: &str, token: i64, scope: Value| RootField {
            name: name.into(),
            token,
            constructor: 0,
            paths: vec![],
            readers: vec![ReaderJoin::Joined {
                callee: POTENTIAL.into(),
                arguments: [
                    ("x0".to_owned(), Value::Owner(0x3f0)),
                    ("x1".to_owned(), Value::Reader(0)),
                    ("x2".to_owned(), scope),
                ]
                .into(),
                tail: true,
            }],
        };

        Clause {
            fields: vec![key("potential", 1, potential), key("limit", 2, limit)],
            paths: vec![],
            fixed_complete: true,
            embedded: BTreeMap::new(),
            other_keys: Err(Unresolved::new("none")),
            stops: vec![],
        }
    }

    fn scopes(clause: &Clause) -> Vec<&Value> {
        clause
            .fields
            .iter()
            .flat_map(|root| &root.readers)
            .filter_map(|join| match join {
                ReaderJoin::Joined { arguments, .. } => arguments.get("x2"),
                _ => None,
            })
            .collect()
    }

    fn word(offset: i64) -> Value {
        Value::Load(Box::new(Value::Owner(offset)), 8)
    }

    #[test]
    fn keys_that_load_the_one_scope_word_read_in_the_clause_scope() {
        let one_word = in_construction_scope(&clause(word(0x3e8), word(0x3e8)));
        let constant = in_construction_scope(&clause(word(0x3e8), Value::Constant(4)));

        assert_eq!(
            scopes(&one_word),
            [&Value::EnclosingScope, &Value::EnclosingScope]
        );
        assert_eq!(
            scopes(&constant),
            [&Value::EnclosingScope, &Value::Constant(4)]
        );
    }

    #[test]
    fn keys_that_load_two_words_keep_their_stored_scopes() {
        let two_words = in_construction_scope(&clause(word(0x3e8), word(0x428)));

        assert_eq!(scopes(&two_words), [&word(0x3e8), &word(0x428)]);
    }

    #[test]
    fn a_clause_reads_in_the_scopes_that_its_constructor_receives() {
        let names = ["none", "planet", "country"].map(String::from);
        let mut gaps = Vec::new();

        let known = read_scope(&[Some(4)], Some(&names), "clause", &mut gaps);
        let GrammarProperty::Known(alternatives) = known else {
            panic!("{known:?}");
        };
        assert!(
            matches!(alternatives.as_slice(), [ReadScope::Types(types)] if types[0].name == "country")
        );
        assert!(gaps.is_empty(), "{gaps:?}");

        let partial = read_scope(&[Some(4), None], Some(&names), "clause", &mut gaps);
        assert!(matches!(partial, GrammarProperty::Partial(_)));
        assert_eq!(gaps[0].detail, "read-scope: scope-argument");

        gaps.clear();
        assert_eq!(
            read_scope(&[], Some(&names), "clause", &mut gaps),
            GrammarProperty::Unresolved
        );
        assert_eq!(gaps[0].detail, "read-scope: construction-scope");
    }
}
