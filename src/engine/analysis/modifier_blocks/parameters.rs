//! The parameter list of a custom-description serializer, such as `description_parameters`: a
//! block of `<any key> = <scalar>` entries. The serializer's member reader requires `{`, reads
//! entries until `}` or the end of input, appends one array element for each and reads it with a
//! stack entry serializer. The entry reader keeps the key's text and accepts a value by its token
//! kind alone. Prefix shapes of the two bodies establish this; the walker cannot follow the loop's
//! indirect lexer calls.
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use super::super::{
    decode::Instruction,
    fields::{PathOutcome, ReaderJoin, Token, TokenPath},
    references::shapes::{Bindings, Shape, canonical},
    stop::Unresolved,
};
use crate::{ReaderKind, RepeatBehavior, ScalarForm};

/// The member shape's line that loads the value token's kind on the list route.
const LIST_ROUTE_LINE: usize = 25;
/// The member shape's `adrp` line; with the next `add` it forms the entry serializer's vtable
/// address point.
const ENTRY_VTABLE_LINE: usize = 30;
/// The entry shape's lines that branch to the keyword tests and to the malformed report, and
/// the line that copies an accepted value.
const KEYWORD_BRANCH_LINE: usize = 19;
const MALFORMED_BRANCH_LINE: usize = 26;
const ACCEPT_LINE: usize = 27;
/// The malformed report's line in the keyword shape.
const MALFORMED_LINE: usize = 6;

/// The token kind of a word that starts with a digit or `-`, or has only digits, `-` and `.`.
const NUMBER_KIND: i64 = 0xc;
/// The token kind of a quoted string, or of a word that names no keyword.
const TEXT_KIND: i64 = 0xf;
/// Kinds that only a token built from a value has (`CToken(float)`, `CToken(bool)` and
/// `CToken(unsigned int)`). The text lexer never makes them, so they have no written form.
const BUILT_KINDS: [i64; 3] = [0xd, 0xe, 0x14];

/// The list route of one serializer member, with what its entry reader accepts. `body` is the
/// member's address range: a path that stopped inside it took one of its routes.
#[derive(Debug, Clone)]
pub(crate) struct ParameterList {
    pub body: Range<u64>,
    pub list: Result<List, Unresolved>,
}

#[derive(Debug, Clone)]
pub(crate) struct List {
    /// The key token that selects the list route.
    pub token: i64,
    /// The address of the list route's first instruction.
    pub branch: u64,
    /// The serializer member's symbol.
    pub reader: String,
    pub accepted: AcceptedKinds,
}

/// The value token kinds that an entry keeps: kinds up to the mask bound, and keywords by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AcceptedKinds {
    pub scalar: BTreeSet<i64>,
    pub keywords: BTreeSet<String>,
}

/// The list route of a serializer member: its key token, its first instruction, and the entry
/// serializer's vtable address point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ListRoute {
    pub token: i64,
    pub branch: u64,
    pub entry_point: u64,
}

/// Match the serializer member against the list shape. `wrapped` is the serializer's slot of
/// its wrapped owner, and `value_token` the reader's value-token offset.
pub(crate) fn list_route(
    member: &[Instruction],
    names: &BTreeMap<u64, String>,
    wrapped: i64,
    value_token: u64,
) -> Result<ListRoute, Unresolved> {
    let failed = || Unresolved::new("parameter-list-shape");
    let lines = canonical(member, names);
    let binding = Shape::parse(include_str!("shapes/parameter_list.txt"))
        .matches_prefix(&lines)
        .ok_or_else(failed)?;
    if number(&binding, "wrapped") != Some(wrapped)
        || number(&binding, "value_token") != Some(value_token as i64)
    {
        return Err(failed());
    }

    let token = number(&binding, "list_token").ok_or_else(failed)?;
    let entry_point = super::super::numeric::lexer::page_address(
        &member[ENTRY_VTABLE_LINE],
        &member[ENTRY_VTABLE_LINE + 1],
    )
    .ok_or_else(failed)?;
    Ok(ListRoute {
        token,
        branch: member[LIST_ROUTE_LINE].address,
        entry_point,
    })
}

/// Match the entry reader against the entry and keyword shapes, and read the kinds that it
/// accepts. Each keyword kind is a token number that `tokens` names.
pub(crate) fn accepted_kinds(
    entry: &[Instruction],
    names: &BTreeMap<u64, String>,
    tokens: &BTreeMap<i64, Token>,
    value_token: u64,
) -> Result<AcceptedKinds, Unresolved> {
    let failed = || Unresolved::new("parameter-entry-shape");
    let lines = canonical(entry, names);
    let test = Shape::parse(include_str!("shapes/parameter_entry.txt"))
        .matches_prefix(&lines)
        .ok_or_else(failed)?;
    if number(&test, "value_token") != Some(value_token as i64) {
        return Err(failed());
    }

    let keywords_line = KEYWORD_BRANCH_LINE + count(&test, "keywords").ok_or_else(failed)?;
    let keyword_tests = Shape::parse(include_str!("shapes/parameter_keywords.txt"))
        .matches_prefix(lines.get(keywords_line..).unwrap_or_default())
        .ok_or_else(failed)?;
    let backs = [("first_back", 1), ("second_back", 3), ("third_back", 5)];
    let routes_hold = MALFORMED_BRANCH_LINE + count(&test, "malformed").ok_or_else(failed)?
        == keywords_line + MALFORMED_LINE
        && backs.iter().all(|(name, line)| {
            count(&keyword_tests, name)
                .is_some_and(|back| keywords_line + line == ACCEPT_LINE + back)
        });
    if !routes_hold {
        return Err(failed());
    }

    kinds(&test, &keyword_tests, tokens)
}

