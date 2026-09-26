use crate::engine::analysis::discovery::{CandidateRecord, Symbol};
use crate::engine::analysis::stop::{Stop, Unresolved};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Raw function bytes from the selected executable slice.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Function {
    /// Exact demangled function symbol.
    pub name: String,
    /// File virtual address.
    pub address: u64,
    /// Complete bounded function bytes.
    pub code: Vec<u8>,
}
/// Read-only data bytes from the selected executable slice, where the compiler puts jump tables.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataSection {
    /// File virtual address of the first byte.
    pub address: u64,
    /// The section's bytes.
    pub bytes: Vec<u8>,
}
/// What the method reads, all from the executable. The selection is a discovered candidate,
/// never a list of field names.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldInput {
    /// The template loader candidate whose fields are asked for.
    pub selection: CandidateRecord,
    /// Executable symbol inventory used to verify selection and resolve calls.
    pub symbols: Vec<Symbol>,
    /// Root, token construction, rejection and bounded owner helper functions.
    pub functions: Vec<Function>,
    /// Executable literal strings, indexed by file address.
    pub strings: BTreeMap<u64, String>,
    /// Read-only data sections, which hold the root's jump tables.
    pub read_only_data: Vec<DataSection>,
    /// Constructed persistent classes directly reached from the root.
    #[serde(default)]
    pub objects: Vec<ObjectReader>,
    /// Input collection limits that prevent a complete result.
    pub gaps: Vec<String>,
}
/// Provenance of a value at a dispatch boundary; unknown values are omitted.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Value {
    /// Immediate integer.
    Constant(i64),
    /// Offset from the original member owner.
    Owner(i64),
    /// Offset from the original reader.
    Reader(i64),
    /// Original signed field token.
    Token,
    /// Offset from the local stack pointer.
    Stack(i64),
    /// Load from a known base, with byte width.
    Load(Box<Value>, u8),
    /// The original field token plus a constant, as a zero-extended 32-bit word.
    TokenWord(i64),
    /// A jump-table entry that the token selects.
    TableEntry(TableEntry),
    /// A code address formed from a jump-table entry: `base + (entry << shift)`.
    TableTarget {
        /// The address that the entry is added to.
        base: u64,
        /// The entry that the token selects.
        entry: TableEntry,
        /// How far the entry is shifted left before the addition.
        shift: u8,
    },
}
/// The entry at `table + index * width`, where the index is the token word `token + offset`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TableEntry {
    /// Address of the first entry.
    pub table: u64,
    /// The constant added to the token to form the index.
    pub offset: i64,
    /// Bytes in one entry.
    pub width: u8,
    /// Whether the load sign-extends the entry.
    pub signed: bool,
}
/// Conditional alternative not determined by the field token.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Condition {
    /// Address of the instruction that tests the value.
    pub at: u64,
    /// Value tested, if its provenance is established.
    pub value: Option<Value>,
    /// Whether this alternative requires zero.
    pub zero: bool,
}
/// A reader join proves routing only, never the reader's accepted grammar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ReaderJoin {
    /// Known reader/delegate receives the original reader and owner-derived destination.
    Joined {
        /// Symbol of the callee in this build. It is not a stable reader-kind identity.
        callee: String,
        /// Proven argument values at the call boundary.
        arguments: BTreeMap<String, Value>,
        /// The root returns through this call, with no unexamined continuation.
        tail: bool,
    },
    /// A root-token path exists but its reader relationship could not be established.
    Missing(Unresolved),
}
/// Terminal disposition of a bounded root-token path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum PathOutcome {
    /// The verified base reader reports an unexpected member.
    Rejected,
    /// Root path reaches a reader boundary or an unresolved helper.
    Reader(ReaderJoin),
    /// Instruction or provenance obstruction; no field claim follows solely from this path.
    Gap(Unresolved),
}
/// Every token domain and state alternative remains visible, including rejected paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TokenPath {
    /// Inclusive signed 32-bit token interval.
    pub domain: [i64; 2],
    /// State conditions retained along this path.
    pub conditions: Vec<Condition>,
    /// Instruction addresses visited, including bounded helpers.
    pub instructions: Vec<u64>,
    /// Last instruction, or the unavailable root entry.
    pub terminal: u64,
    /// What this path establishes or fails to establish.
    pub outcome: PathOutcome,
}
/// One named root token with every corresponding reader alternative.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RootField {
    /// Name recovered from the engine token constructor.
    pub name: String,
    /// Engine-local signed token identity.
    pub token: i64,
    /// Address of the token constructor call that names the field.
    pub constructor: u64,
    /// Indices into the result's token-path ledger.
    pub paths: Vec<usize>,
    /// Token-to-reader join or explicit missing join for every listed path.
    pub readers: Vec<ReaderJoin>,
}
/// What kind of obligation a [`FieldGap`] leaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FieldGapKind {
    /// A function or name that the input should hold could not be collected.
    InputBoundary,
    /// The token constructor function could not be read into names.
    TokenTable,
    /// A path's reader could not be established.
    ReaderJoin,
    /// A path has no single named token.
    UnresolvedTokenPath,
    /// The paths do not account for every token interval.
    TokenPartition,
    /// Local runtime selections retain an unresolved enclosing context or method bound.
    RuntimeSelection,
    /// A jump table in the root reader could not be decoded.
    JumpTable,
}

