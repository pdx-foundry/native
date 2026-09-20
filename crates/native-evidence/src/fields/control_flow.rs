//! Bounded reachability of token construction, with constant branch controls.
use super::tokens::number;
use crate::analysis::Instruction;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Copy, PartialEq, Eq)]
struct Constants {
    registers: [Option<u64>; 31],
    comparison: Option<(u64, u64, bool)>,
}
impl Constants {
    fn value(&self, operand: &str) -> Option<u64> {
        if operand.starts_with('#') {
            return number(operand).map(|v| v as u64);
        }
        if matches!(operand, "xzr" | "wzr") {
            return Some(0);
        }
        let index = operand
            .strip_prefix('x')
            .or_else(|| operand.strip_prefix('w'))?
            .parse::<usize>()
            .ok()?;
        let value = (*self.registers.get(index)?)?;
        Some(if operand.starts_with('w') {
            value & 0xffff_ffff
        } else {
            value
        })
    }
    fn assign(&mut self, destination: &str, value: Option<u64>) {
        let Some(index) = destination
            .strip_prefix('x')
            .or_else(|| destination.strip_prefix('w'))
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|&i| i < 31)
        else {
            return;
        };
        self.registers[index] = value.map(|v| {
            if destination.starts_with('w') {
                v & 0xffff_ffff
            } else {
                v
            }
        });
    }
    fn merge(&mut self, incoming: Self) -> bool {
        let before = *self;
        for (current, next) in self.registers.iter_mut().zip(incoming.registers) {
            if *current != next {
                *current = None;
            }
        }
        if self.comparison != incoming.comparison {
            self.comparison = None;
        }
        before != *self
    }
    fn condition(&self, condition: &str) -> Option<bool> {
        let (left, right, word) = self.comparison?;
        let (signed_left, signed_right) = if word {
            (left as u32 as i32 as i64, right as u32 as i32 as i64)
        } else {
            (left as i64, right as i64)
        };
        match condition {
            "eq" => Some(left == right),
            "ne" => Some(left != right),
            "lt" => Some(signed_left < signed_right),
            "le" => Some(signed_left <= signed_right),
            "gt" => Some(signed_left > signed_right),
            "ge" => Some(signed_left >= signed_right),
            _ => None,
        }
    }
    fn transfer(&mut self, row: &Instruction) -> Result<(), String> {
        let args: Vec<_> = row.operands.split(',').collect();
        match (row.operation.as_str(), args.as_slice()) {
            ("mov", [destination, source]) => self.assign(destination, self.value(source)),
            ("adrp", [destination, address]) => {
                self.assign(destination, number(address).map(|v| v as u64))
            }
            ("add" | "sub", [destination, left, right]) => {
                let value = self
                    .value(left)
                    .zip(self.value(right))
                    .map(|(left, right)| {
                        if row.operation == "add" {
                            left.wrapping_add(right)
                        } else {
                            left.wrapping_sub(right)
                        }
                    });
                self.assign(destination, value);
            }
            ("add" | "sub", [destination, left, right, shift]) => {
                let shift = shift
                    .strip_prefix("lsl")
                    .and_then(number)
                    .filter(|&n| n == 0 || n == 12)
                    .ok_or("unsupported shifted token-construction arithmetic")?;
                if !right.starts_with('#') {
                    return Err("nonliteral shifted token-construction arithmetic".into());
                }
                let value = self
                    .value(left)
                    .zip(self.value(right))
                    .map(|(left, right)| {
                        let right = right.wrapping_shl(shift as u32);
                        if row.operation == "add" {
                            left.wrapping_add(right)
                        } else {
                            left.wrapping_sub(right)
                        }
                    });
                self.assign(destination, value);
            }
            ("cmp", [left, right]) => {
                let word = left.starts_with('w');
                self.comparison = self
                    .value(left)
                    .zip(self.value(right))
                    .map(|(a, b)| (a, if word { b & 0xffff_ffff } else { b }, word));
            }
            ("bl", _) => {
                self.registers.fill(None);
                self.comparison = None;
            }
            ("ldr" | "ldrb" | "ldaprb", _) => {
                if row.operands.contains('!') || row.operands.contains("],") {
                    self.registers.fill(None);
                } else if let Some(destination) = args.first() {
                    self.assign(destination, None);
                }
            }
            ("ldp", _) => {
                if row.operands.contains('!') || row.operands.contains("],") {
                    self.registers.fill(None);
                } else {
                    for destination in args.iter().take(2) {
                        self.assign(destination, None);
                    }
                }
            }
            ("str" | "strb" | "stp", _) => {
                if row.operands.contains('!') || row.operands.contains("],") {
                    self.registers.fill(None);
                }
            }
            ("nop", _) => {}
            _ => {
                return Err(format!(
                    "unsupported token-construction instruction {}",
                    row.operation
                ));
            }
        }
        Ok(())
    }
}

