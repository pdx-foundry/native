use super::instances::Reservation;
use crate::{
    binding::{self, InvestigationPlan},
    investigation::{
        self, CandidateDisposal, CandidateOutcome, Control, InvestigationReport, PlanRequest,
    },
    protocol::{self, Hello, Reply},
    supervisor::SupervisorError,
};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
    sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const HANDSHAKE_BUDGET: Duration = Duration::from_secs(15);
const ATTEMPT_BUDGET: Duration = Duration::from_secs(30);
const DISPOSAL_BUDGET: Duration = Duration::from_secs(10);

enum Input {
    Hello(Hello),
    Plan(PlanRequest),
    Control(Control),
    Lost,
}
fn receive(input: &Receiver<Input>, budget: Duration) -> Result<Input, SupervisorError> {
    input
        .recv_timeout(budget)
        .map_err(|_| SupervisorError("Controller disconnected or handshake timed out".into()))
}
fn reader(mut input: impl Read + Send + 'static) -> Receiver<Input> {
    let (send, receive) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let read = || -> Result<(), SupervisorError> {
            let hello = protocol::read(&mut input)?;
            send.send(Input::Hello(hello))
                .map_err(|e| SupervisorError(e.to_string()))?;
            let plan = protocol::read(&mut input)?;
            send.send(Input::Plan(plan))
                .map_err(|e| SupervisorError(e.to_string()))?;
            loop {
                let control = protocol::read(&mut input)?;
                send.send(Input::Control(control))
                    .map_err(|e| SupervisorError(e.to_string()))?;
            }
        };
        let _ = {
            let mut read = read;
            read()
        };
        let _ = send.send(Input::Lost);
    });
    receive
}

// Output cannot stall the resource owner. The writer thread owns no game or reservation.
struct Reporter {
    send: SyncSender<Reply>,
    finished: Receiver<Result<(), SupervisorError>>,
}
impl Reporter {
    fn new(mut output: impl Write + Send + 'static) -> Self {
        let (send, receive) = mpsc::sync_channel(4);
        let (finished, result) = mpsc::sync_channel(1);
        thread::spawn(move || {
            while let Ok(reply) = receive.recv() {
                let last = matches!(reply, Reply::Finished(_) | Reply::Rejected(_));
                let result = protocol::write(&mut output, &reply);
                if result.is_err() || last {
                    let _ = finished.send(result);
                    return;
                }
            }
        });
        Self {
            send,
            finished: result,
        }
    }
    fn send(&self, reply: Reply) -> Result<(), SupervisorError> {
        self.send
            .try_send(reply)
            .map_err(|_| SupervisorError("Controller output unavailable".into()))
    }
    fn finish(&self) -> Result<(), SupervisorError> {
        self.finished
            .recv_timeout(Duration::from_secs(1))
            .map_err(|_| {
                SupervisorError(
                    "Controller did not receive final report; inspect retained report".into(),
                )
            })?
    }
}

pub(crate) fn serve(
    input: impl Read + Send + 'static,
    output: impl Write + Send + 'static,
) -> Result<(), SupervisorError> {
    let input = reader(input);
    let output = Reporter::new(output);
    let result = handshake(&input, &output).and_then(|plan| run(plan, &input, &output));
    match result {
        Ok(report) => output.send(Reply::Finished(report))?,
        Err(error) => output.send(Reply::Rejected(error.to_string()))?,
    }
    output.finish()
}
fn handshake(input: &Receiver<Input>, output: &Reporter) -> Result<PlanRequest, SupervisorError> {
    let Input::Hello(hello) = receive(input, HANDSHAKE_BUDGET)? else {
        return Err(SupervisorError("Expected hello".into()));
    };
    hello.validate()?;
    binding::prepare_owner(hello.controller)?;
    output.send(Reply::Ready)?;
    let Input::Plan(plan) = receive(input, HANDSHAKE_BUDGET)? else {
        return Err(SupervisorError("Expected candidate request".into()));
    };
    investigation::validate_request(&plan.request)?;
    Ok(plan)
}

