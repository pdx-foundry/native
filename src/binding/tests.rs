use super::{Binding, installation::Installation, registry_instance};
use crate::UnavailableReason;
use std::fs;
use tempfile::{TempDir, tempdir};

#[test]
fn initial_loader_requires_one_named_candidate_with_an_address() {
    let candidate = |directory: &str, initial_loader: Option<&str>| super::NamedCandidate {
        record: crate::engine::analysis::discovery::CandidateRecord {
            database: "CTestDatabase".into(),
            owner_candidate: "CTestOwner".into(),
            loader: "loader".into(),
            address: "0x1000".into(),
            initial_loader: initial_loader.map(str::to_owned),
            has_named_member_reader: false,
        },
        directory: crate::engine::analysis::directories::Directory::Named(directory.into()),
    };
    let known = candidate("common/traditions", Some("0x1234"));
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
    assert!(
        super::initial_loader(
            &candidate("common/traditions", Some("invalid")),
            "common/traditions"
        )
        .is_err()
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
    let layout = super::groups::registry_layout(&[
        super::targets::BindingGroupId::M45TemplateRegistryLayout,
    ])
    .unwrap();
    let bypass = super::groups::registry_binding(layout, "common/bypass", 0x1234, Ok(0x18));
    assert_eq!(bypass.key_offset, Some(0x18));
    assert_eq!(bypass.key_unavailable, None);

    let unknown = super::groups::registry_binding(
        layout,
        "common/unknown",
        0x1234,
        Err("item key storage was not established".into()),
    );
    assert_eq!(unknown.key_offset, None);
    assert_eq!(
        unknown.key_unavailable.as_deref(),
        Some("item key storage was not established")
    );
}

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
fn selected_sessions_ignore_unselected_default_content() {
    let (root, binding) = installation();
    let context = crate::Native::from_binding(binding);
    fs::write(root.path().join("common/traditions/test.txt"), "changed").unwrap();
    assert!(
        context
            .blocking_reasons()
            .contains(&UnavailableReason::ContentChanged)
    );
    assert!(context.selected_blocking_reasons().is_empty());

    let (root, binding) = installation();
    fs::remove_dir_all(root.path().join("common/tradition_categories")).unwrap();
    let context = crate::Native::from_binding(binding);
    assert!(
        context
            .blocking_reasons()
            .contains(&UnavailableReason::InputUnavailable)
    );
    assert!(context.selected_blocking_reasons().is_empty());
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

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn private_profile_copies_the_content_pinned_at_open_including_additions_and_edits() {
    let (root, _) = installation();
    let modified = "common/traditions/test.txt";
    let added = "common/traditions/nested/added.txt";
    fs::write(root.path().join(modified), "edited before open").unwrap();
    fs::create_dir(root.path().join("common/traditions/nested")).unwrap();
    fs::write(root.path().join(added), "added before open").unwrap();
    let (installation, _) = Installation::open(root.path()).unwrap();
    let plan = super::ExecutionPlan {
        binding: Binding {
            installation,
            operation: Some(super::compose::synthetic_variation()),
            analysis: None,
        },
    };
    let names = plan.binding.default_registries();
    let registries: std::collections::BTreeMap<_, _> = names
        .iter()
        .map(|name| {
            (
                name.clone(),
                super::groups::registry_binding(
                    plan.operation().registry_layout.unwrap(),
                    name,
                    0x5678,
                    Ok(0x10),
                ),
            )
        })
        .collect();
    let content = plan.installation().content.as_ref().unwrap();
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

    let fixture =
        crate::FixtureRequest::new("common/tradition_categories/atlas.txt", "atlas = {}\n");
    let fixture_work = tempdir().unwrap();
    fs::create_dir(fixture_work.path().join("profile")).unwrap();
    plan.prepare_registry_profile(fixture_work.path(), Some(&fixture), &registries, content)
        .unwrap();
    let fixture_mount = fixture_work.path().join("profile/mod/native_registry");
    assert_eq!(
        fs::read_to_string(fixture_mount.join(fixture.file())).unwrap(),
        "atlas = {}\n"
    );
    assert_eq!(
        fs::read_dir(fixture_mount.join("common/tradition_categories"))
            .unwrap()
            .count(),
        1
    );
    assert_eq!(
        fs::read(fixture_mount.join(modified)).unwrap(),
        fs::read(root.path().join(modified)).unwrap()
    );
    assert!(!root.path().join(fixture.file()).exists());

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
            .join("profile/mod/native_registry")
            .join(fixture.file())
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
        assert_eq!(setup.registries["common/traditions"].load_entry, 0x5678);
        assert_eq!(setup.package["selected.txt"], b"selected package");
        assert_eq!(setup.fault.unwrap().registry, "common/traditions");
        assert_eq!(setup.startup_seconds, 7);
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
        registries: vec!["common/traditions".into()],
        fixture: None,
        fixture_fault: None,
        loaded_modifiers: None,
        modifier_fault: None,
        fault: Some(Fault {
            registry: "common/traditions".into(),
            control: ObservationControl::MissingHook,
        }),
    };
    let error = plan
        .observer(&request.work_directory, "test", &request, &registries)
        .err()
        .unwrap();
    assert_eq!(error.to_string(), "selected strategy reached");
    assert!(!directory.path().join("unused").exists());
    // A fault for a registry that the session does not observe never reaches the strategy.
    request.fault.as_mut().unwrap().registry = "unknown".into();
    let error = plan
        .observer(&request.work_directory, "test", &request, &registries)
        .err()
        .unwrap();
    assert_ne!(error.to_string(), "selected strategy reached");
}
