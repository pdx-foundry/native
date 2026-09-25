use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// One symbol of the executable. A native name locates code; it is never a public identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Symbol {
    /// Demangled symbol spelling.
    pub name: String,
    /// File virtual address.
    pub address: u64,
}
/// Immutable inputs read from one verified executable buffer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticInput {
    /// Symbol inventory, without a supplied registry list.
    pub symbols: Vec<Symbol>,
    /// Resolved pointer locations and target-local values.
    pub pointers: BTreeMap<u64, u64>,
    /// Imported or chained global locations and their exact demangled binding names.
    pub global_bindings: BTreeMap<u64, String>,
    /// Every pointer location that the loader binds to another image, named or not.
    #[serde(default)]
    pub bound_slots: BTreeSet<u64>,
    /// Literal strings keyed by their file addresses.
    pub strings: BTreeMap<u64, String>,
    /// Vtable address points with executable-derived owner adjustments and dispatch slots.
    pub vtables: BTreeMap<u64, VtableWitness>,
}
/// One template loader candidate, in the build's own names. Not a public identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateRecord {
    /// Database type argument.
    pub database: String,
    /// Owner type argument; not an established owner until joined.
    pub owner_candidate: String,
    /// Loader symbol.
    pub loader: String,
    /// File address of the loader, in hexadecimal.
    pub address: String,
    /// File address of the matching initial-load entry, when exactly one exists.
    pub initial_loader: Option<String>,
    /// Named reader symbol exists.
    pub has_named_member_reader: bool,
}
/// Executable-derived persistent-base adjustment and shared reader slot.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VtableWitness {
    /// Concrete owner named by the executable vtable symbol.
    pub owner: String,
    /// Signed offset from this base to the concrete object.
    pub offset_to_top: i64,
    /// Function pointer at the shared member-dispatch offset.
    pub member: u64,
}
