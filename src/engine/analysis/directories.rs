//! Content directory of a registry: the text that its constructor passes to the base constructor.
//!
//! Every template database constructor calls the shared base constructor with its content
//! directory as a `CString` argument. The method finds that call and establishes what the
//! argument register holds. Two compiled shapes exist:
//!
//! - **Temporary.** The constructor builds a `CString` on its stack from a text literal, then
//!   passes that stack object. Most registries have this shape.
//! - **Global.** The constructor passes a global `CString`. A static initializer elsewhere builds
//!   that global from a text literal. The source file then declares the path as a named constant
//!   (`common/ship_categories` on M45).
//!
//! The scan is linear and bounded. It tracks only constants, stack addresses and register copies.
//! A frame address, such as `x29 - 0x98` after the prologue sets `x29` from the stack pointer, is
//! a stack address.
//! A call clears the caller-saved registers, and any other instruction clears the registers that
//! it may write, including the second register of a load pair. It does not follow branches, so a value that arrives on another path is unknown:
//! the target of a conditional branch is a join, where the scan forgets every register and every
//! built object. A loop before the literal is harmless, since the literal is built after it.
//! The target of an unconditional `b` is not a join. If a constructor closes a loop with `b`
//! before the base-constructor call, treat those targets as joins too.
//! It stops at a return or an unconditional branch, since the next word is not reached from it.
//! When a base-constructor call follows the stop, the arguments found before it are not a
//! proof: the call that the scan did not read may pass another directory.
//! Exactly one distinct directory over all constructor bodies names the registry. None or several
//! is a gap; the method never selects one of several.
//!
//! A database with a custom loader has no base-constructor directory. [`loader_directory`] takes
//! the text literal that the database's own functions pass to the file enumeration instead: one
//! distinct literal names the registry, several literals are [`Directory::Ambiguous`], and an
//! argument that is not established is a gap. It reads every call with the block scan of
//! [`enumeration_calls`], so the literal must be set in the call's own basic block, and a call
//! after a stop is read too. The literal can also arrive through a global `char const*`: an `ldr`
//! from a known address loads the pointer slot, and the slot's rebase target is the literal. The
//! method takes the slot's initial value as its value when the loader runs.
//!
//! The directory can also be the text of a `CString` member of the database instance, which the
//! enumerating function, a member, reads through `x0` at its entry. [`member_texts`] gives the
//! literal that the instance's constructors, or the object that `CreateInstance()` allocates,
//! build that member from. Two assumptions hold on M452 and are not proved: the enumerating
//! function is a member function, and the object that `CreateInstance()` allocates is the
//! instance (it stores it to `_pInstance`).
//!
//! The directory can also be the text of a `CString` that the loader builds on its stack from a
//! literal, or from the literal that a pointer slot holds, and passes through the short-string
//! select in the same block ([`built_text_select`]). The block scan keeps a value across a forward
//! branch when every path into the block holds it, so a frame address set in the prologue
//! survives a branch that skips one store.
use std::collections::{BTreeMap, BTreeSet};

use super::decode::{
    add_immediate, adrp, call_or_jump_target, conditional_branch_target, ldr_immediate,
    sub_immediate,
};

/// Method revision recorded in each answer's source.
pub const METHOD: &str = "registry-directories/v9";

/// Base constructor whose `CString` argument is the content directory.
pub const BASE_CONSTRUCTOR: &str =
    "CSingleObjectGameDatabaseBase::CSingleObjectGameDatabaseBase(CString const&)";
/// Constructor that builds a `CString` from a text literal.
pub const STRING_CONSTRUCTOR: &str = "CString::CString(char const*)";
/// File enumeration whose first argument is a loader's content directory.
pub const FILE_ENUMERATION: &str =
    "VFSGetEnumeratedFiles(char const*, CPdxArray<CString, int>&, char const*, char const*, int)";
/// Allocation whose result `CreateInstance()` builds the database instance in.
pub const ALLOCATION: &str = "operator new(unsigned long)";
/// Name prefix of the static initializers that can build a global `CString`.
pub const INITIALIZER_PREFIX: &str = "__GLOBAL__sub_I_";

/// One function body.
#[derive(Debug, Clone)]
pub struct Constructor {
    /// File virtual address of the first instruction.
    pub address: u64,
    /// Complete function bytes.
    pub code: Vec<u8>,
}

/// Entry addresses of the two engine functions that give the scan its meaning.
#[derive(Debug, Clone, Default)]
pub struct Anchors {
    /// Every body of the base constructor.
    pub base_constructors: BTreeSet<u64>,
    /// Every body of the literal `CString` constructor.
    pub string_constructors: BTreeSet<u64>,
    /// Every body of the file enumeration.
    pub file_enumerations: BTreeSet<u64>,
    /// Every body of the allocation.
    pub allocations: BTreeSet<u64>,
}

/// What the constructors of one class establish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Directory {
    /// Exactly one directory.
    Named(String),
    /// No base-constructor call, or an argument that the scan could not establish.
    Missing,
    /// Several distinct directories. A template registry selects none of them; a custom loader
    /// names each one.
    Ambiguous(Vec<String>),
}

/// What the argument register holds at one call to the base constructor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Argument {
    /// A `CString` that the same function built from this text literal.
    Literal(String),
    /// A global `CString` at this address. `globals` resolves it.
    Global(u64),
    /// Not established.
    Unknown,
}

/// What the scan knows a register holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Value {
    Unknown,
    Constant(u64),
    Stack(u64),
    /// The pointer loaded from the slot at this address.
    Slot(u64),
    /// The instance's address plus this offset.
    Object(u64),
    /// The text of the instance's `CString` member at this offset.
    MemberText(u64),
    /// The text of a `CString` that the scan saw built from the literal at this address.
    Text(u64),
    /// The text of a `CString` that the scan saw built from the literal that the pointer slot at
    /// this address holds.
    SlotText(u64),
}

/// What the scan knows of the database instance.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Instance {
    /// Nothing: the function is not one of the instance's.
    Unknown,
    /// `x0` at the entry is the instance, as in a constructor or another member function.
    Entry,
    /// The result of the one allocation is the instance, as in `CreateInstance()`.
    Allocated,
}

/// The file-selection argument registers at one file-enumeration call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct EnumerationCall {
    /// The address of the call.
    pub address: u64,
    /// The first argument: the directory.
    pub directory: Value,
    /// The third argument: the text that a file name must end with.
    pub suffix: Value,
    /// The fourth argument: the text that a file name must contain.
    pub substring: Value,
    /// The fifth argument: the depth.
    pub depth: Value,
}

/// A `CString` built from a literal: where it was built, and the literal's address.
struct Built {
    destination: Value,
    literal: u64,
}

/// The argument register at one base-constructor call.
enum Passed {
    /// A `CString` object that this function built from the literal at this address.
    Built(u64),
    /// Some other value: a global address, or unknown.
    Other(Value),
}

/// What one linear scan found.
#[derive(Default)]
struct Scan {
    /// Each literal `CString` construction.
    built: Vec<Built>,
    /// Each base-constructor call, with what its argument register holds.
    passed: Vec<Passed>,
    /// Whether a base-constructor call follows the stop that ended the scan, so the scan did not
    /// read its argument.
    unread_base_call: bool,
}

