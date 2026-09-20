use crate::AnalysisError;
use crate::engine::analysis::{
    directories::{self, Anchors, Constructor},
    discovery::{CandidateRecord, StaticInput},
};
use std::collections::BTreeMap;

/// Largest constructor body read. A larger extent is skipped, which leaves the registry unnamed.
const LIMIT: u64 = 64 * 1024;

/// Sorted, distinct function start addresses, used to bound each function body.
fn starts(discovery: &StaticInput) -> Vec<u64> {
    let mut starts: Vec<u64> = discovery.symbols.iter().map(|s| s.address).collect();
    starts.sort_unstable();
    starts.dedup();
    starts
}

/// Read one bounded function body. `None` when its extent is outside the limit.
fn body(bytes: &[u8], starts: &[u64], address: u64) -> Result<Option<Constructor>, AnalysisError> {
    let Some(&end) = starts.get(starts.partition_point(|&a| a <= address)) else {
        return Ok(None);
    };
    let length = end - address;
    if length == 0 || length > LIMIT || !length.is_multiple_of(4) {
        return Ok(None);
    }
    let mut code = Vec::new();
    for offset in (0..length).step_by(4096) {
        code.extend(super::code_range(
            bytes,
            address + offset,
            (length - offset).min(4096),
        )?);
    }
    Ok(Some(Constructor { address, code }))
}

/// Entry addresses of the base constructor and the literal `CString` constructor.
pub(in crate::binding) fn anchors(discovery: &StaticInput) -> Anchors {
    let addresses = |name: &str| {
        discovery
            .symbols
            .iter()
            .filter(|s| s.name == name)
            .map(|s| s.address)
            .collect()
    };
    Anchors {
        base_constructors: addresses(directories::BASE_CONSTRUCTOR),
        string_constructors: addresses(directories::STRING_CONSTRUCTOR),
    }
}

/// Read every static initializer that can build a global `CString`.
pub(in crate::binding) fn initializers(
    bytes: &[u8],
    discovery: &StaticInput,
) -> Result<Vec<Constructor>, AnalysisError> {
    let starts = starts(discovery);
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
        found.extend(body(bytes, &starts, address)?);
    }
    Ok(found)
}

/// Read every constructor body of each candidate's database class from the verified buffer.
pub(in crate::binding) fn read(
    bytes: &[u8],
    discovery: &StaticInput,
    candidates: &[CandidateRecord],
) -> Result<BTreeMap<String, Vec<Constructor>>, AnalysisError> {
    let starts = starts(discovery);
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
            bodies.extend(body(bytes, &starts, symbol.address)?);
        }
    }
    Ok(found)
}
