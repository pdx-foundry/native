//! Registry questions over independently supervised native execution.
use crate::{operation, supervisor::SupervisorError};
use serde::Serialize;
use std::io::{self, Read};
use std::{
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// Execution settings shared by registry queries. Native chooses the collection method.
#[derive(Debug)]
pub struct RegistryOptions {
    /// Existing absolute directory under which Native retains a new directory for each attempt.
    pub retention_directory: PathBuf,
    /// Optional observation deadline in seconds, from 1 through 180. Defaults to 180.
    pub deadline_seconds: Option<u64>,
}

/// A registry cannot be queried. No unsupported query is treated as an empty registry.
#[derive(Debug, Serialize)]
pub enum RegistryError {
    /// No implementation is declared for this public name on the selected target.
    Unsupported {
        /// Exact requested name.
        registry: String,
    },
    /// Current inputs, prerequisites, or qualification do not permit execution.
    Unavailable {
        /// Independent admission failures.
        reasons: Vec<crate::UnavailableReason>,
    },
    /// Retention location or deadline is invalid.
    InvalidOptions(String),
    /// The supervisor channel or setup failed; this is never disposal confirmation.
    Supervisor(String),
}
impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "registry query failed: {self:?}")
    }
}
impl std::error::Error for RegistryError {}
impl From<SupervisorError> for RegistryError {
    fn from(error: SupervisorError) -> Self {
        Self::Supervisor(error.to_string())
    }
}

/// Configured Native consumer. Each query creates and reaps a dedicated supervisor child.
/// Use `get_registry` for completion or `start_registry` when cancellation is needed.
pub struct RegistryClient {
    context: crate::EngineContext,
    command: Command,
    options: RegistryOptions,
}

pub(crate) fn client(
    context: crate::EngineContext,
    command: Command,
    options: RegistryOptions,
) -> Result<RegistryClient, RegistryError> {
    if !(1..=180).contains(&options.deadline_seconds.unwrap_or(180))
        || !options.retention_directory.is_absolute()
        || !options.retention_directory.is_dir()
    {
        return Err(RegistryError::InvalidOptions(
            "Expected an existing absolute retention directory and a 1..=180 second deadline"
                .into(),
        ));
    }
    Ok(RegistryClient {
        context,
        command,
        options,
    })
}

impl RegistryClient {
    /// Check support and current qualification without launching a game or supervisor.
    pub fn capability(&self, registry: &str) -> crate::CapabilityReport {
        self.context.capability(&crate::CapabilityRequest {
            registry: registry.into(),
        })
    }

    /// Retrieve engine collection keys for `traditions` or `tradition_categories`.
    /// Unknown names return `Unsupported`; unavailable admission never starts a process.
    /// Completion, evidence finalization, and disposal remain independent in the report.
    pub fn get_registry(&mut self, registry: &str) -> Result<RegistryReport, RegistryError> {
        self.start_registry(registry)?.finish()
    }

    /// Start a registry query and return a cancellable job. Native owns plans and probes.
    pub fn start_registry(&mut self, registry: &str) -> Result<RegistryJob, RegistryError> {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| RegistryError::InvalidOptions(error.to_string()))?
            .as_nanos();
        let output = self
            .options
            .retention_directory
            .join(format!("registry-{}-{id}", std::process::id()));
        let plan = self.context.prepare_registry(
            registry,
            output,
            self.options.deadline_seconds.unwrap_or(180),
        )?;
        let mut owner = self
            .command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|error| RegistryError::Supervisor(error.to_string()))?;
        let mut input = TimedOutput::new(
            owner.stdout.take().expect("piped output"),
            Duration::from_secs(15),
        );
        let mut output = owner.stdin.take().expect("piped control");
        if let Err(error) =
            operation::handshake(&mut input, &mut output, operation::Authorization::Admitted)
        {
            // No plan has been sent, so this child has no authority to own a game.
            drop(output);
            let _ = owner.kill();
            let _ = owner.wait();
            return Err(error.into());
        }
        input.deadline =
            Instant::now() + Duration::from_secs(self.options.deadline_seconds.unwrap_or(180) + 90);
        let job = match operation::begin(input, output, plan) {
            Ok(job) => job,
            Err(error) => {
                reap_later(owner);
                return Err(error.into());
            }
        };
        Ok(RegistryJob {
            job: Some(job),
            owner: Some(owner),
            registry: registry.into(),
        })
    }
}

/// An in-flight registry query. Dropping it closes control and requests independent disposal.
/// Only `finish` confirms disposal; a background reaper waits for a dropped supervisor.
pub struct RegistryJob {
    job: Option<operation::AttemptJob<TimedOutput, ChildStdin>>,
    owner: Option<Child>,
    registry: String,
}

/// Registry answer and independent execution outcome, even when evidence retention fails.
#[derive(Debug, Serialize)]
pub struct RegistryReport {
    /// Pinned target composition, opaque to the consumer.
    pub context: crate::ContextIdentity,
    /// Unique retained attempt.
    pub attempt: String,
    /// Why the operation stopped; this does not establish answer completeness.
    pub outcome: crate::OperationOutcome,
    /// Independently established game disposal, even if evidence finalization failed.
    pub disposal: crate::OperationDisposal,
    /// Whether cleanup durably resolved the host reservation.
    pub reservation_resolved: bool,
    /// Registry entries and completeness, or an explicit evidence-finalization error.
    pub result: Result<crate::RegistryResult, String>,
    /// Relocatable immutable evidence for `Engine::replay_registry`.
    pub replay: Option<crate::ReplayRequest>,
    /// Retained attempt directory, including the independent owner report.
    pub retained: PathBuf,
    /// Additional retention or supervisor-exit failures.
    pub diagnostics: Vec<String>,
}

