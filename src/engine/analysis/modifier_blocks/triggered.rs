//! Triggered modifier clauses at their family's vtable address points. The member reader names
//! the clause's own keys; every other key goes through the family's bound virtual delegate. That
//! route is accepted only when it tail-calls one member reader of one embedded object, and the
//! object's address point comes from every constructor of the clause's class.
use std::collections::{BTreeMap, BTreeSet};

use super::{MemberLeaves, fixed_fields, member_leaves};
use crate::engine::analysis::{
    declarations::Function,
    discovery::Symbol,
    fields::{
        self, ConcreteReader, DataSection, DispatchInput, KeyReaders, PathOutcome, PersistentInput,
        ReaderJoin, RootField, Token, TokenPath, Value,
    },
    readers,
    stop::Unresolved,
};

pub(crate) struct TriggeredInput {
    /// Clause readers by vtable address point; each carries its bound delegate.
    pub points: BTreeMap<u64, ConcreteReader>,
    /// Constructor evidence of the class of each address point.
    pub classes: BTreeMap<u64, Result<PersistentInput, Unresolved>>,
    /// Member and delegate bodies.
    pub functions: BTreeMap<u64, Function>,
    pub symbols: Vec<Symbol>,
    pub pointers: BTreeMap<u64, u64>,
    pub data: Vec<DataSection>,
    pub tokens: BTreeMap<i64, Token>,
    pub key_readers: KeyReaders,
    pub reader_token_offset: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct Clause {
    pub fields: Vec<RootField>,
    pub paths: Vec<TokenPath>,
    pub fixed_complete: bool,
    /// The address point and, when bound, reader that every constructor installs at each key
    /// destination offset.
    pub embedded: BTreeMap<i64, (u64, Option<ConcreteReader>)>,
    /// The destination offset of the embedded object whose member reads every key that the
    /// clause does not name.
    pub other_keys: Result<i64, Unresolved>,
    pub stops: Vec<(Option<String>, Unresolved)>,
}

#[derive(Default)]
pub(crate) struct TriggeredFacts {
    pub points: BTreeMap<u64, Result<Clause, Unresolved>>,
}

pub(crate) fn analyze(input: &TriggeredInput) -> TriggeredFacts {
    let dispatch = DispatchInput::command(
        &input.functions,
        &input.symbols,
        &input.data,
        input.reader_token_offset,
        &input.key_readers,
    );

    TriggeredFacts {
        points: input
            .points
            .iter()
            .map(|(&point, reader)| {
                let mut dispatch = dispatch.with_owner_vtable(Some((point, &input.pointers)));
                dispatch.virtual_delegate = reader.delegate.as_deref();

                (point, clause(input, &dispatch, point, reader))
            })
            .collect(),
    }
}

fn clause(
    input: &TriggeredInput,
    dispatch: &DispatchInput<'_>,
    point: u64,
    reader: &ConcreteReader,
) -> Result<Clause, Unresolved> {
    let MemberLeaves {
        paths: leaves,
        mut stops,
    } = member_leaves(dispatch, &reader.member)?;
    let paths: Vec<_> = leaves
        .iter()
        .filter(|path| path.domain[0] == path.domain[1])
        .cloned()
        .collect();
    let (fields, key_stops) = fixed_fields(&paths, &input.tokens);
    let fixed_complete = stops.is_empty() && key_stops.is_empty();
    stops.extend(key_stops);

    let other_keys = match reader.delegate.as_deref() {
        Some(delegate) => other_keys(dispatch, &leaves, delegate, &reader.member),
        None => Err(Unresolved::new("triggered-delegate-slot")),
    };
    let offsets: BTreeSet<_> = fields
        .iter()
        .flat_map(|field| &field.readers)
        .filter(|join| reads_embedded_object(join))
        .filter_map(readers::destination)
        .chain(other_keys.iter().map(|(offset, _)| *offset))
        .collect();
    let embedded = match input.classes.get(&point) {
        Some(Ok(binding)) => {
            let found = fields::constructor_points(binding, &input.data, &offsets);
            stops.extend(found.gaps.into_iter().map(|gap| {
                let mut stop = Unresolved::new("triggered-embedded-point");
                stop.stop = gap.stop;

                (None, stop)
            }));

            found
                .points
                .into_iter()
                .map(|(offset, point)| (offset, (point, binding.readers.get(&point).cloned())))
                .collect()
        }
        Some(Err(stop)) => {
            stops.push((None, stop.clone()));
            BTreeMap::new()
        }
        None => BTreeMap::new(),
    };
    let other_keys = other_keys.and_then(|(offset, member)| {
        let embedded_member = embedded
            .get(&offset)
            .and_then(|(_, reader)| reader.as_ref())
            .map(|reader| reader.member.as_str());

        (embedded_member == Some(member.as_str()))
            .then_some(offset)
            .ok_or_else(|| Unresolved::new("triggered-embedded-member"))
    });

    Ok(Clause {
        fields,
        paths,
        fixed_complete,
        embedded,
        other_keys,
        stops,
    })
}

/// Whether a key's reader reads an object that the clause's constructor embeds.
fn reads_embedded_object(join: &ReaderJoin) -> bool {
    matches!(join, ReaderJoin::Joined { callee, .. } if matches!(
        callee.as_str(),
        "CReader::Read(CPersistent&)"
            | "CVariableValue::Read(CReader&, EScopeType)"
            | "CVariableValue::Assign(CToken const&, EScopeType, CString const&)"
    ))
}

/// The embedded object and member that read every key the clause does not name. Each default
/// path must tail-call the bound delegate with no condition of its own, and the delegate must
/// tail-call one member reader of one embedded object, again unconditionally.
fn other_keys(
    dispatch: &DispatchInput<'_>,
    leaves: &[TokenPath],
    delegate: &str,
    root: &str,
) -> Result<(i64, String), Unresolved> {
    let default_route = || Unresolved::new("triggered-default-route");
    let mut route = None;

    for path in leaves
        .iter()
        .filter(|path| path.domain[0] != path.domain[1])
    {
        let PathOutcome::Reader(ReaderJoin::Joined {
            callee, tail: true, ..
        }) = &path.outcome
        else {
            return Err(default_route());
        };

        if callee != delegate || !path.conditions.is_empty() {
            return Err(default_route());
        }

        let (children, gaps) = fields::follow_member(dispatch, path, &[root.to_owned()])?;

        if !gaps.is_empty() || children.is_empty() {
            return Err(Unresolved::new("triggered-delegate"));
        }

        for child in children {
            let found =
                embedded_member(&child).ok_or_else(|| Unresolved::new("triggered-delegate"))?;

            if route.as_ref().is_some_and(|known| *known != found) {
                return Err(Unresolved::new("triggered-delegate"));
            }

            route = Some(found);
        }
    }

    route.ok_or_else(default_route)
}

/// The owner offset and member reader of an unconditional tail call to an embedded object's
/// member reader.
fn embedded_member(path: &TokenPath) -> Option<(i64, String)> {
    let PathOutcome::Reader(ReaderJoin::Joined {
        callee,
        arguments,
        tail: true,
    }) = &path.outcome
    else {
        return None;
    };
    let Some(Value::Owner(offset)) = arguments.get("x0") else {
        return None;
    };

    (readers::is_member(callee) && path.conditions.is_empty() && *offset > 0)
        .then(|| (*offset, callee.clone()))
}

#[cfg(test)]
mod tests;
