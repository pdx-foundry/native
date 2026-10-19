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
//!   unchanged, and goes on after its call where it returns false. It stops at any other
//!   external delegate. A second bounded pass can prove that
//!   a field allocates, constructs, virtually reads and inserts the same persistent object on
//!   every path, or keeps it in one owner pointer. Its named child fields are then derived to depth one. So are the keys of an
//!   embedded object that every path of a field reads, when the constructors agree on its
//!   reader and that reader has no block family.
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
pub const METHOD: &str = "registry-fields/v52";

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
    let owner = Owner {
        class: &input.owner,
        serializer: input.serializer.as_ref(),
        objects: &input.objects,
        persistent: input.persistent.as_ref(),
    };
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

/// A class whose member reader the method walks, with the objects that it inserts and the
/// constructor evidence of its persistent members.
pub(super) struct Owner<'a> {
    pub class: &'a str,
    /// The serializer whose member reader reads the class's keys, when the class has none.
    pub serializer: Option<&'a SerializerRoot>,
    pub objects: &'a [ObjectReader],
    pub persistent: Option<&'a PersistentInput>,
}

impl Owner<'_> {
    /// The member reader of the owner's keys, and the receiver offset of the owner pointer when
    /// it is a serializer's.
    fn reader(&self) -> (String, Option<i64>) {
        match self.serializer {
            Some(serializer) => (serializer.reader.clone(), Some(serializer.owner_slot)),
            None => (format!("{}::ReadMember(CReader&, int)", self.class), None),
        }
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
    let (reader, owner_slot) = owner.reader();
    let (paths, mut gaps) = dispatch::explore_owner(input, symbol_index, &reader, owner_slot);
    let (fields, path_gaps) = inventory::fields_and_gaps(&paths, tokens);
    gaps.extend(path_gaps);
    let partition_accounted = inventory::partition_accounted(&paths);
    let collections = nested::discover(input, symbol_index, owner, &fields, tokens);
    let persistent = persistent::discover(owner.persistent, &input.read_only_data, &fields);
    gaps.extend(persistent.gaps);
    let embedded = persistent::embedded(
        input,
        symbol_index,
        &fields,
        &paths,
        &persistent.readers,
        tokens,
    );
    let keyword_domains = match owner_slot {
        Some(_) => keywords::through_serializer(&fields),
        None => keywords::domains(input, symbol_index, &reader, &fields, tokens),
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
        ..RegistryFieldResult::default()
    }
}
