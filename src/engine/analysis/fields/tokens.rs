use super::{FieldInput, Function};
use crate::engine::analysis::decode::{Instruction, decode_arm64};
use crate::engine::analysis::discovery::Symbol;
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
    let mut instructions = Vec::new();
    for (i, chunk) in function.code.chunks(4096).enumerate() {
        let address = function
            .address
            .checked_add((i * 4096) as u64)
            .ok_or("function address overflow")?;
        instructions.extend(decode_arm64(chunk, address).map_err(|e| e.to_string())?);
    }
    Ok(instructions)
}
pub(super) fn function<'a>(input: &'a FieldInput, name: &str) -> Option<&'a Function> {
    let mut matches = input.functions.iter().filter(|f| f.name == name);
    let first = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    let mut symbols = input.symbols.iter().filter(|s| s.name == name);
    let symbol = symbols.next()?;
    (symbol.address == first.address && symbols.all(|s| s.address == first.address))
        .then_some(first)
}
pub(super) fn symbol_names(input: &FieldInput) -> BTreeMap<u64, Option<&str>> {
    names_by_address(&input.symbols)
}
fn names_by_address(symbols: &[Symbol]) -> BTreeMap<u64, Option<&str>> {
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
pub(super) struct Token {
    pub name: String,
    pub constructor: u64,
    pub ambiguous: bool,
}
pub(super) fn recover(input: &FieldInput) -> (BTreeMap<i64, Token>, Vec<String>) {
    let rows = match function(input, "GetTokenArray()")
        .ok_or("token constructor function missing or ambiguous".into())
        .and_then(decode)
    {
        Ok(rows) => rows,
        Err(reason) => return (BTreeMap::new(), vec![reason]),
    };
    recover_decoded(&rows, &input.symbols, &input.strings)
}
pub(super) fn recover_decoded(
    rows: &[Instruction],
    symbols: &[Symbol],
    strings: &BTreeMap<u64, String>,
) -> (BTreeMap<i64, Token>, Vec<String>) {
    let mut tokens = BTreeMap::<i64, Token>::new();
    let mut gaps = Vec::new();
    let reachable = match super::control_flow::reachable(rows) {
        Ok(reachable) => reachable,
        Err(reason) => return (tokens, vec![reason]),
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
        let pair = values
            .get("x1")
            .zip(values.get("x2"))
            .and_then(|(&token, &address)| {
                strings.get(&(address as u64)).map(|name| (token, name))
            });
        let Some((token, name)) = pair else {
            gaps.push(format!(
                "unsupported token constructor arguments or missing literal at {:#x}",
                row.address
            ));
            continue;
        };
        if let Some(previous) = tokens.get_mut(&token) {
            if previous.name != *name {
                previous.ambiguous = true;
                gaps.push(format!("conflicting names for token {token}"));
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
        gaps.push("no literal token constructor pairs recovered".into());
    }
    (tokens, gaps)
}
