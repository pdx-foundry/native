use super::{FieldGap, FieldGapKind, FieldInput, Function};
use crate::engine::analysis::decode::{Instruction, decode_arm64};
use crate::engine::analysis::discovery::Symbol;
use crate::engine::analysis::stop::{Obstacle, Unknown, Unresolved};
use std::collections::BTreeMap;

pub(super) fn number(operand: &str) -> Option<i64> {
    let operand = operand.strip_prefix('#').unwrap_or(operand);
    let (negative, digits) = operand
        .strip_prefix('-')
        .map_or((false, operand), |s| (true, s));
    let value = if let Some(hex) = digits.strip_prefix("0x") {
        i64::from_str_radix(hex, 16).ok()?
    } else {
        digits.parse().ok()?
    };
    if negative {
        value.checked_neg()
    } else {
        Some(value)
    }
}
pub(super) fn register(operand: &str) -> Option<String> {
    if matches!(operand, "sp" | "xzr" | "wzr") {
        return Some(if operand == "wzr" { "xzr" } else { operand }.into());
    }
    let digits = operand
        .strip_prefix('x')
        .or_else(|| operand.strip_prefix('w'))?;
    let index: u8 = digits.parse().ok()?;
    (index <= 30).then(|| format!("x{index}"))
}
pub(super) fn decode(function: &Function) -> Result<Vec<Instruction>, String> {
    if function.code.is_empty() || function.code.len() > 1024 * 1024 {
        return Err("missing or oversized function bytes".into());
    }
    decode_arm64(&function.code, function.address).map_err(|e| e.to_string())
}
pub(super) fn function<'a>(
    functions: &'a [Function],
    symbols: &[Symbol],
    name: &str,
) -> Option<&'a Function> {
    let mut matches = functions.iter().filter(|f| f.name == name);
    let first = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    let mut symbols = symbols.iter().filter(|s| s.name == name);
    let symbol = symbols.next()?;
    (symbol.address == first.address && symbols.all(|s| s.address == first.address))
        .then_some(first)
}
pub(super) fn names_by_address(symbols: &[Symbol]) -> BTreeMap<u64, Option<&str>> {
    let mut names = BTreeMap::new();
    for symbol in symbols {
        names
            .entry(symbol.address)
            .and_modify(|name| {
                if *name != Some(symbol.name.as_str()) {
                    *name = None;
                }
            })
            .or_insert(Some(symbol.name.as_str()));
    }
    names
}
#[derive(Debug)]
pub(crate) struct Token {
    pub name: String,
    pub constructor: u64,
    pub ambiguous: bool,
}
pub(super) fn recover(input: &FieldInput) -> (BTreeMap<i64, Token>, Vec<FieldGap>) {
    let rows = match function(&input.functions, &input.symbols, "GetTokenArray()")
        .ok_or("token constructor function missing or ambiguous".into())
        .and_then(decode)
    {
        Ok(rows) => rows,
        Err(reason) => return (BTreeMap::new(), vec![token_table_gap(reason)]),
    };
    recover_decoded(&rows, &input.symbols, &input.strings)
}
fn token_table_gap(reason: impl Into<String>) -> FieldGap {
    FieldGap::new(FieldGapKind::TokenTable, reason)
}
pub(crate) fn recover_decoded(
    rows: &[Instruction],
    symbols: &[Symbol],
    strings: &BTreeMap<u64, String>,
) -> (BTreeMap<i64, Token>, Vec<FieldGap>) {
    let mut tokens = BTreeMap::<i64, Token>::new();
    let mut gaps = Vec::new();
    let reachable = match super::control_flow::reachable(rows) {
        Ok(reachable) => reachable,
        Err(unresolved) => {
            let gap = FieldGap::unresolved(FieldGapKind::TokenTable, unresolved);
            return (tokens, vec![gap]);
        }
    };
    let branch_targets: std::collections::BTreeSet<_> = rows
        .iter()
        .filter(|row| {
            row.operation == "b"
                || row.operation.starts_with("b.")
                || matches!(row.operation.as_str(), "cbz" | "cbnz" | "tbz" | "tbnz")
        })
        .filter_map(|row| row.operands.rsplit(',').next().and_then(number))
        .map(|address| address as u64)
        .collect();
    let names = names_by_address(symbols);
    let constructors: std::collections::BTreeSet<_> = names
        .iter()
        .filter(|(_, name)| **name == Some("CToken::CToken(int, char const*)"))
        .map(|(address, _)| *address)
        .collect();
    for (index, row) in rows.iter().enumerate() {
        if row.operation != "bl" || !reachable.contains(&row.address) {
            continue;
        }
        if !number(&row.operands).is_some_and(|a| constructors.contains(&(a as u64))) {
            continue;
        }
        let mut values = BTreeMap::<String, i64>::new();
        for prior in &rows[index.saturating_sub(8)..index] {
            if !reachable.contains(&prior.address) {
                values.clear();
                continue;
            }
            // A branch can bypass earlier argument definitions, including a branch outside
            // the lookback window. Only values re-established after its target survive.
            if branch_targets.contains(&prior.address) {
                values.clear();
            }

            let args: Vec<_> = prior.operands.split(',').collect();
            let destination = args.first().and_then(|s| register(s));
            let recovered = match (prior.operation.as_str(), args.as_slice()) {
                ("adrp", ["x2", address]) => number(address),
                ("add", ["x2", "x2", offset]) => values
                    .get("x2")
                    .and_then(|base| base.checked_add(number(offset)?)),
                ("mov", ["w1", value]) if value.starts_with('#') => {
                    number(value).map(|v| v as u32 as i32 as i64)
                }
                _ => None,
            };
            if prior.operands.contains('!')
                || prior.operands.contains("],")
                || !matches!(
                    prior.operation.as_str(),
                    "adrp" | "add" | "mov" | "ldr" | "ldp" | "str" | "stp" | "sub" | "nop"
                )
            {
                values.clear();
            } else if let Some(destination) = destination {
                // Unknown writes to either alias invalidate constructor arguments. Other writes
                // cannot establish a token or a pointer, even if their textual operand resembles one.
                values.remove(&destination);
                if let Some(value) = recovered {
                    values.insert(destination, value);
                }
                if prior.operation == "ldp"
                    && let Some(second) = args.get(1).and_then(|s| register(s))
                {
                    values.remove(&second);
                }
            }
        }
        if branch_targets.contains(&row.address) {
            values.clear();
        }
        let unknown = |index| {
            let argument = Obstacle::Unknown(Unknown::Register(index));
            let entry = rows[0].address;
            let stop = Unresolved::at("constructor-argument", row.address, entry, argument);
            FieldGap::unresolved(FieldGapKind::TokenTable, stop)
        };
        let (Some(&token), Some(&address)) = (values.get("x1"), values.get("x2")) else {
            gaps.push(unknown(if values.contains_key("x1") { 2 } else { 1 }));
            continue;
        };
        let Some(name) = strings.get(&(address as u64)) else {
            gaps.push(token_table_gap(format!(
                "no literal at {address:#x} for the token constructor call at {:#x}",
                row.address
            )));
            continue;
        };
        if let Some(previous) = tokens.get_mut(&token) {
            if previous.name != *name {
                previous.ambiguous = true;
                gaps.push(token_table_gap(format!(
                    "conflicting names for token {token}"
                )));
            }
        } else {
            tokens.insert(
                token,
                Token {
                    name: name.clone(),
                    constructor: row.address,
                    ambiguous: false,
                },
            );
        }
    }
    if tokens.is_empty() {
        gaps.push(token_table_gap(
            "no literal token constructor pairs recovered",
        ));
    }
    (tokens, gaps)
}
