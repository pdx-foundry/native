//! Attach triggered modifier clause grammar to fields whose collected object has a clause reader.
use crate::engine::analysis::{
    fields::{ConcreteReader, ReaderJoin, RegistryFieldResult, RootField},
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
    Reader, ReaderKind, TriggeredModifierBlock,
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
        let point = clause_point(result, &field.name);

        match point.and_then(|point| facts.clauses.points.get(&point)) {
            Some(Ok(clause)) => {
                let block = normalize(clause, facts, &field.name, gaps);
                field.members = FieldMembers::TriggeredModifier(Box::new(block));
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

/// The address point of the clause object that the field's collection inserts.
fn clause_point(result: &RegistryFieldResult, name: &str) -> Option<u64> {
    let root = result.fields.iter().find(|root| root.name == name)?;
    let collection = result
        .collections
        .iter()
        .find(|collection| collection.token == root.token)?;

    collection.reader.as_ref().map(|bound| bound.point)
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

        (callee == "CReader::Read(CPersistent&)"
            && reader.as_ref()?.family == BlockFamily::Modifier)
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

    if !gaps.contains(&gap) {
        gaps.push(gap);
    }
}
