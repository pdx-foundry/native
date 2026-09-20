use pdx_native::internals::legacy::{
    ArtifactReference, CaptureOrigin, Completion, Engine, ReplayError, ReplayRequest, ResultOrigin,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;

fn retain(root: &Path, path: &str, value: &[u8]) -> ArtifactReference {
    let destination = root.join(path);
    std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
    std::fs::write(destination, value).unwrap();
    ArtifactReference {
        path: path.into(),
        sha256: format!("{:x}", Sha256::digest(value)),
        bytes: value.len() as u64,
    }
}
fn json_artifact(root: &Path, path: &str, value: Value) -> ArtifactReference {
    retain(root, path, &serde_json::to_vec(&value).unwrap())
}
fn fixture(root: &Path) -> ReplayRequest {
    let input = retain(
        root,
        "profile/mod/native_registry/common/traditions/test.txt",
        b"synthetic = {}\n",
    );
    let content = json_artifact(
        root,
        "producer-content.json",
        json!({"common/traditions/test.txt":input.sha256}),
    );
    let manifest = json_artifact(
        root,
        "manifest.json",
        json!({"probeHashes":{},"fixtureHashes":{"mod/native_registry/common/traditions/test.txt":input.sha256},"producerContentManifestSha256":content.sha256}),
    );
    let request = json_artifact(
        root,
        "request.json",
        json!({"observations":["registry:traditions"],"fixtures":{},"deadlineSeconds":180}),
    );
    let owner = json_artifact(
        root,
        "owner.json",
        serde_json::to_value(vec![
            evidence::recorded::OwnerEvent::GameOwnedSuspended {
                pid: 10,
                identity: "synthetic".into(),
            },
            evidence::recorded::OwnerEvent::DisposalChecked {
                confirmed: true,
                reaped_pid: 10,
                game_exit: Some(-9),
                remaining_identity: None,
            },
        ])
        .unwrap(),
    );
    let rows = vec![
        json!({"kind":"hooks-requested"}),
        json!({"kind":"launch-stopped","error":"success","pid":10,"triple":"arm64-synthetic","frames":[{"function":"_dyld_start"}]}),
        json!({"kind":"hooks-active-before-resume","hooks":{"registry":{"enabled":true,"locations":1,"resolved":1,"hits":0}}}),
        json!({"kind":"resume","error":"success"}),
        json!({"kind":"registry-load-start","name":"traditions","directory":"common/traditions","owner":"0x1000"}),
        json!({"kind":"registry-snapshot","name":"traditions","directory":"common/traditions","owner":"0x1000","count":1}),
        json!({"kind":"registry-entry","name":"traditions","owner":"0x1000","index":0,"object":"0x2000","key":"synthetic"}),
        json!({"kind":"registry-end","name":"traditions","owner":"0x1000","count":1,"producerLastSequence":8}),
    ];
    let mut bytes = Vec::new();
    for (index, mut row) in rows.into_iter().enumerate() {
        row["run"] = json!("synthetic");
        row["thread"] = json!(7);
        row["seq"] = json!(index + 1);
        serde_json::to_writer(&mut bytes, &row).unwrap();
        bytes.push(b'\n');
    }
    let trace = retain(root, "trace.jsonl", &bytes);
    let descriptor = json_artifact(
        root,
        "descriptor.json",
        json!({"format":evidence::registry::FORMAT,"contract":evidence::registry::CONTRACT,"attempt":"synthetic","origin":"synthetic","manifest":manifest,"request":request,"trace":trace,"owner":owner,"supporting":[content,input]}),
    );
    ReplayRequest {
        artifact_root: root.into(),
        descriptor,
    }
}

#[test]
fn recorded_registry_is_game_free_synthetic_and_distinct_from_early_replay() {
    let root = tempfile::tempdir().unwrap();
    let request = fixture(root.path());
    let result = Engine.replay_registry(request.clone()).unwrap();
    assert_eq!(result.registered_items[0].key, "synthetic");
    assert_eq!(
        serde_json::to_value(&result).unwrap()["registeredItems"][0]["key"],
        "synthetic"
    );
    assert_eq!(result.capture_origin, CaptureOrigin::Synthetic);
    assert_eq!(result.origin, ResultOrigin::Replay);
    assert_eq!(result.completion, Completion::Complete);
    assert!(matches!(
        Engine.replay(request),
        Err(ReplayError::UnsupportedFormat { .. })
    ));
}
#[test]
fn missing_or_changed_artifacts_never_become_an_empty_registry() {
    for missing in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let request = fixture(root.path());
        let input = root
            .path()
            .join("profile/mod/native_registry/common/traditions/test.txt");
        if missing {
            std::fs::remove_file(input).unwrap();
        } else {
            std::fs::write(input, b"different bytes").unwrap();
        }
        assert!(Engine.replay_registry(request).is_err());
    }
}
#[test]
fn fully_rehashed_wrong_content_copy_still_fails_the_manifest_join() {
    let root = tempfile::tempdir().unwrap();
    let request = fixture(root.path());
    let path = root.path().join("descriptor.json");
    let mut descriptor: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut content: Value =
        serde_json::from_slice(&std::fs::read(root.path().join("producer-content.json")).unwrap())
            .unwrap();
    content["common/traditions/test.txt"] = json!("b".repeat(64));
    let reference = json_artifact(root.path(), "producer-content.json", content);
    descriptor["supporting"][0] = serde_json::to_value(&reference).unwrap();
    let mut manifest: Value =
        serde_json::from_slice(&std::fs::read(root.path().join("manifest.json")).unwrap()).unwrap();
    manifest["producerContentManifestSha256"] = json!(reference.sha256);
    descriptor["manifest"] =
        serde_json::to_value(json_artifact(root.path(), "manifest.json", manifest)).unwrap();
    let reference = json_artifact(root.path(), "descriptor.json", descriptor);
    let error = Engine
        .replay_registry(ReplayRequest {
            descriptor: reference,
            ..request
        })
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Private registry content differs")
    );
}

