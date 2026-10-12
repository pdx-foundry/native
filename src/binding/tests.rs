use super::{Binding, installation::Installation, registry_instance};
use crate::{UnavailableReason, protocol::observation::KeyStorage};
use std::fs;
use tempfile::{TempDir, tempdir};

#[test]
fn initial_loader_requires_one_named_candidate_with_an_address() {
    let candidate = |directory: &str, initial_loader: Option<u64>| super::NamedCandidate {
        record: crate::engine::analysis::discovery::CandidateRecord {
            database: "CTestDatabase".into(),
            owner_candidate: "CTestOwner".into(),
            loader_class: "loader".into(),
            address: 0x1000,
            initial_loader,
        },
        directory: crate::engine::analysis::directories::Directory::Named(directory.into()),
    };
    let known = candidate("common/traditions", Some(0x1234));
    assert_eq!(
        super::initial_loader(
            super::named_candidate(std::slice::from_ref(&known), "common/traditions").unwrap(),
            "common/traditions"
        ),
        Ok(0x1234)
    );
    assert!(super::named_candidate(&[], "common/traditions").is_err());
    assert!(super::named_candidate(&[known.clone(), known], "common/traditions").is_err());
    assert!(
        super::initial_loader(&candidate("common/traditions", None), "common/traditions").is_err()
    );
}

#[test]
fn a_registry_without_an_instance_gets_no_key_read() {
    assert_eq!(registry_instance(Ok(0x40), Ok(0x10)), (0x40, Ok(0x10)));
    let (instance, key_offset) = registry_instance(Err("no symbol".into()), Ok(0x10));
    assert_eq!(instance, 0);
    assert!(key_offset.unwrap_err().contains("no symbol"));
}

#[test]
fn registry_binding_keeps_the_established_key_offset_or_refusal() {
    let layout = super::groups::M45_TEMPLATE_LAYOUT;
    let bypass = super::groups::registry_binding(layout, "common/bypass", 0x1234, Ok(0x18));
    assert_eq!(bypass.key_storage, KeyStorage::Offset(0x18));

    let unknown = super::groups::registry_binding(
        layout,
        "common/unknown",
        0x1234,
        Err("item key storage was not established".into()),
    );
    assert_eq!(
        unknown.key_storage,
        KeyStorage::Unavailable("item key storage was not established".into())
    );
}

/// A binding of an authored installation: its analysis takes the catalogued declarations.
fn authored_binding(
    installation: Installation,
    operation: Option<super::compose::ResolvedObservation>,
) -> Binding {
    let invalidated = std::sync::Arc::<std::sync::Mutex<Option<UnavailableReason>>>::default();
    let analysis = super::BoundAnalysis::new(
        super::targets::test_declarations(),
        None,
        installation.clone(),
        invalidated.clone(),
    );

    Binding {
        analysis: std::sync::Arc::new(analysis),
        operation,
        installation,
        invalidated,
    }
}

/// An authored installation. Its executable is in no catalogue, so the binding has no operation;
/// these tests concern only the integrity of the pinned inputs.
fn installation() -> (TempDir, Binding) {
    let directory = tempdir().unwrap();
    let registries = ["common/tradition_categories", "common/traditions"];
    for relative in registries {
        fs::create_dir_all(directory.path().join(relative)).unwrap();
    }
    fs::write(directory.path().join("launcher-settings.json"), "{}").unwrap();
    fs::write(directory.path().join("stellaris"), "authored test bytes").unwrap();
    fs::write(directory.path().join("common/traditions/test.txt"), "test").unwrap();
    let (installation, _) = Installation::open(directory.path()).unwrap();
    let binding = authored_binding(installation.with_default_registries(&registries), None);
    (directory, binding)
}

