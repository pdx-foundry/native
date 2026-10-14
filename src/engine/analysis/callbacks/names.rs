//! The name pass: what each register and each stack `CString` can hold at each instruction of one
//! function, over every path.
//!
//! This is a forward dataflow over the function's direct control flow. A value is a small set
//! of facts; where paths join, the sets join, and a set that grows past [`VALUE_LIMIT`] becomes
//! unknown. A stack `CString` holds the set of literals that it was built from. Building it again
//! replaces the set; a call that receives it as its object, other than a known string function,
//! makes it unknown; its destructor removes it. Where paths join, a string that one path did not
//! build becomes unknown. So two branches that build different names in one object give both
//! names, and a stack slot that the compiler reuses for another object keeps no stale name.
//!
//! Only these instructions keep facts: `adrp`, `add` and `sub` with an immediate or a register
//! of known constants, `mov` of a register or an immediate, a 64-bit `ldr` from a constant, a
//! loaded global or an address that an argument leads to, with or without pre-index writeback
//! from a general register, and 64-bit stores and loads of stack slots. A `mov`, `add` or `sub`
//! to a 32-bit register keeps only the low half of constants; its other facts become unknown.
//! Any other instruction makes the registers that it may write unknown. A call makes `x0`–`x18`
//! unknown and `x0` the result of that call; the result of a call to an offset getter, whose
//! whole body is `add x0, x0, #k; ret`, is `x0` plus `k`, or, for an owner's getter, the owner's
//! offset `k`. Stack offsets are relative to the stack pointer at entry; the state also knows
//! where the stack pointer is now, until it moves by an amount that is not known.
//!
//! A stack slot keeps the 64-bit value last stored to it on every path. A callee writes only
//! inside the objects that it receives, and an object never starts above its address, so a call
//! that receives a stack address forgets every slot at or above that address. Other memory is
//! not tracked.
//!
//! A load from an argument names the loaded word by the chain of loads that reached it, three
//! loads deep. This is what a virtual call on a block needs: `x0` is a type pointer plus the
//! block's offset, and the called register holds a word loaded through the first word at that
//! address, which [`vtable_slot`] reads as a slot of the object's vtable. Memory
//! that is not tracked can change between two loads, so two loads of one address give the same
//! fact even when a store or a call came between them.
//!
//! A word that points at a buffer of owner pointers ([`Buffer`]), at an argument or in a global
//! instance, loads [`Fact::Elements`]. A move by a multiple of 8, or by a register scaled by 8,
//! stays in the buffer, and a load from it, at such an immediate or at a register index scaled by
//! 8, is an element.
use std::collections::{BTreeMap, BTreeSet};

use crate::engine::analysis::decode::{Instruction, general_register, written_registers};

/// The most facts that one value holds before it becomes unknown.
pub(super) const VALUE_LIMIT: usize = 16;

/// The size of a pointer, which is the stride of a buffer of owner pointers.
const POINTER: i64 = 8;

/// Where a chain of loads starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Root {
    /// Argument register `n` at entry.
    Argument(usize),
    /// What a call to the owner getter at this address returned, less the getter's offset: an
    /// item of the getter's owner.
    Getter(u64),
    /// An element of this buffer of owner pointers.
    Element(Buffer),
}

/// A buffer of owner pointers, by the word that points at it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Buffer {
    /// The word at argument register `n` plus `b` at entry: `Argument(n, b)`.
    Argument(usize, i64),
    /// The word at offset `b` of the object whose address the global at `g` holds, such as a
    /// database's `_pInstance`: `Instance(g, b)`.
    Instance(u64, i64),
}

/// One thing that a register can hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Fact {
    Constant(u64),
    /// The stack pointer at entry, plus this offset.
    Stack(i64),
    /// The address that a root names, plus this offset: `Address(r, k)`.
    Address(Root, i64),
    /// The word at `Address(r, k)`, plus an offset: `Member(r, k, offset)`.
    Member(Root, i64, i64),
    /// The word at `Member(r, k, j)`: `Deref(r, k, j)`.
    Deref(Root, i64, i64),
    /// The word `s` bytes past `Deref(r, k, j)`: `DerefAt(r, k, j, s)`.
    DerefAt(Root, i64, i64, i64),
    /// The address of some word of this buffer of owner pointers.
    Elements(Buffer),
    /// The value stored at this global address.
    Global(u64),
    /// The word at the address held in a global, plus this offset: `Field(g, k)` is the word at
    /// `[g] + k`.
    Field(u64, i64),
    /// The value that the call at this address returned.
    Result(u64),
}

/// A set of facts, or `None` when the value is unknown.
pub(super) type Value = Option<BTreeSet<Fact>>;

/// The string functions that the pass follows.
#[derive(Debug, Clone, Default)]
pub struct StringFunctions {
    /// Build a string from a C string literal.
    pub from_literal: BTreeSet<u64>,
    /// Copy-construct a string from another string.
    pub copy: BTreeSet<u64>,
    /// Destroy a string.
    pub destructors: BTreeSet<u64>,
    /// Size of the engine's string object.
    pub object_size: i64,
}

/// A function whose whole body is `add x0, x0, #k; ret`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OffsetGetter {
    /// Its `k`.
    pub offset: i64,
    /// The registry owner whose member function it is, by every name at its address. With an
    /// owner, a call returns the address at that owner's offset `k`, whatever item `x0` holds; a
    /// path that forms the same address from a type pointer then joins to two facts and names no
    /// block. Without one, a call returns `x0` plus `k`.
    pub owner: Option<String>,
}

/// What the pass knows about one function beyond its instructions.
#[derive(Clone, Copy)]
pub(super) struct Known<'a> {
    pub strings: &'a StringFunctions,
    /// Each offset getter that the function may call, by its address.
    pub offset_getters: &'a BTreeMap<u64, OffsetGetter>,
    /// The words that point at a buffer of owner pointers.
    pub buffers: &'a BTreeSet<Buffer>,
}

impl<'a> Known<'a> {
    /// Only the string functions: no offset getters and no buffers.
    pub(super) fn strings(strings: &'a StringFunctions) -> Self {
        Self {
            strings,
            offset_getters: const { &BTreeMap::new() },
            buffers: const { &BTreeSet::new() },
        }
    }
}

