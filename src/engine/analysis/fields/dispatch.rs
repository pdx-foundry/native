use super::tokens::{decode, function, number, register, symbol_names};
use super::{Condition, FieldInput, PathOutcome, ReaderJoin, TokenPath, Value};
use crate::engine::analysis::decode::Instruction;
use crate::engine::analysis::stop::{Bound, Obstacle, Unknown, Unresolved};
use std::collections::BTreeMap;

const MIN: i64 = i32::MIN as i64;
const MAX: i64 = i32::MAX as i64;
const MAX_STATES: usize = 4096;
/// The most instructions that one path may run.
const MAX_PATH: usize = 500;
#[derive(Clone)]
struct State {
    pc: usize,
    registers: BTreeMap<String, Value>,
    flags: Option<i64>,
    domain: [i64; 2],
    conditions: Vec<Condition>,
    path: Vec<u64>,
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
                Some(Value::Token) => Some(Value::Token),
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
fn intervals(domain: [i64; 2], condition: &str, pivot: i64) -> Option<Vec<[i64; 2]>> {
    let ranges = match condition {
        "eq" => vec![[pivot, pivot]],
        "ne" => vec![[MIN, pivot - 1], [pivot + 1, MAX]],
        "le" => vec![[MIN, pivot]],
        "lt" => vec![[MIN, pivot - 1]],
        "gt" => vec![[pivot + 1, MAX]],
        "ge" => vec![[pivot, MAX]],
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
        _ => None,
    }
}
fn memory(operand: &str) -> Option<(&str, i64)> {
    let interior = operand.strip_prefix('[')?.strip_suffix(']')?;
    let (base, amount) = interior.split_once(',').unwrap_or((interior, "#0"));
    register(base)?;
    Some((base, number(amount)?))
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
    let args: Vec<_> = row.operands.split(',').collect();
    match (row.operation.as_str(), args.as_slice()) {
        ("cmp", [left, right]) => {
            state.flags = match (state.value(left), state.value(right)) {
                (Some(Value::Token), Some(Value::Constant(value))) if left.starts_with('w') => {
                    Some(value)
                }
                _ => None,
            };
        }
        ("mov", [destination, source]) => state.assign(destination, state.value(source)),
        ("add" | "sub", [destination, left, right]) => {
            let value = match (state.value(left), state.value(right)) {
                (Some(base), Some(Value::Constant(n))) => {
                    offset(base, if row.operation == "sub" { -n } else { n })
                }
                _ => None,
            };
            state.assign(destination, value);
        }
        ("ldr" | "ldrb" | "str" | "strb", _) => {
            let Some((operand, address)) = row.operands.split_once(',') else {
                return Err(unsupported("memory-operands"));
            };
            let Some((base, amount)) = memory(address) else {
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
        ("adrp", [destination, _]) => state.assign(destination, None),
        ("nop", [""]) => {}
        _ => return Err(unsupported("instruction")),
    }
    Ok(())
}

pub(super) fn explore(input: &FieldInput) -> Vec<TokenPath> {
    let name = format!(
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
    };
    let Some((entry, rows)) =
        function(input, &name).and_then(|root| Some((root.address, decode(root).ok()?)))
    else {
        let missing = Unresolved::new("root-function");
        return vec![initial.finish(0, PathOutcome::Gap(missing))];
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
                    && state.registers.get("x2") == Some(&Value::Token)
                {
                    PathOutcome::Rejected
                } else {
                    PathOutcome::Reader(reader_join(name, &state, row.address, entry))
                };
                leaves.push(state.finish(row.address, outcome));
                break;
            }
            if let Some(condition) = row.operation.strip_prefix("b.") {
                let target = number(&row.operands).and_then(|a| indexes.get(&(a as u64)));
                let split = match (state.flags, opposite(condition), target) {
                    (None, _, _) => Err(stop("flags", Obstacle::Unknown(Unknown::Flags))),
                    (_, None, _) => Err(stop("branch-condition", Obstacle::Unsupported)),
                    (_, _, None) => Err(stop("branch-target", Obstacle::OutsideCode)),
                    (Some(pivot), Some(inverse), Some(&target)) => Ok((pivot, inverse, target)),
                };
                let (pivot, inverse, target) = match split {
                    Ok(split) => split,
                    Err(unresolved) => {
                        leaves.push(state.finish(row.address, PathOutcome::Gap(unresolved)));
                        break;
                    }
                };
                for (condition, pc) in [(condition, target), (inverse, state.pc)] {
                    for domain in intervals(state.domain, condition, pivot).unwrap_or_default() {
                        let mut next = state.clone();
                        next.pc = pc;
                        next.domain = domain;
                        pending.push(next);
                    }
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
            if let Err(unresolved) = apply(row, &mut state, entry) {
                leaves.push(state.finish(row.address, PathOutcome::Gap(unresolved)));
                break;
            }
        }
    }
    leaves.sort_by(|a, b| {
        a.domain
            .cmp(&b.domain)
            .then(a.conditions.cmp(&b.conditions))
    });
    leaves
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
}
