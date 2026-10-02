use super::tokens::{FunctionView, decode, function, names_by_address, number, register};
use super::{
    Condition, DataSection, FieldGap, FieldGapKind, FieldInput, PathOutcome, ReaderJoin,
    TableEntry, TokenPath, Value,
};
use crate::engine::analysis::decode::Instruction;
use crate::engine::analysis::discovery::Symbol;
use crate::engine::analysis::stop::{Bound, Obstacle, Unknown, Unresolved};
use std::collections::{BTreeMap, BTreeSet};

/// The least token, since a token is a signed 32-bit word.
const MIN_TOKEN: i64 = i32::MIN as i64;
/// The greatest token.
const MAX_TOKEN: i64 = i32::MAX as i64;
const MAX_STATES: usize = 4096;
/// The most instructions that one path may run.
const MAX_PATH: usize = 500;
/// The most tokens that one jump through a table may select.
const MAX_TABLE_ENTRIES: usize = 1024;

/// Executable inputs shared by registry fields and command member dispatch.
#[derive(Clone)]
pub(crate) struct DispatchInput<'a> {
    functions: Vec<FunctionView<'a>>,
    key_readers: &'a super::KeyReaders,
    pub symbols: &'a [Symbol],
    pub read_only_data: &'a [DataSection],
    pub reader_token_offset: Option<u64>,
    pub member_delegates: bool,
    /// Permit unmodeled OR updates without widening existing field or command methods.
    pub bitwise_updates: bool,
    /// The constructor-proven owner address point and executable pointer slots.
    pub owner_vtable: Option<(u64, &'a BTreeMap<u64, u64>)>,
    /// Constructors whose body proves a wrapped owner pointer at this object offset.
    pub serializer_constructors: Option<&'a BTreeMap<u64, i64>>,
}
impl<'a> DispatchInput<'a> {
    pub(crate) fn with_owner_vtable(
        &self,
        owner_vtable: Option<(u64, &'a BTreeMap<u64, u64>)>,
    ) -> Self {
        Self {
            owner_vtable,
            ..self.clone()
        }
    }
    pub(crate) fn command(
        functions: &'a BTreeMap<u64, crate::engine::analysis::declarations::Function>,
        symbols: &'a [Symbol],
        read_only_data: &'a [DataSection],
        reader_token_offset: u64,
        key_readers: &'a super::KeyReaders,
    ) -> Self {
        Self {
            key_readers,
            functions: symbols
                .iter()
                .filter_map(|symbol| {
                    let body = functions.get(&symbol.address)?;
                    Some(FunctionView {
                        name: &symbol.name,
                        address: body.address,
                        code: &body.code,
                    })
                })
                .collect(),
            symbols,
            read_only_data,
            reader_token_offset: Some(reader_token_offset),
            member_delegates: true,
            bitwise_updates: false,
            owner_vtable: None,
            serializer_constructors: None,
        }
    }
}

/// A comparison of the token word `token + offset` with a constant.
#[derive(Clone, Copy)]
struct Comparison {
    offset: i64,
    pivot: i64,
}
/// The case of one token in a jump table.
#[derive(Clone, Copy)]
struct TableCase {
    table: u64,
    /// The code address of the case.
    address: u64,
    /// The last conditional branch before the jump, which sends the tokens outside the table
    /// to the rest of the switch.
    guard: Option<u64>,
}
#[derive(Clone)]
enum Flags {
    Token(Comparison),
    Values { tested: Value, pivots: Vec<i64> },
}

#[derive(Clone)]
struct State {
    stack: BTreeMap<i64, Value>,
    serializer: Option<(i64, i64, i64)>,
    copied_tokens: BTreeSet<i64>,
    targets: BTreeSet<i64>,
    emplaced: BTreeSet<i64>,
    stored: Option<(ReaderJoin, i64, i64)>,
    saved_addresses: BTreeMap<i64, Value>,
    unknown_stack_store: bool,
    pc: usize,
    registers: BTreeMap<String, Value>,
    flags: Option<Flags>,
    domain: [i64; 2],
    conditions: Vec<Condition>,
    path: Vec<u64>,
    /// The jump-table case that this token took.
    table_case: Option<TableCase>,
}
impl State {
    fn save_address(&mut self, at: i64, width: i64, value: Option<Value>) {
        self.unknown_stack_store |= self.stored.is_some() && value.is_none();
        self.saved_addresses
            .retain(|slot, _| *slot + 8 <= at || *slot >= at + width);
        if width == 8
            && let Some(value) = value
            && (Self::owner_derived(&value) || Self::stack_derived(&value))
        {
            self.saved_addresses.insert(at, value);
        }
    }

    fn clobber_call(&mut self) {
        for register in 0..=18 {
            self.registers.remove(&format!("x{register}"));
        }
        self.flags = None;
    }

    fn overlaps_stored(&self, location: &Value, width: i64) -> bool {
        let Some((_, start, size)) = &self.stored else {
            return false;
        };
        matches!(location, Value::Owner(at) if *at < start + size && at + width > *start)
    }

    fn owner_derived(value: &Value) -> bool {
        match value {
            Value::Owner(_) => true,
            Value::Load(base, _) | Value::Offset(base, _) => Self::owner_derived(base),
            Value::SumProduct(base, index, _) | Value::Indexed(base, index, _) => {
                Self::owner_derived(base) || Self::owner_derived(index)
            }
            _ => false,
        }
    }

    fn stack_derived(value: &Value) -> bool {
        match value {
            Value::Stack(_) => true,
            Value::Load(base, _) | Value::Offset(base, _) => Self::stack_derived(base),
            Value::SumProduct(base, index, _) | Value::Indexed(base, index, _) => {
                Self::stack_derived(base) || Self::stack_derived(index)
            }
            _ => false,
        }
    }

    fn call_can_overwrite(&self, value: &Value) -> bool {
        Self::owner_derived(value)
            || (Self::stack_derived(value)
                && (self.unknown_stack_store
                    || self
                        .saved_addresses
                        .values()
                        .chain(self.stack.values())
                        .any(Self::owner_derived)))
    }

    fn stack_store_value(&self, operand: &str) -> Option<Value> {
        if matches!(operand.trim(), "xzr" | "wzr") {
            Some(Value::Constant(0))
        } else {
            self.value(operand)
        }
    }