/// Linear scan of one function, which knows the address of the database instance as `instance`
/// says.
fn scan(function: &Constructor, anchors: &Anchors, instance: Instance) -> Scan {
    let mut registers = [Value::Unknown; 32];
    if instance == Instance::Entry {
        registers[0] = Value::Object(0);
    }
    // Objects built so far, by location. A later construction at one location replaces the
    // earlier one, and any other call that receives the object forgets it.
    let mut objects: Vec<(Value, u64)> = Vec::new();
    let mut found = Scan::default();
    let words = function.code.as_chunks::<4>().0;
    let joins = join_points(words, function.address);
    for (index, word) in words.iter().enumerate() {
        let word = u32::from_le_bytes(*word);
        let address = function.address + index as u64 * 4;
        if joins.contains(&address) {
            registers.fill(Value::Unknown);
            objects.clear();
        }
        if let Some(target) = branch_with_link(word, address) {
            if anchors.base_constructors.contains(&target) {
                found
                    .passed
                    .push(match objects.iter().find(|(at, _)| *at == registers[1]) {
                        Some(&(_, literal)) if registers[1] != Value::Unknown => {
                            Passed::Built(literal)
                        }
                        _ => Passed::Other(registers[1]),
                    });
            } else {
                objects.retain(|(at, _)| *at != registers[0]);
                if anchors.string_constructors.contains(&target)
                    && registers[0] != Value::Unknown
                    && let Value::Constant(literal) = registers[1]
                {
                    objects.push((registers[0], literal));
                    found.built.push(Built {
                        destination: registers[0],
                        literal,
                    });
                }
            }
            registers[..=18].fill(Value::Unknown);
            if instance == Instance::Allocated && anchors.allocations.contains(&target) {
                registers[0] = Value::Object(0);
            }
        } else if let Some(transfer) = unconditional_branch(word) {
            match transfer {
                Transfer::Stop => {
                    let rest = &words[index + 1..];
                    found.unread_base_call =
                        calls_any(rest, address + 4, &anchors.base_constructors);
                    break;
                }
                // The callee is unknown, so it may receive or change any object.
                Transfer::IndirectCall => {
                    objects.clear();
                    registers[..=18].fill(Value::Unknown);
                }
            }
        } else {
            step(&mut registers, word, address);
        }
    }
    found
}

/// The file-selection arguments at every file-enumeration call in one function, as the call's own
/// basic block sets them.
///
/// Unlike [`scan`], this scan reads the whole function. Every branch target and every word after an
/// unconditional branch starts a block with every register unknown, so a value is known only
/// when the block that makes the call sets it. A shared template loader chooses its directory
/// on two paths, then sets the other arguments after they join.
///
/// `x0` at the entry is taken as the database instance, since a loader's enumerating functions
/// are members of it. A block that only forward branches and the word before it enter starts with
/// the values that every one of those paths holds; a block that a backward branch enters, and
/// every block of a function with a register jump, starts with every register unknown. The one
/// join that keeps a value no path holds is a [`member_text_select`]: its block starts with `x0`
/// holding the text of that member. A [`built_text_select`] gives `x0` the text of a `CString`
/// that the same block built from a literal.
pub(super) fn enumeration_calls(function: &Constructor, anchors: &Anchors) -> Vec<EnumerationCall> {
    let words = function.code.as_chunks::<4>().0;
    let starts = block_starts(words, function.address);
    let incoming = incoming_branches(words, function.address);
    let unmerged = unmerged_starts(words, function.address);
    // A register jump can enter any word, so every word starts a block with nothing known.
    let register_jump = jumps_through_register(words);
    let mut entering: BTreeMap<u64, [Value; 32]> = BTreeMap::new();
    let mut registers = [Value::Unknown; 32];
    registers[0] = Value::Object(0);
    let mut falls_through = true;
    let mut selected_text = None;
    let mut built: Vec<(Value, Value)> = Vec::new();
    let mut pending_text = None;
    let mut calls = Vec::new();
    for (index, word) in words.iter().enumerate() {
        let address = function.address + index as u64 * 4;
        if starts.contains(&address) || register_jump {
            let branched = entering.remove(&address);
            registers = match (falls_through, branched) {
                _ if unmerged.contains(&address) => [Value::Unknown; 32],
                (true, Some(branched)) => agreed(&registers, &branched),
                (true, None) => registers,
                (false, Some(branched)) => branched,
                (false, None) => [Value::Unknown; 32],
            };
            built.clear();
            pending_text = None;
            if let Some((join, offset)) = selected_text.take()
                && join == address
            {
                registers[0] = Value::MemberText(offset);
            }
        }

        if let Some(select) = member_text_select(&words[index..], address, &registers, &incoming) {
            selected_text = Some(select);
        }

        let word = u32::from_le_bytes(*word);
        falls_through = !matches!(unconditional_branch(word), Some(Transfer::Stop));

        if let Some(target) = forward_branch_target(word, address) {
            let state = entering.entry(target).or_insert(registers);
            *state = agreed(state, &registers);
        }

        if let Some(target) = branch_with_link(word, address) {
            if anchors.file_enumerations.contains(&target) {
                calls.push(EnumerationCall {
                    address,
                    directory: registers[0],
                    suffix: registers[2],
                    substring: registers[3],
                    depth: registers[4],
                });
            }

            // A call may change any object that it can reach, so only the object that a literal
            // `CString` constructor builds stays known.
            built.clear();
            let text = match registers[1] {
                Value::Constant(literal) => Some(Value::Text(literal)),
                Value::Slot(slot) => Some(Value::SlotText(slot)),
                _ => None,
            };
            if anchors.string_constructors.contains(&target)
                && let (Value::Stack(_), Some(text)) = (registers[0], text)
            {
                built.push((registers[0], text));
            }

            registers[..=18].fill(Value::Unknown);
        } else if unconditional_branch(word).is_some() {
            // An indirect call may change any object, as a direct one may.
            built.clear();
            registers[..=18].fill(Value::Unknown);
        } else if let Some(text) =
            built_text_select(&words[index..], address, &registers, &built, &incoming)
        {
            step(&mut registers, word, address);
            pending_text = Some((address + 12, text));
        } else {
            match stored_range(word, &registers) {
                Stored::Nothing => {}
                Stored::Unknown => built.clear(),
                Stored::Stack(start, width) => built.retain(|(object, _)| {
                    let Value::Stack(at) = *object else {
                        return false;
                    };
                    let (object_end, end) =
                        (at.wrapping_add(STRING_SIZE), start.wrapping_add(width));
                    end <= at || object_end <= start
                }),
            }
            step(&mut registers, word, address);
        }

        if let Some((at, text)) = pending_text
            && at == address
        {
            registers[0] = text;
            pending_text = None;
        }
    }
    calls
}

/// The values that two paths into one block both hold.
fn agreed(first: &[Value; 32], second: &[Value; 32]) -> [Value; 32] {
    std::array::from_fn(|register| match first[register] == second[register] {
        true => first[register],
        false => Value::Unknown,
    })
}

/// The target of a conditional branch or `b` word at `address` that lies after it.
fn forward_branch_target(word: u32, address: u64) -> Option<u64> {
    let jump = (word & 0xfc00_0000 == 0x1400_0000)
        .then(|| call_or_jump_target(word, address))
        .flatten();

    conditional_branch_target(word, address)
        .or(jump)
        .filter(|target| *target > address)
}

/// Whether `words` hold a register jump (`br`), whose targets the scan does not know.
fn jumps_through_register(words: &[[u8; 4]]) -> bool {
    words
        .iter()
        .any(|word| u32::from_le_bytes(*word) & 0xffff_fc1f == 0xd61f_0000)
}

/// The size of a `CString`: its text pointer or inline text, and the flag byte at `+0x17`.
const STRING_SIZE: u64 = 0x18;

/// Where one word writes memory.
enum Stored {
    /// The word writes no memory.
    Nothing,
    /// The word writes memory at an address that the scan does not know.
    Unknown,
    /// The word writes this many bytes from this stack address.
    Stack(u64, u64),
}

/// Where `word` writes memory, for a store of general registers with an immediate offset from a
/// stack address. Any other word of the load and store class that is not a load is an unknown
/// write.
fn stored_range(word: u32, registers: &[Value; 32]) -> Stored {
    if !writes_memory(word) {
        return Stored::Nothing;
    }

    let base = (word >> 5 & 31) as usize;
    let size = u64::from(word >> 30);
    let extent = if word & 0x3fc0_0000 == 0x3900_0000 {
        // `str` with an unsigned scaled offset.
        Some((i64::from(word >> 10 & 0xfff) << size, 1 << size))
    } else if word & 0x3fe0_0000 == 0x3800_0000 {
        // `stur` and the pre- and post-indexed `str`, with a signed 9-bit offset.
        let offset = i64::from(((word >> 12 & 0x1ff) as i32) << 23 >> 23);
        let post_indexed = word & 0xc00 == 0x400;
        Some((if post_indexed { 0 } else { offset }, 1 << size))
    } else if word & 0x3e40_0000 == 0x2800_0000 {
        // `stp` of two 32-bit or 64-bit registers.
        let scale = if word >> 31 == 1 { 8 } else { 4 };
        let offset = i64::from(((word >> 15 & 0x7f) as i32) << 25 >> 25) * scale;
        let post_indexed = word >> 23 & 3 == 1;
        Some((if post_indexed { 0 } else { offset }, 2 * scale as u64))
    } else {
        None
    };
    let address = match (base, registers[base]) {
        (31, _) => Some(0),
        (_, Value::Stack(at)) => Some(at),
        _ => None,
    };

    match (extent, address) {
        (Some((offset, width)), Some(at)) => Stored::Stack(at.wrapping_add_signed(offset), width),
        _ => Stored::Unknown,
    }
}

