use super::*;
use std::fs;

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn fixture_bindings_follow_reader_arguments_and_owner_symbols() {
    let native = crate::Native::open(std::env::var_os("STELLARIS_PATH").unwrap()).unwrap();
    let analysis = native.bound().analysis.as_ref().unwrap();
    let fields = analysis.fixture_string_fields("common/traditions").unwrap();
    let found: Vec<_> = fields
        .iter()
        .map(|field| (field.name.as_str(), field.token, field.storage_offset))
        .collect();
    assert_eq!(
        found,
        [
            ("custom_tooltip", 10001, 0x1c0),
            ("custom_tooltip_with_modifiers", 11046, 0x1e8),
            ("unlocks_agenda", 14639, 0x5a0),
        ]
    );
    assert_eq!(
        analysis.fixture_loader("common/traditions").unwrap(),
        Some(FixtureLoader {
            load_entry: 0x100ce090c,
            reader_entry: 0x100ce1bec,
            reader_return: 0x100ce097c,
            constructor_entry: 0x100cd9a20,
            member_entry: 0x100cda028,
        })
    );
    assert_eq!(
        analysis.fixture_loader("common/relics").unwrap(),
        Some(FixtureLoader {
            load_entry: 0x100ae3298,
            reader_entry: 0x100ae5664,
            reader_return: 0x100ae3308,
            constructor_entry: 0x100ae14dc,
            member_entry: 0x100ae1754,
        })
    );
    let relic_fields = analysis.fixture_string_fields("common/relics").unwrap();
    assert!(
        relic_fields
            .iter()
            .any(|field| field.name == "portrait" && field.storage_offset == 728)
    );
}

use crate::engine::analysis::analysis_support as support;

fn fixture() -> (tempfile::TempDir, BoundAnalysis) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("image");
    fs::write(&path, support::macho(&support::code())).unwrap();
    let (installation, _) = Installation::open(&path).unwrap();
    // These tests read only the executable bytes, so the layout is never used.
    let layout = SchedulerLayout {
        start: 0,
        end: 0,
        offset: 0,
        stride: 48,
        count: 0,
    };
    let analysis = BoundAnalysis::new(layout, None, installation);
    (root, analysis)
}

