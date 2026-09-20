//! A standard API to ask Stellaris questions, the same on each platform and game build.
//!
//! [`Native`] pins an installation and answers static questions from the executable.
//! [`Game`] is a supervised game session that answers live questions. Every answer is an
//! [`Answer`] with a completeness statement, typed gaps and a source stamp.
#![deny(unsafe_code)]
#![warn(missing_docs)]

// Shared test support names this crate as `pdx_native` from unit and integration tests.
extern crate self as pdx_native;

mod answer;
mod api;
mod binding;
mod execution;
mod protocol;
mod qualification;
mod recorded;
mod session;

pub mod supervisor;

#[cfg(feature = "maintainer-tools")]
pub mod investigation;

#[cfg(all(feature = "production", feature = "maintainer-tools"))]
compile_error!("production cannot include maintainer-tools");

pub use answer::Support;
pub use answer::{
    Answer, Basis, BuildId, Completeness, Error, Field, Gap, GapKind, Operation, Reader, ReaderId,
    ReaderKind, Registry, Source,
};
pub use api::{OpenError, OpenRequest};
pub use session::Native;

mod capture;
mod operation;
mod registry;

pub use evidence::registry::GameReadiness;
pub use operation::{OperationDisposal, OperationOutcome};
mod game;
pub use game::{Game, GameError, GameOptions, GameReport};

mod engine;

// Crate-internal names for the earlier types, until the work order removes them.
#[allow(unused_imports)]
pub(crate) use internals::legacy::*;

/// Static method internals for Native's own integration tests. Not a consumer API.
#[doc(hidden)]
pub mod internals {
    pub use crate::engine::analysis::{decode, discovery, fields};

    /// The earlier capability, replay and result types. Native's own live harness and replay
    /// tests still use them; the simplification work order removes them with the evidence package.
    pub mod legacy {
        pub use crate::api::{
            Availability, CapabilityBounds, CapabilityReport, CapabilityRequest, ContextIdentity,
            ContextOrigin, Engine, Qualification, RegistryBounds, ReplayRequest, UnavailableReason,
        };
        pub use crate::engine::analysis::AnalysisError;
        pub use crate::game::RegistryAvailability;
        pub use crate::registry::RegistryError;
        pub use evidence::registry::{RegistryEntry, RegistryProvenance, RegistryResult};
        pub use evidence::{
            Activation, ArtifactReference, CaptureOrigin, Completion, Disposal, EvidenceReference,
            Gap as ObservationGap, Observation, ObservationFact, ObservationResult, ReplayError,
            ReplayResult, ResultOrigin, SubjectHandle,
        };
    }
}
