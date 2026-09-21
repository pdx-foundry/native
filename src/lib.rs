//! A standard API to ask Stellaris questions, the same on each platform and game build.
//!
//! [`Native`] pins an installation and answers static questions from the executable.
//! [`Game`] is a supervised game session that answers live questions. Every answer is an
//! [`Answer`] with a completeness statement, typed gaps and a source stamp.
#![deny(unsafe_code)]
#![warn(missing_docs)]

mod answer;
mod api;
mod binding;
mod engine;
mod execution;
mod fixture;
mod game;
mod protocol;
mod recorded;
mod session;
mod work_directory;

pub mod supervisor;

pub use answer::{
    Answer, Basis, BuildId, Completeness, Disposal, Error, Field, Gap, GapKind, Operation, Reader,
    ReaderId, ReaderKind, Registry, Source, Support,
};
pub use api::OpenError;
pub use engine::operations::registry_items::GameReadiness;
pub use fixture::{
    FieldRead, FixtureObservation, FixtureObservationKind, FixtureOwnerId, FixtureRequest,
    FixtureWindow, ProcessingStage, RegistrationEntry,
};
pub use game::{Game, GameOptions};
pub use session::Native;

pub(crate) use api::UnavailableReason;
pub(crate) use engine::analysis::AnalysisError;

/// Static method internals and the live fault controls, for Native's own integration tests.
/// Not a consumer API.
#[doc(hidden)]
pub mod internals {
    pub use crate::engine::analysis::{decode, discovery, fields, readers, references};
    pub use crate::protocol::session::ObservationControl;

    /// Run the hidden SDK-482 initialization method against a pinned executable.
    pub fn reference_result(
        native: &crate::Native,
        owner: &str,
    ) -> Result<references::ReferenceResult, crate::Error> {
        native.reference_result(owner)
    }

    /// Run the hidden SDK-482 initialization method for several owners from one binary input.
    pub fn reference_results(
        native: &crate::Native,
        owners: &[&str],
    ) -> Result<Vec<references::ReferenceResult>, crate::Error> {
        native.reference_results(owners)
    }
}
