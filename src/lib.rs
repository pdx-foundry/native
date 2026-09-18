//! Engine-level access to retained Native observations. Replay never opens a live context.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod api;

pub use api::{Engine, ReplayRequest};
pub use evidence::{
    Activation, ArtifactReference, CaptureOrigin, Completion, Disposal, EvidenceReference, Gap,
    Observation, ObservationFact, ReplayError, ReplayResult, ResultOrigin, SubjectHandle,
};