/// Whether `word` is a store: a load and store class word that is not a load. A register word is
/// a store when its `opc` is `00` (or `10` for a vector register); a pair or exclusive word when
/// its `L` bit is clear. Any other word of the class, such as an atomic, counts as a store.
fn writes_memory(word: u32) -> bool {
    if word & 0x0a00_0000 != 0x0800_0000 {
        return false;
    }

    let opc = word >> 22 & 3;
    let vector = word & 0x0400_0000 != 0;
    let register = word & 0x3a00_0000 == 0x3800_0000;
    let pair = word & 0x3a00_0000 == 0x2800_0000;
    let exclusive = word & 0x3f00_0000 == 0x0800_0000;
    let literal = word & 0x3b00_0000 == 0x1800_0000;

    match () {
        _ if literal => false,
        _ if register => opc == 0 || (vector && opc == 2),
        _ if pair || exclusive => word & 0x0040_0000 == 0,
        _ => true,
    }
}

/// The addresses in `words`, which start at `address`, where a block starts with no known value:
/// each target of a backward branch.
fn unmerged_starts(words: &[[u8; 4]], address: u64) -> BTreeSet<u64> {
    words
        .iter()
        .enumerate()
        .filter_map(|(index, word)| {
            let at = address + index as u64 * 4;
            let word = u32::from_le_bytes(*word);
            let jump = (word & 0xfc00_0000 == 0x1400_0000)
                .then(|| call_or_jump_target(word, at))
                .flatten();

            conditional_branch_target(word, at)
                .or(jump)
                .filter(|target| *target <= at)
        })
        .collect()
}

/// The text of the `CString` in `built` that the short-string select that starts `words`, at
/// `address`, leaves in `x0`.
///
/// The select is `ldrsb wF, [flag]; ldr xP, [pointer]; cmp wF, #0; csel x0, xP, xS, lt`, with
/// unscaled (`ldursb`, `ldur`) or scaled offsets, where `pointer` and `xS` are the address of
/// the `CString` and `flag` its byte at `+0x17`: a long text is behind the pointer, a short one
/// inline. No branch may enter the select after its first word.
fn built_text_select(
    words: &[[u8; 4]],
    address: u64,
    registers: &[Value; 32],
    built: &[(Value, Value)],
    incoming: &BTreeMap<u64, usize>,
) -> Option<Value> {
    let [load_flag, load_pointer, compare, select, ..] = words else {
        return None;
    };
    let (flag, flag_base, flag_offset) = signed_byte_load(u32::from_le_bytes(*load_flag))?;
    let (pointer, pointer_base, pointer_offset) = pointer_load(u32::from_le_bytes(*load_pointer))?;
    let compares_flag = u32::from_le_bytes(*compare) == 0x7100_001f | (flag as u32) << 5;
    let (destination, taken, other, condition) = conditional_select(u32::from_le_bytes(*select))?;
    let location = |base: usize, offset: i64| match (base, registers[base]) {
        (31, _) => Some(Value::Stack(offset as u64)),
        (_, Value::Stack(at)) => Some(Value::Stack(at.wrapping_add_signed(offset))),
        _ => None,
    };
    let object = location(pointer_base, pointer_offset)?;
    let Value::Stack(at) = object else {
        return None;
    };
    // Neither load may overwrite a register that a later word of the select reads.
    let registers_distinct =
        flag != pointer && flag != pointer_base && other != flag && other != pointer;
    let shape = compares_flag
        && registers_distinct
        && destination == 0
        && taken == pointer
        && condition == LESS_THAN
        && registers[other] == object
        && location(flag_base, flag_offset) == Some(Value::Stack(at.wrapping_add(0x17)));
    let entered_only_at_its_start =
        (1..4).all(|word| !incoming.contains_key(&(address + word * 4)));
    let (_, text) = built.iter().find(|(location, _)| *location == object)?;

    (shape && entered_only_at_its_start).then_some(*text)
}

/// The condition code `lt` of a conditional select.
const LESS_THAN: u32 = 0b1011;

/// The destination, base register and offset of a 32-bit `ldrsb` with a scaled or an unscaled
/// immediate offset.
fn signed_byte_load(word: u32) -> Option<(usize, usize, i64)> {
    if let Some((destination, base, offset)) = ldrsb_immediate(word) {
        return Some((destination, base, offset as i64));
    }

    (word & 0xffe0_0c00 == 0x38c0_0000).then(|| unscaled(word))
}

/// The destination, base register and offset of a 64-bit `ldr` with a scaled or an unscaled
/// immediate offset.
fn pointer_load(word: u32) -> Option<(usize, usize, i64)> {
    if let Some((destination, base, offset)) = ldr_immediate(word) {
        return Some((destination, base, offset as i64));
    }

    (word & 0xffe0_0c00 == 0xf840_0000).then(|| unscaled(word))
}

/// The destination, base register and signed 9-bit offset of an unscaled load.
fn unscaled(word: u32) -> (usize, usize, i64) {
    let offset = i64::from(((word >> 12 & 0x1ff) as i32) << 23 >> 23);

    ((word & 31) as usize, (word >> 5 & 31) as usize, offset)
}

/// The destination, the register taken when the condition holds, the other register and the
/// condition of a 64-bit `csel`.
fn conditional_select(word: u32) -> Option<(usize, usize, usize, u32)> {
    (word & 0xffe0_0c00 == 0x9a80_0000).then_some((
        (word & 31) as usize,
        (word >> 5 & 31) as usize,
        (word >> 16 & 31) as usize,
        word >> 12 & 15,
    ))
}

/// The addresses in `words`, which start at `address`, where a basic block starts: each target of
/// a branch, and each word after a branch that does not return.
fn block_starts(words: &[[u8; 4]], address: u64) -> BTreeSet<u64> {
    let mut starts = join_points(words, address);
    for (index, word) in words.iter().enumerate() {
        let word = u32::from_le_bytes(*word);
        let at = address + index as u64 * 4;
        if let Some(Transfer::Stop) = unconditional_branch(word) {
            starts.insert(at + 4);
            starts.extend(call_or_jump_target(word, at));
        }
    }
    starts
}

/// The number of branches in `words`, which start at `address`, that reach each address: each
/// conditional branch and each `b`.
fn incoming_branches(words: &[[u8; 4]], address: u64) -> BTreeMap<u64, usize> {
    let mut incoming = BTreeMap::new();
    for (index, word) in words.iter().enumerate() {
        let word = u32::from_le_bytes(*word);
        let at = address + index as u64 * 4;
        let jump = (word & 0xfc00_0000 == 0x1400_0000)
            .then(|| call_or_jump_target(word, at))
            .flatten();
        if let Some(target) = conditional_branch_target(word, at).or(jump) {
            *incoming.entry(target).or_insert(0) += 1;
        }
    }

    incoming
}

/// The join and the member offset of the short-string select that starts `words`, at `address`,
/// when `x0` leaves it as the text of the instance's `CString` member.
///
/// A `CString` keeps a short text inline and a long one behind a pointer, and the sign bit of its
/// byte at `+0x17` says which. The select is `ldrsb wN, [xA, #a]; tbz wN, #31, join;
/// ldr x0, [xB, #b]`, where `xA + a - 0x17` and `xB + b` are the same member of the instance, and
/// `x0` already holds that member's address: on each path `x0` then points at its text. No other
/// branch may enter the select or its join, so no other path brings another `x0` there.
fn member_text_select(
    words: &[[u8; 4]],
    address: u64,
    registers: &[Value; 32],
    incoming: &BTreeMap<u64, usize>,
) -> Option<(u64, u64)> {
    let [load_flag, test, load_pointer, ..] = words else {
        return None;
    };
    let (flag, flag_base, flag_offset) = ldrsb_immediate(u32::from_le_bytes(*load_flag))?;
    let test = u32::from_le_bytes(*test);
    let join = address + 12;
    let tests_sign = test & 0xff00_0000 == 0x3600_0000
        && (test >> 19 & 31) as usize == 31
        && (test & 31) as usize == flag
        && conditional_branch_target(test, address + 4) == Some(join);
    let (destination, pointer_base, pointer_offset) =
        ldr_immediate(u32::from_le_bytes(*load_pointer))?;
    let instance_offset = |register: usize, offset: u64| match registers[register] {
        Value::Object(base) => base.checked_add(offset),
        _ => None,
    };
    let member = instance_offset(pointer_base, pointer_offset)?;
    let shape = tests_sign
        && destination == 0
        && instance_offset(flag_base, flag_offset) == member.checked_add(0x17)
        && registers[0] == Value::Object(member);
    let entered_only_by_its_test = incoming.get(&join) == Some(&1)
        && !incoming.contains_key(&(address + 4))
        && !incoming.contains_key(&(address + 8));

    (shape && entered_only_by_its_test).then_some((join, member))
}