    fn value(&self, operand: &str) -> Option<Value> {
        if operand.starts_with('#') {
            return number(operand).map(Value::Constant);
        }
        let value = self.registers.get(&register(operand)?).cloned()?;
        if operand.starts_with('w') {
            match value {
                Value::Constant(value) => Some(Value::Constant(value as u32 as i32 as i64)),
                Value::Token => Some(Value::Token),
                Value::TokenWord(offset) => Some(Value::TokenWord(offset)),
                Value::Load(base, width) => Some(Value::Load(base, width.min(4))),
                value @ Value::Offset(..) if matches!(&value, Value::Offset(base, _) if matches!(base.as_ref(), Value::Load(_, 4))) => {
                    Some(value)
                }
                _ => None,
            }
        } else {
            Some(value)
        }
    }
    fn finish(&self, terminal: u64, outcome: PathOutcome) -> TokenPath {
        TokenPath {
            domain: self.domain,
            conditions: self.conditions.clone(),
            instructions: self.path.clone(),
            terminal,
            outcome,
        }
    }
    fn assign(&mut self, destination: &str, value: Option<Value>) {
        let Some(key) = register(destination) else {
            return;
        };
        if key == "xzr" {
            return;
        }
        self.registers.remove(&key);
        let value = if destination.starts_with('w') {
            match value {
                Some(Value::Constant(v)) => Some(Value::Constant(v as u32 as i64)),
                // A W-register write zero-extends, so a copy of the token is a token word.
                Some(Value::Token) => Some(Value::TokenWord(0)),
                Some(Value::TokenWord(offset)) => Some(Value::TokenWord(offset)),
                Some(value @ Value::Offset(..)) if matches!(&value, Value::Offset(base, _) if matches!(base.as_ref(), Value::Load(_, 4))) => {
                    Some(value)
                }
                _ => None,
            }
        } else {
            value
        };
        if let Some(value) = value {
            self.registers.insert(key, value);
        }
    }
}
fn offset(value: Value, amount: i64) -> Option<Value> {
    match value {
        Value::Owner(v) => v.checked_add(amount).map(Value::Owner),
        Value::Reader(v) => v.checked_add(amount).map(Value::Reader),
        Value::Stack(v) => v.checked_add(amount).map(Value::Stack),
        Value::Constant(v) => v.checked_add(amount).map(Value::Constant),
        Value::Offset(base, previous) => previous
            .checked_add(amount)
            .map(|offset| Value::Offset(base, offset)),
        Value::Load(..) | Value::Indexed(..) | Value::SumProduct(..) => {
            Some(Value::Offset(Box::new(value), amount))
        }
        _ => None,
    }
}
/// A 32-bit value as a signed word.
fn signed_word(value: i64) -> i64 {
    value as u32 as i32 as i64
}
/// The token intervals whose word `token + offset` is in `words`, an unsigned interval.
fn tokens_of_words([low, high]: [i64; 2], offset: i64) -> Vec<[i64; 2]> {
    let start = signed_word(low - offset);
    let end = start + (high - low);
    if end <= MAX_TOKEN {
        vec![[start, end]]
    } else {
        vec![
            [start, MAX_TOKEN],
            [MIN_TOKEN, MIN_TOKEN + (end - MAX_TOKEN - 1)],
        ]
    }
}
/// The token intervals in `domain` for which `condition` holds after `comparison`. A signed
/// condition is followed only on the token itself, since an added constant can overflow.
fn intervals(domain: [i64; 2], condition: &str, comparison: Comparison) -> Option<Vec<[i64; 2]>> {
    let Comparison { offset, pivot } = comparison;
    let word = pivot as u32 as i64;
    let last = u32::MAX as i64;
    let ranges = match (condition, offset) {
        ("eq", 0) => vec![[pivot, pivot]],
        ("ne", 0) => vec![[MIN_TOKEN, pivot - 1], [pivot + 1, MAX_TOKEN]],
        ("le", 0) => vec![[MIN_TOKEN, pivot]],
        ("lt", 0) => vec![[MIN_TOKEN, pivot - 1]],
        ("gt", 0) => vec![[pivot + 1, MAX_TOKEN]],
        ("ge", 0) => vec![[pivot, MAX_TOKEN]],
        ("eq" | "ne" | "ls" | "hi" | "lo" | "hs", _) => {
            let words = match condition {
                "eq" => vec![[word, word]],
                "ne" => vec![[0, word - 1], [word + 1, last]],
                "ls" => vec![[0, word]],
                "hi" => vec![[word + 1, last]],
                "lo" => vec![[0, word - 1]],
                _ => vec![[word, last]],
            };
            words
                .into_iter()
                .filter(|[low, high]| low <= high)
                .flat_map(|words| tokens_of_words(words, offset))
                .collect()
        }
        _ => return None,
    };
    Some(
        ranges
            .into_iter()
            .map(|[lo, hi]| [lo.max(domain[0]), hi.min(domain[1])])
            .filter(|[lo, hi]| lo <= hi)
            .collect(),
    )
}
fn opposite(condition: &str) -> Option<&'static str> {
    match condition {
        "eq" => Some("ne"),
        "ne" => Some("eq"),
        "lt" => Some("ge"),
        "ge" => Some("lt"),
        "gt" => Some("le"),
        "le" => Some("gt"),
        "hi" => Some("ls"),
        "ls" => Some("hi"),
        "hs" => Some("lo"),
        "lo" => Some("hs"),
        _ => None,
    }
}
fn memory(operand: &str) -> Option<(&str, i64)> {
    let interior = operand.strip_prefix('[')?.strip_suffix(']')?;
    let (base, amount) = interior.split_once(',').unwrap_or((interior, "#0"));
    register(base)?;
    Some((base, number(amount)?))
}
/// Stack pair access and its optional pre- or post-index base update.
fn pair_memory(operand: &str) -> Option<(&str, i64, Option<i64>)> {
    if let Some(address) = operand.strip_suffix('!') {
        let (base, amount) = memory(address)?;
        return Some((base, amount, Some(amount)));
    }
    if let Some((address, amount)) = operand.split_once("],") {
        let base = address.strip_prefix('[')?;
        register(base)?;
        return Some((base, 0, Some(number(amount)?)));
    }
    memory(operand).map(|(base, amount)| (base, amount, None))
}