/// Register and stack-string facts before one instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct State {
    registers: [Value; 31],
    /// Literal sets of stack strings by stack offset; `None` for a string whose text is unknown.
    strings: BTreeMap<i64, Value>,
    /// 64-bit values stored to stack slots, by stack offset.
    slots: BTreeMap<i64, Value>,
    /// The stack pointer's offset from its value at entry, when it is known.
    sp: Option<i64>,
}

impl State {
    fn entry() -> Self {
        let mut registers: [Value; 31] = std::array::from_fn(|_| None);
        for (index, register) in registers.iter_mut().enumerate().take(8) {
            *register = single(Fact::Address(Root::Argument(index), 0));
        }
        Self {
            registers,
            strings: BTreeMap::new(),
            slots: BTreeMap::new(),
            sp: Some(0),
        }
    }

    fn unknown() -> Self {
        Self {
            registers: std::array::from_fn(|_| None),
            strings: BTreeMap::new(),
            slots: BTreeMap::new(),
            sp: None,
        }
    }

    /// Independent identities at a local proof boundary; none implies an owner or memory value.
    fn independent_registers() -> Self {
        let mut state = Self::unknown();
        for register in 0..31 {
            state.set(register, single(Fact::Address(Root::Argument(register), 0)));
        }
        state
    }

    /// The facts of general register `index`.
    pub(super) fn register(&self, index: usize) -> &Value {
        &self.registers[index]
    }

    /// The literal addresses that the string object in register `index` can hold. `None` when
    /// the register is not a stack string with known text.
    pub(super) fn string_literals(&self, index: usize) -> Option<BTreeSet<u64>> {
        let facts = self.registers[index].as_ref()?;
        let mut literals = BTreeSet::new();
        for fact in facts {
            let Fact::Stack(offset) = fact else {
                return None;
            };
            for literal in self.strings.get(offset)?.as_ref()? {
                let Fact::Constant(literal) = literal else {
                    return None;
                };
                literals.insert(*literal);
            }
        }
        Some(literals)
    }

    /// Join another path's state into this one. Returns whether this state changed.
    fn join(&mut self, other: &Self) -> bool {
        let before = self.clone();
        for (mine, theirs) in self.registers.iter_mut().zip(&other.registers) {
            *mine = join_values(mine, theirs);
        }
        // A string that one path did not build holds an unknown text: that path may have built
        // it in a way that the pass does not follow.
        let offsets: BTreeSet<i64> = self
            .strings
            .keys()
            .chain(other.strings.keys())
            .copied()
            .collect();
        for offset in offsets {
            let joined = match (self.strings.get(&offset), other.strings.get(&offset)) {
                (Some(mine), Some(theirs)) => join_values(mine, theirs),
                _ => None,
            };
            self.strings.insert(offset, joined);
        }
        // A slot that one path did not store holds an unknown value.
        self.slots = self
            .slots
            .iter()
            .filter_map(|(offset, mine)| {
                let theirs = other.slots.get(offset)?;
                Some((*offset, join_values(mine, theirs)))
            })
            .collect();
        if self.sp != other.sp {
            self.sp = None;
        }
        *self != before
    }

    /// The stack offset in a register that holds exactly one.
    fn stack_offset(&self, index: usize) -> Option<i64> {
        match sole_fact(self.registers.get(index)?)? {
            Fact::Stack(offset) => Some(offset),
            _ => None,
        }
    }

    fn set(&mut self, index: usize, value: Value) {
        if index < 31 {
            self.registers[index] = value;
        }
    }

    fn clear(&mut self, index: usize) {
        self.set(index, None);
    }
}

fn join_values(left: &Value, right: &Value) -> Value {
    let (left, right) = (left.as_ref()?, right.as_ref()?);
    let joined: BTreeSet<_> = left.union(right).copied().collect();
    (joined.len() <= VALUE_LIMIT).then_some(joined)
}

fn single(fact: Fact) -> Value {
    Some(BTreeSet::from([fact]))
}

/// The fact of a known value that holds exactly one.
pub(super) fn sole_fact(value: &Value) -> Option<Fact> {
    let facts = value.as_ref()?;
    if facts.len() != 1 {
        return None;
    }

    facts.first().copied()
}

/// The displacement `s` when `target` is the word `s` bytes past the first word of the object at
/// `object`: a call through `target` with `object` in `x0` calls slot `s` of that object's vtable,
/// when the object has one. The first word of the object at `Address(r, k)` is `Member(r, k, 0)`,
/// so its slot `s` is `Deref(r, k, s)`; the first word of the object at `Member(r, k, j)` is
/// `Deref(r, k, j)`, so its slot `s` is `DerefAt(r, k, j, s)`.
pub(super) fn vtable_slot(object: Fact, target: Fact) -> Option<i64> {
    match (object, target) {
        (Fact::Address(r, k), Fact::Deref(target_r, target_k, slot)) => {
            ((r, k) == (target_r, target_k)).then_some(slot)
        }
        (Fact::Member(r, k, j), Fact::DerefAt(target_r, target_k, target_j, slot)) => {
            ((r, k, j) == (target_r, target_k, target_j)).then_some(slot)
        }
        _ => None,
    }
}

/// A 64-bit value that a function stores in the object that `x0` addresses at entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StoredWord {
    /// The value of this argument register at entry.
    Parameter(usize),
    /// A constant, such as a vtable's address point.
    Constant(u64),
    /// The word at the address that the global at this address holds, such as an object pointer
    /// loaded through its GOT slot.
    Loaded(u64),
    /// An element of this buffer of owner pointers.
    Element(Buffer),
    /// A value that the name pass does not name, or part of a word.
    Unknown,
}

