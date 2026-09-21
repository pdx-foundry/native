//! Recorded answers stand in for an installation and a game. No process starts in these tests.
use pdx_native::{
    Basis, Completeness, Disposal, Error, GameOptions, GapKind, Native, ReaderKind, Support,
};
use serde_json::json;
use std::{fs, path::Path};

fn source() -> serde_json::Value {
    json!({ "build": "example-build", "native_version": "0.1.0",
            "method": "example/v1", "basis": "LiveObservation" })
}

fn write(root: &Path, file: &str, value: serde_json::Value) {
    let path = root.join(file);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

/// Files that a consumer could write by hand: a complete answer, a partial answer, and an error.
fn recorded() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "build.json", json!("example-build"));
    write(
        root.path(),
        "registries.json",
        json!({ "Ok": { "value": [{ "name": "common/traditions" }], "completeness": "Partial",
            "gaps": [{ "kind": "OutsideMethod", "subject": null, "detail": "Example." }],
            "source": source() } }),
    );
    write(
        root.path(),
        "registry_items/common/traditions.json",
        json!({ "Ok": { "value": ["tr_example_adopt", "tr_example_finish"],
            "completeness": "Complete", "gaps": [], "source": source() } }),
    );
    for (file, key) in [
        (
            "registry_items/common/governments/civics.json",
            "civic_example",
        ),
        ("registry_items/map/galaxy.json", "galaxy_example"),
    ] {
        write(
            root.path(),
            file,
            json!({ "Ok": { "value": [key],
            "completeness": "Complete", "gaps": [], "source": source() } }),
        );
    }
    write(
        root.path(),
        "registry_fields/common/traditions.json",
        json!({ "Ok": { "value": [
            { "name": "boolean", "reader": { "id": "boolean", "kind": "Boolean" }, "conditional": false },
            { "name": "integer", "reader": { "id": "integer", "kind": "Integer" }, "conditional": false },
            { "name": "fixed", "reader": { "id": "fixed", "kind": "FixedPoint" }, "conditional": false },
            { "name": "string", "reader": { "id": "string", "kind": "String" }, "conditional": false },
            { "name": "reference", "reader": { "id": "reference", "kind": "Reference" }, "conditional": false },
            { "name": "block", "reader": { "id": "block", "kind": "Block" }, "conditional": false },
            { "name": "unknown", "reader": { "id": null, "kind": "Unknown" }, "conditional": true }
        ], "completeness": "Partial",
            "gaps": [{ "kind": "ReaderSemantics", "subject": "unknown", "detail": "Example." }],
            "source": source() } }),
    );
    write(
        root.path(),
        "registry_items/common/tradition_categories.json",
        json!({ "Err": { "Observation": { "operation": "RegistryItems",
            "reason": "the observation worker was lost" } } }),
    );
    root
}

#[test]
fn static_questions_read_recorded_files_and_always_report_the_recorded_basis() {
    let root = recorded();
    let native = Native::from_recorded_answers(root.path()).unwrap();
    assert_eq!(
        native.supports(pdx_native::Operation::Registries),
        Support::Supported
    );
    let answer = native.registries().unwrap();
    assert_eq!(answer.value[0].name, "common/traditions");
    assert_eq!(answer.completeness, Completeness::Partial);
    assert_eq!(answer.gaps[0].kind, GapKind::OutsideMethod);
    // The file says LiveObservation. A recorded answer never passes as an observation.
    assert_eq!(answer.source.basis, Basis::Recorded);
    assert_eq!(answer.source.build, native.build());

    let fields = native.registry_fields("common/traditions").unwrap();
    assert_eq!(
        fields
            .value
            .iter()
            .map(|field| field.reader.kind)
            .collect::<Vec<_>>(),
        [
            ReaderKind::Boolean,
            ReaderKind::Integer,
            ReaderKind::FixedPoint,
            ReaderKind::String,
            ReaderKind::Reference,
            ReaderKind::Block,
            ReaderKind::Unknown,
        ]
    );
    assert_eq!(fields.source.basis, Basis::Recorded);
}

#[test]
fn a_question_with_no_file_is_not_recorded_and_never_an_empty_answer() {
    let root = recorded();
    let native = Native::from_recorded_answers(root.path()).unwrap();
    for registry in ["common/armies", "../registries", "common/../../escape", ""] {
        assert!(
            matches!(
                native.registry_fields(registry),
                Err(Error::NotRecorded { .. })
            ),
            "{registry}"
        );
    }
    fs::write(root.path().join("registries.json"), "damaged").unwrap();
    assert!(matches!(native.registries(), Err(Error::Recorded(_))));
}

#[test]
fn opening_recorded_answers_requires_valid_build_metadata() {
    let root = tempfile::tempdir().unwrap();
    assert!(matches!(
        Native::from_recorded_answers(root.path()),
        Err(Error::Recorded(_))
    ));
    for invalid in ["broken", "{}", "null"] {
        fs::write(root.path().join("build.json"), invalid).unwrap();
        assert!(matches!(
            Native::from_recorded_answers(root.path()),
            Err(Error::Recorded(_))
        ));
    }
}

#[tokio::test]
async fn static_and_live_answers_from_another_build_are_refused() {
    let root = recorded();
    write(root.path(), "build.json", json!("another-build"));
    let native = Native::from_recorded_answers(root.path()).unwrap();
    assert!(matches!(native.registries(), Err(Error::Recorded(_))));
    let mut game = native
        .start_game(GameOptions::new(std::process::Command::new(
            "must-not-start",
        )))
        .await
        .unwrap();
    assert!(matches!(
        game.registry_items("common/traditions").await,
        Err(Error::Recorded(_))
    ));
    assert_eq!(game.close().await.unwrap(), Disposal::NotApplicable);
}

#[tokio::test]
async fn live_questions_need_no_supervisor_and_start_no_process() {
    let root = recorded();
    let native = Native::from_recorded_answers(root.path()).unwrap();
    // Recorded answers ignore the options: this command is never started.
    let options = GameOptions::new(std::process::Command::new("must-not-start"));
    let mut game = native.start_game(options).await.unwrap();
    let items = game.registry_items("common/traditions").await.unwrap();
    assert_eq!(items.value, ["tr_example_adopt", "tr_example_finish"]);
    assert_eq!(items.completeness, Completeness::Complete);
    assert_eq!(items.source.basis, Basis::Recorded);
    assert_eq!(items.source.build, native.build());
    for (name, key) in [
        ("common/governments/civics", "civic_example"),
        ("map/galaxy", "galaxy_example"),
    ] {
        assert_eq!(game.registry_items(name).await.unwrap().value, [key]);
    }
    // A hand-written failure case comes back as the same error.
    assert!(matches!(
        game.registry_items("common/tradition_categories").await,
        Err(Error::Observation { .. })
    ));
    assert!(matches!(
        game.registry_items("common/armies").await,
        Err(Error::NotRecorded { .. })
    ));
    assert_eq!(game.close().await.unwrap(), Disposal::NotApplicable);
    assert!(matches!(
        game.registry_items("common/traditions").await,
        Err(Error::Closed)
    ));
}
