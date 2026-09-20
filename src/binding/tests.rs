use super::{Binding, Source, installation::Installation};
use crate::qualification::AdmissionInputs;
use crate::{CapabilityRequest, ContextOrigin, UnavailableReason};
use std::fs;
use tempfile::{TempDir, tempdir};

fn installation() -> (TempDir, Binding) {
    let directory = tempdir().unwrap();
    for relative in ["common/tradition_categories", "common/traditions"] {
        fs::create_dir_all(directory.path().join(relative)).unwrap();
    }
    fs::write(directory.path().join("launcher-settings.json"), "{}").unwrap();
    fs::write(directory.path().join("stellaris"), "authored test bytes").unwrap();
    fs::write(directory.path().join("common/traditions/test.txt"), "test").unwrap();
    let (installation, _) = Installation::open(directory.path()).unwrap();
    // Private I/O control, never a catalogue entry or accepted qualification. Public contexts
    // cannot supply these bytes to the composer, and the test factory cannot accept this path.
    let binding = Binding {
        inputs: AdmissionInputs {
            composition: "private-io-control".into(),
            bounds: crate::RegistryBounds {
                registries: vec!["traditions".into()],
            },
            content: installation.content.clone(),
            prerequisites: Vec::new(),
            toolchain: Ok("test-toolchain".into()),
        },
        operation: None,
        analysis: None,
        source: Source::Installation(installation),
    };
    (directory, binding)
}

#[test]
fn executable_replacement_permanently_invalidates_the_context() {
    let (directory, binding) = installation();
    let context = crate::Native::from_binding(binding);
    let request = CapabilityRequest::default();
    assert_eq!(context.origin(), ContextOrigin::Installation);
    let report = context.capability(&request);
    assert!(!report.reasons.contains(&UnavailableReason::TargetChanged));
    fs::write(directory.path().join("stellaris"), "changed executable").unwrap();
    assert!(
        context
            .capability(&request)
            .reasons
            .contains(&UnavailableReason::TargetChanged)
    );
    fs::write(directory.path().join("stellaris"), "authored test bytes").unwrap();
    assert!(
        context
            .capability(&request)
            .reasons
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
        let report = context.capability(&CapabilityRequest::default());
        assert_eq!(report.availability, crate::Availability::Unavailable);
        assert!(
            report.reasons.contains(&UnavailableReason::ContentChanged),
            "{mutation}"
        );
    }
}

#[test]
fn missing_inputs_never_become_empty_success() {
    let (directory, binding) = installation();
    let context = crate::Native::from_binding(binding);
    fs::remove_file(directory.path().join("stellaris")).unwrap();
    let report = context.capability(&CapabilityRequest::default());
    assert!(
        report
            .reasons
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
            .capability(&CapabilityRequest::default())
            .reasons
            .contains(&UnavailableReason::InputUnavailable)
    );
}

#[test]
fn shared_execution_consumes_the_resolved_recipe_and_strategy() {
    use super::{ExecutionPlan, compose, platform::ObservationSetup};
    use crate::operation::{ObservationControl, ObservationSpec};
    use crate::supervisor::SupervisorError;
    fn inspect(
        setup: ObservationSetup<'_>,
    ) -> Result<(super::Observer, crate::capture::Capture), SupervisorError> {
        assert_eq!(setup.bindings["registration-entry"], 0x1234);
        assert_eq!(setup.machine.architecture, "synthetic-machine");
        assert_eq!(setup.registry.unwrap().load_entry, 0x5678);
        assert_eq!(setup.package["selected.txt"], b"selected package");
        assert_eq!(setup.identity["architecture"], "synthetic-machine");
        assert_eq!(setup.identity["bindings"]["registration-entry"], 0x1234);
        assert_eq!(setup.identity["strategy"], "synthetic-strategy");
        Err(SupervisorError("selected strategy reached".into()))
    }
    let (directory, _) = installation();
    let (installed, _) = Installation::open(directory.path()).unwrap();
    let content = installed.content.clone().unwrap();
    let (inputs, mut operation) = compose::synthetic_variation(content.clone());
    operation.content = content;
    operation
        .registries
        .get_mut("traditions")
        .unwrap()
        .load_entry = 0x5678;
    operation.machine.architecture = "synthetic-machine".into();
    operation.strategy.revision = "synthetic-strategy";
    operation.strategy.package = [("selected.txt".into(), b"selected package".to_vec())].into();
    operation.strategy.prepare = inspect;
    let plan = ExecutionPlan {
        binding: Binding {
            inputs,
            operation: Some(operation),
            analysis: None,
            source: Source::Installation(installed),
        },
        admitted_tool: None,
        session_unavailable: Default::default(),
    };
    let spec = ObservationSpec {
        registry: Some("traditions".into()),
        fixture: String::new(),
        deadline_seconds: 1,
        control: ObservationControl::Normal,
        session: None,
    };
    let error = plan
        .observer(&directory.path().join("unused"), "test", &spec, "synthetic")
        .err()
        .unwrap();
    assert_eq!(error.to_string(), "selected strategy reached");
    assert!(!directory.path().join("unused").exists());
}
