//! Root-token dispatch derivation from executable bytes, without content or config inputs.
//!
//! The reducer reads raw instructions, the executable symbol inventory, and engine string
//! literals. It finds reachable token-constructor calls, joining only equal constants across
//! control-flow merges. It then recovers their literal arguments and partitions the signed 32-bit
//! root-token branches. Known zero tests take only their feasible branch; unknown state tests
//! keep both alternatives.
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
//! - Root traversal: at most 500 instructions per path and 4,096 states. Token-construction
//!   reachability: at most 40 visits per decoded instruction in aggregate.
//! - Unknown instructions, unsupported addressing, missing symbols or names, conflicting token
//!   names, cycles and clobbered values become explicit gaps.
//! - Inputs: at most 128 functions and 4 MiB of aggregate code, with a 1 MiB per-function
//!   decode bound.
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

/// Name and revision of the method, as stamped on its answers.
pub const METHOD: &str = "registry-fields/v1";

/// Find the root fields of the selected candidate. Completeness is derived, never supplied.
pub fn analyze(input: &FieldInput) -> Result<RegistryFieldResult, InputError> {
    if input.functions.len() > 128
        || input.functions.iter().map(|f| f.code.len()).sum::<usize>() > 4 * 1024 * 1024
    {
        return Err(InputError("function input budget exceeded".into()));
    }
    if !crate::engine::analysis::discovery::candidates(&input.symbols).contains(&input.selection) {
        return Err(InputError(
            "selected loader is not an executable-derived candidate".into(),
        ));
    }
    let gap = |kind: &str, reason: String| FieldGap {
        kind: kind.into(),
        reason,
        path: None,
    };
    let mut gaps: Vec<_> = input
        .gaps
        .iter()
        .map(|reason| gap("input-boundary", reason.clone()))
        .collect();
    let (tokens, token_gaps) = tokens::recover(input);
    gaps.extend(
        token_gaps
            .into_iter()
            .map(|reason| gap("token-table", reason)),
    );
    let paths = dispatch::explore(input);
    let (fields, path_gaps) = inventory::fields_and_gaps(&paths, &tokens);
    gaps.extend(path_gaps);
    let partition_accounted = inventory::partition_accounted(&paths);
    if !partition_accounted {
        gaps.push(gap(
            "token-partition",
            "token intervals are missing or overlap".into(),
        ));
    }
    gaps.push(gap(
        "reader-contract",
        "Routing does not establish shared-reader semantics or complete registry membership."
            .into(),
    ));
    Ok(RegistryFieldResult {
        fields,
        paths,
        gaps,
        partition_accounted,
        complete_registry: false,
        blocking_readers: inventory::blocking_readers(&input.selection.owner_candidate),
    })
}
