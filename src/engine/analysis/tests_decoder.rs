//! The ARM64 decoder.
use crate::engine::analysis::analysis_support::*;
use crate::engine::analysis::decode::decode_arm64;

#[test]
fn decoder_requires_a_complete_aligned_range() {
    for (bytes, address) in [
        (vec![0; 3], 0),
        (sample_arm64_code(), 1),
        (sample_arm64_code(), u64::MAX - 3),
        (vec![255; 4], 0),
    ] {
        assert!(decode_arm64(&bytes, address).is_err());
    }
}

#[test]
fn decoder_gives_no_instructions_for_an_empty_range() {
    assert_eq!(decode_arm64(&[], 0x1000), Ok(Vec::new()));
}

#[test]
fn decoder_normalizes_operands_and_keeps_every_instruction() {
    let decoded = decode_arm64(&sample_arm64_code(), 0x1000).unwrap();
    let text: Vec<_> = decoded
        .iter()
        .map(|row| (row.operation.as_str(), row.operands.as_str()))
        .collect();
    assert_eq!(text, sample_arm64_disassembly());
    assert_eq!(decoded[3].address, 0x100c);
    assert_eq!(decoded[3].bytes, sample_arm64_code()[12..16]);
}

/// `count` distinct words, each `mov w1, #index`.
fn numbered_words(count: u32) -> Vec<u8> {
    (0..count)
        .flat_map(|index| (0x5280_0001 | index << 5).to_le_bytes())
        .collect()
}

#[test]
fn decoder_reads_a_long_range_as_it_reads_each_word() {
    // One word past the first part, a range that ends inside the second part, two whole parts.
    for count in [1025, 1100, 2048] {
        let bytes = numbered_words(count);
        let separately: Vec<_> = bytes
            .chunks(4)
            .enumerate()
            .map(|(index, word)| {
                decode_arm64(word, 0x1000 + index as u64 * 4)
                    .unwrap()
                    .remove(0)
            })
            .collect();
        assert_eq!(decode_arm64(&bytes, 0x1000).unwrap(), separately);
    }
}

#[test]
fn decoder_refuses_a_long_range_with_an_unknown_word_in_a_later_part() {
    let mut bytes = numbered_words(1100);
    bytes[4200..4204].copy_from_slice(&[255; 4]);
    assert!(decode_arm64(&bytes, 0x1000).is_err());
}
