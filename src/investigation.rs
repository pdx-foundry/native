//! Unqualified lifecycle investigations. These reports cannot enable public live operations.
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
pub struct CandidateRequest {
    /// Exact installation to bind; no target override or nearest-version fallback.
    pub installation_hint: PathBuf,
    /// New directory for retained candidate records and a private profile.
    pub output: PathBuf,
    /// Time to hold the child suspended, from 1 through 30,000 milliseconds.
    /// Reaching 30,000 reports timeout rather than successful completion.
    pub hold_ms: u64,
}

/// Why the candidate attempt ended. This says nothing about disposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CandidateOutcome {
    /// The bounded suspended-lifetime check completed.
    Completed,
    /// The controller explicitly cancelled.
    Cancelled,
    /// The owner deadline elapsed.
    TimedOut,
    /// The controller channel closed or became invalid.
    CallerLost,
    /// The consumer reported loss of its observation worker.
    WorkerLost,
    /// Launch, isolation, integrity, or storage failed.
    Failed(String),
}

/// Independent owner disposal evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CandidateDisposal {
    /// No game was created.
    NotLaunched,
    /// The owner reaped its direct child.
    Reaped,
    /// Disposal could not be confirmed; the reservation remains blocking.
    Unconfirmed(String),
}

/// Retained candidate lifecycle result; never a supported operation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvestigationReport {
    /// Explicitly unqualified origin.
    pub origin: String,
    /// Unique attempt identity, including failed attempts.
    pub attempt: String,
    /// Bound composition identity.
    pub composition: String,
    /// Independent operation outcome.
    pub outcome: CandidateOutcome,
    /// Independent disposal evidence.
    pub disposal: CandidateDisposal,
    /// Whether disposal was durably committed to the reservation journal.
    pub reservation_resolved: bool,
    /// Directory retained for inspection; Native never deletes it automatically.
    pub output: PathBuf,
    /// Failures retaining the report; disposal facts above remain independent.
    pub diagnostics: Vec<String>,
}

/// Versioned candidate artifact, separate from supported replay evidence.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateCapture {
    /// Candidate format version; this is not the observation replay schema.
    pub version: u32,
    /// Exact linked Native source/build identity.
    pub build: String,
    /// Owner-produced lifecycle report.
    pub report: InvestigationReport,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanRequest {
    pub request: CandidateRequest,
    pub composition: String,
}
#[derive(Serialize, Deserialize)]
pub(crate) enum Control {
    Cancel,
    WorkerLost,
}

/// Controller connection. Dropping its writer requests cleanup; it does not confirm disposal.
pub struct CandidateJob<R, W> {
    input: R,
    output: W,
    finished: Option<InvestigationReport>,
}

/// Prepared candidate identity. Construct with `prepare` before starting the supervisor.
/// This is not an execution permit and cannot be promoted into an admitted operation.
pub struct CandidatePlan {
    request: PlanRequest,
}

/// Validate and pin candidate inputs before starting the consumer's supervisor process.
/// The owner repeats these checks before allocation; preparation grants no launch permission.
pub fn prepare(request: CandidateRequest) -> Result<CandidatePlan, SupervisorError> {
    validate_request(&request)?;
    let plan = crate::binding::InvestigationPlan::open(&request.installation_hint)?;
    Ok(CandidatePlan {
        request: PlanRequest {
            composition: plan.composition,
            request,
        },
    })
}

/// Connect a prepared candidate to a consumer-created supervisor using private pipes.
/// Both processes must link the same Native build. This function does not start a process.
pub fn connect<R: Read, W: Write>(
    mut input: R,
    mut output: W,
    plan: CandidatePlan,
) -> Result<CandidateJob<R, W>, SupervisorError> {
    protocol::write(&mut output, &Hello::current())?;
    match protocol::read(&mut input)? {
        Reply::Ready => {}
        Reply::Rejected(reason) => return Err(SupervisorError(reason)),
        _ => return Err(SupervisorError("Expected supervisor handshake".into())),
    }
    protocol::write(&mut output, &plan.request)?;
    Ok(CandidateJob {
        input,
        output,
        finished: None,
    })
}

impl<R: Read, W: Write> CandidateJob<R, W> {
    /// Wait for a recorded child; return its attempt identity and PID.
    /// `None` means the attempt ended before startup; `finish` still returns its disposal report.
    pub fn started(&mut self) -> Result<Option<(String, u32)>, SupervisorError> {
        match protocol::read(&mut self.input)? {
            Reply::Started { attempt, game } => Ok(Some((attempt, game))),
            Reply::Rejected(reason) => Err(SupervisorError(reason)),
            Reply::Finished(report) => {
                self.finished = Some(report);
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
    pub fn worker_lost(&mut self) -> Result<(), SupervisorError> {
        protocol::write(&mut self.output, &Control::WorkerLost)
    }
    /// Await the independent owner report. Channel failure never implies disposal.
    pub fn finish(mut self) -> Result<InvestigationReport, SupervisorError> {
        if let Some(report) = self.finished.take() {
            return Ok(report);
        }
        loop {
            match protocol::read(&mut self.input)? {
                Reply::Started { .. } => continue,
                Reply::Finished(report) => return Ok(report),
                Reply::Rejected(reason) => return Err(SupervisorError(reason)),
                _ => return Err(SupervisorError("Unexpected supervisor reply".into())),
            }
        }
    }
}

pub(crate) fn validate_request(request: &CandidateRequest) -> Result<(), SupervisorError> {
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
pub fn serve(
    input: impl Read + Send + 'static,
    output: impl Write + Send + 'static,
) -> Result<(), SupervisorError> {
    crate::execution::supervisor::serve(input, output)
}
