//! The script values of a modifier category, such as `category = planet`, and the category mask
//! that the engine reads each as.
//!
//! The engine's token-to-category switch maps a value token to a mask. The method runs it once for
//! each named token; a nonzero mask is a key. A zero mask is an invalid category, except for the
//! one token that every reader of the switch accepts without its error message. Each call site
//! must have the readers' exact shape: the reader stores the result, goes to its success block on a
//! nonzero mask, compares the same value token with a constant and goes to the same block on
//! equality, and otherwise calls the log constructor with no branch before it. The constant is
//! the empty key. Any other shape leaves the empty key unresolved.
//!
//! Outside the method: a category that a later step adds to a generated modifier, such as the AI
//! Economy category that economic-category generation adds.
use std::collections::{BTreeMap, BTreeSet};

use super::declarations::number;
use super::decode::{Instruction, is_control_transfer};
use super::evaluate::{Code, Exit, Machine, ReadOnlyData};
use super::modifiers::{CategoryInput, CategoryNames, EVERY_CATEGORY, category_names};
use super::stop::Unresolved;

#[cfg(test)]
mod tests;

/// Name and revision of the category key method.
pub const METHOD: &str = "modifier-category-keys/v1";

/// Executable-derived input of the category key method.
pub struct CategoryKeyInput {
    /// Literal token names, by token.
    pub tokens: BTreeMap<u64, String>,
    /// Entry of the switch from a value token to a category mask.
    pub switch: u64,
    pub code: Code,
    /// For each direct call of the switch, the rows from the instruction before the call through
    /// a bounded window after it.
    pub sites: Vec<Vec<Instruction>>,
    /// Entries of the log constructor that starts the invalid-category message.
    pub log: BTreeSet<u64>,
    pub categories: CategoryInput,
}

/// Each key, and the name of each single category.
pub struct CategoryKeyResult {
    /// The mask of each named value token that the switch maps to a mask, and 0 for the empty key.
    pub keys: BTreeMap<String, u64>,
    /// Why no token is established as the empty key.
    pub empty: Option<Unresolved>,
    /// Named tokens for which the switch did not return a mask.
    pub unreadable: usize,
    pub categories: CategoryNames,
}

/// Read every key's mask and the empty key.
pub fn analyze(input: &CategoryKeyInput) -> CategoryKeyResult {
    let data = ReadOnlyData::default();
    let mut keys = BTreeMap::new();
    let mut zero = BTreeSet::new();
    let mut unreadable = 0;

    for (&token, name) in &input.tokens {
        match token_mask(input, &data, token) {
            Ok(0) => {
                zero.insert(token);
            }
            Ok(mask) => {
                keys.insert(name.clone(), mask);
            }
            Err(_) => unreadable += 1,
        }
    }

    let empty = match empty_token(input) {
        Ok(token) if zero.contains(&token) => {
            keys.insert(input.tokens[&token].clone(), 0);
            None
        }
        Ok(_) => Some(Unresolved::new("empty-key-mask")),
        Err(unresolved) => Some(unresolved),
    };
    let categories = category_names(&input.categories, []);

    CategoryKeyResult {
        keys,
        empty,
        unreadable,
        categories,
    }
}

/// The mask that the switch returns for `token`.
fn token_mask(
    input: &CategoryKeyInput,
    data: &ReadOnlyData,
    token: u64,
) -> Result<u64, Unresolved> {
    let mut machine = Machine::new(&input.code, data);
    machine.set_register(0, token);

    let exit = machine.run(input.switch, &mut |_, _| {
        Err(Unresolved::new("switch-call"))
    })?;
    if exit != Exit::Returned {
        return Err(Unresolved::new("switch-exit"));
    }

    Ok(machine.known_register(0, "switch-result")? & EVERY_CATEGORY)
}

/// The one token that every call site accepts with an empty mask.
fn empty_token(input: &CategoryKeyInput) -> Result<u64, Unresolved> {
    let mut tokens = input
        .sites
        .iter()
        .map(|rows| site_empty_token(rows, input.switch, &input.log));
    let first = tokens.next().ok_or(Unresolved::new("switch-call-site"))??;

    for token in tokens {
        if token? != first {
            return Err(Unresolved::new("empty-key-sites"));
        }
    }

    Ok(first)
}

/// The token that one call site accepts with an empty mask, read from the readers' exact shape:
///
/// ```text
/// ldr  w0, [xB, #off]
/// bl   switch
/// str  w0, [xO, #a]
/// cbnz w0, D
/// ldr  wT, [xB, #off]
/// cmp  wT, #token
/// b.eq D
/// …    no branch, then `bl log`, with D after it
/// ```
fn site_empty_token(
    rows: &[Instruction],
    switch: u64,
    log: &BTreeSet<u64>,
) -> Result<u64, Unresolved> {
    let shape = || Unresolved::new("empty-key-shape");
    let call = rows
        .iter()
        .position(|row| row.operation == "bl" && number(&row.operands) == Some(switch))
        .ok_or_else(shape)?;
    let Some(
        [
            argument,
            _,
            store,
            nonzero,
            reload,
            compare,
            equal,
            rest @ ..,
        ],
    ) = call.checked_sub(1).and_then(|start| rows.get(start..))
    else {
        return Err(shape());
    };

    let token_source = argument
        .operands
        .strip_prefix("w0,")
        .filter(|_| argument.operation == "ldr")
        .ok_or_else(shape)?;
    let success = nonzero
        .operands
        .strip_prefix("w0,")
        .filter(|_| nonzero.operation == "cbnz")
        .and_then(number)
        .ok_or_else(shape)?;
    let (token_register, reloaded) = reload
        .operands
        .split_once(',')
        .filter(|_| reload.operation == "ldr")
        .ok_or_else(shape)?;
    let token = compare
        .operands
        .strip_prefix(token_register)
        .and_then(|operand| operand.strip_prefix(','))
        .filter(|_| compare.operation == "cmp")
        .and_then(number)
        .ok_or_else(shape)?;
    let stores_result = store.operation == "str" && store.operands.starts_with("w0,");
    let rejoins = equal.operation == "b.eq" && number(&equal.operands) == Some(success);

    if !stores_result || reloaded != token_source || !rejoins {
        return Err(shape());
    }

    let logged = rest
        .iter()
        .find(|row| is_control_transfer(&row.operation))
        .filter(|row| {
            row.operation == "bl" && number(&row.operands).is_some_and(|to| log.contains(&to))
        })
        .ok_or_else(shape)?;
    if success <= logged.address {
        return Err(shape());
    }

    Ok(token as u32 as u64)
}
