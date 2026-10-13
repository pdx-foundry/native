use crate::engine::analysis::discovery::Symbol;
use crate::engine::analysis::stop::{Stop, Unresolved};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// Raw function bytes from the selected executable slice.
#[derive(Debug, Clone, Serialize)]
pub struct Function {
    /// Exact demangled function symbol.
    pub name: String,
    /// File virtual address.
    pub address: u64,
    /// Complete bounded function bytes.
    pub code: Vec<u8>,
}
/// Read-only data bytes from the selected executable slice, where the compiler puts jump tables.
#[derive(Debug, Clone, Serialize)]
pub struct DataSection {
    /// File virtual address of the first byte.
    pub address: u64,
    /// The section's bytes.
    pub bytes: Vec<u8>,
}
/// What the method reads, all from the executable. The owner is a discovered class, never a list
/// of field names.
#[derive(Debug, Clone, Serialize)]
pub struct FieldInput {
    /// Bound signatures and layouts for compound key readers.
    pub key_readers: KeyReaders,
    /// The class whose `ReadMember(CReader&, int)` is the root. The binding establishes it:
    /// `VerifiedAnalysis::field_input` refuses an owner that no registry of the executable has.
    pub owner: String,
    /// Executable symbol inventory used to resolve calls.
    pub symbols: Vec<Symbol>,
    /// Root, token construction, rejection and bounded owner helper functions.
    pub functions: Vec<Function>,
    /// Executable literal strings, indexed by file address.
    pub strings: BTreeMap<u64, String>,
    /// Read-only data sections, which hold the root's jump tables.
    pub read_only_data: Vec<DataSection>,
    /// Constructed persistent classes directly reached from the root.
    pub objects: Vec<ObjectReader>,
    /// Constructor evidence for generic persistent member destinations.
    pub persistent: Option<PersistentInput>,
    /// The loader function whose root switch builds the owner, when a custom loader selects a
    /// scope for each root key that it accepts.
    pub loader_root: Option<LoaderRoot>,
    /// Input collection limits that prevent a complete result.
    pub gaps: Vec<String>,
}

