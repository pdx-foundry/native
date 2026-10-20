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
//! - Root dispatch follows an inherited member reader that receives the owner, reader and token
//!   unchanged, or a value reader `F(T&, CReader&)` that receives an owner member and the reader,
//!   and goes on after its call where it returns false. It stops at any other external delegate. A second bounded pass can prove that
//!   a field allocates, constructs, virtually reads and inserts the same persistent object on
//!   every path, or keeps it in one owner pointer. So are the keys of an embedded object that
//!   every path of a field reads, when the constructors agree on its reader and that reader has
//!   no block family. Each such object is an owner that the same analysis walks, with its
//!   receiver in the coordinates of the allocation that holds it, for the root and three owners
//!   below it; a fourth keeps its keys, and it and an owner that holds itself are gaps.
//! - Direct const owner methods supply local Boolean storage selections separately from loader
//!   conditions. Other signatures do not establish the original receiver and remain unresolved.
//!   Collection provenance is joined across loops; the enclosing selection context is unresolved.
//!   The use walk bounds each register to 16 origins and each function to 64 visits per instruction
//!   in aggregate. A local arm stops after 256 instructions. Unknown transfers lose provenance.
//! - It does not carry argument provenance through unknown calls, and does not assume that a
//!   stack restore recovers provenance.
//! - Root traversal: at most 500 instructions per path, 4,096 states and 1,024 tokens per jump
//!   table. Token-construction reachability: at most 40 visits per decoded instruction in
//!   aggregate.
//! - A jump table is followed only when its entries are offsets from a code address. A table of
//!   addresses, an unbounded index, an unreadable entry, a case outside the root or a default
//!   case that reaches a call is a `JumpTable` gap that names the table and the root. When a wide
//!   interval that leaves through the table's guard does not end at the rejection, a case of
//!   that table without a known reader is treated as the default.
//! - Bit-field reads and ORs (`ubfx`, `and`, `orr`) forget their result. A word integer read into
//!   a stack temporary can join its reader; the join does not establish final member storage. A
//!   Boolean read into a stack temporary is followed to the loader's merge after the call: one
//!   cleared bit of an owner word ORed with the temporary, or a copy of the temporary, stored
//!   before the return. Anything else on that way keeps the join at the call.
//! - Unknown instructions, unsupported addressing, missing symbols or names, conflicting token
//!   names, cycles and clobbered values become explicit gaps.
//! - Inputs: at most 256 functions and 4 MiB of aggregate code, with a 1 MiB per-function
//!   decode bound, and at most 64 MiB of read-only data.
//!
//! `partition_accounted` means that the ledger accounts for every signed token interval,
//! including gaps. It never means that every path was resolved. Council agenda keeps five
//! shared-reader contracts from the SDK-487 prototype that root dispatch does not resolve:
//! scoped integer values, triggers, effects, graphical modifiers and AI weight. The modifier
//! block and weight block methods attach the last two grammars.
//!
//! What the result does not establish: the template loader and owner symbol relationship is
//! static, not observed live ownership. Deeper grammars, full post-read behavior and dynamic names
//! stay open. Names come only from literal engine
//! token constructors; no config or content file is an authority.
mod containers;
mod control_flow;
mod dispatch;
mod inventory;
mod keywords;
mod member;
mod nested;
mod persistent;
mod records;
mod tokens;
mod uses;
pub(crate) use dispatch::{DispatchInput, explore_member};
pub(crate) use inventory::fields_and_gaps;
pub(crate) use member::{PersistentReads, follow_member, serializer_owner_slot, serializer_read};
pub(crate) use persistent::constructor_points;
pub use records::*;
pub(crate) use tokens::{Token, named_key, recover_decoded};
pub(crate) use uses::{has_owner_receiver, storage_offset};

use super::InputError;
use super::discovery::SymbolIndex;

/// Name and revision of the method, as stamped on its answers.
pub const METHOD: &str = "registry-fields/v71";

/// Find the root fields of the input's owner. The binding has established the owner, so the
/// method does not discover candidates again. Completeness is derived, never supplied.
pub fn analyze(input: &FieldInput) -> Result<RegistryFieldResult, InputError> {
    if input.functions.len() > 256
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
    let mut gaps: Vec<_> = input
        .gaps
        .iter()
        .map(|reason| FieldGap::new(FieldGapKind::InputBoundary, reason))
        .collect();
    let (tokens, token_gaps) = tokens::recover(input);
    gaps.extend(token_gaps);
    let symbol_index = SymbolIndex::new(&input.symbols);
    let owner = Owner::allocation(
        &input.owner,
        input.serializer.as_ref(),
        input.base_root.as_ref(),
        &input.objects,
        input.persistent.as_ref(),
    );
    let mut result = owner_result(input, &symbol_index, &owner, &tokens);
    gaps.append(&mut result.gaps);
    if !result.partition_accounted {
        gaps.push(FieldGap::new(
            FieldGapKind::TokenPartition,
            "token intervals are missing or overlap",
        ));
    }
    result.uses = uses::discover(
        input,
        &result.fields,
        &result.collections,
        result.partition_accounted,
    );
    if !result.collections.is_empty() {
        gaps.push(FieldGap::new(FieldGapKind::RuntimeSelection,
            "Local Boolean selections only: enclosing reachability, loop winner, callees, indirect callers, unsupported transfers and method bounds remain unresolved."));
    }
    result.loader = input.loader_root.as_ref().map(|root| {
        let (paths, mut gaps) = dispatch::explore_loader(input, &symbol_index, root);
        let (keys, path_gaps) = inventory::fields_and_gaps(&paths, &tokens);
        gaps.extend(path_gaps);
        LoaderKeys {
            keys,
            paths,
            gaps,
            item_reader: root.item_reader.clone(),
        }
    });
    result.gaps = gaps;
    Ok(result)
}

