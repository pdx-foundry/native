use pdx_native::internals::discovery::{
    SchedulerLayout, StaticInput, Symbol, candidates, scheduler,
};
use pdx_native::{
    AnalysisOrigin, CaptureOrigin, DiscoveryDescriptor, DiscoveryGapKind, Engine, ReplayRequest,
};
use std::{collections::BTreeMap, fs};
#[path = "analysis_support/mod.rs"]
mod support;

fn input() -> StaticInput {
    StaticInput {
        symbols: vec![Symbol {name:"TSingleObjectGameDatabase<CExampleDatabase, CExample, false>::LoadFile(char const*, bool)".into(),address:0x3000}],
        code: [0x910003f3u32, 0xb0000008,0xf9003268,0xd0000009,0xf9003669,0xa9077e7f,0xa9087e7f].into_iter().flat_map(u32::to_le_bytes).collect(),
        layout:SchedulerLayout{start:0x1000,end:0x101c,offset:96,stride:48,count:1},
        pointers:BTreeMap::new(),strings:BTreeMap::from([(0x2000,"example".into())]),vtables:BTreeMap::new(),
    }
}
fn descriptor(bytes: &[u8]) -> DiscoveryDescriptor {
    let mut provenance = support::descriptor().provenance;
    provenance.method = pdx_native::internals::discovery::METHOD.into();
    DiscoveryDescriptor {
        format: pdx_native::internals::discovery::FORMAT.into(),
        capture_origin: CaptureOrigin::Synthetic,
        provenance,
        input: support::reference("input.json", bytes),
        runs: vec![],
    }
}
#[test]
fn static_discovery_and_replay_are_bounded_and_context_owned() {
    let input = input();
    let bytes = serde_json::to_vec(&input).unwrap();
    let descriptor = descriptor(&bytes);
    let result = pdx_native::internals::discovery::derive(
        descriptor.clone(),
        &bytes,
        AnalysisOrigin::Executable,
    )
    .unwrap();
    assert_eq!(result.candidates.len(), 1);
    assert!(!result.candidates[0].has_named_member_reader);
    assert!(result.scheduling[0].recovered);
    assert_eq!(result.scheduling[0].candidates.len(), 1);
    assert!(result.relationships.is_empty());
    assert!(
        result
            .gaps
            .iter()
            .any(|g| g.kind == DiscoveryGapKind::UnobservedCandidate)
    );
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("input.json"), &bytes).unwrap();
    let recorded = serde_json::to_vec(&descriptor).unwrap();
    fs::write(root.path().join("descriptor.json"), &recorded).unwrap();
    let request = ReplayRequest {
        artifact_root: root.path().into(),
        descriptor: support::reference("descriptor.json", &recorded),
    };
    let replay = Engine.replay_registry_discovery(request.clone()).unwrap();
    assert_eq!(replay.origin, AnalysisOrigin::Replay);
    assert_eq!(replay.descriptor.capture_origin, CaptureOrigin::Synthetic);
    assert!(result.subject(&result.candidates[0].subject).is_ok());
    assert!(replay.subject(&result.candidates[0].subject).is_err());
    let again = Engine.replay_registry_discovery(request.clone()).unwrap();
    assert!(again.subject(&replay.candidates[0].subject).is_err());
    let handle = serde_json::to_string(&replay.candidates[0].subject).unwrap();
    assert!(!handle.contains("Example") && !handle.contains("0x"));
    let mut expected = serde_json::to_value(result).unwrap();
    expected["origin"] = serde_json::json!("Replay");
    assert_eq!(serde_json::to_value(replay).unwrap(), expected);
    fs::write(root.path().join("input.json"), "damaged").unwrap();
    assert!(Engine.replay_registry_discovery(request.clone()).is_err());
    fs::remove_file(root.path().join("input.json")).unwrap();
    assert!(Engine.replay_registry_discovery(request).is_err());
}
#[test]
fn omissions_and_clobbers_preserve_obligations() {
    let mut input = input();
    assert_eq!(scheduler(&input).unwrap().0[0].status, "recovered");
    input.strings.clear();
    assert_eq!(scheduler(&input).unwrap().0[0].status, "gap");
    input = input_fixture_with_clobber(0x52800008); // mov w8,#0 before the name store
    let (rows, _) = scheduler(&input).unwrap();
    assert!(rows[0].values[0].is_none());
    input = input_fixture_with_clobber(0x94000000); // unknown call clobbers x8
    let (rows, gaps) = scheduler(&input).unwrap();
    assert!(rows[0].values[0].is_none() && !gaps.is_empty());
    input = input_fixture_with_clobber(0x91002273); // add x19,x19,#8 changes table owner
    let bytes = serde_json::to_vec(&input).unwrap();
    let result = pdx_native::internals::discovery::derive(
        descriptor(&bytes),
        &bytes,
        AnalysisOrigin::Replay,
    )
    .unwrap();
    assert!(
        result
            .gaps
            .iter()
            .any(|g| g.kind == DiscoveryGapKind::Scheduler)
    );
    let mut input = self::input();
    input.symbols.clear();
    let bytes = serde_json::to_vec(&input).unwrap();
    let result = pdx_native::internals::discovery::derive(
        descriptor(&bytes),
        &bytes,
        AnalysisOrigin::Replay,
    )
    .unwrap();
    assert!(result.candidates.is_empty());
    assert_eq!(result.scheduling.len(), 1);
    assert!(
        result
            .gaps
            .iter()
            .any(|g| g.kind == DiscoveryGapKind::OutsideTemplate)
    );
}
fn input_fixture_with_clobber(code: u32) -> StaticInput {
    let mut input = input();
    input.code.splice(8..8, code.to_le_bytes());
    input.layout.end += 4;
    input
}
#[test]
fn template_matching_does_not_require_a_named_reader_or_accept_near_matches() {
    let mut symbols = input().symbols;
    symbols.push(Symbol {
        name: "TSingleObjectGameDatabase<CWrong, CWrong, false>::LoadFile(char*, bool)".into(),
        address: 0x4000,
    });
    assert_eq!(candidates(&symbols).len(), 1);
    symbols.push(Symbol {
        name: "CExample::ReadMember(CReader&, int)".into(),
        address: 0x5000,
    });
    assert!(candidates(&symbols)[0].has_named_member_reader);
}