/// A loader function that reads each root statement, switches on its token and passes a scope
/// constant to the owner's `(EScopeType)` constructor for each root key that it accepts.
#[derive(Debug, Clone, Serialize)]
pub struct LoaderRoot {
    /// The loader function's name; its body is one of the input's functions.
    pub function: String,
    /// The instruction after the statement read, where the walk starts.
    pub start: u64,
    /// The callee-saved register that holds the reader at `start`.
    pub reader: String,
    /// The reader offset of the statement's token word.
    pub token_offset: u64,
    /// Entry addresses of the owner's `(EScopeType)` constructor.
    pub constructors: BTreeSet<u64>,
    /// The persistent reader at the owner's primary address point.
    pub item_reader: Option<ConcreteReader>,
}
/// Provenance of a value at a dispatch boundary; unknown values are omitted.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Value {
    /// Immediate integer.
    Constant(i64),
    /// Offset from the original member owner.
    Owner(i64),
    /// Offset from the original reader.
    Reader(i64),
    /// Original signed field token.
    Token,
    /// The scope supplied to the enclosing command member reader.
    EnclosingScope,
    /// Offset from the local stack pointer.
    Stack(i64),
    /// Load from a known base, with byte width.
    Load(Box<Value>, u8),
    /// An address offset from a loaded or computed value.
    Offset(Box<Value>, i64),
    /// An address formed by adding a scaled index.
    Indexed(Box<Value>, Box<Value>, u8),
    /// Base plus an index multiplied by a constant stride.
    SumProduct(Box<Value>, Box<Value>, i64),
    /// Boolean result of comparing a value with any listed constant.
    EqualsAny(Box<Value>, Vec<i64>),
    /// The original field token plus a constant, as a zero-extended 32-bit word.
    TokenWord(i64),
    /// A loaded word with one bit cleared: the load and the bit.
    ClearedBit(Box<Value>, u8),
    /// A loaded word whose cleared bit now holds the low bit of a stack byte: the load and the
    /// bit.
    InsertedBit(Box<Value>, u8),
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
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
    /// Known reader/delegate receives the original reader and a proven owner or integer stack
    /// destination.
    Joined {
        /// Symbol of the callee in this build. It is not a stable reader-kind identity.
        callee: String,
        /// Proven argument values at the call boundary.
        arguments: BTreeMap<String, Value>,
        /// The root returns through this call, with no unexamined continuation.
        tail: bool,
    },
    /// A compound reader whose input and destination are established on this path.
    Stored {
        /// Bound function at the final read or store.
        callee: String,
        /// Broad value kind established by the compound idiom.
        kind: crate::ReaderKind,
        /// Storage offset in the original owner, or `None` when the idiom establishes only the
        /// value's kind.
        destination: Option<i64>,
        /// Storage behavior established by the idiom.
        repeat: crate::RepeatBehavior,
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
#[derive(Debug, Clone, Default, Serialize)]
pub struct RegistryFieldResult {
    /// Local use-time storage selections with unresolved enclosing context.
    pub uses: Vec<StorageSelection>,
    /// Concrete methods established at owner-relative persistent destinations.
    pub persistent: BTreeMap<i64, ConcreteReader>,
    /// Constructor-agreed address points for persistent member destinations.
    pub persistent_points: BTreeMap<i64, u64>,
    /// Constructor-agreed vtable address points at scoped numeric destinations.
    pub scoped_destinations: BTreeMap<i64, u64>,
    /// Constructor-agreed words inside persistent destinations, by destination and offset within
    /// it. Only the words that `PersistentInput::requested_words` names are read.
    pub stored_words: BTreeMap<(i64, u64), u64>,
    /// The category mask of each modifier container that the owner constructors build at a
    /// persistent destination, or why it is not established.
    pub container_masks: BTreeMap<i64, Result<u64, Unresolved>>,
    /// Established nested object collections.
    pub collections: Vec<CollectionField>,
    /// Fixed keys of the embedded objects that root fields read and that no block family reads.
    pub embedded: Vec<EmbeddedField>,
    /// Named root fields, with explicit reader alternatives.
    pub fields: Vec<RootField>,
    /// Full bounded token-path ledger.
    pub paths: Vec<TokenPath>,
    /// Missing names, joins, instructions and other search obligations.
    pub gaps: Vec<FieldGap>,
    /// Whether all token intervals are accounted for; this does not close path gaps.
    pub partition_accounted: bool,
    /// The root keys of the input's loader root, when it has one.
    pub loader: Option<LoaderKeys>,
}

/// The root keys that a loader root names, with their own path ledger. Each key's readers join
/// the owner's constructor, end in a gap, or are absent where every path rejects.
#[derive(Debug, Clone, Default, Serialize)]
pub struct LoaderKeys {
    /// Named root keys of the loader's switch; `paths` indexes `LoaderKeys::paths`.
    pub keys: Vec<RootField>,
    /// The loader root's bounded token-path ledger.
    pub paths: Vec<TokenPath>,
    /// Gaps of the loader walk; `path` indexes `LoaderKeys::paths`.
    pub gaps: Vec<FieldGap>,
    /// The persistent reader at the owner's primary address point.
    pub item_reader: Option<ConcreteReader>,
}

impl RegistryFieldResult {
    /// The result of the object that the root field with `token` collects or embeds.
    pub(crate) fn nested(&self, token: i64) -> Option<&RegistryFieldResult> {
        self.nested_results()
            .find(|(nested, _)| *nested == token)
            .map(|(_, result)| result)
    }

    /// The results of the collected and embedded objects, by the token of their root field.
    pub(crate) fn nested_results(&self) -> impl Iterator<Item = (i64, &RegistryFieldResult)> {
        self.collections
            .iter()
            .map(|collection| (collection.token, collection.fields.as_ref()))
            .chain(
                self.embedded
                    .iter()
                    .map(|embedded| (embedded.token, embedded.fields.as_ref())),
            )
    }
}

/// A persistent object's executable-derived constructor and virtual reader bindings.
#[derive(Debug, Clone, Serialize)]
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
    /// Entry addresses of factories that allocate, construct and return the object through `x8`.
    pub factories: Vec<u64>,
    /// Entry addresses of insertion specializations accepting this object's pointer.
    pub insert: Vec<u64>,
    /// Entry addresses of insertion specializations that take the object from a scoped pointer
    /// and are proven to clear that pointer.
    pub moving_insert: Vec<u64>,
    /// Byte offset of the pointer buffer within the collection, if proven.
    pub data_offset: Option<u64>,
    /// Byte offset of the element count within the collection, if proven.
    pub count_offset: Option<u64>,
    /// The persistent reader at the object's primary address point.
    pub reader: Option<PointReader>,
    /// Constructor evidence for the persistent members of an object whose reader has no block
    /// family, which the method walks as a nested owner.
    pub persistent: Option<PersistentInput>,
    /// The objects that such a nested owner's member reader inserts.
    pub objects: Vec<ObjectReader>,
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
    /// Element-count byte offset relative to the collection, if proven.
    pub count_offset: Option<u64>,
    /// Demangled class name of the inserted object.
    pub class: String,
    /// The inserted object's reader when its family has a block grammar. Such a collection
    /// carries no loader fields; the family's grammar describes its members.
    pub reader: Option<PointReader>,
    /// Loader fields and gaps discovered for the inserted object's class.
    pub fields: Box<RegistryFieldResult>,
    /// The category mask that the inserted object's constructor gives it, when that constructor
    /// builds a modifier container.
    pub container_mask: Option<Result<u64, Unresolved>>,
    /// The distinct `EScopeType` masks that the inserted object's constructor receives on the
    /// joined paths, `None` where a path's scope is not known. Empty when the constructor takes
    /// no scope.
    pub construction_scopes: Vec<Option<i64>>,
}

