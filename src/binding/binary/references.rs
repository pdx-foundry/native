use crate::AnalysisError;
use crate::engine::analysis::decode::{Instruction, decode_arm64};
use crate::engine::analysis::discovery::{StaticInput, Symbol};
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

fn recover_token_names(
    rows: &[Instruction],
    symbols: &[Symbol],
    strings: &BTreeMap<u64, String>,
) -> BTreeMap<i64, String> {
    let constructors: BTreeSet<_> = symbols
        .iter()
        .filter(|symbol| symbol.name == "CToken::CToken(int, char const*)")
        .map(|symbol| symbol.address)
        .collect();
    let mut names = BTreeMap::new();
    for (index, row) in rows.iter().enumerate() {
        let Some(token) = row
            .operands
            .strip_prefix("w1,")
            .filter(|_| row.operation == "mov")
            .and_then(parse_number)
        else {
            continue;
        };
        if index < 3 || index + 1 >= rows.len() {
            continue;
        }
        let page = &rows[index - 3];
        let offset = &rows[index - 2];
        let call = &rows[index + 1];
        let Some(page) = page
            .operands
            .strip_prefix("x2,")
            .filter(|_| page.operation == "adrp")
            .and_then(parse_number)
        else {
            continue;
        };
        let Some(offset) = offset
            .operands
            .strip_prefix("x2,x2,")
            .filter(|_| offset.operation == "add")
            .and_then(parse_number)
        else {
            continue;
        };
        let Some(target) = (call.operation == "bl")
            .then(|| parse_number(&call.operands))
            .flatten()
            .map(|address| address as u64)
        else {
            continue;
        };
        if constructors.contains(&target)
            && let Some(name) = page
                .checked_add(offset)
                .and_then(|address| strings.get(&(address as u64)))
        {
            names.insert(token as u32 as i32 as i64, name.clone());
        }
    }
    names
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
