//! Command registration sites and their documented declarations.
//!
//! A registration site is a call or tail call to the database's register function, or to a
//! registry helper constructor that inserts its entry itself. At most sites the token and the
//! entry's documentation are literals next to the call. Where the code composes the token at run
//! time, `composition` runs the registering function from each chain of its callers and reads the
//! composed name and documentation at the call. A site that cannot be followed remains a gap.
use std::collections::{BTreeMap, BTreeSet};

use super::{
    InputError,
    decode::{Instruction, decode_arm64},
    stop::Unresolved,
};
mod composition;

pub use composition::{CALLER_DEPTH, Composition};

/// Name and revision of this static method.
pub const METHOD: &str = "command-declarations/v3";

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
    pub tokens: BTreeMap<u64, String>,
    /// Code that ends at each call or tail call to a registration function.
    pub registrars: Vec<Function>,
    /// The database's register function: the token in `w1`, the entry in `x2`.
    pub register_entry: BTreeSet<u64>,
    /// Registry helper constructors that insert their entry themselves: the token in `w1`, the
    /// documentation text in `x2`.
    pub entry_helpers: BTreeSet<u64>,
    pub operator_new: BTreeSet<u64>,
    pub functions: BTreeMap<u64, Function>,
    pub pointers: BTreeMap<u64, u64>,
    pub strings: BTreeMap<u64, String>,
    pub slots: ScopeSlots,
    pub scope_names: Option<Vec<String>>,
    pub composition: Composition,
}

/// One registration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Site {
    Declared {
        name: String,
        description: String,
        usage: String,
        scopes: ScopeOutcome,
    },
    /// The code composes the token at run time, and the method stopped at `obstacle`.
    RuntimeToken { obstacle: &'static str },
    Unreadable {
        name: Option<String>,
        what: &'static str,
    },
}

/// Scope mask followed through the factory and command virtual methods.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeOutcome {
    Any,
    Listed(Vec<ScopeType>),
    Unresolved(Unresolved),
}

/// One scope type: its bit in the engine's scope-type mask, which identifies it, and the engine's
/// display name, which two types can share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeType {
    pub bit: usize,
    pub name: String,
}

/// Every registration, by the address of its registration call, and any input-wide gap. A call
/// that the code reaches from several callers has one registration for each.
pub struct DeclarationResult {
    pub sites: Vec<(u64, Site)>,
    pub table_gaps: Vec<&'static str>,
}

/// The kind of registration function that a call reaches.
#[derive(Debug, Clone, Copy)]
enum Registrar {
    Entry,
    Helper(u64),
}