/// A root field that reads one embedded persistent object whose member reader has no block
/// family, with the keys that the member reader dispatches.
#[derive(Debug, Clone, Serialize)]
pub struct EmbeddedField {
    /// Root loader token whose every path reads the object.
    pub token: i64,
    /// The object's reader, which hands its keys to the member reader.
    pub read: String,
    /// The object's member reader.
    pub member: String,
    /// Loader fields and gaps of the member reader. Its keys are derived to depth one.
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

/// Concrete methods and independently established family at one vtable address point.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConcreteReader {
    pub read: String,
    pub member: String,
    /// The targets of the post-read hook slots, when they have a unique symbol: `InitPostRead()`
    /// first, then `InitPostRead(CString const&, int, int)`.
    pub post_read: [Option<String>; 2],
    pub family: crate::BlockFamily,
    /// The target of the family's delegate slot, for readers that share read and member.
    pub delegate: Option<String>,
}

/// A persistent reader with the vtable address point that holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PointReader {
    /// Executable virtual address of the object's vtable address point.
    pub point: u64,
    /// The concrete methods and family bound at that address point.
    pub reader: ConcreteReader,
}

/// Exact executable inputs for owner constructor evaluation.
#[derive(Debug, Clone, Serialize)]
pub struct PersistentInput {
    pub(crate) constructor_calls: super::super::receivers::ConstructorCalls,
    pub constructors: Vec<Function>,
    pub summaries: BTreeMap<u64, BTreeMap<u64, u64>>,
    /// Called constructor bodies for initial embedded storage.
    pub constructor_bodies: BTreeMap<u64, Function>,
    pub pointers: BTreeMap<u64, u64>,
    /// The pointer slots outside the constant sections, whose targets code may replace.
    pub writable_slots: BTreeSet<u64>,
    pub never_return: Vec<u64>,
    pub readers: BTreeMap<u64, ConcreteReader>,
    /// The offsets of the words to read inside each persistent object, by its address point.
    pub requested_words: BTreeMap<u64, BTreeSet<u64>>,
    /// How owner constructors build a modifier container and where it keeps its mask.
    pub containers: ModifierContainers,
    /// 32-bit words in zero-fill memory, with the value that the static initializer that stores
    /// each one leaves there.
    pub initialized_words: BTreeMap<u64, u64>,
}

/// The constructors of a modifier container, which store its category mask, and where the
/// container keeps the mask.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ModifierContainers {
    /// Constructors that take the mask as their category argument in `w2`.
    pub category: BTreeSet<u64>,
    /// Constructors that store the mask of every category.
    pub default: BTreeSet<u64>,
    /// Offset of the 32-bit category mask from the container's start.
    pub mask_offset: u64,
}

/// Signature addresses for shared compound key-reader idioms.
#[derive(Debug, Clone, Default, Serialize)]
pub struct KeyReaders {
    /// Destination sizes for target, optional string, and string array.
    pub compound_sizes: [i64; 3],
    pub array_data: i64,
    pub array_count: i64,
    pub string_stride: i64,
    pub value_token: i64,
    pub token_text: i64,
    pub token_copy: Vec<u64>,
    pub target_construct: Vec<u64>,
    pub target_move: Option<u64>,
    pub string_emplace: Option<u64>,
    pub optional_string: Option<u64>,
    pub string_read: Option<u64>,
    pub persistent_read: Option<u64>,
    /// `CString::CString(char const*)`, which makes a string from a token's text.
    pub string_from_text: Vec<u64>,
}