/// A remaining obligation, separate from a discovered field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldGap {
    /// What kind of obligation remains.
    pub kind: FieldGapKind,
    /// Human-readable boundary or missing fact.
    pub reason: String,
    /// Where the walk through code stopped, when a walk stopped.
    pub stop: Option<Stop>,
    /// Related path index, when applicable.
    pub path: Option<usize>,
}

impl FieldGap {
    /// A gap that no walk located and no path holds.
    pub fn new(kind: FieldGapKind, reason: impl Into<String>) -> Self {
        Self {
            kind,
            reason: reason.into(),
            stop: None,
            path: None,
        }
    }

    /// A gap for a walk that could not follow code.
    pub fn unresolved(kind: FieldGapKind, unresolved: Unresolved) -> Self {
        Self {
            stop: unresolved.stop,
            ..Self::new(kind, unresolved.reason)
        }
    }
}
/// The root fields of one registry. This is bounded routing knowledge, never a complete schema.
#[derive(Debug, Clone, Serialize)]
pub struct RegistryFieldResult {
    /// Local use-time storage selections with unresolved enclosing context.
    pub uses: Vec<StorageSelection>,
    /// Established nested object collections.
    pub collections: Vec<CollectionField>,
    /// Named root fields, with explicit reader alternatives.
    pub fields: Vec<RootField>,
    /// Full bounded token-path ledger.
    pub paths: Vec<TokenPath>,
    /// Missing names, joins, instructions and other search obligations.
    pub gaps: Vec<FieldGap>,
    /// Whether all token intervals are accounted for; this does not close path gaps.
    pub partition_accounted: bool,
}

/// A persistent object's executable-derived constructor and virtual reader bindings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectReader {
    /// Demangled class name associated with these constructors and readers.
    pub class: String,
    /// Constructor entry addresses in the verified executable.
    pub constructors: Vec<u64>,
    /// Byte offsets within the object mapped to executable vtable address points.
    pub vtables: BTreeMap<u64, u64>,
    /// Executable pointer-slot addresses mapped to their resolved pointer values.
    pub pointers: BTreeMap<u64, u64>,
    /// Entry address of the persistent reader called on the constructed object.
    pub read: u64,
    /// Entry addresses of insertion specializations accepting this object's pointer.
    pub insert: Vec<u64>,
    /// Byte offset of the pointer buffer within the collection, if proven.
    pub data_offset: Option<u64>,
}

/// A root field that reads constructed objects into an owner collection.
#[derive(Debug, Clone, Serialize)]
pub struct CollectionField {
    /// Root loader token that constructs, reads, and inserts an object.
    pub token: i64,
    /// Collection byte offset relative to the owning object.
    pub offset: u64,
    /// Buffer-pointer byte offset relative to the collection, if proven.
    pub data_offset: Option<u64>,
    /// Demangled class name of the inserted object.
    pub class: String,
    /// Loader fields and gaps discovered for the inserted object's class.
    pub fields: Box<RegistryFieldResult>,
}

/// A conditional selection of a stored field in an owner method.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StorageSelection {
    /// Demangled owner method in which the local selection occurs.
    pub method: String,
    /// Selected storage field path from the registry root.
    pub field: Vec<String>,
    /// Boolean field path tested by the local selection.
    pub tested: Vec<String>,
    /// Whether the tested field is zero on this selection's branch.
    pub zero: bool,
}
