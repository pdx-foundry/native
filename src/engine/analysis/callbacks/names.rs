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
//! of known constants, `mov` of a register or an immediate, a 64-bit `ldr` from a constant or a
//! loaded global, and 64-bit stores and loads of stack slots. Any other instruction makes the
//! registers that it may write unknown. A call makes `x0`–`x18` unknown and `x0` the result of
//! that call. Stack offsets are relative to the stack pointer at entry; the state also knows
//! where the stack pointer is now, until it moves by an amount that is not known.
//!
//! A stack slot keeps the 64-bit value last stored to it on every path. A callee writes only
//! inside the objects that it receives, and an object never starts above its address, so a call
//! that receives a stack address forgets every slot at or above that address. Other memory is
//! not tracked.
use std::collections::{BTreeMap, BTreeSet};

use crate::engine::analysis::decode::Instruction;

/// The most facts that one value holds before it becomes unknown.
pub(super) const VALUE_LIMIT: usize = 16;

/// One thing that a register can hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Fact {
    Constant(u64),
    /// The stack pointer at entry, plus this offset.
    Stack(i64),
    /// Argument register `n` at entry, plus this offset.
    Argument(usize, i64),
    /// The value stored at this global address.
    Global(u64),
    /// The value stored at the address held in a global, plus this offset.
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
            *register = Some(BTreeSet::from([Fact::Argument(index, 0)]));
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
        match self
            .registers
            .get(index)?
            .as_ref()?
            .iter()
            .collect::<Vec<_>>()
            .as_slice()
        {
            [Fact::Stack(offset)] => Some(*offset),
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

/// Run the pass over one function and give `visit` the state before each instruction, once the
/// states are stable.
///
/// A block that no direct branch reaches is a target of a branch through a register, such as a
/// jump table, when the function has one: it starts from the joined states at those branches.
/// Otherwise it is dead code, such as padding, and it starts from unknown registers and passes
/// nothing to the blocks after it.
pub(super) fn each_state(
    rows: &[Instruction],
    strings: &StringFunctions,
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
                step(row, strings, &mut state);
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
                    step(row, strings, &mut state);
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
            step(row, strings, &mut state);
        }
    }
}

/// Basic blocks of one function, by the position of their first instruction.
struct Blocks {
    /// Block start → the position after its last instruction.
    starts: BTreeMap<usize, usize>,
    /// Block start → the starts of the blocks that can run next.
    next: BTreeMap<usize, Vec<usize>>,
}

