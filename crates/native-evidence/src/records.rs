use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt};

pub(crate) const FORMAT: &str = "pdx-native/sdk-483-replay-v1";
pub(crate) const CONTRACT: &str = "pdx-native/early-read-entries-v1";

/// Immutable artifact identity and portable storage location. Paths must be archive-relative.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactReference {
    /// Portable path beneath the supplied artifact root.
    pub path: String,
    /// Lowercase SHA-256 of the exact retained bytes.
    pub sha256: String,
    /// Exact byte length.
    pub bytes: u64,
}

/// What originally produced this retained attempt; this is never fresh qualification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CaptureOrigin {
    /// Historical engine capture; replay does not re-establish its native guarantees.
    Captured,
    /// Authored test data, with no claim of engine observation.
    Synthetic,
}

/// How the returned observations were obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResultOrigin {
    /// Derived from verified historical bytes, without a new game run.
    Replay,
}

/// Reference to verified evidence, optionally locating a trace sequence or journal row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceReference {
    /// Exact artifact supporting the fact.
    pub artifact: ArtifactReference,
    /// Producer sequence for a trace record; one-based row for an owner journal record.
    pub record: Option<u64>,
}

/// Opaque subject identity valid only in this retained evidence context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SubjectHandle {
    pub(crate) context: String,
    pub(crate) identity: String,
}

/// One retained observation, distinct from a stored value, validation result, or game rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Observation {
    /// Observed engine-level fact and its processing stage.
    pub fact: ObservationFact,
    /// Exact retained witness.
    pub evidence: EvidenceReference,
}

/// Supported entry observations in the bounded initial window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ObservationFact {
    /// Entry to a registration call; successful return is not established.
    RegistrationEntry {
        /// Engine token observed as the call argument.
        engine_token: u64,
        /// One-based call ordinal within the bounded window.
        ordinal: u64,
    },
    /// Entry to a category field reader; stored value and validation remain unknown.
    CategoryReadEntry {
        /// Fixture-relative source file.
        file: String,
        /// One-based source line.
        line: u64,
        /// Field read at this entry.
        field: String,
        /// One-based field ordinal within the file window.
        ordinal: u64,
        /// Context-bound owner identity; callers may compare it for equality.
        owner: SubjectHandle,
    },
}

/// Whether the retained ordering witnesses establish the requested activation boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Activation {
    /// All required hooks and bounded ordering witnesses are present.
    Demonstrated,
    /// Required hook or phase ordering is not established.
    NotEstablished,
}

/// Completion of observations in this bounded window, independent of cleanup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Completion {
    /// This bounded window completed; it is not complete registry or rule knowledge.
    Complete,
    /// A required observation capability was explicitly unavailable.
    Unavailable,
    /// Retained records do not prove the bounded window completed.
    Incomplete,
    /// Independent ownership evidence records observation-worker loss.
    WorkerLost,
}

/// Independent historical disposal result; replay creates no process to dispose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Disposal {
    /// Journal confirms the same owned child exited and was reaped.
    Confirmed,
    /// Confirmation is missing or inconsistent.
    Unconfirmed,
    /// Journal records no owned child.
    NotApplicable,
}

/// An explicit limit on what readable retained records establish.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Gap {
    /// The game rewrote its profile settings; the pre-launch bytes were not retained.
    OriginalProfileInputUnavailable {
        /// Verified post-run settings artifact, which is not the original input.
        retained: ArtifactReference,
        /// Pre-launch hash recorded in the original producer manifest.
        original_sha256: String,
    },
    /// Producer records are missing, duplicated, or out of order.
    Sequence {
        /// Next required producer sequence.
        expected: u64,
        /// Sequence actually retained.
        found: u64,
    },
    /// Required activation or phase ordering witness is missing or invalid.
    ActivationNotEstablished,
    /// A hook, callback, or engine access failed explicitly.
    Unavailable {
        /// Retained diagnostic, without a fallback observation.
        reason: String,
    },
    /// The producer did not retain an observation terminal.
    MissingTerminal,
    /// Terminal sequence/totals or bounded observation structure disagree.
    WindowIntegrity {
        /// Specific failed relation.
        reason: String,
    },
    /// An owner/source join cannot be established.
    OwnerJoin,
    /// Observation worker died before completion.
    WorkerLost,
    /// Independent owner journal does not confirm disposal.
    DisposalUnconfirmed,
}

/// Retained derivation result, carrying original identities and precise bounds.
#[derive(Debug, Clone, Serialize)]
pub struct ReplayResult {
    /// Retained artifact format used for this derivation.
    pub evidence_format: String,
    /// Native observation-contract identity, distinct from a game version.
    pub contract: String,
    /// Original attempt identity; replay does not mint a new capture.
    pub attempt: String,
    /// Evidence context: immutable descriptor SHA-256, opaque to consumers.
    pub context: String,
    /// Always replay for this operation.
    pub origin: ResultOrigin,
    /// Original captured or synthetic origin.
    pub capture_origin: CaptureOrigin,
    /// Required-hook and bounded phase ordering result.
    pub activation: Activation,
    /// Bounded observation completion result.
    pub completion: Completion,
    /// Separate historical process disposal result.
    pub disposal: Disposal,
    /// Retained facts, including partial observations from failed attempts.
    pub observations: Vec<Observation>,
    /// Missing or inconsistent evidence relations.
    pub gaps: Vec<Gap>,
    /// Explicit observation limits; never a whole-registry or whole-category guarantee.
    pub limits: Vec<String>,
    /// Descriptor, original manifest, and all verified supporting artifacts.
    pub evidence: Vec<EvidenceReference>,
}

