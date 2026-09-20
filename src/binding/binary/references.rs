use crate::AnalysisError;
use crate::engine::analysis::decode::{Instruction, decode_arm64};
use crate::engine::analysis::discovery::{StaticInput, Symbol};
use crate::engine::analysis::fields::recover_token_names;
use crate::engine::analysis::references::{DecodedFunction, ReferenceInput};
use std::collections::{BTreeMap, BTreeSet};

const MAX_FUNCTION_BYTES: u64 = 1024 * 1024;

pub(in crate::binding) fn read(
    bytes: &[u8],
    discovery: &StaticInput,
    owner: &str,
) -> Result<ReferenceInput, AnalysisError> {
    let mut functions = Vec::new();
    let initializer_name = format!("{owner}::PostInit()");
    if unique_address(&discovery.symbols, &initializer_name).is_none() {
        return Ok(ReferenceInput {
            target_available: true,
            owner: owner.into(),
            functions,
            symbols: discovery.symbols.clone(),
            global_bindings: discovery.global_bindings.clone(),
            token_names: BTreeMap::new(),
        });
    }
    let initializer = read_function(bytes, &discovery.symbols, &initializer_name)?;
    let initializer_calls = called_functions(&initializer.instructions, &discovery.symbols);
    functions.push(initializer);

    let reader_name = format!("{owner}::ReadMember(CReader&, int, EScopeType)");
    let has_reader = unique_address(&discovery.symbols, &reader_name).is_some();
    if has_reader {
        functions.push(read_function(bytes, &discovery.symbols, &reader_name)?);
    }
    for name in initializer_calls
        .into_iter()
        .filter(|name| name.contains("Database::") && name.contains("(CString const&) const"))
        .take(4)
    {
        functions.push(read_function(bytes, &discovery.symbols, &name)?);
    }

    let token_names = if has_reader {
        let token_function = read_function(bytes, &discovery.symbols, "GetTokenArray()")?;
        recover_token_names(
            &token_function.instructions,
            &discovery.symbols,
            &discovery.strings,
        )
    } else {
        BTreeMap::new()
    };
    Ok(ReferenceInput {
        target_available: true,
        owner: owner.into(),
        functions,
        symbols: discovery.symbols.clone(),
        global_bindings: discovery.global_bindings.clone(),
        token_names,
    })
}

fn read_function(
    bytes: &[u8],
    symbols: &[Symbol],
    name: &str,
) -> Result<DecodedFunction, AnalysisError> {
    let start = unique_address(symbols, name).ok_or(AnalysisError::InvalidRange)?;
    let end = symbols
        .iter()
        .filter(|symbol| symbol.address > start)
        .map(|symbol| symbol.address)
        .min()
        .ok_or(AnalysisError::InvalidRange)?;
    let length = end.checked_sub(start).ok_or(AnalysisError::InvalidRange)?;
    if length == 0 || length > MAX_FUNCTION_BYTES || !length.is_multiple_of(4) {
        return Err(AnalysisError::InvalidRange);
    }
    let mut instructions = Vec::new();
    for offset in (0..length).step_by(4096) {
        let address = start + offset;
        let code = super::code_range(bytes, address, (length - offset).min(4096))?;
        instructions.extend(decode_arm64(&code, address).map_err(|_| AnalysisError::InvalidRange)?);
    }
    Ok(DecodedFunction {
        name: name.into(),
        instructions,
    })
}

fn unique_address(symbols: &[Symbol], name: &str) -> Option<u64> {
    let addresses: BTreeSet<_> = symbols
        .iter()
        .filter(|symbol| symbol.name == name)
        .map(|symbol| symbol.address)
        .collect();
    (addresses.len() == 1).then(|| *addresses.first().unwrap())
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

fn called_functions(rows: &[Instruction], symbols: &[Symbol]) -> BTreeSet<String> {
    let names = names_by_address(symbols);
    rows.iter()
        .filter(|row| row.operation == "bl")
        .filter_map(|row| parse_number(&row.operands).map(|address| address as u64))
        .filter_map(|address| names.get(&address).copied().flatten())
        .map(str::to_owned)
        .collect()
}

fn parse_number(value: &str) -> Option<i64> {
    let value = value.strip_prefix('#').unwrap_or(value);
    let (negative, digits) = value
        .strip_prefix('-')
        .map_or((false, value), |digits| (true, digits));
    let number = digits.strip_prefix("0x").map_or_else(
        || digits.parse().ok(),
        |hex| i64::from_str_radix(hex, 16).ok(),
    )?;
    Some(if negative { -number } else { number })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(address: u64, operation: &str, operands: &str) -> Instruction {
        Instruction {
            address,
            operation: operation.into(),
            operands: operands.into(),
            bytes: [0; 4],
        }
    }

    fn symbols() -> Vec<Symbol> {
        vec![Symbol {
            name: "CToken::CToken(int, char const*)".into(),
            address: 0x3000,
        }]
    }

    fn constructor(address: u64, token: i64, string: u64) -> Vec<Instruction> {
        vec![
            row(address, "adrp", "x2,#0x2000"),
            row(
                address + 4,
                "add",
                &format!("x2,x2,#{:#x}", string - 0x2000),
            ),
            row(address + 8, "nop", ""),
            row(address + 12, "mov", &format!("w1,#{token}")),
            row(address + 16, "bl", "#0x3000"),
        ]
    }

    #[test]
    fn clobbered_name_is_not_recovered() {
        let rows = vec![
            row(0x1000, "adrp", "x2,#0x2000"),
            row(0x1004, "add", "x2,x2,#0"),
            row(0x1008, "mov", "x2,xzr"),
            row(0x100c, "mov", "w1,#10000"),
            row(0x1010, "bl", "#0x3000"),
        ];
        assert!(
            recover_token_names(
                &rows,
                &symbols(),
                &BTreeMap::from([(0x2000, "invented".into())]),
            )
            .is_empty()
        );
    }

    #[test]
    fn branch_bypass_does_not_recover_arguments() {
        let rows = vec![
            row(0x1000, "adrp", "x2,#0x2000"),
            row(0x1004, "add", "x2,x2,#0"),
            row(0x1008, "b", "#0x1010"),
            row(0x100c, "mov", "w1,#10000"),
            row(0x1010, "bl", "#0x3000"),
        ];
        assert!(
            recover_token_names(
                &rows,
                &symbols(),
                &BTreeMap::from([(0x2000, "bypassed".into())]),
            )
            .is_empty()
        );
    }

    #[test]
    fn ambiguous_constructor_address_is_not_authoritative() {
        let rows = constructor(0x1000, 10_000, 0x2000);
        let mut symbols = symbols();
        symbols.push(Symbol {
            name: "Unrelated::Function()".into(),
            address: 0x3000,
        });
        assert!(
            recover_token_names(
                &rows,
                &symbols,
                &BTreeMap::from([(0x2000, "ambiguous".into())]),
            )
            .is_empty()
        );
    }

    #[test]
    fn conflicting_names_do_not_choose_one() {
        let mut rows = constructor(0x1000, 10_000, 0x2000);
        rows.extend(constructor(0x1020, 10_000, 0x2010));
        assert!(
            recover_token_names(
                &rows,
                &symbols(),
                &BTreeMap::from([(0x2000, "first".into()), (0x2010, "second".into()),]),
            )
            .is_empty()
        );
    }
}
