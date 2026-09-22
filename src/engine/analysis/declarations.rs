//! Direct command registration sites and their documented declarations.
//! A direct branch in executable text is one site. An unresolved site remains a gap.
use std::collections::{BTreeMap, BTreeSet};

use super::{
    InputError,
    decode::{Instruction, decode_arm64},
};
use crate::DeclarationKind;

/// Name and revision of this static method.
pub const METHOD: &str = "command-declarations/v1";

/// A bounded executable code range.
#[derive(Debug, Clone)]
pub struct Function {
    pub address: u64,
    pub code: Vec<u8>,
}

/// Virtual method slots for one exact build.
#[derive(Debug, Clone, Copy)]
pub struct ScopeSlots {
    pub create: u64,
    pub supported_scopes: u64,
}

/// Executable-derived input for one command kind.
pub struct DeclarationInput {
    pub kind: DeclarationKind,
    pub tokens: BTreeMap<u64, String>,
    pub registrars: Vec<Function>,
    pub register_entry: BTreeSet<u64>,
    pub operator_new: BTreeSet<u64>,
    pub functions: BTreeMap<u64, Function>,
    pub pointers: BTreeMap<u64, u64>,
    pub strings: BTreeMap<u64, String>,
    pub slots: ScopeSlots,
    pub scope_names: Option<Vec<String>>,
    pub symbols_by_address: BTreeMap<u64, String>,
}

/// One directly registered command site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Site {
    Declared {
        name: String,
        description: String,
        usage: String,
        scopes: ScopeOutcome,
    },
    RuntimeToken {
        family: String,
    },
    Unreadable {
        name: Option<String>,
        what: &'static str,
    },
}

/// Scope mask followed through the factory and command virtual methods.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeOutcome {
    Any,
    Listed(Vec<String>),
    Unresolved(&'static str),
}

/// Every direct site and any input-wide gap.
pub struct DeclarationResult {
    pub sites: Vec<(u64, Site)>,
    pub table_gaps: Vec<&'static str>,
}

/// Analyze each direct registration call. The input reader owns executable layout.
pub fn analyze(input: &DeclarationInput) -> Result<DeclarationResult, InputError> {
    let mut sites = Vec::new();
    for registrar in &input.registrars {
        let rows = decode(registrar)?;
        let Some(row) = rows.last() else {
            continue;
        };
        if row.operation != "bl"
            || !target(row).is_some_and(|target| input.register_entry.contains(&target))
        {
            continue;
        }
        sites.push((row.address, site(input, &rows[..rows.len() - 1])));
    }
    sites.sort_by_key(|(address, _)| *address);
    sites.dedup_by_key(|(address, _)| *address);
    if sites.is_empty() {
        return Err(InputError("no direct registration sites".into()));
    }
    Ok(DeclarationResult {
        sites,
        table_gaps: if input.scope_names.is_none() {
            vec!["scope-table"]
        } else {
            Vec::new()
        },
    })
}

fn decode(function: &Function) -> Result<Vec<Instruction>, InputError> {
    let mut rows = Vec::new();
    for (index, chunk) in function.code.chunks(4096).enumerate() {
        rows.extend(
            decode_arm64(chunk, function.address + (index * 4096) as u64)
                .map_err(|error| InputError(error.to_string()))?,
        );
    }
    Ok(rows)
}

fn target(row: &Instruction) -> Option<u64> {
    number(row.operands.as_str())
}

fn number(text: &str) -> Option<u64> {
    let text = text.strip_prefix('#').unwrap_or(text);
    if let Some(negative) = text.strip_prefix('-') {
        return number(negative).map(|value| 0u64.wrapping_sub(value));
    }
    if let Some(hex) = text.strip_prefix("0x") {
        u64::from_str_radix(hex, 16).ok()
    } else {
        text.parse().ok()
    }
}

