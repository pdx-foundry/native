//! Bounded registry discovery from recorded executable inputs and historical startup traces.
mod ownership;
mod records;
mod replay;
mod scheduler;

pub use records::*;
pub use replay::{derive, replay};
pub use scheduler::{candidates, scheduler};

/// Discovery derivation revision, independent of live registry enumeration.
pub const METHOD: &str = "registry-discovery/v1";
/// Retained input contract.
pub const FORMAT: &str = "pdx-native-registry-discovery/v1";
