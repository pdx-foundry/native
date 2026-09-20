//! The request that a caller sends to its supervisor, the controls that follow it, and the
//! supervisor's final report.
//!
//! [`ObservationControl`] holds the deliberate faults that Native's live tests inject. A request
//! carries a fault only when the caller names the registry that receives it; every other request
//! is `Normal`.
use crate::supervisor::SupervisorError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The origin label on every owner report and live capture.
pub(crate) const ORIGIN: &str = "qualified-live";

/// A bounded suspended-launch experiment, not an admitted operation.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptRequest {
    /// Exact installation to bind; no target override or nearest-version fallback.
    pub installation_hint: PathBuf,
    /// New directory for retained attempt records and a private profile.
    pub output: PathBuf,
    /// Time to hold the child suspended, from 1 through 30,000 milliseconds.
    /// Reaching 30,000 reports timeout rather than successful completion.
    pub hold_ms: u64,
}

/// Why execution ended. This says nothing about disposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationOutcome {
    /// Execution ended normally; consult observation completion separately.
    Completed,
    /// The controller explicitly cancelled.
    Cancelled,
    /// The owner deadline elapsed.
    TimedOut,
    /// The controller channel closed or became invalid.
    CallerLost,
    /// The observation worker exited unexpectedly or its loss was reported.
    WorkerLost,
    /// Launch, isolation, integrity, or storage failed.
    Failed(String),
}

/// Independent owner disposal evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationDisposal {
    /// No game was created.
    NotLaunched,
    /// The owner reaped its direct child.
    Reaped,
    /// Disposal could not be confirmed; the reservation remains blocking.
    Unconfirmed(String),
}

/// The supervisor's final report on one attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptReport {
    /// Always [`ORIGIN`]; the caller checks it.
    pub origin: String,
    /// Unique attempt identity, including failed attempts.
    pub attempt: String,
    /// Bound composition identity.
    pub composition: String,
    /// Independent operation outcome.
    pub outcome: OperationOutcome,
    /// Independent disposal evidence.
    pub disposal: OperationDisposal,
    /// Whether disposal was durably committed to the reservation journal.
    pub reservation_resolved: bool,
    /// Directory retained for inspection; Native never deletes it automatically.
    pub output: PathBuf,
    /// Failures retaining the report; disposal facts above remain independent.
    pub diagnostics: Vec<String>,
    /// Hash-pinned descriptor beneath `output/evidence`, available after capture finalization.
    #[serde(default)]
    pub replay: Option<crate::ArtifactReference>,
    /// Independent session registry descriptors, relative to the attempt directory.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub registries: std::collections::BTreeMap<String, crate::ArtifactReference>,
}

/// Versioned owner report, with an optional separate recorded-evidence descriptor.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptCapture {
    /// Owner report version; this is not the observation replay schema.
    pub version: u32,
    /// Exact linked Native source/build identity.
    pub build: String,
    /// Owner-produced lifecycle report.
    pub report: AttemptReport,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanRequest {
    pub request: AttemptRequest,
    pub composition: String,
    #[serde(default)]
    pub observation: Option<ObservationSpec>,
}
#[derive(Serialize, Deserialize)]
pub(crate) enum Control {
    Cancel,
    Close,
    ReadRegistry {
        name: String,
        request: u64,
    },
    /// Only the supervisor's own tests send this; a release build cannot decode it.
    #[cfg(test)]
    WorkerLost,
}

/// Private prepared request, pinned before starting the supervisor.
/// This is not an execution permit; the supervisor validates the request again.
pub struct PreparedPlan {
    pub(crate) request: PlanRequest,
}

pub(crate) fn validate_request(request: &AttemptRequest) -> Result<(), SupervisorError> {
    if !(1..=30_000).contains(&request.hold_ms) || !request.output.is_absolute() {
        return Err(SupervisorError(
            "Expected absolute output path and hold_ms in 1..=30000".into(),
        ));
    }
    Ok(())
}

