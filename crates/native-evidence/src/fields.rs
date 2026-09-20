//! Root-token dispatch derivation from executable bytes, without content or config inputs.
mod control_flow;
mod dispatch;
mod inventory;
mod records;
mod replay;
mod tokens;
pub use records::*;
pub use replay::{derive, replay};

/// Bounded root-token and reader-routing method revision.
pub const METHOD: &str = "registry-fields/v1";
/// Recorded executable-input format.
pub const FORMAT: &str = "pdx-native-registry-fields/v1";

/// Resolve a candidate handle against immutable discovery inputs, never caller-edited output rows.
pub fn selection(
    discovery: &crate::discovery::RegistryDiscoveryResult,
    subject: &crate::discovery::RegistrySubject,
) -> Result<crate::discovery::CandidateRecord, crate::discovery::ForeignRegistrySubject> {
    use crate::discovery::{ForeignRegistrySubject, StaticInput, candidates};
    discovery.subject(subject)?;
    let input: StaticInput =
        serde_json::from_slice(discovery.input_bytes()).map_err(|_| ForeignRegistrySubject)?;
    candidates(&input.symbols)
        .get(subject.ordinal)
        .cloned()
        .ok_or(ForeignRegistrySubject)
}