/// The base, index register and index extension of a register-offset address.
fn indexed(address: &str) -> Option<(&str, &str, &str)> {
    let interior = address.strip_prefix('[')?.strip_suffix(']')?;
    let mut parts = interior.split(',');
    let (base, index) = (parts.next()?, parts.next()?);
    let extension = parts.next().unwrap_or("");
    (parts.next().is_none() && !index.starts_with('#')).then_some((base, index, extension))
}
/// The base and byte offset of a register-offset address whose index is a constant.
fn constant_index<'a>(state: &State, address: &'a str) -> Option<(&'a str, i64)> {
    let (base, index, extension) = indexed(address)?;
    let scale = match extension {
        "" => 0,
        _ => number(extension.strip_prefix("lsl")?).filter(|scale| (0..4).contains(scale))?,
    };
    let Value::Constant(index) = state.value(index)? else {
        return None;
    };
    Some((base, index.checked_mul(1 << scale)?))
}
/// The jump-table entry that a register-offset load reads, with its destination, when a
/// constant table address is indexed by a token word.
fn table_load<'a>(row: &'a Instruction, state: &State) -> Option<(&'a str, TableEntry)> {
    let (destination, address) = row.operands.split_once(',')?;
    let (width, signed) = match (row.operation.as_str(), destination.chars().next()?) {
        ("ldrb", 'w') => (1, false),
        ("ldrh", 'w') => (2, false),
        ("ldr", 'w') => (4, false),
        ("ldrsw", 'x') => (4, true),
        ("ldr", 'x') => (8, false),
        _ => return None,
    };
    let (base, index, extension) = indexed(address)?;
    let (extend, scale) = match extension.split_once('#') {
        Some((extend, scale)) => (extend, number(scale)?),
        None => (extension, 0),
    };
    // A table load scales the index by the entry width; another scale is not a table.
    if !(0..4).contains(&scale) || 1 << scale != width {
        return None;
    }
    // A 64-bit index must already be a zero-extended word; the original token's upper half is
    // undefined.
    let offset = match (extend, state.value(index)?) {
        ("" | "lsl", Value::TokenWord(offset)) if index.starts_with('x') => offset,
        ("uxtw", Value::Token) if index.starts_with('w') => 0,
        ("uxtw", Value::TokenWord(offset)) if index.starts_with('w') => offset,
        _ => return None,
    };
    let Value::Constant(table) = state.value(base)? else {
        return None;
    };
    let table = u64::try_from(table).ok()?;
    Some((
        destination,
        TableEntry {
            table,
            offset,
            width: width as u8,
            signed,
        },
    ))
}
/// The unsigned little-endian value of `width` bytes of `data` at `address`.
fn read_unsigned(data: &[DataSection], address: u64, width: u8) -> Option<u64> {
    let section = data.iter().find(|section| {
        address >= section.address && address - section.address < section.bytes.len() as u64
    })?;
    let start = usize::try_from(address - section.address).ok()?;
    let bytes = section.bytes.get(start..start.checked_add(width.into())?)?;
    Some(
        bytes
            .iter()
            .rev()
            .fold(0, |value, &byte| value << 8 | u64::from(byte)),
    )
}
/// The address that a jump through `entry` reaches for `token`, when its entry is readable.
fn case_address(
    token: i64,
    base: u64,
    entry: TableEntry,
    shift: u8,
    data: &[DataSection],
) -> Option<u64> {
    let index = (token + entry.offset) as u32 as u64;
    let at = entry
        .table
        .checked_add(index.checked_mul(entry.width.into())?)?;
    let raw = read_unsigned(data, at, entry.width)?;
    let unused = 64 - 8 * u32::from(entry.width);
    let value = if entry.signed {
        ((raw << unused) as i64 >> unused) as u64
    } else {
        raw
    };
    Some(base.wrapping_add(value.checked_shl(shift.into())?))
}
/// Add the gap for an undecoded jump table at `table` in the root `root`, once.
fn push_table_gap(
    gaps: &mut Vec<FieldGap>,
    root: &str,
    table: u64,
    why: &str,
    unresolved: Unresolved,
) {
    let gap = FieldGap {
        reason: format!("jump table at {table:#x} in {root}: {why}"),
        ..FieldGap::unresolved(FieldGapKind::JumpTable, unresolved)
    };
    if !gaps.contains(&gap) {
        gaps.push(gap);
    }
}
fn rejection(input: &DispatchInput<'_>, names: &BTreeMap<u64, Option<&str>>) -> bool {
    let Some((_, rows)) = decode_root(input, "CPersistent::ReadMember(CReader&, int)") else {
        return false;
    };
    rows.len() == 2
        && rows[0].operation == "mov"
        && rows[0].operands == "x0,x1"
        && rows[1].operation == "b"
        && number(&rows[1].operands).and_then(|a| names.get(&(a as u64)).copied().flatten())
            == Some("CReader::ReportUnexpected()")
}
/// The reader that the call at `at` joins. `entry` is the root function.
fn reader_join(
    name: Option<&str>,
    state: &State,
    at: u64,
    entry: u64,
    tail: bool,
    member_delegates: bool,
    reader_value_token_offset: Option<i64>,
) -> ReaderJoin {
    let Some(name) = name else {
        return ReaderJoin::Missing(Unresolved::at("callee", at, entry, Obstacle::Call));
    };
    let joined = crate::engine::analysis::readers::arguments_join(
        name,
        &state.registers,
        member_delegates,
        reader_value_token_offset,
    );
    if joined {
        ReaderJoin::Joined {
            callee: name.into(),
            arguments: state.registers.clone(),
            tail,
        }
    } else {
        ReaderJoin::Missing(Unresolved::at("reader-routing", at, entry, Obstacle::Call))
    }
}
/// The obstacle when `base` does not hold the provenance that an access needs.
fn unestablished(state: &State, base: &str) -> Obstacle {
    match (state.value(base), register(base)) {
        (None, Some(name)) => match name.strip_prefix('x').map(str::parse::<u8>) {
            Some(Ok(index)) => Obstacle::Unknown(Unknown::Register(index)),
            _ => Obstacle::Unsupported,
        },
        _ => Obstacle::Unsupported,
    }
}
/// Run `row`. `entry` is the root function, where a stop is entered.
fn apply(
    row: &Instruction,
    state: &mut State,
    entry: u64,
    input: &DispatchInput<'_>,
    wrapped_owner: Option<(i64, i64)>,
) -> Result<(), Unresolved> {
    if matches!(row.operation.as_str(), "str" | "strb" | "stp") {
        state.serializer = None;
    }
    let stop = |reason, obstacle| Unresolved::at(reason, row.address, entry, obstacle);
    let unsupported = |reason| stop(reason, Obstacle::Unsupported);
    if let Some((destination, entry)) = table_load(row, state) {
        let key = register(destination).ok_or_else(|| unsupported("load-destination"))?;
        state.registers.insert(key, Value::TableEntry(entry));
        return Ok(());
    }
    let args: Vec<_> = row.operands.split(',').collect();
    match (row.operation.as_str(), args.as_slice()) {
        ("cmp", [left, right]) => {
            let offset = match state.value(left) {
                Some(Value::Token) => Some(0),
                Some(Value::TokenWord(offset)) => Some(offset),
                _ => None,
            };
            state.flags = match (offset, state.value(right)) {
                (Some(offset), Some(Value::Constant(pivot))) if left.starts_with('w') => {
                    Some(Flags::Token(Comparison { offset, pivot }))
                }
                (_, Some(Value::Constant(pivot))) => {
                    state.value(left).map(|tested| Flags::Values {
                        tested,
                        pivots: vec![pivot],
                    })
                }
                _ => None,
            };
        }
        ("ccmp", [left, right, "#4", "ne"]) => {
            let previous = match state.flags.take() {
                Some(Flags::Token(Comparison { offset, pivot })) => {
                    Some((Value::TokenWord(offset), vec![pivot]))
                }
                Some(Flags::Values { tested, pivots }) => Some((tested, pivots)),
                None => None,
            };
            state.flags = match (previous, state.value(left), state.value(right)) {
                (Some((tested, mut pivots)), Some(current), Some(Value::Constant(pivot)))
                    if tested == current
                        || matches!((&tested, &current), (Value::TokenWord(0), Value::Token)) =>
                {
                    pivots.push(pivot);
                    pivots.sort();
                    pivots.dedup();
                    Some(Flags::Values { tested, pivots })
                }
                _ => None,
            };
        }
        ("madd", [destination, index, scale, base]) => {
            let value = match (state.value(index), state.value(scale), state.value(base)) {
                (Some(index), Some(Value::Constant(scale)), Some(base)) => {
                    Some(Value::SumProduct(Box::new(base), Box::new(index), scale))
                }
                _ => None,
            };
            state.assign(destination, value);
        }
        ("mov", [destination, source]) => state.assign(destination, state.value(source)),
        ("add" | "sub", [destination, left, right]) => {
            let word = destination.starts_with('w');
            let sign = if row.operation == "sub" { -1 } else { 1 };
            let value = match (state.value(left), state.value(right)) {
                (Some(Value::Token), Some(Value::Constant(n))) if word => {
                    Some(Value::TokenWord(signed_word(sign * n)))
                }
                (Some(Value::TokenWord(offset)), Some(Value::Constant(n))) if word => {
                    Some(Value::TokenWord(signed_word(offset + sign * n)))
                }
                (Some(Value::Constant(base)), Some(Value::TableEntry(entry)))
                | (Some(Value::TableEntry(entry)), Some(Value::Constant(base)))
                    if sign == 1 && !word =>
                {
                    Some(Value::TableTarget {
                        base: base as u64,
                        entry,
                        shift: 0,
                    })
                }
                (Some(base), Some(Value::Constant(n))) => offset(base, sign * n),
                _ => None,
            };
            state.assign(destination, value);
        }
        ("add", [destination, left, right, shift]) if destination.starts_with('x') => {
            let shift = shift.strip_prefix("lsl").and_then(number);
            let (Some(base), Some(index), Some(shift)) =
                (state.value(left), state.value(right), shift)
            else {
                return Err(unsupported("instruction"));
            };
            let shift = u8::try_from(shift)
                .ok()
                .filter(|shift| *shift < 64)
                .ok_or_else(|| unsupported("instruction"))?;
            let value = match (base, index) {
                (Value::Constant(base), Value::TableEntry(entry)) => Value::TableTarget {
                    base: base as u64,
                    entry,
                    shift,
                },
                (base, index) => Value::Indexed(Box::new(base), Box::new(index), shift),
            };
            state.assign(destination, Some(value));
        }
        ("ldr" | "ldrb" | "ldrsw" | "ldur" | "str" | "strb", _) => {
            let Some((operand, address)) = row.operands.split_once(',') else {
                return Err(unsupported("memory-operands"));
            };
            let Some((base, amount)) = memory(address).or_else(|| constant_index(state, address))
            else {
                return Err(unsupported("addressing"));
            };
            let location = state.value(base).and_then(|v| offset(v, amount));
            let width = if row.operation == "strb" {
                1
            } else if operand.starts_with('q') {
                16
            } else if operand.starts_with(['x', 'd']) {
                8
            } else {
                4
            };
            if row.operation.starts_with("str")
                && let Some(Value::Owner(at)) = &location
            {
                let overlaps = |slot: i64| *at < slot + 8 && at.saturating_add(width) > slot;
                if wrapped_owner.is_some_and(|(slot, _)| overlaps(slot)) {
                    return Err(unsupported("serializer-owner-overwrite"));
                }
                if input.owner_vtable.is_some() && overlaps(0) {
                    return Err(unsupported("owner-vtable-overwrite"));
                }
            }
            if row.operation.starts_with("str")
                && location
                    .as_ref()
                    .is_some_and(|at| state.overlaps_stored(at, width))
            {
                return Err(unsupported("compound-reader-overwrite"));
            }
            if row.operation.starts_with("ld") {
                let width = if row.operation == "ldrb" {
                    1
                } else if row.operation == "ldrsw" {
                    4
                } else if operand.starts_with('x') {
                    8
                } else {
                    4
                };
                // A load retains origin as a load, never as the original receiver or pointer.
                let value = location.map(|v| {
                    if let Some((slot, owner)) = wrapped_owner
                        && width == 8 && v == Value::Owner(slot)
                    {
                        return Value::Owner(owner);
                    }
                    if state.stored.is_some() && width == 8
                        && let Value::Stack(at) = &v
                        && let Some(saved) = state.saved_addresses.get(at)
                    {
                        return saved.clone();
                    }
                    if width == 4 && matches!(v, Value::Reader(offset) if Some(offset as u64) == input.reader_token_offset) {
                        Value::Token
                    } else {
                        Value::Load(Box::new(v), width)
                    }
                });
                let key = register(operand).ok_or_else(|| unsupported("load-destination"))?;
                state.registers.remove(&key);
                if let Some(value) = value {
                    state.registers.insert(key, value);
                }
            } else if let Some(Value::Stack(at)) = location {
                state.save_address(at, width, state.stack_store_value(operand));
                state.copied_tokens.clear();
                state.targets.clear();
                state.stack.clear();
                if row.operation == "str"
                    && operand.starts_with('x')
                    && let Some(value) = state.value(operand)
                {
                    state.stack.insert(at, value);
                }
            } else if matches!(location, Some(Value::Owner(_))) {
                state.emplaced.clear();
            } else {
                return Err(stop("store-destination", unestablished(state, base)));
            }
        }
        ("stp" | "ldp", _) => {
            let mut parts = row.operands.splitn(3, ',');
            let (Some(first), Some(second), Some(address)) =
                (parts.next(), parts.next(), parts.next())
            else {
                return Err(unsupported("pair-operands"));
            };
            let (base, amount, update) =
                pair_memory(address).ok_or_else(|| unsupported("addressing"))?;
            let location = state.value(base).and_then(|v| offset(v, amount));
            if row.operation == "stp"
                && location.as_ref().is_some_and(|at| {
                    state.overlaps_stored(at, if first.starts_with('x') { 16 } else { 8 })
                })
            {
                return Err(unsupported("compound-reader-overwrite"));
            }
            if !matches!(location, Some(Value::Stack(_))) {
                return Err(stop("pair-address", unestablished(state, base)));
            }
            let updated =
                update.and_then(|amount| state.value(base).and_then(|v| offset(v, amount)));
            if row.operation == "stp" {
                if let Some(Value::Stack(at)) = location {
                    let width = if first.starts_with('x') { 8 } else { 4 };
                    state.save_address(at, width, state.stack_store_value(first));
                    state.save_address(at + width, width, state.stack_store_value(second));
                }
                state.stack.clear();
                state.copied_tokens.clear();
                state.targets.clear();
            }
            if row.operation == "ldp" {
                if update.is_some() && [register(first), register(second)].contains(&register(base))
                {
                    return Err(unsupported("pair-writeback-alias"));
                }
                let restored = if state.stored.is_some() && first.starts_with('x') {
                    if let Some(Value::Stack(at)) = location {
                        (
                            state.saved_addresses.get(&at).cloned(),
                            state.saved_addresses.get(&(at + 8)).cloned(),
                        )
                    } else {
                        (None, None)
                    }
                } else {
                    (None, None)
                };
                state.assign(first, restored.0);
                state.assign(second, restored.1);
            }
            if update.is_some() {
                state.assign(base, updated);
            }
        }
        // Bit-field reads do not set flags; their result is not followed.
        ("ubfx" | "and", [destination, ..]) => state.assign(destination, None),
        ("orr", [destination, ..]) if input.bitwise_updates => state.assign(destination, None),
        ("adrp" | "adr", [destination, address]) => {
            state.assign(destination, number(address).map(Value::Constant))
        }
        ("nop", [""]) => {}
        _ => return Err(unsupported("instruction")),
    }
    Ok(())
}