/// The values that `rows`, one whole function, stores into the object that `x0` addresses at
/// entry with `str`, `stur` and `stp`, by the offset of each 8-byte word in that object that a
/// store writes. A value that the pass knows to be one of several gives each; a narrower store
/// gives its word [`StoredWord::Unknown`]. A load from a word of `buffers` names an element.
pub(super) fn receiver_stores(
    rows: &[Instruction],
    strings: &StringFunctions,
    buffers: &BTreeSet<Buffer>,
) -> BTreeMap<i64, BTreeSet<StoredWord>> {
    let known = Known {
        buffers,
        ..Known::strings(strings)
    };
    let mut stores: BTreeMap<i64, BTreeSet<StoredWord>> = BTreeMap::new();
    each_state(rows, known, |row, state| {
        if !matches!(row.operation.as_str(), "str" | "stur" | "stp") {
            return;
        }
        let operands = split(&row.operands);
        let Some(position) = operands.iter().position(|operand| operand.starts_with('[')) else {
            return;
        };
        let (sources, memory) = (&operands[..position], operands[position]);
        let Some(address) = stored_address(state, memory) else {
            return;
        };

        let mut offset = address;
        for source in sources {
            let width = register_width(source);
            let stored = match *source {
                "xzr" => BTreeSet::from([StoredWord::Constant(0)]),
                source if source.starts_with('x') => match general_register(source) {
                    Some(register) => stored_words(state.register(register)),
                    None => BTreeSet::from([StoredWord::Unknown]),
                },
                _ => BTreeSet::from([StoredWord::Unknown]),
            };
            for word in (offset..offset + width).step_by(8) {
                stores.entry(word & !7).or_default().extend(&stored);
            }
            offset += width;
        }
    });
    stores
}

/// The bytes that a stored register holds: 16 for a `q` register, 4 for a `w` or `s` register,
/// otherwise 8.
fn register_width(register: &str) -> i64 {
    match register.chars().next() {
        Some('q') => 16,
        Some('w' | 's') => 4,
        _ => 8,
    }
}

/// The stored words that a value may be: [`StoredWord::Unknown`] for a fact, or a value, that the
/// pass does not name.
fn stored_words(value: &Value) -> BTreeSet<StoredWord> {
    let Some(facts) = value else {
        return BTreeSet::from([StoredWord::Unknown]);
    };
    facts
        .iter()
        .map(|fact| stored_word(*fact).unwrap_or(StoredWord::Unknown))
        .collect()
}

/// The stored word that a fact names.
fn stored_word(fact: Fact) -> Option<StoredWord> {
    match fact {
        Fact::Address(Root::Argument(parameter), 0) => Some(StoredWord::Parameter(parameter)),
        Fact::Constant(value) => Some(StoredWord::Constant(value)),
        Fact::Field(global, 0) => Some(StoredWord::Loaded(global)),
        Fact::Address(Root::Element(buffer), 0) => Some(StoredWord::Element(buffer)),
        _ => None,
    }
}

/// The offset in the object that `x0` addresses at entry that a store's memory operand
/// addresses: `[base]`, `[base,#d]`, or `[base,#d]!`. A post-index store writes at its base.
fn stored_address(state: &State, memory: &str) -> Option<i64> {
    let inner = memory.strip_prefix('[')?;
    let inner = inner
        .strip_suffix("]!")
        .or_else(|| inner.strip_suffix(']'))?;
    let mut parts = inner.split(',');
    let base = general_register(parts.next()?)?;
    let displacement = match parts.next() {
        None => 0,
        Some(text) => immediate(text)?,
    };
    match sole_fact(state.register(base))? {
        Fact::Address(Root::Argument(0), offset) => Some(offset + displacement),
        _ => None,
    }
}

/// Run the pass over one function and give `visit` the state before each instruction, once the
/// states are stable.
///
/// A block that no direct branch reaches is a target of a branch through a register, such as a
/// jump table, when the function has one: it starts from the joined states at those branches.
/// Otherwise it is dead code, such as padding, and it starts from unknown registers and passes
/// nothing to the blocks after it.
pub(super) fn each_state(
    rows: &[Instruction],
    known: Known<'_>,
    mut visit: impl FnMut(&Instruction, &State),
) {
    if rows.is_empty() {
        return;
    }

    let blocks = Blocks::new(rows);
    let mut entry: BTreeMap<usize, State> = BTreeMap::from([(0, State::entry())]);
    let mut dead = BTreeMap::new();
    let mut pending = BTreeSet::from([0]);

    loop {
        let mut indirect: Option<State> = None;
        while let Some(block) = pending.pop_first() {
            let mut state = entry[&block].clone();
            for row in &rows[blocks.range(block)] {
                step(row, known, &mut state);
            }
            for successor in blocks.successors(block) {
                match entry.get_mut(&successor) {
                    Some(existing) => {
                        if existing.join(&state) {
                            pending.insert(successor);
                        }
                    }
                    None => {
                        entry.insert(successor, state.clone());
                        pending.insert(successor);
                    }
                }
            }
        }
        for (&block, start) in &entry {
            if blocks.jumps_through_register(rows, block) {
                let mut state = start.clone();
                for row in &rows[blocks.range(block)] {
                    step(row, known, &mut state);
                }
                match &mut indirect {
                    Some(joined) => {
                        joined.join(&state);
                    }
                    None => indirect = Some(state),
                }
            }
        }

        let unreached: Vec<usize> = blocks
            .starts
            .keys()
            .copied()
            .filter(|block| !entry.contains_key(block) && !dead.contains_key(block))
            .collect();
        if unreached.is_empty() {
            break;
        }
        match indirect {
            Some(state) => {
                for block in unreached {
                    entry.insert(block, state.clone());
                    pending.insert(block);
                }
            }
            None => {
                for block in unreached {
                    dead.insert(block, State::unknown());
                }
            }
        }
    }

    for (&block, start) in entry.iter().chain(&dead) {
        let mut state = start.clone();
        for row in &rows[blocks.range(block)] {
            visit(row, &state);
            step(row, known, &mut state);
        }
    }
}

/// Basic blocks of one function, by the position of their first instruction.
pub(super) struct Blocks {
    /// Block start → the position after its last instruction.
    starts: BTreeMap<usize, usize>,
    /// Block start → the starts of the blocks that can run next.
    next: BTreeMap<usize, Vec<usize>>,
}

impl Blocks {
    pub(super) fn new(rows: &[Instruction]) -> Self {
        let index: BTreeMap<u64, usize> = rows
            .iter()
            .enumerate()
            .map(|(position, row)| (row.address, position))
            .collect();
        let mut leaders = BTreeSet::from([0]);
        for (position, row) in rows.iter().enumerate() {
            let Some(successors) = successors(row, &index, position) else {
                continue;
            };
            leaders.extend(successors.into_iter().filter(|&next| next < rows.len()));
            if position + 1 < rows.len() {
                leaders.insert(position + 1);
            }
        }

        let bounds: Vec<usize> = leaders.iter().copied().chain([rows.len()]).collect();
        let mut starts = BTreeMap::new();
        let mut next = BTreeMap::new();
        for pair in bounds.windows(2) {
            let [start, end] = [pair[0], pair[1]];
            let last = end - 1;
            let successors = successors(&rows[last], &index, last)
                .unwrap_or_else(|| vec![last + 1])
                .into_iter()
                .filter(|&position| position < rows.len())
                .collect();
            starts.insert(start, end);
            next.insert(start, successors);
        }
        Self { starts, next }
    }