/// A deliberate fault in the observation of one registry, for Native's live tests.
///
/// Reach it through `GameOptions::fault`. The supervisor applies a fault only to the registry
/// that the request names; the other registries are observed as usual.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ObservationControl {
    /// Capture the normal bounded window.
    #[default]
    Normal,
    /// Omit the field hook and refuse resume.
    MissingHook,
    /// Disable the field hook at the activation gate and refuse resume.
    LateHook,
    /// Omit one emitted field record without changing producer counts.
    DroppedRecord,
    /// Omit the observation terminal.
    MissingTerminal,
    /// Exercise a failed native memory read at the fixture boundary.
    AccessFailure,
    /// Kill LLDB while stopped after the registration window.
    WorkerLoss,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ObservationSpec {
    #[serde(default)]
    pub registry: Option<String>,
    pub fixture: String,
    pub deadline_seconds: u64,
    pub control: ObservationControl,
    #[serde(default)]
    pub session: Option<SessionSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionSpec {
    pub idle_seconds: u64,
    pub control_registry: Option<String>,
}

impl ObservationSpec {
    pub(crate) fn validate(&self) -> Result<(), SupervisorError> {
        if (self.session.is_none()
            && self.registry.is_none()
            && self.fixture != crate::capture::FIXTURE_BODY)
            || ((self.registry.is_some() || self.session.is_some()) && !self.fixture.is_empty())
            || self.session.as_ref().is_some_and(|session| {
                !(1..=180).contains(&session.idle_seconds) || self.registry.is_some()
            })
            || !(1..=180).contains(&self.deadline_seconds)
        {
            return Err(SupervisorError(
                "Expected the retained category fixture and a 1..=180 second deadline".into(),
            ));
        }
        Ok(())
    }
}

impl PlanRequest {
    /// Every request is a game session. A fault and the registry that receives it come together
    /// or not at all.
    pub(crate) fn validate(&self) -> Result<(), SupervisorError> {
        let Some(spec) = &self.observation else {
            return Err(SupervisorError("Expected a game session request".into()));
        };
        let Some(session) = &spec.session else {
            return Err(SupervisorError("Expected a game session request".into()));
        };
        if (spec.control == ObservationControl::Normal) != session.control_registry.is_none() {
            return Err(SupervisorError(
                "Expected a fault together with the registry that receives it".into(),
            ));
        }
        validate_request(&self.request)?;
        spec.validate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn plan() -> PlanRequest {
        PlanRequest {
            request: AttemptRequest {
                installation_hint: "/absent".into(),
                output: std::env::temp_dir().join("unused-native-test"),
                hold_ms: 1,
            },
            composition: "test".into(),
            observation: Some(ObservationSpec {
                registry: None,
                fixture: String::new(),
                deadline_seconds: 180,
                control: ObservationControl::Normal,
                session: Some(SessionSpec {
                    idle_seconds: 180,
                    control_registry: None,
                }),
            }),
        }
    }
    #[test]
    fn ordinary_wire_requires_a_session_and_valid_deadlines() {
        assert!(plan().validate().is_ok());
        let mut request = plan();
        request.observation = None;
        assert!(request.validate().is_err());
        for deadline in [0, 181] {
            let mut request = plan();
            request.observation.as_mut().unwrap().deadline_seconds = deadline;
            assert!(request.validate().is_err());
        }
        let mut legacy = plan();
        legacy.observation.as_mut().unwrap().session = None;
        legacy.observation.as_mut().unwrap().registry = Some("traditions".into());
        assert!(legacy.validate().is_err());
        for deadline in [0, 181] {
            let mut request = plan();
            request
                .observation
                .as_mut()
                .unwrap()
                .session
                .as_mut()
                .unwrap()
                .idle_seconds = deadline;
            assert!(request.validate().is_err());
        }
        let mut request = plan();
        request.observation.as_mut().unwrap().fixture = "other".into();
        assert!(request.validate().is_err());
    }
    #[test]
    fn a_fault_and_its_registry_come_together_or_not_at_all() {
        let mut unnamed = plan();
        unnamed.observation.as_mut().unwrap().control = ObservationControl::MissingHook;
        assert!(unnamed.validate().is_err());
        let mut no_fault = plan();
        let session = no_fault.observation.as_mut().unwrap().session.as_mut();
        session.unwrap().control_registry = Some("traditions".into());
        assert!(no_fault.validate().is_err());
        for control in [
            ObservationControl::MissingHook,
            ObservationControl::LateHook,
            ObservationControl::DroppedRecord,
            ObservationControl::MissingTerminal,
            ObservationControl::AccessFailure,
            ObservationControl::WorkerLoss,
        ] {
            let mut request = plan();
            let spec = request.observation.as_mut().unwrap();
            spec.control = control;
            spec.session.as_mut().unwrap().control_registry = Some("traditions".into());
            assert!(request.validate().is_ok());
        }
    }
}