#[test]
fn session_replay_keeps_active_and_final_disposal_separate() {
    let root = tempfile::tempdir().unwrap();
    let original = fixture(root.path());
    let mut descriptor: Value =
        serde_json::from_slice(&std::fs::read(root.path().join("descriptor.json")).unwrap())
            .unwrap();
    descriptor["format"] = evidence::registry::SESSION_FORMAT.into();
    let mut rows: Vec<Value> = std::fs::read_to_string(root.path().join("trace.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let hook = rows[2]["hooks"]["registry"].take();
    rows[2]["hooks"] = json!({"registry:traditions":hook});
    rows.insert(5, json!({"kind":"registry-load-returned","name":"traditions","owner":"0x1000","run":"synthetic","thread":7}));
    let mut bytes = Vec::new();
    for (index, row) in rows.iter_mut().enumerate() {
        row["seq"] = json!(index + 1);
        if row["kind"] == "registry-end" {
            row["producerLastSequence"] = json!(index + 1);
        }
        serde_json::to_writer(&mut bytes, row).unwrap();
        bytes.push(b'\n');
    }
    descriptor["trace"] =
        serde_json::to_value(retain(root.path(), "session-trace.jsonl", &bytes)).unwrap();
    descriptor["owner"] = serde_json::to_value(json_artifact(
        root.path(),
        "active-owner.json",
        json!([{"kind":"game-owned-suspended","pid":10,"identity":"synthetic"}]),
    ))
    .unwrap();
    let active = ReplayRequest {
        artifact_root: root.path().into(),
        descriptor: json_artifact(root.path(), "active.json", descriptor.clone()),
    };
    let before = Engine.replay_registry(active.clone()).unwrap();
    assert_eq!(before.completion, Completion::Complete);
    assert_eq!(
        before.disposal,
        pdx_native::internals::legacy::Disposal::Unconfirmed
    );
    let old: Value =
        serde_json::from_slice(&std::fs::read(root.path().join("descriptor.json")).unwrap())
            .unwrap();
    descriptor["owner"] = old["owner"].clone();
    let final_request = ReplayRequest {
        artifact_root: root.path().into(),
        descriptor: json_artifact(root.path(), "final.json", descriptor),
    };
    assert_eq!(
        Engine
            .replay_registry(final_request.clone())
            .unwrap()
            .disposal,
        pdx_native::internals::legacy::Disposal::Confirmed
    );
    assert_eq!(
        serde_json::to_value(before).unwrap(),
        serde_json::to_value(Engine.replay_registry(active).unwrap()).unwrap()
    );
    assert_eq!(
        Engine.replay_registry(original).unwrap().completion,
        Completion::Complete
    );
    std::fs::write(root.path().join("session-trace.jsonl"), b"changed").unwrap();
    assert!(Engine.replay_registry(final_request).is_err());
}
