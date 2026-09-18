//! Exact-target admission and retained observations, with optional maintainer lifecycle experiments.
#![deny(unsafe_code)]
#![warn(missing_docs)]

mod api;
mod binding;
mod execution;
mod protocol;
mod qualification;
mod session;

pub mod supervisor;

#[cfg(feature = "maintainer-tools")]
pub mod investigation;

#[cfg(all(
    feature = "production",
    any(feature = "test-support", feature = "maintainer-tools")
))]
compile_error!("production cannot include test-support or maintainer-tools");

#[cfg(feature = "test-support")]
#[doc(hidden)]
pub mod test_support;

pub use api::{
    Availability, CapabilityReport, CapabilityRequest, ContextIdentity, ContextOrigin,
    ObservationBounds, OpenError, OpenRequest, Qualification, UnavailableReason,
};
pub use api::{Engine, ReplayRequest};
pub use evidence::{
    Activation, ArtifactReference, CaptureOrigin, Completion, Disposal, EvidenceReference, Gap,
    Observation, ObservationFact, ReplayError, ReplayResult, ResultOrigin, SubjectHandle,
};
pub use session::EngineContext;
