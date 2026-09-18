//! Shared live artifact writing. Evidence owns the recorded types and all replay conclusions.
use crate::{operation::ObservationSpec, supervisor::SupervisorError};
use evidence::{
    ArtifactReference, CaptureOrigin,
    recorded::{self, Descriptor, Manifest, OwnerEvent, RecordedRequest, TraceEvent, TraceRecord},
};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub(crate) const FIXTURE_FILE: &str = "common/tradition_categories/atlas_early_fixture.txt";
pub(crate) const FIXTURE_BODY: &str =
    "atlas_early_category = {\n tree_template = \"atlas_early_template\"\n traditions = { }\n}\n";

pub(crate) fn hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

pub(crate) fn write_new(path: &Path, bytes: &[u8]) -> Result<(), SupervisorError> {
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    #[cfg(unix)]
    File::open(path.parent().unwrap())?.sync_all()?;
    Ok(())
}
pub(crate) fn write_json(path: &Path, value: &impl Serialize) -> Result<(), SupervisorError> {
    write_new(path, &serde_json::to_vec_pretty(value)?)
}

/// Publish a complete control message without exposing partial bytes or replacing an old grant.
#[cfg_attr(
    not(all(target_os = "macos", target_arch = "aarch64")),
    allow(dead_code)
)]
pub(crate) fn publish_json(path: &Path, value: &impl Serialize) -> Result<(), SupervisorError> {
    let pending = path.with_extension("pending");
    write_json(&pending, value)?;
    fs::hard_link(&pending, path)?;
    fs::remove_file(&pending)?;
    #[cfg(unix)]
    File::open(path.parent().unwrap())?.sync_all()?;
    Ok(())
}

pub(crate) fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, SupervisorError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.is_symlink() || metadata.len() > limit as u64 {
        return Err(SupervisorError(format!(
            "Unsafe or oversized artifact: {}",
            path.display()
        )));
    }
    let mut bytes = Vec::new();
    File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(SupervisorError("Artifact exceeded read bound".into()));
    }
    Ok(bytes)
}

pub(crate) struct Capture {
    root: PathBuf,
    attempt: String,
    supporting: Vec<ArtifactReference>,
    manifest: ArtifactReference,
    request: ArtifactReference,
    owner: Vec<OwnerEvent>,
}

fn reference(root: &Path, path: &str) -> Result<ArtifactReference, SupervisorError> {
    let bytes = read_bounded(&root.join(path), 8 * 1024 * 1024)?;
    Ok(ArtifactReference {
        path: path.into(),
        sha256: hash(&bytes),
        bytes: bytes.len() as u64,
    })
}

