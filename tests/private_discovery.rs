use pdx_native::internals::discovery::{StaticInput, candidates, scheduler};
use pdx_native::{
    DiscoveryBasis, DiscoveryGapKind, Engine, RegistryDiscoveryResult, ReplayRequest,
};
use serde_json::Value;
use std::{collections::BTreeMap, fs, path::PathBuf};
#[path = "analysis_support/mod.rs"]
mod support;
fn root() -> PathBuf {
    PathBuf::from(std::env::var_os("PDX_NATIVE_DISCOVERY_OUTPUT").expect("prepared discovery root"))
}
fn retained() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(".local/evidence/restored/atlas-ownership/prototype/registry-ownership")
}
fn read(path: impl AsRef<std::path::Path>) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn replay(root: &std::path::Path, descriptor: &Value) -> RegistryDiscoveryResult {
    let bytes = serde_json::to_vec(descriptor).unwrap();
    fs::write(root.join("test-descriptor.json"), &bytes).unwrap();
    Engine
        .replay_registry_discovery(ReplayRequest {
            artifact_root: root.into(),
            descriptor: support::reference("test-descriptor.json", &bytes),
        })
        .unwrap()
}
fn trace(root: &std::path::Path, run: &str) -> Vec<Value> {
    fs::read_to_string(root.join(run).join("trace.jsonl"))
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}
fn event_ref<'a>(
    result: &'a RegistryDiscoveryResult,
    trace: &'a [Value],
    path: &str,
    basis: DiscoveryBasis,
) -> Vec<&'a Value> {
    result
        .relationships
        .iter()
        .filter(|r| r.basis == basis)
        .flat_map(|r| &r.evidence)
        .filter(|e| e.artifact.path == path)
        .filter_map(|e| trace.get(e.record? as usize - 1))
        .filter(|r| r["kind"] == "owner-reader")
        .collect()
}
#[test]
#[ignore = "requires verified SDK-489 evidence and a new static discovery capture"]
fn retained_discovery_parity_and_41_controls() {
    let root = root();
    let old = retained();
    let descriptor = read(root.join("historical-descriptor.json"));
    let result = replay(&root, &descriptor);
    let input: StaticInput =
        serde_json::from_slice(&fs::read(root.join("registry-discovery/input.json")).unwrap())
            .unwrap();
    let records = candidates(&input.symbols);
    let (rows, _) = scheduler(&input).unwrap();
    let mut checks = BTreeMap::<String, bool>::new();
    checks.insert(
        "candidateReplay".into(),
        serde_json::to_value(&records).unwrap() == read(old.join("candidates.json")),
    );
    let expected = read(old.join("static-table.json"));
    checks.insert(
        "schedulerReplay".into(),
        rows.iter().zip(expected.as_array().unwrap()).all(|(r, e)| {
            let mut e = e.clone();
            e.as_object_mut().unwrap().remove("slots");
            serde_json::to_value(r).unwrap() == e
        }),
    );
    checks.insert(
        "schedulerRecovered198".into(),
        rows.len() == 198 && rows.iter().all(|r| r.status == "recovered"),
    );
    let development = "runs/20260917-185056";
    let final_run = "runs/20260917-185330";
    let same_table = |run: &str| {
        let table = read(root.join(run).join("startup-table.json"));
        expected
            .as_array()
            .unwrap()
            .iter()
            .zip(table.as_array().unwrap())
            .all(|(a, b)| a["name"] == b["name"] && a["slots"] == b["slots"])
    };
    checks.insert(
        "staticLiveSchedulerAgreement".into(),
        same_table(development) && same_table(final_run),
    );
    let mut missing = input.clone();
    // Remove every used pointer rather than selecting an unrelated executable fixup.
    missing.pointers.clear();
    checks.insert(
        "missingPointerCreatesGap".into(),
        scheduler(&missing)
            .unwrap()
            .0
            .iter()
            .any(|r| r.status == "gap"),
    );
    missing = input.clone();
    missing.strings.clear();
    checks.insert(
        "missingNamesCreateGaps".into(),
        scheduler(&missing)
            .unwrap()
            .0
            .iter()
            .all(|r| r.status == "gap"),
    );
    let mut shortened = input.clone();
    shortened.layout.count = 197;
    checks.insert(
        "missingRowNotComplete".into(),
        scheduler(&shortened).unwrap().0.len() == 197,
    );
    let mut omitted = input.clone();
    omitted.symbols.retain(|s| {
        !s.name
            .starts_with("TSingleObjectGameDatabase<CTechnologyDatabase,")
            || !s.name.ends_with("::LoadFile(char const*, bool)")
    });
    let bytes = serde_json::to_vec(&omitted).unwrap();
    let mut d: pdx_native::DiscoveryDescriptor =
        serde_json::from_value(descriptor.clone()).unwrap();
    d.input = support::reference("omission.json", &bytes);
    d.runs.clear();
    let omission =
        pdx_native::internals::discovery::derive(d, &bytes, pdx_native::AnalysisOrigin::Replay)
            .unwrap();
    checks.insert(
        "omissionWitnessFindsTechnology".into(),
        omission
            .scheduling
            .iter()
            .filter(|r| r.candidates.is_empty())
            .count()
            > 35,
    );
    checks.insert(
        "independentRediscoveryRestoresTechnology".into(),
        records.iter().any(|r| r.database == "CTechnologyDatabase"),
    );
    omitted.symbols.clear();
    let bytes = serde_json::to_vec(&omitted).unwrap();
    let mut d: pdx_native::DiscoveryDescriptor =
        serde_json::from_value(descriptor.clone()).unwrap();
    d.input = support::reference("stripped.json", &bytes);
    d.runs.clear();
    let stripped =
        pdx_native::internals::discovery::derive(d, &bytes, pdx_native::AnalysisOrigin::Replay)
            .unwrap();
    checks.insert(
        "strippedNamesRetain198UnresolvedWitnesses".into(),
        stripped.candidates.is_empty()
            && stripped.scheduling.len() == 198
            && stripped.scheduling.iter().all(|r| r.candidates.is_empty()),
    );
    let final_trace = trace(&root, final_run);
    let expected_snapshot = read(old.join("snapshot.json"));
    let actual_roots = event_ref(
        &result,
        &final_trace,
        &format!("{final_run}/trace.jsonl"),
        DiscoveryBasis::HistoricalOwner,
    );
    let heldout: Vec<_> = actual_roots
        .into_iter()
        .filter(|r| r["database"] == "CAIEconomicPlanDatabase")
        .collect();
    let expected_roots = expected_snapshot["qualification"]["heldout"]["rootReads"]
        .as_array()
        .unwrap();
    checks.insert(
        "heldoutRootJoin".into(),
        heldout.len() == 6
            && heldout.iter().copied().eq(expected_roots.iter())
            && final_trace
                .iter()
                .filter(|r| {
                    r["kind"] == "owner-reader" && r["database"] == "CAIEconomicPlanDatabase"
                })
                .count()
                == 249,
    );
    checks.insert(
        "incidentalReadsAreNotDefinitions".into(),
        heldout.iter().all(|r| {
            r["key"].as_str().is_some_and(|k| !k.is_empty())
                && r["vtable"]["symbol"] == "vtable for CAIEconomicPlan"
        }),
    );
    let unobserved = result
        .gaps
        .iter()
        .filter(|g| g.kind == DiscoveryGapKind::UnobservedCandidate)
        .count();
    let families = expected_snapshot["families"].as_array().unwrap();
    let family_parity = records
        .iter()
        .zip(families)
        .enumerate()
        .all(|(i, (candidate, family))| {
            let events: Vec<_> = result
                .relationships
                .iter()
                .filter(|r| {
                    r.basis == DiscoveryBasis::HistoricalLoader
                        && r.subject.as_ref() == Some(&result.candidates[i].subject)
                })
                .flat_map(|r| &r.evidence)
                .filter(|e| e.artifact.path == format!("{final_run}/trace.jsonl"))
                .filter_map(|e| final_trace.get(e.record? as usize - 1))
                .collect();
            let files: std::collections::BTreeSet<_> =
                events.iter().filter_map(|r| r["file"].as_str()).collect();
            let directories: std::collections::BTreeSet<_> = events
                .iter()
                .filter_map(|r| r["directory"].as_str())
                .collect();
            family["database"] == candidate.database
                && family["ownerCandidate"] == candidate.owner_candidate
                && family["loader"] == candidate.loader
                && family["hasNamedMemberReader"] == candidate.has_named_member_reader
                && family["loaded"] == !events.is_empty()
                && files.iter().copied().eq(family["observedFiles"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|s| s.as_str().unwrap()))
                && directories
                    .iter()
                    .copied()
                    .eq(family["ownerDirectoryValues"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|s| s.as_str().unwrap()))
        });
    let scheduling_parity = result
        .scheduling
        .iter()
        .zip(expected_snapshot["startupWitness"].as_array().unwrap())
        .all(|(row, expected)| {
            let matches: Vec<_> = row
                .candidates
                .iter()
                .map(|s| {
                    let candidate = result.subject(s).unwrap();
                    let i = result
                        .candidates
                        .iter()
                        .position(|r| r.subject == candidate.subject)
                        .unwrap();
                    records[i].database.as_str()
                })
                .collect();
            matches.iter().copied().eq(expected["candidateMatches"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s.as_str().unwrap()))
        });
    checks.insert(
        "snapshotReplay".into(),
        family_parity
            && scheduling_parity
            && result.candidates.len() == 164
            && unobserved == 2
            && result.scheduling.len() == 198
            && heldout.iter().copied().eq(expected_roots.iter()),
    );
    for run in [development, final_run] {
        let t = trace(&root, run);
        let status = read(root.join(run).join("result.json"));
        checks.insert(
            format!("{run}/completeAndDisposed"),
            [
                "completed",
                "sequenceContiguous",
                "disposalConfirmed",
                "binaryUnchanged",
                "protectedUnchanged",
                "contentUnchanged",
            ]
            .iter()
            .all(|k| status[k] == true),
        );
        checks.insert(
            format!("{run}/sequence"),
            t.iter().enumerate().all(|(i, r)| r["seq"] == i + 1),
        );
        checks.insert(
            format!("{run}/negativeDirectoryUnobserved"),
            !t.iter().any(|r| {
                r["path"]
                    .as_str()
                    .unwrap_or("")
                    .contains("atlas_registry_not_a_database")
                    || r["file"]
                        .as_str()
                        .unwrap_or("")
                        .contains("atlas_registry_negative")
            }),
        );
        checks.insert(
            format!("{run}/nonTxtFileObserved"),
            t.iter().any(|r| {
                r["kind"] == "load-file"
                    && r["file"]
                        .as_str()
                        .unwrap_or("")
                        .ends_with("atlas_registry_wrong_extension.bin")
            }),
        );
        checks.insert(
            format!("{run}/earlyActivation"),
            t[0]["atDyldEntry"] == true
                && t[0]["hooks"]
                    .as_object()
                    .unwrap()
                    .values()
                    .all(|h| h["resolved"] == 1 && h["hits"] == 0),
        );
        checks.insert(
            format!("{run}/customTwoPasses"),
            t.iter()
                .filter(|r| r["kind"] == "custom-start")
                .map(|r| r["mode"].as_u64().unwrap())
                .collect::<Vec<_>>()
                == [1, 0],
        );
    }
    for (order, run, shared) in [
        ("normal", development, "atlas_registry_shared_b"),
        ("reversed", final_run, "atlas_registry_shared_a"),
    ] {
        let t = trace(&root, run);
        for (kind, event) in [
            ("template", "constructor-key"),
            ("custom", "custom-owner-reader"),
        ] {
            let fixtures: Vec<_> = t
                .iter()
                .filter(|r| {
                    r["kind"] == event
                        && r["key"]
                            .as_str()
                            .unwrap_or("")
                            .starts_with("atlas_registry")
                })
                .collect();
            let shared_keys: std::collections::BTreeSet<_> = fixtures
                .iter()
                .filter_map(|r| r["key"].as_str())
                .filter(|k| k.contains("shared"))
                .collect();
            checks.insert(
                format!("{order}/{kind}/mountReplacement"),
                shared_keys == std::collections::BTreeSet::from([shared]),
            );
            let duplicates: Vec<_> = fixtures
                .iter()
                .filter(|r| r["key"] == "atlas_registry_duplicate")
                .collect();
            let basis = if kind == "template" {
                DiscoveryBasis::HistoricalOwner
            } else {
                DiscoveryBasis::HistoricalCustomOwner
            };
            let owner_links: Vec<_> = result
                .relationships
                .iter()
                .filter(|r| {
                    r.basis == basis
                        && r.key.as_deref() == Some("atlas_registry_duplicate")
                        && r.evidence
                            .iter()
                            .any(|e| e.artifact.path == format!("{run}/trace.jsonl"))
                })
                .collect();
            assert!(
                owner_links.len() >= 2,
                "missing public duplicate owner joins for {order}/{kind}"
            );
            assert!(
                owner_links
                    .iter()
                    .all(|r| r.owner.is_some() && r.owner == owner_links[0].owner),
                "public duplicate identity changed"
            );
            checks.insert(
                format!("{order}/{kind}/duplicateOwnerReused"),
                duplicates.len() >= 2
                    && duplicates
                        .iter()
                        .all(|r| r["owner"] == duplicates[0]["owner"]),
            );
            checks.insert(
                format!("{order}/{kind}/observedFileOrder"),
                duplicates
                    .iter()
                    .map(|r| r["file"].as_str().unwrap().rsplit('/').next().unwrap())
                    .collect::<Vec<_>>()
                    == ["atlas_registry_a.txt", "atlas_registry_b.txt"],
            );
        }
    }
    for (key, file, field) in [
        (
            "frozenMethodUnchanged",
            "qualification-freeze.json",
            "files",
        ),
        (
            "engineResultsStillFrozen",
            "engine-results-freeze.json",
            "files",
        ),
        ("capsuleHashes", "capsule.json", "files"),
    ] {
        let manifest = read(old.join(file));
        checks.insert(
            key.into(),
            manifest[field]
                .as_object()
                .unwrap()
                .iter()
                .all(|(path, hash)| {
                    support::reference(path, &fs::read(old.join(path)).unwrap()).sha256
                        == hash.as_str().unwrap()
                }),
        );
    }
    let freeze = read(old.join("qualification-freeze.json"));
    checks.insert(
        "capturedObserverMatchesFreeze".into(),
        support::reference(
            "source",
            &fs::read(old.join(final_run).join("source/qual_worker.py")).unwrap(),
        )
        .sha256
            == freeze["files"]["native/qual_worker.py"],
    );
    assert_eq!(checks.len(), 41);
    let failures: Vec<_> = checks.iter().filter(|(_, passed)| !**passed).collect();
    fs::write(
        root.join("rust-controls.json"),
        serde_json::to_vec_pretty(&checks).unwrap(),
    )
    .unwrap();
    fs::write(
        root.join("historical-result.json"),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
    assert!(
        failures.is_empty(),
        "{failures:?}; owner joins {}, gaps {:?}",
        heldout.len(),
        result
            .gaps
            .iter()
            .map(|g| (&g.kind, &g.reason))
            .take(10)
            .collect::<Vec<_>>()
    );
}

#[test]
#[ignore = "requires promoted M45 discovery and executable"]
fn public_discovery_admission_and_replay() {
    use pdx_native::{
        AnalysisError, Availability, CapabilityRequest, Native, OpenRequest, UnavailableReason,
    };
    let output = root();
    let executable = PathBuf::from(
        std::env::var_os("PDX_NATIVE_ANALYSIS_EXECUTABLE").expect("exact executable"),
    );
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("image");
    fs::copy(executable, &path).unwrap();
    let native = Native::open(OpenRequest {
        installation_hint: path.clone(),
    })
    .unwrap();
    assert_eq!(
        native
            .capability(&CapabilityRequest::RegistryDiscovery)
            .availability,
        Availability::Available
    );
    let context = native.analysis().unwrap();
    let result = context.discover_registries().unwrap();
    assert_eq!(result.candidates.len(), 164);
    assert!(result.relationships.is_empty());
    assert_eq!(
        result
            .gaps
            .iter()
            .filter(|g| g.kind == DiscoveryGapKind::UnobservedCandidate)
            .count(),
        164
    );
    let second = native.analysis().unwrap().discover_registries().unwrap();
    assert!(second.subject(&result.candidates[0].subject).is_err());
    assert_eq!(
        directory.path().read_dir().unwrap().count(),
        1,
        "static discovery must not create process/profile artifacts"
    );
    assert_eq!(
        result.input_bytes(),
        fs::read(output.join("registry-discovery/input.json")).unwrap()
    );
    let descriptor = serde_json::to_value(&result.descriptor).unwrap();
    let replayed = replay(&output, &descriptor);
    let mut expected = serde_json::to_value(&result).unwrap();
    expected["origin"] = serde_json::json!("Replay");
    assert_eq!(serde_json::to_value(replayed).unwrap(), expected);
    let live = native.capability(&CapabilityRequest::default());
    assert_eq!(live.availability, Availability::Unavailable);
    assert_eq!(
        native
            .capability(&CapabilityRequest::RegistryDiscovery)
            .availability,
        Availability::Available
    );
    fs::write(&path, b"changed").unwrap();
    assert!(
        matches!(context.discover_registries(),Err(AnalysisError::Unavailable{reasons}) if reasons==[UnavailableReason::TargetChanged])
    );
    assert!(context.decode_control().is_err());
}

#[test]
#[ignore = "requires verified SDK-489 capture identities"]
fn captured_replay_rejects_cross_run_artifact_substitution() {
    let root = root();
    let mut descriptor: pdx_native::DiscoveryDescriptor =
        serde_json::from_value(read(root.join("historical-descriptor.json"))).unwrap();
    let foreign = descriptor.runs[1].result.clone();
    descriptor.runs.truncate(1);
    descriptor.runs[0].result = foreign;
    let raw = serde_json::to_vec(&descriptor).unwrap();
    fs::write(root.join("cross-run-descriptor.json"), &raw).unwrap();
    let error = Engine
        .replay_registry_discovery(ReplayRequest {
            artifact_root: root,
            descriptor: support::reference("cross-run-descriptor.json", &raw),
        })
        .unwrap_err();
    assert!(
        matches!(error,pdx_native::ReplayError::Malformed{reason,..} if reason.contains("does not belong to the declared capture"))
    );
}
