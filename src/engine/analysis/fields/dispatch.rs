use super::tokens::{FunctionView, decode, function, number, register};
use super::{
    Condition, DataSection, FieldGap, FieldGapKind, FieldInput, Function, LoaderRoot, PathOutcome,
    ReaderJoin, TableEntry, TokenPath, Value,
};
use crate::engine::analysis::decode::Instruction;
use crate::engine::analysis::discovery::{Symbol, SymbolIndex};
use crate::engine::analysis::readers::{self, ConverterArgument};
use crate::engine::analysis::references::{self, ReaderForm};
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
    pub scope: Option<Value>,
    bodies: Bodies<'a>,
    key_readers: &'a super::KeyReaders,
    pub symbols: &'a [Symbol],
    /// The index of `symbols`.
    symbol_index: &'a SymbolIndex,
    pub read_only_data: &'a [DataSection],
    pub reader_token_offset: Option<u64>,
    pub member_delegates: bool,
    /// The constructor-proven owner address point and executable pointer slots.
    pub owner_vtable: Option<(u64, &'a BTreeMap<u64, u64>)>,
    /// Constructors whose body proves a wrapped owner pointer at this object offset.
    pub serializer_constructors: Option<&'a BTreeMap<u64, i64>>,
    /// The one owner-vtable target that joins as a member delegate when a tail call passes it the
    /// owner, reader and token unchanged.
    pub virtual_delegate: Option<&'a str>,
    /// A loader root to walk from its statement read instead of a member reader's entry.
    pub loader: Option<&'a LoaderRoot>,
    /// Whether the walk follows order checks. Only an owner's walk reports them, as key order
    /// notes; any other walk forks at the check, so its keys keep the order as a condition.
    pub follows_order_checks: bool,
}

