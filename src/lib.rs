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
mod engine;
mod execution;
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
pub use game::{Game, GameOptions};
pub use session::Native;

pub(crate) use api::UnavailableReason;
pub(crate) use engine::analysis::AnalysisError;

/// Static method internals and the live fault controls, for Native's own integration tests.
/// Not a consumer API.
#[doc(hidden)]
pub mod internals {
    pub use crate::engine::analysis::{decode, discovery, fields};
    pub use crate::protocol::session::ObservationControl;
}