fn historical_fixture(
    root: &std::path::Path,
    events: &[serde_json::Value],
    input: &StaticInput,
) -> DiscoveryDescriptor {
    use serde_json::json;
    let bytes = serde_json::to_vec(input).unwrap();
    let mut descriptor = descriptor(&bytes);
    fs::write(root.join("input.json"), &bytes).unwrap();
    let record = |name: &str, bytes: Vec<u8>| {
        fs::write(root.join(name), &bytes).unwrap();
        support::reference(name, &bytes)
    };
    let mut events = events.to_vec();
    for event in &mut events {
        event["run"] = json!("synthetic-run");
    }
    let trace = events
        .iter()
        .map(|r| serde_json::to_string(r).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    let trace = record("trace.jsonl", trace.into_bytes());
    let table=record("table.json",serde_json::to_vec(&json!([{"run":"synthetic-run","index":0,"name":"example","slots":[{"address":"0x3000"},null,null,null,null]}])).unwrap());
    let result=record("run.json",serde_json::to_vec(&json!({"run":"synthetic-run","completed":true,"sequenceContiguous":true,"binaryUnchanged":true,"contentUnchanged":true})).unwrap());
    let manifest=record("manifest.json",serde_json::to_vec(&json!({"run":"synthetic-run","target":{"executableSha256":descriptor.provenance.executable,"sliceSha256":descriptor.provenance.slice}})).unwrap());
    let capsule=record("capsule.json",serde_json::to_vec(&json!({"identity":"synthetic-run","files":{
        "synthetic-run/trace.jsonl":trace.sha256,"synthetic-run/startup-table.json":table.sha256,
        "synthetic-run/result.json":result.sha256,"synthetic-run/manifest.json":manifest.sha256
    }})).unwrap());
    descriptor.runs = vec![pdx_native::DiscoveryRun {
        identity: "synthetic-run".into(),
        capsule,
        trace,
        table,
        result,
        manifest,
    }];
    descriptor
}
fn historical_events() -> Vec<serde_json::Value> {
    use serde_json::json;
    let mut events = vec![
        json!({"kind":"attached","atDyldEntry":true,"error":"success","hooks":{"loader":{"resolved":1,"hits":0}}}),
        json!({"kind":"vfs-end","path":"common/example","count":1,"caller":{"symbol":"CExampleDatabase::Init()"}}),
        json!({"kind":"load-file","database":"CExampleDatabase","receiver":"0x7000","vtable":{"symbol":"vtable for CExampleDatabase"},"directory":"common/example","file":"common/example/one.txt"}),
        json!({"kind":"constructor-key","database":"CExampleDatabase","owner":"0x8000","key":"one","file":"common/example/one.txt"}),
        json!({"kind":"owner-reader","database":"CExampleDatabase","top":"0x8000","receiver":"0x8010","key":"one","file":"common/example/one.txt","vtable":{"address":"0x6000","symbol":"vtable for CExample"},"memberFunction":{"address":"0x4000","symbol":"CExample::ReadMember(CReader&, int)"}}),
        json!({"kind":"stream-end"}),
    ];
    for (i, event) in events.iter_mut().enumerate() {
        event["seq"] = json!(i + 1);
    }
    events
}
#[test]
fn historical_owner_joins_require_each_independent_witness() {
    let root = tempfile::tempdir().unwrap();
    let mut input = input();
    input.vtables.insert(
        0x6000,
        pdx_native::internals::discovery::VtableWitness {
            owner: "CExample".into(),
            offset_to_top: -16,
            member: 0x4000,
        },
    );
    for case in [
        "normal",
        "removed-owner",
        "wrong-owner",
        "wrong-receiver",
        "wrong-directory",
        "missing-key",
        "wrong-vtable",
        "wrong-member",
        "missing-enumeration",
        "missing-terminal",
        "bad-sequence",
    ] {
        let mut events = historical_events();
        match case {
            "removed-owner" => {
                events[3]["owner"] = serde_json::Value::Null;
            }
            "wrong-owner" => {
                events[4]["top"] = serde_json::json!("0x9000");
            }
            "wrong-receiver" => {
                events[4]["receiver"] = serde_json::json!("0x8020");
            }
            "wrong-directory" => {
                events[2]["directory"] = serde_json::json!("common/wrong");
            }
            "missing-key" => {
                events[3]["key"] = serde_json::Value::Null;
            }
            "wrong-vtable" => {
                events[4]["vtable"]["address"] = serde_json::json!("0x6010");
            }
            "wrong-member" => {
                events[4]["memberFunction"]["address"] = serde_json::json!("0x4008");
            }
            "missing-enumeration" => {
                events[1]["count"] = serde_json::json!(0);
            }
            "missing-terminal" => {
                events.pop();
            }
            "bad-sequence" => {
                events[4]["seq"] = serde_json::json!(99);
            }
            _ => {}
        }
        let descriptor = historical_fixture(root.path(), &events, &input);
        let bytes = serde_json::to_vec(&descriptor).unwrap();
        fs::write(root.path().join("descriptor.json"), &bytes).unwrap();
        let result = Engine
            .replay_registry_discovery(ReplayRequest {
                artifact_root: root.path().into(),
                descriptor: support::reference("descriptor.json", &bytes),
            })
            .unwrap();
        let owners = result
            .relationships
            .iter()
            .filter(|r| r.basis == pdx_native::DiscoveryBasis::HistoricalOwner)
            .count();
        assert_eq!(
            owners,
            usize::from(case == "normal"),
            "{case}: {:?}",
            result.gaps
        );
        if case == "normal" {
            let owner = result
                .relationships
                .iter()
                .find_map(|r| r.owner.as_ref())
                .unwrap();
            assert_eq!(result.relationships_for(owner).unwrap().len(), 1);
            let other = Engine
                .replay_registry_discovery(ReplayRequest {
                    artifact_root: root.path().into(),
                    descriptor: support::reference("descriptor.json", &bytes),
                })
                .unwrap();
            assert!(other.relationships_for(owner).is_err());
        }
        if case != "normal" {
            assert!(!result.gaps.is_empty());
        }
    }
}
#[test]
fn removed_scheduler_receiver_is_not_an_empty_success() {
    let mut input = input();
    input.code.drain(..4);
    input.layout.start += 4;
    assert!(
        scheduler(&input)
            .unwrap()
            .0
            .iter()
            .all(|r| r.status == "gap")
    );
}

fn replay_fixture(
    root: &std::path::Path,
    descriptor: &DiscoveryDescriptor,
) -> Result<pdx_native::RegistryDiscoveryResult, pdx_native::ReplayError> {
    let bytes = serde_json::to_vec(descriptor).unwrap();
    fs::write(root.join("descriptor.json"), &bytes).unwrap();
    Engine.replay_registry_discovery(ReplayRequest {
        artifact_root: root.into(),
        descriptor: support::reference("descriptor.json", &bytes),
    })
}
#[test]
fn historical_terminal_closes_the_observation_window() {
    let root = tempfile::tempdir().unwrap();
    let mut events = historical_events();
    events.insert(1, serde_json::json!({"kind":"stream-end"}));
    for (i, event) in events.iter_mut().enumerate() {
        event["seq"] = serde_json::json!(i + 1);
    }
    let descriptor = historical_fixture(root.path(), &events, &input());
    let result = replay_fixture(root.path(), &descriptor).unwrap();
    assert!(result.relationships.is_empty());
    assert!(
        result
            .gaps
            .iter()
            .any(|g| g.kind == DiscoveryGapKind::HistoricalIntegrity
                && g.reason.contains("after the terminal"))
    );
    assert!(
        result
            .gaps
            .iter()
            .any(|g| g.kind == DiscoveryGapKind::UnobservedCandidate)
    );
    let mut events = historical_events();
    events.push(serde_json::json!({"seq":7,"kind":"worker-ended"}));
    events.push(serde_json::json!({"seq":8,"kind":"worker-kill"}));
    let descriptor = historical_fixture(root.path(), &events, &input());
    let result = replay_fixture(root.path(), &descriptor).unwrap();
    assert!(
        result
            .gaps
            .iter()
            .all(|g| g.kind != DiscoveryGapKind::HistoricalIntegrity)
    );
}
#[test]
fn aggregate_budget_is_validated_before_reading_replay_inputs() {
    let root = tempfile::tempdir().unwrap();
    let base = historical_fixture(root.path(), &historical_events(), &input());
    fs::remove_file(root.path().join("input.json")).unwrap();
    for (case, reason) in [
        ("many", "at most two"),
        ("repeated", "duplicate or empty"),
        ("bytes", "aggregate"),
        ("overflow", "aggregate"),
    ] {
        let mut descriptor = base.clone();
        match case {
            "many" => descriptor.runs = vec![descriptor.runs[0].clone(); 3],
            "repeated" => descriptor.runs.push(descriptor.runs[0].clone()),
            "bytes" => {
                descriptor.input.bytes = 40 * 1024 * 1024;
                descriptor.runs[0].trace.bytes = 40 * 1024 * 1024;
            }
            "overflow" => descriptor.runs[0].trace.bytes = u64::MAX,
            _ => unreachable!(),
        }
        let error = replay_fixture(root.path(), &descriptor).unwrap_err();
        assert!(
            matches!(error,pdx_native::ReplayError::Malformed{reason:message,..} if message.contains(reason)),
            "{case}"
        );
    }
}
#[test]
fn historical_artifacts_must_share_the_capture_identity_and_seal() {
    let root = tempfile::tempdir().unwrap();
    for name in ["trace.jsonl", "table.json", "run.json", "manifest.json"] {
        let mut descriptor = historical_fixture(root.path(), &historical_events(), &input());
        let path = root.path().join(name);
        let bytes = if name == "trace.jsonl" {
            let mut rows: Vec<serde_json::Value> = fs::read_to_string(&path)
                .unwrap()
                .lines()
                .map(|s| serde_json::from_str(s).unwrap())
                .collect();
            rows[0]["run"] = serde_json::json!("foreign");
            rows.iter()
                .map(|r| serde_json::to_string(r).unwrap())
                .collect::<Vec<_>>()
                .join("\n")
                .into_bytes()
        } else {
            let mut value: serde_json::Value =
                serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            if name == "table.json" {
                value[0]["run"] = serde_json::json!("foreign");
            } else {
                value["run"] = serde_json::json!("foreign");
            }
            serde_json::to_vec(&value).unwrap()
        };
        fs::write(&path, &bytes).unwrap();
        let reference = support::reference(name, &bytes);
        let run = &mut descriptor.runs[0];
        let member = match name {
            "trace.jsonl" => {
                run.trace = reference.clone();
                "trace.jsonl"
            }
            "table.json" => {
                run.table = reference.clone();
                "startup-table.json"
            }
            "run.json" => {
                run.result = reference.clone();
                "result.json"
            }
            _ => {
                run.manifest = reference.clone();
                "manifest.json"
            }
        };
        let error = replay_fixture(root.path(), &descriptor).unwrap_err();
        assert!(
            matches!(error,pdx_native::ReplayError::Malformed{reason,..} if reason.contains("does not belong"))
        );
        // Even resealing synthetic bytes cannot conceal an embedded foreign capture identity.
        let mut capsule: serde_json::Value =
            serde_json::from_slice(&fs::read(root.path().join("capsule.json")).unwrap()).unwrap();
        capsule["files"][format!("synthetic-run/{member}")] = serde_json::json!(reference.sha256);
        let bytes = serde_json::to_vec(&capsule).unwrap();
        fs::write(root.path().join("capsule.json"), &bytes).unwrap();
        descriptor.runs[0].capsule = support::reference("capsule.json", &bytes);
        let error = replay_fixture(root.path(), &descriptor).unwrap_err();
        assert!(
            matches!(error,pdx_native::ReplayError::Malformed{reason,..} if reason.contains("conflicting capture identities"))
        );
    }
    let mut descriptor = historical_fixture(root.path(), &historical_events(), &input());
    descriptor.capture_origin = CaptureOrigin::Captured;
    let error = replay_fixture(root.path(), &descriptor).unwrap_err();
    assert!(
        matches!(error,pdx_native::ReplayError::Malformed{reason,..} if reason.contains("accepted SDK-489"))
    );
}