#[test]
fn static_support_checks_the_pinned_executable_without_requiring_content() {
    use crate::{Native, Operation, Support};
    for mutation in ["changed", "missing"] {
        let (root, analysis) = fixture();
        let binding = crate::binding::Binding {
            installation: analysis.installation.clone(),
            analysis: Some(std::sync::Arc::new(analysis)),
            operation: None,
        };
        let native = Native::from_binding(binding);
        for operation in [Operation::Registries, Operation::RegistryFields] {
            assert_eq!(native.supports(operation), Support::Supported);
        }
        let path = root.path().join("image");
        let original = fs::read(&path).unwrap();
        if mutation == "changed" {
            fs::write(&path, "changed").unwrap();
        } else {
            fs::remove_file(&path).unwrap();
        }
        for operation in [Operation::Registries, Operation::RegistryFields] {
            assert!(matches!(
                native.supports(operation),
                Support::Unsupported(_)
            ));
        }
        fs::write(path, original).unwrap();
        for operation in [Operation::Registries, Operation::RegistryFields] {
            assert!(matches!(
                native.supports(operation),
                Support::Unsupported(_)
            ));
        }
    }
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn every_m45_named_candidate_has_one_initial_loader_entry() {
    let installation =
        std::env::var_os("STELLARIS_PATH").expect("STELLARIS_PATH names the installation");
    let binding = crate::binding::Binding::open(std::path::Path::new(&installation)).unwrap();
    let candidates = binding.analysis.as_ref().unwrap().verified().unwrap();
    assert_eq!(candidates.named_candidates().len(), 164);
    assert!(
        candidates
            .named_candidates()
            .iter()
            .all(|candidate| candidate.record.initial_loader.is_some())
    );
    let known = binding
        .registry_bindings(&binding.default_registries())
        .unwrap();
    assert_eq!(known["common/traditions"].load_entry, 0x100ce0474);
    assert_eq!(known["common/tradition_categories"].load_entry, 0x100cd7d70);
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn repeated_public_and_binding_queries_agree() {
    use crate::Native;
    let installation =
        std::env::var_os("STELLARIS_PATH").expect("STELLARIS_PATH names the installation");
    let native = Native::open(installation).unwrap();
    let first = native.registries().unwrap();
    assert_eq!(native.registries().unwrap(), first);
    for registry in ["common/traditions", "common/tradition_categories"] {
        let fields = native.registry_fields(registry).unwrap();
        assert_eq!(native.registry_fields(registry).unwrap(), fields);
        let fixture_fields = native
            .bound()
            .analysis
            .as_ref()
            .unwrap()
            .registry_fields(registry)
            .unwrap()
            .unwrap();
        assert_eq!(fixture_fields, fields.value);
    }
    let selected = [
        "common/traditions".into(),
        "common/tradition_categories".into(),
    ];
    let first = native.bound().registry_bindings(&selected).unwrap();
    let second = native.bound().registry_bindings(&selected).unwrap();
    assert_eq!(
        serde_json::to_value(first).unwrap(),
        serde_json::to_value(second).unwrap()
    );
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn cached_static_answers_refuse_changed_or_missing_executables() {
    use crate::{Error, Native};
    let installed =
        std::env::var_os("STELLARIS_PATH").expect("STELLARIS_PATH names the installation");
    let (_, bytes) = Installation::open(std::path::Path::new(&installed)).unwrap();
    for mutation in ["changed", "missing"] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("image");
        fs::write(&path, &bytes).unwrap();
        let native = Native::open(&path).unwrap();
        assert!(!native.registries().unwrap().value.is_empty());
        assert!(matches!(
            native.registry_fields("common/no_such_registry"),
            Err(Error::UnknownRegistry { .. })
        ));
        if mutation == "changed" {
            fs::write(&path, "changed").unwrap();
        } else {
            fs::remove_file(&path).unwrap();
        }
        let error = native.registries().unwrap_err();
        match mutation {
            "changed" => assert_eq!(error, Error::BuildChanged),
            _ => assert!(matches!(error, Error::Unsupported { .. })),
        }
        for registry in ["common/traditions", "common/no_such_registry"] {
            assert!(matches!(
                native.registry_fields(registry),
                Err(Error::BuildChanged | Error::Unsupported { .. })
            ));
        }
        assert!(
            native
                .bound()
                .registry_bindings(&["common/traditions".into()])
                .is_err()
        );
        fs::write(path, &bytes).unwrap();
        assert_eq!(native.registries().unwrap_err(), error);
        assert!(
            native
                .bound()
                .registry_bindings(&["common/traditions".into()])
                .is_err()
        );
    }
}

#[test]
fn reader_uses_verified_executable_bytes_without_content_or_live_tools() {
    let (root, binding) = fixture();
    assert!(!root.path().join("common").exists());
    let image = support::macho(&support::code());
    assert_eq!(binding.executable().unwrap(), image);
    fs::create_dir(root.path().join("common")).unwrap();
    fs::write(root.path().join("common/arbitrary.txt"), "content changed").unwrap();
    assert_eq!(binding.executable().unwrap(), image);
}

#[test]
fn a_changed_or_missing_executable_permanently_invalidates_static_reads() {
    for mutation in ["changed", "missing"] {
        let (root, binding) = fixture();
        let path = root.path().join("image");
        let original = fs::read(&path).unwrap();
        if mutation == "changed" {
            fs::write(&path, "changed").unwrap();
        } else {
            fs::remove_file(&path).unwrap();
        }
        let error = binding.executable().unwrap_err();
        let expected = if mutation == "changed" {
            UnavailableReason::TargetChanged
        } else {
            UnavailableReason::InputUnavailable
        };
        assert_eq!(
            error,
            AnalysisError::Unavailable {
                reasons: vec![expected]
            }
        );
        fs::write(path, original).unwrap();
        assert_eq!(binding.executable().unwrap_err(), error);
    }
}

#[cfg(unix)]
#[test]
fn retargeted_executable_permanently_invalidates_static_reads() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let image = support::macho(&support::code());
    let original = root.path().join("original");
    let replacement = root.path().join("replacement");
    let hint = root.path().join("hint");
    fs::write(&original, &image).unwrap();
    fs::write(&replacement, &image).unwrap();
    symlink(&original, &hint).unwrap();
    let (installation, _) = Installation::open(&hint).unwrap();
    let analysis = BoundAnalysis::new(
        SchedulerLayout {
            start: 0,
            end: 0,
            offset: 0,
            stride: 48,
            count: 0,
        },
        None,
        installation,
    );
    assert_eq!(analysis.executable().unwrap(), image);
    fs::remove_file(&hint).unwrap();
    symlink(&replacement, &hint).unwrap();
    let error = analysis.executable().unwrap_err();
    assert_eq!(
        error,
        AnalysisError::Unavailable {
            reasons: vec![UnavailableReason::TargetChanged]
        }
    );
    fs::remove_file(&hint).unwrap();
    symlink(&original, &hint).unwrap();
    assert_eq!(analysis.executable().unwrap_err(), error);
}

#[test]
fn range_reader_refuses_unmapped_truncated_misaligned_and_overlapping_sections() {
    let bytes = support::macho(&support::code());
    for (address, length) in [
        (0xffc, 4),
        (0x1028, 8),
        (0x1000, 0),
        (0x1001, 4),
        (0x1000, 3),
        (u64::MAX - 3, 8),
        (0x1000, 4100),
    ] {
        assert_eq!(
            binary::code_range(&bytes, address, length),
            Err(AnalysisError::InvalidRange)
        );
    }
    let mut truncated = bytes.clone();
    truncated.pop();
    assert_eq!(
        binary::code_range(&truncated, 0x1000, 44),
        Err(AnalysisError::InvalidRange)
    );
    // A second __text section with the same mapped range must not silently choose one.
    let mut overlap = bytes.clone();
    overlap[20..24].copy_from_slice(&232u32.to_le_bytes());
    overlap[36..40].copy_from_slice(&232u32.to_le_bytes());
    overlap[64..72].copy_from_slice(&264u64.to_le_bytes());
    overlap[96..100].copy_from_slice(&2u32.to_le_bytes());
    let mut section = overlap[104..184].to_vec();
    section[48..52].copy_from_slice(&264u32.to_le_bytes());
    overlap[152..156].copy_from_slice(&264u32.to_le_bytes());
    overlap.splice(184..184, section);
    assert_eq!(
        binary::code_range(&overlap, 0x1000, 44),
        Err(AnalysisError::InvalidRange)
    );
}

#[test]
fn range_reader_selects_the_same_arm64_slice_from_a_universal_image() {
    let arm = support::macho(&support::code());
    let mut intel = arm.clone();
    intel[4..8].copy_from_slice(&0x01000007u32.to_le_bytes());
    let offset = 48u32;
    let mut fat: Vec<u8> = [
        0xcafebabeu32,
        2,
        0x01000007,
        0,
        offset,
        intel.len() as u32,
        0,
        0x0100000c,
        0,
        offset + intel.len() as u32,
        arm.len() as u32,
        0,
    ]
    .into_iter()
    .flat_map(u32::to_be_bytes)
    .collect();
    fat.extend(intel);
    fat.extend(arm);
    assert_eq!(
        binary::code_range(&fat, 0x1000, 44).unwrap(),
        support::code()
    );
    fat[8..12].copy_from_slice(&0x0100000cu32.to_be_bytes());
    assert!(binary::code_range(&fat, 0x1000, 44).is_err());
}
