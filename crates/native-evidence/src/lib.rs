//! Recorded data and read-only derivation, with no dependency on the live Native runtime.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod records;
pub mod replay;
pub mod store;
mod stream;

pub use records::{
    Activation, ArtifactReference, CaptureOrigin, Completion, Disposal, EvidenceReference, Gap,
    Observation, ObservationFact, ReplayError, ReplayResult, ResultOrigin, SubjectHandle,
};
