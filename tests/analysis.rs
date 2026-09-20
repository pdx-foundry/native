mod analysis_support;
use analysis_support::*;

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