    pub(super) fn range(&self, block: usize) -> std::ops::Range<usize> {
        block..self.starts[&block]
    }

    pub(super) fn successors(&self, block: usize) -> Vec<usize> {
        self.next[&block].clone()
    }

    fn jumps_through_register(&self, rows: &[Instruction], block: usize) -> bool {
        rows[self.starts[&block] - 1].operation == "br"
    }
}

/// The state at `site` within its straight-line block, with independent symbolic registers at
/// the block's start. Calls and stores break the proof: memory loaded before one need not still
/// be the receiver's vtable afterwards. Narrow writes also break the proof because they
/// truncate pointer identities.
pub(super) fn local_state(rows: &[Instruction], site: u64) -> Option<State> {
    let position = rows.iter().position(|row| row.address == site)?;
    let blocks = Blocks::new(rows);
    let (&start, _) = blocks.starts.range(..=position).next_back()?;
    let mut state = State::independent_registers();
    for row in &rows[start..position] {
        let supported = match row.operation.as_str() {
            "mov" | "add" | "sub" | "ldr" | "ldp" => !row.operands.starts_with('w'),
            "nop" | "cmp" | "cmn" | "tst" => true,
            _ => false,
        };
        if !supported {
            state = State::independent_registers();
            continue;
        }
        step(row, Known::strings(&StringFunctions::default()), &mut state);
    }
    Some(state)
}

/// Positions that can run after the instruction at `position` by direct control flow, when the
/// instruction ends a block; `None` when it does not.
fn successors(
    row: &Instruction,
    index: &BTreeMap<u64, usize>,
    position: usize,
) -> Option<Vec<usize>> {
    let operation = row.operation.as_str();
    let target = row
        .operands
        .rsplit(',')
        .next()
        .and_then(|last| last.strip_prefix("#0x"))
        .and_then(|hex| u64::from_str_radix(hex, 16).ok())
        .and_then(|address| index.get(&address).copied());
    let next = position + 1;
    Some(match operation {
        "b" => target.into_iter().collect(),
        "br" | "ret" | "brk" => Vec::new(),
        "cbz" | "cbnz" | "tbz" | "tbnz" => target.into_iter().chain([next]).collect(),
        branch if branch.starts_with("b.") => target.into_iter().chain([next]).collect(),
        _ => return None,
    })
}

/// Apply one instruction to the state.
fn step(row: &Instruction, known: Known<'_>, state: &mut State) {
    let operands: Vec<&str> = split(&row.operands);
    let operation = row.operation.as_str();

    match (operation, operands.as_slice()) {
        ("adrp" | "adr", [destination, target]) => {
            if let (Some(destination), Some(target)) =
                (general_register(destination), immediate(target))
            {
                state.set(destination, single(Fact::Constant(target as u64)));
            }
        }
        ("add" | "sub", ["sp", source, amount, rest @ ..]) => {
            state.sp = moved_stack(state, source, amount, rest, operation);
        }
        ("add" | "sub", [destination, source, amount, rest @ ..]) => {
            let Some(destination) = general_register(destination) else {
                return;
            };
            let moved = |amounts: Vec<i64>| {
                let mut joined = BTreeSet::new();
                for amount in amounts {
                    let amount = if operation == "sub" { -amount } else { amount };
                    joined.extend(offset(state, source, amount)?);
                }
                (joined.len() <= VALUE_LIMIT).then_some(joined)
            };
            let value = match amounts(state, amount, rest) {
                Some(amounts) => moved(amounts),
                None => moved_in_buffer(state, source, rest),
            };
            state.set(destination, narrowed(value, operands[0]));
        }
        ("mov", ["sp", source]) => state.sp = moved_stack(state, source, "#0", &[], "add"),
        ("mov", [destination, source]) => {
            let Some(destination) = general_register(destination) else {
                return;
            };
            let value = match immediate(source) {
                Some(value) if source.starts_with('#') => single(Fact::Constant(value as u64)),
                _ => offset(state, source, 0),
            };
            state.set(destination, narrowed(value, operands[0]));
        }
        ("ldr", [destination, memory]) if destination.starts_with('x') && memory.ends_with(']') => {
            let Some(destination) = general_register(destination) else {
                return;
            };
            state.set(destination, loaded(state, memory, known.buffers));
        }
        ("ldr", [destination, indexed])
            if destination.starts_with('x')
                && indexed.ends_with("]!")
                && !indexed.starts_with("[sp") =>
        {
            let value = loaded(state, indexed.trim_end_matches('!'), known.buffers);
            let moved = write_back(&operands, 1).and_then(|back| {
                let base = general_register(back.base)?;
                Some((
                    base,
                    back.amount
                        .and_then(|amount| offset(state, back.base, amount)),
                ))
            });
            if let Some(destination) = general_register(destination) {
                state.set(destination, value);
            }
            if let Some((base, value)) = moved {
                state.set(base, value);
            }
        }
        ("ldp", [first, second, memory]) if first.starts_with('x') && memory.ends_with(']') => {
            let slot = stack_address(state, memory);
            for (index, destination) in [first, second].into_iter().enumerate() {
                if let Some(destination) = general_register(destination) {
                    let value = slot
                        .and_then(|slot| state.slots.get(&(slot + 8 * index as i64)).cloned())
                        .flatten();
                    state.set(destination, value);
                }
            }
        }
        ("str" | "stp", [.., memory])
            if memory.ends_with(']')
                && operands[..operands.len() - 1]
                    .iter()
                    .all(|source| source.starts_with('x') || *source == "xzr")
                && stack_address(state, memory).is_some() =>
        {
            let slot = stack_address(state, memory).expect("a stack slot");
            let sources = &operands[..operands.len() - 1];
            forget_stack(
                state,
                slot,
                8 * sources.len() as i64,
                known.strings.object_size,
            );
            for (index, source) in sources.iter().enumerate() {
                let value = match *source {
                    "xzr" => single(Fact::Constant(0)),
                    source => offset(state, source, 0),
                };
                state.slots.insert(slot + 8 * index as i64, value);
            }
        }
        ("bl", [target]) => {
            let target = immediate(target).map(|target| target as u64);
            call(row.address, target, known, state);
        }
        ("blr", _) => call(row.address, None, known, state),
        _ => clear_written(row, &operands, known.strings, state),
    }
}