fn site(input: &DeclarationInput, rows: &[Instruction]) -> Site {
    let mut token = None;
    let site_start = rows
        .iter()
        .rposition(|row| {
            row.operation == "bl"
                && target(row).is_some_and(|address| input.register_entry.contains(&address))
        })
        .map_or(0, |index| index + 1);
    for row in rows[site_start.max(rows.len().saturating_sub(64))..]
        .iter()
        .rev()
    {
        if let Some(value) = row.operands.strip_prefix("w1,#")
            && matches!(row.operation.as_str(), "mov" | "movz")
        {
            token = number(value);
            break;
        }
        if row.operands.starts_with("w1,") && !matches!(row.operation.as_str(), "movk") {
            break;
        }
    }
    let Some(token) = token else {
        return Site::RuntimeToken {
            family: family(input, rows),
        };
    };
    let Some(name) = input.tokens.get(&token).cloned() else {
        return Site::Unreadable {
            name: None,
            what: "token-table",
        };
    };
    let Some(new_index) = rows.iter().rposition(|row| {
        row.operation == "bl"
            && target(row).is_some_and(|target| input.operator_new.contains(&target))
    }) else {
        return Site::Unreadable {
            name: Some(name),
            what: "entry-shape",
        };
    };
    let Some((factory, documentation)) = object_stores(rows, new_index + 1) else {
        return Site::Unreadable {
            name: Some(name),
            what: "entry-shape",
        };
    };
    let Some(documentation) = input.strings.get(&documentation) else {
        return Site::Unreadable {
            name: Some(name),
            what: "documentation",
        };
    };
    let (description, usage) = split_documentation(documentation);
    let scopes = scopes(input, factory);
    Site::Declared {
        name,
        description,
        usage,
        scopes,
    }
}

fn family(input: &DeclarationInput, rows: &[Instruction]) -> String {
    rows.iter()
        .rev()
        .filter(|row| row.operation == "bl")
        .filter_map(target)
        .filter_map(|address| input.symbols_by_address.get(&address))
        .find(|name| name.contains("Entry<"))
        .map(|_| "scripted command".to_owned())
        .unwrap_or_else(|| format!("unknown {} command", input.kind.subject()))
}

/// First line is the description. One terminal newline is formatting, not usage.
pub(crate) fn split_documentation(text: &str) -> (String, String) {
    let (description, usage) = text.split_once('\n').unwrap_or((text, ""));
    (
        description.into(),
        usage.strip_suffix('\n').unwrap_or(usage).into(),
    )
}

fn object_stores(rows: &[Instruction], since: usize) -> Option<(u64, u64)> {
    let mut values = BTreeMap::<&str, u64>::new();
    let mut factory = None;
    let mut documentation = None;
    for (index, row) in rows.iter().enumerate() {
        let args: Vec<_> = row.operands.split(',').collect();
        match (row.operation.as_str(), args.as_slice()) {
            ("adrp", [reg, value]) => {
                if let Some(value) = number(value) {
                    values.insert(reg, value);
                }
            }
            ("add", [dst, base, offset]) => {
                if let Some(value) = values
                    .get(base)
                    .and_then(|base| base.checked_add(number(offset)?))
                {
                    values.insert(dst, value);
                }
            }
            ("stp", [first, second, "[x0]"]) if index >= since => {
                factory = values.get(first).copied();
                documentation = values.get(second).copied();
            }
            ("str", [reg, "[x0]"]) if index >= since => factory = values.get(reg).copied(),
            ("str", [reg, "[x0", "#8]"]) if index >= since => {
                documentation = values.get(reg).copied()
            }
            _ => {}
        }
    }
    factory.zip(documentation)
}

fn vtable_store(rows: &[Instruction]) -> Option<u64> {
    let mut values = BTreeMap::<&str, u64>::new();
    let mut object_vtable = None;
    let mut result_vtable = None;
    for row in rows {
        let args: Vec<_> = row.operands.split(',').collect();
        match (row.operation.as_str(), args.as_slice()) {
            ("adrp", [reg, value]) => {
                if let Some(value) = number(value) {
                    values.insert(reg, value);
                }
            }
            ("add", [dst, base, offset]) => {
                if let Some(value) = values
                    .get(base)
                    .and_then(|base| base.checked_add(number(offset)?))
                {
                    values.insert(dst, value);
                }
            }
            ("stp", [first, _, "[x19]"]) => {
                if let Some(value) = values.get(first) {
                    result_vtable = Some(*value);
                }
            }
            ("str", [reg, "[x19]"]) => {
                if let Some(value) = values.get(reg) {
                    result_vtable = Some(*value);
                }
            }
            ("stp", [first, _, "[x0]"]) | ("str", [first, "[x0]"]) => {
                if let Some(value) = values.get(first) {
                    object_vtable = Some(*value);
                }
            }
            ("ldr", [reg, ..]) => {
                values.remove(reg);
            }
            _ => {}
        }
    }
    result_vtable.or(object_vtable)
}

