//! Why a live registry question has no answer.
use serde::Serialize;

/// Why a registry question cannot be answered.
#[derive(Debug, Clone, Serialize)]
pub enum RegistryError {
    /// The selected target declares no registry with this name.
    Unsupported {
        /// Requested name.
        registry: String,
    },
    /// Pinned installation inputs are no longer available.
    Unavailable {
        /// Independent integrity failures.
        reasons: Vec<crate::UnavailableReason>,
    },
    /// This session did not establish an available snapshot for the registry.
    ObservationUnavailable {
        /// Requested name.
        registry: String,
        /// Evidence or admission limits.
        diagnostics: Vec<String>,
    },
    /// The session is closing or closed; retained results remain in its final report.
    Closed,
    /// The supervisor connection failed; disposal is not inferred.
    Supervisor(String),
}
impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Registry unavailable: {self:?}")
    }
}
impl std::error::Error for RegistryError {}
