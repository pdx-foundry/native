//! The supervisor process: it owns the game, the debugger worker and the host reservation of one
//! session, and it reduces the worker's event stream to answers when the game is paused.
//!
//! The order of a session: handshake, request, admission, reservation, private profile, worker
//! package, suspended game, worker, pause, answers, controls, then cleanup. Cleanup always runs:
//! stop the worker, reap the game, confirm disposal and report.
use super::{instances::Reservation, owner_events::OwnerEvents};
use crate::{
    answer::Disposal,
    binding::{self, ExecutionPlan},
    engine::operations::{
        event_stream::{self, OwnerEvent},
        registry_items::{self, Observed},
    },
    protocol::{
        self, Hello, Reply,
        session::{Control, SessionOutcome, SessionReport, SessionRequest},
    },
    supervisor::SupervisorError,
    work_directory as files,
};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const HANDSHAKE_BUDGET: Duration = Duration::from_secs(15);
const SETUP_BUDGET: Duration = Duration::from_secs(30);
const DISPOSAL_BUDGET: Duration = Duration::from_secs(10);

enum Input {
    Hello(Hello),
    Request(Box<SessionRequest>),
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
            let request = protocol::read(&mut input)?;
            send.send(Input::Request(Box::new(request)))
                .map_err(|e| SupervisorError(e.to_string()))?;
            loop {
                let control: Control = protocol::read(&mut input)?;
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
                    "Controller did not receive the final report; see report.json".into(),
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
    let result = handshake(&input, &output).and_then(|request| run(request, &input, &output));
    match result {
        Ok(report) => output.send(Reply::Finished(Box::new(report)))?,
        Err(error) => output.send(Reply::Rejected(error.to_string()))?,
    }
    output.finish()
}
fn handshake(
    input: &Receiver<Input>,
    output: &Reporter,
) -> Result<SessionRequest, SupervisorError> {
    let Input::Hello(hello) = receive(input, HANDSHAKE_BUDGET)? else {
        return Err(SupervisorError("Expected hello".into()));
    };
    hello.validate()?;
    binding::prepare_owner(hello.controller)?;
    output.send(Reply::Ready)?;
    let Input::Request(request) = receive(input, HANDSHAKE_BUDGET)? else {
        return Err(SupervisorError("Expected a session request".into()));
    };
    request.validate()?;
    Ok(*request)
}

fn run(
    request: SessionRequest,
    input: &Receiver<Input>,
    output: &Reporter,
) -> Result<SessionReport, SupervisorError> {
    let plan = ExecutionPlan::open(&request.installation)?;
    if plan.build() != request.build {
        return Err(SupervisorError(
            "The supervisor found another game build than the caller".into(),
        ));
    }
    plan.admit()?;
    let registries = plan.registry_bindings(&request.registries)?;
    let content = plan.session_content(&request.registries)?;
    let parent = request
        .work_directory
        .parent()
        .ok_or_else(|| SupervisorError("The work directory needs a parent directory".into()))?
        .canonicalize()?;
    let name = request
        .work_directory
        .file_name()
        .ok_or_else(|| SupervisorError("Invalid work directory".into()))?;
    let work = parent.join(name);
    let attempt = format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| SupervisorError(e.to_string()))?
            .as_nanos()
    );
    let mut reservation = Reservation::acquire(attempt.clone(), work.clone())?;
    let mut report = SessionReport {
        attempt,
        outcome: SessionOutcome::Completed,
        disposal: Disposal::NotApplicable,
        reservation_resolved: false,
        diagnostics: Vec::new(),
    };
    let started = Instant::now();
    let mut game = None;
    let mut observer = None;
    let mut events = OwnerEvents::new(&work);
    let mut owns_work_directory = false;
    let session = (|| -> Result<SessionOutcome, SupervisorError> {
        binding::private_directory(&work)?;
        owns_work_directory = true;
        files::write_json(&work.join("request.json"), &request)?;
        if binding::conflicting_game(None)? {
            return Err(SupervisorError("Conflicting ordinary game instance".into()));
        }
        prepare_profile(&work)?;
        plan.prepare_registry_profile(&work, request.fixture.as_ref(), &registries, &content)?;
        if !plan.session_content_unchanged(&request.registries, &content) {
            return Err(SupervisorError(
                "Registry content changed during profile preparation".into(),
            ));
        }
        let observer =
            observer.insert(plan.observer(&work, &report.attempt, &request, &registries)?);
        plan.integrity()?;
        match input.try_recv() {
            Ok(event) => return Ok(interruption(event)),
            Err(mpsc::TryRecvError::Disconnected) => return Ok(SessionOutcome::CallerLost),
            Err(mpsc::TryRecvError::Empty) => {}
        }
        if started.elapsed() >= SETUP_BUDGET {
            return Ok(SessionOutcome::TimedOut);
        }
        let child = game.insert(plan.spawn_observed(&work, observer)?);
        reservation.record_game(child.identity()?)?;
        if !child.suspended()? {
            return Err(SupervisorError(
                "Child suspension was not established".into(),
            ));
        }
        if started.elapsed() >= SETUP_BUDGET {
            return Ok(SessionOutcome::TimedOut);
        }
        events.record(OwnerEvent::GameOwnedSuspended {
            pid: u64::from(child.pid()),
            identity: serde_json::to_string(&child.identity()?)?,
        })?;
        observer.start(child.pid())?;
        events.record(OwnerEvent::WorkerStarted)?;
        observe_session(
            input,
            output,
            child,
            observer,
            &mut events,
            &Session {
                work_directory: &work,
                attempt: &report.attempt,
                registries: request.registries.clone(),
                fixture: request.fixture.as_ref(),
                build: crate::BuildId(request.build.clone()),
                startup: Duration::from_secs(request.startup_seconds),
                idle: Duration::from_secs(request.idle_seconds),
            },
        )
    })();
    report.outcome = session.unwrap_or_else(|error| {
        if owns_work_directory
            && let Err(journal) = events.record(OwnerEvent::ObservationUnavailable {
                reason: error.to_string(),
            })
        {
            report.diagnostics.push(journal.to_string());
        }
        SessionOutcome::Failed(error.to_string())
    });
    let mut record = |event, diagnostics: &mut Vec<String>| {
        if let Err(error) = events.record(event) {
            diagnostics.push(error.to_string());
        }
    };
    let mut worker_stopped = true;
    if let Some(observer) = &mut observer {
        if report.outcome != SessionOutcome::WorkerLost {
            record(OwnerEvent::WorkerStopRequested, &mut report.diagnostics);
        }
        if let Err(error) = observer.stop() {
            worker_stopped = false;
            report.diagnostics.push(error.to_string());
        }
        if let Some(returncode) = observer.exited {
            record(
                OwnerEvent::WorkerExited { returncode },
                &mut report.diagnostics,
            );
        }
    }
    if let Some(mut child) = game {
        report.disposal = match child.dispose(DISPOSAL_BUDGET) {
            Ok(()) => Disposal::Confirmed,
            Err(error) => Disposal::Unconfirmed(error.to_string()),
        };
        record(
            OwnerEvent::DisposalChecked {
                confirmed: report.disposal == Disposal::Confirmed,
                reaped_pid: u64::from(child.pid()),
                game_exit: child.exit_status(),
            },
            &mut report.diagnostics,
        );
    }
    if let Err(error) = plan.integrity() {
        report.outcome = SessionOutcome::Failed(error.to_string());
    }
    if worker_stopped && !matches!(report.disposal, Disposal::Unconfirmed(_)) {
        match reservation.disposed() {
            Ok(()) => report.reservation_resolved = true,
            Err(error) => {
                report.outcome =
                    SessionOutcome::Failed(format!("Disposal bookkeeping failed: {error}"))
            }
        }
    }
    if owns_work_directory {
        write_report(&work, &reservation, &mut report);
    }
    Ok(report)
}

