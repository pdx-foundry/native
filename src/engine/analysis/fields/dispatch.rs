use super::tokens::{decode, function, number, register, symbol_names};
use super::{
    Condition, DataSection, FieldGap, FieldGapKind, FieldInput, PathOutcome, ReaderJoin,
    TableEntry, TokenPath, Value,
};
use crate::engine::analysis::decode::Instruction;
use crate::engine::analysis::stop::{Bound, Obstacle, Unknown, Unresolved};
use std::collections::{BTreeMap, BTreeSet};

const MIN: i64 = i32::MIN as i64;
const MAX: i64 = i32::MAX as i64;
const MAX_STATES: usize = 4096;
/// The most instructions that one path may run.
const MAX_PATH: usize = 500;
/// The most tokens that one jump through a table may select.
const MAX_TABLE_ENTRIES: usize = 1024;
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
struct State {
    pc: usize,
    registers: BTreeMap<String, Value>,
    flags: Option<Comparison>,
    domain: [i64; 2],
    conditions: Vec<Condition>,
    path: Vec<u64>,
    /// The jump-table case that this token took.
    table_case: Option<TableCase>,
}
impl State {
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
    if end <= MAX {
        vec![[start, end]]
    } else {
        vec![[start, MAX], [MIN, MIN + (end - MAX - 1)]]
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
        ("ne", 0) => vec![[MIN, pivot - 1], [pivot + 1, MAX]],
        ("le", 0) => vec![[MIN, pivot]],
        ("lt", 0) => vec![[MIN, pivot - 1]],
        ("gt", 0) => vec![[pivot + 1, MAX]],
        ("ge", 0) => vec![[pivot, MAX]],
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
fn rejection(input: &FieldInput, names: &BTreeMap<u64, Option<&str>>) -> bool {
    let Some(base) = function(input, "CPersistent::ReadMember(CReader&, int)") else {
        return false;
    };
    let Ok(rows) = decode(base) else {
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
fn reader_join(name: Option<&str>, state: &State, at: u64, entry: u64) -> ReaderJoin {
    let Some(name) = name else {
        return ReaderJoin::Missing(Unresolved::at("callee", at, entry, Obstacle::Call));
    };
    let get = |key: &str| state.registers.get(key);
    let owner = |value: Option<&Value>| matches!(value, Some(Value::Owner(_)));
    let joined = if name.starts_with("CReader::Read(") {
        get("x0") == Some(&Value::Reader(0)) && owner(get("x1"))
    } else if name == "CVariableValue::Read(CReader&, EScopeType)" {
        owner(get("x0")) && get("x1") == Some(&Value::Reader(0))
    } else if name.starts_with("void NParserUtil::ReadEffect<")
        || name.starts_with("void NParserUtil::ReadTrigger<")
    {
        get("x0") == Some(&Value::Reader(0)) && owner(get("x1"))
    } else if name.starts_with("void NParserUtil::ReadKeyReferenceDeferred<") {
        owner(get("x0")) && get("x1") == Some(&Value::Reader(0)) && owner(get("x2"))
    } else {
        false
    };
    if joined {
        ReaderJoin::Joined {
            callee: name.into(),
            arguments: state.registers.clone(),
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
fn apply(row: &Instruction, state: &mut State, entry: u64) -> Result<(), Unresolved> {
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
                    Some(Comparison { offset, pivot })
                }
                _ => None,
            };
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
            let (Some(Value::Constant(base)), Some(Value::TableEntry(entry)), Some(shift)) =
                (state.value(left), state.value(right), shift)
            else {
                return Err(unsupported("instruction"));
            };
            let shift = u8::try_from(shift).map_err(|_| unsupported("instruction"))?;
            state.assign(
                destination,
                Some(Value::TableTarget {
                    base: base as u64,
                    entry,
                    shift,
                }),
            );
        }
        ("ldr" | "ldrb" | "str" | "strb", _) => {
            let Some((operand, address)) = row.operands.split_once(',') else {
                return Err(unsupported("memory-operands"));
            };
            let Some((base, amount)) = memory(address).or_else(|| constant_index(state, address))
            else {
                return Err(unsupported("addressing"));
            };
            let location = state.value(base).and_then(|v| offset(v, amount));
            if row.operation.starts_with("ld") {
                let width = if row.operation == "ldrb" {
                    1
                } else if operand.starts_with('x') {
                    8
                } else {
                    4
                };
                // A load retains origin as a load, never as the original receiver or pointer.
                let value = location.map(|v| Value::Load(Box::new(v), width));
                let key = register(operand).ok_or_else(|| unsupported("load-destination"))?;
                state.registers.remove(&key);
                if let Some(value) = value {
                    state.registers.insert(key, value);
                }
            } else if !matches!(location, Some(Value::Owner(_) | Value::Stack(_))) {
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
            let (base, amount) = memory(address).ok_or_else(|| unsupported("addressing"))?;
            if !matches!(
                state.value(base).and_then(|v| offset(v, amount)),
                Some(Value::Stack(_))
            ) {
                return Err(stop("pair-address", unestablished(state, base)));
            }
            if row.operation == "ldp" {
                state.assign(first, None);
                state.assign(second, None);
            }
        }
        // Bit-field reads do not set flags; their result is not followed.
        ("ubfx" | "and", [destination, ..]) => state.assign(destination, None),
        ("adrp" | "adr", [destination, address]) => {
            state.assign(destination, number(address).map(Value::Constant))
        }
        ("nop", [""]) => {}
        _ => return Err(unsupported("instruction")),
    }
    Ok(())
}

/// Every token path through the root, and the jump tables in it that could not be decoded.
pub(super) fn explore(input: &FieldInput) -> (Vec<TokenPath>, Vec<FieldGap>) {
    let root = format!(
        "{}::ReadMember(CReader&, int)",
        input.selection.owner_candidate
    );
    let initial = State {
        pc: 0,
        registers: BTreeMap::from([
            ("x0".into(), Value::Owner(0)),
            ("x1".into(), Value::Reader(0)),
            ("x2".into(), Value::Token),
            ("xzr".into(), Value::Constant(0)),
            ("sp".into(), Value::Stack(0)),
        ]),
        flags: None,
        domain: [MIN, MAX],
        conditions: vec![],
        path: vec![],
        table_case: None,
    };
    let Some((entry, rows)) = function(input, &root)
        .and_then(|function| Some((function.address, decode(function).ok()?)))
    else {
        let missing = Unresolved::new("root-function");
        return (vec![initial.finish(0, PathOutcome::Gap(missing))], vec![]);
    };
    let after_last = rows.last().map_or(entry, |row| row.address + 4);
    let indexes: BTreeMap<_, _> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| (r.address, i))
        .collect();
    let names = symbol_names(input);
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
            if matches!(row.operation.as_str(), "b" | "bl") {
                let target = number(&row.operands).map(|a| a as u64);
                if row.operation == "b"
                    && let Some(index) = target.and_then(|a| indexes.get(&a))
                {
                    state.pc = *index;
                    continue;
                }
                let name = target.and_then(|a| names.get(&a).copied().flatten());
                let outcome = if name == Some("CPersistent::ReadMember(CReader&, int)")
                    && rejects
                    && matches!(state.registers.get("x0"), Some(Value::Owner(_)))
                    && state.registers.get("x1") == Some(&Value::Reader(0))
                    && matches!(
                        state.registers.get("x2"),
                        Some(Value::Token | Value::TokenWord(0))
                    ) {
                    PathOutcome::Rejected
                } else {
                    if let Some(case) = state.table_case {
                        table_readers.push((leaves.len(), case));
                    }
                    PathOutcome::Reader(reader_join(name, &state, row.address, entry))
                };
                leaves.push(state.finish(row.address, outcome));
                break;
            }
            if let Some(condition) = row.operation.strip_prefix("b.") {
                let target = number(&row.operands).and_then(|a| indexes.get(&(a as u64)));
                let condition = match condition {
                    "cs" => "hs",
                    "cc" => "lo",
                    condition => condition,
                };
                let split = match (state.flags, opposite(condition), target) {
                    (None, _, _) => Err(stop("flags", Obstacle::Unknown(Unknown::Flags))),
                    (_, None, _) => Err(stop("branch-condition", Obstacle::Unsupported)),
                    (_, _, None) => Err(stop("branch-target", Obstacle::OutsideCode)),
                    (Some(comparison), Some(inverse), Some(&target)) => {
                        let taken = intervals(state.domain, condition, comparison);
                        let not_taken = intervals(state.domain, inverse, comparison);
                        match (taken, not_taken) {
                            (Some(taken), Some(not_taken)) => Ok((taken, target, not_taken)),
                            _ => Err(stop("branch-condition", Obstacle::Unsupported)),
                        }
                    }
                };
                let (taken, target, not_taken) = match split {
                    Ok(split) => split,
                    Err(unresolved) => {
                        leaves.push(state.finish(row.address, PathOutcome::Gap(unresolved)));
                        break;
                    }
                };
                let taken = taken.into_iter().map(|domain| (domain, target));
                let not_taken = not_taken.into_iter().map(|domain| (domain, state.pc));
                for (domain, pc) in taken.chain(not_taken) {
                    let mut next = state.clone();
                    next.pc = pc;
                    next.domain = domain;
                    pending.push(next);
                }
                break;
            }
            if matches!(row.operation.as_str(), "cbz" | "cbnz") && args.len() == 2 {
                let Some(&target) = number(args[1]).and_then(|a| indexes.get(&(a as u64))) else {
                    let outside = stop("branch-target", Obstacle::OutsideCode);
                    leaves.push(state.finish(row.address, PathOutcome::Gap(outside)));
                    break;
                };
                let value = state.value(args[0]);
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
                    pending.push(next);
                }
                break;
            }
            if row.operation == "br" {
                let mut table_gap = |table: u64, why: &str, unresolved: Unresolved| {
                    push_table_gap(&mut tables, &root, table, why, unresolved);
                };
                let (base, table_entry, shift) = match state.value(&row.operands) {
                    Some(Value::TableTarget { base, entry, shift }) => (base, entry, shift),
                    other => {
                        let unresolved = if let Some(Value::TableEntry(table_entry)) = other {
                            let unresolved = stop("jump-table", Obstacle::Unsupported);
                            table_gap(table_entry.table, "its entries are addresses", unresolved);
                            unresolved
                        } else {
                            stop("branch-value", unestablished(&state, &row.operands))
                        };
                        leaves.push(state.finish(row.address, PathOutcome::Gap(unresolved)));
                        break;
                    }
                };
                let table = table_entry.table;
                let [low, high] = state.domain;
                if high - low >= MAX_TABLE_ENTRIES as i64 {
                    let bound = Obstacle::Bound(Bound::TableEntries(MAX_TABLE_ENTRIES));
                    let unresolved = stop("jump-table", bound);
                    table_gap(table, "its index is not bounded", unresolved);
                    leaves.push(state.finish(row.address, PathOutcome::Gap(unresolved)));
                    break;
                }
                let guard = state.path.iter().rev().copied().find(|address| {
                    indexes
                        .get(address)
                        .is_some_and(|&index| rows[index].operation.starts_with("b."))
                });
                let data = &input.read_only_data;
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
                            pending.push(case);
                        }
                        Some((_, None)) => {
                            let unresolved = stop("jump-table-case", Obstacle::OutsideCode);
                            table_gap(table, "a case is outside the reader", unresolved);
                            leaves.push(case.finish(row.address, PathOutcome::Gap(unresolved)));
                        }
                        None => {
                            let unresolved = stop("jump-table-entry", Obstacle::Unsupported);
                            table_gap(table, "an entry could not be read", unresolved);
                            leaves.push(case.finish(row.address, PathOutcome::Gap(unresolved)));
                        }
                    }
                }
                break;
            }
            if let Err(unresolved) = apply(row, &mut state, entry) {
                leaves.push(state.finish(row.address, PathOutcome::Gap(unresolved)));
                break;
            }
        }
    }
    reject_default_cases(&mut leaves, &table_readers, &mut tables, &root, entry);
    leaves.sort_by(|a, b| {
        a.domain
            .cmp(&b.domain)
            .then(a.conditions.cmp(&b.conditions))
    });
    (leaves, tables)
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
        push_table_gap(gaps, root, case.table, why, unresolved);
        changed.push((index, unresolved));
    }
    for (index, unresolved) in changed {
        leaves[index].outcome = PathOutcome::Gap(unresolved);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn register_widths_preserve_only_valid_provenance() {
        let mut state = State {
            pc: 0,
            registers: BTreeMap::new(),
            flags: None,
            domain: [MIN, MAX],
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
        let all = [MIN, MAX];
        let range = |offset, pivot| Comparison { offset, pivot };
        assert_eq!(intervals(all, "ls", range(-100, 2)), Some(vec![[100, 102]]));
        assert_eq!(
            intervals(all, "hi", range(-100, 2)),
            Some(vec![[103, MAX], [MIN, 99]])
        );
        assert_eq!(
            intervals(all, "hi", range(0, 10)),
            Some(vec![[11, MAX], [MIN, -1]])
        );
        assert_eq!(intervals([0, 50], "eq", range(-100, 2)), Some(vec![]));
        assert_eq!(intervals(all, "gt", range(-100, 2)), None);
        assert_eq!(intervals(all, "gt", range(0, 2)), Some(vec![[3, MAX]]));
    }
}