#[test]
fn executable_replacement_permanently_invalidates_the_context() {
    let (directory, binding) = installation();
    let context = crate::Native::from_binding(binding);
    assert_eq!(context.blocking_reasons(context.bound()), []);
    fs::write(directory.path().join("stellaris"), "changed executable").unwrap();
    assert!(
        context
            .blocking_reasons(context.bound())
            .contains(&UnavailableReason::TargetChanged)
    );
    fs::write(directory.path().join("stellaris"), "authored test bytes").unwrap();
    assert!(
        context
            .blocking_reasons(context.bound())
            .contains(&UnavailableReason::TargetChanged)
    );
    let analysis = &context.bound().analysis;
    assert!(analysis.executable().is_err());
}

#[test]
fn a_live_operation_reports_a_changed_executable_as_the_build_changing() {
    use crate::{Native, Operation, Support};
    let (directory, mut binding) = installation();
    binding.operation = Some(super::compose::synthetic_variation());
    let native = Native::from_binding(binding);
    fs::write(directory.path().join("stellaris"), "changed executable").unwrap();

    for operation in [Operation::ObserveFixture, Operation::CheckScript] {
        assert_eq!(
            native.supports(operation),
            Support::Unsupported("the executable changed after it was opened".into()),
            "{operation}"
        );
    }
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
                .blocking_reasons(context.bound())
                .contains(&UnavailableReason::ContentChanged),
            "{mutation}"
        );
    }
}

#[test]
fn selected_sessions_ignore_unselected_default_content() {
    let (root, binding) = installation();
    let context = crate::Native::from_binding(binding);
    fs::write(root.path().join("common/traditions/test.txt"), "changed").unwrap();
    assert!(
        context
            .blocking_reasons(context.bound())
            .contains(&UnavailableReason::ContentChanged)
    );
    assert!(
        context
            .selected_blocking_reasons(context.bound())
            .is_empty()
    );

    let (root, binding) = installation();
    fs::remove_dir_all(root.path().join("common/tradition_categories")).unwrap();
    let context = crate::Native::from_binding(binding);
    assert!(
        context
            .blocking_reasons(context.bound())
            .contains(&UnavailableReason::InputUnavailable)
    );
    assert!(
        context
            .selected_blocking_reasons(context.bound())
            .is_empty()
    );
}

#[test]
fn missing_inputs_never_become_empty_success() {
    let (directory, binding) = installation();
    let context = crate::Native::from_binding(binding);
    fs::remove_file(directory.path().join("stellaris")).unwrap();
    assert!(
        context
            .blocking_reasons(context.bound())
            .contains(&UnavailableReason::InputUnavailable)
    );
}