/// The entry address and instructions of the root function, or `None` when the image has no
/// single root function or its code does not decode.
fn decode_root(input: &DispatchInput<'_>, root: &str) -> Option<(u64, Vec<Instruction>)> {
    let body = function(input.functions.iter().copied(), input.symbols, root)?;
    let rows = decode(body).ok()?;
    Some((body.address, rows))
}

/// Every token path through the root, and the jump tables in it that could not be decoded.
pub(super) fn explore(input: &FieldInput) -> (Vec<TokenPath>, Vec<FieldGap>) {
    explore_owner(input, &input.selection.owner_candidate)
}

pub(super) fn explore_owner(input: &FieldInput, owner: &str) -> (Vec<TokenPath>, Vec<FieldGap>) {
    let root = format!("{owner}::ReadMember(CReader&, int)");
    explore_member(
        &DispatchInput {
            key_readers: &input.key_readers,
            functions: input.functions.iter().map(FunctionView::from).collect(),
            symbols: &input.symbols,
            read_only_data: &input.read_only_data,
            reader_token_offset: None,
            member_delegates: false,
            bitwise_updates: false,
            owner_vtable: None,
            serializer_constructors: None,
        },
        &root,
    )
}

/// Bounded token paths through a proven member-reader function.
pub(crate) fn explore_member(
    input: &DispatchInput<'_>,
    root: &str,
) -> (Vec<TokenPath>, Vec<FieldGap>) {
    explore_member_with_wrapped_owner(input, root, None, 0)
}