/// The destination register, base register and offset of a 32-bit `ldrsb` with an unsigned
/// immediate offset, such as `ldrsb w8, [x0, #0x17]`.
fn ldrsb_immediate(word: u32) -> Option<(usize, usize, u64)> {
    (word & 0xffc0_0000 == 0x39c0_0000).then(|| {
        (
            (word & 31) as usize,
            (word >> 5 & 31) as usize,
            u64::from(word >> 10 & 0xfff),
        )
    })
}

/// Follow one word that is neither a call nor an unconditional branch. An `adrp`, an `add` or
/// `sub` of an immediate, a register move, an immediate move and a load from a known address give
/// the value that they write; any other word makes every register that it may write unknown.
fn step(registers: &mut [Value; 32], word: u32, address: u64) {
    if let Some((destination, page)) = adrp(word, address) {
        registers[destination] = Value::Constant(page);
    } else if let Some((destination, source, offset, sign)) = add_immediate(word)
        .map(|(destination, source, addend)| (destination, source, addend, 1))
        .or_else(|| {
            sub_immediate(word)
                .map(|(destination, source, subtrahend)| (destination, source, subtrahend, -1))
        })
    {
        // Register 31 is the stack pointer: a source gives a stack address, and a write to it
        // is not followed.
        let offset = (offset as i64 * sign) as u64;
        let value = match (source, registers[source]) {
            (31, _) => Value::Stack(offset),
            (_, Value::Constant(base)) => Value::Constant(base.wrapping_add(offset)),
            (_, Value::Stack(base)) => Value::Stack(base.wrapping_add(offset)),
            (_, Value::Object(base)) => Value::Object(base.wrapping_add(offset)),
            _ => Value::Unknown,
        };
        if destination != 31 {
            registers[destination] = value;
        }
    } else if let Some((destination, source)) = move_register(word) {
        // In a register move, register 31 is the zero register.
        registers[destination] = match source {
            31 => Value::Constant(0),
            _ => registers[source],
        };
    } else if let Some((destination, value)) = move_wide_immediate(word) {
        registers[destination] = Value::Constant(value);
    } else if let Some((destination, base, offset)) =
        // A load into register 31 discards its value.
        ldr_immediate(word).filter(|(destination, ..)| *destination != 31)
    {
        // Base register 31 is the stack pointer, not the zero register.
        registers[destination] = match (base, registers[base]) {
            (31, _) => Value::Unknown,
            (_, Value::Constant(address)) => Value::Slot(address.wrapping_add(offset)),
            _ => Value::Unknown,
        };
    } else {
        // Any other instruction may write its low register field, and a load or store with
        // writeback also writes its base register.
        if let Some(base) = writeback_base(word) {
            registers[base] = Value::Unknown;
        }
        for register in second_written(word) {
            registers[register] = Value::Unknown;
        }
        registers[(word & 31) as usize] = Value::Unknown;
    }
}

/// The destination and source registers of a 64-bit register move, `mov xd, xm`, which is
/// `orr xd, xzr, xm`.
fn move_register(word: u32) -> Option<(usize, usize)> {
    (word & 0xffe0_ffe0 == 0xaa00_03e0)
        .then_some(((word & 31) as usize, (word >> 16 & 31) as usize))
}

/// The destination and value of a `movz` or `movn` word, the immediate forms of `mov`. A 32-bit
/// form gives the 32-bit value. A write to the zero register is not a move.
fn move_wide_immediate(word: u32) -> Option<(usize, u64)> {
    let destination = (word & 31) as usize;
    if destination == 31 {
        return None;
    }

    let inverted = match word & 0x7f80_0000 {
        0x5280_0000 => false,
        0x1280_0000 => true,
        _ => return None,
    };
    let wide = word >> 31 == 1;
    let shifted = u64::from(word >> 5 & 0xffff) << (16 * (word >> 21 & 3));
    let value = if inverted { !shifted } else { shifted };
    let value = if wide { value } else { value & 0xffff_ffff };

    Some((destination, value))
}

/// The target of a `bl` word at `address`.
fn branch_with_link(word: u32, address: u64) -> Option<u64> {
    if word & 0xfc00_0000 != 0x9400_0000 {
        return None;
    }
    let offset = ((word & 0x03ff_ffff) as i64) << 38 >> 36;
    Some((address as i64).wrapping_add(offset) as u64)
}

/// The addresses in `words`, which start at `address`, that a conditional branch of the same
/// function can reach.
fn join_points(words: &[[u8; 4]], address: u64) -> BTreeSet<u64> {
    words
        .iter()
        .enumerate()
        .filter_map(|(index, word)| {
            let at = address + index as u64 * 4;
            conditional_branch_target(u32::from_le_bytes(*word), at)
        })
        .collect()
}

/// Whether any `bl` in `words`, which start at `address`, calls one of `targets`.
fn calls_any(words: &[[u8; 4]], address: u64, targets: &BTreeSet<u64>) -> bool {
    words.iter().enumerate().any(|(index, word)| {
        let address = address + index as u64 * 4;
        branch_with_link(u32::from_le_bytes(*word), address)
            .is_some_and(|target| targets.contains(&target))
    })
}

/// What an unconditional branch does to the scan.
enum Transfer {
    /// `ret`, `b`, `br` or another return or register branch without link: the scan ends, but
    /// code after it may still run.
    Stop,
    /// `blr` or an authenticated form: the scan continues after an unknown call.
    IndirectCall,
}

/// The transfer of an unconditional immediate or register branch word.
fn unconditional_branch(word: u32) -> Option<Transfer> {
    if word & 0xfc00_0000 == 0x1400_0000 {
        return Some(Transfer::Stop);
    }
    if word & 0xfe00_0000 != 0xd600_0000 {
        return None;
    }

    // Bits 21 to 23 of the register-branch opcode are 001 for `blr`, with its authenticated
    // forms.
    Some(match word >> 21 & 0b111 {
        0b001 => Transfer::IndirectCall,
        _ => Transfer::Stop,
    })
}

/// The base register of a pre- or post-indexed load or store of one register or a pair, which
/// writes the address back to its base.
fn writeback_base(word: u32) -> Option<usize> {
    let pair_writeback = word & 0x3a00_0000 == 0x2800_0000 && word & 0x0080_0000 != 0;
    let single_writeback = word & 0x3b20_0400 == 0x3800_0400;
    (pair_writeback || single_writeback).then_some((word >> 5 & 31) as usize)
}

/// The registers other than the low register field that a load or store writes: the second
/// register of a load pair, and the status register of an exclusive store or the compare
/// registers of a compare-and-swap, which use the same fields. Clearing a field that an
/// instruction of the exclusive class only reads is conservative.
fn second_written(word: u32) -> Vec<usize> {
    let pair_load = word & 0x3c40_0000 == 0x2840_0000;
    let exclusive = word & 0x3f00_0000 == 0x0800_0000;
    let second = (word >> 10 & 31) as usize;
    let status = (word >> 16 & 31) as usize;
    match (pair_load, exclusive) {
        (true, _) => vec![second],
        // A compare-and-swap pair also writes the register after its first compare register.
        (_, true) => vec![second, status, (status + 1).min(31)],
        _ => Vec::new(),
    }
}

/// The literal as a directory path without trailing `/`, when that path is nonempty.
fn directory_path(strings: &BTreeMap<u64, String>, literal: u64) -> Option<String> {
    let path = strings.get(&literal)?.trim_end_matches('/');
    (!path.is_empty()).then(|| path.to_owned())
}

