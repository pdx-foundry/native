//! Installation-bound registry descriptions and explicit discovery gaps.
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

/// Explicit discovery status. Unknown never means a successful empty schema.
#[derive(Debug, Clone, Serialize)]
pub enum DiscoveryStatus {
    /// No qualified discovery supplies these facts.
    Unknown {
        /// Qualification or evidence gap.
        reason: String,
    },
}

/// Installation-bound declarations, independent of registered item enumeration.
#[derive(Debug, Clone, Serialize)]
pub struct RegistryDescription {
    /// Public registry name.
    pub name: String,
    /// Target-declared content directory; not a parsed registry inventory.
    pub content_directory: String,
    /// Opaque identity of the pinned installation composition.
    pub context: crate::ContextIdentity,
    /// Installed or synthetic source.
    pub origin: crate::ContextOrigin,
    /// Status of qualified reader discovery.
    pub reader_discovery: DiscoveryStatus,
    /// Status of qualified field discovery.
    pub field_discovery: DiscoveryStatus,
    /// Limits of the declared facts.
    pub limits: Vec<String>,
}