/// The accepted kinds that the matched immediates give. A kind that the method cannot give a
/// written form, or a keyword test that no keyword token names, fails closed.
fn kinds(
    test: &Bindings,
    keyword_tests: &Bindings,
    tokens: &BTreeMap<i64, Token>,
) -> Result<AcceptedKinds, Unresolved> {
    let failed = || Unresolved::new("parameter-entry-kind");
    let floor = number(test, "floor").ok_or_else(failed)?;
    let bound = number(test, "bound").ok_or_else(failed)?;
    let mask = number(test, "low_mask").ok_or_else(failed)?
        & number(test, "high_mask").ok_or_else(failed)?;
    if !(0..32).contains(&bound) || bound >= floor {
        return Err(failed());
    }

    let scalar: BTreeSet<i64> = (0..=bound).filter(|kind| mask & (1 << kind) != 0).collect();
    let known = |kind: &i64| [NUMBER_KIND, TEXT_KIND].contains(kind) || BUILT_KINDS.contains(kind);
    if !scalar.iter().all(known) {
        return Err(failed());
    }

    let keywords = ["first", "second", "third"]
        .into_iter()
        .map(|name| {
            let kind = number(keyword_tests, name).filter(|kind| *kind > floor)?;
            tokens.get(&kind)?.name.clone()
        })
        .collect::<Option<BTreeSet<_>>>()
        .ok_or_else(failed)?;
    Ok(AcceptedKinds { scalar, keywords })
}

/// The written forms of the accepted kinds. Only the text lexer's kinds have one.
pub(crate) fn value_forms(accepted: &AcceptedKinds) -> Vec<ScalarForm> {
    let scalar = [
        (NUMBER_KIND, ScalarForm::Number),
        (TEXT_KIND, ScalarForm::Text),
    ]
    .into_iter()
    .filter(|(kind, _)| accepted.scalar.contains(kind))
    .map(|(_, form)| form);
    let keywords = accepted.keywords.iter().cloned().map(ScalarForm::Keyword);

    scalar.chain(keywords).collect()
}

/// Read each stopped leaf that took a list route as that list, and give each leaf that stopped
/// in a member whose list is not established that member's stop. A leaf keeps its conditions;
/// leaves that become equal are kept once. Returns the joined list of each key token.
pub(crate) fn join<'a>(
    leaves: &mut Vec<TokenPath>,
    lists: &'a [ParameterList],
) -> BTreeMap<i64, &'a List> {
    let mut joined = BTreeMap::new();
    for leaf in leaves.iter_mut() {
        if matches!(
            leaf.outcome,
            PathOutcome::Rejected
                | PathOutcome::Reader(ReaderJoin::Joined { .. } | ReaderJoin::Stored { .. })
        ) {
            continue;
        }

        let Some(found) = lists.iter().find(|list| list.body.contains(&leaf.terminal)) else {
            continue;
        };
        match &found.list {
            Ok(list)
                if leaf.domain == [list.token, list.token]
                    && leaf.instructions.contains(&list.branch) =>
            {
                leaf.outcome = PathOutcome::Reader(ReaderJoin::Stored {
                    callee: list.reader.clone(),
                    kind: ReaderKind::Block,
                    destination: None,
                    repeat: RepeatBehavior::Accumulate,
                });
                joined.insert(list.token, list);
            }
            Ok(_) => {}
            Err(stop) => leaf.outcome = PathOutcome::Reader(ReaderJoin::Missing(stop.clone())),
        }
    }

    let mut kept: Vec<TokenPath> = Vec::with_capacity(leaves.len());
    for leaf in leaves.drain(..) {
        let repeated = kept.iter().any(|earlier| {
            earlier.domain == leaf.domain
                && earlier.conditions == leaf.conditions
                && earlier.outcome == leaf.outcome
        });
        if !repeated {
            kept.push(leaf);
        }
    }
    *leaves = kept;

    joined
}

/// A captured hexadecimal immediate, such as `0x39a4`.
fn number(binding: &Bindings, name: &str) -> Option<i64> {
    i64::from_str_radix(binding.get(name)?.strip_prefix("0x")?, 16).ok()
}

/// A captured decimal line count, such as the `130` of `@+130`.
fn count(binding: &Bindings, name: &str) -> Option<usize> {
    binding.get(name)?.parse().ok()
}

#[cfg(test)]
#[path = "parameters_tests.rs"]
mod tests;
