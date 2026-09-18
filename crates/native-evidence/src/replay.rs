//! Retained SDK-483 derivation. This package cannot import the live Native runtime.
//!
//! ```compile_fail
//! use pdx_native::Engine;
//! ```
//! ```compile_fail
//! use pdx_native::internal::owner_main;
//! ```
//! ```compile_fail
//! use pdx_native::internal::worker_main;
//! ```
//! ```compile_fail
//! use pdx_native::execution;
//! ```
//! ```compile_fail
//! use pdx_native::binding::platform;
//! ```

use serde::de::DeserializeOwned;
use std::collections::BTreeMap;

use crate::recorded::{
    CONTRACT, Descriptor, FORMAT, Manifest, OwnerEvent, RecordedRequest, TraceRecord,
};
use crate::store::{ArtifactStore, is_sha256, sha256};
use crate::{ArtifactReference, EvidenceReference, Gap, ReplayError, ReplayResult};

/// Verify a pinned descriptor and all required artifacts, then derive the bounded historical result.
/// No installation, process, debugger, callback, or executable plan is accepted.
pub fn replay(
    store: &ArtifactStore,
    reference: &ArtifactReference,
) -> Result<ReplayResult, ReplayError> {
    let bytes = store.read(reference)?;
    #[derive(serde::Deserialize)]
    struct Header {
        format: String,
        contract: String,
    }
    let header: Header = parse(reference, &bytes)?;
    if header.format != FORMAT {
        return Err(ReplayError::UnsupportedFormat {
            found: header.format,
        });
    }
    if header.contract != CONTRACT {
        return Err(ReplayError::UnsupportedContract {
            found: header.contract,
        });
    }
    let descriptor: Descriptor = parse(reference, &bytes)?;
    if descriptor.attempt.is_empty() || descriptor.supporting.len() > 128 {
        return Err(malformed(
            reference,
            "empty attempt or too many supporting artifacts",
        ));
    }
    let references: Vec<_> = [
        &descriptor.manifest,
        &descriptor.request,
        &descriptor.trace,
        &descriptor.owner,
    ]
    .into_iter()
    .chain(&descriptor.supporting)
    .collect();
    let total_bytes = references
        .iter()
        .try_fold(0_u64, |total, artifact| total.checked_add(artifact.bytes));
    if !total_bytes.is_some_and(|total| total <= 64 * 1024 * 1024) {
        return Err(malformed(
            reference,
            "attempt exceeds the 64 MiB total replay limit",
        ));
    }
    let mut artifacts = BTreeMap::new();
    for artifact in &references {
        if artifacts.contains_key(&artifact.path) {
            return Err(malformed(reference, "duplicate artifact locator"));
        }
        artifacts.insert(artifact.path.clone(), store.read(artifact)?);
    }
    let manifest: Manifest = parse(&descriptor.manifest, &artifacts[&descriptor.manifest.path])?;
    let request: RecordedRequest =
        parse(&descriptor.request, &artifacts[&descriptor.request.path])?;
    let fixture = validate_request(&descriptor.request, &request)?;
    let provenance_gaps =
        validate_provenance(&descriptor, &manifest, &request, &references, &artifacts)?;
    let owner: Vec<OwnerEvent> = parse(&descriptor.owner, &artifacts[&descriptor.owner.path])?;
    let trace = parse_trace(&descriptor.trace, &artifacts[&descriptor.trace.path])?;
    if trace
        .iter()
        .any(|record| record.run != descriptor.attempt || record.seq == 0)
    {
        return Err(malformed(
            &descriptor.trace,
            "foreign attempt identity or zero producer sequence",
        ));
    }
    let fixture_body = &request.fixtures[&fixture];
    let mut result = crate::stream::derive(
        &descriptor,
        reference,
        &fixture,
        fixture_body,
        &trace,
        &owner,
    );
    result.gaps.extend(provenance_gaps);
    result.evidence = std::iter::once(reference)
        .chain(references)
        .map(|artifact| EvidenceReference {
            artifact: artifact.clone(),
            record: None,
        })
        .collect();
    Ok(result)
}

fn parse_trace(
    reference: &ArtifactReference,
    bytes: &[u8],
) -> Result<Vec<TraceRecord>, ReplayError> {
    let text =
        std::str::from_utf8(bytes).map_err(|error| malformed(reference, &error.to_string()))?;
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(index, line)| {
            serde_json::from_str(line)
                .map_err(|error| malformed(reference, &format!("line {}: {error}", index + 1)))
        })
        .collect()
}

