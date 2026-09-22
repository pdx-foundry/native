//! Bounded registry discovery from the executable.
//!
//! The method needs no config seeds, name list, content files, debugger, or game launch. It
//! enumerates exact template loader symbols, then reconstructs a bounded startup scheduling table
//! from raw instructions, literal strings and Mach-O chained fixups (offset-format 64-bit chained
//! pointers and addend64 imports). Same-image weak bindings stay static candidates: they cannot
//! establish current runtime interposition.
//!
//! Unknown calls invalidate volatile register values. Unsupported instructions, missing pointers,
//! and clobbered or missing table owners become explicit gaps. Candidates and scheduling
//! witnesses are separate results. Counts from this method never establish that all registries
//! were found.
//!
//! Custom, nested and late loaders appear only as scheduling rows outside the template method;
//! SDK-551 starts from those rows.
mod records;
mod scheduler;

pub use records::*;
pub use scheduler::{candidates, scheduler};

use super::InputError;

/// Find the template candidates, recover the scheduling table, and join the two.
pub fn discover(input: &StaticInput) -> Result<RegistryDiscovery, InputError> {
    let candidates = candidates(&input.symbols);
    let (rows, unknown) = scheduler(input).map_err(InputError)?;
    let mut result = RegistryDiscovery {
        candidates,
        scheduling: Vec::new(),
        gaps: Vec::new(),
    };
    // A function slot names a candidate when a symbol at the slot's address holds the
    // candidate's database type.
    let mut names_by_address = std::collections::BTreeMap::<u64, Vec<&str>>::new();
    for symbol in &input.symbols {
        names_by_address
            .entry(symbol.address)
            .or_default()
            .extend(database_names(&symbol.name));
    }
    for row in &rows {
        let matches: Vec<usize> = result
            .candidates
            .iter()
            .enumerate()
            .filter(|(_, candidate)| {
                row.values[1..].iter().flatten().any(|address| {
                    names_by_address
                        .get(address)
                        .is_some_and(|names| names.contains(&candidate.database.as_str()))
                })
            })
            .map(|(position, _)| position)
            .collect();
        if row.status == "gap" {
            result.gaps.push(DiscoveryGap {
                kind: DiscoveryGapKind::Scheduler,
                candidate: None,
                reason: format!(
                    "scheduling row {} has missing literal slots or name",
                    row.index
                ),
            });
        }
        if matches.is_empty() {
            result.gaps.push(DiscoveryGap {
                kind: DiscoveryGapKind::OutsideTemplate,
                candidate: None,
                reason: format!(
                    "scheduling row {} is outside the template candidate method",
                    row.index
                ),
            });
        }
        result.scheduling.push(SchedulingWitness {
            index: row.index,
            candidates: matches,
            recovered: row.status == "recovered",
        });
    }
    for (address, reason) in unknown {
        result.gaps.push(DiscoveryGap {
            kind: DiscoveryGapKind::UnknownInstruction,
            candidate: None,
            reason: format!("{address:#x}: {reason}"),
        });
    }
    result.gaps.push(DiscoveryGap {
        kind: DiscoveryGapKind::UnresolvedHelper,
        candidate: None,
        reason: "custom, nested, late and shared-reader paths are not exhausted by this method"
            .into(),
    });
    for position in 0..result.candidates.len() {
        result.gaps.push(DiscoveryGap {
            kind: DiscoveryGapKind::UnobservedCandidate,
            candidate: Some(position),
            reason: "static discovery observes no live loader".into(),
        });
    }
    Ok(result)
}

fn database_names(name: &str) -> Vec<&str> {
    name.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|s| s.starts_with('C') && (s.ends_with("Database") || s.ends_with("Manager")))
        .collect()
}
