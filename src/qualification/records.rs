use super::AcceptedRecord;

// Promotion is a reviewed source change. Historical SDK-483 acceptance does not qualify this
// Rust implementation; capture files and callers cannot add records to this authority.
pub(super) fn accepted() -> Vec<AcceptedRecord> {
    serde_json::from_str(include_str!("records/accepted.json"))
        .expect("reviewed qualification records")
}
pub(super) const WITHDRAWN: &[&str] = &[];
