//! Template registry candidates from the executable's symbols.
//!
//! The method needs no config seeds, name list, content files, debugger, or game launch. It
//! enumerates exact template loader symbols. A candidate count never establishes that all
//! registries were found; custom, nested and late loaders are outside the template method. The
//! [`schedule`] lists the databases that the engine schedules, so it counts the ones outside it.
pub mod loader_owner;
mod records;
pub mod schedule;
mod symbol_index;

pub use records::*;
pub use symbol_index::SymbolIndex;

use std::collections::BTreeMap;

/// Enumerate exact template LoadFile candidates independently of named member readers.
pub fn candidates(symbols: &[Symbol]) -> Vec<CandidateRecord> {
    let mut entries: BTreeMap<&str, Vec<u64>> = BTreeMap::new();
    for symbol in symbols {
        if symbol.name.ends_with("::Init()") {
            entries
                .entry(&symbol.name)
                .or_default()
                .push(symbol.address);
        }
    }
    let mut result = Vec::new();
    for symbol in symbols {
        let Some(template_arguments) = symbol
            .name
            .strip_prefix("TSingleObjectGameDatabase<")
            .and_then(|s| s.strip_suffix(">::LoadFile(char const*, bool)"))
        else {
            continue;
        };
        let template_arguments: Vec<_> = template_arguments.split(',').map(str::trim).collect();
        let [database, owner_candidate, template_flag] = template_arguments[..] else {
            continue;
        };
        if !matches!(template_flag, "true" | "false")
            || [database, owner_candidate]
                .iter()
                .any(|name| name.is_empty() || name.contains(['<', '>']))
        {
            continue;
        }
        let loader_class =
            format!("TSingleObjectGameDatabase<{database}, {owner_candidate}, {template_flag}>");
        let initial_loader = entries
            .get(format!("{loader_class}::Init()").as_str())
            .and_then(|addresses| match addresses[..] {
                [address] => Some(address),
                _ => None,
            });
        result.push(CandidateRecord {
            database: database.into(),
            owner_candidate: owner_candidate.into(),
            loader_class,
            address: symbol.address,
            initial_loader,
        });
    }
    result.sort_by(|a, b| a.database.cmp(&b.database).then(a.address.cmp(&b.address)));
    result
}
