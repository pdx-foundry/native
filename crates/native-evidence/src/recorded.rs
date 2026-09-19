//! Serializable retained records shared by capture and offline replay.
//! These describe evidence bytes, never executable plans or qualification authority.
#![allow(missing_docs)]
use crate::{ArtifactReference, CaptureOrigin};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub const FORMAT: &str = "pdx-native/sdk-483-replay-v1";
pub const CONTRACT: &str = "pdx-native/early-read-entries-v1";

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Descriptor {
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
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub probe_hashes: BTreeMap<String, String>,
    pub fixture_hashes: BTreeMap<String, String>,
    pub producer_content_manifest_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordedRequest {
    pub observations: Vec<String>,
    pub fixtures: BTreeMap<String, String>,
    pub deadline_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TraceRecord {
    pub seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread: Option<u64>,
    pub run: String,
    #[serde(flatten)]
    pub event: TraceEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Hook {
    pub enabled: bool,
    pub locations: u64,
    pub resolved: u64,
    pub hits: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Frame {
    pub function: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum TraceEvent {
    RegistryUnavailable {
        name: String,
        reason: String,
    },
    RegistryLoadReturned {
        name: String,
        owner: String,
    },
    SessionPaused {
        returned: Vec<String>,
    },
    RegistryLoadStart {
        name: String,
        owner: String,
        directory: String,
    },
    RegistrySnapshot {
        name: String,
        owner: String,
        directory: String,
        count: u64,
    },
    RegistryEntry {
        name: String,
        owner: String,
        index: u64,
        object: String,
        key: String,
    },
    RegistryEnd {
        name: String,
        owner: String,
        count: u64,
        #[serde(rename = "producerLastSequence")]
        producer_last_sequence: u64,
    },
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

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum OwnerEvent {
    GamePauseConfirmed {
        pid: u64,
        returned: Vec<String>,
    },
    ObservationUnavailable {
        reason: String,
    },
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
        #[schemars(required)]
        remaining_identity: Option<String>,
    },
}

// A missing witness must not deserialize as the explicit null that confirms process absence.
fn required_nullable_identity<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    Option::<String>::deserialize(deserializer)
}
