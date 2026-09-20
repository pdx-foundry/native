//! Exact-target SDK-487 parity and ordinary public-operation controls. Never launches a game.
use evidence::{
    fields::{self, FieldInput, Value},
    store::ArtifactStore,
};
use pdx_native::{
    AnalysisError, AnalysisOrigin, Availability, CapabilityRequest, Engine, Native, OpenRequest,
    PathOutcome, ReaderJoin, ReplayRequest,
};
use serde_json::Value as Json;
use std::{
    fs,
    path::{Path, PathBuf},
};
#[path = "analysis_support/mod.rs"]
mod support;
fn output() -> PathBuf {
    std::env::var_os("PDX_NATIVE_FIELDS_OUTPUT")
        .expect("prepared private fields evidence")
        .into()
}
fn replay(root: &Path) -> pdx_native::RegistryFieldResult {
    let reference =
        serde_json::from_slice(&fs::read(root.join("descriptor.ref.json")).unwrap()).unwrap();
    Engine
        .replay_registry_fields(ReplayRequest {
            artifact_root: root.into(),
            descriptor: reference,
        })
        .unwrap()
}
fn old_value(value: &Json) -> Value {
    let n = || value[1].as_i64().unwrap();
    match value[0].as_str().unwrap() {
        "owner" => Value::Owner(n()),
        "reader" => Value::Reader(n()),
        "token" => Value::Token,
        "constant" => Value::Constant(n()),
        "stack" => Value::Stack(n()),
        "load" => {
            let base = match value[1].as_str().unwrap() {
                "owner" => Value::Owner(value[2].as_i64().unwrap()),
                other => panic!("unexpected retained load {other}"),
            };
            Value::Load(Box::new(base), value[3].as_u64().unwrap() as u8)
        }
        other => panic!("unsupported retained value {other}"),
    }
}
fn address(value: &Json) -> u64 {
    u64::from_str_radix(value.as_str().unwrap().trim_start_matches("0x"), 16).unwrap()
}
#[test]
#[ignore = "requires freshly captured fields and verified SDK-487 comparison inputs"]
fn retained_agenda_paths_names_and_reader_arguments_match() {
    let output = output();
    let root = output.join("council_agenda");
    let result = replay(&root);
    let comparison = output.join("comparison");
    let refs: std::collections::BTreeMap<String, pdx_native::ArtifactReference> =
        serde_json::from_slice(&fs::read(comparison.join("references.json")).unwrap()).unwrap();
    let store = ArtifactStore::new(&comparison);
    let expected: Json =
        serde_json::from_slice(&store.read(&refs["inventory.json"]).unwrap()).unwrap();
    assert_eq!(result.fields.len(), 10);
    assert_eq!(result.paths.len(), 21);
    assert!(!result.complete_registry);
    assert_eq!(result.blocking_readers.len(), 5);
    let contract: Json =
        serde_json::from_slice(&store.read(&refs["registry-contract.json"]).unwrap()).unwrap();
    let expected_blockers: std::collections::BTreeSet<_> = contract["blockingGaps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|gap| gap["reader"].as_str().unwrap())
        .collect();
    assert_eq!(
        result
            .blocking_readers
            .iter()
            .map(|gap| gap.reader.as_str())
            .collect::<std::collections::BTreeSet<_>>(),
        expected_blockers
    );

    for (field, old) in result
        .fields
        .iter()
        .zip(expected["fields"].as_array().unwrap())
    {
        assert_eq!(field.name, old["name"].as_str().unwrap());
        assert_eq!(field.token, old["token"].as_i64().unwrap());
        assert_eq!(field.constructor, address(&old["tokenEvidence"]));
        let paths: Vec<usize> = old["paths"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                p.as_str()
                    .unwrap()
                    .trim_start_matches("path-")
                    .parse()
                    .unwrap()
            })
            .collect();
        assert_eq!(field.paths, paths);
    }
    for (path, old) in result
        .paths
        .iter()
        .zip(expected["paths"].as_array().unwrap())
    {
        assert_eq!(serde_json::to_value(path.domain).unwrap(), old["domain"]);
        assert_eq!(path.terminal, address(&old["terminal"]));
        assert_eq!(
            path.instructions,
            old["path"]
                .as_array()
                .unwrap()
                .iter()
                .map(address)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            path.conditions.len(),
            old["conditions"].as_array().unwrap().len()
        );
        for (condition, prior) in path
            .conditions
            .iter()
            .zip(old["conditions"].as_array().unwrap())
        {
            assert_eq!(condition.at, address(&prior["at"]));
            assert_eq!(condition.value, Some(old_value(&prior["value"])));
            assert_eq!(condition.zero, prior["is"] == "zero");
        }
        if old["kind"] == "unexpected-member" {
            assert!(matches!(path.outcome, PathOutcome::Rejected));
            continue;
        }
        let PathOutcome::Reader(ReaderJoin::Joined { callee, arguments }) = &path.outcome else {
            panic!("missing retained reader: {path:?}")
        };
        assert_eq!(callee, old["callee"].as_str().unwrap());
        let expected_arguments = old["arguments"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(key, value)| (key.clone(), old_value(value)))
            .collect();
        assert_eq!(*arguments, expected_arguments);
    }
    // Compare the captured raw bytes with the retained disassembly, not only its normalized answer.
    let input: FieldInput = serde_json::from_slice(result.input_bytes()).unwrap();
    let manifest: Json =
        serde_json::from_slice(&store.read(&refs["manifest.json"]).unwrap()).unwrap();
    for function in &input.functions {
        let artifact = manifest["functions"][&function.name]["artifact"]
            .as_str()
            .unwrap();
        let raw = store.read(&refs[artifact]).unwrap();
        let disassembly = String::from_utf8(raw).unwrap();
        let words: Vec<u8> = disassembly
            .lines()
            .filter_map(|line| {
                let (at, tail) = line.split_once(':')?;
                u64::from_str_radix(at, 16).ok()?;
                let word = tail.split_whitespace().next()?;
                if word.len() != 8 {
                    return None;
                }
                u32::from_str_radix(word, 16).ok()
            })
            .flat_map(u32::to_le_bytes)
            .collect();
        assert_eq!(function.code, words, "{}", function.name);
    }
    let prior: std::collections::BTreeSet<_> = result
        .fields
        .iter()
        .skip(1)
        .map(|f| f.name.clone())
        .collect();
    let again = replay(&root);
    let added: Vec<_> = again
        .fields
        .iter()
        .filter(|f| !prior.contains(&f.name))
        .map(|f| &f.name)
        .collect();
    assert_eq!(added, [&result.fields[0].name]);
    fs::write(output.join("parity.json"),serde_json::to_vec_pretty(&serde_json::json!({"exact_fields":10,"exact_token_paths":21,"exact_reader_arguments":true,"exact_raw_functions":true,"omission_rediscovered":added,"complete_registry":false,"blocking_readers":result.blocking_readers})).unwrap()).unwrap();
}
#[test]
#[ignore = "requires prepared exact-executable fields inputs"]
fn captured_omission_clobber_and_unknown_shape_controls() {
    let output = output();
    let result = replay(&output.join("council_agenda"));
    let input: FieldInput = serde_json::from_slice(result.input_bytes()).unwrap();
    let run = |input: &FieldInput| {
        let bytes = serde_json::to_vec(input).unwrap();
        let mut descriptor = result.descriptor.clone();
        descriptor.capture_origin = pdx_native::CaptureOrigin::Synthetic;
        descriptor.input = support::reference("input.json", &bytes);
        fields::derive(descriptor, &bytes, AnalysisOrigin::Replay).unwrap()
    };
    let mut broken = input.clone();
    broken.strings.clear();
    assert!(run(&broken).fields.is_empty());
    broken = input.clone();
    let root = broken
        .functions
        .iter_mut()
        .find(|f| f.name.starts_with("CCouncilAgenda::"))
        .unwrap();
    root.code[..4].copy_from_slice(&0xd65f03c0u32.to_le_bytes());
    assert!(run(&broken).fields.is_empty());
    broken = input.clone();
    broken
        .symbols
        .retain(|s| !s.name.starts_with("CReader::Read("));
    let missing = run(&broken);
    assert!(
        missing
            .fields
            .iter()
            .flat_map(|f| &f.readers)
            .any(|j| matches!(j, ReaderJoin::Missing { .. }))
    );
    broken = input.clone();
    let root = broken
        .functions
        .iter_mut()
        .find(|f| f.name.starts_with("CCouncilAgenda::"))
        .unwrap();
    // Replace every move of the original reader into x0 with a clobber. Static token labels survive,
    // but those paths must no longer claim their old joins.
    for word in root.code.as_chunks_mut::<4>().0 {
        if u32::from_le_bytes(*word) == 0xaa0103e0 {
            word.copy_from_slice(&0xaa1f03e0u32.to_le_bytes());
        }
    }
    let clobbered = run(&broken);
    assert!(
        clobbered
            .fields
            .iter()
            .flat_map(|f| &f.readers)
            .filter(|j| matches!(j, ReaderJoin::Missing { .. }))
            .count()
            >= 8
    );
    for name in ["traditions", "tradition_categories"] {
        let found = replay(&output.join(name));
        assert!(!found.fields.is_empty());
        assert!(
            found
                .fields
                .iter()
                .all(|f| f.paths.len() == f.readers.len() && !f.readers.is_empty())
        );
        assert!(!found.complete_registry);
    }
    fs::write(output.join("controls.json"),b"{\"missing_names\":true,\"unsupported_root\":true,\"missing_callees\":true,\"clobbered_receivers\":true,\"tradition_joins_explicit\":true}\n").unwrap();
}
#[test]
#[ignore = "requires promoted field/discovery compositions and exact M45 executable"]
fn public_fields_need_only_an_executable_and_reject_foreign_subjects() {
    let output = output();
    let executable = PathBuf::from(
        std::env::var_os("PDX_NATIVE_ANALYSIS_EXECUTABLE").expect("exact executable"),
    );
    let isolated = tempfile::tempdir().unwrap();
    let path = isolated.path().join("executable");
    fs::copy(executable, &path).unwrap();
    let native = Native::open(OpenRequest {
        installation_hint: path.clone(),
    })
    .unwrap();
    assert_eq!(
        native
            .capability(&CapabilityRequest::RegistryFields)
            .availability,
        Availability::Available
    );
    let context = native.analysis().unwrap();
    let discovery = context.discover_registries().unwrap();
    let static_input: evidence::discovery::StaticInput =
        serde_json::from_slice(discovery.input_bytes()).unwrap();
    let candidates = evidence::discovery::candidates(&static_input.symbols);
    let select = |database: &str| {
        discovery.candidates[candidates
            .iter()
            .position(|c| c.database == database)
            .unwrap()]
        .subject
        .clone()
    };
    let subject = select("CCouncilAgendaDatabase");
    let another = native.analysis().unwrap();
    assert!(matches!(
        another.analyze_subject(&discovery, &subject),
        Err(AnalysisError::ForeignSubject)
    ));
    let other_result = context.discover_registries().unwrap();
    assert!(matches!(
        context.analyze_subject(&other_result, &subject),
        Err(AnalysisError::ForeignSubject)
    ));
    for (name, database) in [
        ("council_agenda", "CCouncilAgendaDatabase"),
        ("traditions", "CTraditionTypeDatabase"),
        ("tradition_categories", "CTraditionCategoryDatabase"),
    ] {
        let found = context
            .analyze_subject(&discovery, &select(database))
            .unwrap();
        let retained = replay(&output.join(name));
        assert_eq!(
            found
                .fields
                .iter()
                .map(|f| (&f.name, &f.readers))
                .collect::<Vec<_>>(),
            retained
                .fields
                .iter()
                .map(|f| (&f.name, &f.readers))
                .collect::<Vec<_>>()
        );
        assert!(!found.descriptor.provenance.qualification_records.is_empty());
        let capture = output.join(format!("public-{name}"));
        fs::create_dir(&capture).unwrap();
        fs::create_dir(capture.join("registry-fields")).unwrap();
        fs::write(
            capture.join(&found.descriptor.input.path),
            found.input_bytes(),
        )
        .unwrap();
        let bytes = serde_json::to_vec(&found.descriptor).unwrap();
        fs::write(capture.join("descriptor.json"), &bytes).unwrap();
        fs::write(
            capture.join("descriptor.ref.json"),
            serde_json::to_vec(&support::reference("descriptor.json", &bytes)).unwrap(),
        )
        .unwrap();
        let replayed = replay(&capture);
        assert_eq!(found.fields, replayed.fields);
        assert_eq!(found.paths, replayed.paths);
    }
    assert_eq!(
        fs::read_dir(isolated.path()).unwrap().count(),
        1,
        "discovery must require no config, content or name seeds"
    );
    fs::write(&path, b"changed target").unwrap();
    assert!(matches!(
        context.analyze_subject(&discovery, &subject),
        Err(AnalysisError::Unavailable { .. })
    ));
    fs::write(output.join("public-controls.json"),b"{\"isolated_executable\":true,\"foreign_context_rejected\":true,\"foreign_result_rejected\":true,\"exact_replay\":true,\"target_change_rejected\":true,\"game_launches\":0}\n").unwrap();
}