fn scopes(input: &DeclarationInput, factory: u64) -> ScopeOutcome {
    let Some(create) = input.pointers.get(&(factory + input.slots.create)) else {
        return ScopeOutcome::Unresolved("factory-create");
    };
    let Some(rows) = create_rows(input, *create, 0) else {
        return ScopeOutcome::Unresolved("create-body");
    };
    if !rows.iter().any(|row| {
        row.operation == "bl"
            && target(row).is_some_and(|target| input.operator_new.contains(&target))
    }) {
        return ScopeOutcome::Unresolved("create-object");
    }
    let end = rows
        .iter()
        .position(|row| row.operation == "ret")
        .unwrap_or(rows.len());
    let command = vtable_store(&rows[..end])
        .filter(|address| {
            input
                .pointers
                .contains_key(&(address + input.slots.supported_scopes))
        })
        .or_else(|| {
            rows[..end]
                .iter()
                .filter(|row| row.operation == "bl")
                .filter_map(target)
                .filter_map(|address| create_rows(input, address, 0))
                .filter_map(|body| vtable_store(&body))
                .find(|address| {
                    input
                        .pointers
                        .contains_key(&(address + input.slots.supported_scopes))
                })
        });
    let Some(command) = command else {
        return ScopeOutcome::Unresolved("command-vtable");
    };
    let Some(getter) = input
        .pointers
        .get(&(command + input.slots.supported_scopes))
    else {
        return ScopeOutcome::Unresolved("scope-getter");
    };
    let Some(getter_body) = input.functions.get(getter) else {
        return ScopeOutcome::Unresolved("scope-getter");
    };
    let Ok(rows) = decode(getter_body) else {
        return ScopeOutcome::Unresolved("scope-getter");
    };
    let Some(mask) = constant_return(&rows) else {
        return ScopeOutcome::Unresolved("scope-mask");
    };
    if mask == 0 || mask == u64::MAX {
        return ScopeOutcome::Any;
    }
    let Some(names) = &input.scope_names else {
        return ScopeOutcome::Unresolved("scope-table");
    };
    let mut listed = Vec::new();
    for index in 0..64 {
        if mask & (1u64 << index) == 0 {
            continue;
        }
        let Some(name) = names.get(index).filter(|name| !name.is_empty()) else {
            return ScopeOutcome::Unresolved("scope-name");
        };
        listed.extend(name.split_whitespace().map(str::to_owned));
    }
    ScopeOutcome::Listed(listed)
}

fn create_rows(input: &DeclarationInput, address: u64, depth: usize) -> Option<Vec<Instruction>> {
    if depth > 2 {
        return None;
    }
    let body = input.functions.get(&address)?;
    let rows = decode(body).ok()?;
    if rows.first().is_some_and(|row| row.operation == "b") {
        return create_rows(input, target(&rows[0])?, depth + 1);
    }
    let end = rows
        .iter()
        .position(|row| row.operation == "ret")
        .unwrap_or(rows.len());
    Some(rows[..end].to_vec())
}

fn constant_return(rows: &[Instruction]) -> Option<u64> {
    let mut value = None;
    for row in rows.iter().take(64) {
        if row.operation == "ret" {
            return value;
        }
        let args: Vec<_> = row.operands.split(',').collect();
        match (row.operation.as_str(), args.as_slice()) {
            ("mov" | "movz", ["x0" | "w0", immediate]) => value = number(immediate),
            ("movn", ["x0", immediate]) => value = number(immediate).map(|part| !part),
            ("movn", ["w0", immediate]) => {
                value = number(immediate).map(|part| !(part as u32) as u64)
            }
            ("movk", ["x0" | "w0", immediate, shift]) => {
                let shift = shift.strip_prefix("lsl#").and_then(number)?;
                let mask = 0xffffu64 << shift;
                value = value
                    .zip(number(immediate))
                    .map(|(prior, part)| (prior & !mask) | (part << shift));
            }
            ("orr", ["x0", "xzr", immediate]) => value = number(immediate),
            _ if row.operands.starts_with("x0,") || row.operands.starts_with("w0,") => value = None,
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documentation_split_preserves_usage_without_terminal_newline() {
        assert_eq!(
            split_documentation("desc\nl1\nl2\n"),
            ("desc".into(), "l1\nl2".into())
        );
        assert_eq!(split_documentation("desc"), ("desc".into(), "".into()));
        assert_eq!(split_documentation("desc\n"), ("desc".into(), "".into()));
    }

    #[test]
    fn constant_getter_ends_at_return() {
        let row = |operation: &str, operands: &str| Instruction {
            address: 0,
            bytes: [0; 4],
            operation: operation.into(),
            operands: operands.into(),
        };
        assert_eq!(
            constant_return(&[row("mov", "x0,#0"), row("ret", "")]),
            Some(0)
        );
        assert_eq!(
            constant_return(&[row("ldr", "x0,[x1]"), row("ret", "")]),
            None
        );
    }
}
