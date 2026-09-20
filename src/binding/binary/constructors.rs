use crate::AnalysisError;
use crate::engine::analysis::{
    directories::Constructor,
    discovery::{CandidateRecord, StaticInput},
};
use std::collections::BTreeMap;

/// Largest constructor body read. A larger extent is skipped, which leaves the registry unnamed.
const LIMIT: u64 = 64 * 1024;

/// Read every constructor body of each candidate's database class from the verified buffer.
pub(in crate::binding) fn read(
    bytes: &[u8],
    discovery: &StaticInput,
    candidates: &[CandidateRecord],
) -> Result<BTreeMap<String, Vec<Constructor>>, AnalysisError> {
    let mut starts: Vec<u64> = discovery.symbols.iter().map(|s| s.address).collect();
    starts.sort_unstable();
    starts.dedup();
    let wanted: BTreeMap<String, &str> = candidates
        .iter()
        .map(|c| (format!("{0}::{0}()", c.database), c.database.as_str()))
        .collect();
    let mut found: BTreeMap<String, Vec<Constructor>> = BTreeMap::new();
    for symbol in &discovery.symbols {
        let Some(database) = wanted.get(&symbol.name) else {
            continue;
        };
        let next = starts.partition_point(|&a| a <= symbol.address);
        let Some(&end) = starts.get(next) else {
            continue;
        };
        let length = end - symbol.address;
        if length == 0 || length > LIMIT || !length.is_multiple_of(4) {
            continue;
        }
        let mut code = Vec::new();
        for offset in (0..length).step_by(4096) {
            code.extend(super::code_range(
                bytes,
                symbol.address + offset,
                (length - offset).min(4096),
            )?);
        }
        let bodies = found.entry((*database).to_owned()).or_default();
        if !bodies.iter().any(|b| b.address == symbol.address) {
            bodies.push(Constructor {
                address: symbol.address,
                code,
            });
        }
    }
    Ok(found)
}