/// Where dispatch finds a function body by name.
#[derive(Clone, Copy)]
enum Bodies<'a> {
    /// Bodies by entry address, named by every symbol at that address, from command discovery.
    ByAddress(&'a BTreeMap<u64, crate::engine::analysis::declarations::Function>),
    /// Named bodies from registry discovery, and the import stubs that the image binds to them.
    Named(&'a [Function], &'a BTreeMap<u64, u64>),
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
    /// Dispatch input for a command member reader. `symbol_index` is the index of `symbols`.
    pub(crate) fn command(
        functions: &'a BTreeMap<u64, crate::engine::analysis::declarations::Function>,
        symbols: &'a [Symbol],
        symbol_index: &'a SymbolIndex,
        read_only_data: &'a [DataSection],
        reader_token_offset: u64,
        key_readers: &'a super::KeyReaders,
    ) -> Self {
        Self {
            scope: None,
            key_readers,
            bodies: Bodies::ByAddress(functions),
            symbols,
            symbol_index,
            read_only_data,
            reader_token_offset: Some(reader_token_offset),
            member_delegates: true,
            owner_vtable: None,
            serializer_constructors: None,
            virtual_delegate: None,
            loader: None,
            follows_order_checks: false,
        }
    }

    /// The one function named `name`, when every symbol of that name is at its entry.
    fn function(&self, name: &str) -> Option<FunctionView<'a>> {
        let named = self.symbol_index.named(self.symbols, name);

        match self.bodies {
            Bodies::ByAddress(bodies) => {
                let views = named.clone().filter_map(|symbol| {
                    let body = bodies.get(&symbol.address)?;

                    Some(FunctionView {
                        name: &symbol.name,
                        address: body.address,
                        code: &body.code,
                    })
                });

                function(views, named, name)
            }
            // A symbol of the name at an import stub that the image binds to the body names it.
            Bodies::Named(functions, stubs) => {
                let entries = named.map(|symbol| Symbol {
                    address: stubs
                        .get(&symbol.address)
                        .copied()
                        .unwrap_or(symbol.address),
                    ..symbol.clone()
                });
                let entries: Vec<_> = entries.collect();

                function(functions.iter().map(FunctionView::from), &entries, name)
            }
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
    /// Stack strings made from the value token's text since the last stack store.
    value_strings: BTreeSet<i64>,
    /// Where the path first made a string from the value token's text. A path that then joins
    /// no reader ends there, as it did before the string idiom was followed.
    value_read: Option<Unresolved>,
    targets: BTreeSet<i64>,
    emplaced: BTreeSet<i64>,
    /// Stack words that hold the value token, as a converter's argument.
    value_token_words: BTreeSet<i64>,
    /// The converter that the path called with the value token, until its result is stored.
    converted: Option<u64>,
    /// How many conditions the path had when it called a converter with the value token.
    /// Branches after the call test the read's result or later state, not which reader reads.
    keyword_conditions: Option<usize>,
    stored: Option<(ReaderJoin, i64, i64)>,
    /// A Boolean that a reader call wrote into a stack temporary, while the path follows it to
    /// its member.
    temporary: Option<Temporary>,
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
    /// The outcome of each state comparison that the path has taken (`EqualsAny` to whether
    /// it held), until a call or a store outside the frame may change the compared memory.
    settled: BTreeMap<Value, bool>,
    /// The side that the path took at an ordered compare of an owner word, by the word, until a
    /// call or a store outside the frame may change it.
    ordered: BTreeMap<Value, OrderedSide>,
}
/// The side of an ordered compare that a path took: the branch's condition and pivots, and
/// whether the branch was taken.
#[derive(Clone, PartialEq)]
struct OrderedSide {
    condition: &'static str,
    pivots: Vec<i64>,
    taken: bool,
}
/// A reader call's Boolean in a stack temporary. The loader merges it into a bit of an owner
/// word, or copies it to an owner byte, after the call.
#[derive(Clone)]
struct Temporary {
    /// The path as it ends at the reader call. It stands when the merge is not followed.
    at_call: TokenPath,
    /// The stack offset of the temporary.
    slot: i64,
    /// Once the merge is stored: the owner offset of a copy, or `None` for a bit.
    merged: Option<Option<i64>>,
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
        self.settled.clear();
        self.ordered.clear();
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
        if let Value::ClearedBit(load, _) | Value::InsertedBit(load, _) = &value
            && operand.starts_with('w')
        {
            return matches!(load.as_ref(), Value::Load(_, width) if *width <= 4).then_some(value);
        }
        if operand.starts_with('w') {
            match value {
                Value::Constant(value) => Some(Value::Constant(value as u32 as i32 as i64)),
                Value::Token => Some(Value::Token),
                Value::TokenWord(offset) => Some(Value::TokenWord(offset)),
                Value::Converted(entry) => Some(Value::Converted(entry)),
                value @ Value::EqualsAny(..) => Some(value),
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
    /// The path as it ends at `terminal` with `outcome`. A path that stops while it follows a
    /// Boolean temporary to its member ends at the reader call instead.
    fn finish(&self, terminal: u64, outcome: PathOutcome) -> TokenPath {
        if let Some(temporary) = &self.temporary {
            return temporary.at_call.clone();
        }
        let outcome = match (outcome, &self.value_read) {
            (PathOutcome::Gap(_) | PathOutcome::Reader(ReaderJoin::Missing(_)), Some(read)) => {
                PathOutcome::Reader(ReaderJoin::Missing(read.clone()))
            }
            // A path that called a converter with the value token reached the field's reader.
            (PathOutcome::Gap(stop), _) if self.keyword_conditions.is_some() => {
                PathOutcome::Reader(ReaderJoin::Missing(stop))
            }
            (outcome, _) => outcome,
        };
        let conditions = match self.keyword_conditions {
            Some(before) => self.conditions[..before].to_vec(),
            None => self.conditions.clone(),
        };
        TokenPath {
            domain: self.domain,
            conditions,
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
                Some(Value::Converted(entry)) => Some(Value::Converted(entry)),
                Some(value @ Value::EqualsAny(..)) => Some(value),
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
        Value::Serializer(v) => v.checked_add(amount).map(Value::Serializer),
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
fn rejection(input: &DispatchInput<'_>) -> bool {
    let Some((_, rows)) = decode_root(input, "CPersistent::ReadMember(CReader&, int)") else {
        return false;
    };
    rows.len() == 2
        && rows[0].operation == "mov"
        && rows[0].operands == "x0,x1"
        && rows[1].operation == "b"
        && number(&rows[1].operands).and_then(|a| input.symbol_index.name(input.symbols, a as u64))
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
    if matches!(row.operation.as_str(), "str" | "stur" | "strb" | "stp") {
        state.serializer = None;
        if !row.operands.contains("[sp") {
            state.settled.clear();
            state.ordered.clear();
        }
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
        ("cmp", [left, right, shift @ ..]) if matches!(shift, [] | ["lsl#12"]) => {
            let offset = match state.value(left) {
                Some(Value::Token) => Some(0),
                Some(Value::TokenWord(offset)) => Some(offset),
                _ => None,
            };
            let compared = match shift {
                [] => state.value(right),
                _ => number(right).map(|immediate| Value::Constant(immediate << 12)),
            };
            state.flags = match (offset, compared) {
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
        ("ldr" | "ldrb" | "ldrsb" | "ldrsw" | "ldur" | "str" | "stur" | "strb", _) => {
            // An unscaled store writes as `str` does.
            let operation = match row.operation.as_str() {
                "stur" => "str",
                operation => operation,
            };
            let Some((operand, address)) = row.operands.split_once(',') else {
                return Err(unsupported("memory-operands"));
            };
            let Some((base, amount)) = memory(address).or_else(|| constant_index(state, address))
            else {
                return Err(unsupported("addressing"));
            };
            let location = state.value(base).and_then(|v| offset(v, amount));
            let width = if operation == "strb" {
                1
            } else if operand.starts_with('q') {
                16
            } else if operand.starts_with(['x', 'd']) {
                8
            } else {
                4
            };
            let overlaps = |at: i64, slot: i64| at < slot + 8 && at.saturating_add(width) > slot;
            if operation.starts_with("str") {
                match location {
                    Some(Value::Serializer(at))
                        if wrapped_owner.is_some_and(|(slot, _)| overlaps(at, slot)) =>
                    {
                        return Err(unsupported("serializer-owner-overwrite"));
                    }
                    Some(Value::Owner(at)) if input.owner_vtable.is_some() && overlaps(at, 0) => {
                        return Err(unsupported("owner-vtable-overwrite"));
                    }
                    _ => {}
                }
            }
            if operation.starts_with("str")
                && location
                    .as_ref()
                    .is_some_and(|at| state.overlaps_stored(at, width))
            {
                return Err(unsupported("compound-reader-overwrite"));
            }
            if operation.starts_with("ld") {
                let width = if matches!(operation, "ldrb" | "ldrsb") {
                    1
                } else if operation == "ldrsw" {
                    4
                } else if operand.starts_with('x') {
                    8
                } else {
                    4
                };
                // A load retains origin as a load, never as the original receiver or pointer.
                let value = location.map(|v| {
                    if let Some((slot, owner)) = wrapped_owner
                        && width == 8 && v == Value::Serializer(slot)
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
                state.value_strings.clear();
                state.targets.clear();
                state.stack.clear();
                state.value_token_words.clear();
                if operation == "str"
                    && operand.starts_with('x')
                    && let Some(value) = state.value(operand)
                {
                    state.stack.insert(at, value);
                }
                if operation == "str"
                    && state.value(operand) == Some(value_token_word(input.key_readers))
                {
                    state.value_token_words.insert(at);
                }
            } else if let Some(Value::Serializer(_)) = location {
                // The serializer's own members hold no owner field.
            } else if let Some(Value::Owner(at)) = location {
                state.emplaced.clear();
                if let Some(Value::EqualsAny(tested, pivots)) = state.value(operand)
                    && *tested == value_token_word(input.key_readers)
                    && let [literal] = pivots.as_slice()
                {
                    let (callee, kind) = if input.key_readers.true_token == Some(*literal) {
                        (TRUE_TOKEN_TEST, crate::ReaderKind::Boolean)
                    } else {
                        (LITERAL_TOKEN_TEST, crate::ReaderKind::Keyword)
                    };
                    let join = ReaderJoin::Stored {
                        callee: callee.into(),
                        kind,
                        destination: Some(at),
                        repeat: crate::RepeatBehavior::Replace,
                    };
                    state.stored = Some((join, at, width));
                }
                if input.key_readers.value_token != 0
                    && width == 4
                    && state.value(operand) == Some(value_token_word(input.key_readers))
                {
                    let join = ReaderJoin::Stored {
                        callee: VALUE_TOKEN_STORE.into(),
                        kind: crate::ReaderKind::Keyword,
                        destination: Some(at),
                        repeat: crate::RepeatBehavior::Replace,
                    };
                    state.stored = Some((join, at, width));
                }
                if let Some(Value::Converted(converter)) = state.value(operand) {
                    let callee = input.symbol_index.name(input.symbols, converter);
                    let join = ReaderJoin::Stored {
                        callee: callee.unwrap_or_default().into(),
                        kind: crate::ReaderKind::Keyword,
                        destination: Some(at),
                        repeat: crate::RepeatBehavior::Replace,
                    };
                    state.stored = Some((join, at, width));
                    state.converted = None;
                }
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
                state.value_strings.clear();
                state.targets.clear();
                state.value_token_words.clear();
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
        // Bit-field reads, right shifts, ORs, half-word moves, selects and floating-point moves do
        // not set flags; their result is not followed.
        ("ubfx" | "lsr" | "and" | "orr" | "movk" | "csel" | "fmov", [destination, ..]) => {
            state.assign(destination, None)
        }
        ("cset", [destination, "eq"]) if literal_token_test(state, input.key_readers).is_some() => {
            state.assign(destination, literal_token_test(state, input.key_readers))
        }
        // A bit test of a value that the walk does not follow leaves the flags unknown.
        ("tst", _) => state.flags = None,
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
    let body = input.function(root)?;
    let rows = decode(body).ok()?;
    Some((body.address, rows))
}

/// Every token path through the member reader `root` of an owner, and the jump tables in it that
/// could not be decoded. With `owner_slot`, `root` is a serializer's reader, whose receiver holds
/// a pointer to the owner at that offset. `symbol_index` is the index of the input's symbols.
pub(super) fn explore_owner(
    input: &FieldInput,
    symbol_index: &SymbolIndex,
    root: &str,
    owner_slot: Option<i64>,
) -> (Vec<TokenPath>, Vec<FieldGap>, Vec<OrderCheck>) {
    explore_member_with_wrapped_owner(
        &DispatchInput {
            scope: None,
            key_readers: &input.key_readers,
            bodies: Bodies::Named(&input.functions, &input.stubs),
            symbols: &input.symbols,
            symbol_index,
            read_only_data: &input.read_only_data,
            reader_token_offset: input.key_readers.reader_token,
            member_delegates: false,
            owner_vtable: None,
            serializer_constructors: None,
            virtual_delegate: None,
            loader: None,
            follows_order_checks: true,
        },
        root,
        owner_slot.map(|slot| (slot, 0)),
        None,
        0,
        None,
    )
}

/// Every token path through the root switch of `loader`, a function of the input, and the jump
/// tables in it that could not be decoded. A path ends at a call to the owner's constructor, which
/// joins with its arguments, or as rejected where it returns to the statement loop. Other calls
/// keep the callee-saved registers, as the ABI requires, so a scope constant held there reaches
/// the constructor.
pub(super) fn explore_loader(
    input: &FieldInput,
    symbol_index: &SymbolIndex,
    loader: &LoaderRoot,
) -> (Vec<TokenPath>, Vec<FieldGap>) {
    explore_member(
        &DispatchInput {
            scope: None,
            key_readers: &input.key_readers,
            bodies: Bodies::Named(&input.functions, &input.stubs),
            symbols: &input.symbols,
            symbol_index,
            read_only_data: &input.read_only_data,
            reader_token_offset: Some(loader.token_offset),
            member_delegates: false,
            owner_vtable: None,
            serializer_constructors: None,
            virtual_delegate: None,
            loader: Some(loader),
            follows_order_checks: false,
        },
        &loader.function,
    )
}

/// Bounded token paths through a proven member-reader function.
pub(crate) fn explore_member(
    input: &DispatchInput<'_>,
    root: &str,
) -> (Vec<TokenPath>, Vec<FieldGap>) {
    let (paths, gaps, _) = explore_member_with_wrapped_owner(input, root, None, None, 0, None);
    (paths, gaps)
}

/// The token paths through `root`. With `declined`, a path that returns false there instead,
/// so that the caller of an inherited reader goes on after its call. With `delegated_receiver`,
/// `root` receives it in `x0` instead of the owner: the subobject of an inherited reader, or the
/// member of a value reader ([`readers::is_member_value_reader`]), which reads the token from the
/// reader in `x1`.
fn explore_member_with_wrapped_owner(
    input: &DispatchInput<'_>,
    root: &str,
    wrapped_owner: Option<(i64, i64)>,
    delegated_receiver: Option<Value>,
    depth: usize,
    mut declined: Option<&mut Vec<TokenPath>>,
) -> (Vec<TokenPath>, Vec<FieldGap>, Vec<OrderCheck>) {
    let mut initial = State {
        stack: BTreeMap::new(),
        serializer: None,
        copied_tokens: BTreeSet::new(),
        value_strings: BTreeSet::new(),
        value_read: None,
        targets: BTreeSet::new(),
        emplaced: BTreeSet::new(),
        value_token_words: BTreeSet::new(),
        converted: None,
        keyword_conditions: None,
        stored: None,
        temporary: None,
        saved_addresses: BTreeMap::new(),
        unknown_stack_store: false,
        pc: 0,
        registers: BTreeMap::from([
            ("x0".into(), receiver(wrapped_owner)),
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
        settled: BTreeMap::new(),
        ordered: BTreeMap::new(),
    };
    if root.ends_with(", EScopeType)")
        && let Some(scope) = &input.scope
    {
        initial.registers.insert("x3".into(), scope.clone());
    }
    if let Some(receiver) = delegated_receiver {
        initial.registers.insert("x0".into(), receiver);
        if readers::is_member_value_reader(root) {
            initial.registers.remove("x2");
        }
    }
    let Some((entry, rows)) = decode_root(input, root) else {
        let missing = Unresolved::new("root-function");
        return (
            vec![initial.finish(0, PathOutcome::Gap(missing))],
            vec![],
            vec![],
        );
    };
    let after_last = rows.last().map_or(entry, |row| row.address + 4);
    let indexes: BTreeMap<_, _> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| (r.address, i))
        .collect();
    if let Some(loader) = input.loader {
        let Some(&start) = indexes.get(&loader.start) else {
            let missing = Unresolved::new("loader-start");
            return (
                vec![initial.finish(0, PathOutcome::Gap(missing))],
                vec![],
                vec![],
            );
        };
        initial.pc = start;
        initial.registers = BTreeMap::from([
            (loader.reader.clone(), Value::Reader(0)),
            ("xzr".into(), Value::Constant(0)),
            ("sp".into(), Value::Stack(0)),
        ]);
    }
    let rejects = rejection(input);
    let mut pending = vec![initial];
    let mut leaves = Vec::new();
    let mut tables = Vec::new();
    let mut table_readers = Vec::new();
    let mut order_checks = Vec::new();
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
                if let (Some(_), Some(case)) = (&state.temporary, state.table_case) {
                    table_readers.push((leaves.len(), case));
                }
                leaves.push(state.finish(0, PathOutcome::Gap(past)));
                break;
            };
            let stop = |reason, obstacle| Unresolved::at(reason, row.address, entry, obstacle);
            // The statement loop reads the next statement: this one built no owner on the path.
            if input
                .loader
                .is_some_and(|loader| !state.path.is_empty() && row.address <= loader.start)
            {
                leaves.push(state.finish(row.address, PathOutcome::Rejected));
                break;
            }
            let repeated = if state.path.len() >= MAX_PATH {
                Some(stop("step-limit", Obstacle::Bound(Bound::Steps(MAX_PATH))))
            } else if state.path.contains(&row.address) {
                Some(stop("cycle", Obstacle::Cycle))
            } else {
                None
            };
            if let Some(repeated) = repeated {
                if let (Some(_), Some(case)) = (&state.temporary, state.table_case) {
                    table_readers.push((leaves.len(), case));
                }
                leaves.push(state.finish(row.address, PathOutcome::Gap(repeated)));
                break;
            }
            state.pc += 1;
            state.path.push(row.address);
            if let Some(mut temporary) = state.temporary.take() {
                let path = match follow_temporary(row, &mut state, &mut temporary, &indexes) {
                    Some(TemporaryStep::Next) => {
                        state.temporary = Some(temporary);
                        continue;
                    }
                    Some(TemporaryStep::Merged(join)) => {
                        state.finish(row.address, PathOutcome::Reader(join))
                    }
                    None => temporary.at_call,
                };
                if let Some(case) = state.table_case {
                    table_readers.push((leaves.len(), case));
                }
                leaves.push(path);
                break;
            }
            let args: Vec<_> = row.operands.split(',').collect();
            if input.loader.is_some() && row.operation == "blr" {
                state.clobber_call();
                continue;
            }
            if input.owner_vtable.is_some()
                && matches!(row.operation.as_str(), "blr" | "br")
                && !matches!(state.value(&row.operands), Some(Value::TableTarget { .. }))
            {
                let target = owner_virtual_target(input, &state, &row.operands);
                let name =
                    target.and_then(|address| input.symbol_index.name(input.symbols, address));
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
                && state.value("x0") == Some(Value::Constant(0))
                && let Some(declined) = declined.as_deref_mut()
            {
                declined.push(state.finish(row.address, PathOutcome::Rejected));
                break;
            }
            // A converted value that is never stored leaves an earlier id store as the read.
            if row.operation == "ret" && state.converted.is_some() && state.stored.is_none() {
                let unstored = stop("keyword-store", Obstacle::Unsupported);
                leaves.push(state.finish(row.address, PathOutcome::Gap(unstored)));
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
                if let Some(loader) = input.loader {
                    if target.is_some_and(|target| loader.constructors.contains(&target)) {
                        let callee = target
                            .and_then(|address| input.symbol_index.name(input.symbols, address))
                            .unwrap_or_default();
                        let join = ReaderJoin::Joined {
                            callee: callee.into(),
                            arguments: state.registers.clone(),
                            tail: row.operation == "b",
                        };
                        leaves.push(state.finish(row.address, PathOutcome::Reader(join)));
                        break;
                    }
                    if row.operation == "bl" {
                        state.clobber_call();
                        continue;
                    }

                    // A tail call to another function leaves the loader: no owner on this path.
                    leaves.push(state.finish(
                        row.address,
                        PathOutcome::Gap(stop("loader-tail-call", Obstacle::Call)),
                    ));
                    break;
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
                    let name =
                        target.and_then(|address| input.symbol_index.name(input.symbols, address));
                    if depth < 8
                        && name.is_some_and(crate::engine::analysis::readers::is_member)
                        && state.value("x0") == Some(Value::Stack(stack))
                        && state.value("x1") == Some(Value::Reader(0))
                        && matches!(state.value("x2"), Some(Value::Token | Value::TokenWord(0)))
                    {
                        let child_input = input.with_owner_vtable(None);
                        let (children, gaps, checks) = explore_member_with_wrapped_owner(
                            &child_input,
                            name.unwrap(),
                            Some((slot, owner)),
                            None,
                            depth + 1,
                            None,
                        );
                        tables.extend(gaps);
                        order_checks.extend(checks);
                        leaves.extend(continued(children, &state, row.operation == "b"));
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
                let callee =
                    target.and_then(|address| input.symbol_index.name(input.symbols, address));
                let inherited = match (callee, state.value("x0")) {
                    (Some(name), _)
                        if readers::is_inheritable_member(name)
                            && forwards_member_arguments(&state) =>
                    {
                        Some((name, None))
                    }
                    // A base class's reader on its subobject at a constant offset in the owner.
                    (Some(name), Some(base @ Value::Owner(offset)))
                        if readers::is_inheritable_member(name)
                            && offset > 0
                            && forwards_reader_and_token(&state) =>
                    {
                        Some((name, Some(base)))
                    }
                    (Some(name), Some(member @ Value::Owner(_)))
                        if readers::is_member_value_reader(name)
                            && state.value("x1") == Some(Value::Reader(0)) =>
                    {
                        Some((name, Some(member)))
                    }
                    _ => None,
                };
                // The owner's inherited reader, or a value reader of one of its members, handles
                // the tokens that this reader passes on.
                if let Some((inherited, value_receiver)) = inherited
                    && state.stored.is_none()
                    && !input.member_delegates
                    && depth < 8
                    && input.function(inherited).is_some()
                {
                    let returns = row.operation == "bl";
                    // A tail call returns the inherited reader's false to this reader's caller.
                    let mut inherited_declined = Vec::new();
                    let collects = returns || declined.is_some();
                    // A serializer's inherited reader keeps its receiver; an owner's has none.
                    let inherited_wrapped = wrapped_owner
                        .filter(|_| state.registers.get("x0") == Some(&Value::Serializer(0)));
                    let (children, gaps, checks) = explore_member_with_wrapped_owner(
                        input,
                        inherited,
                        inherited_wrapped,
                        value_receiver,
                        depth + 1,
                        collects.then_some(&mut inherited_declined),
                    );
                    tables.extend(gaps);
                    order_checks.extend(checks);
                    leaves.extend(continued(children, &state, !returns));
                    let inherited_declined = continued(inherited_declined, &state, false);
                    if !returns {
                        if let Some(declined) = declined.as_deref_mut() {
                            declined.extend(inherited_declined);
                        }
                        break;
                    }
                    for path in inherited_declined {
                        let mut after = state.clone();
                        after.domain = path.domain;
                        after.conditions = path.conditions;
                        after.path = path.instructions;
                        after.clobber_call();
                        after.assign("x0", Some(Value::Constant(0)));
                        pending.push(after);
                    }
                    break;
                }
                // A converter of the value token goes on past a store of its id. The id store
                // stays the path's read until the converted value is stored.
                if row.operation == "bl"
                    && let Some(target) = target
                    && state.stored.as_ref().is_none_or(stores_value_token_id)
                    && converts_value_token(input, target, &state)
                {
                    state.clobber_call();
                    state.assign("x0", Some(Value::Converted(target)));
                    state.converted = Some(target);
                    state.keyword_conditions = Some(state.conditions.len());
                    continue;
                }
                let harmless = target
                    .and_then(|address| input.symbol_index.name(input.symbols, address))
                    .is_some_and(readers::changes_no_script_state);
                if state.stored.is_some() && harmless && row.operation == "bl" {
                    state.clobber_call();
                    continue;
                }
                // A diagnostic about a converted value changes no script state, so the path goes
                // on to the value's store. The reader's own reports stay rejections.
                if row.operation == "bl"
                    && state.converted.is_some()
                    && target
                        .and_then(|address| input.symbol_index.name(input.symbols, address))
                        .is_some_and(|name| {
                            readers::changes_no_script_state(name)
                                && !name.starts_with("CReader::Report")
                        })
                {
                    state.clobber_call();
                    continue;
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
                let callee =
                    target.and_then(|address| input.symbol_index.name(input.symbols, address));
                let text = value_text(input.key_readers);
                if row.operation == "bl"
                    && callee == Some(readers::TEXT_LENGTH)
                    && state.value("x0") == Some(text.clone())
                    && assigns_next(&rows, state.pc, &indexes, input)
                {
                    // A path that measures the text and then joins no reader ends here, as it did
                    // before the assignment was followed.
                    state
                        .value_read
                        .get_or_insert_with(|| stop("reader-routing", Obstacle::Call));
                    state.clobber_call();
                    state.assign("x0", Some(Value::TextLength(Box::new(text))));
                    continue;
                }
                if let Some(callee) = callee
                    && readers::assigns_string_text(callee)
                    && let Some(Value::Owner(destination)) = state.value("x0")
                    && state.value("x1") == Some(text.clone())
                    && state.value("x2") == Some(Value::TextLength(Box::new(text)))
                {
                    let join = ReaderJoin::Stored {
                        callee: callee.into(),
                        kind: crate::ReaderKind::String,
                        destination: Some(destination),
                        repeat: crate::RepeatBehavior::Replace,
                    };
                    if row.operation == "b" {
                        leaves.push(state.finish(row.address, PathOutcome::Reader(join)));
                        break;
                    }
                    state.stored = Some((join, destination, STANDARD_STRING_SIZE));
                    state.clobber_call();
                    continue;
                }
                if let Some(target) = target {
                    let call = compound_call(input.key_readers, target, &mut state);
                    if matches!(call, CompoundCall::Continue)
                        && input.key_readers.string_from_text.contains(&target)
                    {
                        state
                            .value_read
                            .get_or_insert_with(|| stop("reader-routing", Obstacle::Call));
                    }
                    match call {
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
                            let callee = input
                                .symbol_index
                                .name(input.symbols, target)
                                .unwrap_or_default()
                                .to_owned();
                            let join = ReaderJoin::Stored {
                                callee,
                                kind,
                                destination: Some(destination),
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
                let name = target.and_then(|a| input.symbol_index.name(input.symbols, a));
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
                        destination: Some(destination),
                        repeat: crate::RepeatBehavior::Accumulate,
                    });
                }
                if let Some(name) = name
                    && passes_value_string(&state)
                {
                    outcome = PathOutcome::Reader(ReaderJoin::Stored {
                        callee: name.into(),
                        kind: crate::ReaderKind::String,
                        destination: None,
                        repeat: crate::RepeatBehavior::Unknown,
                    });
                }
                if row.operation == "bl"
                    && let PathOutcome::Reader(
                        join @ ReaderJoin::Stored {
                            destination: Some(destination),
                            ..
                        },
                    ) = &outcome
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
                if row.operation == "bl"
                    && let Some(slot) = boolean_temporary(&outcome)
                {
                    let at_call = state.finish(row.address, outcome);
                    state.clobber_call();
                    // A register loaded from the temporary before the call holds the old value.
                    // A register that holds its address, such as `sp`, still reaches the new one.
                    state.registers.retain(|_, value| !loads_stack(value, slot));
                    state.temporary = Some(Temporary {
                        at_call,
                        slot,
                        merged: None,
                    });
                    continue;
                }
                // An immediate reference reader returns its item: a tail call gives the item to
                // the loader's caller, and a store after the call is the path's last write.
                if name.is_some_and(returns_item)
                    && let PathOutcome::Reader(ReaderJoin::Joined { tail, .. }) = &mut outcome
                {
                    *tail = row.operation == "bl"
                        && stores_item_and_returns(&rows, state.pc, &indexes, &state);
                }
                if row.operation == "bl"
                    && target == input.key_readers.string_read
                    && returns_through_empty_call(&rows[state.pc..], input)
                    && let PathOutcome::Reader(ReaderJoin::Joined { tail, .. }) = &mut outcome
                {
                    *tail = true;
                }
                if let (PathOutcome::Reader(_), Some(case)) = (&outcome, state.table_case) {
                    table_readers.push((leaves.len(), case));
                }
                leaves.push(state.finish(row.address, outcome));
                break;
            }
            // A side that only calls a function that never returns, such as the stack guard's
            // failure, reads no field: an unknown condition takes the other side.
            if row.operation.starts_with("b.")
                && state.flags.is_none()
                && let Some(&target) = number(&row.operands).and_then(|a| indexes.get(&(a as u64)))
            {
                if aborts(&rows, target, input) {
                    continue;
                }
                if aborts(&rows, state.pc, input) {
                    state.pc = target;
                    continue;
                }
            }
            if let Some(settled) =
                settled_comparison(row, &state, &rows, &indexes, input, depth == 0)
            {
                state.pc = settled.side;
                state.settled.insert(settled.compared, settled.equal);
                order_checks.extend(settled.check);
                continue;
            }
            let branching = if let Some(condition) = row.operation.strip_prefix("b.") {
                condition_branch(condition, row, &state, &indexes, entry)
            } else if matches!(row.operation.as_str(), "cbz" | "cbnz")
                && let [tested, target] = args.as_slice()
            {
                zero_test_branch(row, tested, target, &state, &indexes, entry)
            } else if matches!(row.operation.as_str(), "tbz" | "tbnz")
                && let [tested, bit, target] = args.as_slice()
            {
                bit_test_branch(row, tested, bit, target, &state, &indexes, entry)
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
    (leaves, tables, order_checks)
}

/// Whether the code after a read (`continuation`) only restores registers and the frame, then
/// returns through a tail call to a function whose body only returns. Such a continuation writes
/// no memory, so the read is the path's last write to its destination, as a tail-called read is.
fn returns_through_empty_call(continuation: &[Instruction], input: &DispatchInput<'_>) -> bool {
    for row in continuation {
        match row.operation.as_str() {
            "mov" => {}
            "ldp" | "ldr" if row.operands.contains("[sp") => {}
            "add" if row.operands.starts_with("sp,sp,") => {}
            "b" => {
                return number(&row.operands)
                    .and_then(|address| input.symbol_index.name(input.symbols, address as u64))
                    .and_then(|name| input.function(name))
                    .and_then(|body| decode(body).ok())
                    .is_some_and(
                        |rows| matches!(rows.as_slice(), [only] if only.operation == "ret"),
                    );
            }
            _ => return false,
        }
    }

    false
}

/// Whether `callee` looks up one key while it reads and returns the item in `x0`.
fn returns_item(callee: &str) -> bool {
    references::reader(callee).is_some_and(|reader| reader.form == ReaderForm::Immediate)
}

/// Whether the code from `rows[next]` stores the item that an immediate reference reader returns
/// in `x0` to an owner member, then only restores the frame and returns, following local
/// branches. The store is then the path's last write, as a tail-called read is.
fn stores_item_and_returns(
    rows: &[Instruction],
    next: usize,
    indexes: &BTreeMap<u64, usize>,
    state: &State,
) -> bool {
    let Some(store) = rows.get(next) else {
        return false;
    };
    // The reader may overwrite the caller-saved registers, so only a preserved owner base holds.
    let mut after_call = state.clone();
    after_call.clobber_call();
    let stores_item = store.operation == "str"
        && store
            .operands
            .split_once(',')
            .and_then(|(source, address)| Some((source, memory(address)?)))
            .is_some_and(|(source, (base, _))| {
                source == "x0" && matches!(after_call.value(base), Some(Value::Owner(_)))
            });
    if !stores_item {
        return false;
    }

    let mut at = next + 1;
    // A bound on the steps keeps a branch cycle from looping.
    for _ in 0..32 {
        let Some(row) = rows.get(at) else {
            return false;
        };
        match row.operation.as_str() {
            "ret" => return true,
            "ldp" | "ldr" if row.operands.contains("[sp") => at += 1,
            "add" if row.operands.starts_with("sp,sp,") => at += 1,
            "b" => {
                let Some(&index) =
                    number(&row.operands).and_then(|target| indexes.get(&(target as u64)))
                else {
                    return false;
                };
                at = index;
            }
            _ => return false,
        }
    }

    false
}

/// The stack offset that a joined `CReader::Read(bool&)` writes, when its destination is a stack
/// temporary.
fn boolean_temporary(outcome: &PathOutcome) -> Option<i64> {
    let PathOutcome::Reader(ReaderJoin::Joined {
        callee, arguments, ..
    }) = outcome
    else {
        return None;
    };
    match arguments.get("x1") {
        Some(Value::Stack(slot)) if callee == BOOLEAN_READ => Some(*slot),
        _ => None,
    }
}

const BOOLEAN_READ: &str = "CReader::Read(bool&)";

/// Whether `value` is the stack address `slot` or derives from it.
fn mentions_stack(value: &Value, slot: i64) -> bool {
    match value {
        Value::Stack(at) => *at == slot,
        Value::Load(base, _) | Value::Offset(base, _) => mentions_stack(base, slot),
        Value::ClearedBit(load, _) | Value::InsertedBit(load, _) => mentions_stack(load, slot),
        Value::Indexed(base, index, _) | Value::SumProduct(base, index, _) => {
            mentions_stack(base, slot) || mentions_stack(index, slot)
        }
        _ => false,
    }
}

/// Whether `value` derives from a load at the stack address `slot`, not only from the address.
fn loads_stack(value: &Value, slot: i64) -> bool {
    match value {
        Value::Load(base, _) => mentions_stack(base, slot),
        Value::Offset(base, _) => loads_stack(base, slot),
        Value::ClearedBit(load, _) | Value::InsertedBit(load, _) => loads_stack(load, slot),
        Value::Indexed(base, index, _) | Value::SumProduct(base, index, _) => {
            loads_stack(base, slot) || loads_stack(index, slot)
        }
        _ => false,
    }
}

/// What a path does at one instruction after a Boolean reader call into a stack temporary.
enum TemporaryStep {
    /// Go on to the next instruction.
    Next,
    /// The path returns after the merge; the join holds what the merge established.
    Merged(ReaderJoin),
}

/// Follow `row` on the way from the reader call to the member. The loader loads the temporary
/// byte and the owner word, clears one bit of the word, ORs the byte in at that bit and stores the
/// word; or it stores the byte itself. The bit gets the reader's value, so a second occurrence
/// replaces the first. `None` when the row is anything else: the path then ends at the call.
fn follow_temporary(
    row: &Instruction,
    state: &mut State,
    temporary: &mut Temporary,
    indexes: &BTreeMap<u64, usize>,
) -> Option<TemporaryStep> {
    let args: Vec<_> = row.operands.split(',').collect();
    let temporary_byte = Value::Load(Box::new(Value::Stack(temporary.slot)), 1);
    match (row.operation.as_str(), args.as_slice()) {
        ("ldrb" | "ldrh" | "ldr", [destination, ..]) if temporary.merged.is_none() => {
            let (_, address) = row.operands.split_once(',')?;
            let (base, amount) = memory(address)?;
            let width = access_width(&row.operation, destination);
            let location = offset(state.value(base)?, amount)?;
            if !matches!(location, Value::Owner(_)) && location != Value::Stack(temporary.slot) {
                return None;
            }
            state.registers.insert(
                register(destination)?,
                Value::Load(Box::new(location), width),
            );
        }
        ("and", [destination, source, mask]) => {
            let Some(Value::Load(word, width)) = state.value(source) else {
                return None;
            };
            let bit = cleared_bit(mask, width)?;
            if !matches!(word.as_ref(), Value::Owner(_)) {
                return None;
            }
            let cleared = Value::ClearedBit(Box::new(Value::Load(word, width)), bit);
            state.registers.insert(register(destination)?, cleared);
        }
        ("orr", [destination, word, byte, shift @ ..]) => {
            let Some(Value::ClearedBit(load, bit)) = state.value(word) else {
                return None;
            };
            let shifted_by = match shift {
                [] => 0,
                [shift] => number(shift.strip_prefix("lsl")?)?,
                _ => return None,
            };
            if state.value(byte) != Some(temporary_byte) || shifted_by != i64::from(bit) {
                return None;
            }
            state
                .registers
                .insert(register(destination)?, Value::InsertedBit(load, bit));
        }
        ("strb" | "strh" | "str", [source, ..]) if temporary.merged.is_none() => {
            let (_, address) = row.operands.split_once(',')?;
            let (base, amount) = memory(address)?;
            let Value::Owner(at) = offset(state.value(base)?, amount)? else {
                return None;
            };
            let width = access_width(&row.operation, source);
            temporary.merged = match state.value(source)? {
                Value::InsertedBit(load, _)
                    if *load == Value::Load(Box::new(Value::Owner(at)), width) =>
                {
                    Some(None)
                }
                value if value == temporary_byte => Some(Some(at)),
                _ => return None,
            };
        }
        ("b", [target]) => state.pc = *indexes.get(&(number(target)? as u64))?,
        // A restore that moves the stack pointer comes only after the merge, when no load of the
        // temporary follows.
        ("ldp", [first, second, address, ..])
            if address.starts_with("[sp")
                && (temporary.merged.is_some() || !moves_stack_pointer(&row.operands)) =>
        {
            state.assign(first, None);
            state.assign(second, None);
        }
        ("add", [destination, "sp", amount]) if *destination == "sp" => {
            let restored = offset(state.value("sp")?, number(amount)?)?;
            state.assign("sp", Some(restored));
        }
        ("ret", [""]) => {
            let destination = temporary.merged?;
            return Some(TemporaryStep::Merged(ReaderJoin::Stored {
                callee: BOOLEAN_READ.into(),
                kind: crate::ReaderKind::Boolean,
                destination,
                repeat: crate::RepeatBehavior::Replace,
            }));
        }
        _ => return None,
    }

    Some(TemporaryStep::Next)
}

/// Whether the pair access in `operands` writes its address back to the base register.
fn moves_stack_pointer(operands: &str) -> bool {
    operands.ends_with('!') || operands.contains("],")
}

/// The bytes that a load or store moves.
fn access_width(operation: &str, register: &str) -> u8 {
    match operation {
        "ldrb" | "strb" => 1,
        "ldrh" | "strh" => 2,
        _ if register.starts_with('x') => 8,
        _ => 4,
    }
}

/// The one bit of a `width`-byte word that the AND immediate `mask` clears, when it clears exactly
/// one.
fn cleared_bit(mask: &str, width: u8) -> Option<u8> {
    let mask = u64::from_str_radix(mask.strip_prefix("#0x")?, 16).ok()?;
    let bits = u32::from(width) * 8;
    let word = if bits == 64 {
        u64::MAX
    } else {
        (1 << bits) - 1
    };
    let cleared = !mask & word;

    (cleared.count_ones() == 1).then(|| cleared.trailing_zeros() as u8)
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
    if let Some(delegate) = name.filter(|&name| tail && Some(name) == input.virtual_delegate)
        && forwards_member_arguments(state)
    {
        return PathOutcome::Reader(ReaderJoin::Joined {
            callee: delegate.into(),
            arguments: state.registers.clone(),
            tail,
        });
    }
    if input.owner_vtable.is_some()
        && name.is_some_and(|name| name.ends_with("::TryReadMember(CReader&, int)"))
        && forwards_member_arguments(state)
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
        && matches!(
            state.registers.get("x0"),
            Some(Value::Owner(_) | Value::Serializer(0))
        )
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
        // Offset 0 is the reader itself. A default `KeyReaders` leaves the value token unbound.
        let value_token = Some(input.key_readers.value_token).filter(|&offset| offset != 0);
        PathOutcome::Reader(reader_join(
            name,
            state,
            at,
            entry,
            tail,
            input.member_delegates,
            value_token,
        ))
    }
}

/// The paths of a member reader that `state` calls, limited to its token domain and preceded by
/// its conditions and instructions. A reader join is a tail call, and a stored read keeps its
/// repeat behavior, only when the call is a tail call.
fn continued(
    children: Vec<TokenPath>,
    state: &State,
    tail_call: bool,
) -> impl Iterator<Item = TokenPath> {
    children.into_iter().filter_map(move |child| {
        let mut child =
            super::member::inherit_path(child, state.domain, &state.conditions, &state.path)?;
        match &mut child.outcome {
            PathOutcome::Reader(ReaderJoin::Joined { tail, .. }) => *tail &= tail_call,
            // The caller's continuation after the call may change the destination again.
            PathOutcome::Reader(ReaderJoin::Stored { repeat, .. }) if !tail_call => {
                *repeat = crate::RepeatBehavior::Unknown;
            }
            _ => {}
        }

        Some(child)
    })
}

/// The receiver of a member reader: the owner, or the serializer that holds it.
fn receiver(wrapped_owner: Option<(i64, i64)>) -> Value {
    match wrapped_owner {
        Some(_) => Value::Serializer(0),
        None => Value::Owner(0),
    }
}

/// Whether `x0` to `x2` still hold the member reader's receiver, reader and token.
fn forwards_member_arguments(state: &State) -> bool {
    matches!(
        state.registers.get("x0"),
        Some(Value::Owner(0) | Value::Serializer(0))
    ) && forwards_reader_and_token(state)
}

/// Whether `x1` and `x2` still hold the member reader's reader and token.
fn forwards_reader_and_token(state: &State) -> bool {
    state.registers.get("x1") == Some(&Value::Reader(0))
        && matches!(
            state.registers.get("x2"),
            Some(Value::Token | Value::TokenWord(0))
        )
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

/// An order check: a test of an owner word, before the path reads its key's value, whose one side
/// logs a diagnostic and returns whatever the token. The walk takes the other side, so a key's
/// read conditions do not hold the order; the check's tested value and the paths through it state
/// the order instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OrderCheck {
    /// The branch of the check.
    pub at: u64,
    /// The state value that the check compares.
    pub tested: Value,
}

/// A state comparison that a branch decides without a fork.
struct Settled {
    /// The comparison, as an `EqualsAny` value.
    compared: Value,
    /// Whether the compared value equals a pivot on the side taken.
    equal: bool,
    /// The row index of the side taken.
    side: usize,
    /// The order check that the branch passes, when it is one.
    check: Option<OrderCheck>,
}

/// How the conditional branch `row` decides a state comparison without a fork: an earlier
/// comparison of the same state on the path settled it, or one side only logs and returns.
/// `None` when the branch forks as usual.
fn settled_comparison(
    row: &Instruction,
    state: &State,
    rows: &[Instruction],
    indexes: &BTreeMap<u64, usize>,
    input: &DispatchInput<'_>,
    root_reader: bool,
) -> Option<Settled> {
    let condition = row.operation.strip_prefix("b.")?;
    let Some(Flags::Values { tested, pivots }) = &state.flags else {
        return None;
    };
    if !matches!(condition, "eq" | "ne") || !is_owner_word(tested) {
        return None;
    }
    let target = *number(&row.operands).and_then(|a| indexes.get(&(a as u64)))?;
    let (equal_side, unequal_side) = if condition == "eq" {
        (target, state.pc)
    } else {
        (state.pc, target)
    };
    let side = |equal| if equal { equal_side } else { unequal_side };
    let compared = Value::EqualsAny(Box::new(tested.clone()), pivots.clone());

    if let Some(&equal) = state.settled.get(&compared) {
        return Some(Settled {
            side: side(equal),
            compared,
            equal,
            check: None,
        });
    }

    // A delegated or wrapped child starts with a fresh state that does not show the parent's
    // reads, so only the root reader follows order checks.
    if !input.follows_order_checks || !root_reader || !reads_nothing_yet(state, rows, indexes) {
        return None;
    }
    let rejects = |side| logs_and_returns(rows, side, indexes, input);
    let equal = match (rejects(equal_side), rejects(unequal_side)) {
        (true, false) => false,
        (false, true) => true,
        _ => return None,
    };
    let check = OrderCheck {
        at: row.address,
        tested: tested.clone(),
    };

    Some(Settled {
        side: side(equal),
        compared,
        equal,
        check: Some(check),
    })
}

/// Whether `value` is a word that the walk loaded from the owner. Only such a word is settled:
/// a call or a store outside the frame is the only way the walk can see it change.
fn is_owner_word(value: &Value) -> bool {
    matches!(value, Value::Load(word, _) if matches!(word.as_ref(), Value::Owner(_)))
}

/// Whether the path has not yet read, converted or stored anything: every instruction so far is
/// in this function and is neither a call nor a store outside the frame, and no reader idiom has
/// started.
fn reads_nothing_yet(state: &State, rows: &[Instruction], indexes: &BTreeMap<u64, usize>) -> bool {
    let only_local_frame_work = state.path.iter().all(|address| {
        indexes.get(address).is_some_and(|&index| {
            let row = &rows[index];
            let call = matches!(row.operation.as_str(), "bl" | "blr" | "br");
            let store = row.operation.starts_with("st") && !row.operands.contains("[sp");
            !call && !store
        })
    });

    only_local_frame_work
        && state.stored.is_none()
        && state.converted.is_none()
        && state.temporary.is_none()
        && state.value_read.is_none()
        && state.keyword_conditions.is_none()
        && state.copied_tokens.is_empty()
        && state.value_strings.is_empty()
        && state.value_token_words.is_empty()
        && state.targets.is_empty()
        && state.emplaced.is_empty()
}

/// The instructions that a diagnostic side may run besides its calls and branches: register
/// moves and arithmetic, loads, compares, and stores to the frame.
fn diagnostic_side_instruction(row: &Instruction) -> bool {
    match row.operation.as_str() {
        "mov" | "movz" | "movk" | "add" | "sub" | "adrp" | "adr" | "cmp" | "tst" | "cset"
        | "csel" | "nop" | "ldr" | "ldrb" | "ldrh" | "ldrsb" | "ldrsh" | "ldrsw" | "ldur"
        | "ldp" => true,
        "str" | "stur" | "strb" | "strh" | "stp" => stores_to_a_frame_slot(&row.operands),
        _ => false,
    }
}

/// Whether a store's address is a fixed slot of the frame: `[sp]` or `sp` with an immediate
/// offset, not a register offset.
fn stores_to_a_frame_slot(operands: &str) -> bool {
    let Some((_, address)) = operands.split_once("[sp") else {
        return false;
    };

    address.starts_with(']') || address.starts_with(",#")
}

/// The most instructions that [`logs_and_returns`] examines, over all its ways.
const MAX_DIAGNOSTIC_SIDE: usize = 128;

/// Whether every way from `rows[start]` writes a diagnostic and returns, calling nothing else
/// that can change script state and writing only the frame. Such a side reads no key, whatever
/// the token. Each way is followed to its own `ret`, so a loop exhausts the bound and fails.
fn logs_and_returns(
    rows: &[Instruction],
    start: usize,
    indexes: &BTreeMap<u64, usize>,
    input: &DispatchInput<'_>,
) -> bool {
    let local = |operand: &str| number(operand).and_then(|a| indexes.get(&(a as u64)).copied());
    // Each way, and whether it has written a diagnostic.
    let mut pending = vec![(start, false)];
    let mut examined = 0;

    while let Some((mut index, mut logged)) = pending.pop() {
        loop {
            examined += 1;
            if examined > MAX_DIAGNOSTIC_SIDE {
                return false;
            }
            let Some(row) = rows.get(index) else {
                return false;
            };
            let target = row.operands.rsplit(',').next().unwrap_or_default();
            index += 1;

            match row.operation.as_str() {
                "ret" if logged => break,
                "ret" | "br" | "blr" => return false,
                "bl" => {
                    let Some(callee) = number(&row.operands)
                        .and_then(|address| input.symbol_index.name(input.symbols, address as u64))
                    else {
                        return false;
                    };
                    if !readers::changes_no_script_state(callee) {
                        return false;
                    }
                    logged |= readers::DIAGNOSTICS.contains(&callee);
                }
                "b" => match local(target) {
                    Some(next) => index = next,
                    None => return false,
                },
                "cbz" | "cbnz" | "tbz" | "tbnz" => match local(target) {
                    Some(next) => pending.push((next, logged)),
                    None => return false,
                },
                operation if operation.starts_with("b.") => match local(target) {
                    Some(next) => pending.push((next, logged)),
                    None => return false,
                },
                _ if diagnostic_side_instruction(row) => {}
                _ => return false,
            }
        }
    }

    true
}

/// Whether the instruction at `index` calls a function that never returns.
fn aborts(rows: &[Instruction], index: usize, input: &DispatchInput<'_>) -> bool {
    rows.get(index).is_some_and(|row| {
        row.operation == "bl"
            && number(&row.operands)
                .and_then(|address| input.symbol_index.name(input.symbols, address as u64))
                .is_some_and(readers::never_returns)
    })
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
    // A word that an earlier ordered compare constrained is decided only by the same compare or
    // its opposite; any other test of it could take a side that the earlier one excludes.
    if let Some(earlier) = state.ordered.get(tested) {
        let taken = if earlier.pivots != pivots {
            None
        } else if earlier.condition == condition {
            Some(earlier.taken)
        } else if opposite(earlier.condition) == Some(condition) {
            Some(!earlier.taken)
        } else {
            None
        };
        let Some(taken) = taken else {
            return Branching::gap(
                state,
                row.address,
                Unresolved::at(
                    "ordered-compare-repeat",
                    row.address,
                    entry,
                    Obstacle::Unsupported,
                ),
            );
        };
        let mut next = state.clone();
        next.pc = if taken { target } else { state.pc };
        return Branching {
            continued: vec![next],
            ..Branching::default()
        };
    }
    // An ordered compare of an owner word, such as a count, keeps both sides under a condition
    // that the walk does not resolve, and the path remembers its side.
    if let Some(condition) = opposite(condition)
        .and_then(opposite)
        .filter(|condition| !matches!(*condition, "eq" | "ne"))
        && is_owner_word(tested)
    {
        let settled_before = state.settled.keys().any(
            |compared| matches!(compared, Value::EqualsAny(word, _) if word.as_ref() == tested),
        );
        if settled_before {
            return Branching::gap(
                state,
                row.address,
                Unresolved::at(
                    "ordered-compare-repeat",
                    row.address,
                    entry,
                    Obstacle::Unsupported,
                ),
            );
        }
        let continued = [(target, true), (state.pc, false)]
            .into_iter()
            .map(|(pc, taken)| {
                let mut next = state.clone();
                next.pc = pc;
                next.conditions.push(Condition {
                    at: row.address,
                    value: None,
                    zero: !taken,
                });
                next.ordered.insert(
                    tested.clone(),
                    OrderedSide {
                        condition,
                        pivots: pivots.to_vec(),
                        taken,
                    },
                );
                next
            })
            .collect();
        return Branching {
            continued,
            ..Branching::default()
        };
    }
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
                let compared = Value::EqualsAny(Box::new(tested.clone()), pivots.to_vec());
                if is_owner_word(tested) {
                    next.settled.insert(compared.clone(), equal);
                }
                next.conditions.push(Condition {
                    at: row.address,
                    value: Some(compared),
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

/// The paths of a bit test of a value that is not the token, each under a condition that no
/// field states: the feasible one for a known constant, otherwise both. A bit test of the token
/// would split its domain by a bit, which the token intervals cannot hold, so it is a gap.
fn bit_test_branch(
    row: &Instruction,
    tested: &str,
    bit: &str,
    target: &str,
    state: &State,
    indexes: &BTreeMap<u64, usize>,
    entry: u64,
) -> Branching {
    let Some(&target) = number(target).and_then(|a| indexes.get(&(a as u64))) else {
        let outside = Unresolved::at("branch-target", row.address, entry, Obstacle::OutsideCode);
        return Branching::gap(state, row.address, outside);
    };
    if matches!(
        state.value(tested),
        Some(Value::Token | Value::TokenWord(_))
    ) {
        let unsupported =
            Unresolved::at("token-bit-test", row.address, entry, Obstacle::Unsupported);
        return Branching::gap(state, row.address, unsupported);
    }

    // `tbnz` branches when the bit is set, `tbz` when it is clear.
    let constant = match state.value(tested) {
        Some(Value::Constant(value)) => Some(value),
        _ => None,
    };
    let known_taken = match (constant, number(bit)) {
        (Some(value), Some(bit)) => {
            let set = value >> bit & 1 == 1;

            Some(set == (row.operation == "tbnz"))
        }
        _ => None,
    };
    // A known constant's condition holds on every path, as a zero test of one does.
    let tested_value = known_taken.and(constant).map(Value::Constant);
    let continued = [false, true]
        .into_iter()
        .filter(|&taken| known_taken.is_none_or(|known| known == taken))
        .map(|taken| {
            let mut next = state.clone();
            next.pc = if taken { target } else { state.pc };
            next.conditions.push(Condition {
                at: row.address,
                value: tested_value.clone(),
                zero: taken == (row.operation == "tbz"),
            });
            next
        })
        .collect();

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
        let joined = matches!(
            leaf.outcome,
            PathOutcome::Reader(ReaderJoin::Joined { .. } | ReaderJoin::Stored { .. })
        );
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
    } else if bindings.string_from_text.contains(&target)
        && source == Some(value_text(bindings))
        && let Some(Value::Stack(at)) = receiver
    {
        state.value_strings.insert(at);
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
        && state.stack.get(at) == Some(&value_text(bindings))
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
    state.clobber_call();
    CompoundCall::Continue
}

/// The most instructions that [`assigns_next`] examines.
const MAX_ASSIGNMENT_LOOKAHEAD: usize = 16;

/// Whether the code from `rows[start]` reaches a string assignment call with no conditional
/// branch or other call first. Only then does the walk follow `strlen` of the value text: a
/// length that the reader tests instead, as a one-character code does, keeps the stop at the call.
fn assigns_next(
    rows: &[Instruction],
    start: usize,
    indexes: &BTreeMap<u64, usize>,
    input: &DispatchInput<'_>,
) -> bool {
    let mut index = start;

    for _ in 0..MAX_ASSIGNMENT_LOOKAHEAD {
        let Some(row) = rows.get(index) else {
            return false;
        };
        index += 1;
        match row.operation.as_str() {
            "b" | "bl" => {
                let Some(target) = number(&row.operands).map(|address| address as u64) else {
                    return false;
                };
                if row.operation == "b"
                    && let Some(&local) = indexes.get(&target)
                {
                    index = local;
                    continue;
                }

                return input
                    .symbol_index
                    .name(input.symbols, target)
                    .is_some_and(readers::assigns_string_text);
            }
            operation
                if operation.starts_with('b')
                    || operation.starts_with("cb")
                    || operation.starts_with("tb")
                    || operation == "ret" =>
            {
                return false;
            }
            _ => {}
        }
    }

    false
}

/// The size of a libc++ `std::basic_string` on a 64-bit target: three words.
const STANDARD_STRING_SIZE: i64 = 0x18;

/// The value token's word, loaded from the reader.
fn value_token_word(bindings: &super::KeyReaders) -> Value {
    Value::Load(Box::new(Value::Reader(bindings.value_token)), 4)
}

/// The reader that stores the value token's id in an owner member. Any token is accepted; the
/// tokens that mean something are known only where the member is used, so the keyword's domain
/// stays unknown.
const VALUE_TOKEN_STORE: &str = "value token id";

/// Whether `stored` is the store of the value token's id.
fn stores_value_token_id(stored: &(ReaderJoin, i64, i64)) -> bool {
    matches!(&stored.0, ReaderJoin::Stored { callee, .. } if callee == VALUE_TOKEN_STORE)
}

/// The reader that stores whether the value token is `yes` in an owner member. Every other token
/// stores false, so the reader accepts any token without an error.
const TRUE_TOKEN_TEST: &str = "value token == yes";

/// The reader that stores whether the value token is one literal token other than `yes`: a
/// keyword with two meanings, that token and every other. Every token is accepted without an
/// error, so the keyword's domain is not established.
const LITERAL_TOKEN_TEST: &str = "value token == literal";

/// The comparison of the value token with one literal token that the flags hold, as a Boolean;
/// `None` for any other comparison, or when the value token or `yes` is not bound: without `yes`,
/// the literal could be it.
fn literal_token_test(state: &State, bindings: &super::KeyReaders) -> Option<Value> {
    let Some(Flags::Values { tested, pivots }) = &state.flags else {
        return None;
    };
    let token_word = value_token_word(bindings);

    let bound = bindings.value_token != 0 && bindings.true_token.is_some();

    (bound && *tested == token_word && pivots.len() == 1)
        .then(|| Value::EqualsAny(Box::new(token_word), pivots.clone()))
}

/// Whether the call to `target` converts the value token with a pure converter, which a
/// keyword's caller then stores.
fn converts_value_token(input: &DispatchInput<'_>, target: u64, state: &State) -> bool {
    let Some(callee) = input.symbol_index.name(input.symbols, target) else {
        return false;
    };

    match readers::converter(callee) {
        Some(ConverterArgument::Pointer) => {
            matches!(state.value("x0"), Some(Value::Stack(at)) if state.value_token_words.contains(&at))
        }
        Some(ConverterArgument::Word) => {
            state.value("w0") == Some(value_token_word(input.key_readers))
        }
        None => false,
    }
}

/// The pointer to the value token's text, loaded from the reader.
fn value_text(bindings: &super::KeyReaders) -> Value {
    Value::Load(
        Box::new(Value::Reader(bindings.value_token + bindings.token_text)),
        8,
    )
}

/// A call that passes the owner a stack string made from the value token's text reads the value
/// as a string. This is a stated rule: the method does not follow the callee, so where the
/// string is stored, and which strings it accepts, are not established.
fn passes_value_string(state: &State) -> bool {
    state.value("x0") == Some(Value::Owner(0))
        && matches!(state.value("x1"), Some(Value::Stack(at)) if state.value_strings.contains(&at))
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
    fn an_or_update_forgets_its_result_and_the_walk_goes_on() {
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
        let symbol_index = SymbolIndex::new(&symbols);
        let input =
            DispatchInput::command(&functions, &symbols, &symbol_index, &[], 0x38, &readers);
        let (paths, _) = explore_member(&input, root);
        assert!(
            matches!(&paths[0].outcome, PathOutcome::Reader(ReaderJoin::Joined { callee, .. })
            if callee == "CReader::Read(CString&, bool)")
        );
    }

    #[test]
    fn an_unknown_condition_takes_the_side_that_returns() {
        let root = "Owner::ReadMember(CReader&, int)";
        let read = "CReader::Read(CString&, bool)";
        let abort_side_taken = arm64!(at 0x1000;
            ldr x9, [x1, #0x40]; // a value that no token decides
            ldr x8, [x0, #0x50];
            cmp x9, x8;
            b.ne extern 0x1024;
            add x8, x0, #24;
            mov x0, x1;
            mov x1, x8;
            mov w2, #0;
            b extern 0x2000;
            bl extern 0x3000
        );
        let abort_side_next = arm64!(at 0x1000;
            ldr x9, [x1, #0x40];
            ldr x8, [x0, #0x50];
            cmp x9, x8;
            b.eq extern 0x1014;
            bl extern 0x3000;
            add x8, x0, #24;
            mov x0, x1;
            mov x1, x8;
            mov w2, #0;
            b extern 0x2000
        );
        for (code, callee, joins) in [
            (&abort_side_taken, "___stack_chk_fail", true),
            (&abort_side_next, "___stack_chk_fail", true),
            (&abort_side_taken, "Owner::Report()", false),
        ] {
            let symbols = vec![
                Symbol {
                    name: root.into(),
                    address: 0x1000,
                },
                Symbol {
                    name: read.into(),
                    address: 0x2000,
                },
                Symbol {
                    name: callee.into(),
                    address: 0x3000,
                },
            ];
            let functions = BTreeMap::from([(
                0x1000,
                Function {
                    address: 0x1000,
                    code: code.clone(),
                },
            )]);
            let readers = super::super::KeyReaders::default();
            let symbol_index = SymbolIndex::new(&symbols);
            let input =
                DispatchInput::command(&functions, &symbols, &symbol_index, &[], 0x38, &readers);
            let (paths, _) = explore_member(&input, root);
            let joined = paths.iter().any(|path| {
                matches!(&path.outcome, PathOutcome::Reader(ReaderJoin::Joined { callee, .. })
                    if callee == read)
            });

            assert_eq!(joined, joins, "{callee}: {paths:?}");
            assert_eq!(paths.len(), 1, "{callee}: {paths:?}");
        }
    }

    #[test]
    fn an_unbound_value_token_joins_no_assignment() {
        let root = "Owner::ReadMember(CReader&, int)";
        let assign = "CVariableValue::Assign(CToken const&, EScopeType, CString const&)";
        let symbols = vec![
            Symbol {
                name: root.into(),
                address: 0x1000,
            },
            Symbol {
                name: assign.into(),
                address: 0x2000,
            },
        ];
        let functions = |code| {
            BTreeMap::from([(
                0x1000,
                Function {
                    address: 0x1000,
                    code,
                },
            )])
        };
        let unchanged_reader = functions(arm64!(at 0x1000; b extern 0x2000));
        let value_token_slot = functions(arm64!(at 0x1000;
            add x1, x1, #0x278; // the reader's value token
            b extern 0x2000
        ));
        let bound = super::super::KeyReaders {
            value_token: 0x278,
            ..Default::default()
        };
        let default = super::super::KeyReaders::default();
        let joins = |functions, readers| {
            let symbol_index = SymbolIndex::new(&symbols);
            let input =
                DispatchInput::command(functions, &symbols, &symbol_index, &[], 0x38, readers);
            let (paths, _) = explore_member(&input, root);
            matches!(&paths[0].outcome, PathOutcome::Reader(ReaderJoin::Joined { callee, .. })
                if callee == assign)
        };

        assert!(joins(&value_token_slot, &bound));
        assert!(!joins(&unchanged_reader, &bound));
        assert!(!joins(&unchanged_reader, &default));
    }

    #[test]
    fn a_member_that_stores_whether_the_value_token_is_one_token_reads_a_boolean_or_a_keyword() {
        let root = "Owner::ReadMember(CReader&, int)";
        let symbols = vec![Symbol {
            name: root.into(),
            address: 0x1000,
        }];
        let functions = |code| {
            BTreeMap::from([(
                0x1000,
                Function {
                    address: 0x1000,
                    code,
                },
            )])
        };
        let yes_test = functions(arm64!(at 0x1000;
            ldr w8, [x1, #0x278];
            mov w9, #7; // the true token
            cmp w8, w9;
            cset w8, eq;
            strb w8, [x0, #0x20];
            ret
        ));
        let other_token_test = functions(arm64!(at 0x1000;
            ldr w8, [x1, #0x278];
            mov w9, #8; // a token other than yes
            cmp w8, w9;
            cset w8, eq;
            strb w8, [x0, #0x20];
            ret
        ));
        let readers = super::super::KeyReaders {
            value_token: 0x278,
            true_token: Some(7),
            ..Default::default()
        };
        let outcome = |functions| {
            let symbol_index = SymbolIndex::new(&symbols);
            let input =
                DispatchInput::command(functions, &symbols, &symbol_index, &[], 0x38, &readers);
            let (paths, _) = explore_member(&input, root);

            paths[0].outcome.clone()
        };

        assert!(matches!(
            outcome(&yes_test),
            PathOutcome::Reader(ReaderJoin::Stored {
                kind: crate::ReaderKind::Boolean,
                destination: Some(0x20),
                repeat: crate::RepeatBehavior::Replace,
                ..
            })
        ));
        assert!(matches!(
            outcome(&other_token_test),
            PathOutcome::Reader(ReaderJoin::Stored {
                kind: crate::ReaderKind::Keyword,
                destination: Some(0x20),
                repeat: crate::RepeatBehavior::Replace,
                ..
            })
        ));
        let unbound_yes = super::super::KeyReaders {
            value_token: 0x278,
            ..Default::default()
        };
        let symbol_index = SymbolIndex::new(&symbols);
        let input = DispatchInput::command(
            &other_token_test,
            &symbols,
            &symbol_index,
            &[],
            0x38,
            &unbound_yes,
        );
        let (paths, _) = explore_member(&input, root);
        assert!(
            matches!(paths[0].outcome, PathOutcome::Gap(_)),
            "without yes the literal could be it"
        );
    }

    #[test]
    fn a_member_that_stores_the_value_token_id_reads_a_keyword_of_unknown_domain() {
        let root = "Owner::ReadMember(CReader&, int)";
        let symbols = vec![Symbol {
            name: root.into(),
            address: 0x1000,
        }];
        let functions = |code| {
            BTreeMap::from([(
                0x1000,
                Function {
                    address: 0x1000,
                    code,
                },
            )])
        };
        let token_store = functions(arm64!(at 0x1000;
            ldr w8, [x1, #0x278]; // the value token
            str w8, [x0, #0x20];
            ret
        ));
        let other_word_store = functions(arm64!(at 0x1000;
            ldr w8, [x1, #0x27c]; // another reader word
            str w8, [x0, #0x20];
            ret
        ));
        let byte_store = functions(arm64!(at 0x1000;
            ldr w8, [x1, #0x278]; // the value token
            strb w8, [x0, #0x20]; // only its low byte
            ret
        ));
        let unbound_store = functions(arm64!(at 0x1000;
            ldr w8, [x1]; // the word of an unbound value token
            str w8, [x0, #0x20];
            ret
        ));
        let bound = super::super::KeyReaders {
            value_token: 0x278,
            ..Default::default()
        };
        let unbound = super::super::KeyReaders::default();
        let outcome_with = |functions, readers| {
            let symbol_index = SymbolIndex::new(&symbols);
            let input =
                DispatchInput::command(functions, &symbols, &symbol_index, &[], 0x38, readers);
            let (paths, _) = explore_member(&input, root);

            paths[0].outcome.clone()
        };
        let outcome = |functions| outcome_with(functions, &bound);

        assert!(matches!(
            outcome(&token_store),
            PathOutcome::Reader(ReaderJoin::Stored {
                kind: crate::ReaderKind::Keyword,
                destination: Some(0x20),
                repeat: crate::RepeatBehavior::Replace,
                ..
            })
        ));
        assert!(matches!(outcome(&other_word_store), PathOutcome::Gap(_)));
        assert!(matches!(outcome(&byte_store), PathOutcome::Gap(_)));
        assert!(matches!(
            outcome_with(&unbound_store, &unbound),
            PathOutcome::Gap(_)
        ));
    }

    #[test]
    fn a_converter_of_the_value_token_supersedes_a_store_of_its_id() {
        let root = "Owner::ReadMember(CReader&, int)";
        let symbols = vec![
            Symbol {
                name: root.into(),
                address: 0x1000,
            },
            Symbol {
                name: "EKind TokenToEnum<EKind>(int const&)".into(),
                address: 0x2000,
            },
        ];
        let functions = |stores_converted: bool| {
            let mut code = crate::engine::analysis::assembler::Arm64::at(0x1000);
            arm64!(code;
                mov x19, x0;
                ldr w8, [x1, #0x278]; // the value token
                str w8, [x19, #0x24]; // its id
                str w8, [sp, #0x10];
                add x0, sp, #0x10;
                bl extern 0x2000 // the converter
            );
            if stores_converted {
                arm64!(code; str w0, [x19, #0x20]); // the converted keyword
            }
            arm64!(code; ret);

            BTreeMap::from([(
                0x1000,
                Function {
                    address: 0x1000,
                    code: code.bytes(),
                },
            )])
        };
        let readers = super::super::KeyReaders {
            value_token: 0x278,
            ..Default::default()
        };
        let symbol_index = SymbolIndex::new(&symbols);
        let outcome = |stores_converted| {
            let functions = functions(stores_converted);
            let input =
                DispatchInput::command(&functions, &symbols, &symbol_index, &[], 0x38, &readers);
            let (paths, _) = explore_member(&input, root);

            paths[0].outcome.clone()
        };

        assert!(
            matches!(
                &outcome(true),
                PathOutcome::Reader(ReaderJoin::Stored {
                    callee,
                    kind: crate::ReaderKind::Keyword,
                    destination: Some(0x20),
                    ..
                }) if callee.contains("TokenToEnum")
            ),
            "{:?}",
            outcome(true)
        );
        assert!(
            matches!(
                &outcome(false),
                PathOutcome::Reader(ReaderJoin::Stored {
                    callee,
                    destination: Some(0x24),
                    ..
                }) if callee == VALUE_TOKEN_STORE
            ),
            "the id store stands when the converted value is not stored: {:?}",
            outcome(false)
        );
    }

    #[test]
    fn a_string_assigned_from_the_value_text_and_its_length_reads_a_string() {
        let root = "Owner::ReadMember(CReader&, int)";
        let assign = "std::__1::basic_string<char, std::__1::char_traits<char>, CPdxCommonStringAllocator>::__assign_external(char const*, unsigned long)";
        let symbols = vec![
            Symbol {
                name: root.into(),
                address: 0x1000,
            },
            Symbol {
                name: "_strlen".into(),
                address: 0x2000,
            },
            Symbol {
                name: assign.into(),
                address: 0x3000,
            },
        ];
        let functions = |measured: bool| {
            let mut code = crate::engine::analysis::assembler::Arm64::at(0x1000);
            arm64!(code;
                mov x19, x0;
                ldr x20, [x1, #0x288]; // the value text
                mov x0, x20
            );
            if measured {
                arm64!(code; bl extern 0x2000); // strlen
            }
            arm64!(code;
                mov x2, x0;
                add x0, x19, #0xf0;
                mov x1, x20;
                b extern 0x3000 // assign the text and its length
            );

            BTreeMap::from([(
                0x1000,
                Function {
                    address: 0x1000,
                    code: code.bytes(),
                },
            )])
        };
        let readers = super::super::KeyReaders {
            value_token: 0x278,
            token_text: 0x10,
            ..Default::default()
        };
        let symbol_index = SymbolIndex::new(&symbols);
        let outcome = |measured| {
            let functions = functions(measured);
            let input =
                DispatchInput::command(&functions, &symbols, &symbol_index, &[], 0x38, &readers);
            let (paths, _) = explore_member(&input, root);

            paths[0].outcome.clone()
        };

        assert!(matches!(
            outcome(true),
            PathOutcome::Reader(ReaderJoin::Stored {
                kind: crate::ReaderKind::String,
                destination: Some(0xf0),
                repeat: crate::RepeatBehavior::Replace,
                ..
            })
        ));
        assert!(
            !matches!(
                outcome(false),
                PathOutcome::Reader(ReaderJoin::Stored { .. })
            ),
            "a length that is not the text's own proves no whole-text read"
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
            let symbol_index = SymbolIndex::new(&symbols);
            let mut input =
                DispatchInput::command(&functions, &symbols, &symbol_index, &[], 0x38, &readers);
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
            let symbol_index = SymbolIndex::new(&symbols);
            let mut input = DispatchInput::command(
                &functions,
                &symbols,
                &symbol_index,
                &[],
                0x38,
                &key_readers,
            );
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
            settled: BTreeMap::new(),
            ordered: BTreeMap::new(),
            stack: BTreeMap::new(),
            serializer: None,
            copied_tokens: BTreeSet::new(),
            value_strings: BTreeSet::new(),
            value_read: None,
            targets: BTreeSet::new(),
            emplaced: BTreeSet::new(),
            value_token_words: BTreeSet::new(),
            converted: None,
            keyword_conditions: None,
            stored: None,
            temporary: None,
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

    #[test]
    fn a_function_lookup_needs_one_body_and_every_symbol_of_its_name_at_or_bound_to_that_body() {
        let symbol = |name: &str, address| Symbol {
            name: name.into(),
            address,
        };
        let symbols = [
            symbol("twin", 0x10),
            symbol("twin", 0x10),
            symbol("split", 0x20),
            symbol("split", 0x30),
            symbol("alias", 0x40),
            symbol("other", 0x40),
            symbol("moved", 0x50),
        ];
        let symbol_index = SymbolIndex::new(&symbols);
        let body = |address| Function {
            address,
            code: vec![0; 4],
        };
        let bodies = BTreeMap::from([
            (0x10, body(0x10)),
            (0x20, body(0x20)),
            (0x40, body(0x40)),
            (0x50, body(0x54)),
        ]);
        let readers = super::super::KeyReaders::default();
        let command = DispatchInput::command(&bodies, &symbols, &symbol_index, &[], 0x38, &readers);
        let named = [
            super::super::Function {
                name: "twin".into(),
                address: 0x10,
                code: vec![0; 4],
            },
            super::super::Function {
                name: "split".into(),
                address: 0x20,
                code: vec![0; 4],
            },
        ];
        let no_stubs = BTreeMap::new();
        let registry = DispatchInput {
            bodies: Bodies::Named(&named, &no_stubs),
            ..command.clone()
        };
        let bound_stub = BTreeMap::from([(0x30, 0x20)]);
        let registry_with_stub = DispatchInput {
            bodies: Bodies::Named(&named, &bound_stub),
            ..command.clone()
        };
        let found = |input: &DispatchInput<'_>, name| input.function(name).map(|f| f.address);

        assert_eq!(found(&command, "twin"), None, "one view per twin symbol");
        assert_eq!(found(&registry, "twin"), Some(0x10));
        assert_eq!(
            found(&registry, "split"),
            None,
            "an unbound symbol away from the body"
        );
        assert_eq!(
            found(&registry_with_stub, "split"),
            Some(0x20),
            "a stub that the image binds to the body"
        );
        assert_eq!(
            found(&command, "split"),
            None,
            "a symbol away from the body"
        );
        assert_eq!(found(&command, "alias"), Some(0x40));
        assert_eq!(
            command.symbol_index.name(command.symbols, 0x40),
            None,
            "two names at one address"
        );
        assert_eq!(found(&command, "moved"), None, "a body at another entry");
    }
}