fn explore_member_with_wrapped_owner(
    input: &DispatchInput<'_>,
    root: &str,
    wrapped_owner: Option<(i64, i64)>,
    depth: usize,
) -> (Vec<TokenPath>, Vec<FieldGap>) {
    let initial = State {
        stack: BTreeMap::new(),
        serializer: None,
        copied_tokens: BTreeSet::new(),
        targets: BTreeSet::new(),
        emplaced: BTreeSet::new(),
        stored: None,
        saved_addresses: BTreeMap::new(),
        unknown_stack_store: false,
        pc: 0,
        registers: BTreeMap::from([
            ("x0".into(), Value::Owner(0)),
            ("x1".into(), Value::Reader(0)),
            ("x2".into(), Value::Token),
            ("xzr".into(), Value::Constant(0)),
            ("sp".into(), Value::Stack(0)),
        ]),
        flags: None,
        domain: [MIN_TOKEN, MAX_TOKEN],
        conditions: vec![],
        path: vec![],
        table_case: None,
    };
    let Some((entry, rows)) = decode_root(input, root) else {
        let missing = Unresolved::new("root-function");
        return (vec![initial.finish(0, PathOutcome::Gap(missing))], vec![]);
    };
    let after_last = rows.last().map_or(entry, |row| row.address + 4);
    let indexes: BTreeMap<_, _> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| (r.address, i))
        .collect();
    let names = names_by_address(input.symbols);
    let rejects = rejection(input, &names);
    let mut pending = vec![initial];
    let mut leaves = Vec::new();
    let mut tables = Vec::new();
    let mut table_readers = Vec::new();
    let mut visited = 0;
    while let Some(mut state) = pending.pop() {
        visited += 1;
        if visited > MAX_STATES {
            let spent = |state: State| {
                let at = rows.get(state.pc).map_or(after_last, |row| row.address);
                let limit = Obstacle::Bound(Bound::States(MAX_STATES));
                state.finish(
                    0,
                    PathOutcome::Gap(Unresolved::at("state-limit", at, entry, limit)),
                )
            };
            leaves.push(spent(state));
            leaves.extend(pending.drain(..).map(spent));
            break;
        }
        loop {
            let Some(row) = rows.get(state.pc) else {
                let past =
                    Unresolved::at("end-of-function", after_last, entry, Obstacle::OutsideCode);
                leaves.push(state.finish(0, PathOutcome::Gap(past)));
                break;
            };
            let stop = |reason, obstacle| Unresolved::at(reason, row.address, entry, obstacle);
            let repeated = if state.path.len() >= MAX_PATH {
                Some(stop("step-limit", Obstacle::Bound(Bound::Steps(MAX_PATH))))
            } else if state.path.contains(&row.address) {
                Some(stop("cycle", Obstacle::Cycle))
            } else {
                None
            };
            if let Some(repeated) = repeated {
                leaves.push(state.finish(row.address, PathOutcome::Gap(repeated)));
                break;
            }
            state.pc += 1;
            state.path.push(row.address);
            let args: Vec<_> = row.operands.split(',').collect();
            if input.owner_vtable.is_some()
                && matches!(row.operation.as_str(), "blr" | "br")
                && !matches!(state.value(&row.operands), Some(Value::TableTarget { .. }))
            {
                let target = owner_virtual_target(input, &state, &row.operands);
                let name = target.and_then(|address| names.get(&address).copied().flatten());
                let outcome = call_outcome(
                    name,
                    &state,
                    rejects,
                    row.address,
                    entry,
                    row.operation == "br",
                    input,
                );
                leaves.push(state.finish(row.address, outcome));
                break;
            }
            if row.operation == "ret"
                && matches!(row.operands.as_str(), "" | "x30")
                && let Some((join, _, _)) = &state.stored
            {
                if let Some(case) = state.table_case {
                    table_readers.push((leaves.len(), case));
                }
                leaves.push(state.finish(row.address, PathOutcome::Reader(join.clone())));
                break;
            }
            if matches!(row.operation.as_str(), "b" | "bl") {
                let target = number(&row.operands).map(|a| a as u64);
                if row.operation == "b"
                    && let Some(index) = target.and_then(|a| indexes.get(&a))
                {
                    state.pc = *index;
                    continue;
                }
                if let Some(slot) = target.and_then(|target| {
                    input
                        .serializer_constructors
                        .and_then(|constructors| constructors.get(&target))
                }) {
                    if row.operation == "bl"
                        && let (Some(Value::Stack(stack)), Some(Value::Owner(owner))) =
                            (state.value("x0"), state.value("x1"))
                    {
                        state.serializer = Some((stack, *slot, owner));
                        state.clobber_call();
                        continue;
                    }
                    leaves.push(state.finish(
                        row.address,
                        PathOutcome::Reader(ReaderJoin::Missing(stop(
                            "serializer-delegate",
                            Obstacle::Call,
                        ))),
                    ));
                    break;
                }
                if let Some((stack, slot, owner)) = state.serializer {
                    let name = target.and_then(|address| names.get(&address).copied().flatten());
                    if depth < 8
                        && name.is_some_and(crate::engine::analysis::readers::is_member)
                        && state.value("x0") == Some(Value::Stack(stack))
                        && state.value("x1") == Some(Value::Reader(0))
                        && matches!(state.value("x2"), Some(Value::Token | Value::TokenWord(0)))
                    {
                        let child_input = input.with_owner_vtable(None);
                        let (children, gaps) = explore_member_with_wrapped_owner(
                            &child_input,
                            name.unwrap(),
                            Some((slot, owner)),
                            depth + 1,
                        );
                        tables.extend(gaps);
                        for child in children {
                            let Some(mut child) = super::member::inherit_path(
                                child,
                                state.domain,
                                &state.conditions,
                                &state.path,
                            ) else {
                                continue;
                            };
                            if let PathOutcome::Reader(ReaderJoin::Joined { tail, .. }) =
                                &mut child.outcome
                            {
                                *tail &= row.operation == "b";
                            }
                            leaves.push(child);
                        }
                    } else {
                        leaves.push(state.finish(
                            row.address,
                            PathOutcome::Reader(ReaderJoin::Missing(stop(
                                "serializer-delegate",
                                Obstacle::Call,
                            ))),
                        ));
                    }
                    break;
                }
                if state.stored.is_some() {
                    let touches = (0..=8)
                        .filter_map(|index| state.value(&format!("x{index}")))
                        .any(|value| state.call_can_overwrite(&value));
                    if touches || row.operation == "b" {
                        let reason = if touches {
                            "compound-reader-overwrite"
                        } else {
                            "compound-reader-return"
                        };
                        leaves.push(
                            state.finish(
                                row.address,
                                PathOutcome::Gap(stop(reason, Obstacle::Call)),
                            ),
                        );
                        break;
                    }
                    state.clobber_call();
                    continue;
                }
                if let Some(target) = target {
                    match compound_call(input.key_readers, target, &mut state) {
                        // A tail call does not return to the next instruction.
                        CompoundCall::Continue if row.operation == "b" => {
                            leaves.push(state.finish(
                                row.address,
                                PathOutcome::Gap(stop("compound-reader-return", Obstacle::Call)),
                            ));
                            break;
                        }
                        CompoundCall::Continue => continue,
                        CompoundCall::Stored {
                            kind,
                            destination,
                            repeat,
                        } => {
                            let callee = names
                                .get(&target)
                                .copied()
                                .flatten()
                                .unwrap_or_default()
                                .to_owned();
                            let join = ReaderJoin::Stored {
                                callee,
                                kind,
                                destination,
                                repeat,
                            };
                            if row.operation == "b" {
                                leaves.push(state.finish(row.address, PathOutcome::Reader(join)));
                                break;
                            }
                            let size = input.key_readers.compound_sizes[if kind
                                == crate::ReaderKind::Target
                            {
                                0
                            } else {
                                1
                            }];
                            if size <= 0 {
                                leaves.push(state.finish(
                                    row.address,
                                    PathOutcome::Gap(stop(
                                        "compound-reader-size",
                                        Obstacle::Unsupported,
                                    )),
                                ));
                                break;
                            }
                            state.stored = Some((join, destination, size));
                            state.clobber_call();
                            if kind == crate::ReaderKind::Target {
                                state.assign("x0", Some(Value::Owner(destination)));
                            }
                            continue;
                        }
                        CompoundCall::Unclassified => {}
                    }
                }
                let name = target.and_then(|a| names.get(&a).copied().flatten());
                let mut outcome = call_outcome(
                    name,
                    &state,
                    rejects,
                    row.address,
                    entry,
                    row.operation == "b",
                    input,
                );
                if target == input.key_readers.string_read
                    && state.value("x0") == Some(Value::Reader(0))
                    && let Some(destination) = emplaced_destination(input.key_readers, &state)
                {
                    outcome = PathOutcome::Reader(ReaderJoin::Stored {
                        callee: name.unwrap_or_default().into(),
                        kind: crate::ReaderKind::String,
                        destination,
                        repeat: crate::RepeatBehavior::Accumulate,
                    });
                }
                if row.operation == "bl"
                    && let PathOutcome::Reader(join @ ReaderJoin::Stored { destination, .. }) =
                        &outcome
                {
                    let size = input.key_readers.compound_sizes[2];
                    if size <= 0 {
                        leaves.push(state.finish(
                            row.address,
                            PathOutcome::Gap(stop("compound-reader-size", Obstacle::Unsupported)),
                        ));
                        break;
                    }
                    state.stored = Some((join.clone(), *destination, size));
                    state.clobber_call();
                    continue;
                }
                if let (PathOutcome::Reader(_), Some(case)) = (&outcome, state.table_case) {
                    table_readers.push((leaves.len(), case));
                }
                leaves.push(state.finish(row.address, outcome));
                break;
            }
            let branching = if let Some(condition) = row.operation.strip_prefix("b.") {
                condition_branch(condition, row, &state, &indexes, entry)
            } else if matches!(row.operation.as_str(), "cbz" | "cbnz")
                && let [tested, target] = args.as_slice()
            {
                zero_test_branch(row, tested, target, &state, &indexes, entry)
            } else if row.operation == "br" {
                table_jump(row, &state, &rows, &indexes, input.read_only_data, entry)
            } else {
                match apply(row, &mut state, entry, input, wrapped_owner) {
                    Ok(()) => continue,
                    Err(unresolved) => Branching::gap(&state, row.address, unresolved),
                }
            };
            for gap in branching.table_gaps {
                push_table_gap(&mut tables, root, gap.table, gap.why, gap.unresolved);
            }
            leaves.extend(branching.ended);
            pending.extend(branching.continued);
            break;
        }
    }
    reject_default_cases(&mut leaves, &table_readers, &mut tables, root, entry);
    leaves.sort_by(|a, b| {
        a.domain
            .cmp(&b.domain)
            .then(a.conditions.cmp(&b.conditions))
    });
    (leaves, tables)
}

/// The paths that one branch continues and ends, and the jump tables it could not decode.
#[derive(Default)]
struct Branching {
    /// The states to walk later, in the order they join the worklist.
    continued: Vec<State>,
    ended: Vec<TokenPath>,
    table_gaps: Vec<TableGap>,
}
impl Branching {
    /// A branch that ends `state` at `at` in `unresolved`.
    fn gap(state: &State, at: u64, unresolved: Unresolved) -> Self {
        Self {
            ended: vec![state.finish(at, PathOutcome::Gap(unresolved))],
            ..Self::default()
        }
    }

    /// End `state` at `at` in `unresolved`, which the undecoded jump table `table` causes.
    fn end_in_table_gap(
        &mut self,
        state: &State,
        at: u64,
        table: u64,
        why: &'static str,
        unresolved: Unresolved,
    ) {
        self.table_gaps.push(TableGap {
            table,
            why,
            unresolved: unresolved.clone(),
        });
        self.ended
            .push(state.finish(at, PathOutcome::Gap(unresolved)));
    }
}

/// A jump table that could not be decoded, and why.
struct TableGap {
    table: u64,
    why: &'static str,
    unresolved: Unresolved,
}

/// The outcome of a path that ends at a direct call to `name` at `at`. The call is the base
/// rejection when `rejects` says that `CPersistent::ReadMember` rejects and the call passes it
/// the owner, the reader and the token; otherwise it is a reader call. `entry` is the root
/// function.
fn call_outcome(
    name: Option<&str>,
    state: &State,
    rejects: bool,
    at: u64,
    entry: u64,
    tail: bool,
    input: &DispatchInput<'_>,
) -> PathOutcome {
    if input.owner_vtable.is_some()
        && name.is_some_and(|name| name.ends_with("::TryReadMember(CReader&, int)"))
        && state.registers.get("x0") == Some(&Value::Owner(0))
        && state.registers.get("x1") == Some(&Value::Reader(0))
        && matches!(
            state.registers.get("x2"),
            Some(Value::Token | Value::TokenWord(0))
        )
    {
        return PathOutcome::Reader(ReaderJoin::Joined {
            callee: name.unwrap().into(),
            arguments: state.registers.clone(),
            tail,
        });
    }
    if name == Some("CReader::ReportUnexpected()")
        && state.registers.get("x0") == Some(&Value::Reader(0))
    {
        return PathOutcome::Rejected;
    }
    if name == Some("CPersistent::ReadMember(CReader&, int)")
        && rejects
        && matches!(state.registers.get("x0"), Some(Value::Owner(_)))
        && state.registers.get("x1") == Some(&Value::Reader(0))
        && matches!(
            state.registers.get("x2"),
            Some(Value::Token | Value::TokenWord(0))
        )
    {
        PathOutcome::Rejected
    } else if name == Some("CPersistent::ReadMember(CReader&, int)") {
        PathOutcome::Reader(ReaderJoin::Missing(Unresolved::at(
            "base-rejection",
            at,
            entry,
            Obstacle::Call,
        )))
    } else {
        PathOutcome::Reader(reader_join(
            name,
            state,
            at,
            entry,
            tail,
            input.member_delegates,
            Some(input.key_readers.value_token),
        ))
    }
}

