use std::path::PathBuf;

use evidence::{ArtifactReference, ReplayError, ReplayResult, store::ArtifactStore};
use serde::{Deserialize, Serialize};

/// Location and immutable identity of one retained bounded attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayRequest {
    /// Relocatable directory containing the descriptor and its archive-relative artifacts.
    pub artifact_root: PathBuf,
    /// Expected descriptor bytes, including its SHA-256 identity.
    pub descriptor: ArtifactReference,
}

/// Consumer interface for retained replay and installation capability reporting.
#[derive(Debug, Default)]
pub struct Engine;

impl Engine {
    /// Recompute registry candidates and historical ownership from verified retained artifacts.
    pub fn replay_registry_discovery(
        &self,
        request: ReplayRequest,
    ) -> Result<crate::RegistryDiscoveryResult, ReplayError> {
        evidence::discovery::replay(
            &ArtifactStore::new(request.artifact_root),
            &request.descriptor,
        )
    }

    /// Verify and decode retained instructions without opening an installation or launching a game.
    pub fn replay_analysis(
        &self,
        request: ReplayRequest,
    ) -> Result<crate::AnalysisResult, ReplayError> {
        evidence::analysis::replay(
            &ArtifactStore::new(request.artifact_root),
            &request.descriptor,
        )
    }

    /// Verify and replay a retained registry snapshot without an installed game.
    pub fn replay_registry(
        &self,
        request: ReplayRequest,
    ) -> Result<crate::RegistryResult, ReplayError> {
        evidence::registry::replay(
            &ArtifactStore::new(request.artifact_root),
            &request.descriptor,
        )
    }

    /// Bind an exact installation without launching a game. No version or adapter fallback is used.
    pub fn open(request: OpenRequest) -> Result<crate::EngineContext, OpenError> {
        crate::session::open(request)
    }

    /// Verify and derive one historical attempt without installation discovery or live execution.
    /// Incomplete attempts return their retained observations and gaps. Inaccessible or incompatible
    /// evidence returns an error rather than an empty successful result.
    pub fn replay(&self, request: ReplayRequest) -> Result<ReplayResult, ReplayError> {
        evidence::replay::replay(
            &ArtifactStore::new(request.artifact_root),
            &request.descriptor,
        )
    }
}

/// Location of an executable, application bundle, or installation directory to identify.
#[derive(Debug, Clone)]
pub struct OpenRequest {
    /// Native searches only recognized executable locations beneath this hint.
    pub installation_hint: PathBuf,
}

/// Why an installation could not be bound. No process has been created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenError {
    /// The hint or a recognized executable is absent.
    Missing(PathBuf),
    /// A required path cannot be read or resolved.
    Unreadable(PathBuf),
    /// The executable image is malformed or truncated.
    MalformedExecutable,
    /// More than one executable, eligible slice, or exact catalogue entry matched.
    Ambiguous,
    /// The format or architecture is outside Native's declared target scope.
    UnsupportedTarget,
    /// The image has a supported shape, but no exact registered identity.
    UnknownTarget,
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "installation identification failed: {self:?}")
    }
}

impl std::error::Error for OpenError {}

/// Whether a context describes installed inputs or a fixed synthetic test scenario.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ContextOrigin {
    /// Read from an actual installation; does not imply a live capture or qualification.
    Installation,
    /// Authored test inputs; never evidence of native behavior.
    Synthetic,
}

/// Opaque identity of the fixed target composition; retain or compare it without parsing it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContextIdentity(pub(crate) String);

/// Operation whose qualification and current availability should be inspected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityRequest {
    /// Live observations for one exact registry name.
    Registry {
        /// Stable Native registry name, currently `traditions` or `tradition_categories`.
        registry: String,
    },
    /// The single qualified, game-free instruction decode control.
    StaticDecode,
    /// Bounded static registry candidates and startup scheduling links.
    RegistryDiscovery,
}

impl Default for CapabilityRequest {
    fn default() -> Self {
        Self::Registry {
            registry: "traditions".into(),
        }
    }
}

/// Declared or qualified bounds for one operation family.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum CapabilityBounds {
    /// Named registry observations.
    Registry(RegistryBounds),
    /// Exactly the recipe's bounded decode control, not arbitrary executable ranges.
    StaticDecode,
    /// Bounded static registry candidates and startup scheduling links.
    RegistryDiscovery,
}

/// Registry names declared by a method or covered by one acceptance record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryBounds {
    /// Exact public names; declarations do not imply current admission.
    pub registries: Vec<String>,
}

/// Accepted support for this request, independent of present implementation availability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Qualification {
    /// A bundled accepted record matches all identities, content dependencies, and bounds.
    Qualified,
    /// The request exceeds the declared or accepted scope.
    OutsideSupport,
    /// No applicable, unwithdrawn acceptance establishes the request.
    Incomplete,
}

/// Whether this context can admit the request with current inputs and prerequisites.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Availability {
    /// Admission succeeds; a synthetic context still cannot perform live execution.
    Available,
    /// Admission failed for the independently reported reasons.
    Unavailable,
}

/// A structured admission gap. Multiple independent gaps can occur together.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum UnavailableReason {
    /// Admitted live execution requires the explicit production Cargo feature.
    ProductionFeatureRequired,
    /// The compiled host cannot use the selected live strategy.
    HostUnavailable,
    /// The selected live strategy has no implementation in this release.
    ImplementationUnavailable,
    /// The selected debugger/tool identity differs from the accepted qualification.
    HelperMismatch,
    /// Qualification for the exact operation composition is absent.
    QualificationMissing,
    /// An otherwise matching acceptance has been withdrawn.
    QualificationWithdrawn,
    /// Accepted target, recipe, strategy, method, or binding revisions do not match.
    RevisionMismatch,
    /// Current content does not match the accepted dependency identities.
    ContentMismatch,
    /// The request is invalid or exceeds the supported operation bounds.
    OutsideBounds,
    /// The executable changed after binding; this context must be reopened.
    TargetChanged,
    /// Relevant installed content changed after binding; this context must be reopened.
    ContentChanged,
    /// A current input cannot be checked.
    InputUnavailable,
    /// A required runtime prerequisite is not established.
    PrerequisiteMissing,
}

/// Independent qualification and availability results for one operation request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityReport {
    /// Operation-specific composition identity, opaque to consumers.
    pub context: ContextIdentity,
    /// Source of this context's inputs.
    pub origin: ContextOrigin,
    /// Accepted support for the requested scope.
    pub qualification: Qualification,
    /// Whether this request currently passes admission.
    pub availability: Availability,
    /// Declared operation bounds; qualification is required separately.
    pub bounds: CapabilityBounds,
    /// Bounds of applicable accepted records, kept separate so their union grants no support.
    pub accepted_bounds: Vec<CapabilityBounds>,
    /// All established reasons that block admission.
    pub reasons: Vec<UnavailableReason>,
    /// Accepted qualification record identities supporting this result.
    pub qualification_records: Vec<String>,
    /// Immutable references reviewed for applicable accepted records; bytes are not loaded here.
    pub evidence: Vec<ArtifactReference>,
}
