//! Shared bounded operation transport and lifecycle records.
//!
//! Start a separate instance of your executable, pass its private input/output pipes to
//! `connect`, and call `serve` in that instance. See the `investigate` Cargo example.
use crate::{
    protocol::{self, Hello, Reply},
    supervisor::SupervisorError,
};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::PathBuf,
};

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

/// Internal owner report. Only the admitted client can return a public live result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptReport {
    /// Authorization origin established by the supervisor entry point.
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
    pub authorization: Authorization,
    #[serde(default)]
    pub observation: Option<ObservationSpec>,
}
#[derive(Serialize, Deserialize)]
pub(crate) enum Control {
    Cancel,
    #[cfg(any(test, feature = "maintainer-tools"))]
    WorkerLost,
}

/// Controller connection. Dropping its writer requests cleanup; it does not confirm disposal.
pub struct AttemptJob<R, W> {
    input: R,
    output: W,
    finished: Option<AttemptReport>,
}

/// Private prepared request, pinned before starting the supervisor.
/// This is not an execution permit; the owner validates authorization independently.
pub struct PreparedPlan {
    pub(crate) request: PlanRequest,
}

/// Validate and pin candidate inputs before starting the consumer's supervisor process.
/// The owner repeats these checks before allocation; preparation grants no launch permission.
#[cfg(feature = "maintainer-tools")]
pub fn prepare(request: AttemptRequest) -> Result<PreparedPlan, SupervisorError> {
    validate_request(&request)?;
    let plan = crate::binding::ExecutionPlan::open(&request.installation_hint)?;
    Ok(PreparedPlan {
        request: PlanRequest {
            composition: plan.composition().into(),
            authorization: Authorization::Candidate,
            request,
            observation: None,
        },
    })
}

/// Connect a prepared request to a consumer-created supervisor using private pipes.
/// Both processes must link the same Native build. This function does not start a process.
pub fn connect<R: Read, W: Write>(
    mut input: R,
    mut output: W,
    plan: PreparedPlan,
) -> Result<AttemptJob<R, W>, SupervisorError> {
    protocol::write(&mut output, &Hello::current(plan.request.authorization))?;
    match protocol::read(&mut input)? {
        Reply::Ready => {}
        Reply::Rejected(reason) => return Err(SupervisorError(reason)),
        _ => return Err(SupervisorError("Expected supervisor handshake".into())),
    }
    protocol::write(&mut output, &plan.request)?;
    Ok(AttemptJob {
        input,
        output,
        finished: None,
    })
}

impl<R: Read, W: Write> AttemptJob<R, W> {
    /// Wait for a recorded child; return its attempt identity and PID.
    /// `None` means the attempt ended before startup; `finish` still returns its disposal report.
    pub fn started(&mut self) -> Result<Option<(String, u32)>, SupervisorError> {
        match protocol::read(&mut self.input)? {
            Reply::Started { attempt, game } => Ok(Some((attempt, game))),
            Reply::Rejected(reason) => Err(SupervisorError(reason)),
            Reply::Finished(report) => {
                self.finished = Some(*report);
                Ok(None)
            }
            _ => Err(SupervisorError("Expected startup report".into())),
        }
    }
    /// Request bounded cleanup. Await `finish` for disposal confirmation.
    pub fn cancel(&mut self) -> Result<(), SupervisorError> {
        protocol::write(&mut self.output, &Control::Cancel)
    }
    /// Notify the owner that the consumer's observation worker exited unexpectedly.
    #[cfg(feature = "maintainer-tools")]
    pub fn worker_lost(&mut self) -> Result<(), SupervisorError> {
        protocol::write(&mut self.output, &Control::WorkerLost)
    }
    /// Await the independent owner report. Channel failure never implies disposal.
    pub fn finish(mut self) -> Result<AttemptReport, SupervisorError> {
        if let Some(report) = self.finished.take() {
            return Ok(report);
        }
        loop {
            match protocol::read(&mut self.input)? {
                Reply::Started { .. } => continue,
                Reply::Finished(report) => return Ok(*report),
                Reply::Rejected(reason) => return Err(SupervisorError(reason)),
                _ => return Err(SupervisorError("Unexpected supervisor reply".into())),
            }
        }
    }
}

pub(crate) fn validate_request(request: &AttemptRequest) -> Result<(), SupervisorError> {
    if !(1..=30_000).contains(&request.hold_ms) || !request.output.is_absolute() {
        return Err(SupervisorError(
            "Expected absolute output path and hold_ms in 1..=30000".into(),
        ));
    }
    Ok(())
}

/// Run the candidate owner in a dedicated consumer process.
///
/// The input reader must return EOF when the controller dies. The process must remain alive
/// until this function returns. After it returns, exit the dedicated process; its reader thread
/// can still be waiting for EOF. No observation worker is started by this lifecycle-only API.
#[cfg(feature = "maintainer-tools")]
pub fn serve(
    input: impl Read + Send + 'static,
    output: impl Write + Send + 'static,
) -> Result<(), SupervisorError> {
    crate::execution::supervisor::serve(input, output, Authorization::Candidate)
}

/// A candidate capture of the retained category read-entry window.
#[derive(Debug)]
#[cfg(feature = "maintainer-tools")]
pub struct ObservationRequest {
    /// Exact installation; no nearest-version fallback.
    pub installation_hint: PathBuf,
    /// New absolute directory for retained artifacts.
    pub output: PathBuf,
    /// Bytes of the retained two-field category fixture.
    pub fixture: String,
    /// Observation budget in seconds, from 1 through 180.
    pub deadline_seconds: u64,
}