impl RegistryJob {
    /// Wait for game ownership. A false result still has a final report available.
    pub fn started(&mut self) -> Result<bool, RegistryError> {
        Ok(self.job.as_mut().unwrap().started()?.is_some())
    }
    /// Request cancellation. Await `finish` to confirm cleanup.
    pub fn cancel(&mut self) -> Result<(), RegistryError> {
        self.job.as_mut().unwrap().cancel().map_err(Into::into)
    }
    /// Await the registry answer and independently reported cleanup.
    pub fn finish(mut self) -> Result<RegistryReport, RegistryError> {
        let report = self.job.take().unwrap().finish()?;
        let mut report = normalize(report, &self.registry)?;
        let mut owner = self.owner.take().unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match owner.try_wait() {
                Ok(Some(status)) => {
                    if !status.success() {
                        report
                            .diagnostics
                            .push(format!("Supervisor exit: {status}"));
                    }
                    break;
                }
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                other => {
                    report
                        .diagnostics
                        .push(format!("Supervisor exit not confirmed: {other:?}"));
                    reap_later(owner);
                    break;
                }
            }
        }
        Ok(report)
    }
}
impl Drop for RegistryJob {
    fn drop(&mut self) {
        drop(self.job.take());
        if let Some(owner) = self.owner.take() {
            reap_later(owner);
        }
    }
}
fn reap_later(mut owner: Child) {
    std::thread::spawn(move || {
        let _ = owner.wait();
    });
}
fn normalize(
    report: operation::AttemptReport,
    registry: &str,
) -> Result<RegistryReport, RegistryError> {
    if report.origin != operation::Authorization::Admitted.origin() {
        return Err(RegistryError::Supervisor(
            "Expected admitted registry report".into(),
        ));
    }
    let replay = report.replay.map(|descriptor| crate::ReplayRequest {
        artifact_root: report.output.join("evidence"),
        descriptor,
    });
    let result = replay
        .as_ref()
        .ok_or_else(|| "Registry evidence was not finalized".into())
        .and_then(|request| {
            crate::Engine
                .replay_registry(request.clone())
                .map_err(|error| error.to_string())
        })
        .and_then(|mut result| {
            if result.registry != registry {
                return Err("Retained registry does not match the query".into());
            }
            result.origin = crate::ResultOrigin::Live;
            Ok(result)
        });
    Ok(RegistryReport {
        context: crate::ContextIdentity(report.composition),
        attempt: report.attempt,
        outcome: report.outcome,
        disposal: report.disposal,
        reservation_resolved: report.reservation_resolved,
        result,
        replay,
        retained: report.output,
        diagnostics: report.diagnostics,
    })
}

// A bounded reader keeps a silent or malformed consumer helper from blocking the API forever.
// Its thread owns no control pipe, game, or reservation.
struct TimedOutput {
    chunks: std::sync::mpsc::Receiver<io::Result<Vec<u8>>>,
    pending: io::Cursor<Vec<u8>>,
    deadline: Instant,
}
impl TimedOutput {
    fn new(mut input: impl Read + Send + 'static, budget: Duration) -> Self {
        let (send, chunks) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            loop {
                let mut bytes = vec![0; 8192];
                let chunk = input.read(&mut bytes).map(|count| {
                    bytes.truncate(count);
                    bytes
                });
                let last = !chunk.as_ref().is_ok_and(|bytes| !bytes.is_empty());
                if send.send(chunk).is_err() || last {
                    return;
                }
            }
        });
        Self {
            chunks,
            pending: io::Cursor::new(Vec::new()),
            deadline: Instant::now() + budget,
        }
    }
}
impl Read for TimedOutput {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        let count = self.pending.read(output)?;
        if count != 0 {
            return Ok(count);
        }
        match self
            .chunks
            .recv_timeout(self.deadline.saturating_duration_since(Instant::now()))
        {
            Ok(chunk) => {
                self.pending = io::Cursor::new(chunk?);
                self.pending.read(output)
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Ok(0),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Native supervisor response deadline elapsed",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn failed_evidence() -> operation::AttemptReport {
        operation::AttemptReport {
            origin: operation::Authorization::Admitted.origin().into(),
            attempt: "unit".into(),
            composition: "opaque".into(),
            outcome: crate::OperationOutcome::Cancelled,
            disposal: crate::OperationDisposal::Reaped,
            reservation_resolved: true,
            output: std::env::temp_dir(),
            diagnostics: vec!["retention failed".into()],
            replay: None,
        }
    }
    #[test]
    fn silent_helper_read_expires_without_waiting_for_process_exit() {
        struct Silent(std::sync::mpsc::Receiver<()>);
        impl Read for Silent {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                let _ = self.0.recv();
                Ok(0)
            }
        }
        let (release, wait) = std::sync::mpsc::channel();
        let mut reader = TimedOutput::new(Silent(wait), Duration::from_millis(10));
        assert_eq!(
            reader.read(&mut [0; 4]).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        drop(release);
    }

    #[test]
    fn failed_evidence_preserves_disposal_and_termination() {
        let report = normalize(failed_evidence(), "traditions").unwrap();
        assert!(report.result.is_err() && report.reservation_resolved);
        assert_eq!(report.outcome, crate::OperationOutcome::Cancelled);
        assert_eq!(report.disposal, crate::OperationDisposal::Reaped);
    }
    #[test]
    fn candidate_is_never_an_admitted_registry_report() {
        let mut report = failed_evidence();
        report.origin = "unqualified-candidate".into();
        assert!(normalize(report, "traditions").is_err());
    }
}