/// Failure to access or interpret retained bytes; never a successful empty result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ReplayError {
    /// Required bundle/descriptor/artifact is not available.
    EvidenceUnavailable {
        /// Unavailable locator.
        path: String,
    },
    /// Required artifact could not be read.
    Read {
        /// Failed locator.
        path: String,
        /// Operating-system reason.
        reason: String,
    },
    /// Exact retained byte length differs.
    SizeMismatch {
        /// Damaged locator.
        path: String,
        /// Pinned byte length.
        expected: u64,
        /// Retained byte length.
        found: u64,
    },
    /// Exact retained SHA-256 differs.
    HashMismatch {
        /// Damaged locator.
        path: String,
    },
    /// JSON or JSONL cannot be interpreted under the declared format.
    Malformed {
        /// Artifact with invalid data.
        path: String,
        /// Parser or validation reason.
        reason: String,
    },
    /// Descriptor format is not implemented.
    UnsupportedFormat {
        /// Retained format identity.
        found: String,
    },
    /// Observation contract is incompatible.
    UnsupportedContract {
        /// Retained contract identity.
        found: String,
    },
    /// Locator is absolute, escapes the artifact root, or uses a symbolic link.
    UnsafePath {
        /// Rejected locator.
        path: String,
    },
}

impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "retained evidence replay failed: {self:?}")
    }
}

impl std::error::Error for ReplayError {}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Descriptor {
    pub format: String,
    pub contract: String,
    pub attempt: String,
    pub origin: CaptureOrigin,
    pub manifest: ArtifactReference,
    pub request: ArtifactReference,
    pub trace: ArtifactReference,
    pub owner: ArtifactReference,
    pub supporting: Vec<ArtifactReference>,
}

// These DTOs describe the fixed retained prototype format. Native-only diagnostic fields remain
// in the verified artifacts, rather than becoming consumer-facing native control inputs.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Manifest {
    pub probe_hashes: BTreeMap<String, String>,
    pub fixture_hashes: BTreeMap<String, String>,
    pub producer_content_manifest_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RecordedRequest {
    pub observations: Vec<String>,
    pub fixtures: BTreeMap<String, String>,
    pub deadline_seconds: u64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TraceRecord {
    pub seq: u64,
    pub run: String,
    #[serde(flatten)]
    pub event: TraceEvent,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Hook {
    pub enabled: bool,
    pub locations: u64,
    pub resolved: u64,
    pub hits: u64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Frame {
    pub function: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub(crate) enum TraceEvent {
    HooksRequested,
    LaunchStopped {
        error: String,
        pid: u64,
        triple: String,
        frames: Vec<Frame>,
    },
    HooksActiveBeforeResume {
        hooks: BTreeMap<String, Hook>,
    },
    Resume {
        error: String,
    },
    PhaseReached {
        phase: String,
        #[serde(default)]
        file: Option<String>,
        #[serde(default)]
        stack: Vec<String>,
    },
    RegistrationObserved {
        ordinal: u64,
        #[serde(rename = "engineToken")]
        engine_token: u64,
    },
    RegistrationWindowComplete {
        observed: u64,
    },
    FieldObserved {
        file: String,
        line: u64,
        field: String,
        owner: String,
        ordinal: u64,
    },
    PhaseComplete {
        phase: String,
        file: String,
        #[serde(rename = "producerFieldCount")]
        producer_field_count: u64,
    },
    StreamEnd {
        #[serde(rename = "producerLastSequence")]
        producer_last_sequence: u64,
        #[serde(rename = "producerFieldCount")]
        producer_field_count: u64,
        registrations: u64,
    },
    CapabilityUnavailable {
        reason: String,
    },
    EarlyActivationUnavailable {
        reason: String,
    },
    CallbackError {
        #[serde(default)]
        error: String,
    },
    NativeException {
        #[serde(default)]
        reason: String,
    },
    WorkerLossReady,
    WorkerFinished,
    WorkerDispose,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub(crate) enum OwnerEvent {
    GameOwnedSuspended {
        pid: u64,
        identity: String,
    },
    WorkerStarted,
    WorkerExited {
        returncode: i64,
    },
    WorkerLossInjected,
    WorkerStopRequested,
    OwnerDisposeRequested,
    DisposalChecked {
        confirmed: bool,
        #[serde(rename = "reapedPid")]
        reaped_pid: u64,
        #[serde(rename = "gameExit")]
        game_exit: Option<i64>,
        #[serde(
            rename = "remainingIdentity",
            deserialize_with = "required_nullable_identity"
        )]
        remaining_identity: Option<String>,
    },
}

// A missing witness must not deserialize as the explicit null that confirms process absence.
fn required_nullable_identity<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    Option::<String>::deserialize(deserializer)
}
