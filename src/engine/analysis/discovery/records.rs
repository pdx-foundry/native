use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One symbol of the executable. A native name locates code; it is never a public identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Symbol {
    /// Demangled symbol spelling.
    pub name: String,
    /// File virtual address.
    pub address: u64,
}
/// Where the build's recipe places the startup scheduling table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchedulerLayout {
    /// Start of the literal initialization range.
    pub start: u64,
    /// Exclusive end before scheduling begins.
    pub end: u64,
    /// Table offset from the receiver register x19.
    pub offset: u64,
    /// Row stride in bytes.
    pub stride: u64,
    /// Number of bounded scheduling rows, not the number of registries.
    pub count: usize,
}
/// Immutable inputs read from one verified executable buffer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticInput {
    /// Symbol inventory, without a supplied registry list.
    pub symbols: Vec<Symbol>,
    /// Raw ARM64 initialization bytes.
    pub code: Vec<u8>,
    /// Bounds of the scheduling table.
    pub layout: SchedulerLayout,
    /// Resolved pointer locations and target-local values.
    pub pointers: BTreeMap<u64, u64>,
    /// Literal strings keyed by their file addresses.
    pub strings: BTreeMap<u64, String>,
    /// Vtable address points with executable-derived owner adjustments and dispatch slots.
    pub vtables: BTreeMap<u64, VtableWitness>,
}
/// A scheduling row stays visible even when no template candidate matches it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SchedulingWitness {
    /// Position in the bounded startup table.
    pub index: usize,
    /// Positions, in the result's candidates, of the candidates whose database type a function
    /// slot of this row names.
    pub candidates: Vec<usize>,
    /// All literal slots and the name were recovered.
    pub recovered: bool,
}
/// A missing or unresolved part of the bounded search.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiscoveryGap {
    /// Machine-readable reason.
    pub kind: DiscoveryGapKind,
    /// Position of the related candidate, when there is one.
    pub candidate: Option<usize>,
    /// Precise missing obligation.
    pub reason: String,
}
/// Discovery gaps never imply the game has no corresponding registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum DiscoveryGapKind {
    /// A static candidate: no live loader was observed for it.
    UnobservedCandidate,
    /// The scheduling row is outside the template method: a custom, nested or late loader.
    OutsideTemplate,
    /// Missing literal, pointer, name, or slot.
    Scheduler,
    /// Unknown call or unsupported instruction invalidates tracked values.
    UnknownInstruction,
    /// Shared, custom, nested and late paths exceed this method.
    UnresolvedHelper,
}
/// Template candidates and the startup scheduling table, with explicit bounds. The method
/// never establishes that every registry was found.
#[derive(Debug, Clone, Serialize)]
pub struct RegistryDiscovery {
    /// Template loader candidates. None is an established owner yet.
    pub candidates: Vec<CandidateRecord>,
    /// Every scheduling row, including unresolved rows.
    pub scheduling: Vec<SchedulingWitness>,
    /// Unobserved and unresolved obligations.
    pub gaps: Vec<DiscoveryGap>,
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
    /// Named reader symbol exists.
    pub has_named_member_reader: bool,
}
/// One literal row of the scheduling table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SchedulerRow {
    /// Bounded table index.
    pub index: usize,
    /// Literal scheduling name, not a public registry identity.
    pub name: Option<String>,
    /// Name pointer and five function slots.
    pub values: Vec<Option<u64>>,
    /// Instruction addresses that wrote each slot.
    pub sources: Vec<Option<String>>,
    /// Recovered or gap.
    pub status: String,
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