impl Capture {
    #[cfg_attr(
        not(all(target_os = "macos", target_arch = "aarch64")),
        allow(dead_code)
    )]
    pub(crate) fn prepare(
        output: &Path,
        attempt: &str,
        spec: &ObservationSpec,
        identity: serde_json::Value,
        content: &BTreeMap<String, String>,
        artifacts: &BTreeMap<String, String>,
    ) -> Result<Self, SupervisorError> {
        let root = output.join("evidence");
        crate::binding::private_directory(&root)?;
        crate::binding::private_directory(&root.join("source"))?;
        crate::binding::private_directory(&root.join("profile"))?;
        let mut supporting = Vec::new();
        for (name, expected) in artifacts {
            let bytes = read_bounded(&output.join("source").join(name), 8 * 1024 * 1024)?;
            if hash(&bytes) != *expected {
                return Err(SupervisorError("Worker artifact changed".into()));
            }
            let path = format!("source/{name}");
            write_new(&root.join(&path), &bytes)?;
            supporting.push(reference(&root, &path)?);
        }
        let mut fixture_hashes = BTreeMap::new();
        snapshot_profile(
            &output.join("profile"),
            &root,
            Path::new(""),
            &mut fixture_hashes,
            &mut supporting,
        )?;
        write_json(&root.join("producer-content.json"), content)?;
        let producer = reference(&root, "producer-content.json")?;
        let manifest = Manifest {
            probe_hashes: artifacts.clone(),
            fixture_hashes,
            producer_content_manifest_sha256: producer.sha256.clone(),
        };
        let mut manifest = serde_json::to_value(manifest)?;
        manifest
            .as_object_mut()
            .unwrap()
            .insert("nativeIdentity".into(), identity);
        write_json(&root.join("manifest.json"), &manifest)?;
        write_json(
            &root.join("request.json"),
            &RecordedRequest {
                observations: vec![
                    "effect-registration".into(),
                    "tradition-category-field-reads".into(),
                ],
                fixtures: BTreeMap::from([(FIXTURE_FILE.into(), spec.fixture.clone())]),
                deadline_seconds: spec.deadline_seconds,
            },
        )?;
        supporting.push(producer);
        Ok(Self {
            manifest: reference(&root, "manifest.json")?,
            request: reference(&root, "request.json")?,
            root,
            attempt: attempt.into(),
            supporting,
            owner: Vec::new(),
        })
    }

    pub(crate) fn record(&mut self, event: OwnerEvent) -> Result<(), SupervisorError> {
        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(self.root.join("owner-events.jsonl"))?;
        serde_json::to_writer(&mut file, &event)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        self.owner.push(event);
        Ok(())
    }

    pub(crate) fn finish(mut self, output: &Path) -> Result<ArtifactReference, SupervisorError> {
        let raw = read_bounded(
            &output.join("raw-trace.jsonl"),
            crate::protocol::observation::MAX_TRACE,
        )?;
        let (trace, diagnostics) = normalize(&raw, &self.attempt);
        write_new(&self.root.join("raw-trace.jsonl"), &raw)?;
        write_json(&self.root.join("transport.json"), &diagnostics)?;
        self.supporting
            .push(reference(&self.root, "raw-trace.jsonl")?);
        self.supporting
            .push(reference(&self.root, "transport.json")?);
        for name in [
            "worker-request.json",
            "hello.json",
            "resume-granted.json",
            "tool.json",
            "worker-owned.json",
        ] {
            let path = output.join(name);
            if path.try_exists()? {
                write_new(
                    &self.root.join(name),
                    &read_bounded(&path, crate::protocol::observation::MAX_RECORD)?,
                )?;
                self.supporting.push(reference(&self.root, name)?);
            }
        }
        for name in [
            "worker.stdout",
            "worker.stderr",
            "game.stdout",
            "game.stderr",
        ] {
            let path = output.join(name);
            if path.try_exists()? {
                write_new(
                    &self.root.join(name),
                    &read_bounded(&path, crate::protocol::observation::MAX_TRACE)?,
                )?;
                self.supporting.push(reference(&self.root, name)?);
            }
        }
        if self.root.join("owner-events.jsonl").try_exists()? {
            self.supporting
                .push(reference(&self.root, "owner-events.jsonl")?);
        }
        let mut normalized = Vec::new();
        for record in trace {
            serde_json::to_writer(&mut normalized, &record)?;
            normalized.push(b'\n');
        }
        write_new(&self.root.join("trace.jsonl"), &normalized)?;
        write_json(&self.root.join("owner.json"), &self.owner)?;
        let descriptor = Descriptor {
            format: recorded::FORMAT.into(),
            contract: recorded::CONTRACT.into(),
            attempt: self.attempt,
            origin: CaptureOrigin::Captured,
            manifest: self.manifest,
            request: self.request,
            trace: reference(&self.root, "trace.jsonl")?,
            owner: reference(&self.root, "owner.json")?,
            supporting: self.supporting,
        };
        write_json(&self.root.join("descriptor.json"), &descriptor)?;
        let descriptor = reference(&self.root, "descriptor.json")?;
        let store = evidence::store::ArtifactStore::new(&self.root);
        let replay = evidence::replay::replay(&store, &descriptor)
            .map_err(|error| SupervisorError(error.to_string()))?;
        write_json(&self.root.join("replay.json"), &replay)?;
        write_json(&self.root.join("descriptor.ref.json"), &descriptor)?;
        Ok(descriptor)
    }
}

#[cfg_attr(
    not(all(target_os = "macos", target_arch = "aarch64")),
    allow(dead_code)
)]
fn snapshot_profile(
    source: &Path,
    root: &Path,
    relative: &Path,
    hashes: &mut BTreeMap<String, String>,
    supporting: &mut Vec<ArtifactReference>,
) -> Result<(), SupervisorError> {
    let mut entries = fs::read_dir(source)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = relative.join(entry.file_name());
        let portable = name
            .to_str()
            .ok_or_else(|| SupervisorError("Non-UTF8 profile path".into()))?
            .replace('\\', "/");
        let destination = root.join("profile").join(&name);
        if entry.file_type()?.is_dir() {
            crate::binding::private_directory(&destination)?;
            snapshot_profile(&entry.path(), root, &name, hashes, supporting)?;
        } else {
            let bytes = read_bounded(&entry.path(), 64 * 1024)?;
            write_new(&destination, &bytes)?;
            hashes.insert(portable.clone(), hash(&bytes));
            supporting.push(reference(root, &format!("profile/{portable}"))?);
        }
    }
    Ok(())
}