/// A call: follow the string functions, then make the caller-saved registers unknown and `x0`
/// the call's result. An owner getter returns the address at its owner's offset `k`, whatever
/// item its receiver is; another offset getter returns `x0` plus its `k`.
fn call(address: u64, target: Option<u64>, known: Known<'_>, state: &mut State) {
    let strings = known.strings;
    let object = state.stack_offset(0);
    let getter = target.and_then(|target| Some((target, known.offset_getters.get(&target)?)));
    let result = match getter {
        Some((target, getter)) if getter.owner.is_some() => {
            single(Fact::Address(Root::Getter(target), getter.offset))
        }
        Some((_, getter)) => offset(state, "x0", getter.offset),
        None => single(Fact::Result(address)),
    };

    match target {
        Some(target) if strings.from_literal.contains(&target) => {
            if let Some(object) = object {
                let literal = state.registers[1].clone();
                let text = match literal {
                    Some(facts) if facts.iter().all(|fact| matches!(fact, Fact::Constant(_))) => {
                        Some(facts)
                    }
                    _ => None,
                };
                state.strings.insert(object, text);
            } else {
                forget_objects(state, 0);
            }
        }
        Some(target) if strings.copy.contains(&target) => {
            if let Some(object) = object {
                let text = state
                    .stack_offset(1)
                    .and_then(|source| state.strings.get(&source).cloned().flatten());
                state.strings.insert(object, text);
            } else {
                forget_objects(state, 0);
            }
        }
        Some(target) if strings.destructors.contains(&target) => {
            if let Some(object) = object {
                state.strings.remove(&object);
            } else {
                forget_objects(state, 0);
            }
        }
        _ => forget_objects(state, 0),
    }

    let lowest = (0..=7)
        .filter_map(|index| state.registers[index].as_ref())
        .flatten()
        .filter_map(|fact| match fact {
            Fact::Stack(offset) => Some(*offset),
            _ => None,
        })
        .min();
    if let Some(lowest) = lowest {
        state.slots.retain(|slot, _| *slot < lowest);
    }

    for index in 0..=18 {
        state.clear(index);
    }
    state.set(0, result);
}

/// The known amounts of an `add` or `sub` operand: an immediate, or a register of constants,
/// with an optional left shift.
fn amounts(state: &State, amount: &str, rest: &[&str]) -> Option<Vec<i64>> {
    let shift = match rest {
        [] => 0,
        [shift] => shift.strip_prefix("lsl#").and_then(immediate)?,
        _ => return None,
    };
    let values: Vec<i64> = match immediate(amount) {
        Some(value) if amount.starts_with('#') => vec![value],
        _ => state
            .registers
            .get(general_register(amount)?)?
            .as_ref()?
            .iter()
            .map(|fact| match fact {
                Fact::Constant(value) => Some(*value as i64),
                _ => None,
            })
            .collect::<Option<_>>()?,
    };
    Some(values.into_iter().map(|value| value << shift).collect())
}

/// The value of `source` moved by an unknown number of pointers, a register amount shifted by
/// `rest`: each address in a buffer of owner pointers stays in that buffer.
fn moved_in_buffer(state: &State, source: &str, rest: &[&str]) -> Value {
    if !matches!(rest, [scale] if is_pointer_scale(scale)) {
        return None;
    }

    let facts = state.registers.get(general_register(source)?)?.as_ref()?;
    facts
        .iter()
        .map(|fact| matches!(fact, Fact::Elements(_)).then_some(*fact))
        .collect()
}

/// Where the stack pointer is after it becomes `source` plus or minus an amount: known when
/// `source` is the stack pointer or holds a stack offset, and the amount is known.
fn moved_stack(
    state: &State,
    source: &str,
    amount: &str,
    rest: &[&str],
    operation: &str,
) -> Option<i64> {
    let base = match source {
        "sp" => state.sp?,
        source => state.stack_offset(general_register(source)?)?,
    };
    match amounts(state, amount, rest)?.as_slice() {
        [amount] if operation == "sub" => Some(base - amount),
        [amount] => Some(base + amount),
        _ => None,
    }
}

/// The stack offset that a memory operand without writeback addresses.
fn stack_address(state: &State, memory: &str) -> Option<i64> {
    let inner = memory.strip_prefix('[')?.strip_suffix(']')?;
    let mut parts = inner.split(',');
    let base = parts.next()?;
    let displacement = match parts.next() {
        None => 0,
        Some(text) => immediate(text)?,
    };
    if parts.next().is_some() {
        return None;
    }
    let base = if base == "sp" {
        state.sp?
    } else {
        state.stack_offset(general_register(base)?)?
    };
    Some(base + displacement)
}

/// A store of `width` bytes at a stack offset: slots and strings that it overlaps change.
fn forget_stack(state: &mut State, address: i64, width: i64, object_size: i64) {
    state
        .slots
        .retain(|slot, _| *slot + 8 <= address || *slot >= address + width);
    let touched: Vec<i64> = state
        .strings
        .keys()
        .copied()
        .filter(|start| *start < address + width && address < *start + object_size)
        .collect();
    for start in touched {
        state.strings.insert(start, None);
    }
}

/// A call receives the objects in register `index` as its object: their text becomes unknown.
/// A string is passed by its stack address, so an unknown value is no string that the pass
/// tracks.
fn forget_objects(state: &mut State, index: usize) {
    let Some(facts) = state.registers[index].clone() else {
        return;
    };
    for fact in facts {
        if let Fact::Stack(offset) = fact
            && state.strings.contains_key(&offset)
        {
            state.strings.insert(offset, None);
        }
    }
}