/// The argument of every base-constructor call in one constructor body.
pub fn arguments(
    constructor: &Constructor,
    anchors: &Anchors,
    strings: &BTreeMap<u64, String>,
) -> Vec<Argument> {
    let scan = scan(constructor, anchors, Instance::Unknown);
    // Arguments read before a stop do not prove the directory when the code after it, which the
    // scan did not follow, calls the base constructor again.
    let unread = scan.unread_base_call.then_some(Argument::Unknown);
    scan.passed
        .into_iter()
        .map(|passed| match passed {
            Passed::Built(literal) => {
                directory_path(strings, literal).map_or(Argument::Unknown, Argument::Literal)
            }
            Passed::Other(Value::Constant(global)) => Argument::Global(global),
            Passed::Other(_) => Argument::Unknown,
        })
        .chain(unread)
        .collect()
}

/// Global `CString` objects that the static initializers build from text literals. A global that
/// is built more than once with different text is left out.
pub fn globals(
    initializers: &[Constructor],
    anchors: &Anchors,
    strings: &BTreeMap<u64, String>,
) -> BTreeMap<u64, String> {
    let mut found: BTreeMap<u64, BTreeSet<String>> = BTreeMap::new();
    for initializer in initializers {
        for built in scan(initializer, anchors, Instance::Unknown).built {
            if let (Value::Constant(global), Some(path)) =
                (built.destination, directory_path(strings, built.literal))
            {
                found.entry(global).or_default().insert(path);
            }
        }
    }
    found
        .into_iter()
        .filter(|(_, texts)| texts.len() == 1)
        .map(|(global, mut texts)| (global, texts.pop_first().expect("one text")))
        .collect()
}

/// The text of each `CString` member that the database instance's `constructors` and `creators`
/// build from a literal, by member offset. A constructor receives the instance in `x0`; a
/// creator, `CreateInstance()`, allocates it. A member built with several texts, and a creator
/// that allocates more than once, give nothing.
pub fn member_texts(
    constructors: &[Constructor],
    creators: &[Constructor],
    anchors: &Anchors,
    strings: &BTreeMap<u64, String>,
) -> BTreeMap<u64, String> {
    let allocates_once =
        |creator: &&Constructor| allocation_count(creator, &anchors.allocations) == 1;
    let built = constructors
        .iter()
        .flat_map(|constructor| scan(constructor, anchors, Instance::Entry).built)
        .chain(
            creators
                .iter()
                .filter(allocates_once)
                .flat_map(|creator| scan(creator, anchors, Instance::Allocated).built),
        );
    let mut found: BTreeMap<u64, BTreeSet<String>> = BTreeMap::new();
    for built in built {
        if let (Value::Object(offset), Some(path)) =
            (built.destination, directory_path(strings, built.literal))
        {
            found.entry(offset).or_default().insert(path);
        }
    }

    found
        .into_iter()
        .filter(|(_, texts)| texts.len() == 1)
        .map(|(offset, mut texts)| (offset, texts.pop_first().expect("one text")))
        .collect()
}

/// The number of `bl` words in `function` that call one of `allocations`.
fn allocation_count(function: &Constructor, allocations: &BTreeSet<u64>) -> usize {
    let words = function.code.as_chunks::<4>().0;

    words
        .iter()
        .enumerate()
        .filter(|(index, word)| {
            let address = function.address + *index as u64 * 4;
            branch_with_link(u32::from_le_bytes(**word), address)
                .is_some_and(|target| allocations.contains(&target))
        })
        .count()
}

/// The directory that a custom loader enumerates: the literal first argument of every
/// file-enumeration call in `functions`, the database's own functions, or the text of the
/// instance's member in `members` that it passes. `pointers` holds the rebase target of each
/// pointer slot.
pub fn loader_directory(
    functions: &[Constructor],
    members: &BTreeMap<u64, String>,
    anchors: &Anchors,
    strings: &BTreeMap<u64, String>,
    pointers: &BTreeMap<u64, u64>,
) -> Directory {
    let arguments: Vec<Argument> =
        enumerated_directories(functions, members, anchors, strings, pointers)
            .into_iter()
            .map(|(_, directory)| directory.map_or(Argument::Unknown, Argument::Literal))
            .collect();

    directory(&arguments, &BTreeMap::new())
}

/// The address of every file-enumeration call in `functions`, with the directory that its call
/// block passes, or `None` when the block does not establish one: a literal, a literal that a
/// pointer slot holds, or the text of an instance member in `members`. `pointers` holds the
/// rebase target of each pointer slot.
pub fn enumerated_directories(
    functions: &[Constructor],
    members: &BTreeMap<u64, String>,
    anchors: &Anchors,
    strings: &BTreeMap<u64, String>,
    pointers: &BTreeMap<u64, u64>,
) -> Vec<(u64, Option<String>)> {
    functions
        .iter()
        .flat_map(|function| enumeration_calls(function, anchors))
        .map(|call| {
            let directory = match call.directory {
                Value::Constant(literal) => directory_path(strings, literal),
                Value::Slot(slot) => pointers
                    .get(&slot)
                    .and_then(|literal| directory_path(strings, *literal)),
                Value::MemberText(offset) => members.get(&offset).cloned(),
                Value::Text(literal) => directory_path(strings, literal),
                Value::SlotText(slot) => pointers
                    .get(&slot)
                    .and_then(|literal| directory_path(strings, *literal)),
                Value::Unknown | Value::Stack(_) | Value::Object(_) => None,
            };

            (call.address, directory)
        })
        .collect()
}