impl Blocks {
    fn new(rows: &[Instruction]) -> Self {
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

    fn range(&self, block: usize) -> std::ops::Range<usize> {
        block..self.starts[&block]
    }

    fn successors(&self, block: usize) -> Vec<usize> {
        self.next[&block].clone()
    }

    fn jumps_through_register(&self, rows: &[Instruction], block: usize) -> bool {
        rows[self.starts[&block] - 1].operation == "br"
    }
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
fn step(row: &Instruction, strings: &StringFunctions, state: &mut State) {
    let operands: Vec<&str> = split(&row.operands);
    let operation = row.operation.as_str();

    match (operation, operands.as_slice()) {
        ("adrp" | "adr", [destination, target]) => {
            if let (Some(destination), Some(target)) = (register(destination), immediate(target)) {
                state.set(destination, single(Fact::Constant(target as u64)));
            }
        }
        ("add" | "sub", ["sp", source, amount, rest @ ..]) => {
            state.sp = moved_stack(state, source, amount, rest, operation);
        }
        ("add" | "sub", [destination, source, amount, rest @ ..]) => {
            let Some(destination) = register(destination) else {
                return;
            };
            let value = amounts(state, amount, rest).and_then(|amounts| {
                let mut joined = BTreeSet::new();
                for amount in amounts {
                    let amount = if operation == "sub" { -amount } else { amount };
                    joined.extend(offset(state, source, amount)?);
                }
                (joined.len() <= VALUE_LIMIT).then_some(joined)
            });
            state.set(destination, value);
        }
        ("mov", ["sp", source]) => state.sp = moved_stack(state, source, "#0", &[], "add"),
        ("mov", [destination, source]) => {
            let Some(destination) = register(destination) else {
                return;
            };
            let value = match immediate(source) {
                Some(value) if source.starts_with('#') => single(Fact::Constant(truncate(
                    value as u64,
                    destination_is_wide(operands[0]),
                ))),
                _ => offset(state, source, 0),
            };
            state.set(destination, value);
        }
        ("ldr", [destination, memory]) if destination.starts_with('x') && memory.ends_with(']') => {
            let Some(destination) = register(destination) else {
                return;
            };
            let value = match stack_address(state, memory) {
                Some(slot) => state.slots.get(&slot).cloned().flatten(),
                None => load(state, memory),
            };
            state.set(destination, value);
        }
        ("ldp", [first, second, memory]) if first.starts_with('x') && memory.ends_with(']') => {
            let slot = stack_address(state, memory);
            for (index, destination) in [first, second].into_iter().enumerate() {
                if let Some(destination) = register(destination) {
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
            forget_stack(state, slot, 8 * sources.len() as i64, strings.object_size);
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
            call(row.address, target, strings, state);
        }
        ("blr", _) => call(row.address, None, strings, state),
        _ => clear_written(operation, &operands, strings, state),
    }
}

/// A call: follow the string functions, then make the caller-saved registers unknown and `x0`
/// the call's result.
fn call(address: u64, target: Option<u64>, strings: &StringFunctions, state: &mut State) {
    let object = match state.registers[0]
        .as_ref()
        .map(|facts| facts.iter().collect::<Vec<_>>())
    {
        Some(facts) => match facts.as_slice() {
            [Fact::Stack(offset)] => Some(*offset),
            _ => None,
        },
        None => None,
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
                let text = match state.registers[1]
                    .as_ref()
                    .map(|f| f.iter().collect::<Vec<_>>())
                {
                    Some(facts) => match facts.as_slice() {
                        [Fact::Stack(source)] => state.strings.get(source).cloned().flatten(),
                        _ => None,
                    },
                    None => None,
                };
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
    state.set(0, single(Fact::Result(address)));
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
            .get(register(amount)?)?
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
        source => state.stack_offset(register(source)?)?,
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
        state.stack_offset(register(base)?)?
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
    let source = register(source)?;
    let facts = state.registers.get(source)?.as_ref()?;
    let moved = facts
        .iter()
        .map(|fact| match *fact {
            Fact::Constant(value) => Some(Fact::Constant(value.wrapping_add(amount as u64))),
            Fact::Stack(base) => Some(Fact::Stack(base + amount)),
            Fact::Argument(index, base) => Some(Fact::Argument(index, base + amount)),
            Fact::Field(global, base) => Some(Fact::Field(global, base + amount)),
            Fact::Global(_) | Fact::Result(_) if amount == 0 => Some(*fact),
            Fact::Global(_) | Fact::Result(_) => None,
        })
        .collect::<Option<BTreeSet<_>>>()?;
    Some(moved)
}

/// A 64-bit load without writeback: from a constant address it is that global's value; from a
/// global's value it is a field of the object that the global points at.
fn load(state: &State, memory: &str) -> Value {
    let inner = memory.strip_prefix('[')?.strip_suffix(']')?;
    let mut parts = inner.split(',');
    let base = register(parts.next()?)?;
    let displacement = match parts.next() {
        None => 0,
        Some(text) => immediate(text)?,
    };
    if parts.next().is_some() {
        return None;
    }
    let facts = state.registers.get(base)?.as_ref()?;
    facts
        .iter()
        .map(|fact| match *fact {
            Fact::Constant(address) => {
                Some(Fact::Global(address.wrapping_add(displacement as u64)))
            }
            Fact::Global(global) => Some(Fact::Field(global, displacement)),
            _ => None,
        })
        .collect::<Option<BTreeSet<_>>>()
}

/// Make unknown every register that an instruction outside the followed forms may write.
fn clear_written(operation: &str, operands: &[&str], strings: &StringFunctions, state: &mut State) {
    let writes_first = !(operation.starts_with("st") && !operation.contains("xr")
        || operation.starts_with("b.")
        || matches!(
            operation,
            "b" | "bl"
                | "br"
                | "blr"
                | "cmp"
                | "cmn"
                | "tst"
                | "ccmp"
                | "ccmn"
                | "fcmp"
                | "fccmp"
                | "cbz"
                | "cbnz"
                | "tbz"
                | "tbnz"
                | "ret"
                | "nop"
                | "brk"
                | "prfm"
                | "dmb"
                | "dsb"
                | "isb"
                | "hint"
        ));
    if writes_first && let Some(first) = operands.first().and_then(|first| register(first)) {
        state.clear(first);
    }
    if matches!(operation, "ldp" | "ldpsw" | "ldnp" | "ldaxp" | "ldxp")
        && let Some(second) = operands.get(1).and_then(|second| register(second))
    {
        state.clear(second);
    }
    if operation.starts_with("st") && !operation.contains("xr") {
        forget_stored_strings(operation, operands, strings.object_size, state);
    }
    // Pre- or post-index addressing writes the base register back, moved by its offset.
    for (position, operand) in operands.iter().enumerate() {
        let Some(inner) = operand.strip_prefix('[') else {
            continue;
        };
        let (base, amount) = match inner.strip_suffix("]!") {
            Some(pre) => {
                let mut parts = pre.split(',');
                (parts.next(), parts.next().and_then(immediate))
            }
            None => match operands.get(position + 1) {
                Some(next) if next.starts_with('#') => (inner.strip_suffix(']'), immediate(next)),
                _ => continue,
            },
        };
        let Some(base) = base else {
            continue;
        };
        if let Some(index) = register(base) {
            let moved = amount.and_then(|amount| offset(state, base, amount));
            state.set(index, moved);
        }
    }
    // Pre- or post-index addressing on the stack pointer moves it.
    for (position, operand) in operands.iter().enumerate() {
        let Some(inner) = operand.strip_prefix("[sp") else {
            continue;
        };
        if let Some(pre) = inner.strip_suffix("]!") {
            let amount = pre.strip_prefix(',').and_then(immediate).unwrap_or(0);
            state.sp = state.sp.map(|sp| sp + amount);
        } else if inner == "]"
            && let Some(post) = operands.get(position + 1).and_then(|next| immediate(next))
        {
            state.sp = state.sp.map(|sp| sp + post);
        }
    }
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
        register(base)
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

/// General register number of an operand such as `x1`, `w20`, `fp` or `lr`; `None` for the stack
/// pointer, a zero register or anything else.
pub(super) fn register(text: &str) -> Option<usize> {
    match text {
        "fp" => return Some(29),
        "lr" => return Some(30),
        _ => {}
    }
    let number = text.strip_prefix(['x', 'w'])?.parse::<usize>().ok()?;
    (number <= 30).then_some(number)
}

fn destination_is_wide(text: &str) -> bool {
    text.starts_with('x') || matches!(text, "fp" | "lr")
}

fn truncate(value: u64, wide: bool) -> u64 {
    if wide { value } else { value as u32 as u64 }
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
