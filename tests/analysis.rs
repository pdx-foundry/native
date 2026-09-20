mod analysis_support;
use analysis_support::*;
use pdx_native::{AnalysisOrigin, CaptureOrigin, Engine, ReplayRequest};

#[test]
fn public_replay_decodes_authored_arm64_on_every_host() {
    let root = tempfile::tempdir().unwrap();
    let descriptor = descriptor();
    std::fs::write(root.path().join("control.bin"), code()).unwrap();
    let bytes = serde_json::to_vec(&descriptor).unwrap();
    std::fs::write(root.path().join("descriptor.json"), &bytes).unwrap();
    let result = Engine
        .replay_analysis(ReplayRequest {
            artifact_root: root.path().into(),
            descriptor: reference("descriptor.json", &bytes),
        })
        .unwrap();
    assert_eq!(result.origin, AnalysisOrigin::Replay);
    assert_eq!(result.descriptor.capture_origin, CaptureOrigin::Synthetic);
    assert_eq!(result.descriptor, descriptor);
    for (index, (instruction, (operation, operands))) in
        result.instructions.iter().zip(expected()).enumerate()
    {
        assert_eq!(instruction.address, 0x1000 + index as u64 * 4);
        assert_eq!(instruction.operation, operation);
        assert_eq!(instruction.operands, operands);
        assert_eq!(instruction.bytes, code()[index * 4..index * 4 + 4]);
    }
    assert_eq!(result.instructions.len(), expected().len());
    assert!(
        result
            .descriptor
            .provenance
            .qualification_records
            .is_empty()
    );
}

#[test]
fn replay_rejects_corruption_missing_bytes_and_unsupported_revisions() {
    for mutation in [
        "descriptor",
        "bytes",
        "missing",
        "method",
        "decoder",
        "format",
        "address",
        "identity",
        "oversized",
        "path",
        "invalid-instruction",
        "truncated",
    ] {
        let root = tempfile::tempdir().unwrap();
        let mut descriptor = descriptor();
        let mut raw = code();
        match mutation {
            "method" => descriptor.provenance.method = "future".into(),
            "decoder" => descriptor.provenance.decoder = "future".into(),
            "format" => descriptor.format = "future".into(),
            "address" => descriptor.address = u64::MAX,
            "identity" => descriptor.provenance.slice = "invalid".into(),
            "oversized" => descriptor.code.bytes = 4097,
            "path" => descriptor.code.path = "../escape.bin".into(),
            "invalid-instruction" => {
                raw[20..24].copy_from_slice(&[0xff; 4]);
                descriptor.code = reference("control.bin", &raw);
            }
            "truncated" => {
                raw.pop();
                descriptor.code = reference("control.bin", &raw);
            }
            "bytes" => raw[0] ^= 1,
            _ => {}
        }
        if mutation != "missing" {
            std::fs::write(root.path().join("control.bin"), raw).unwrap();
        }
        let mut bytes = serde_json::to_vec(&descriptor).unwrap();
        let descriptor = reference("descriptor.json", &bytes);
        if mutation == "descriptor" {
            bytes[0] ^= 1;
        }
        std::fs::write(root.path().join("descriptor.json"), bytes).unwrap();
        assert!(
            Engine
                .replay_analysis(ReplayRequest {
                    artifact_root: root.path().into(),
                    descriptor
                })
                .is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn decoder_requires_a_complete_aligned_bounded_range() {
    for (bytes, address) in [
        (vec![], 0),
        (vec![0; 3], 0),
        (code(), 1),
        (code(), u64::MAX - 3),
        (vec![0; 4100], 0),
        (vec![255; 4], 0),
    ] {
        assert!(pdx_native::internals::decode::decode_arm64(&bytes, address).is_err());
    }
}