/// Analyze each registration call. The input reader owns executable layout.
pub fn analyze(input: &DeclarationInput) -> Result<DeclarationResult, InputError> {
    let mut composer = composition::Composer::new(input);
    let mut sites = Vec::new();
    for registrar in &input.registrars {
        let rows = decode(registrar)?;
        let Some((row, rows)) = rows.split_last() else {
            continue;
        };
        let Some(registrar) = registrar_of(input, row) else {
            continue;
        };
        let site = match registrar {
            Registrar::Entry => site(input, rows),
            Registrar::Helper(helper) => helper_site(input, rows, helper),
        };
        match (site, registrar) {
            (Site::RuntimeToken { .. }, Registrar::Entry) => sites.extend(
                composer
                    .registrations(row.address)
                    .into_iter()
                    .map(|site| (row.address, site)),
            ),
            (site, _) => sites.push((row.address, site)),
        }
    }
    sites.sort_by_key(|(address, _)| *address);
    if sites.is_empty() {
        return Err(InputError("no registration sites".into()));
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

/// The registration function that `row` calls or tail-calls.
fn registrar_of(input: &DeclarationInput, row: &Instruction) -> Option<Registrar> {
    if !matches!(row.operation.as_str(), "bl" | "b") {
        return None;
    }
    let target = target(row)?;
    if input.register_entry.contains(&target) {
        Some(Registrar::Entry)
    } else if input.entry_helpers.contains(&target) {
        Some(Registrar::Helper(target))
    } else {
        None
    }
}

fn decode(function: &Function) -> Result<Vec<Instruction>, InputError> {
    decode_arm64(&function.code, function.address).map_err(|error| InputError(error.to_string()))
}

fn target(row: &Instruction) -> Option<u64> {
    number(row.operands.as_str())
}

pub(crate) fn number(text: &str) -> Option<u64> {
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
    let name = match literal_name(input, rows) {
        Ok(name) => name,
        Err(site) => return site,
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
    declared(input, name, factory, documentation)
}

/// A call to a registry helper constructor: the documentation is the text in `x2`, and the
/// helper builds the entry with its own factory.
fn helper_site(input: &DeclarationInput, rows: &[Instruction], helper: u64) -> Site {
    let name = match literal_name(input, rows) {
        Ok(name) => name,
        Err(site) => return site,
    };
    let documentation = register_values(rows).get("x2").copied();
    let (Some(factory), Some(documentation)) = (helper_factory(input, helper), documentation)
    else {
        return Site::Unreadable {
            name: Some(name),
            what: "entry-shape",
        };
    };
    declared(input, name, factory, documentation)
}

fn declared(input: &DeclarationInput, name: String, factory: u64, documentation: u64) -> Site {
    let Some(documentation) = input.strings.get(&documentation) else {
        return Site::Unreadable {
            name: Some(name),
            what: "documentation",
        };
    };
    let (description, usage) = split_documentation(documentation);
    Site::Declared {
        name,
        description,
        usage,
        scopes: scopes(input, factory),
    }
}

/// The name of the literal token in `w1`, set since the previous registration call. A token that
/// the code computes is a `RuntimeToken`.
fn literal_name(input: &DeclarationInput, rows: &[Instruction]) -> Result<String, Site> {
    let site_start = rows
        .iter()
        .rposition(|row| registrar_of(input, row).is_some())
        .map_or(0, |index| index + 1);
    let mut token = None;
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
        if (row.operands.starts_with("w1,") || row.operands.starts_with("x1,"))
            && !matches!(row.operation.as_str(), "movk" | "str" | "stp" | "stur")
        {
            break;
        }
    }
    let token = token.ok_or(Site::RuntimeToken { obstacle: "token" })?;
    input.tokens.get(&token).cloned().ok_or(Site::Unreadable {
        name: None,
        what: "token-table",
    })
}

/// The entry factory of a registry helper constructor: the object that it allocates holds the
/// factory vtable and then the documentation argument from `x2`.
fn helper_factory(input: &DeclarationInput, helper: u64) -> Option<u64> {
    let rows = decode(input.functions.get(&helper)?).ok()?;
    let new_index = rows.iter().position(|row| {
        row.operation == "bl"
            && target(row).is_some_and(|target| input.operator_new.contains(&target))
    })?;
    let mut documentation = BTreeSet::from(["x2"]);
    for row in &rows[..new_index] {
        if let ("mov", [destination, source]) = (
            row.operation.as_str(),
            row.operands.split(',').collect::<Vec<_>>().as_slice(),
        ) && documentation.contains(source)
        {
            documentation.insert(*destination);
        }
    }
    rows.iter()
        .enumerate()
        .skip(new_index + 1)
        .find_map(|(index, row)| {
            let args: Vec<_> = row.operands.split(',').collect();
            match (row.operation.as_str(), args.as_slice()) {
                ("stp", [factory, second, "[x0]"]) if documentation.contains(second) => {
                    register_values(&rows[..index]).get(factory).copied()
                }
                _ => None,
            }
        })
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

/// The address that each register holds at the end of `rows`, when an `adrp` and `add` pair set
/// it. Any other write to a register, and a call for the caller-saved registers, forgets its
/// value.
fn register_values(rows: &[Instruction]) -> BTreeMap<&str, u64> {
    let mut values = BTreeMap::<&str, u64>::new();
    for row in rows {
        let args: Vec<_> = row.operands.split(',').collect();
        match (row.operation.as_str(), args.as_slice()) {
            ("adrp", [reg, value]) => match number(value) {
                Some(value) => {
                    values.insert(reg, value);
                }
                None => {
                    values.remove(reg);
                }
            },
            ("add", [dst, base, offset]) => {
                match values
                    .get(base)
                    .and_then(|base| base.checked_add(number(offset)?))
                {
                    Some(value) => values.insert(dst, value),
                    None => values.remove(dst),
                };
            }
            ("bl" | "blr", _) => values.retain(|reg, _| {
                !reg.strip_prefix('x')
                    .and_then(|index| index.parse::<u8>().ok())
                    .is_some_and(|index| index <= 18)
            }),
            (operation, [destination, ..]) if !operation.starts_with("st") => {
                let register = destination.replacen('w', "x", 1);
                values.retain(|reg, _| **reg != register);
            }
            _ => {}
        }
    }
    values
}

/// The vtable that the code stores in the new object: at `[x19]`, or a copy of `x19`, when a
/// constructor call intervenes, otherwise at `[x0]`. A post-indexed store writes at the base
/// address too. A vtable address can come through a pointer in `pointers`, as a load from the
/// global offset table does.
fn vtable_store(rows: &[Instruction], pointers: &BTreeMap<u64, u64>) -> Option<u64> {
    let mut values = BTreeMap::<&str, u64>::new();
    let mut object_bases = BTreeSet::from(["[x19]".to_owned()]);
    let mut x0_object_vtable = None;
    let mut x19_object_vtable = None;
    for row in rows {
        let args: Vec<_> = row.operands.split(',').collect();
        let stored = match (row.operation.as_str(), args.as_slice()) {
            ("stp", [first, _, base]) | ("str", [first, base]) => Some((*first, *base, false)),
            ("stp", [first, _, base, _]) | ("str", [first, base, _]) => Some((*first, *base, true)),
            _ => None,
        };
        if let Some((first, base, post_indexed)) = stored.filter(|(_, base, _)| base.ends_with(']'))
        {
            let value = values.get(first).copied();
            if object_bases.contains(base) {
                x19_object_vtable = value.or(x19_object_vtable);
            } else if base == "[x0]" {
                x0_object_vtable = value.or(x0_object_vtable);
            }
            if post_indexed {
                object_bases.remove(base);
            }
            continue;
        }
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
            ("mov", [dst, "x19"]) => {
                object_bases.insert(format!("[{dst}]"));
                continue;
            }
            ("ldr", [reg, base, offset]) if base.starts_with('[') && offset.ends_with(']') => {
                match values
                    .get(&base[1..])
                    .zip(number(&offset[..offset.len() - 1]))
                    .and_then(|(base, offset)| pointers.get(&base.checked_add(offset)?))
                {
                    Some(value) => values.insert(reg, *value),
                    None => values.remove(reg),
                };
            }
            ("ldr", [reg, ..]) => {
                values.remove(reg);
            }
            _ => {}
        }
        // A copy of `x19` stops being the object when the code writes its register.
        match (row.operation.as_str(), args.first()) {
            ("bl" | "blr", _) => object_bases.retain(|base| !caller_saved(base)),
            (_, Some(destination)) if !matches!(*destination, "x19" | "w19") => {
                object_bases.remove(&format!("[{}]", destination.replacen('w', "x", 1)));
            }
            _ => {}
        }
    }
    x19_object_vtable.or(x0_object_vtable)
}

/// Whether `[xN]` names a register that a call can change.
fn caller_saved(base: &str) -> bool {
    base.trim_start_matches("[x")
        .trim_end_matches(']')
        .parse::<u8>()
        .is_ok_and(|index| index <= 18)
}

/// The supported scopes of the command that `factory` creates.
fn scopes(input: &DeclarationInput, factory: u64) -> ScopeOutcome {
    let command = match command_vtable(input, factory) {
        Ok(command) => command,
        Err(link) => return ScopeOutcome::Unresolved(Unresolved::new(link)),
    };
    let Some(rows) = input
        .pointers
        .get(&(command + input.slots.supported_scopes))
        .and_then(|address| input.functions.get(address))
        .and_then(|body| decode(body).ok())
    else {
        return ScopeOutcome::Unresolved(Unresolved::new("scope-getter"));
    };
    match constant_return(&rows) {
        Some(mask) => scope_mask(mask, input.scope_names.as_deref()),
        None => ScopeOutcome::Unresolved(Unresolved::new("scope-mask")),
    }
}

/// The vtable of the command object that the factory's create method builds.
fn command_vtable(input: &DeclarationInput, factory: u64) -> Result<u64, &'static str> {
    let create = input
        .pointers
        .get(&(factory + input.slots.create))
        .ok_or("factory-create")?;
    let rows = create_rows(input, *create, 0).ok_or("create-body")?;
    if !rows.iter().any(|row| {
        row.operation == "bl"
            && target(row).is_some_and(|target| input.operator_new.contains(&target))
    }) {
        return Err("create-object");
    }
    let end = rows
        .iter()
        .position(|row| row.operation == "ret")
        .unwrap_or(rows.len());
    let is_command = |address: &u64| {
        input
            .pointers
            .contains_key(&(address + input.slots.supported_scopes))
    };
    vtable_store(&rows[..end], &input.pointers)
        .filter(is_command)
        .or_else(|| {
            rows[..end]
                .iter()
                .filter(|row| row.operation == "bl")
                .filter_map(target)
                .filter_map(|address| create_rows(input, address, 0))
                .filter_map(|body| vtable_store(&body, &input.pointers))
                .find(is_command)
        })
        .ok_or("command-vtable")
}

/// Scope names of a declared scope mask. Zero and all bits mean every scope. A name is kept as
/// the engine spells it, even with a space (`pop job`); splitting it would invent a scope.
pub(crate) fn scope_mask(mask: u64, names: Option<&[String]>) -> ScopeOutcome {
    if mask == 0 || mask == u64::MAX {
        return ScopeOutcome::Any;
    }
    let Some(names) = names else {
        return ScopeOutcome::Unresolved(Unresolved::new("scope-table"));
    };
    let mut listed = Vec::new();
    for index in 0..64 {
        if mask & (1u64 << index) == 0 {
            continue;
        }
        let Some(name) = names.get(index).filter(|name| !name.is_empty()) else {
            return ScopeOutcome::Unresolved(Unresolved::new("scope-name"));
        };
        listed.push(ScopeType {
            bit: index,
            name: name.clone(),
        });
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
mod tests;