fn owner_virtual_target(input: &DispatchInput<'_>, state: &State, operand: &str) -> Option<u64> {
    let (point, pointers) = input.owner_vtable?;
    if state.registers.get("x0") != Some(&Value::Owner(0)) {
        return None;
    }
    let Value::Load(slot, 8) = state.value(operand)? else {
        return None;
    };
    let (base, offset) = match slot.as_ref() {
        Value::Offset(base, offset) => (base.as_ref(), *offset),
        base => (base, 0),
    };
    if *base != Value::Load(Box::new(Value::Owner(0)), 8) || offset < 0 {
        return None;
    }
    pointers.get(&point.checked_add(offset as u64)?).copied()
}

/// The paths on each side of the conditional branch `row`, each limited to the token
/// intervals that take that side. `entry` is the root function.
fn condition_branch(
    condition: &str,
    row: &Instruction,
    state: &State,
    indexes: &BTreeMap<u64, usize>,
    entry: u64,
) -> Branching {
    let stop = |reason, obstacle| Unresolved::at(reason, row.address, entry, obstacle);
    let target = number(&row.operands).and_then(|a| indexes.get(&(a as u64)));
    let condition = match condition {
        "cs" => "hs",
        "cc" => "lo",
        condition => condition,
    };
    if state.stored.is_some()
        && state.flags.is_none()
        && opposite(condition).is_some()
        && let Some(&target) = target
    {
        let mut taken = state.clone();
        taken.pc = target;
        taken.conditions.push(Condition {
            at: row.address,
            value: None,
            zero: false,
        });
        let mut other = state.clone();
        other.conditions.push(Condition {
            at: row.address,
            value: None,
            zero: true,
        });
        return Branching {
            continued: vec![taken, other],
            ..Branching::default()
        };
    }
    if let Some(Flags::Values { tested, pivots }) = &state.flags {
        return values_branch(
            condition,
            row,
            state,
            target.copied(),
            tested,
            pivots,
            entry,
        );
    }
    let split = match (&state.flags, opposite(condition), target) {
        (None, _, _) => Err(stop("flags", Obstacle::Unknown(Unknown::Flags))),
        (_, None, _) => Err(stop("branch-condition", Obstacle::Unsupported)),
        (_, _, None) => Err(stop("branch-target", Obstacle::OutsideCode)),
        (Some(Flags::Token(comparison)), Some(inverse), Some(&target)) => {
            let taken = intervals(state.domain, condition, *comparison);
            let not_taken = intervals(state.domain, inverse, *comparison);
            match (taken, not_taken) {
                (Some(taken), Some(not_taken)) => Ok((taken, target, not_taken)),
                _ => Err(stop("branch-condition", Obstacle::Unsupported)),
            }
        }
        _ => unreachable!("value flags handled above"),
    };
    let (taken, target, not_taken) = match split {
        Ok(split) => split,
        Err(unresolved) => return Branching::gap(state, row.address, unresolved),
    };
    let taken = taken.into_iter().map(|domain| (domain, target));
    let not_taken = not_taken.into_iter().map(|domain| (domain, state.pc));
    let mut continued = Vec::new();
    for (domain, pc) in taken.chain(not_taken) {
        let mut next = state.clone();
        next.pc = pc;
        next.domain = domain;
        continued.push(next);
    }
    Branching {
        continued,
        ..Branching::default()
    }
}

/// Conditional comparisons preserve a union of equalities, including state-dependent ones.
fn values_branch(
    condition: &str,
    row: &Instruction,
    state: &State,
    target: Option<usize>,
    tested: &Value,
    pivots: &[i64],
    entry: u64,
) -> Branching {
    let Some(target) = target else {
        return Branching::gap(
            state,
            row.address,
            Unresolved::at("branch-target", row.address, entry, Obstacle::OutsideCode),
        );
    };
    if !matches!(condition, "eq" | "ne") {
        return Branching::gap(
            state,
            row.address,
            Unresolved::at(
                "branch-condition",
                row.address,
                entry,
                Obstacle::Unsupported,
            ),
        );
    }
    if let Value::Constant(value) = tested {
        let equal = pivots.contains(value);
        let mut next = state.clone();
        next.pc = if equal == (condition == "eq") {
            target
        } else {
            state.pc
        };
        return Branching {
            continued: vec![next],
            ..Branching::default()
        };
    }
    let mut continued = Vec::new();
    let token_offset = match tested {
        Value::Token => Some(0),
        Value::TokenWord(offset) => Some(*offset),
        _ => None,
    };
    for equal in [true, false] {
        let domains = if let Some(offset) = token_offset {
            let mut remaining = vec![state.domain];
            let mut equal_domains = Vec::new();
            for &pivot in pivots {
                let comparison = Comparison { offset, pivot };
                let mut next = Vec::new();
                for domain in remaining {
                    equal_domains.extend(intervals(domain, "eq", comparison).unwrap());
                    next.extend(intervals(domain, "ne", comparison).unwrap());
                }
                remaining = next;
            }
            if equal { equal_domains } else { remaining }
        } else {
            vec![state.domain]
        };
        for domain in domains {
            let mut next = state.clone();
            next.domain = domain;
            next.pc = if equal == (condition == "eq") {
                target
            } else {
                state.pc
            };
            if token_offset.is_none() {
                next.conditions.push(Condition {
                    at: row.address,
                    value: Some(Value::EqualsAny(Box::new(tested.clone()), pivots.to_vec())),
                    zero: !equal,
                });
            }
            continued.push(next);
        }
    }
    Branching {
        continued,
        ..Branching::default()
    }
}

/// The paths on each side of the zero test `row` of the register `tested`, which branches to
/// `target`. A side that a known value cannot take is dropped. `entry` is the root function.
fn zero_test_branch(
    row: &Instruction,
    tested: &str,
    target: &str,
    state: &State,
    indexes: &BTreeMap<u64, usize>,
    entry: u64,
) -> Branching {
    let Some(&target) = number(target).and_then(|a| indexes.get(&(a as u64))) else {
        let outside = Unresolved::at("branch-target", row.address, entry, Obstacle::OutsideCode);
        return Branching::gap(state, row.address, outside);
    };
    let value = state.value(tested);
    let mut continued = Vec::new();
    for taken in [false, true] {
        let zero = taken == (row.operation == "cbz");
        if let Some(Value::Constant(value)) = &value
            && (*value == 0) != zero
        {
            continue;
        }
        let mut next = state.clone();
        next.pc = if taken { target } else { state.pc };
        next.conditions.push(Condition {
            at: row.address,
            value: value.clone(),
            zero,
        });
        continued.push(next);
    }
    Branching {
        continued,
        ..Branching::default()
    }
}

/// A path for each case of the jump through a table at `row`, and a gap when the table cannot
/// be decoded or a case cannot be followed. `entry` is the root function.
fn table_jump(
    row: &Instruction,
    state: &State,
    rows: &[Instruction],
    indexes: &BTreeMap<u64, usize>,
    data: &[DataSection],
    entry: u64,
) -> Branching {
    let stop = |reason, obstacle| Unresolved::at(reason, row.address, entry, obstacle);
    let mut branching = Branching::default();
    let (base, table_entry, shift) = match state.value(&row.operands) {
        Some(Value::TableTarget { base, entry, shift }) => (base, entry, shift),
        Some(Value::TableEntry(table_entry)) => {
            let unresolved = stop("jump-table", Obstacle::Unsupported);
            let why = "its entries are addresses";
            branching.end_in_table_gap(state, row.address, table_entry.table, why, unresolved);
            return branching;
        }
        _ => {
            let unresolved = stop("branch-value", unestablished(state, &row.operands));
            return Branching::gap(state, row.address, unresolved);
        }
    };
    let table = table_entry.table;
    let [low, high] = state.domain;
    if high - low >= MAX_TABLE_ENTRIES as i64 {
        let bound = Obstacle::Bound(Bound::TableEntries(MAX_TABLE_ENTRIES));
        let unresolved = stop("jump-table", bound);
        let why = "its index is not bounded";
        branching.end_in_table_gap(state, row.address, table, why, unresolved);
        return branching;
    }
    let guard = state.path.iter().rev().copied().find(|address| {
        indexes
            .get(address)
            .is_some_and(|&index| rows[index].operation.starts_with("b."))
    });
    let cases: Vec<_> = (low..=high)
        .map(|token| (token, case_address(token, base, table_entry, shift, data)))
        .collect();
    for (token, address) in cases {
        let mut case = state.clone();
        case.domain = [token, token];
        match address.map(|address| (address, indexes.get(&address))) {
            Some((address, Some(&pc))) => {
                case.pc = pc;
                case.table_case = Some(TableCase {
                    table,
                    address,
                    guard,
                });
                branching.continued.push(case);
            }
            Some((_, None)) => {
                let unresolved = stop("jump-table-case", Obstacle::OutsideCode);
                let why = "a case is outside the reader";
                branching.end_in_table_gap(&case, row.address, table, why, unresolved);
            }
            None => {
                let unresolved = stop("jump-table-entry", Obstacle::Unsupported);
                let why = "an entry could not be read";
                branching.end_in_table_gap(&case, row.address, table, why, unresolved);
            }
        }
    }
    branching
}

