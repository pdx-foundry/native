use super::*;
use crate::binding::{Binding, compose};
use std::fs;

#[path = "../../../tests/analysis_support/mod.rs"]
mod support;

fn fixture() -> (tempfile::TempDir, BoundAnalysis) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("image");
    fs::write(&path, support::macho(&support::code())).unwrap();
    let (installation, bytes) = Installation::open(&path).unwrap();
    let image = binary::identify(&bytes).unwrap();
    let inputs = AnalysisInputs {
        composition: "c".repeat(64),
        executable: image.executable,
        slice: image.slice,
        implementation: "d".repeat(64),
        method: crate::engine::analysis::decode::METHOD,
        decoder: crate::engine::analysis::decode::DECODER,
    };
    let control = DecodeControl {
        address: 0x1000,
        length: 44,
        code: support::reference("code.bin", &support::code()),
    };
    (
        root,
        BoundAnalysis::new(
            inputs,
            control,
            crate::engine::analysis::decode::decode_arm64,
            installation,
        ),
    )
}

#[test]
fn reader_uses_verified_slice_bytes_without_content_or_live_tools() {
    let (root, binding) = fixture();
    assert!(!root.path().join("common").exists());
    assert_eq!(binding.read().unwrap(), support::code());
    fs::create_dir(root.path().join("common")).unwrap();
    fs::write(root.path().join("common/arbitrary.txt"), "content changed").unwrap();
    assert_eq!(binding.read().unwrap(), support::code());
    let mut binding = binding;
    binding.inputs.slice = "e".repeat(64);
    assert!(
        matches!(binding.read(), Err(AnalysisError::Unavailable { reasons }) if reasons == [UnavailableReason::TargetChanged])
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
        let error = binding.read().unwrap_err();
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
        assert_eq!(binding.read().unwrap_err(), error);
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
fn shared_static_composition_never_probes_the_live_strategy() {
    let (root, analysis) = fixture();
    let image = super::super::targets::test_identity();
    let (inputs, mut operation) =
        compose::compose(&image, Err(UnavailableReason::InputUnavailable)).unwrap();
    operation.strategy.probe = || panic!("static admission called live strategy");
    let (installation, _) = Installation::open(&root.path().join("image")).unwrap();
    let binding = Binding {
        inputs,
        operation: Some(operation),
        analysis: Some(std::sync::Arc::new(analysis)),
        source: super::super::Source::Installation(installation),
        authority: crate::qualification::Authority {
            accepted: vec![],
            withdrawn: vec![],
        },
    };
    let native = crate::Native::from_binding(binding);
    let report = native.capability(&crate::CapabilityRequest::StaticDecode);
    assert!(!report.reasons.iter().any(|reason| matches!(
        reason,
        UnavailableReason::ProductionFeatureRequired
            | UnavailableReason::HostUnavailable
            | UnavailableReason::InputUnavailable
            | UnavailableReason::PrerequisiteMissing
    )));
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