fn normalize(raw: &[u8], attempt: &str) -> (Vec<TraceRecord>, Vec<String>) {
    let mut records = Vec::new();
    let mut diagnostics = Vec::new();
    for (index, line) in raw.split_inclusive(|byte| *byte == b'\n').enumerate() {
        let parsed = serde_json::from_slice::<TraceRecord>(line);
        if line.len() > crate::protocol::observation::MAX_RECORD
            || !line.ends_with(b"\n")
            || !parsed
                .as_ref()
                .is_ok_and(|record| record.run == attempt && record.seq != 0)
        {
            diagnostics.push(format!(
                "Invalid or partial transport row {}; retained prefix only",
                index + 1
            ));
            break;
        }
        records.push(parsed.unwrap());
    }
    // Even corruption after a terminal must not turn a damaged fresh capture into success.
    if !diagnostics.is_empty() {
        records.retain(|record| !matches!(record.event, TraceEvent::StreamEnd { .. }));
    }
    (records, diagnostics)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn damaged_tail_cannot_preserve_completion() {
        let raw = b"{\"run\":\"a\",\"seq\":1,\"kind\":\"stream-end\",\"producerLastSequence\":1,\"producerFieldCount\":2,\"registrations\":3}\n{\"seq\":";
        let (records, diagnostics) = normalize(raw, "a");
        assert!(records.is_empty());
        assert_eq!(diagnostics.len(), 1);
        assert!(!normalize(raw, "foreign").1.is_empty());
    }
}

#[cfg(test)]
mod storage_tests {
    use super::*;
    #[test]
    fn immutable_writes_and_bounded_reads_fail_without_overwriting() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("artifact");
        write_new(&path, b"original").unwrap();
        assert!(write_new(&path, b"replacement").is_err());
        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert!(read_bounded(&path, 2).is_err());
        assert!(read_bounded(root.path(), 1024).is_err());
    }

    #[test]
    fn normalization_keeps_sequence_gaps_and_partial_facts() {
        let raw = b"{\"seq\":1,\"run\":\"a\",\"kind\":\"hooks-requested\"}\n{\"seq\":3,\"run\":\"a\",\"kind\":\"registration-observed\",\"ordinal\":1,\"engineToken\":12}\n";
        let (records, diagnostics) = normalize(raw, "a");
        assert!(diagnostics.is_empty());
        assert_eq!(
            records.iter().map(|record| record.seq).collect::<Vec<_>>(),
            [1, 3]
        );
        let (records, diagnostics) = normalize(&[raw.as_slice(), b"{"].concat(), "a");
        assert_eq!(records.len(), 2);
        assert_eq!(diagnostics.len(), 1);
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn capture_snapshots_original_profile_and_replays_without_live_inputs() {
        let root = tempfile::tempdir().unwrap();
        let fixture_path = root
            .path()
            .join("profile/mod/atlas_early")
            .join(FIXTURE_FILE);
        fs::create_dir_all(fixture_path.parent().unwrap()).unwrap();
        fs::write(&fixture_path, FIXTURE_BODY).unwrap();
        fs::write(root.path().join("profile/settings.txt"), "before").unwrap();
        let spec = ObservationSpec {
            fixture: FIXTURE_BODY.into(),
            deadline_seconds: 180,
            control: crate::operation::ObservationControl::Normal,
        };
        let mut capture = Capture::prepare(
            root.path(),
            "unit",
            &spec,
            serde_json::json!({"unitTest":true}),
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap();
        let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/synthetic");
        let trace = fs::read_to_string(fixture_root.join("cases/normal.jsonl"))
            .unwrap()
            .replace("synthetic-normal", "unit")
            .replace("common/tradition_categories/synthetic.txt", FIXTURE_FILE);
        fs::write(root.path().join("raw-trace.jsonl"), trace).unwrap();
        let owner: Vec<OwnerEvent> =
            serde_json::from_slice(&fs::read(fixture_root.join("common/owner.json")).unwrap())
                .unwrap();
        for event in owner {
            capture.record(event).unwrap();
        }
        fs::write(
            root.path().join("profile/settings.txt"),
            "game rewrote this",
        )
        .unwrap();
        let descriptor = capture.finish(root.path()).unwrap();
        fs::remove_dir_all(root.path().join("profile")).unwrap();
        let result = crate::Engine
            .replay(crate::ReplayRequest {
                artifact_root: root.path().join("evidence"),
                descriptor,
            })
            .unwrap();
        assert_eq!(result.completion, crate::Completion::Complete);
        assert_eq!(result.disposal, crate::Disposal::Confirmed);
        assert!(
            !result
                .gaps
                .iter()
                .any(|gap| matches!(gap, crate::Gap::OriginalProfileInputUnavailable { .. }))
        );
        assert_eq!(
            fs::read(root.path().join("evidence/profile/settings.txt")).unwrap(),
            b"before"
        );
    }
}

#[cfg(test)]
mod publication_tests {
    use super::*;
    #[test]
    fn grants_are_published_complete_and_never_replaced() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("grant.json");
        publish_json(&path, &serde_json::json!({"attempt":"first"})).unwrap();
        assert!(!path.with_extension("pending").exists());
        assert!(publish_json(&path, &serde_json::json!({"attempt":"second"})).is_err());
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&fs::read(path).unwrap()).unwrap()["attempt"],
            "first"
        );
    }
}