/// The fixed facts of one session that the observation loop needs.
struct Session<'a> {
    work_directory: &'a Path,
    attempt: &'a str,
    /// Internal names of the registries that the session observes.
    registries: Vec<String>,
    fixture: Option<&'a crate::FixtureRequest>,
    build: crate::BuildId,
    startup: Duration,
    idle: Duration,
}

/// Wait for the pause, reduce the worker's stream once, send the answers, then serve controls
/// until the session ends.
fn observe_session(
    input: &Receiver<Input>,
    output: &Reporter,
    child: &binding::OwnedGame,
    observer: &mut binding::Observer,
    events: &mut OwnerEvents,
    session: &Session<'_>,
) -> Result<SessionOutcome, SupervisorError> {
    let mut deadline = Instant::now() + session.startup;
    let mut answers = None;
    loop {
        if Instant::now() >= deadline {
            return Ok(SessionOutcome::TimedOut);
        }
        if binding::conflicting_game(Some(child.pid()))? {
            return Err(SupervisorError(
                "External game invalidated isolation".into(),
            ));
        }
        if observer.poll()? {
            return Ok(SessionOutcome::WorkerLost);
        }
        if answers.is_none() {
            if let Some(witness) = observer.pause_witness()? {
                child.identity()?;
                events.record(OwnerEvent::GamePauseConfirmed {
                    pid: u64::from(child.pid()),
                    returned: witness.returned,
                })?;
                let raw = files::read_bounded(
                    &session.work_directory.join("raw-trace.jsonl"),
                    protocol::observation::MAX_TRACE,
                )?;
                // A damaged stream has lost its terminals, so no answer from it is complete.
                let (records, _) = event_stream::read_worker_stream(&raw, session.attempt);
                let readiness =
                    registry_items::readiness(&records, events.all(), &session.registries)
                        .ok_or_else(|| {
                            SupervisorError(
                                "The witnesses of the registry initialization pause are missing or inconsistent"
                                    .into(),
                            )
                        })?;
                let registries: std::collections::BTreeMap<_, _> = session
                    .registries
                    .iter()
                    .map(|name| {
                        let items = registry_items::reduce(name, &records, events.all());
                        (name.clone(), items)
                    })
                    .collect();
                let fixture = session.fixture.map(|request| {
                    crate::engine::operations::fixture::reduce(
                        request,
                        &records,
                        events.all(),
                        session.build.clone(),
                    )
                });
                output.send(Reply::Paused {
                    readiness,
                    fixture: Box::new(fixture),
                    registries: registries.clone(),
                })?;
                answers = Some(registries);
                deadline = Instant::now() + session.idle;
            }
        } else {
            child.identity()?;
            if observer.pause_witness()?.is_none() {
                return Err(SupervisorError("Session pause witness lost".into()));
            }
        }
        match input.recv_timeout(Duration::from_millis(50)) {
            Ok(Input::Control(Control::ReadRegistry { name, request })) => {
                // Only an answer that the caller can give restarts the idle time.
                let answered = answers
                    .as_ref()
                    .and_then(|answers| answers.get(&name))
                    .is_some_and(|items| items.observed != Observed::Unavailable);
                if !answered {
                    return Err(SupervisorError(
                        "Registry read before the pause or for a registry with no answer".into(),
                    ));
                }
                output.send(Reply::ObservationRead { request })?;
                deadline = Instant::now() + session.idle;
            }
            Ok(Input::Control(Control::ReadFixture { request })) => {
                if answers.is_none() || session.fixture.is_none() {
                    return Err(SupervisorError(
                        "Fixture read without a prepared fixture at the pause".into(),
                    ));
                }
                output.send(Reply::ObservationRead { request })?;
                deadline = Instant::now() + session.idle;
            }
            Ok(Input::Control(Control::Close)) => return Ok(SessionOutcome::Completed),
            Ok(event) => return Ok(interruption(event)),
            Err(RecvTimeoutError::Disconnected) => return Ok(SessionOutcome::CallerLost),
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

/// Leave the owner details and the report in the work directory. Native keeps the
/// directory after a failure, and a caller that was lost never received the report.
fn write_report(work_directory: &Path, reservation: &Reservation, report: &mut SessionReport) {
    let owner = reservation
        .snapshot()
        .and_then(|snapshot| files::write_json(&work_directory.join("owner.json"), &snapshot));
    if let Err(error) = owner {
        report.diagnostics.push(format!("owner.json: {error}"));
        report.reservation_resolved = false;
        report.outcome = SessionOutcome::Failed("Session bookkeeping failed".into());
    }
    // Write the report last, so that it names every earlier failure.
    if let Err(error) = files::write_json(&work_directory.join("report.json"), report) {
        report.diagnostics.push(format!("report.json: {error}"));
        report.reservation_resolved = false;
        report.outcome = SessionOutcome::Failed("Session bookkeeping failed".into());
    }
}

fn interruption(event: Input) -> SessionOutcome {
    match event {
        Input::Control(Control::Cancel) => SessionOutcome::Cancelled,
        _ => SessionOutcome::CallerLost,
    }
}

/// A private game profile: a small window, no sound, no mods. The ordinary profile is never
/// touched.
fn prepare_profile(work_directory: &Path) -> Result<(), SupervisorError> {
    binding::private_directory(&work_directory.join("profile"))?;
    fs::write(
        work_directory.join("profile/settings.txt"),
        "graphics={size={x=640 y=360} fullScreen=no borderless=no renderer=2}\nmaster_volume=0\nmusic_volume=0\n",
    )?;
    fs::write(work_directory.join("profile/pdx_settings.txt"), "")?;
    fs::write(
        work_directory.join("profile/dlc_load.json"),
        "{\"enabled_mods\":[],\"disabled_dlcs\":[]}",
    )?;
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
    fn an_owned_suspended_child_is_reaped_and_its_reservation_resolved() {
        let _guard = binding::LIFECYCLE_TEST_LOCK.lock().unwrap();
        let root = store();
        let output = store();
        let mut reservation = reserve(root.path(), "owned", output.path()).unwrap();
        let mut child = binding::test_child(output.path()).unwrap();
        reservation.record_game(child.identity().unwrap()).unwrap();
        assert!(child.suspended().unwrap());
        child.dispose(DISPOSAL_BUDGET).unwrap();
        assert!(binding::process_identity(child.pid()).is_err());
        reservation.disposed().unwrap();
        assert_eq!(reservation.snapshot().unwrap()["state"], "disposed");
    }

    #[test]
    fn earlier_session_files_do_not_block_a_new_reservation() {
        let _guard = binding::LIFECYCLE_TEST_LOCK.lock().unwrap();
        let root = store();
        let output = store();
        let reservation = reserve(root.path(), "prior", output.path()).unwrap();
        drop(reservation);
        fs::write(root.path().join("prior.pending"), "old session").unwrap();
        let mut next = reserve(root.path(), "next", output.path()).unwrap();
        next.disposed().unwrap();
        assert_eq!(next.snapshot().unwrap()["state"], "disposed");
    }

    #[test]
    fn the_report_names_a_file_that_could_not_be_written_and_replaces_nothing() {
        let _guard = binding::LIFECYCLE_TEST_LOCK.lock().unwrap();
        let root = store();
        let output = store();
        let mut reservation = reserve(root.path(), "report", output.path()).unwrap();
        reservation.disposed().unwrap();
        fs::write(output.path().join("owner.json"), "existing file").unwrap();
        let mut report = SessionReport {
            attempt: "report".into(),
            outcome: SessionOutcome::Cancelled,
            disposal: Disposal::NotApplicable,
            reservation_resolved: true,
            diagnostics: Vec::new(),
        };
        write_report(output.path(), &reservation, &mut report);
        let written: SessionReport =
            serde_json::from_slice(&fs::read(output.path().join("report.json")).unwrap()).unwrap();
        assert_eq!(written.diagnostics, report.diagnostics);
        assert_eq!(written.diagnostics.len(), 1);
        assert!(written.diagnostics[0].contains("owner.json"));
        assert_eq!(written.disposal, Disposal::NotApplicable);
        assert_eq!(
            fs::read_to_string(output.path().join("owner.json")).unwrap(),
            "existing file"
        );
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
        let _guard = binding::LIFECYCLE_TEST_LOCK.lock().unwrap();
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
        // The lock is free; the old owner no longer blocks a launch.
        assert!(binding::test_reservation(root.path()).is_ok());
        assert!(reserve(root.path(), "afterdeath", output.path()).is_ok());
    }
}

#[cfg(test)]
mod interruption_tests {
    use super::*;
    #[test]
    fn a_cancel_and_a_lost_caller_keep_their_cause() {
        assert_eq!(
            interruption(Input::Control(Control::Cancel)),
            SessionOutcome::Cancelled
        );
        assert_eq!(interruption(Input::Lost), SessionOutcome::CallerLost);
    }
}