/// Turn the reader paths of jump-table default cases into gaps. `table_readers` holds the index
/// in `leaves` of each path that took a table case and reached a reader call.
///
/// The switch's default case also serves the wide token intervals that no case handles. Every
/// token has a name, so a default case may only reject; it never becomes a field. When a wide
/// interval that leaves through the table's guard does not end at the rejection, the default
/// may be past its end. A case of that table is then kept only when it joins a known reader,
/// which a default does not do.
fn reject_default_cases(
    leaves: &mut [TokenPath],
    table_readers: &[(usize, TableCase)],
    gaps: &mut Vec<FieldGap>,
    root: &str,
    entry: u64,
) {
    let wide_paths: Vec<_> = leaves
        .iter()
        .filter(|path| path.domain[0] != path.domain[1])
        .collect();
    let default_code: BTreeSet<u64> = wide_paths
        .iter()
        .flat_map(|path| path.instructions.iter().copied())
        .collect();
    let default_may_be_unseen = |guard: Option<u64>| {
        wide_paths.iter().any(|path| {
            guard.is_none_or(|guard| path.instructions.contains(&guard))
                && path.outcome != PathOutcome::Rejected
        })
    };
    let mut changed = Vec::new();
    for &(index, case) in table_readers {
        let leaf = &leaves[index];
        let joined = matches!(leaf.outcome, PathOutcome::Reader(ReaderJoin::Joined { .. }));
        let why = if default_code.contains(&case.address) {
            "its default case reaches a call other than the rejection"
        } else if !joined && default_may_be_unseen(case.guard) {
            "a case without a known reader may be the default, which an unresolved path hides"
        } else {
            continue;
        };
        let obstacle = Obstacle::Unsupported;
        let unresolved = Unresolved::at("jump-table-default", leaf.terminal, entry, obstacle);
        push_table_gap(gaps, root, case.table, why, unresolved.clone());
        changed.push((index, unresolved));
    }
    for (index, unresolved) in changed {
        leaves[index].outcome = PathOutcome::Gap(unresolved);
    }
}

enum CompoundCall {
    Continue,
    Stored {
        kind: crate::ReaderKind,
        destination: i64,
        repeat: crate::RepeatBehavior,
    },
    Unclassified,
}

fn compound_call(bindings: &super::KeyReaders, target: u64, state: &mut State) -> CompoundCall {
    let receiver = state.value("x0");
    let source = state.value("x1");
    if bindings.token_copy.contains(&target)
        && source == Some(Value::Reader(bindings.value_token))
        && let Some(Value::Stack(at)) = receiver
    {
        state.copied_tokens.clear();
        state.targets.clear();
        state.stack.clear();
        state.copied_tokens.insert(at);
    } else if bindings.target_construct.contains(&target)
        && let (Some(Value::Stack(at)), Some(Value::Stack(token))) = (&receiver, &source)
        && state.copied_tokens.contains(token)
    {
        state.copied_tokens.clear();
        state.targets.clear();
        state.stack.clear();
        state.targets.insert(*at);
    } else if bindings.target_move == Some(target)
        && let (Some(Value::Owner(destination)), Some(Value::Stack(at))) = (&receiver, &source)
        && state.targets.contains(at)
    {
        return CompoundCall::Stored {
            kind: crate::ReaderKind::Target,
            destination: *destination,
            repeat: crate::RepeatBehavior::Unknown,
        };
    } else if bindings.optional_string == Some(target)
        && let (Some(Value::Owner(destination)), Some(Value::Stack(at))) = (&receiver, &source)
        && state.stack.get(at)
            == Some(&Value::Load(
                Box::new(Value::Reader(bindings.value_token + bindings.token_text)),
                8,
            ))
    {
        return CompoundCall::Stored {
            kind: crate::ReaderKind::String,
            destination: *destination,
            repeat: crate::RepeatBehavior::Unknown,
        };
    } else if bindings.string_emplace == Some(target)
        && let Some(Value::Owner(at)) = receiver
        && source
            == Some(Value::Offset(
                Box::new(Value::Load(
                    Box::new(Value::Owner(at + bindings.array_count)),
                    4,
                )),
                1,
            ))
    {
        state.emplaced.insert(at);
    } else {
        return CompoundCall::Unclassified;
    }
    for register in 0..=18 {
        state.registers.remove(&format!("x{register}"));
    }
    state.flags = None;
    CompoundCall::Continue
}

