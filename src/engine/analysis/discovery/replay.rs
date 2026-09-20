use super::ownership::HistoricalRun;
use super::*;
use crate::{
    ArtifactReference, EvidenceReference, ReplayError, engine::analysis::decode::AnalysisOrigin,
};
use evidence::store::{ArtifactStore, is_sha256, sha256};
use serde_json::Value;
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

/// Verify every recorded input and recompute discovery without an installation or process.
pub fn replay(
    store: &ArtifactStore,
    reference: &ArtifactReference,
) -> Result<RegistryDiscoveryResult, ReplayError> {
    if reference.bytes > 1024 * 1024 {
        return Err(malformed(
            &reference.path,
            "discovery descriptor exceeds 1 MiB",
        ));
    }
    let descriptor: DiscoveryDescriptor = json(&store.read(reference)?, &reference.path)?;
    validate_budget(&descriptor, reference)?;
    let bytes = store.read(&descriptor.input)?;
    let mut runs = Vec::new();
    for run in &descriptor.runs {
        validate_capture(store, &descriptor, run)?;
        let trace = store.read(&run.trace)?;
        let trace = std::str::from_utf8(&trace)
            .map_err(|e| malformed(&run.trace.path, e))?
            .lines()
            .map(|line| json(line.as_bytes(), &run.trace.path))
            .collect::<Result<Vec<Value>, _>>()?;
        let historical = HistoricalRun {
            reference: run.clone(),
            trace,
            table: json(&store.read(&run.table)?, &run.table.path)?,
            result: json(&store.read(&run.result)?, &run.result.path)?,
            manifest: json(&store.read(&run.manifest)?, &run.manifest.path)?,
        };
        validate_run_identity(&historical, descriptor.capture_origin)?;
        runs.push(historical);
    }
    let mut result = derive(descriptor, &bytes, AnalysisOrigin::Replay)?;
    let input: StaticInput = json(&bytes, &result.descriptor.input.path)?;
    let records = candidates(&input.symbols);
    let (rows, _) = scheduler(&input).map_err(|e| malformed(&result.descriptor.input.path, e))?;
    result
        .gaps
        .retain(|g| g.kind != DiscoveryGapKind::UnobservedCandidate);
    ownership::append(&mut result, &input, &records, &rows, &runs);
    Ok(result)
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
    ownership::append(&mut result, &input, &records, &rows, &[]);
    Ok(result)
}
fn database_names(name: &str) -> Vec<&str> {
    name.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|s| s.starts_with('C') && (s.ends_with("Database") || s.ends_with("Manager")))
        .collect()
}

// The only captured legacy adapter is the independently verified SDK-489 capsule. Its
// trace/table formats predate embedded run IDs, so this immutable manifest is their join.
const SDK489_CAPSULE: &str = "66b82504c596a0921a08a0ebc445f3f66668769f564ffe89d6279ff5c80bae6e";

fn validate_budget(
    descriptor: &DiscoveryDescriptor,
    reference: &ArtifactReference,
) -> Result<(), ReplayError> {
    if descriptor.runs.len() > 2 {
        return Err(malformed(
            &reference.path,
            "discovery replay permits at most two historical runs",
        ));
    }
    let mut references = vec![reference, &descriptor.input];
    let mut identities = std::collections::BTreeSet::new();
    let mut paths =
        std::collections::BTreeSet::from([reference.path.as_str(), descriptor.input.path.as_str()]);
    for run in &descriptor.runs {
        if run.identity.is_empty() || !identities.insert(run.identity.as_str()) {
            return Err(malformed(
                &reference.path,
                "duplicate or empty capture identity",
            ));
        }
        for artifact in [&run.trace, &run.table, &run.result, &run.manifest] {
            if !paths.insert(&artifact.path) {
                return Err(malformed(
                    &reference.path,
                    "duplicate historical artifact locator",
                ));
            }
            references.push(artifact);
        }
        references.push(&run.capsule);
    }
    let total = references
        .into_iter()
        .try_fold(0u64, |sum, r| sum.checked_add(r.bytes));
    if !total.is_some_and(|bytes| bytes <= 64 * 1024 * 1024) {
        return Err(malformed(
            &reference.path,
            "discovery replay exceeds the 64 MiB aggregate input limit",
        ));
    }
    Ok(())
}
fn validate_capture(
    store: &ArtifactStore,
    descriptor: &DiscoveryDescriptor,
    run: &DiscoveryRun,
) -> Result<(), ReplayError> {
    if descriptor.capture_origin == crate::CaptureOrigin::Captured
        && run.capsule.sha256 != SDK489_CAPSULE
    {
        return Err(malformed(
            &run.capsule.path,
            "captured ownership requires the accepted SDK-489 capture capsule",
        ));
    }
    let capsule: Value = json(&store.read(&run.capsule)?, &run.capsule.path)?;
    for (name, reference) in [
        ("trace.jsonl", &run.trace),
        ("startup-table.json", &run.table),
        ("result.json", &run.result),
        ("manifest.json", &run.manifest),
    ] {
        let key = format!("{}/{name}", run.identity);
        if capsule["files"][&key].as_str() != Some(&reference.sha256) {
            return Err(malformed(
                &run.capsule.path,
                "artifact does not belong to the declared capture",
            ));
        }
    }
    if descriptor.capture_origin == crate::CaptureOrigin::Synthetic
        && capsule["identity"].as_str() != Some(&run.identity)
    {
        return Err(malformed(
            &run.capsule.path,
            "synthetic capture identity mismatch",
        ));
    }
    Ok(())
}
fn validate_run_identity(
    run: &HistoricalRun,
    origin: crate::CaptureOrigin,
) -> Result<(), ReplayError> {
    let identity = &run.reference.identity;
    if origin == crate::CaptureOrigin::Captured {
        if !run.result["archive"]
            .as_str()
            .is_some_and(|path| path.ends_with(&format!("/{identity}")))
        {
            return Err(malformed(
                &run.reference.result.path,
                "historical result belongs to another capture",
            ));
        }
    } else if run.result["run"].as_str() != Some(identity)
        || run.manifest["run"].as_str() != Some(identity)
        || run
            .trace
            .iter()
            .chain(&run.table)
            .any(|row| row["run"].as_str() != Some(identity))
    {
        return Err(malformed(
            &run.reference.trace.path,
            "historical artifacts have conflicting capture identities",
        ));
    }
    Ok(())
}
