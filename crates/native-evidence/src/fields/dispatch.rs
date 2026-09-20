use super::tokens::{callee, decode, function, number, register};
use super::{Condition, FieldInput, PathOutcome, ReaderJoin, TokenPath, Value};
use crate::{EvidenceReference, analysis::Instruction};
use std::collections::BTreeMap;

const MIN: i64 = i32::MIN as i64;
const MAX: i64 = i32::MAX as i64;
const MAX_STATES: usize = 4096;
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
    fn finish(
        &self,
        terminal: u64,
        outcome: PathOutcome,
        evidence: &EvidenceReference,
    ) -> TokenPath {
        TokenPath {
            domain: self.domain,
            conditions: self.conditions.clone(),
            instructions: self.path.clone(),
            terminal,
            outcome,
            evidence: evidence.clone(),
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
fn rejection(input: &FieldInput) -> bool {
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
        && number(&rows[1].operands).and_then(|a| callee(input, a as u64))
            == Some("CReader::ReportUnexpected()")
}
fn reader_join(name: Option<&str>, state: &State) -> ReaderJoin {
    let Some(name) = name else {
        return ReaderJoin::Missing {
            reason: "unresolved or ambiguous callee".into(),
        };
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
        ReaderJoin::Missing {
            reason: format!(
                "unqualified reader routing or clobbered receiver/destination at {name}"
            ),
        }
    }
}
fn apply(row: &Instruction, state: &mut State) -> Result<(), String> {
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
                return Err("unsupported memory operands".into());
            };
            let Some((base, amount)) = memory(address) else {
                return Err("unsupported memory addressing".into());
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
                let key = register(operand).ok_or("invalid load destination")?;
                state.registers.remove(&key);
                if let Some(value) = value {
                    state.registers.insert(key, value);
                }
            } else if !matches!(location, Some(Value::Owner(_) | Value::Stack(_))) {
                return Err("store has unresolved destination".into());
            }
        }
        ("stp" | "ldp", _) => {
            let mut parts = row.operands.splitn(3, ',');
            let first = parts.next().ok_or("missing pair register")?;
            let second = parts.next().ok_or("missing pair register")?;
            let address = parts.next().ok_or("missing pair address")?;
            let (base, amount) = memory(address).ok_or("unsupported pair addressing")?;
            if !matches!(
                state.value(base).and_then(|v| offset(v, amount)),
                Some(Value::Stack(_))
            ) {
                return Err("pair access is not on the known stack".into());
            }
            if row.operation == "ldp" {
                state.assign(first, None);
                state.assign(second, None);
            }
        }
        ("adrp", [destination, _]) => state.assign(destination, None),
        ("nop", [""]) => {}
        _ => {
            return Err(format!(
                "unsupported instruction {} {}",
                row.operation, row.operands
            ));
        }
    }
    Ok(())
}

pub(super) fn explore(input: &FieldInput, evidence: &EvidenceReference) -> Vec<TokenPath> {
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
    let rows = match function(input, &name)
        .ok_or("root member function missing or ambiguous".into())
        .and_then(decode)
    {
        Ok(rows) => rows,
        Err(reason) => return vec![initial.finish(0, PathOutcome::Gap(reason), evidence)],
    };
    let indexes: BTreeMap<_, _> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| (r.address, i))
        .collect();
    let rejects = rejection(input);
    let mut pending = vec![initial];
    let mut leaves = Vec::new();
    let mut visited = 0;
    while let Some(mut state) = pending.pop() {
        visited += 1;
        if visited > MAX_STATES {
            leaves.push(state.finish(
                0,
                PathOutcome::Gap("state budget exhausted".into()),
                evidence,
            ));
            leaves.extend(pending.drain(..).map(|s| {
                s.finish(
                    0,
                    PathOutcome::Gap("state budget exhausted".into()),
                    evidence,
                )
            }));
            break;
        }
        loop {
            let Some(row) = rows.get(state.pc) else {
                leaves.push(state.finish(
                    0,
                    PathOutcome::Gap("end of function without reader or rejection".into()),
                    evidence,
                ));
                break;
            };
            if state.path.len() >= 500 || state.path.contains(&row.address) {
                leaves.push(state.finish(
                    row.address,
                    PathOutcome::Gap("cycle or instruction budget".into()),
                    evidence,
                ));
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
                let name = target.and_then(|a| callee(input, a));
                let outcome = if name == Some("CPersistent::ReadMember(CReader&, int)")
                    && rejects
                    && matches!(state.registers.get("x0"), Some(Value::Owner(_)))
                    && state.registers.get("x1") == Some(&Value::Reader(0))
                    && state.registers.get("x2") == Some(&Value::Token)
                {
                    PathOutcome::Rejected
                } else {
                    PathOutcome::Reader(reader_join(name, &state))
                };
                leaves.push(state.finish(row.address, outcome, evidence));
                break;
            }
            if let Some(condition) = row.operation.strip_prefix("b.") {
                let split = state
                    .flags
                    .zip(opposite(condition))
                    .zip(number(&row.operands).and_then(|a| indexes.get(&(a as u64))));
                let Some(((pivot, inverse), &target)) = split else {
                    leaves.push(state.finish(
                        row.address,
                        PathOutcome::Gap("unknown flags, condition, or branch target".into()),
                        evidence,
                    ));
                    break;
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
                    leaves.push(state.finish(
                        row.address,
                        PathOutcome::Gap("unresolved state branch target".into()),
                        evidence,
                    ));
                    break;
                };
                for taken in [false, true] {
                    let mut next = state.clone();
                    next.pc = if taken { target } else { state.pc };
                    next.conditions.push(Condition {
                        at: row.address,
                        value: state.value(args[0]),
                        zero: taken == (row.operation == "cbz"),
                    });
                    pending.push(next);
                }
                break;
            }
            if let Err(reason) = apply(row, &mut state) {
                leaves.push(state.finish(row.address, PathOutcome::Gap(reason), evidence));
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
