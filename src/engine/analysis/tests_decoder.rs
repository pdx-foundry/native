//! The bounded ARM64 decoder.
use crate::engine::analysis::analysis_support::*;
use crate::engine::analysis::decode::decode_arm64;

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
        assert!(decode_arm64(&bytes, address).is_err());
    }
}

#[test]
fn decoder_normalizes_operands_and_keeps_every_instruction() {
    let decoded = decode_arm64(&code(), 0x1000).unwrap();
    let text: Vec<_> = decoded
        .iter()
        .map(|row| (row.operation.as_str(), row.operands.as_str()))
        .collect();
    assert_eq!(text, expected());
    assert_eq!(decoded[3].address, 0x100c);
    assert_eq!(decoded[3].bytes, code()[12..16]);
}
