//! Recorded answers stand in for an installation and a game. No process starts in these tests.
use pdx_native::{Basis, Completeness, Error, GapKind, Native, OperationDisposal, Support};
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
    let native = Native::from_recorded_answers(root.path());
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
}

#[test]
fn a_question_with_no_file_is_not_recorded_and_never_an_empty_answer() {
    let root = recorded();
    let native = Native::from_recorded_answers(root.path());
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

#[tokio::test]
async fn live_questions_need_no_supervisor_and_start_no_process() {
    let root = recorded();
    let native = Native::from_recorded_answers(root.path());
    let mut game = native.start_game().await.unwrap();
    let items = game.registry_items("common/traditions").await.unwrap();
    assert_eq!(items.value, ["tr_example_adopt", "tr_example_finish"]);
    assert_eq!(items.completeness, Completeness::Complete);
    assert_eq!(items.source.basis, Basis::Recorded);
    // A hand-written failure case comes back as the same error.
    assert!(matches!(
        game.registry_items("common/tradition_categories").await,
        Err(Error::Observation { .. })
    ));
    assert!(matches!(
        game.registry_items("common/armies").await,
        Err(Error::NotRecorded { .. })
    ));
    let report = game.close().await.unwrap();
    assert_eq!(report.disposal, OperationDisposal::NotLaunched);
    assert!(matches!(
        game.registry_items("common/traditions").await,
        Err(Error::Closed)
    ));
}
