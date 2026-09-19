use super::*;
use crate::binding::{Binding, compose};
use crate::qualification::analysis::{AnalysisAuthority, AnalysisRecord, evaluate};
use crate::{
    AnalysisDescriptor, AnalysisOrigin, AnalysisProvenance, Availability, CaptureOrigin,
    ContextOrigin, OpenRequest, Qualification,
};
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
        method: evidence::analysis::METHOD,
        decoder: evidence::analysis::DECODER,
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
            evidence::analysis::decode_arm64,
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
fn static_qualification_is_separate_and_fails_closed() {
    let (_, binding) = fixture();
    for (case, reason) in [
        ("missing", UnavailableReason::QualificationMissing),
        ("revision", UnavailableReason::RevisionMismatch),
        ("withdrawn", UnavailableReason::QualificationWithdrawn),
        ("changed", UnavailableReason::TargetChanged),
    ] {
        let mut authority = AnalysisAuthority {
            accepted: vec![AnalysisRecord {
                id: "synthetic".into(),
                composition: binding.inputs.composition.clone(),
                evidence: vec![],
            }],
            withdrawn: vec![],
        };
        match case {
            "missing" => authority.accepted.clear(),
            "revision" => authority.accepted[0].composition = "changed".into(),
            "withdrawn" => authority.withdrawn.push("synthetic".into()),
            _ => {}
        }
        let integrity = (case == "changed").then_some(UnavailableReason::TargetChanged);
        let report = evaluate(
            &binding.inputs,
            &authority,
            ContextOrigin::Synthetic,
            integrity,
        );
        assert_eq!(report.qualification, Qualification::Incomplete);
        assert_eq!(report.availability, Availability::Unavailable);
        assert_eq!(report.reasons, [reason]);
    }
    let authority = AnalysisAuthority {
        accepted: vec![AnalysisRecord {
            id: "synthetic".into(),
            composition: binding.inputs.composition.clone(),
            evidence: vec![],
        }],
        withdrawn: vec![],
    };
    assert_eq!(
        evaluate(&binding.inputs, &authority, ContextOrigin::Synthetic, None).availability,
        Availability::Available
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
    assert!(native.analysis().is_err()); // Synthetic bytes cannot use production qualification.
}

#[test]
#[ignore = "requires the pinned M45 executable and a new private output directory"]
fn qualify_retained_decode() {
    let path =
        std::env::var_os("PDX_NATIVE_ANALYSIS_EXECUTABLE").expect("exact M45 executable required");
    let output = std::path::PathBuf::from(
        std::env::var_os("PDX_NATIVE_ANALYSIS_OUTPUT").expect("private candidate output required"),
    );
    fs::create_dir(&output).expect("use a new output directory");
    let binding = Binding::open(OpenRequest {
        installation_hint: path.into(),
    })
    .unwrap();
    let analysis = binding.analysis.unwrap();
    let raw = analysis.read().unwrap();
    let instructions = (analysis.decoder)(&raw, analysis.control.address).unwrap();
    let expected: Vec<[String; 2]> = serde_json::from_str(include_str!(
        "../../../tests/fixtures/analysis/planet-getter.expected.json"
    ))
    .unwrap();
    assert_eq!(instructions.len(), expected.len());
    for (instruction, [operation, operands]) in instructions.iter().zip(expected) {
        assert_eq!(instruction.operation, operation);
        assert_eq!(instruction.operands, operands);
    }
    let inputs = &analysis.inputs;
    let descriptor = AnalysisDescriptor {
        format: evidence::analysis::FORMAT.into(),
        capture_origin: CaptureOrigin::Captured,
        address: analysis.control.address,
        code: analysis.control.code.clone(),
        provenance: AnalysisProvenance {
            executable: inputs.executable.clone(),
            slice: inputs.slice.clone(),
            composition: inputs.composition.clone(),
            implementation: inputs.implementation.clone(),
            method: inputs.method.into(),
            decoder: inputs.decoder.into(),
            qualification_records: vec![],
            evidence: vec![],
        },
    };
    let code_path = output.join(&descriptor.code.path);
    fs::create_dir_all(code_path.parent().unwrap()).unwrap();
    fs::write(code_path, raw).unwrap();
    let bytes = serde_json::to_vec_pretty(&descriptor).unwrap();
    fs::write(output.join("descriptor.json"), &bytes).unwrap();
    let reference = support::reference("descriptor.json", &bytes);
    let replay = crate::Engine
        .replay_analysis(crate::ReplayRequest {
            artifact_root: output.clone(),
            descriptor: reference.clone(),
        })
        .unwrap();
    assert_eq!(replay.origin, AnalysisOrigin::Replay);
    assert_eq!(replay.instructions, instructions);
    let result_bytes = serde_json::to_vec_pretty(&replay).unwrap();
    fs::write(output.join("result.json"), &result_bytes).unwrap();
    let report = serde_json::json!({
        "status": "candidate-verified", "composition": inputs.composition,
        "implementation": inputs.implementation, "descriptor": reference,
        "result": support::reference("result.json", &result_bytes),
        "normalization_control": support::reference("tests/fixtures/analysis/planet-getter.expected.json", include_bytes!("../../../tests/fixtures/analysis/planet-getter.expected.json")),
        "instruction_count": instructions.len(), "decode_matches_historical_control": true,
        "replay_matches": true, "game_launches": 0,
        "historical_bundle": "70fce0ce8dae5dbb473937b08ef77e711218e34a1e2d6406519ad3b574dbfe79",
        "historical_control": "typed-extraction/reference-observation-prototype/evidence/planet-getter.txt",
    });
    fs::write(
        output.join("qualification.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}

#[test]
fn bundled_static_acceptance_matches_the_portable_composition() {
    // Authored file supplies only a locator. This checks composition portability, not native bytes.
    let (root, _) = fixture();
    let (installation, _) = Installation::open(&root.path().join("image")).unwrap();
    let bound = compose::analysis(&super::super::targets::test_identity(), installation).unwrap();
    let report = evaluate(
        &bound.inputs,
        &AnalysisAuthority::bundled(),
        ContextOrigin::Synthetic,
        None,
    );
    assert_eq!(
        report.availability,
        Availability::Available,
        "static sources changed: requalify the retained control before publishing: {report:?}"
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
