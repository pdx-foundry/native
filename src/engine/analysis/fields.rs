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
//! - Inputs: descriptors to 1 MiB; recorded inputs to 64 MiB, 128 functions and 4 MiB of
//!   aggregate code, with a 1 MiB per-function decode bound.
//!
//! `partition_accounted` means that the ledger accounts for every signed token interval,
//! including gaps. It never means that every path was resolved. Council agenda keeps five
//! unresolved shared-reader contracts from the SDK-487 prototype: scoped integer values,
//! triggers, effects, graphical modifiers and AI weight.
mod control_flow;
mod dispatch;
mod inventory;
mod records;
mod replay;
mod tokens;
pub use records::*;
pub use replay::{derive, replay};

/// Bounded root-token and reader-routing method revision.
pub const METHOD: &str = "registry-fields/v1";
/// Recorded executable-input format.
pub const FORMAT: &str = "pdx-native-registry-fields/v1";

/// Resolve a candidate handle against immutable discovery inputs, never caller-edited output rows.
pub fn selection(
    discovery: &crate::engine::analysis::discovery::RegistryDiscoveryResult,
    subject: &crate::engine::analysis::discovery::RegistrySubject,
) -> Result<
    crate::engine::analysis::discovery::CandidateRecord,
    crate::engine::analysis::discovery::ForeignRegistrySubject,
> {
    use crate::engine::analysis::discovery::{ForeignRegistrySubject, StaticInput, candidates};
    discovery.subject(subject)?;
    let input: StaticInput =
        serde_json::from_slice(discovery.input_bytes()).map_err(|_| ForeignRegistrySubject)?;
    candidates(&input.symbols)
        .get(subject.ordinal)
        .cloned()
        .ok_or(ForeignRegistrySubject)
}