fn offset(state: &State, source: &str, amount: i64) -> Value {
    if source == "sp" {
        return single(Fact::Stack(state.sp? + amount));
    }
    let source = general_register(source)?;
    let facts = state.registers.get(source)?.as_ref()?;
    let moved = facts
        .iter()
        .map(|fact| match *fact {
            Fact::Constant(value) => Some(Fact::Constant(value.wrapping_add(amount as u64))),
            Fact::Stack(base) => Some(Fact::Stack(base + amount)),
            Fact::Address(root, base) => Some(Fact::Address(root, base + amount)),
            Fact::Member(root, member, base) => Some(Fact::Member(root, member, base + amount)),
            Fact::Elements(_) if amount % POINTER == 0 => Some(*fact),
            Fact::Elements(_) => None,
            Fact::Global(_)
            | Fact::Field(..)
            | Fact::Deref(..)
            | Fact::DerefAt(..)
            | Fact::Result(_)
                if amount == 0 =>
            {
                Some(*fact)
            }
            Fact::Global(_)
            | Fact::Field(..)
            | Fact::Deref(..)
            | Fact::DerefAt(..)
            | Fact::Result(_) => None,
        })
        .collect::<Option<BTreeSet<_>>>()?;
    Some(moved)
}

/// The value that a 64-bit load without writeback from `memory` gives: a stack slot's, or what
/// [`load`] names.
fn loaded(state: &State, memory: &str, buffers: &BTreeSet<Buffer>) -> Value {
    match stack_address(state, memory) {
        Some(slot) => state.slots.get(&slot).cloned().flatten(),
        None => load(state, memory, buffers),
    }
}

/// A 64-bit load without writeback: from a constant address it is that global's value; from a
/// global's value it is a field of the object that the global points at. From an address that a
/// root leads to, it is the word at that address, up to a vtable slot; a word of `buffers` points
/// at a buffer of owner pointers, each word of which is an element. A load at a register index
/// scaled by the pointer size, `[xb, xi, lsl #3]`, reads an element of a buffer and nothing else.
fn load(state: &State, memory: &str, buffers: &BTreeSet<Buffer>) -> Value {
    let inner = memory.strip_prefix('[')?.strip_suffix(']')?;
    let parts: Vec<&str> = inner.split(',').collect();
    let (base, displacement) = match parts[..] {
        [base] => (base, 0),
        [base, displacement] => (base, immediate(displacement)?),
        [base, index, scale] => return indexed_element(state, base, index, scale),
        _ => return None,
    };
    let facts = state.registers.get(general_register(base)?)?.as_ref()?;
    facts
        .iter()
        .map(|fact| match *fact {
            Fact::Constant(address) => {
                Some(Fact::Global(address.wrapping_add(displacement as u64)))
            }
            Fact::Global(global) => Some(Fact::Field(global, displacement)),
            Fact::Field(global, 0) if buffers.contains(&Buffer::Instance(global, displacement)) => {
                Some(Fact::Elements(Buffer::Instance(global, displacement)))
            }
            Fact::Address(Root::Argument(index), offset)
                if buffers.contains(&Buffer::Argument(index, offset + displacement)) =>
            {
                Some(Fact::Elements(Buffer::Argument(
                    index,
                    offset + displacement,
                )))
            }
            Fact::Address(root, offset) => Some(Fact::Member(root, offset + displacement, 0)),
            Fact::Member(root, member, offset) => {
                Some(Fact::Deref(root, member, offset + displacement))
            }
            Fact::Deref(root, member, offset) => {
                Some(Fact::DerefAt(root, member, offset, displacement))
            }
            Fact::Elements(buffer) if displacement % POINTER == 0 => {
                Some(Fact::Address(Root::Element(buffer), 0))
            }
            _ => None,
        })
        .collect::<Option<BTreeSet<_>>>()
}

/// Whether a register operand's shift or extension, such as `lsl#3`, scales it by the pointer size.
fn is_pointer_scale(scale: &str) -> bool {
    matches!(scale, "lsl#3" | "uxtw#3" | "sxtw#3")
}

/// The element that a load at `base` plus a general register `index` scaled by the pointer size
/// reads, when `base` holds only addresses in buffers of owner pointers.
fn indexed_element(state: &State, base: &str, index: &str, scale: &str) -> Value {
    general_register(index)?;
    if !is_pointer_scale(scale) {
        return None;
    }

    let facts = state.registers.get(general_register(base)?)?.as_ref()?;
    facts
        .iter()
        .map(|fact| match *fact {
            Fact::Elements(buffer) => Some(Fact::Address(Root::Element(buffer), 0)),
            _ => None,
        })
        .collect()
}

/// Make unknown every register that an instruction outside the followed forms may write.
fn clear_written(
    row: &Instruction,
    operands: &[&str],
    strings: &StringFunctions,
    state: &mut State,
) {
    let operation = row.operation.as_str();
    if operation.starts_with("st") && !operation.contains("xr") {
        forget_stored_strings(operation, operands, strings.object_size, state);
    }

    // Pre- or post-index addressing writes the base register back, moved by its offset.
    let mut moved_bases = Vec::new();
    for position in 0..operands.len() {
        let Some(WriteBack { base, amount }) = write_back(operands, position) else {
            continue;
        };

        if base == "sp" {
            state.sp = state.sp.map(|sp| sp + amount.unwrap_or(0));
        } else if let Some(index) = general_register(base) {
            let moved = amount.and_then(|amount| offset(state, base, amount));
            moved_bases.push((index, moved));
        }
    }

    for register in written_registers(operation, &row.operands) {
        state.clear(register);
    }
    for (index, moved) in moved_bases {
        state.set(index, moved);
    }
}

/// The base register that a memory operand writes back, and the amount it moves by when that
/// amount is known.
struct WriteBack<'a> {
    base: &'a str,
    amount: Option<i64>,
}

/// The write-back of the operand at `position`: pre-index `[base,#amount]!` or post-index
/// `[base],#amount`. `None` for any other operand.
fn write_back<'a>(operands: &[&'a str], position: usize) -> Option<WriteBack<'a>> {
    let inner = operands[position].strip_prefix('[')?;
    if let Some(pre) = inner.strip_suffix("]!") {
        let mut parts = pre.split(',');
        return Some(WriteBack {
            base: parts.next()?,
            amount: parts.next().and_then(immediate),
        });
    }

    let next = operands
        .get(position + 1)
        .filter(|next| next.starts_with('#'))?;
    Some(WriteBack {
        base: inner.strip_suffix(']')?,
        amount: immediate(next),
    })
}

