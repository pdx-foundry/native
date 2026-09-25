//! Bounded static methods. Each reads the executable and needs no game process.
pub mod callbacks;
pub mod declarations;
pub mod decode;
pub mod defines;
pub mod directories;
pub mod evaluate;
pub mod families;
// The startup scheduler remains a bounded test method while shared-template candidates are used
// by the current static API.
#[allow(dead_code)]
pub mod discovery;
pub mod fields;
pub mod localization;
pub mod modifier_table;
pub mod modifiers;
pub mod readers;
pub mod scopes;

#[cfg(test)]
#[path = "analysis/analysis_support.rs"]
pub(crate) mod analysis_support;
#[cfg(test)]
#[path = "analysis/assembler.rs"]
pub(crate) mod assembler;
#[cfg(test)]
#[path = "analysis/tests_decoder.rs"]
mod tests_decoder;
#[cfg(test)]
#[path = "analysis/tests_discovery.rs"]
mod tests_discovery;
#[cfg(test)]
#[path = "analysis/tests_fields.rs"]
mod tests_fields;

use crate::UnavailableReason;

/// Why a static method gave no result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AnalysisError {
    /// The build has no recipe for the method, or the executable changed or cannot be read.
    Unavailable {
        /// Each independent reason.
        reasons: Vec<UnavailableReason>,
    },
    /// The selected range is ambiguous, unmapped, or truncated.
    InvalidRange,
    /// The method refused its input.
    Input(InputError),
}

impl std::fmt::Display for AnalysisError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "static analysis failed: {self:?}")
    }
}
impl std::error::Error for AnalysisError {}

/// A static method refused its input: the input is outside the method's bounds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputError(pub String);

impl std::fmt::Display for InputError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}
impl std::error::Error for InputError {}
impl From<InputError> for AnalysisError {
    fn from(error: InputError) -> Self {
        Self::Input(error)
    }
}
