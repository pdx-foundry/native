//! Root-token dispatch derivation from executable bytes, without content or config inputs.
//!
//! The reducer reads raw instructions, the executable symbol inventory, and engine string
//! literals. It finds reachable token-constructor calls, joining only equal constants across
//! control-flow merges. It then recovers their literal arguments and partitions the signed 32-bit
//! root-token branches. Known zero tests take only their feasible branch; unknown state tests
//! keep both alternatives. A compiler jump table is decoded from read-only data: a bounded
//! unsigned compare of `token + constant` guards it, and each token in the guarded range is
//! followed to its own case. The switch's default case, which a wide token interval also
//! reaches, may only reject: every token has a name, so a default slot never becomes a field.
//!
//! A reader join proves argument routing to a callee. It does not establish the reader's grammar,
//! accepted types, scope contract, or runtime behavior. A bare comparison pivot or an unsupported
//! path does not establish a field. A gap on another alternative of an established field stays
//! attached to that field.
//!
//! Limits of this revision:
//! - It stops at the first external delegate. Owner helpers, inherited readers beyond the
//!   verified base rejection, indirect calls, dynamic names and nested grammars are boundaries.
//! - It does not carry argument provenance through unknown calls, and does not assume that a
//!   stack restore recovers provenance.
//! - Root traversal: at most 500 instructions per path, 4,096 states and 1,024 tokens per jump
//!   table. Token-construction reachability: at most 40 visits per decoded instruction in
//!   aggregate.
//! - A jump table is followed only when its entries are offsets from a code address. A table of
//!   addresses, an unbounded index, an unreadable entry, a case outside the root or a default
//!   case that reaches a call is a `JumpTable` gap that names the table and the root. When a wide
//!   interval does not end at the rejection, a case without a known reader is treated as the
//!   default.
//! - Bit-field reads (`ubfx`, `and`) forget their result. A field read into a temporary reaches
//!   its reader call, but the reader join stays missing.
//! - Unknown instructions, unsupported addressing, missing symbols or names, conflicting token
//!   names, cycles and clobbered values become explicit gaps.
//! - Inputs: at most 128 functions and 4 MiB of aggregate code, with a 1 MiB per-function
//!   decode bound, and at most 64 MiB of read-only data.
//!
//! `partition_accounted` means that the ledger accounts for every signed token interval,
//! including gaps. It never means that every path was resolved. Council agenda keeps five
//! unresolved shared-reader contracts from the SDK-487 prototype: scoped integer values,
//! triggers, effects, graphical modifiers and AI weight.
//!
//! What the result does not establish: the template loader and owner symbol relationship is
//! static, not observed live ownership. Dispatch stops at a delegate or an obstruction, so nested
//! grammars, post-read behavior and dynamic names stay open. Names come only from literal engine
//! token constructors; no config or content file is an authority.
mod control_flow;
mod dispatch;
mod inventory;
mod records;
mod tokens;
pub use records::*;

use super::InputError;
use std::collections::BTreeMap;

/// Literal token identifiers shared with other static command readers.
pub(crate) fn literal_token_names(
    code: &[u8],
    address: u64,
    symbols: &[crate::engine::analysis::discovery::Symbol],
    strings: &BTreeMap<u64, String>,
) -> Result<BTreeMap<u64, String>, InputError> {
    let mut rows = Vec::new();
    for (index, chunk) in code.chunks(4096).enumerate() {
        rows.extend(
            super::decode::decode_arm64(chunk, address + (index * 4096) as u64)
                .map_err(|error| InputError(error.to_string()))?,
        );
    }
    let (tokens, _) = tokens::recover_decoded(&rows, symbols, strings);
    Ok(tokens
        .into_iter()
        .filter(|(_, token)| !token.ambiguous)
        .map(|(number, token)| (number as u32 as u64, token.name))
        .collect())
}

/// Name and revision of the method, as stamped on its answers.
pub const METHOD: &str = "registry-fields/v4";

/// Find the root fields of the selected candidate. Completeness is derived, never supplied.
pub fn analyze(input: &FieldInput) -> Result<RegistryFieldResult, InputError> {
    if input.functions.len() > 128
        || input.functions.iter().map(|f| f.code.len()).sum::<usize>() > 4 * 1024 * 1024
    {
        return Err(InputError("function input budget exceeded".into()));
    }
    if input
        .read_only_data
        .iter()
        .map(|section| section.bytes.len())
        .sum::<usize>()
        > 64 * 1024 * 1024
    {
        return Err(InputError("read-only data budget exceeded".into()));
    }
    if !crate::engine::analysis::discovery::candidates(&input.symbols).contains(&input.selection) {
        return Err(InputError(
            "selected loader is not an executable-derived candidate".into(),
        ));
    }
    let mut gaps: Vec<_> = input
        .gaps
        .iter()
        .map(|reason| FieldGap::new(FieldGapKind::InputBoundary, reason))
        .collect();
    let (tokens, token_gaps) = tokens::recover(input);
    gaps.extend(token_gaps);
    let (paths, table_gaps) = dispatch::explore(input);
    gaps.extend(table_gaps);
    let (fields, path_gaps) = inventory::fields_and_gaps(&paths, &tokens);
    gaps.extend(path_gaps);
    let partition_accounted = inventory::partition_accounted(&paths);
    if !partition_accounted {
        gaps.push(FieldGap::new(
            FieldGapKind::TokenPartition,
            "token intervals are missing or overlap",
        ));
    }
    Ok(RegistryFieldResult {
        fields,
        paths,
        gaps,
        partition_accounted,
    })
}