/// A store that the pass does not follow changes the slots and strings that it overlaps.
fn forget_stored_strings(operation: &str, operands: &[&str], object_size: i64, state: &mut State) {
    let Some(memory) = operands.iter().find(|operand| operand.starts_with('[')) else {
        return;
    };
    let inner = memory
        .trim_start_matches('[')
        .trim_end_matches('!')
        .trim_end_matches(']');
    let mut parts = inner.split(',');
    let Some(base) = parts.next() else {
        return;
    };
    let displacement = parts.next().and_then(immediate).unwrap_or(0);
    let address = if base == "sp" {
        state.sp.map(|sp| sp + displacement)
    } else {
        general_register(base)
            .and_then(|base| state.stack_offset(base))
            .map(|offset| offset + displacement)
    };
    if let Some(address) = address {
        forget_stack(
            state,
            address,
            store_width(operation, operands),
            object_size,
        );
    }
}

/// The bytes that a store writes: its register width, twice for a pair. A form that the pass
/// does not know writes at most 32 bytes, as a pair of vector registers does.
fn store_width(operation: &str, operands: &[&str]) -> i64 {
    let register_width = match operation {
        "strb" | "sturb" | "stlrb" => return 1,
        "strh" | "sturh" | "stlrh" => return 2,
        _ => match operands.first().and_then(|first| first.chars().next()) {
            Some('x' | 'd') => 8,
            Some('w' | 's') => 4,
            Some('q') => 16,
            Some('h') => 2,
            Some('b') => 1,
            _ => return 32,
        },
    };
    if operation.starts_with("stp") || operation.starts_with("stnp") {
        2 * register_width
    } else {
        register_width
    }
}

/// The value that a write to `destination` keeps. A 32-bit register keeps the low half of each
/// constant; a symbolic fact names a 64-bit value, so its low half is unknown.
fn narrowed(value: Value, destination: &str) -> Value {
    if destination.starts_with('x') {
        return value;
    }

    value?
        .into_iter()
        .map(|fact| match fact {
            Fact::Constant(constant) => Some(Fact::Constant(constant as u32 as u64)),
            _ => None,
        })
        .collect()
}

pub(super) fn immediate(text: &str) -> Option<i64> {
    let text = text.strip_prefix('#').unwrap_or(text);
    let (negative, text) = match text.strip_prefix('-') {
        Some(text) => (true, text),
        None => (false, text),
    };
    let value = match text.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).ok()? as i64,
        None => text.parse::<i64>().ok()?,
    };
    Some(if negative { -value } else { value })
}

