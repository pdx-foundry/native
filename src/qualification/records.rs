use super::AcceptedRecord;

// Promotion is a reviewed source change. Historical SDK-483 acceptance does not qualify this
// Rust implementation; capture files and callers cannot add records to this authority.
pub(super) const ACCEPTED: &[AcceptedRecord] = &[];
pub(super) const WITHDRAWN: &[&str] = &[];