/// A plan over the authored installation at `root`, and the default registries it observes.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn profile_plan(
    root: &std::path::Path,
) -> (
    super::ExecutionPlan,
    std::collections::BTreeMap<String, crate::protocol::observation::RegistryBinding>,
) {
    let (installation, _) = Installation::open(root).unwrap();
    let operation = super::compose::synthetic_variation();
    let plan = super::ExecutionPlan {
        binding: authored_binding(
            installation.with_default_registries(operation.default_registries),
            Some(operation),
        ),
    };
    let registries = plan
        .binding
        .default_registries()
        .into_iter()
        .map(|name| {
            let binding = super::groups::registry_binding(
                plan.operation().registry_layout.unwrap(),
                &name,
                0x5678,
                Ok(0x10),
            );
            (name, binding)
        })
        .collect();

    (plan, registries)
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn private_profile_refuses_content_that_changes_while_it_is_prepared() {
    let (root, _) = installation();
    let (plan, registries) = profile_plan(root.path());
    let directories: Vec<String> = registries.keys().cloned().collect();
    let content = plan.session_content(&directories).unwrap();
    let work = tempdir().unwrap();
    fs::create_dir(work.path().join("profile")).unwrap();
    plan.prepare_registry_profile(work.path(), None, &registries, &content)
        .unwrap();

    // The copy checks each pinned file, so a file added after the snapshot is not seen there.
    fs::write(root.path().join("common/traditions/late.txt"), "late").unwrap();
    let next = tempdir().unwrap();
    fs::create_dir(next.path().join("profile")).unwrap();
    let error = plan
        .prepare_registry_profile(next.path(), None, &registries, &content)
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "Registry content changed during profile preparation"
    );
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn private_profile_copies_the_content_pinned_at_open_including_additions_and_edits() {
    let (root, _) = installation();
    let modified = "common/traditions/test.txt";
    let added = "common/traditions/nested/added.txt";
    fs::write(root.path().join(modified), "edited before open").unwrap();
    fs::create_dir(root.path().join("common/traditions/nested")).unwrap();
    fs::write(root.path().join(added), "added before open").unwrap();
    let (plan, registries) = profile_plan(root.path());
    let directories: Vec<String> = registries.keys().cloned().collect();
    let content = &plan.session_content(&directories).unwrap();
    let work = tempdir().unwrap();
    fs::create_dir(work.path().join("profile")).unwrap();
    plan.prepare_registry_profile(work.path(), None, &registries, content)
        .unwrap();
    let mount = work.path().join("profile/mod/native_registry");
    for relative in [modified, added] {
        assert_eq!(
            fs::read(mount.join(relative)).unwrap(),
            fs::read(root.path().join(relative)).unwrap()
        );
    }
    assert!(mount.join("common/tradition_categories").is_dir());
    assert!(!mount.join("launcher-settings.json").exists());
    let descriptor =
        fs::read_to_string(work.path().join("profile/mod/native_registry.mod")).unwrap();
    assert!(descriptor.contains("replace_path=\"common/traditions\""));
    assert!(descriptor.contains("replace_path=\"common/tradition_categories\""));

    let fixture = crate::FixtureRequest::field_outcomes(
        "common/tradition_categories/atlas.txt",
        "atlas = {}\n",
        [
            crate::FixtureFieldQuestion::new("common/tradition_categories", "test", "traditions")
                .with_parsing(),
        ],
    )
    .with_file("common/tradition_categories/beta.txt", "beta = {}\n");
    let fixture_work = tempdir().unwrap();
    fs::create_dir(fixture_work.path().join("profile")).unwrap();
    plan.prepare_registry_profile(fixture_work.path(), Some(&fixture), &registries, content)
        .unwrap();
    let fixture_mount = fixture_work.path().join("profile/mod/native_registry");
    for (path, text) in &fixture.files {
        assert_eq!(&fs::read_to_string(fixture_mount.join(path)).unwrap(), text);
        assert!(!root.path().join(path).exists());
    }
    assert_eq!(
        fs::read_dir(fixture_mount.join("common/tradition_categories"))
            .unwrap()
            .count(),
        2
    );
    assert_eq!(
        fs::read(fixture_mount.join(modified)).unwrap(),
        fs::read(root.path().join(modified)).unwrap()
    );

    let selected = std::collections::BTreeMap::from([(
        "common/traditions".into(),
        registries["common/traditions"].clone(),
    )]);
    let selected_content = plan.session_content(&["common/traditions".into()]).unwrap();
    let fixture_only_work = tempdir().unwrap();
    fs::create_dir(fixture_only_work.path().join("profile")).unwrap();
    plan.prepare_registry_profile(
        fixture_only_work.path(),
        Some(&fixture),
        &selected,
        &selected_content,
    )
    .unwrap();
    let fixture_only_mod = fs::read_to_string(
        fixture_only_work
            .path()
            .join("profile/mod/native_registry.mod"),
    )
    .unwrap();
    assert!(fixture_only_mod.contains("replace_path=\"common/tradition_categories\""));
    assert!(fixture_only_mod.contains("replace_path=\"common/traditions\""));
    assert!(
        fixture_only_work
            .path()
            .join("profile/mod/native_registry/common/tradition_categories/beta.txt")
            .is_file()
    );

    fs::write(root.path().join(added), "changed after open").unwrap();
    let next = tempdir().unwrap();
    assert!(
        plan.prepare_registry_profile(next.path(), None, &registries, content)
            .is_err()
    );
    assert!(!next.path().join("profile").exists());
}

#[cfg(unix)]
#[test]
fn retargeting_the_original_executable_hint_is_detected() {
    use std::os::unix::fs::symlink;
    let (directory, _) = installation();
    let hint = directory.path().join("hint");
    symlink(directory.path().join("stellaris"), &hint).unwrap();
    let (bound, _) = Installation::open(&hint).unwrap();
    assert_eq!(bound.target_integrity(), None);
    let replacement = directory.path().join("replacement");
    fs::write(&replacement, "authored test bytes").unwrap();
    fs::remove_file(&hint).unwrap();
    symlink(&replacement, &hint).unwrap();
    assert_eq!(
        bound.target_integrity(),
        Some(UnavailableReason::TargetChanged)
    );
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
            .blocking_reasons(context.bound())
            .contains(&UnavailableReason::InputUnavailable)
    );
}

