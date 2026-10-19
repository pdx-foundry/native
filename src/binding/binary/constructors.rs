use super::declarations::Text;
use crate::AnalysisError;
use crate::engine::analysis::{
    directories::{self, Anchors, Constructor},
    discovery::{CandidateRecord, StaticInput, Symbol},
};
use std::collections::BTreeMap;

/// Largest constructor body read. A larger extent is skipped, which leaves the registry unnamed.
const LIMIT: u64 = 64 * 1024;

/// Read one bounded function body. `None` when `address` is not a function start or its extent is
/// outside the limit.
fn body(text: &Text, address: u64) -> Option<Constructor> {
    let code = text.function_within(address, LIMIT)?;

    Some(Constructor {
        address,
        code: code.to_vec(),
    })
}

/// Entry addresses of the base constructor, the literal `CString` constructor, the file
/// enumeration and the allocation.
pub(in crate::binding) fn anchors(symbols: &[Symbol]) -> Anchors {
    let addresses = |name: &str| {
        symbols
            .iter()
            .filter(|s| s.name == name)
            .map(|s| s.address)
            .collect()
    };
    Anchors {
        base_constructors: addresses(directories::BASE_CONSTRUCTOR),
        string_constructors: addresses(directories::STRING_CONSTRUCTOR),
        file_enumerations: addresses(directories::FILE_ENUMERATION),
        allocations: addresses(directories::ALLOCATION),
    }
}

/// Read every static initializer that can build a global `CString`.
pub(in crate::binding) fn initializers(
    bytes: &[u8],
    discovery: &StaticInput,
) -> Result<Vec<Constructor>, AnalysisError> {
    let text = Text::read(bytes, &discovery.symbols)?;
    let mut addresses: Vec<u64> = discovery
        .symbols
        .iter()
        .filter(|s| s.name.starts_with(directories::INITIALIZER_PREFIX))
        .map(|s| s.address)
        .collect();
    addresses.sort_unstable();
    addresses.dedup();
    let mut found = Vec::new();
    for address in addresses {
        found.extend(body(&text, address));
    }
    Ok(found)
}

/// Read every constructor body of each candidate's database class from the verified buffer.
pub(in crate::binding) fn read(
    bytes: &[u8],
    discovery: &StaticInput,
    candidates: &[CandidateRecord],
) -> Result<BTreeMap<String, Vec<Constructor>>, AnalysisError> {
    let text = Text::read(bytes, &discovery.symbols)?;
    let wanted: BTreeMap<String, &str> = candidates
        .iter()
        .map(|c| (format!("{0}::{0}()", c.database), c.database.as_str()))
        .collect();
    let mut found: BTreeMap<String, Vec<Constructor>> = BTreeMap::new();
    for symbol in &discovery.symbols {
        let Some(database) = wanted.get(&symbol.name) else {
            continue;
        };
        let bodies = found.entry((*database).to_owned()).or_default();
        if !bodies.iter().any(|b| b.address == symbol.address) {
            bodies.extend(body(&text, symbol.address));
        }
    }
    Ok(found)
}
