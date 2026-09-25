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
    /// Every pointer location that the loader binds to another image, named or not.
    #[serde(default)]
    pub bound_slots: BTreeSet<u64>,
    /// Literal strings keyed by their file addresses.
    pub strings: BTreeMap<u64, String>,
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