#[test]
fn a_scope_analysis_that_fails_is_a_reason_and_leaves_the_call_binding() {
    let (directory, _) = installation();
    let (installed, _) = Installation::open(directory.path()).unwrap();
    let binding = authored_binding(installed, Some(super::compose::synthetic_variation()));

    // The authored executable is in no catalogue, so no scope function is found.
    assert!(binding.script_scopes().is_err());
    assert!(binding.script_check_binding().is_some());
}

#[test]
fn shared_execution_consumes_the_resolved_recipe_and_strategy() {
    use super::{ExecutionPlan, compose, platform::ObservationSetup};
    use crate::protocol::session::{Fault, ObservationControl, SessionRequest};
    use crate::supervisor::SupervisorError;
    fn inspect(setup: ObservationSetup<'_>) -> Result<super::Observer, SupervisorError> {
        assert_eq!(setup.machine.architecture, "synthetic-machine");
        assert_eq!(setup.registries["common/traditions"].load_entry, 0x5678);
        assert_eq!(setup.package["selected.txt"], b"selected package");
        assert_eq!(
            setup.fault.unwrap().target,
            crate::protocol::session::ObservationTarget::Registry("common/traditions".into())
        );
        assert_eq!(setup.startup_seconds, 7);
        assert_eq!(
            setup.executable_hash,
            crate::work_directory::sha256(b"authored test bytes")
        );
        Err(SupervisorError("selected strategy reached".into()))
    }
    let (directory, _) = installation();
    let (installed, _) = Installation::open(directory.path()).unwrap();
    let mut operation = compose::synthetic_variation();
    let registry = super::groups::registry_binding(
        operation.registry_layout.unwrap(),
        "common/traditions",
        0x5678,
        Ok(0x10),
    );
    let registries = std::collections::BTreeMap::from([("common/traditions".into(), registry)]);
    operation.machine.architecture = "synthetic-machine".into();
    operation.strategy.package = [("selected.txt".into(), b"selected package".to_vec())].into();
    operation.strategy.prepare = inspect;
    let plan = ExecutionPlan {
        binding: authored_binding(installed, Some(operation)),
    };
    // The strategy gets the hash bound at open and does not read the executable again; the
    // check before spawn and the worker refuse a changed file.
    fs::write(directory.path().join("stellaris"), "changed test bytes!").unwrap();
    let request = SessionRequest {
        installation: directory.path().into(),
        build: "unused".into(),
        work_directory: directory.path().join("unused"),
        startup_seconds: 7,
        idle_seconds: 1,
        registries: vec!["common/traditions".into()],
        fixture: None,
        loaded_modifiers: None,
        fault: Some(Fault {
            target: crate::protocol::session::ObservationTarget::Registry(
                "common/traditions".into(),
            ),
            control: ObservationControl::MissingHook,
        }),
    };
    let error = plan
        .observer(&request.work_directory, "test", &request, None, &registries)
        .err()
        .unwrap();
    assert_eq!(error.to_string(), "selected strategy reached");
    assert!(!directory.path().join("unused").exists());
}