/// How many owners deep the method analyzes, the root included: an owner this deep below the
/// root keeps its keys, and the passes that reach deeper owners leave a gap.
pub(crate) const OWNER_DEPTH: usize = 4;

/// What a member reader receives in `x0`, in the coordinates of the allocation that holds the
/// owner: the allocation's constructors describe the same offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Receiver {
    /// The allocation plus this offset: the owner itself, a base subobject or an embedded object.
    Owner(i64),
    /// A serializer whose word at `slot` points at the allocation plus `owner_offset`.
    Serializer { slot: i64, owner_offset: i64 },
}

/// An owner whose member reader the method walks: the reader and its receiver, the constructor
/// evidence of the allocation that holds the receiver, and the objects that the reader inserts.
pub(super) struct Owner<'a> {
    pub reader: String,
    pub receiver: Receiver,
    pub objects: &'a [ObjectReader],
    pub persistent: Option<&'a PersistentInput>,
    /// The readers and receivers of the owners that hold this one, outermost first.
    pub enclosing: Vec<(String, Receiver)>,
}

impl<'a> Owner<'a> {
    /// The owner that starts an allocation of `class`: its keys are read by the serializer's
    /// member reader, by the base class's reader on its subobject, or by its own reader.
    pub fn allocation(
        class: &str,
        serializer: Option<&SerializerRoot>,
        base: Option<&BaseRoot>,
        objects: &'a [ObjectReader],
        persistent: Option<&'a PersistentInput>,
    ) -> Self {
        let (reader, receiver) = match (serializer, base) {
            (Some(serializer), _) => (
                serializer.reader.clone(),
                Receiver::Serializer {
                    slot: serializer.owner_slot,
                    owner_offset: 0,
                },
            ),
            (None, Some(base)) => (base.reader.clone(), Receiver::Owner(base.offset)),
            (None, None) => (
                format!("{class}::ReadMember(CReader&, int)"),
                Receiver::Owner(0),
            ),
        };

        Self {
            reader,
            receiver,
            objects,
            persistent,
            enclosing: Vec::new(),
        }
    }

    /// The owner that this one holds: an embedded object or a stack serializer in the same
    /// allocation, or a new allocation that `child` starts.
    pub fn holding<'b>(&self, child: Owner<'b>) -> Owner<'b> {
        let mut enclosing = self.enclosing.clone();
        enclosing.push((self.reader.clone(), self.receiver));

        Owner { enclosing, ..child }
    }

    /// Whether the method stops at this owner: it is as deep as the bound, or an owner that
    /// holds it has the same reader and receiver.
    pub(super) fn stops(&self) -> Option<&'static str> {
        if self.enclosing.len() >= OWNER_DEPTH {
            return Some("the nested owner is deeper than the method's bound");
        }

        let cycle = self
            .enclosing
            .iter()
            .any(|(reader, receiver)| *reader == self.reader && *receiver == self.receiver);

        cycle.then_some("the nested owner holds itself")
    }
}

/// The fields of `owner` and what they read: dispatch paths, persistent members, embedded keys
/// and inserted objects. Root and nested owners share it.
fn owner_result(
    input: &FieldInput,
    symbol_index: &SymbolIndex,
    owner: &Owner<'_>,
    tokens: &std::collections::BTreeMap<i64, Token>,
) -> RegistryFieldResult {
    let (paths, mut gaps, order_checks) =
        dispatch::explore_owner(input, symbol_index, &owner.reader, owner.receiver);
    let (fields, path_gaps) = inventory::fields_and_gaps(&paths, tokens);
    gaps.extend(path_gaps);
    let partition_accounted = inventory::partition_accounted(&paths);
    let key_order = inventory::key_order(&order_checks, &paths, &fields, partition_accounted);
    // At the bound or a cycle the owner keeps its keys, and the passes that reach deeper owners
    // leave a gap.
    if let Some(stop) = owner.stops() {
        gaps.push(FieldGap::new(FieldGapKind::ReaderJoin, stop));
        return RegistryFieldResult {
            fields,
            paths,
            gaps,
            partition_accounted,
            key_order,
            ..RegistryFieldResult::default()
        };
    }
    let collections = nested::discover(input, symbol_index, owner, &fields, tokens);
    let persistent = persistent::discover(owner.persistent, &input.read_only_data, &fields);
    gaps.extend(persistent.gaps);
    let embedded = persistent::embedded(
        input,
        symbol_index,
        owner,
        &fields,
        &paths,
        &persistent.readers,
        tokens,
    );
    let keyword_domains = match owner.receiver {
        Receiver::Serializer { .. } => keywords::through_serializer(&fields),
        Receiver::Owner(_) => {
            keywords::domains(input, symbol_index, &owner.reader, &fields, tokens)
        }
    };

    RegistryFieldResult {
        persistent: persistent.readers,
        persistent_points: persistent.points,
        scoped_destinations: persistent.scoped,
        stored_words: persistent.words,
        container_masks: persistent.containers,
        collections,
        embedded,
        keyword_domains,
        fields,
        paths,
        gaps,
        partition_accounted,
        key_order,
        ..RegistryFieldResult::default()
    }
}
