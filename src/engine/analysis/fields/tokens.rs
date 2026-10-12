use super::{FieldGap, FieldGapKind, FieldInput, Function};
use crate::engine::analysis::decode::{Instruction, decode_arm64, general_register};
use crate::engine::analysis::discovery::Symbol;
use crate::engine::analysis::stop::{Obstacle, Unknown, Unresolved};
use std::collections::{BTreeMap, BTreeSet};

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
/// The 64-bit name of register `operand`, such as `x8` for `w8` and `xzr` for `wzr`; `sp`
/// stays `sp`. `None` for any other operand.
pub(super) fn register(operand: &str) -> Option<String> {
    if matches!(operand, "sp" | "xzr" | "wzr") {
        return Some(if operand == "wzr" { "xzr" } else { operand }.into());
    }
    general_register(operand).map(|index| format!("x{index}"))
}
/// Named function input borrowed from either registry or command discovery.
#[derive(Clone, Copy)]
pub(super) struct FunctionView<'a> {
    pub name: &'a str,
    pub address: u64,
    pub code: &'a [u8],
}

impl<'a> From<&'a Function> for FunctionView<'a> {
    fn from(function: &'a Function) -> Self {
        Self {
            name: &function.name,
            address: function.address,
            code: &function.code,
        }
    }
}

pub(super) fn decode(function: FunctionView<'_>) -> Result<Vec<Instruction>, String> {
    if function.code.is_empty() || function.code.len() > 1024 * 1024 {
        return Err("missing or oversized function bytes".into());
    }
    decode_arm64(function.code, function.address).map_err(|e| e.to_string())
}

pub(super) fn function<'a, 's>(
    functions: impl IntoIterator<Item = FunctionView<'a>>,
    symbols: impl IntoIterator<Item = &'s Symbol>,
    name: &str,
) -> Option<FunctionView<'a>> {
    let mut matches = functions.into_iter().filter(|f| f.name == name);
    let first = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    let mut symbols = symbols.into_iter().filter(|s| s.name == name);
    let symbol = symbols.next()?;
    (symbol.address == first.address && symbols.all(|s| s.address == first.address))
        .then_some(first)
}
/// A token that the engine's token constructor names.
#[derive(Debug)]
pub(crate) struct Token {
    /// The literal name, or `None` once constructor calls give the token different names.
    pub name: Option<String>,
    pub constructor: u64,
}

/// The token and name of the one key that `domain` holds: a singleton interval whose token has
/// one name.
pub(crate) fn named_key(domain: [i64; 2], tokens: &BTreeMap<i64, Token>) -> Option<(&Token, &str)> {
    if domain[0] != domain[1] {
        return None;
    }
    let token = tokens.get(&domain[0])?;

    Some((token, token.name.as_deref()?))
}
pub(super) fn recover(input: &FieldInput) -> (BTreeMap<i64, Token>, Vec<FieldGap>) {
    let rows = match function(
        input.functions.iter().map(FunctionView::from),
        &input.symbols,
        "GetTokenArray()",
    )
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
    let branch_targets: BTreeSet<_> = rows
        .iter()
        .filter(|row| {
            row.operation == "b"
                || row.operation.starts_with("b.")
                || matches!(row.operation.as_str(), "cbz" | "cbnz" | "tbz" | "tbnz")
        })
        .filter_map(|row| row.operands.rsplit(',').next().and_then(number))
        .map(|address| address as u64)
        .collect();
    let constructors = token_constructors(symbols);
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
            if previous.name.as_ref().is_some_and(|known| known != name) {
                previous.name = None;
                gaps.push(token_table_gap(format!(
                    "conflicting names for token {token}"
                )));
            }
        } else {
            tokens.insert(
                token,
                Token {
                    name: Some(name.clone()),
                    constructor: row.address,
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

/// The addresses whose every symbol is the token constructor.
fn token_constructors(symbols: &[Symbol]) -> BTreeSet<u64> {
    const CONSTRUCTOR: &str = "CToken::CToken(int, char const*)";
    let mut constructors: BTreeSet<_> = symbols
        .iter()
        .filter(|symbol| symbol.name == CONSTRUCTOR)
        .map(|symbol| symbol.address)
        .collect();

    for symbol in symbols.iter().filter(|symbol| symbol.name != CONSTRUCTOR) {
        constructors.remove(&symbol.address);
    }

    constructors
}

#[cfg(test)]
mod function_tests {
    use super::*;

    #[test]
    fn borrowed_functions_keep_name_address_and_body_guards() {
        let body = Function {
            name: "root".into(),
            address: 0x1000,
            code: 0xd65f03c0_u32.to_le_bytes().to_vec(),
        };
        let view = FunctionView::from(&body);
        let symbol = Symbol {
            name: body.name.clone(),
            address: body.address,
        };
        let symbols = [symbol.clone(), symbol];
        let found = function([view], &symbols, "root").unwrap();
        assert_eq!(found.code.as_ptr(), body.code.as_ptr());
        assert!(decode(found).is_ok());
        assert!(function([view, view], &symbols, "root").is_none());
        let mut conflicting = symbols;
        conflicting[1].address += 4;
        assert!(function([view], &conflicting, "root").is_none());
        assert!(function([view], &[], "root").is_none());
        assert!(function([view], &conflicting, "other").is_none());
        for bytes in [vec![], vec![0; 1024 * 1024 + 4]] {
            assert!(
                decode(FunctionView {
                    code: &bytes,
                    ..view
                })
                .is_err()
            );
        }
    }
}

#[cfg(test)]
mod key_tests {
    use super::*;

    #[test]
    fn a_token_constructor_address_has_no_other_name() {
        let symbol = |name: &str, address| Symbol {
            name: name.into(),
            address,
        };
        let constructor = "CToken::CToken(int, char const*)";
        let symbols = [
            symbol("other", 0x10),
            symbol(constructor, 0x10),
            symbol(constructor, 0x20),
            symbol(constructor, 0x30),
            symbol("other", 0x30),
            symbol(constructor, 0x40),
            symbol(constructor, 0x40),
        ];

        assert_eq!(token_constructors(&symbols), BTreeSet::from([0x20, 0x40]));
    }

    #[test]
    fn a_named_key_is_a_singleton_interval_of_one_named_token() {
        let named = |name: Option<&str>| Token {
            name: name.map(String::from),
            constructor: 0x40,
        };
        let tokens = BTreeMap::from([(7, named(Some("limit"))), (8, named(None))]);

        let (token, name) = named_key([7, 7], &tokens).unwrap();
        assert_eq!((token.constructor, name), (0x40, "limit"));
        assert!(named_key([7, 8], &tokens).is_none(), "a wider interval");
        assert!(named_key([8, 8], &tokens).is_none(), "a conflicting name");
        assert!(named_key([9, 9], &tokens).is_none(), "an unknown token");
    }
}
