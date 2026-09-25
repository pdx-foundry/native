//! Template registry candidates from the executable's symbols.
//!
//! The method needs no config seeds, name list, content files, debugger, or game launch. It
//! enumerates exact template loader symbols. A candidate count never establishes that all
//! registries were found; custom, nested and late loaders are outside the template method.
mod records;

pub use records::*;

use std::collections::{BTreeMap, BTreeSet};

/// Enumerate exact template LoadFile candidates independently of named member readers.
pub fn candidates(symbols: &[Symbol]) -> Vec<CandidateRecord> {
    let names: BTreeSet<_> = symbols.iter().map(|s| s.name.as_str()).collect();
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
        let Some(args) = symbol
            .name
            .strip_prefix("TSingleObjectGameDatabase<")
            .and_then(|s| s.strip_suffix(">::LoadFile(char const*, bool)"))
        else {
            continue;
        };
        let args: Vec<_> = args.split(',').map(str::trim).collect();
        if args.len() != 3
            || !matches!(args[2], "true" | "false")
            || args[..2]
                .iter()
                .any(|s| s.is_empty() || s.contains(['<', '>']))
        {
            continue;
        }
        result.push(CandidateRecord {
            database: args[0].into(),
            owner_candidate: args[1].into(),
            loader: symbol.name.clone(),
            address: format!("{:#x}", symbol.address),
            initial_loader: {
                let init = format!("TSingleObjectGameDatabase<{}>::Init()", args.join(", "));
                entries.get(init.as_str()).and_then(|addresses| {
                    (addresses.len() == 1).then(|| format!("{:#x}", addresses[0]))
                })
            },
            has_named_member_reader: names
                .contains(format!("{}::ReadMember(CReader&, int)", args[1]).as_str()),
        });
    }
    result.sort_by(|a, b| a.database.cmp(&b.database).then(a.address.cmp(&b.address)));
    result
}
