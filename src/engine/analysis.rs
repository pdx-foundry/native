//! Bounded static methods. Each reads the executable and needs no game process.
pub mod decode;
pub mod directories;
pub mod discovery;
pub mod fields;

use crate::UnavailableReason;

/// Why a static operation could not produce complete decoded instructions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalysisError {
    /// Admission or current executable integrity failed.
    Unavailable {
        /// Independent admission gaps.
        reasons: Vec<UnavailableReason>,
    },
    /// The selected range is ambiguous, unmapped, truncated, or differs from the control.
    InvalidRange,
}

impl std::fmt::Display for AnalysisError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "static analysis failed: {self:?}")
    }
}
impl std::error::Error for AnalysisError {}