fn successors(
    rows: &[Instruction],
    index: usize,
    state: &mut Constants,
    indexes: &BTreeMap<u64, usize>,
) -> Result<Vec<usize>, String> {
    let row = &rows[index];
    let args: Vec<_> = row.operands.split(',').collect();
    let target = || {
        args.last()
            .and_then(|s| number(s))
            .and_then(|a| indexes.get(&(a as u64)))
            .copied()
            .ok_or_else(|| "token-construction branch leaves the recorded function".to_string())
    };
    let choose = |condition: Option<bool>, target: usize| match condition {
        Some(true) => vec![target],
        Some(false) => vec![index + 1],
        None => vec![target, index + 1],
    };
    match row.operation.as_str() {
        "ret" => Ok(vec![]),
        // An unconditional external branch ends this function (a tail call).
        "b" => {
            let address =
                args.last()
                    .and_then(|s| number(s))
                    .ok_or("invalid token-construction branch target")? as u64;
            Ok(indexes.get(&address).copied().into_iter().collect())
        }
        "cbz" | "cbnz" if args.len() == 2 => Ok(choose(
            state
                .value(args[0])
                .map(|v| (v == 0) == (row.operation == "cbz")),
            target()?,
        )),
        "tbz" | "tbnz" if args.len() == 3 => {
            let bit = number(args[1])
                .filter(|&bit| (0..64).contains(&bit))
                .ok_or("invalid token-construction bit test")?;
            Ok(choose(
                state
                    .value(args[0])
                    .map(|v| (v & (1 << bit) == 0) == (row.operation == "tbz")),
                target()?,
            ))
        }
        condition if condition.starts_with("b.") => {
            if !matches!(&condition[2..], "eq" | "ne" | "lt" | "le" | "gt" | "ge") {
                return Err("unsupported token-construction branch condition".into());
            }
            Ok(choose(state.condition(&condition[2..]), target()?))
        }
        _ => {
            state.transfer(row)?;
            Ok(vec![index + 1])
        }
    }
}

/// Join only equal constants at merges; loops monotonically lose uncertain facts.
pub(super) fn reachable(rows: &[Instruction]) -> Result<BTreeSet<u64>, String> {
    if rows.is_empty() {
        return Err("empty token-construction function".into());
    }
    let indexes: BTreeMap<_, _> = rows
        .iter()
        .enumerate()
        .map(|(i, row)| (row.address, i))
        .collect();
    let mut states = vec![None; rows.len()];
    states[0] = Some(Constants {
        registers: [None; 31],
        comparison: None,
    });
    let mut pending = VecDeque::from([0]);
    let mut queued = vec![false; rows.len()];
    queued[0] = true;
    let mut budget = rows.len() * 40;
    while let Some(index) = pending.pop_front() {
        if budget == 0 {
            return Err("token-construction control-flow budget exhausted".into());
        }
        budget -= 1;
        queued[index] = false;
        let mut state = states[index].expect("queued reachable state");
        for next in successors(rows, index, &mut state, &indexes)? {
            if next >= rows.len() {
                return Err("token construction falls past the recorded function".into());
            }
            let changed = match &mut states[next] {
                Some(previous) => previous.merge(state),
                slot @ None => {
                    *slot = Some(state);
                    true
                }
            };
            if changed && !queued[next] {
                queued[next] = true;
                pending.push_back(next);
            }
        }
    }
    Ok(rows
        .iter()
        .zip(states)
        .filter(|(_, state)| state.is_some())
        .map(|(row, _)| row.address)
        .collect())
}
