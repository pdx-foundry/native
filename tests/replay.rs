use pdx_native::{
    Activation, ArtifactReference, CaptureOrigin, Completion, Disposal, Engine, Gap,
    ObservationFact, ReplayError, ReplayRequest, ReplayResult, ResultOrigin,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/synthetic")
}

fn request(root: &Path, case: &str) -> ReplayRequest {
    let descriptor =
        serde_json::from_slice(&fs::read(root.join(format!("cases/{case}.ref.json"))).unwrap())
            .unwrap();
    ReplayRequest {
        artifact_root: root.into(),
        descriptor,
    }
}

#[test]
fn atlas_caller_replays_the_four_control_outcomes() {
    let cases = [
        ("normal", Activation::Demonstrated, Completion::Complete, 5),
        (
            "missing-hook",
            Activation::NotEstablished,
            Completion::Unavailable,
            3,
        ),
        (
            "incomplete-stream",
            Activation::Demonstrated,
            Completion::Incomplete,
            4,
        ),
        (
            "worker-loss",
            Activation::NotEstablished,
            Completion::WorkerLost,
            3,
        ),
    ];
    for (case, activation, completion, count) in cases {
        let result = Engine.replay(request(&fixtures(), case)).unwrap();
        assert_eq!(result.origin, ResultOrigin::Replay);
        assert_eq!(result.capture_origin, CaptureOrigin::Synthetic);
        assert_eq!(result.activation, activation, "{case}");
        assert_eq!(result.completion, completion, "{case}");
        assert_eq!(result.disposal, Disposal::Confirmed, "{case}");
        assert_eq!(result.observations.len(), count, "{case}");
        assert_eq!(result.attempt, format!("synthetic-{case}"));
        assert!(!result.limits.is_empty());
        assert!(
            result
                .evidence
                .iter()
                .any(|reference| reference.artifact.path == "common/manifest.json")
        );
        if case == "normal" {
            assert!(result.gaps.is_empty());
        }
        if case == "incomplete-stream" {
            assert!(result.gaps.contains(&Gap::Sequence {
                expected: 11,
                found: 12
            }));
        }
        if case == "worker-loss" {
            assert!(result.gaps.contains(&Gap::MissingTerminal));
        }
    }
}

