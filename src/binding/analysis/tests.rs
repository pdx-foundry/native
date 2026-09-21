use super::*;
use std::fs;

#[path = "../../../tests/analysis_support/mod.rs"]
mod support;

fn fixture() -> (tempfile::TempDir, BoundAnalysis) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("image");
    fs::write(&path, support::macho(&support::code())).unwrap();
    let (installation, bytes) = Installation::open(&path).unwrap();
    let image = binary::identify(&bytes).unwrap();
    // These tests read only the executable bytes, so the layout is never used.
    let layout = SchedulerLayout {
        start: 0,
        end: 0,
        offset: 0,
        stride: 48,
        count: 0,
    };
    let analysis = BoundAnalysis::new(image.executable, image.slice, layout, installation);
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
    let candidates = binding
        .analysis
        .as_ref()
        .unwrap()
        .named_candidates()
        .unwrap();
    assert_eq!(candidates.len(), 164);
    assert!(
        candidates
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
        fs::write(path, &bytes).unwrap();
        assert_eq!(native.registries().unwrap_err(), error);
    }
}

#[test]
fn reader_uses_verified_slice_bytes_without_content_or_live_tools() {
    let (root, binding) = fixture();
    assert!(!root.path().join("common").exists());
    let image = support::macho(&support::code());
    assert_eq!(binding.executable().unwrap(), image);
    fs::create_dir(root.path().join("common")).unwrap();
    fs::write(root.path().join("common/arbitrary.txt"), "content changed").unwrap();
    assert_eq!(binding.executable().unwrap(), image);
    let mut binding = binding;
    binding.slice = "e".repeat(64);
    assert!(
        matches!(binding.executable(), Err(AnalysisError::Unavailable { reasons }) if reasons == [UnavailableReason::TargetChanged])
    );
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