fn validate_request(
    reference: &ArtifactReference,
    request: &RecordedRequest,
) -> Result<String, ReplayError> {
    if request.observations != ["effect-registration", "tradition-category-field-reads"]
        || request.fixtures.len() != 1
        || request.deadline_seconds == 0
    {
        return Err(malformed(
            reference,
            "unsupported observation request or fixture bounds",
        ));
    }
    let file = request.fixtures.keys().next().expect("one fixture checked");
    if !file.starts_with("common/tradition_categories/") || !file.ends_with(".txt") {
        return Err(malformed(reference, "expected a category fixture source"));
    }
    Ok(file.clone())
}

fn validate_provenance(
    descriptor: &Descriptor,
    manifest: &Manifest,
    request: &RecordedRequest,
    references: &[&ArtifactReference],
    artifacts: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<Gap>, ReplayError> {
    let mut gaps = Vec::new();
    let mut required = vec![(
        "producer-content.json".to_string(),
        manifest.producer_content_manifest_sha256.clone(),
    )];
    required.extend(
        manifest
            .probe_hashes
            .iter()
            .filter(|(name, _)| !name.ends_with(".dylib"))
            .map(|(name, hash)| (format!("source/{name}"), hash.clone())),
    );
    for (name, hash) in &manifest.fixture_hashes {
        if !is_sha256(hash) {
            return Err(malformed(
                &descriptor.manifest,
                "expected lowercase fixture SHA-256",
            ));
        }
        let suffix = format!("profile/{name}");
        let retained = references
            .iter()
            .find(|artifact| {
                artifact.path == suffix || artifact.path.ends_with(&format!("/{suffix}"))
            })
            .ok_or_else(|| {
                malformed(
                    &descriptor.manifest,
                    &format!("unverified provenance artifact: {suffix}"),
                )
            })?;
        if name == "settings.txt" && retained.sha256 != *hash {
            gaps.push(Gap::OriginalProfileInputUnavailable {
                retained: (*retained).clone(),
                original_sha256: hash.clone(),
            });
        } else {
            required.push((suffix, hash.clone()));
        }
    }
    // SDK-483 did not preserve a per-run copy of dylibs; their pinned bytes survive in the parent
    // prototype directory. Match by basename and digest, never by the original machine path.
    required.extend(
        manifest
            .probe_hashes
            .iter()
            .filter(|(name, _)| name.ends_with(".dylib"))
            .map(|(name, hash)| (name.clone(), hash.clone())),
    );
    for (suffix, hash) in required {
        if !references.iter().any(|artifact| {
            (artifact.path == suffix || artifact.path.ends_with(&format!("/{suffix}")))
                && artifact.sha256 == hash
        }) {
            return Err(malformed(
                &descriptor.manifest,
                &format!("unverified provenance artifact: {suffix}"),
            ));
        }
    }
    for (file, body) in &request.fixtures {
        let profile_path = format!("mod/atlas_early/{file}");
        if manifest.fixture_hashes.get(&profile_path) != Some(&sha256(body.as_bytes())) {
            return Err(malformed(
                &descriptor.manifest,
                "requested fixture is not pinned by the producer manifest",
            ));
        }
        let suffix = format!("profile/mod/atlas_early/{file}");
        let fixture = references
            .iter()
            .find(|artifact| {
                artifact.path == suffix || artifact.path.ends_with(&format!("/{suffix}"))
            })
            .ok_or_else(|| malformed(&descriptor.request, "requested fixture is not retained"))?;
        if sha256(body.as_bytes()) != fixture.sha256 || artifacts[&fixture.path] != body.as_bytes()
        {
            return Err(malformed(
                &descriptor.request,
                "requested fixture differs from retained profile",
            ));
        }
    }
    Ok(gaps)
}

fn parse<T: DeserializeOwned>(
    reference: &ArtifactReference,
    bytes: &[u8],
) -> Result<T, ReplayError> {
    serde_json::from_slice(bytes).map_err(|error| malformed(reference, &error.to_string()))
}

fn malformed(reference: &ArtifactReference, reason: &str) -> ReplayError {
    ReplayError::Malformed {
        path: reference.path.clone(),
        reason: reason.into(),
    }
}