#[test]
fn read_entries_retain_source_owner_and_exact_trace_witnesses() {
    let result = Engine.replay(request(&fixtures(), "normal")).unwrap();
    let owners: Vec<_> = result
        .observations
        .iter()
        .filter_map(|observation| {
            if let ObservationFact::CategoryReadEntry {
                file, owner, line, ..
            } = &observation.fact
            {
                assert_eq!(file, "common/tradition_categories/synthetic.txt");
                assert!([2, 3].contains(line));
                assert_eq!(observation.evidence.artifact.path, "cases/normal.jsonl");
                assert_eq!(observation.evidence.record, Some(line + 9));
                Some(owner)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(owners.len(), 2);
    assert_eq!(owners[0], owners[1]);
    assert!(
        !serde_json::to_string(owners[0])
            .unwrap()
            .contains("synthetic-owner")
    );
}

#[test]
fn relocating_artifacts_preserves_context_evidence_and_handles() {
    let original = Engine.replay(request(&fixtures(), "normal")).unwrap();
    let relocated = Fixture::new();
    let restored = Engine
        .replay(request(relocated.dir.path(), "normal"))
        .unwrap();
    assert_eq!(original.context, restored.context);
    assert_eq!(original.observations, restored.observations);
    assert_eq!(original.evidence, restored.evidence);
}

#[test]
fn missing_bundle_and_missing_artifact_are_explicit_access_errors() {
    let fixture = Fixture::new();
    let mut absent = request(fixture.dir.path(), "normal");
    absent.artifact_root = fixture.dir.path().join("absent-bundle");
    assert!(matches!(
        Engine.replay(absent),
        Err(ReplayError::EvidenceUnavailable { .. })
    ));
    fs::remove_file(fixture.dir.path().join("common/owner.json")).unwrap();
    assert!(
        matches!(fixture.replay(), Err(ReplayError::EvidenceUnavailable { path }) if path == "common/owner.json")
    );
}

#[test]
fn missing_source_content_and_fixture_bytes_cannot_be_empty_success() {
    let fixture = Fixture::new();
    fs::remove_file(fixture.dir.path().join("common/producer-content.json")).unwrap();
    assert!(matches!(
        fixture.replay(),
        Err(ReplayError::EvidenceUnavailable { .. })
    ));
    let mut fixture = Fixture::new();
    fixture.descriptor["supporting"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert!(
        matches!(fixture.replay(), Err(ReplayError::Malformed { reason, .. }) if reason.contains("provenance"))
    );
    let mut fixture = Fixture::new();
    fixture.json_artifact("manifest", |manifest| {
        manifest["probeHashes"]["missing.py"] = json!("1".repeat(64));
    });
    assert!(
        matches!(fixture.replay(), Err(ReplayError::Malformed { reason, .. }) if reason.contains("source/missing.py"))
    );
}

#[test]
fn corrupt_and_truncated_bytes_fail_exact_identity_checks() {
    let fixture = Fixture::new();
    let path = fixture.dir.path().join("cases/normal.jsonl");
    let mut bytes = fs::read(&path).unwrap();
    bytes[0] = b'!';
    fs::write(&path, &bytes).unwrap();
    assert!(matches!(
        fixture.replay(),
        Err(ReplayError::HashMismatch { .. })
    ));
    bytes.pop();
    fs::write(path, &bytes).unwrap();
    assert!(matches!(
        fixture.replay(),
        Err(ReplayError::SizeMismatch { .. })
    ));
}

#[test]
fn identity_valid_but_malformed_json_is_a_parse_error() {
    let mut fixture = Fixture::new();
    fixture.bytes_artifact("trace", b"{\"seq\":1");
    assert!(
        matches!(fixture.replay(), Err(ReplayError::Malformed { reason, .. }) if reason.contains("line 1"))
    );
}

#[test]
fn unsupported_format_and_contract_do_not_discard_new_meaning() {
    let mut fixture = Fixture::new();
    fixture.descriptor = json!({"format":"future-v2", "contract":"future-contract"});
    assert!(
        matches!(fixture.replay(), Err(ReplayError::UnsupportedFormat { found }) if found == "future-v2")
    );
    let mut fixture = Fixture::new();
    fixture.descriptor["contract"] = json!("future-contract");
    assert!(matches!(
        fixture.replay(),
        Err(ReplayError::UnsupportedContract { .. })
    ));
    let mut fixture = Fixture::new();
    fixture.trace(|trace| trace[0]["kind"] = json!("new-semantic-event"));
    assert!(matches!(
        fixture.replay(),
        Err(ReplayError::Malformed { .. })
    ));
    let mut fixture = Fixture::new();
    fixture.descriptor["allow_unqualified"] = json!(true);
    assert!(matches!(
        fixture.replay(),
        Err(ReplayError::Malformed { .. })
    ));
}

#[test]
fn foreign_attempts_and_duplicate_locators_are_rejected() {
    let mut fixture = Fixture::new();
    fixture.trace(|trace| trace[4]["run"] = json!("another-attempt"));
    assert!(matches!(
        fixture.replay(),
        Err(ReplayError::Malformed { .. })
    ));
    let mut fixture = Fixture::new();
    let duplicate = fixture.descriptor["owner"].clone();
    fixture.descriptor["supporting"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    assert!(
        matches!(fixture.replay(), Err(ReplayError::Malformed { reason, .. }) if reason.contains("duplicate"))
    );
}

#[test]
fn missing_terminal_preserves_partial_facts_and_confirmed_disposal() {
    let mut fixture = Fixture::new();
    fixture.trace(|trace| trace.truncate(13));
    let result = fixture.replay().unwrap();
    assert_eq!(result.completion, Completion::Incomplete);
    assert_eq!(result.disposal, Disposal::Confirmed);
    assert_eq!(result.observations.len(), 5);
    assert!(result.gaps.contains(&Gap::MissingTerminal));
}

#[test]
fn empty_readable_stream_is_incomplete_and_keeps_disposal_evidence() {
    let mut fixture = Fixture::new();
    fixture.bytes_artifact("trace", b"");
    let result = fixture.replay().unwrap();
    assert!(result.observations.is_empty());
    assert_eq!(result.completion, Completion::Incomplete);
    assert_eq!(result.disposal, Disposal::Confirmed);
    assert!(result.gaps.contains(&Gap::MissingTerminal));
}

#[test]
fn saved_summary_and_fault_label_cannot_override_raw_witnesses() {
    let mut fixture = Fixture::new();
    fixture.json_artifact("manifest", |manifest| {
        manifest["fault"] = json!("worker-loss")
    });
    let summary = fixture.seal(
        "saved-result.json",
        br#"{"status":"incomplete","orderingEstablished":false,"disposal":{"confirmed":false}}"#,
    );
    fixture.descriptor["supporting"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::to_value(summary).unwrap());
    let result = fixture.replay().unwrap();
    assert_eq!(result.activation, Activation::Demonstrated);
    assert_eq!(result.completion, Completion::Complete);
    assert_eq!(result.disposal, Disposal::Confirmed);
}

#[test]
fn rewritten_settings_report_missing_original_bytes_without_changing_trace_completion() {
    let mut fixture = Fixture::new();
    let settings = fixture.seal("profile/settings.txt", b"retained post-run settings");
    fixture.descriptor["supporting"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::to_value(&settings).unwrap());
    fixture.json_artifact("manifest", |manifest| {
        manifest["fixtureHashes"]["settings.txt"] = json!("a".repeat(64))
    });
    let result = fixture.replay().unwrap();
    assert_eq!(result.completion, Completion::Complete);
    assert!(result.gaps.contains(&Gap::OriginalProfileInputUnavailable {
        retained: settings,
        original_sha256: "a".repeat(64)
    }));
}

#[test]
fn process_absence_requires_an_explicit_witness() {
    let mut fixture = Fixture::new();
    fixture.json_artifact("owner", |journal| {
        journal[3]
            .as_object_mut()
            .unwrap()
            .remove("remainingIdentity");
    });
    assert!(matches!(
        fixture.replay(),
        Err(ReplayError::Malformed { .. })
    ));
    let mut fixture = Fixture::new();
    fixture.json_artifact("owner", |journal| journal[0]["identity"] = json!(""));
    let result = fixture.replay().unwrap();
    assert_eq!(result.disposal, Disposal::Unconfirmed);
    assert_eq!(result.activation, Activation::NotEstablished);
}

#[test]
fn ordering_and_required_hooks_are_checked_instead_of_trusting_labels() {
    for case in [
        "late-hook",
        "disabled-hook",
        "hit-before-resume",
        "wrong-entry",
        "wrong-resume",
        "wrong-child",
        "missing-initializer",
    ] {
        let mut fixture = Fixture::new();
        fixture.trace(|trace| match case {
            "late-hook" => {
                trace.swap(2, 3);
                renumber(trace);
            }
            "disabled-hook" => trace[2]["hooks"]["field"]["enabled"] = json!(false),
            "hit-before-resume" => trace[2]["hooks"]["field"]["hits"] = json!(1),
            "wrong-entry" => trace[1]["frames"][0]["function"] = json!("main"),
            "wrong-resume" => trace[3]["error"] = json!("failed"),
            "wrong-child" => trace[1]["pid"] = json!(999),
            "missing-initializer" => trace[4]["stack"] = json!([]),
            _ => unreachable!(),
        });
        let result = fixture.replay().unwrap();
        assert_eq!(result.activation, Activation::NotEstablished, "{case}");
        assert_ne!(result.completion, Completion::Complete, "{case}");
    }
}

#[test]
fn invalid_totals_ordinals_owner_and_terminal_relations_prevent_completion() {
    for case in [
        "count",
        "ordinal",
        "owner",
        "terminal-sequence",
        "duplicate-terminal",
        "late-read",
        "parse-count",
    ] {
        let mut fixture = Fixture::new();
        fixture.trace(|trace| match case {
            "count" => trace[13]["registrations"] = json!(99),
            "ordinal" => trace[6]["ordinal"] = json!(1),
            "owner" => trace[11]["owner"] = json!("another-owner"),
            "terminal-sequence" => trace[13]["producerLastSequence"] = json!(99),
            "duplicate-terminal" => {
                trace.push(trace[13].clone());
                renumber(trace);
            }
            "late-read" => {
                trace.push(trace[11].clone());
                renumber(trace);
            }
            "parse-count" => trace[12]["producerFieldCount"] = json!(99),
            _ => unreachable!(),
        });
        let result = fixture.replay().unwrap();
        assert_eq!(result.completion, Completion::Incomplete, "{case}");
        assert_eq!(result.disposal, Disposal::Confirmed, "{case}");
        if case == "owner" {
            assert!(result.gaps.contains(&Gap::OwnerJoin));
        }
    }
}

#[test]
fn disposal_requires_reaping_the_same_child_and_remains_separate() {
    for case in [
        "missing-check",
        "different-child",
        "remaining-process",
        "no-exit",
    ] {
        let mut fixture = Fixture::new();
        fixture.json_artifact("owner", |journal| match case {
            "missing-check" => {
                journal.as_array_mut().unwrap().pop();
            }
            "different-child" => journal[3]["reapedPid"] = json!(999),
            "remaining-process" => journal[3]["remainingIdentity"] = json!("still running"),
            "no-exit" => journal[3]["gameExit"] = Value::Null,
            _ => unreachable!(),
        });
        let result = fixture.replay().unwrap();
        assert_eq!(result.completion, Completion::Complete, "{case}");
        assert_eq!(result.disposal, Disposal::Unconfirmed, "{case}");
        assert!(result.gaps.contains(&Gap::DisposalUnconfirmed));
    }
}

#[test]
fn malicious_storage_paths_cannot_escape_or_execute() {
    for path in [
        "../outside",
        "/tmp/outside",
        "C:/outside",
        "a\\b",
        "./file",
        "a//b",
    ] {
        let mut fixture = Fixture::new();
        fixture.descriptor["trace"]["path"] = json!(path);
        assert!(
            matches!(fixture.replay(), Err(ReplayError::UnsafePath { .. })),
            "{path}"
        );
    }
}

#[cfg(unix)]
#[test]
fn stored_symlinks_are_rejected() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let path = fixture.dir.path().join("cases/normal.jsonl");
    fs::rename(&path, fixture.dir.path().join("outside.jsonl")).unwrap();
    symlink("../outside.jsonl", path).unwrap();
    assert!(matches!(
        fixture.replay(),
        Err(ReplayError::UnsafePath { .. })
    ));
}

#[cfg(unix)]
#[test]
fn public_replay_never_executes_retained_targets_or_source_tools() {
    use std::os::unix::fs::PermissionsExt;
    let mut fixture = Fixture::new();
    let marker = fixture.dir.path().join("LAUNCHED");
    let script = fixture.dir.path().join("game");
    fs::write(
        &script,
        format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    fixture.json_artifact("manifest", |manifest| {
        manifest["target"]["executable"] = json!(script)
    });
    let original = fs::read(&script).unwrap();
    let source = fixture.seal("source/dangerous.py", &original);
    fixture.json_artifact("manifest", |manifest| {
        manifest["probeHashes"]["dangerous.py"] = json!(source.sha256)
    });
    fixture.descriptor["supporting"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::to_value(source).unwrap());
    let descriptor = fixture.seal_descriptor();
    let before = snapshot(fixture.dir.path());
    let result = Engine
        .replay(ReplayRequest {
            artifact_root: fixture.dir.path().into(),
            descriptor,
        })
        .unwrap();
    assert_eq!(result.completion, Completion::Complete);
    assert!(!marker.exists());
    assert_eq!(before, snapshot(fixture.dir.path()));
}

fn renumber(trace: &mut [Value]) {
    for (index, record) in trace.iter_mut().enumerate() {
        record["seq"] = json!(index + 1);
    }
}

struct Fixture {
    dir: TempDir,
    descriptor: Value,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        copy_tree(&fixtures(), dir.path());
        let descriptor =
            serde_json::from_slice(&fs::read(dir.path().join("cases/normal.json")).unwrap())
                .unwrap();
        Self { dir, descriptor }
    }

    fn seal(&self, path: &str, bytes: &[u8]) -> ArtifactReference {
        let location = self.dir.path().join(path);
        fs::create_dir_all(location.parent().unwrap()).unwrap();
        fs::write(location, bytes).unwrap();
        ArtifactReference {
            path: path.into(),
            bytes: bytes.len() as u64,
            sha256: format!("{:x}", Sha256::digest(bytes)),
        }
    }

    fn bytes_artifact(&mut self, role: &str, bytes: &[u8]) {
        let path = self.descriptor[role]["path"].as_str().unwrap().to_owned();
        self.descriptor[role] = serde_json::to_value(self.seal(&path, bytes)).unwrap();
    }

    fn json_artifact(&mut self, role: &str, edit: impl FnOnce(&mut Value)) {
        let path = self.descriptor[role]["path"].as_str().unwrap();
        let mut value =
            serde_json::from_slice(&fs::read(self.dir.path().join(path)).unwrap()).unwrap();
        edit(&mut value);
        self.bytes_artifact(role, &serde_json::to_vec(&value).unwrap());
    }

    fn trace(&mut self, edit: impl FnOnce(&mut Vec<Value>)) {
        let path = self.descriptor["trace"]["path"].as_str().unwrap();
        let text = fs::read_to_string(self.dir.path().join(path)).unwrap();
        let mut trace = text
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        edit(&mut trace);
        let bytes = trace
            .iter()
            .map(|record| serde_json::to_string(record).unwrap() + "\n")
            .collect::<String>();
        self.bytes_artifact("trace", bytes.as_bytes());
    }

    fn seal_descriptor(&self) -> ArtifactReference {
        self.seal(
            "cases/normal.json",
            &serde_json::to_vec(&self.descriptor).unwrap(),
        )
    }

    fn replay(&self) -> Result<ReplayResult, ReplayError> {
        Engine.replay(ReplayRequest {
            artifact_root: self.dir.path().into(),
            descriptor: self.seal_descriptor(),
        })
    }
}

fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &destination.join(entry.file_name()));
        } else {
            fs::copy(entry.path(), destination.join(entry.file_name())).unwrap();
        }
    }
}

fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    for entry in fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            files.extend(snapshot(&entry.path()));
        } else {
            files.push((entry.path(), fs::read(entry.path()).unwrap()));
        }
    }
    files.sort();
    files
}
