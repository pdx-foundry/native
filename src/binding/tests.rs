use super::{Binding, installation::Installation};
use crate::UnavailableReason;
use std::fs;
use tempfile::{TempDir, tempdir};

/// An authored installation. Its executable is in no catalogue, so the binding has no operation;
/// these tests concern only the integrity of the pinned inputs.
fn installation() -> (TempDir, Binding) {
    let directory = tempdir().unwrap();
    for relative in ["common/tradition_categories", "common/traditions"] {
        fs::create_dir_all(directory.path().join(relative)).unwrap();
    }
    fs::write(directory.path().join("launcher-settings.json"), "{}").unwrap();
    fs::write(directory.path().join("stellaris"), "authored test bytes").unwrap();
    fs::write(directory.path().join("common/traditions/test.txt"), "test").unwrap();
    let (installation, _) = Installation::open(directory.path()).unwrap();
    let binding = Binding {
        operation: None,
        analysis: None,
        installation,
    };
    (directory, binding)
}

#[test]
fn executable_replacement_permanently_invalidates_the_context() {
    let (directory, binding) = installation();
    let context = crate::Native::from_binding(binding);
    assert_eq!(context.blocking_reasons(), []);
    fs::write(directory.path().join("stellaris"), "changed executable").unwrap();
    assert!(
        context
            .blocking_reasons()
            .contains(&UnavailableReason::TargetChanged)
    );
    fs::write(directory.path().join("stellaris"), "authored test bytes").unwrap();
    assert!(
        context
            .blocking_reasons()
            .contains(&UnavailableReason::TargetChanged)
    );
}

#[test]
fn content_additions_deletions_and_edits_invalidate_the_bound_snapshot() {
    for mutation in ["add", "binary-extension", "delete", "edit"] {
        let (directory, binding) = installation();
        let context = crate::Native::from_binding(binding);
        let original = directory.path().join("common/traditions/test.txt");
        match mutation {
            "add" => fs::write(directory.path().join("common/traditions/new.txt"), "new").unwrap(),
            "binary-extension" => {
                fs::write(directory.path().join("common/traditions/new.bin"), "new").unwrap()
            }
            "delete" => fs::remove_file(original).unwrap(),
            _ => fs::write(original, "changed").unwrap(),
        }
        assert!(
            context
                .blocking_reasons()
                .contains(&UnavailableReason::ContentChanged),
            "{mutation}"
        );
    }
}

#[test]
fn missing_inputs_never_become_empty_success() {
    let (directory, binding) = installation();
    let context = crate::Native::from_binding(binding);
    fs::remove_file(directory.path().join("stellaris")).unwrap();
    assert!(
        context
            .blocking_reasons()
            .contains(&UnavailableReason::InputUnavailable)
    );
}

#[cfg(unix)]
#[test]
fn retargeting_the_original_executable_hint_is_detected() {
    use std::os::unix::fs::symlink;
    let (directory, _) = installation();
    let hint = directory.path().join("hint");
    symlink(directory.path().join("stellaris"), &hint).unwrap();
    let (bound, _) = Installation::open(&hint).unwrap();
    assert_eq!(bound.integrity(), None);
    let replacement = directory.path().join("replacement");
    fs::write(&replacement, "authored test bytes").unwrap();
    fs::remove_file(&hint).unwrap();
    symlink(&replacement, &hint).unwrap();
    assert_eq!(bound.integrity(), Some(UnavailableReason::TargetChanged));
}

#[cfg(unix)]
#[test]
fn content_parent_symlinks_are_unavailable() {
    use std::os::unix::fs::symlink;
    let (directory, binding) = installation();
    let context = crate::Native::from_binding(binding);
    fs::rename(
        directory.path().join("common"),
        directory.path().join("moved-common"),
    )
    .unwrap();
    symlink(
        directory.path().join("moved-common"),
        directory.path().join("common"),
    )
    .unwrap();
    assert!(
        context
            .blocking_reasons()
            .contains(&UnavailableReason::InputUnavailable)
    );
}

#[test]
fn shared_execution_consumes_the_resolved_recipe_and_strategy() {
    use super::{ExecutionPlan, compose, platform::ObservationSetup};
    use crate::protocol::session::{Fault, ObservationControl, SessionRequest};
    use crate::supervisor::SupervisorError;
    fn inspect(setup: ObservationSetup<'_>) -> Result<super::Observer, SupervisorError> {
        assert_eq!(setup.machine.architecture, "synthetic-machine");
        assert_eq!(setup.registries["traditions"].load_entry, 0x5678);
        assert_eq!(setup.package["selected.txt"], b"selected package");
        assert_eq!(setup.fault.unwrap().registry, "traditions");
        assert_eq!(setup.startup_seconds, 7);
        Err(SupervisorError("selected strategy reached".into()))
    }
    let (directory, _) = installation();
    let (installed, _) = Installation::open(directory.path()).unwrap();
    let mut operation = compose::synthetic_variation();
    operation
        .registries
        .get_mut("traditions")
        .unwrap()
        .load_entry = 0x5678;
    operation.machine.architecture = "synthetic-machine".into();
    operation.strategy.package = [("selected.txt".into(), b"selected package".to_vec())].into();
    operation.strategy.prepare = inspect;
    let plan = ExecutionPlan {
        binding: Binding {
            operation: Some(operation),
            analysis: None,
            installation: installed,
        },
    };
    let mut request = SessionRequest {
        installation: directory.path().into(),
        build: "unused".into(),
        work_directory: directory.path().join("unused"),
        startup_seconds: 7,
        idle_seconds: 1,
        fault: Some(Fault {
            registry: "traditions".into(),
            control: ObservationControl::MissingHook,
        }),
    };
    let error = plan
        .observer(&request.work_directory, "test", &request)
        .err()
        .unwrap();
    assert_eq!(error.to_string(), "selected strategy reached");
    assert!(!directory.path().join("unused").exists());
    // A fault for a registry that the session does not observe never reaches the strategy.
    request.fault.as_mut().unwrap().registry = "unknown".into();
    let error = plan
        .observer(&request.work_directory, "test", &request)
        .err()
        .unwrap();
    assert_ne!(error.to_string(), "selected strategy reached");
}