/// Split operands at commas outside brackets.
pub(super) fn split(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (index, character) in text.char_indices() {
        match character {
            '[' => depth += 1,
            ']' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&text[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    if start < text.len() {
        parts.push(&text[start..]);
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::assembler::arm64;
    use crate::engine::analysis::decode::decode_arm64;

    /// The state before the last instruction of `bytes`, authored at `0x1000`.
    fn state_at_end(bytes: &[u8]) -> State {
        known_state_at_end(bytes, Known::strings(&StringFunctions::default()))
    }

    /// The state before the last instruction of `bytes`, authored at `0x1000`, with `known`.
    fn known_state_at_end(bytes: &[u8], known: Known<'_>) -> State {
        let rows = decode_arm64(bytes, 0x1000).unwrap();
        let last = rows.last().unwrap().address;
        let mut end = None;
        each_state(&rows, known, |row, state| {
            if row.address == last {
                end = Some(state.clone());
            }
        });
        end.unwrap()
    }

    #[test]
    fn an_atomic_forgets_its_destination_and_keeps_its_source() {
        let bytes = arm64!(at 0x1000;
            mov x9, x1;
            mov x8, x2;
            ldaddal x9, x8, [x8];
            ret
        );
        let state = state_at_end(&bytes);

        assert_eq!(
            *state.register(9),
            single(Fact::Address(Root::Argument(1), 0))
        );
        assert_eq!(*state.register(8), None);
    }

    #[test]
    fn an_exclusive_pair_store_forgets_its_status_register() {
        let bytes = arm64!(at 0x1000;
            mov x9, x1;
            stxp w9, x2, x3, [x0];
            ret
        );

        assert_eq!(*state_at_end(&bytes).register(9), None);
    }

    #[test]
    fn a_32_bit_write_keeps_the_low_half_of_a_constant_and_no_symbolic_fact() {
        let bytes = arm64!(at 0x1000;
            movn x11, #0;
            mov w11, w11;
            mov w9, w1;
            add w12, w2, #8;
            ret
        );
        let state = state_at_end(&bytes);

        assert_eq!(*state.register(11), single(Fact::Constant(0xffff_ffff)));
        assert_eq!(*state.register(9), None);
        assert_eq!(*state.register(12), None);
    }

    /// The facts of `x0` before the last instruction of `bytes`, where the words of `buffers` point
    /// at buffers of owner pointers.
    fn x0_at_end_with_buffers(bytes: &[u8], buffers: &[Buffer]) -> Value {
        let buffers = buffers.iter().copied().collect();
        let strings = StringFunctions::default();
        let known = Known {
            buffers: &buffers,
            ..Known::strings(&strings)
        };
        known_state_at_end(bytes, known).register(0).clone()
    }

    /// The facts of `x0` before the last instruction of `bytes`, whose word at `x0 + 8` points at
    /// a buffer of owner pointers when `typed`.
    fn x0_at_end_with_buffer(bytes: &[u8], typed: bool) -> Value {
        let buffers: &[Buffer] = if typed {
            &[Buffer::Argument(0, 8)]
        } else {
            &[]
        };
        x0_at_end_with_buffers(bytes, buffers)
    }

    #[test]
    fn a_cursor_over_a_typed_buffer_loads_an_element() {
        let bytes = arm64!(at 0x1000;
            ldr x19, [x0, #8];
            mov x20, #3;
            ->next:;
            ldr x8, [x19];
            cbz x8, ->skip;
            add x0, x8, #0x268;
            ->skip:;
            add x19, x19, #8;
            subs x20, x20, #1;
            b.ne ->next;
            ldr x8, [x19];
            add x0, x8, #0x268;
            ret
        );

        let element = single(Fact::Address(Root::Element(Buffer::Argument(0, 8)), 0x268));
        assert_eq!(x0_at_end_with_buffer(&bytes, true), element);
        assert_eq!(x0_at_end_with_buffer(&bytes, false), None);
    }

    #[test]
    fn a_misaligned_cursor_over_a_typed_buffer_loads_no_element() {
        let bytes = arm64!(at 0x1000;
            ldr x19, [x0, #8];
            add x19, x19, #4;
            ldr x8, [x19];
            add x0, x8, #0x268;
            ret
        );

        assert_eq!(x0_at_end_with_buffer(&bytes, true), None);
    }

    #[test]
    fn a_cursor_moved_by_a_scaled_register_stays_in_its_buffer() {
        let bytes = arm64!(at 0x1000;
            ldr x9, [x0, #8];
            add x8, x9, x1, lsl #3;
            ldr x10, [x8];
            add x0, x10, #0x10;
            ret
        );

        let element = single(Fact::Address(Root::Element(Buffer::Argument(0, 8)), 0x10));
        assert_eq!(x0_at_end_with_buffer(&bytes, true), element);
        assert_eq!(x0_at_end_with_buffer(&bytes, false), None);
    }

    #[test]
    fn a_load_at_a_scaled_register_index_reads_an_element_only_from_a_buffer() {
        let bytes = arm64!(at 0x1000;
            ldr x9, [x0, #8];
            ldr x10, [x9, x1, lsl #3];
            add x0, x10, #0x10;
            ret
        );
        let element = single(Fact::Address(Root::Element(Buffer::Argument(0, 8)), 0x10));
        assert_eq!(x0_at_end_with_buffer(&bytes, true), element);
        assert_eq!(x0_at_end_with_buffer(&bytes, false), None);

        let unscaled = arm64!(at 0x1000;
            ldr x9, [x0, #8];
            ldr x10, [x9, x1];
            add x0, x10, #0x10;
            ret
        );
        assert_eq!(x0_at_end_with_buffer(&unscaled, true), None);
    }

    #[test]
    fn a_global_instance_leads_its_array_buffer_to_elements() {
        let instance = arm64!(at 0x1000;
            adrp x8, #0x5000; // the page 0x6000
            ldr x8, [x8, #0x40]; // the instance's pointer slot
            ldr x27, [x8];
            ldr x9, [x27, #0x48];
            ldr x10, [x9, x1, lsl #3];
            add x0, x10, #0x58;
            ret
        );
        let buffer = Buffer::Instance(0x6040, 0x48);
        let element = single(Fact::Address(Root::Element(buffer), 0x58));
        assert_eq!(x0_at_end_with_buffers(&instance, &[buffer]), element);
        assert_eq!(x0_at_end_with_buffers(&instance, &[]), None);

        let beside_the_instance = arm64!(at 0x1000;
            adrp x8, #0x5000;
            ldr x8, [x8, #0x40];
            ldr x27, [x8, #8];
            ldr x9, [x27, #0x40];
            ldr x10, [x9, x1, lsl #3];
            add x0, x10, #0x58;
            ret
        );
        assert_eq!(
            x0_at_end_with_buffers(&beside_the_instance, &[buffer]),
            None
        );

        let moved_instance = arm64!(at 0x1000;
            adrp x8, #0x5000;
            ldr x8, [x8, #0x40];
            ldr x27, [x8];
            add x27, x27, #0x40;
            ldr x9, [x27, #8];
            ldr x10, [x9, x1, lsl #3];
            add x0, x10, #0x58;
            ret
        );
        assert_eq!(x0_at_end_with_buffers(&moved_instance, &[buffer]), None);
    }

    #[test]
    fn a_receiver_store_gives_each_word_that_its_value_may_be() {
        let bytes = arm64!(at 0x1000;
            mov x19, x0;
            adrp x8, #0x5000;
            ldr x8, [x8, #0x40];
            cbz x1, ->other;
            ldr x9, [x8];
            b ->store;
            ->other:;
            mov x9, x2;
            ->store:;
            str x9, [x19, #0x20];
            str x3, [x19, #0x28];
            bl extern 0x7000;
            str x0, [x19, #0x30];
            stp w4, w5, [x19, #0x38];
            ret
        );
        let rows = decode_arm64(&bytes, 0x1000).unwrap();
        let stores = receiver_stores(&rows, &StringFunctions::default(), &BTreeSet::new());

        let words = |words: &[StoredWord]| words.iter().copied().collect::<BTreeSet<_>>();
        assert_eq!(
            stores,
            BTreeMap::from([
                (
                    0x20,
                    words(&[StoredWord::Loaded(0x6040), StoredWord::Parameter(2)])
                ),
                (0x28, words(&[StoredWord::Parameter(3)])),
                (0x30, words(&[StoredWord::Unknown])),
                (0x38, words(&[StoredWord::Unknown])),
            ])
        );
    }

    #[test]
    fn a_receiver_store_keeps_a_named_alternative_beside_an_unnamed_one() {
        let bytes = arm64!(at 0x1000;
            mov x19, x0;
            cbz x1, ->other;
            adrp x8, #0x5000;
            ldr x8, [x8, #0x40];
            ldr x9, [x8];
            b ->store;
            ->other:;
            bl extern 0x7000;
            mov x9, x0;
            ->store:;
            str x9, [x19, #0x20];
            ret
        );
        let rows = decode_arm64(&bytes, 0x1000).unwrap();
        let stores = receiver_stores(&rows, &StringFunctions::default(), &BTreeSet::new());

        assert_eq!(
            stores,
            BTreeMap::from([(
                0x20,
                BTreeSet::from([StoredWord::Loaded(0x6040), StoredWord::Unknown])
            )])
        );
    }

    #[test]
    fn an_owner_getter_returns_its_owner_offset_whatever_its_receiver() {
        let bytes = arm64!(at 0x1000;
            ldr x0, [x1, x2];
            bl extern 0x7000;
            ret
        );
        let strings = StringFunctions::default();
        let state = |owner: Option<String>| {
            let getters = BTreeMap::from([(
                0x7000,
                OffsetGetter {
                    offset: 0xa0,
                    owner,
                },
            )]);
            let known = Known {
                offset_getters: &getters,
                ..Known::strings(&strings)
            };
            known_state_at_end(&bytes, known).register(0).clone()
        };

        assert_eq!(
            state(Some("COwner".into())),
            single(Fact::Address(Root::Getter(0x7000), 0xa0))
        );
        assert_eq!(state(None), None);
    }
}