fn emplaced_destination(bindings: &super::KeyReaders, state: &State) -> Option<i64> {
    state.emplaced.iter().copied().find(|&at| {
        state.value("x1")
            == Some(Value::Offset(
                Box::new(Value::SumProduct(
                    Box::new(Value::Load(
                        Box::new(Value::Owner(at + bindings.array_data)),
                        8,
                    )),
                    Box::new(Value::Load(
                        Box::new(Value::Owner(at + bindings.array_count)),
                        4,
                    )),
                    bindings.string_stride,
                )),
                -bindings.string_stride,
            ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::{assembler::arm64, declarations::Function};

    #[test]
    fn bitwise_updates_extend_only_the_opted_in_member_walk() {
        let root = "Owner::ReadMember(CReader&, int)";
        let symbols = vec![
            Symbol {
                name: root.into(),
                address: 0x1000,
            },
            Symbol {
                name: "CReader::Read(CString&, bool)".into(),
                address: 0x2000,
            },
        ];
        let functions = BTreeMap::from([(
            0x1000,
            Function {
                address: 0x1000,
                code: arm64!(at 0x1000;
                    ldrb w8, [x0, #16];
                    orr w8, w8, #1;
                    strb w8, [x0, #16];
                    add x8, x0, #24; // string destination after an unrelated flag update
                    mov x0, x1;
                    mov x1, x8;
                    mov w2, #0;
                    b extern 0x2000
                ),
            },
        )]);
        let readers = super::super::KeyReaders::default();
        let mut input = DispatchInput::command(&functions, &symbols, &[], 0x38, &readers);
        let (paths, _) = explore_member(&input, root);
        assert!(
            matches!(&paths[0].outcome, PathOutcome::Gap(stop) if stop.reason == "instruction")
        );
        input.bitwise_updates = true;
        let (paths, _) = explore_member(&input, root);
        assert!(
            matches!(&paths[0].outcome, PathOutcome::Reader(ReaderJoin::Joined { callee, .. })
            if callee == "CReader::Read(CString&, bool)")
        );
    }

    #[test]
    fn serializer_delegate_reads_the_constructor_wrapped_owner() {
        let root = "Owner::ReadMember(CReader&, int)";
        let member = "Serializer::ReadMember(CReader&, int)";
        let symbols = vec![
            Symbol {
                name: root.into(),
                address: 0x1000,
            },
            Symbol {
                name: "constructor".into(),
                address: 0x2000,
            },
            Symbol {
                name: member.into(),
                address: 0x3000,
            },
            Symbol {
                name: "CReader::Read(CString&, bool)".into(),
                address: 0x4000,
            },
        ];
        let constructor =
            arm64!(at 0x2000; adrp x8, extern 0x8000; add x8, x8, #0x10; stp x8, x1, [x0]; ret);
        let rows = crate::engine::analysis::decode::decode_arm64(&constructor, 0x2000).unwrap();
        let slot = super::super::serializer_owner_slot(&rows).unwrap();
        let constructors = BTreeMap::from([(0x2000, slot)]);
        for control in [
            "joined",
            "wrong-receiver",
            "non-owner",
            "overwritten",
            "slot-store",
            "slot-word-store",
            "slot-byte-store",
            "slot-alias-store",
            "adjacent-store",
        ] {
            let mut code = arm64!(at 0x1000;
                mov x20, x1;
                mov x19, x2;
                add x1, x0, #0x40;
                mov x0, sp;
                bl extern 0x2000;
                mov x0, sp;
                mov x1, x20;
                mov x2, x19;
                bl extern 0x3000;
                ret
            );
            match control {
                "wrong-receiver" => {
                    code[20..24].copy_from_slice(&arm64!(at 0x1014; add x0, sp, #0x10))
                }
                "non-owner" => code[8..12].copy_from_slice(&arm64!(at 0x1008; mov x1, x20)),
                "overwritten" => {
                    code[20..24].copy_from_slice(&arm64!(at 0x1014; str xzr, [sp, #8]))
                }
                _ => {}
            }
            let mut body = crate::engine::analysis::assembler::Arm64::at(0x3000);
            match control {
                "slot-store" => {
                    arm64!(body; str xzr, [x0, #8]);
                }
                "slot-word-store" => {
                    arm64!(body; str wzr, [x0, #12]);
                }
                "slot-byte-store" => {
                    arm64!(body; strb wzr, [x0, #15]);
                }
                "slot-alias-store" => {
                    arm64!(body; add x9, x0, #8; str xzr, [x9]);
                }
                "adjacent-store" => {
                    arm64!(body; str xzr, [x0, #16]);
                }
                _ => {}
            }
            arm64!(body; mov x8, x1; ldr x1, [x0, #8]; mov x0, x8; mov w2, #0; b extern 0x4000);
            let functions = BTreeMap::from([
                (
                    0x1000,
                    Function {
                        address: 0x1000,
                        code,
                    },
                ),
                (
                    0x3000,
                    Function {
                        address: 0x3000,
                        code: body.bytes(),
                    },
                ),
            ]);
            let readers = super::super::KeyReaders::default();
            let mut input = DispatchInput::command(&functions, &symbols, &[], 0x38, &readers);
            input.serializer_constructors = Some(&constructors);
            let (paths, gaps) = explore_member(&input, root);
            assert!(gaps.is_empty());
            assert_eq!(paths.len(), 1);
            if matches!(control, "joined" | "adjacent-store") {
                let PathOutcome::Reader(join @ ReaderJoin::Joined { tail: false, .. }) =
                    &paths[0].outcome
                else {
                    panic!("{:?}", paths[0])
                };
                assert_eq!(
                    crate::engine::analysis::readers::destination(join),
                    Some(0x40)
                );
            } else {
                assert!(
                    matches!(
                        paths[0].outcome,
                        PathOutcome::Reader(ReaderJoin::Missing(_)) | PathOutcome::Gap(_)
                    ),
                    "{control}: {:?}",
                    paths[0]
                );
            }
        }
    }

    #[test]
    fn virtual_member_requires_the_proven_owner_slot_and_arguments() {
        let root = "CExample::ReadMember(CReader&, int)";
        let member = "CExample::TryReadMember(CReader&, int)";
        let symbols = vec![
            Symbol {
                name: root.into(),
                address: 0x1000,
            },
            Symbol {
                name: member.into(),
                address: 0x2000,
            },
        ];
        let original = arm64!(at 0x1000;
            ldr x8, [x0];
            ldr x8, [x8, #0x40];
            blr x8;
            ret
        );
        for control in [
            "joined",
            "missing-point",
            "missing-pointer",
            "wrong-slot",
            "non-owner",
            "wrong-receiver",
            "offset-receiver",
            "wrong-reader",
            "wrong-token",
            "vtable-store",
            "vtable-word-store",
            "vtable-byte-store",
            "vtable-alias-store",
            "adjacent-store",
            "before-delegate",
        ] {
            let mut code = original.clone();
            match control {
                "wrong-slot" => code[4..8].copy_from_slice(&arm64!(at 0x1000; ldr x8, [x8, #0x48])),
                "non-owner" => code[..4].copy_from_slice(&arm64!(at 0x1000; ldr x8, [x1])),
                "wrong-receiver" => code
                    .splice(8..8, arm64!(at 0x1000; mov x0, x1))
                    .for_each(drop),
                "offset-receiver" => code
                    .splice(8..8, arm64!(at 0x1000; add x0, x0, #8))
                    .for_each(drop),
                "wrong-reader" => code
                    .splice(8..8, arm64!(at 0x1000; mov x1, x0))
                    .for_each(drop),
                "wrong-token" => code
                    .splice(8..8, arm64!(at 0x1000; mov w2, #7))
                    .for_each(drop),
                "vtable-store" => {
                    code.splice(0..0, arm64!(at 0x1000; str xzr, [x0]))
                        .for_each(drop);
                }
                "vtable-word-store" => {
                    code.splice(0..0, arm64!(at 0x1000; str wzr, [x0, #4]))
                        .for_each(drop);
                }
                "vtable-byte-store" => {
                    code.splice(0..0, arm64!(at 0x1000; strb wzr, [x0, #7]))
                        .for_each(drop);
                }
                "vtable-alias-store" => {
                    code.splice(0..0, arm64!(at 0x1000; mov x9, x0; str xzr, [x9]))
                        .for_each(drop);
                }
                "adjacent-store" => {
                    code.splice(0..0, arm64!(at 0x1000; str xzr, [x0, #8]))
                        .for_each(drop);
                }
                "before-delegate" => {
                    code = arm64!(at 0x1000; str xzr, [x0]; b extern 0x2000);
                }
                _ => {}
            }
            let functions = BTreeMap::from([(
                0x1000,
                Function {
                    address: 0x1000,
                    code,
                },
            )]);
            let pointers = if control == "missing-pointer" {
                BTreeMap::new()
            } else {
                BTreeMap::from([(0x8040, 0x2000)])
            };
            let key_readers = super::super::KeyReaders::default();
            let mut input = DispatchInput::command(&functions, &symbols, &[], 0x38, &key_readers);
            if control != "missing-point" {
                input.owner_vtable = Some((0x8000, &pointers));
            }
            let (paths, gaps) = explore_member(&input, root);
            assert!(gaps.is_empty());
            assert_eq!(paths.len(), 1);
            if matches!(control, "joined" | "adjacent-store") {
                assert!(
                    matches!(&paths[0].outcome, PathOutcome::Reader(ReaderJoin::Joined { callee, tail: false, .. }) if callee == member)
                );
            } else {
                assert!(
                    matches!(
                        &paths[0].outcome,
                        PathOutcome::Gap(_) | PathOutcome::Reader(ReaderJoin::Missing(_))
                    ),
                    "{control}: {:?}",
                    paths[0]
                );
            }
        }
    }

    #[test]
    fn register_widths_preserve_only_valid_provenance() {
        let mut state = State {
            stack: BTreeMap::new(),
            serializer: None,
            copied_tokens: BTreeSet::new(),
            targets: BTreeSet::new(),
            emplaced: BTreeSet::new(),
            stored: None,
            saved_addresses: BTreeMap::new(),
            unknown_stack_store: false,
            pc: 0,
            registers: BTreeMap::new(),
            flags: None,
            domain: [MIN_TOKEN, MAX_TOKEN],
            conditions: vec![],
            path: vec![],
            table_case: None,
        };
        state.assign("x8", Some(Value::Constant(0x1_0000_0007)));
        assert_eq!(state.value("w8"), Some(Value::Constant(7)));
        state.assign("w8", Some(Value::Constant(-1)));
        assert_eq!(state.value("w8"), Some(Value::Constant(-1)));
        assert_eq!(state.value("x8"), Some(Value::Constant(0xffff_ffff)));
        state.assign("x1", Some(Value::Reader(0)));
        assert_eq!(state.value("w1"), None);
        state.assign("w2", state.value("x1"));
        assert_eq!(state.value("x2"), None);
    }
    #[test]
    fn unsigned_conditions_on_a_token_word_split_at_the_signed_boundary() {
        let all = [MIN_TOKEN, MAX_TOKEN];
        let range = |offset, pivot| Comparison { offset, pivot };
        assert_eq!(intervals(all, "ls", range(-100, 2)), Some(vec![[100, 102]]));
        assert_eq!(
            intervals(all, "hi", range(-100, 2)),
            Some(vec![[103, MAX_TOKEN], [MIN_TOKEN, 99]])
        );
        assert_eq!(
            intervals(all, "hi", range(0, 10)),
            Some(vec![[11, MAX_TOKEN], [MIN_TOKEN, -1]])
        );
        assert_eq!(intervals([0, 50], "eq", range(-100, 2)), Some(vec![]));
        assert_eq!(intervals(all, "gt", range(-100, 2)), None);
        assert_eq!(
            intervals(all, "gt", range(0, 2)),
            Some(vec![[3, MAX_TOKEN]])
        );
    }
}
