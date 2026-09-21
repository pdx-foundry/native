//! The messages of one game session between the caller and its supervisor: the request, the
//! controls that follow it, and the supervisor's final report.
//!
//! [`ObservationControl`] holds the deliberate faults that Native's live tests inject. A request
//! carries a fault only together with the registry that receives it.
use crate::{answer::Disposal, supervisor::SupervisorError};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// What the caller asks its supervisor to do: start one game and hold it at its pause.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionRequest {
    /// The installation that the caller opened. The supervisor opens it again.
    pub installation: PathBuf,
    /// Identity of the game build that the caller opened. The supervisor must find the same.
    pub build: String,
    /// A new directory for the private game profile and the session's files.
    pub work_directory: PathBuf,
    /// Seconds allowed for the game to reach its pause, 1 to 180.
    pub startup_seconds: u64,
    /// Seconds that the paused game may stay idle, 1 to 180.
    pub idle_seconds: u64,
    /// A deliberate fault, for Native's live tests.
    pub fault: Option<Fault>,
    /// Consumer fixture, mounted before launch.
    pub fixture: Option<crate::FixtureRequest>,
    /// A fault restricted to fixture observations.
    pub fixture_fault: Option<ObservationControl>,
}

/// A deliberate fault and the internal name of the registry that receives it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Fault {
    pub registry: String,
    pub control: ObservationControl,
}

impl SessionRequest {
    pub(crate) fn validate(&self) -> Result<(), SupervisorError> {
        if let Some(fixture) = &self.fixture {
            fixture
                .validate()
                .map_err(|error| SupervisorError(error.to_string()))?;
        }
        if self.fixture_fault.is_some()
            && (self.fixture.is_none()
                || self.fault.is_some()
                || self.fixture_fault == Some(ObservationControl::Normal))
        {
            return Err(SupervisorError(
                "A fixture fault requires a fixture and no registry fault".into(),
            ));
        }
        if !self.work_directory.is_absolute()
            || !(1..=180).contains(&self.startup_seconds)
            || !(1..=180).contains(&self.idle_seconds)
        {
            return Err(SupervisorError(
                "Expected an absolute work directory and budgets of 1 to 180 seconds".into(),
            ));
        }
        if self
            .fault
            .as_ref()
            .is_some_and(|fault| fault.control == ObservationControl::Normal)
        {
            return Err(SupervisorError(
                "Expected a fault together with the registry that receives it".into(),
            ));
        }
        Ok(())
    }
}

/// What the caller sends after the request.
#[derive(Serialize, Deserialize)]
pub(crate) enum Control {
    /// End the session because the caller cancelled.
    Cancel,
    /// End the session in order.
    Close,
    /// The caller answered a question about this registry, so the idle time starts again.
    ReadRegistry { name: String, request: u64 },
    /// The caller read its prepared fixture observation.
    ReadFixture { request: u64 },
}

/// Why a session ended. This says nothing about disposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum SessionOutcome {
    /// The caller closed the session.
    Completed,
    /// The caller cancelled.
    Cancelled,
    /// The startup or idle time elapsed.
    TimedOut,
    /// The control channel closed or became invalid.
    CallerLost,
    /// The observation worker exited before the supervisor stopped it.
    WorkerLost,
    /// Launch, isolation, integrity, or storage failed.
    Failed(String),
}

/// The supervisor's final report on one session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionReport {
    /// Identity of this session in the reservation journal and the worker's stream.
    pub attempt: String,
    pub outcome: SessionOutcome,
    /// Whether the supervisor reaped the game. Independent of the outcome.
    pub disposal: Disposal,
    /// Whether the disposal is committed to the reservation journal.
    pub reservation_resolved: bool,
    /// Failures that did not decide the outcome.
    pub diagnostics: Vec<String>,
}

/// A deliberate fault in the observation of one registry, for Native's live tests.
///
/// Reach it through `GameOptions::fault`. The supervisor applies a fault only to the registry
/// that the request names; the other registries are observed as usual.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ObservationControl {
    /// No fault.
    #[default]
    Normal,
    /// Do not set the registry's hook.
    MissingHook,
    /// Set the registry's hook, but leave it disabled when the game resumes.
    LateHook,
    /// Do not write the record of the registry's first item.
    DroppedRecord,
    /// Do not write the registry's terminal record.
    MissingTerminal,
    /// Make a read of game memory fail when the registry's loader returns.
    AccessFailure,
    /// Stop the debugger worker while it reads the registry.
    WorkerLoss,
}

impl ObservationControl {
    /// The name that the worker knows this fault by.
    #[cfg_attr(
        not(all(target_os = "macos", target_arch = "aarch64")),
        allow(dead_code)
    )]
    pub(crate) fn wire_name(self) -> String {
        serde_json::to_value(self)
            .ok()
            .and_then(|value| value.as_str().map(String::from))
            .expect("a fault serializes as its name")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> SessionRequest {
        SessionRequest {
            installation: "/absent".into(),
            build: "test".into(),
            work_directory: std::env::temp_dir().join("unused-native-test"),
            startup_seconds: 180,
            idle_seconds: 180,
            fault: None,
            fixture: None,
            fixture_fault: None,
        }
    }

    #[test]
    fn a_request_needs_an_absolute_work_directory_and_valid_budgets() {
        assert!(request().validate().is_ok());
        let mut relative = request();
        relative.work_directory = "relative".into();
        assert!(relative.validate().is_err());
        for seconds in [0, 181] {
            let mut startup = request();
            startup.startup_seconds = seconds;
            assert!(startup.validate().is_err());
            let mut idle = request();
            idle.idle_seconds = seconds;
            assert!(idle.validate().is_err());
        }
    }

    #[test]
    fn a_fault_and_its_registry_come_together_or_not_at_all() {
        let with = |control| {
            let mut request = request();
            request.fault = Some(Fault {
                registry: "traditions".into(),
                control,
            });
            request
        };
        assert!(with(ObservationControl::Normal).validate().is_err());
        for control in [
            ObservationControl::MissingHook,
            ObservationControl::LateHook,
            ObservationControl::DroppedRecord,
            ObservationControl::MissingTerminal,
            ObservationControl::AccessFailure,
            ObservationControl::WorkerLoss,
        ] {
            assert!(with(control).validate().is_ok());
        }
        // A request with a session field that this build does not know is refused.
        let mut unknown = serde_json::to_value(request()).unwrap();
        unknown["registry"] = "traditions".into();
        assert!(serde_json::from_value::<SessionRequest>(unknown).is_err());
    }
}