fn run(
    request: PlanRequest,
    input: &Receiver<Input>,
    output: &Reporter,
) -> Result<InvestigationReport, SupervisorError> {
    let plan = InvestigationPlan::open(&request.request.installation_hint)?;
    if plan.composition != request.composition {
        return Err(SupervisorError("Candidate composition mismatch".into()));
    }
    let parent = request
        .request
        .output
        .parent()
        .ok_or_else(|| SupervisorError("Output needs a parent directory".into()))?
        .canonicalize()?;
    let name = request
        .request
        .output
        .file_name()
        .ok_or_else(|| SupervisorError("Invalid output directory".into()))?;
    let retained = parent.join(name);
    let attempt = format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| SupervisorError(e.to_string()))?
            .as_nanos()
    );
    let mut reservation = Reservation::acquire(attempt.clone(), retained.clone())?;
    let mut report = InvestigationReport {
        origin: "unqualified-candidate".into(),
        attempt,
        composition: plan.composition.clone(),
        outcome: CandidateOutcome::Completed,
        disposal: CandidateDisposal::NotLaunched,
        reservation_resolved: false,
        output: retained,
        diagnostics: Vec::new(),
    };
    let started = Instant::now();
    let mut game = None;
    let mut owns_output = false;
    let operation = (|| -> Result<(), SupervisorError> {
        if binding::conflicting_game(None)? {
            return Err(SupervisorError("Conflicting ordinary game instance".into()));
        }
        binding::private_directory(&report.output)?;
        owns_output = true;
        prepare_profile(&report.output)?;
        write_new(&report.output.join("request.json"), &request)?;
        plan.integrity()?;
        match input.try_recv() {
            Ok(Input::Control(Control::Cancel)) => {
                report.outcome = CandidateOutcome::Cancelled;
                return Ok(());
            }
            Ok(_) | Err(mpsc::TryRecvError::Disconnected) => {
                report.outcome = CandidateOutcome::CallerLost;
                return Ok(());
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
        if started.elapsed() >= ATTEMPT_BUDGET {
            report.outcome = CandidateOutcome::TimedOut;
            return Ok(());
        }
        game = Some(plan.spawn(&report.output)?);
        let child = game.as_ref().unwrap();
        reservation.record_game(child.identity()?)?;
        if !child.suspended()? {
            return Err(SupervisorError(
                "Child suspension was not established".into(),
            ));
        }
        output.send(Reply::Started {
            attempt: report.attempt.clone(),
            game: child.pid(),
        })?;
        report.outcome = observe(
            input,
            child,
            started,
            Duration::from_millis(request.request.hold_ms),
        )?;
        Ok(())
    })();
    if let Err(error) = operation {
        report.outcome = CandidateOutcome::Failed(error.to_string());
    }
    if let Some(mut child) = game {
        report.disposal = match child.dispose(DISPOSAL_BUDGET) {
            Ok(()) => CandidateDisposal::Reaped,
            Err(error) => CandidateDisposal::Unconfirmed(error.to_string()),
        };
    }
    if let Err(error) = plan.integrity() {
        report.outcome = CandidateOutcome::Failed(error.to_string());
    }
    if !matches!(report.disposal, CandidateDisposal::Unconfirmed(_)) {
        match reservation.disposed() {
            Ok(()) => report.reservation_resolved = true,
            Err(error) => {
                report.outcome =
                    CandidateOutcome::Failed(format!("Disposal journal commit failed: {error}"))
            }
        }
    }
    if owns_output {
        let capture = investigation::CandidateCapture {
            version: 1,
            build: env!("PDX_NATIVE_BUILD").into(),
            report: report.clone(),
        };
        let retain = (|| -> Result<(), SupervisorError> {
            write_new(&report.output.join("owner.json"), &reservation.snapshot()?)?;
            write_new(&report.output.join("report.json"), &report)?;
            write_new(&report.output.join("capture.json"), &capture)
        })();
        if let Err(error) = retain {
            report.diagnostics.push(error.to_string());
        }
    }
    Ok(report)
}

fn observe(
    input: &Receiver<Input>,
    child: &binding::OwnedGame,
    started: Instant,
    hold: Duration,
) -> Result<CandidateOutcome, SupervisorError> {
    let hold_started = Instant::now();
    loop {
        if started.elapsed() >= ATTEMPT_BUDGET {
            return Ok(CandidateOutcome::TimedOut);
        }
        if binding::conflicting_game(Some(child.pid()))? {
            return Err(SupervisorError(
                "External game invalidated isolation".into(),
            ));
        }
        if !child.suspended()? {
            return Err(SupervisorError("Owned game no longer suspended".into()));
        }
        if hold_started.elapsed() >= hold {
            return Ok(CandidateOutcome::Completed);
        }
        match input.recv_timeout(Duration::from_millis(100)) {
            Ok(Input::Control(Control::Cancel)) => return Ok(CandidateOutcome::Cancelled),
            Ok(Input::Control(Control::WorkerLost)) => return Ok(CandidateOutcome::WorkerLost),
            Ok(_) | Err(RecvTimeoutError::Disconnected) => return Ok(CandidateOutcome::CallerLost),
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}
fn prepare_profile(output: &Path) -> Result<(), SupervisorError> {
    binding::private_directory(&output.join("profile"))?;
    fs::write(
        output.join("profile/settings.txt"),
        "graphics={size={x=640 y=360} fullScreen=no borderless=no}\nmaster_volume=0\n",
    )?;
    fs::write(output.join("profile/pdx_settings.txt"), "")?;
    fs::write(
        output.join("profile/dlc_load.json"),
        "{\"enabled_mods\":[],\"disabled_dlcs\":[]}",
    )?;
    Ok(())
}
fn write_new(path: &Path, value: &impl serde::Serialize) -> Result<(), SupervisorError> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.sync_all()?;
    File::open(path.parent().unwrap())?.sync_all()?;
    Ok(())
}

#[cfg(all(test, target_os = "macos", target_arch = "aarch64"))]
mod tests {
    use super::*;
    use std::{
        os::unix::fs::PermissionsExt,
        process::{Command, Stdio},
    };

    fn store() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        root
    }
    fn reserve(root: &Path, attempt: &str, output: &Path) -> Result<Reservation, SupervisorError> {
        Reservation::reserve(
            binding::test_reservation(root)?,
            attempt.into(),
            output.into(),
        )
    }

    #[test]
    fn disposal_is_independent_of_completion_and_worker_exit() {
        let _guard = binding::LIFECYCLE_TEST_LOCK.lock().unwrap();
        let root = store();
        for (index, expected) in [
            CandidateOutcome::Completed,
            CandidateOutcome::Cancelled,
            CandidateOutcome::CallerLost,
            CandidateOutcome::WorkerLost,
            CandidateOutcome::TimedOut,
        ]
        .into_iter()
        .enumerate()
        {
            let output = store();
            let mut reservation =
                reserve(root.path(), &format!("case{index}"), output.path()).unwrap();
            let mut child = binding::test_child(output.path()).unwrap();
            reservation.record_game(child.identity().unwrap()).unwrap();
            assert!(child.suspended().unwrap());
            let (send, input) = mpsc::sync_channel(1);
            let started = if expected == CandidateOutcome::TimedOut {
                Instant::now() - ATTEMPT_BUDGET
            } else {
                Instant::now()
            };
            match expected {
                CandidateOutcome::Cancelled => send.send(Input::Control(Control::Cancel)).unwrap(),
                CandidateOutcome::WorkerLost => {
                    // Real worker termination precedes notification; its handle never owns game/lock.
                    let mut worker = Command::new("/bin/sleep").arg("60").spawn().unwrap();
                    worker.kill().unwrap();
                    worker.wait().unwrap();
                    send.send(Input::Control(Control::WorkerLost)).unwrap();
                }
                CandidateOutcome::CallerLost => {
                    drop(send);
                }
                _ => {}
            }
            let outcome = observe(
                &input,
                &child,
                started,
                if expected == CandidateOutcome::Completed {
                    Duration::ZERO
                } else {
                    Duration::from_secs(1)
                },
            )
            .unwrap();
            assert_eq!(outcome, expected);
            child.dispose(DISPOSAL_BUDGET).unwrap();
            assert!(binding::process_identity(child.pid()).is_err());
            reservation.disposed().unwrap();
        }
    }

    #[test]
    fn partial_launch_and_failed_commit_preserve_ownership() {
        let root = store();
        let output = store();
        let mut reservation = reserve(root.path(), "partial", output.path()).unwrap();
        let mut child = binding::test_child(output.path()).unwrap();
        fs::write(root.path().join("partial.pending"), "interrupted write").unwrap();
        assert!(reservation.record_game(child.identity().unwrap()).is_err());
        child.dispose(DISPOSAL_BUDGET).unwrap();
        assert!(reservation.disposed().is_err());
        drop(reservation);
        assert!(reserve(root.path(), "next", output.path()).is_err());
        assert_eq!(
            fs::read_to_string(root.path().join("partial.pending")).unwrap(),
            "interrupted write"
        );
    }

    #[test]
    fn unknown_unreadable_and_unresolved_records_block_without_overwrite() {
        let root = store();
        let output = store();
        let reservation = reserve(root.path(), "prior", output.path()).unwrap();
        drop(reservation);
        let path = root.path().join("prior.json");
        let original = fs::read(&path).unwrap();
        assert!(reserve(root.path(), "unresolved", output.path()).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        for content in [b"{\"version\":999}".as_slice(), b"{", b"null"] {
            fs::write(&path, content).unwrap();
            assert!(reserve(root.path(), "next", output.path()).is_err());
            assert_eq!(fs::read(&path).unwrap(), content);
        }
        fs::write(&path, &original).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o0)).unwrap();
        assert!(reserve(root.path(), "denied", output.path()).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(fs::read(&path).unwrap(), original);
    }

    // Invoked only by the parent test in a separate test-runner process. No production override.
    #[test]
    fn reservation_process_driver() {
        let Some(root) = std::env::var_os("NATIVE_TEST_RESERVATION") else {
            return;
        };
        let root = std::path::PathBuf::from(root);
        let output = std::path::PathBuf::from(std::env::var_os("NATIVE_TEST_READY").unwrap());
        let _reservation = reserve(&root, "deadowner", output.parent().unwrap()).unwrap();
        fs::write(&output, "reserved").unwrap();
        thread::sleep(Duration::from_secs(30));
    }

    #[test]
    fn competing_process_and_dead_owner_cannot_reuse_namespace() {
        let root = store();
        let output = store();
        let ready = output.path().join("ready");
        let mut owner = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "execution::supervisor::tests::reservation_process_driver",
                "--nocapture",
            ])
            .env("NATIVE_TEST_RESERVATION", root.path())
            .env("NATIVE_TEST_READY", &ready)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !ready.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(ready.exists());
        assert!(reserve(root.path(), "competitor", output.path()).is_err());
        owner.kill().unwrap();
        owner.wait().unwrap();
        // OS lock is now free, but unresolved durable ownership still prevents launch.
        assert!(binding::test_reservation(root.path()).is_ok());
        assert!(reserve(root.path(), "afterdeath", output.path()).is_err());
    }
}