/// Resolve the directory from the arguments of every constructor body of one database class.
pub fn directory(arguments: &[Argument], globals: &BTreeMap<u64, String>) -> Directory {
    let mut found = BTreeSet::new();
    for argument in arguments {
        match argument {
            Argument::Literal(text) => found.insert(text.clone()),
            Argument::Global(global) => match globals.get(global) {
                Some(text) => found.insert(text.clone()),
                None => return Directory::Missing,
            },
            Argument::Unknown => return Directory::Missing,
        };
    }
    let mut found: Vec<_> = found.into_iter().collect();
    match found.len() {
        0 => Directory::Missing,
        1 => Directory::Named(found.remove(0)),
        _ => Directory::Ambiguous(found),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::assembler::{Arm64, arm64};

    const BASE: u64 = 0x1100;
    const STRING: u64 = 0x1200;
    const ENUMERATE: u64 = 0x1300;
    const ALLOCATE: u64 = 0x1500;
    /// A function that is none of the anchors.
    const HELPER: u64 = 0x1400;
    /// The literals that `strings` holds.
    const EXAMPLES: u64 = 0x2010;
    const OTHER: u64 = 0x2020;
    /// A global `CString` that an initializer builds.
    const GLOBAL: u64 = 0x2828;

    fn function(code: Vec<u8>) -> Constructor {
        Constructor {
            address: 0x1000,
            code,
        }
    }
    fn anchors() -> Anchors {
        Anchors {
            base_constructors: BTreeSet::from([BASE]),
            string_constructors: BTreeSet::from([STRING]),
            file_enumerations: BTreeSet::from([ENUMERATE]),
            allocations: BTreeSet::from([ALLOCATE]),
        }
    }
    /// A pointer slot that holds `EXAMPLES`, and one that holds an address that is no literal.
    const SLOT: u64 = 0x2830;
    const NOT_LITERAL_SLOT: u64 = 0x2838;
    fn pointers() -> BTreeMap<u64, u64> {
        BTreeMap::from([(SLOT, EXAMPLES), (NOT_LITERAL_SLOT, HELPER)])
    }
    fn strings() -> BTreeMap<u64, String> {
        BTreeMap::from([
            (EXAMPLES, "common/examples".to_owned()),
            (OTHER, "map/other/".to_owned()),
        ])
    }
    /// A custom loader that enumerates the files under `literal`.
    fn loader(literal: u64) -> Constructor {
        let mut code = Arm64::at(0x1000);
        code.address(0, literal).call(ENUMERATE);

        function(code.bytes())
    }
    /// A stack `CString` built from `literal`, appended to `code`.
    fn build_in(code: &mut Arm64, literal: u64) {
        code.address(1, literal);
        arm64!(code; add x0, sp, #8);
        code.call(STRING);
    }
    /// The stack `CString` passed to the base constructor, appended to `code`.
    fn pass_in(code: &mut Arm64) {
        arm64!(code; nop; add x1, sp, #8);
        code.call(BASE);
    }
    /// The ordinary shape, appended to `code`: literal, stack `CString`, base constructor.
    fn temporary_in(code: &mut Arm64, literal: u64) {
        build_in(code, literal);
        pass_in(code);
    }
    fn temporary(literal: u64) -> Constructor {
        let mut code = Arm64::at(0x1000);
        temporary_in(&mut code, literal);

        function(code.bytes())
    }
    fn ret(code: &mut Arm64) {
        arm64!(code; ret);
    }
    fn branch_register(code: &mut Arm64) {
        arm64!(code; br x8);
    }
    fn jump(code: &mut Arm64) {
        arm64!(code; b extern 0x1100);
    }
    const STOPS: [fn(&mut Arm64); 3] = [ret, branch_register, jump];
    /// Conditional branches to the label `skip`, which the code defines further on.
    const SKIPS: [fn(&mut Arm64); 3] = [
        |code| arm64!(code; b.eq >skip),
        |code| arm64!(code; cbz w0, >skip),
        |code| arm64!(code; tbz w0, #3, >skip),
    ];
    /// A complete `temporary` of `EXAMPLES`, then the stop `end`, then `rest`.
    fn stopped_after_temporary(end: fn(&mut Arm64), rest: fn(&mut Arm64)) -> Constructor {
        let mut code = Arm64::at(0x1000);
        temporary_in(&mut code, EXAMPLES);
        end(&mut code);
        rest(&mut code);

        function(code.bytes())
    }
    fn resolve(constructors: &[Constructor], globals: &BTreeMap<u64, String>) -> Directory {
        let arguments: Vec<_> = constructors
            .iter()
            .flat_map(|c| arguments(c, &anchors(), &strings()))
            .collect();
        directory(&arguments, globals)
    }

    #[test]
    fn a_custom_loader_is_named_by_the_one_literal_it_enumerates() {
        let named = loader_directory(
            &[loader(EXAMPLES), loader(EXAMPLES)],
            &BTreeMap::new(),
            &anchors(),
            &strings(),
            &pointers(),
        );
        assert_eq!(named, Directory::Named("common/examples".into()));

        let several = loader_directory(
            &[loader(EXAMPLES), loader(OTHER)],
            &BTreeMap::new(),
            &anchors(),
            &strings(),
            &pointers(),
        );
        assert_eq!(
            several,
            Directory::Ambiguous(vec!["common/examples".into(), "map/other".into()])
        );

        let unknown = function(arm64!(at 0x1000; add x0, sp, #8; bl extern ENUMERATE as usize));
        assert_eq!(
            loader_directory(
                &[loader(EXAMPLES), unknown],
                &BTreeMap::new(),
                &anchors(),
                &strings(),
                &pointers()
            ),
            Directory::Missing
        );
        assert_eq!(
            loader_directory(
                &[temporary(EXAMPLES)],
                &BTreeMap::new(),
                &anchors(),
                &strings(),
                &pointers()
            ),
            Directory::Missing
        );
    }

    /// A custom loader that enumerates the text that the pointer slot at `slot` holds.
    fn slot_loader(slot: u64) -> Constructor {
        let mut code = Arm64::at(0x1000);
        code.load(0, slot).call(ENUMERATE);

        function(code.bytes())
    }

    /// A `CreateInstance()` that allocates the instance and builds its `CString` member at
    /// `offset` from `literal`.
    fn creator(offset: u32, literal: u64) -> Constructor {
        let mut code = Arm64::at(0x1000);
        code.call(ALLOCATE);
        arm64!(code; mov x19, x0; add x0, x19, #offset);
        code.address(1, literal).call(STRING);

        function(code.bytes())
    }
    /// A member function that enumerates the text of its instance's `CString` member at
    /// `offset`, through the short-string select, after `before`.
    fn member_loader(offset: u32, before: fn(&mut Arm64)) -> Constructor {
        let flag = offset + 0x17;
        let mut code = Arm64::at(0x1000);
        arm64!(code; mov x20, x0; add x0, x20, #offset);
        before(&mut code);
        arm64!(code
            ; ldrsb w8, [x20, #flag]
            ; tbz w8, #31, >join
            ; ldr x0, [x20, #offset]
            ; join:
        );
        code.call(ENUMERATE);

        function(code.bytes())
    }
    #[test]
    fn the_text_of_an_instance_member_names_the_loader() {
        let members = member_texts(&[], &[creator(0x28, EXAMPLES)], &anchors(), &strings());
        assert_eq!(members, BTreeMap::from([(0x28, "common/examples".into())]));
        assert_eq!(
            loader_directory(
                &[member_loader(0x28, |_| {})],
                &members,
                &anchors(),
                &strings(),
                &pointers()
            ),
            Directory::Named("common/examples".into())
        );

        let mut constructor = Arm64::at(0x1000);
        constructor.address(1, OTHER).call(STRING);
        let constructed = member_texts(
            &[function(constructor.bytes())],
            &[],
            &anchors(),
            &strings(),
        );
        assert_eq!(constructed, BTreeMap::from([(0, "map/other".into())]));
    }

    /// A loader that builds a stack `CString` at `x29 - 0x30` from the text that `text` puts in
    /// `x1`, after `before`, and enumerates it through the short-string select.
    fn built_text_loader(
        text: fn(&mut Arm64),
        before: fn(&mut Arm64),
        select: fn(&mut Arm64),
    ) -> Constructor {
        let mut code = Arm64::at(0x1000);
        arm64!(code; add x29, sp, #0x40);
        before(&mut code);
        text(&mut code);
        arm64!(code; sub x19, x29, #0x30; sub x0, x29, #0x30);
        code.call(STRING);
        select(&mut code);
        code.call(ENUMERATE);

        function(code.bytes())
    }
    fn unscaled_select(code: &mut Arm64) {
        arm64!(code
            ; ldursb w8, [x29, #-0x19] // the CString's flag byte at +0x17
            ; ldur x9, [x29, #-0x30] // its text pointer
            ; cmp w8, #0
            ; csel x0, x9, x19, lt
        );
    }

    #[test]
    fn the_text_of_a_stack_string_that_the_loader_builds_names_it() {
        let named = |loader: Constructor| {
            loader_directory(
                &[loader],
                &BTreeMap::new(),
                &anchors(),
                &strings(),
                &pointers(),
            )
        };
        let named_examples = Directory::Named("common/examples".into());
        let examples = |code: &mut Arm64| {
            code.address(1, EXAMPLES);
        };
        let slot = |code: &mut Arm64| {
            code.load(1, SLOT);
        };

        assert_eq!(
            named(built_text_loader(slot, |_| {}, unscaled_select)),
            named_examples
        );

        assert_eq!(
            named(built_text_loader(examples, |_| {}, unscaled_select)),
            named_examples
        );
        // A forward branch that skips a store keeps the frame address that both paths hold.
        let skip = |code: &mut Arm64| arm64!(code; cbz w0, >skip; strb w1, [x2]; skip:);
        assert_eq!(
            named(built_text_loader(examples, skip, unscaled_select)),
            named_examples
        );

        let other_frame = |code: &mut Arm64| arm64!(code; cbz w0, >skip; add x29, sp, #0x50; skip:);
        assert_eq!(
            named(built_text_loader(examples, other_frame, unscaled_select)),
            Directory::Missing
        );
        let loop_back = |code: &mut Arm64| arm64!(code; back: ; cbz w0, <back);
        assert_eq!(
            named(built_text_loader(examples, loop_back, unscaled_select)),
            Directory::Missing
        );
        let other_string = |code: &mut Arm64| {
            arm64!(code
                ; ldursb w8, [x29, #-0x19] // the CString's flag byte at +0x17
                ; ldur x9, [x29, #-0x30] // its text pointer
                ; cmp w8, #0
                ; csel x0, x9, x20, lt // another register than the CString's address
            )
        };
        assert_eq!(
            named(built_text_loader(examples, |_| {}, other_string)),
            Directory::Missing
        );
    }

    #[test]
    fn a_stack_string_that_may_change_before_its_select_names_nothing() {
        let named = |loader: Constructor| {
            loader_directory(
                &[loader],
                &BTreeMap::new(),
                &anchors(),
                &strings(),
                &pointers(),
            )
        };
        let examples = |code: &mut Arm64| {
            code.address(1, EXAMPLES);
        };
        let overwritten = |code: &mut Arm64| {
            arm64!(code; stur xzr, [x29, #-0x30]); // a store into the CString's text pointer
            unscaled_select(code);
        };
        let indirect_call = |code: &mut Arm64| {
            arm64!(code; blr x10); // an unknown callee that may change the CString
            unscaled_select(code);
        };
        let reused_register = |code: &mut Arm64| {
            arm64!(code
                ; ldursb w8, [x29, #-0x19] // the CString's flag byte at +0x17
                ; ldur x8, [x29, #-0x30] // its text pointer, over the flag
                ; cmp w8, #0
                ; csel x0, x8, x19, lt
            );
        };
        let register_jump = |code: &mut Arm64| {
            arm64!(code; cbz w0, >skip; br x10; skip:); // a jump that may enter any word
        };
        let away_store = |code: &mut Arm64| {
            arm64!(code; stur xzr, [x29, #-0x40]); // a store below the CString
            unscaled_select(code);
        };

        assert_eq!(
            named(built_text_loader(examples, |_| {}, away_store)),
            Directory::Named("common/examples".into())
        );
        type Emit = fn(&mut Arm64);
        let nothing: Emit = |_| {};
        let cases: [(Emit, Emit); 4] = [
            (nothing, overwritten),
            (nothing, indirect_call),
            (nothing, reused_register),
            (register_jump, unscaled_select),
        ];
        for (before, select) in cases {
            assert_eq!(
                named(built_text_loader(examples, before, select)),
                Directory::Missing
            );
        }
    }

    #[test]
    fn a_member_text_that_is_not_proven_leaves_the_loader_unnamed() {
        let members = member_texts(&[], &[creator(0x28, EXAMPLES)], &anchors(), &strings());
        let unnamed = |loader: Constructor| {
            loader_directory(&[loader], &members, &anchors(), &strings(), &pointers())
        };

        assert_eq!(
            unnamed(member_loader(0x28, |code| arm64!(code; mov x0, x1))),
            Directory::Missing
        );
        assert_eq!(
            unnamed(member_loader(0x28, |code| arm64!(code; cbz w1, >join))),
            Directory::Missing
        );
        assert_eq!(unnamed(member_loader(0x30, |_| {})), Directory::Missing);

        let twice = member_texts(
            &[],
            &[creator(0x28, EXAMPLES), creator(0x28, OTHER)],
            &anchors(),
            &strings(),
        );
        assert!(twice.is_empty());
        let mut allocates_twice = Arm64::at(0x1000);
        allocates_twice.call(ALLOCATE).call(ALLOCATE);
        arm64!(allocates_twice; add x0, x0, #0x28);
        allocates_twice.address(1, EXAMPLES).call(STRING);
        assert!(
            member_texts(
                &[],
                &[function(allocates_twice.bytes())],
                &anchors(),
                &strings()
            )
            .is_empty()
        );
    }

    #[test]
    fn a_literal_loaded_from_a_pointer_slot_names_the_loader() {
        let examples = Directory::Named("common/examples".into());
        assert_eq!(
            loader_directory(
                &[slot_loader(SLOT)],
                &BTreeMap::new(),
                &anchors(),
                &strings(),
                &pointers()
            ),
            examples
        );
        assert_eq!(
            loader_directory(
                &[slot_loader(SLOT), loader(EXAMPLES)],
                &BTreeMap::new(),
                &anchors(),
                &strings(),
                &pointers()
            ),
            examples
        );
        assert_eq!(
            loader_directory(
                &[slot_loader(SLOT), loader(OTHER)],
                &BTreeMap::new(),
                &anchors(),
                &strings(),
                &pointers()
            ),
            Directory::Ambiguous(vec!["common/examples".into(), "map/other".into()])
        );
    }

    #[test]
    fn a_pointer_slot_without_a_literal_leaves_the_loader_unproven() {
        let unresolved = slot_loader(0x2840);
        let not_literal = slot_loader(NOT_LITERAL_SLOT);
        let mut moved = Arm64::at(0x1000);
        moved.load(0, SLOT);
        arm64!(moved; add x0, x0, #1);
        moved.call(ENUMERATE);
        let mut dereferenced = Arm64::at(0x1000);
        dereferenced.load(0, SLOT);
        arm64!(dereferenced; ldr x0, [x0]);
        dereferenced.call(ENUMERATE);
        let stack = function(arm64!(at 0x1000; ldr x0, [sp, #8]; bl extern ENUMERATE as usize));
        let mut discarded = Arm64::at(0x1000);
        discarded.address(8, SLOT);
        arm64!(discarded; ldr xzr, [x8]; mov x0, xzr);
        discarded.call(ENUMERATE);
        let mut stack_after_zero = Arm64::at(0x1000);
        stack_after_zero.address(8, SLOT);
        arm64!(stack_after_zero; mov xzr, x8; ldr x0, [sp]);
        stack_after_zero.call(ENUMERATE);

        for loader in [
            unresolved,
            not_literal,
            function(moved.bytes()),
            function(dereferenced.bytes()),
            stack,
            function(discarded.bytes()),
            function(stack_after_zero.bytes()),
        ] {
            assert_eq!(
                loader_directory(
                    &[loader],
                    &BTreeMap::new(),
                    &anchors(),
                    &strings(),
                    &pointers()
                ),
                Directory::Missing
            );
        }
    }

    #[test]
    fn a_pointer_slot_passed_to_the_base_constructor_names_nothing() {
        let mut code = Arm64::at(0x1000);
        code.load(1, SLOT).call(BASE);

        assert_eq!(
            resolve(&[function(code.bytes())], &BTreeMap::new()),
            Directory::Missing
        );
    }

    #[test]
    fn a_temporary_built_from_a_literal_names_the_registry() {
        assert_eq!(
            resolve(&[temporary(EXAMPLES)], &BTreeMap::new()),
            Directory::Named("common/examples".into())
        );
        assert_eq!(
            resolve(&[temporary(OTHER)], &BTreeMap::new()),
            Directory::Named("map/other".into())
        );
    }

    #[test]
    fn a_temporary_at_a_frame_address_names_the_registry() {
        let frame_with_string = || {
            let mut code = Arm64::at(0x1000);
            arm64!(code; add x29, sp, #0x40);
            code.address(1, EXAMPLES);
            arm64!(code; sub x0, x29, #0x20);
            code.call(STRING);
            code
        };

        let mut framed = frame_with_string();
        arm64!(framed; sub x1, x29, #0x20);
        framed.call(BASE);
        assert_eq!(
            resolve(&[function(framed.bytes())], &BTreeMap::new()),
            Directory::Named("common/examples".into())
        );

        let mut elsewhere = frame_with_string();
        arm64!(elsewhere; add x1, sp, #8);
        elsewhere.call(BASE);
        assert_eq!(
            resolve(&[function(elsewhere.bytes())], &BTreeMap::new()),
            Directory::Missing
        );
    }

    #[test]
    fn a_literal_that_is_not_passed_to_the_base_constructor_names_nothing() {
        let mut unrelated = Arm64::at(0x1000);
        unrelated.address(1, EXAMPLES);
        arm64!(unrelated; add x0, sp, #8);
        unrelated.call(STRING);

        assert_eq!(
            resolve(&[function(unrelated.bytes())], &BTreeMap::new()),
            Directory::Missing
        );
        assert_eq!(resolve(&[], &BTreeMap::new()), Directory::Missing);
    }

    #[test]
    fn a_global_argument_is_resolved_through_its_static_initializer() {
        let mut initializer = Arm64::at(0x1000);
        initializer.address(19, 0x2800);
        arm64!(initializer; add x22, x19, #0x28);
        initializer.address(1, EXAMPLES);
        arm64!(initializer; mov x0, x22);
        initializer.call(STRING);
        let globals = globals(&[function(initializer.bytes())], &anchors(), &strings());
        assert_eq!(
            globals,
            BTreeMap::from([(GLOBAL, "common/examples".into())])
        );

        let mut passes_global = Arm64::at(0x1000);
        passes_global.address(1, GLOBAL).call(BASE);
        let constructor = function(passes_global.bytes());
        assert_eq!(
            resolve(std::slice::from_ref(&constructor), &globals),
            Directory::Named("common/examples".into())
        );
        assert_eq!(
            resolve(&[constructor], &BTreeMap::new()),
            Directory::Missing
        );
    }

    #[test]
    fn several_directories_are_never_resolved_to_one() {
        assert_eq!(
            resolve(&[temporary(EXAMPLES), temporary(OTHER)], &BTreeMap::new()),
            Directory::Ambiguous(vec!["common/examples".into(), "map/other".into()])
        );
        assert_eq!(
            resolve(
                &[temporary(EXAMPLES), temporary(EXAMPLES)],
                &BTreeMap::new()
            ),
            Directory::Named("common/examples".into())
        );
    }

    #[test]
    fn a_call_or_a_write_between_the_steps_clears_the_tracked_value() {
        // A call between the literal and the CString constructor clears x1.
        let mut call = Arm64::at(0x1000);
        call.address(1, EXAMPLES).call(HELPER);
        arm64!(call; add x0, sp, #8);
        call.call(STRING);
        assert!(
            scan(&function(call.bytes()), &anchors(), Instance::Unknown)
                .built
                .is_empty()
        );

        // mov x1, x2 replaces the stack argument before the base constructor.
        let mut overwritten = Arm64::at(0x1000);
        overwritten.address(1, EXAMPLES);
        arm64!(overwritten; add x0, sp, #8);
        overwritten.call(STRING);
        arm64!(overwritten; add x1, sp, #8; mov x1, x2);
        overwritten.call(BASE);
        assert_eq!(
            resolve(&[function(overwritten.bytes())], &BTreeMap::new()),
            Directory::Missing
        );
    }

    #[test]
    fn a_load_pair_or_an_exclusive_store_clears_the_register_outside_the_low_field() {
        let writes: [fn(&mut Arm64); 4] = [
            |code| arm64!(code; ldp x0, x1, [sp, #16]),
            |code| arm64!(code; ldxp x0, x1, [x2]),
            |code| arm64!(code; stxr w1, x2, [x3]),
            |code| arm64!(code; cas x1, x2, [x3]),
        ];
        for write in writes {
            let mut code = Arm64::at(0x1000);
            code.address(1, EXAMPLES);
            write(&mut code);
            arm64!(code; add x0, sp, #8);
            code.call(STRING);
            assert!(
                scan(&function(code.bytes()), &anchors(), Instance::Unknown)
                    .built
                    .is_empty()
            );
        }

        // A store pair reads both registers, so the literal survives it.
        let mut stored = Arm64::at(0x1000);
        stored.address(1, EXAMPLES);
        arm64!(stored; stp x0, x1, [sp, #16]; add x0, sp, #8);
        stored.call(STRING);
        assert_eq!(
            scan(&function(stored.bytes()), &anchors(), Instance::Unknown)
                .built
                .len(),
            1
        );
    }

    #[test]
    fn an_indirect_call_forgets_every_built_object() {
        let mut code = Arm64::at(0x1000);
        code.address(1, EXAMPLES);
        arm64!(code; add x0, sp, #8);
        code.call(STRING);
        arm64!(code; blr x8; add x1, sp, #8);
        code.call(BASE);

        assert_eq!(
            resolve(&[function(code.bytes())], &BTreeMap::new()),
            Directory::Missing
        );
    }

    #[test]
    fn a_base_constructor_call_after_a_stop_leaves_the_earlier_argument_unproven() {
        let skipped_call: fn(&mut Arm64) = |code| temporary_in(code, OTHER);
        let nothing_more: fn(&mut Arm64) = |code| arm64!(code; nop);

        for stop in STOPS {
            assert_eq!(
                resolve(
                    &[stopped_after_temporary(stop, skipped_call)],
                    &BTreeMap::new()
                ),
                Directory::Missing
            );
            assert_eq!(
                resolve(
                    &[stopped_after_temporary(stop, nothing_more)],
                    &BTreeMap::new()
                ),
                Directory::Named("common/examples".into())
            );
        }
    }

    #[test]
    fn a_body_that_only_calls_the_base_constructor_after_a_stop_leaves_the_class_unnamed() {
        let mut unread = Arm64::at(0x1000);
        ret(&mut unread);
        temporary_in(&mut unread, OTHER);

        assert_eq!(
            resolve(
                &[temporary(EXAMPLES), function(unread.bytes())],
                &BTreeMap::new()
            ),
            Directory::Missing
        );
    }

    #[test]
    fn an_enumeration_call_after_a_stop_is_read_in_its_own_block() {
        for stop in STOPS {
            let mut later_call = Arm64::at(0x1000);
            later_call.address(0, EXAMPLES).call(ENUMERATE);
            stop(&mut later_call);
            later_call.address(0, OTHER).call(ENUMERATE);
            assert_eq!(
                loader_directory(
                    &[function(later_call.bytes())],
                    &BTreeMap::new(),
                    &anchors(),
                    &strings(),
                    &pointers()
                ),
                Directory::Ambiguous(vec!["common/examples".into(), "map/other".into()])
            );

            let mut literal_before_stop = Arm64::at(0x1000);
            literal_before_stop.address(0, EXAMPLES);
            stop(&mut literal_before_stop);
            literal_before_stop.call(ENUMERATE);
            assert_eq!(
                loader_directory(
                    &[function(literal_before_stop.bytes())],
                    &BTreeMap::new(),
                    &anchors(),
                    &strings(),
                    &pointers()
                ),
                Directory::Missing
            );
        }
    }

    #[test]
    fn a_return_or_unconditional_branch_ends_the_scan() {
        for stop in STOPS {
            let mut code = Arm64::at(0x1000);
            stop(&mut code);
            temporary_in(&mut code, EXAMPLES);

            assert_eq!(
                resolve(&[function(code.bytes())], &BTreeMap::new()),
                Directory::Missing
            );
        }
    }

    #[test]
    fn a_conditional_branch_that_skips_the_literal_leaves_the_registry_unnamed() {
        for skip in SKIPS {
            let mut code = Arm64::at(0x1000);
            skip(&mut code);
            build_in(&mut code, EXAMPLES);
            arm64!(code; skip:);
            pass_in(&mut code);

            assert_eq!(
                resolve(&[function(code.bytes())], &BTreeMap::new()),
                Directory::Missing
            );
        }
    }

    #[test]
    fn a_conditional_branch_that_skips_the_enumerated_literal_leaves_the_loader_unproven() {
        for skip in SKIPS {
            let mut code = Arm64::at(0x1000);
            skip(&mut code);
            code.address(0, EXAMPLES);
            arm64!(code; skip:);
            code.call(ENUMERATE);

            assert_eq!(
                loader_directory(
                    &[function(code.bytes())],
                    &BTreeMap::new(),
                    &anchors(),
                    &strings(),
                    &pointers()
                ),
                Directory::Missing
            );
        }
    }

    #[test]
    fn a_conditional_branch_past_the_call_keeps_the_registry_named() {
        for skip in SKIPS {
            let mut code = Arm64::at(0x1000);
            skip(&mut code);
            temporary_in(&mut code, EXAMPLES);
            arm64!(code; skip:; nop);

            assert_eq!(
                resolve(&[function(code.bytes())], &BTreeMap::new()),
                Directory::Named("common/examples".into())
            );
        }
    }

    #[test]
    fn a_loop_before_the_literal_keeps_the_directory_named() {
        let loop_in = |code: &mut Arm64| {
            arm64!(code; head:; sub w8, w8, #1; cbnz w8, <head);
        };

        let mut constructor = Arm64::at(0x1000);
        loop_in(&mut constructor);
        temporary_in(&mut constructor, EXAMPLES);
        assert_eq!(
            resolve(&[function(constructor.bytes())], &BTreeMap::new()),
            Directory::Named("common/examples".into())
        );

        let mut enumerates = Arm64::at(0x1000);
        loop_in(&mut enumerates);
        enumerates.address(0, EXAMPLES).call(ENUMERATE);
        assert_eq!(
            loader_directory(
                &[function(enumerates.bytes())],
                &BTreeMap::new(),
                &anchors(),
                &strings(),
                &pointers()
            ),
            Directory::Named("common/examples".into())
        );
    }
}
