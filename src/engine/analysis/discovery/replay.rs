use super::*;
use crate::{EvidenceReference, ReplayError, engine::analysis::decode::AnalysisOrigin};
use evidence::store::{is_sha256, sha256};
use std::sync::Arc;
fn malformed(path: &str, reason: impl ToString) -> ReplayError {
    ReplayError::Malformed {
        path: path.into(),
        reason: reason.to_string(),
    }
}
fn json<T: serde::de::DeserializeOwned>(bytes: &[u8], path: &str) -> Result<T, ReplayError> {
    serde_json::from_slice(bytes).map_err(|e| malformed(path, e))
}

/// Derive static candidates from exact retained input bytes. Historical joins require replay validation.
pub fn derive(
    descriptor: DiscoveryDescriptor,
    bytes: &[u8],
    origin: AnalysisOrigin,
) -> Result<RegistryDiscoveryResult, ReplayError> {
    if descriptor.format != FORMAT {
        return Err(ReplayError::UnsupportedFormat {
            found: descriptor.format,
        });
    }
    if descriptor.provenance.method != METHOD
        || descriptor.provenance.decoder != crate::engine::analysis::decode::DECODER
    {
        return Err(ReplayError::UnsupportedContract {
            found: descriptor.provenance.method,
        });
    }
    if [
        &descriptor.provenance.executable,
        &descriptor.provenance.slice,
        &descriptor.provenance.composition,
    ]
    .iter()
    .any(|s| !is_sha256(s))
    {
        return Err(malformed(
            &descriptor.input.path,
            "invalid provenance identity",
        ));
    }
    if bytes.len() as u64 != descriptor.input.bytes {
        return Err(ReplayError::SizeMismatch {
            path: descriptor.input.path,
            expected: descriptor.input.bytes,
            found: bytes.len() as u64,
        });
    }
    if sha256(bytes) != descriptor.input.sha256 {
        return Err(ReplayError::HashMismatch {
            path: descriptor.input.path,
        });
    }
    let input: StaticInput = json(bytes, &descriptor.input.path)?;
    let records = candidates(&input.symbols);
    let (rows, unknown) = scheduler(&input).map_err(|e| malformed(&descriptor.input.path, e))?;
    let scope = Arc::new(());
    let candidates: Vec<_> = records
        .iter()
        .enumerate()
        .map(|(ordinal, r)| RegistryCandidate {
            subject: RegistrySubject {
                scope: scope.clone(),
                ordinal,
            },
            has_named_member_reader: r.has_named_member_reader,
            basis: DiscoveryBasis::TemplateSymbol,
            evidence: EvidenceReference {
                artifact: descriptor.input.clone(),
                record: input
                    .symbols
                    .iter()
                    .position(|s| s.name == r.loader && format!("{:#x}", s.address) == r.address)
                    .map(|i| i as u64 + 1),
            },
        })
        .collect();
    let mut result = RegistryDiscoveryResult {
        subject_count: candidates.len(), origin, descriptor, candidates,
        scheduling: Vec::new(), relationships: Vec::new(), gaps: Vec::new(), scope,
        input_bytes: bytes.to_vec(), limits: vec![
        "Named template LoadFile candidates and one bounded startup scheduling table; neither enumerates all registries.".into(),
        "Local weak function bindings are static candidates; current runtime interposition is not observed.".into(),
        "Historical ownership applies only to each retained target/content/window; replay establishes no current live ownership.".into(),
        "Custom, nested and late discovery, identifier grammar, full reader semantics and field schemas remain unresolved.".into(),
    ],
    };
    let names_by_address: std::collections::BTreeMap<_, Vec<_>> =
        input
            .symbols
            .iter()
            .fold(std::collections::BTreeMap::new(), |mut map, s| {
                map.entry(s.address)
                    .or_default()
                    .extend(database_names(&s.name));
                map
            });
    for row in &rows {
        let matches: Vec<_> = records
            .iter()
            .enumerate()
            .filter(|(_, candidate)| {
                row.values[1..].iter().flatten().any(|a| {
                    names_by_address
                        .get(a)
                        .is_some_and(|names| names.contains(&candidate.database.as_str()))
                })
            })
            .map(|(i, _)| result.candidates[i].subject.clone())
            .collect();
        let evidence = EvidenceReference {
            artifact: result.descriptor.input.clone(),
            record: Some(row.index as u64 + 1),
        };
        if row.status == "gap" {
            result.gaps.push(DiscoveryGap {
                kind: DiscoveryGapKind::Scheduler,
                subject: None,
                reason: format!(
                    "scheduling row {} has missing literal slots or name",
                    row.index
                ),
                evidence: evidence.clone(),
            });
        }
        if matches.is_empty() {
            result.gaps.push(DiscoveryGap {
                kind: DiscoveryGapKind::OutsideTemplate,
                subject: None,
                reason: format!(
                    "scheduling row {} is outside the template candidate method",
                    row.index
                ),
                evidence: evidence.clone(),
            });
        }
        result.scheduling.push(SchedulingWitness {
            index: row.index,
            candidates: matches,
            recovered: row.status == "recovered",
            basis: DiscoveryBasis::StaticScheduling,
            evidence,
        });
    }
    for (address, reason) in unknown {
        result.gaps.push(DiscoveryGap {
            kind: DiscoveryGapKind::UnknownInstruction,
            subject: None,
            reason,
            evidence: EvidenceReference {
                artifact: result.descriptor.input.clone(),
                record: Some((address - input.layout.start) / 4 + 1),
            },
        });
    }
    result.gaps.push(DiscoveryGap {
        kind: DiscoveryGapKind::UnresolvedHelper,
        subject: None,
        reason: "custom, nested, late and shared-reader paths are not exhausted by this method"
            .into(),
        evidence: EvidenceReference {
            artifact: result.descriptor.input.clone(),
            record: None,
        },
    });
    for candidate in &result.candidates {
        result.gaps.push(DiscoveryGap {
            kind: DiscoveryGapKind::UnobservedCandidate,
            subject: Some(candidate.subject.clone()),
            reason: "static discovery observes no live loader".into(),
            evidence: candidate.evidence.clone(),
        });
    }
    Ok(result)
}
fn database_names(name: &str) -> Vec<&str> {
    name.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|s| s.starts_with('C') && (s.ends_with("Database") || s.ends_with("Manager")))
        .collect()
}