/// Maintainer controls; none of these grants live-operation admission.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ObservationControl {
    /// Capture the normal bounded window.
    #[default]
    Normal,
    /// Omit the field hook and refuse resume.
    #[cfg(feature = "maintainer-tools")]
    MissingHook,
    /// Disable the field hook at the activation gate and refuse resume.
    #[cfg(feature = "maintainer-tools")]
    LateHook,
    /// Omit one emitted field record without changing producer counts.
    #[cfg(feature = "maintainer-tools")]
    DroppedRecord,
    /// Omit the observation terminal.
    #[cfg(feature = "maintainer-tools")]
    MissingTerminal,
    /// Exercise a failed native memory read at the fixture boundary.
    #[cfg(feature = "maintainer-tools")]
    AccessFailure,
    /// Kill LLDB while stopped after the registration window.
    #[cfg(feature = "maintainer-tools")]
    WorkerLoss,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ObservationSpec {
    pub fixture: String,
    pub deadline_seconds: u64,
    pub control: ObservationControl,
}

/// Prepare the retained fixture for a candidate observation attempt.
/// Start a consumer-owned supervisor and pass this plan to `connect`, as for lifecycle requests.
#[cfg(feature = "maintainer-tools")]
pub fn prepare_observation(request: ObservationRequest) -> Result<PreparedPlan, SupervisorError> {
    prepare_observation_control(request, ObservationControl::Normal)
}

/// Prepare a deliberate failure control for maintainer qualification work.
#[doc(hidden)]
#[cfg(feature = "maintainer-tools")]
pub fn prepare_observation_control(
    request: ObservationRequest,
    control: ObservationControl,
) -> Result<PreparedPlan, SupervisorError> {
    let spec = ObservationSpec {
        fixture: request.fixture,
        deadline_seconds: request.deadline_seconds,
        control,
    };
    spec.validate()?;
    let mut plan = prepare(AttemptRequest {
        installation_hint: request.installation_hint,
        output: request.output,
        hold_ms: 1,
    })?;
    crate::binding::ExecutionPlan::open(&plan.request.request.installation_hint)?
        .validate_observation_content()?;
    crate::binding::ExecutionPlan::open(&plan.request.request.installation_hint)?.probe()?;
    plan.request.observation = Some(spec);
    Ok(plan)
}

impl ObservationSpec {
    pub(crate) fn validate(&self) -> Result<(), SupervisorError> {
        if self.fixture != crate::capture::FIXTURE_BODY
            || !(1..=180).contains(&self.deadline_seconds)
        {
            return Err(SupervisorError(
                "Expected the retained category fixture and a 1..=180 second deadline".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Authorization {
    Admitted,
    #[cfg(feature = "maintainer-tools")]
    Candidate,
}
impl Authorization {
    pub(crate) fn origin(self) -> &'static str {
        match self {
            Self::Admitted => "qualified-live",
            #[cfg(feature = "maintainer-tools")]
            Self::Candidate => "unqualified-candidate",
        }
    }
}

impl PlanRequest {
    pub(crate) fn validate(&self, authorization: Authorization) -> Result<(), SupervisorError> {
        if self.authorization != authorization {
            return Err(SupervisorError("Plan authorization mismatch".into()));
        }
        if authorization == Authorization::Admitted
            && (self.observation.is_none()
                || self
                    .observation
                    .as_ref()
                    .is_some_and(|spec| !matches!(spec.control, ObservationControl::Normal)))
        {
            return Err(SupervisorError(
                "Ordinary requests require an unmodified observation window".into(),
            ));
        }
        validate_request(&self.request)?;
        if let Some(spec) = &self.observation {
            spec.validate()?;
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn reported_job(report: AttemptReport) -> AttemptJob<std::io::Empty, Vec<u8>> {
    AttemptJob {
        input: std::io::empty(),
        output: Vec::new(),
        finished: Some(report),
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
            authorization: Authorization::Admitted,
            observation: Some(ObservationSpec {
                fixture: crate::capture::FIXTURE_BODY.into(),
                deadline_seconds: 180,
                control: ObservationControl::Normal,
            }),
        }
    }
    #[test]
    fn ordinary_wire_requires_the_complete_bounded_request() {
        assert!(plan().validate(Authorization::Admitted).is_ok());
        let mut request = plan();
        request.observation = None;
        assert!(request.validate(Authorization::Admitted).is_err());
        for deadline in [0, 181] {
            let mut request = plan();
            request.observation.as_mut().unwrap().deadline_seconds = deadline;
            assert!(request.validate(Authorization::Admitted).is_err());
        }
        let mut request = plan();
        request.observation.as_mut().unwrap().fixture = "other".into();
        assert!(request.validate(Authorization::Admitted).is_err());
    }
    #[cfg(feature = "maintainer-tools")]
    #[test]
    fn ordinary_wire_rejects_candidate_authority_and_fault_controls() {
        let mut request = plan();
        request.authorization = Authorization::Candidate;
        assert!(request.validate(Authorization::Admitted).is_err());
        for control in [
            ObservationControl::MissingHook,
            ObservationControl::LateHook,
            ObservationControl::DroppedRecord,
            ObservationControl::MissingTerminal,
            ObservationControl::AccessFailure,
            ObservationControl::WorkerLoss,
        ] {
            let mut request = plan();
            request.observation.as_mut().unwrap().control = control;
            assert!(request.validate(Authorization::Admitted).is_err());
        }
    }
}
