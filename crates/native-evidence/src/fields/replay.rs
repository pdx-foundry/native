use super::*;
use crate::{
    ArtifactReference, EvidenceReference, ReplayError,
    analysis::AnalysisOrigin,
    store::{ArtifactStore, is_sha256, sha256},
};
fn malformed(path: &str, reason: impl ToString) -> ReplayError {
    ReplayError::Malformed {
        path: path.into(),
        reason: reason.to_string(),
    }
}
fn validate(descriptor: &FieldDescriptor) -> Result<(), ReplayError> {
    if descriptor.format != FORMAT {
        return Err(ReplayError::UnsupportedFormat {
            found: descriptor.format.clone(),
        });
    }
    if descriptor.provenance.method != METHOD
        || descriptor.provenance.decoder != crate::analysis::DECODER
    {
        return Err(ReplayError::UnsupportedContract {
            found: descriptor.provenance.method.clone(),
        });
    }
    if [
        &descriptor.provenance.executable,
        &descriptor.provenance.slice,
        &descriptor.provenance.composition,
        &descriptor.provenance.implementation,
    ]
    .iter()
    .any(|s| !is_sha256(s))
    {
        return Err(malformed(
            &descriptor.input.path,
            "invalid field-analysis provenance",
        ));
    }
    if descriptor.input.bytes > 64 * 1024 * 1024 {
        return Err(malformed(
            &descriptor.input.path,
            "field input exceeds 64 MiB",
        ));
    }
    Ok(())
}
/// Verify retained artifacts and rerun root dispatch; no installation or launch is required.
pub fn replay(
    store: &ArtifactStore,
    reference: &ArtifactReference,
) -> Result<RegistryFieldResult, ReplayError> {
    if reference.bytes > 1024 * 1024 {
        return Err(malformed(&reference.path, "field descriptor exceeds 1 MiB"));
    }
    let descriptor: FieldDescriptor = serde_json::from_slice(&store.read(reference)?)
        .map_err(|e| malformed(&reference.path, e))?;
    validate(&descriptor)?;
    let bytes = store.read(&descriptor.input)?;
    derive(descriptor, &bytes, AnalysisOrigin::Replay)
}
/// Recompute root fields from hashed executable inputs. Completeness is derived, never supplied.
pub fn derive(
    descriptor: FieldDescriptor,
    bytes: &[u8],
    origin: AnalysisOrigin,
) -> Result<RegistryFieldResult, ReplayError> {
    validate(&descriptor)?;
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
    let input: FieldInput =
        serde_json::from_slice(bytes).map_err(|e| malformed(&descriptor.input.path, e))?;
    if input.functions.len() > 128
        || input.functions.iter().map(|f| f.code.len()).sum::<usize>() > 4 * 1024 * 1024
    {
        return Err(malformed(
            &descriptor.input.path,
            "function input budget exceeded",
        ));
    }
    let evidence = EvidenceReference {
        artifact: descriptor.input.clone(),
        record: None,
    };
    let mut gaps: Vec<_> = input
        .gaps
        .iter()
        .map(|reason| FieldGap {
            kind: "input-boundary".into(),
            reason: reason.clone(),
            path: None,
            evidence: evidence.clone(),
        })
        .collect();
    if !crate::discovery::candidates(&input.symbols).contains(&input.selection) {
        return Err(malformed(
            &descriptor.input.path,
            "selected loader is not an executable-derived candidate",
        ));
    }
    let (tokens, token_gaps) = tokens::recover(&input);
    gaps.extend(token_gaps.into_iter().map(|reason| FieldGap {
        kind: "token-table".into(),
        reason,
        path: None,
        evidence: evidence.clone(),
    }));
    let paths = dispatch::explore(&input, &evidence);
    let (fields, path_gaps) = super::inventory::fields_and_gaps(&paths, &tokens, &evidence);
    gaps.extend(path_gaps);
    let partition_accounted = super::inventory::partition_accounted(&paths);
    if !partition_accounted {
        gaps.push(FieldGap {
            kind: "token-partition".into(),
            reason: "token intervals are missing or overlap".into(),
            path: None,
            evidence: evidence.clone(),
        });
    }
    let blocking_readers = super::inventory::blocking_readers(&input.selection.owner_candidate);
    gaps.push(FieldGap {
        kind: "reader-contract".into(),
        reason: "Routing does not qualify shared-reader semantics or complete registry membership."
            .into(),
        path: None,
        evidence,
    });
    Ok(RegistryFieldResult {
        origin,
        descriptor,
        fields,
        paths,
        gaps,
        partition_accounted,
        complete_registry: false,
        blocking_readers,
        limits: vec![
            "Template loader/owner symbol relationship is static evidence, not observed live ownership.".into(),
            "Root token dispatch stops at a delegate or obstruction; nested grammars, post-read behavior and dynamic names remain unqualified.".into(),
            "Names come only from literal engine token constructors. No config or content comparison is discovery authority.".into(),
        ],
        input_bytes: bytes.to_vec(),
    })
}
