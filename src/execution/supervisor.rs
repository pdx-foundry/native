use super::instances::Reservation;
use crate::{
    binding::{self, ExecutionPlan},
    operation::{
        self, AttemptReport, Authorization, Control, OperationDisposal, OperationOutcome,
        PlanRequest,
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
const SETUP_BUDGET: Duration = Duration::from_secs(30);
const HOLD_LIMIT: Duration = Duration::from_secs(30);
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
fn reader(mut input: impl Read + Send + 'static, authorization: Authorization) -> Receiver<Input> {
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
                let control: Control = protocol::read(&mut input)?;
                if authorization == Authorization::Admitted
                    && !matches!(
                        control,
                        Control::Cancel | Control::Close | Control::ReadRegistry { .. }
                    )
                {
                    return Err(SupervisorError("Unsupported ordinary control".into()));
                }
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
    authorization: Authorization,
) -> Result<(), SupervisorError> {
    let input = reader(input, authorization);
    let output = Reporter::new(output);
    let result =
        handshake(&input, &output, authorization).and_then(|plan| run(plan, &input, &output));
    match result {
        Ok(report) => output.send(Reply::Finished(Box::new(report)))?,
        Err(error) => output.send(Reply::Rejected(error.to_string()))?,
    }
    output.finish()
}
fn handshake(
    input: &Receiver<Input>,
    output: &Reporter,
    authorization: Authorization,
) -> Result<PlanRequest, SupervisorError> {
    let Input::Hello(hello) = receive(input, HANDSHAKE_BUDGET)? else {
        return Err(SupervisorError("Expected hello".into()));
    };
    hello.validate()?;
    if hello.authorization != authorization {
        return Err(SupervisorError("Supervisor authorization mismatch".into()));
    }
    binding::prepare_owner(hello.controller)?;
    output.send(Reply::Ready)?;
    let Input::Plan(plan) = receive(input, HANDSHAKE_BUDGET)? else {
        return Err(SupervisorError("Expected operation request".into()));
    };
    plan.validate(authorization)?;
    Ok(plan)
}

fn run(
    request: PlanRequest,
    input: &Receiver<Input>,
    output: &Reporter,
) -> Result<AttemptReport, SupervisorError> {
    let mut plan = ExecutionPlan::open(&request.request.installation_hint)?;
    if plan.composition() != request.composition {
        return Err(SupervisorError("Composition mismatch".into()));
    }
    let session = request
        .observation
        .as_ref()
        .and_then(|spec| spec.session.as_ref());
    if request.authorization == Authorization::Admitted {
        if session.is_some() {
            plan.admit_session()?;
        } else {
            plan.admit(
                request
                    .observation
                    .as_ref()
                    .and_then(|spec| spec.registry.as_deref())
                    .ok_or_else(|| {
                        SupervisorError("Ordinary requests require a registry".into())
                    })?,
            )?;
        }
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
    let mut report = AttemptReport {
        origin: request.authorization.origin().into(),
        attempt,
        composition: plan.composition().into(),
        outcome: OperationOutcome::Completed,
        disposal: OperationDisposal::NotLaunched,
        reservation_resolved: false,
        output: retained,
        diagnostics: Vec::new(),
        replay: None,
        registries: Default::default(),
    };
    let started = Instant::now();
    let mut game = None;
    let mut observer = None;
    let mut capture = None;
    let mut owns_output = false;
    let operation = (|| -> Result<(), SupervisorError> {
        binding::private_directory(&report.output)?;
        owns_output = true;
        write_new(&report.output.join("request.json"), &request)?;
        if binding::conflicting_game(None)? {
            return Err(SupervisorError("Conflicting ordinary game instance".into()));
        }
        prepare_profile(&report.output)?;
        if let Some(spec) = &request.observation {
            if spec.registry.is_none() && spec.session.is_none() {
                prepare_fixture(&report.output, spec)?;
            } else {
                plan.prepare_registry_profile(&report.output)?;
            }
            let (prepared, retained) = plan.observer(
                &report.output,
                &report.attempt,
                spec,
                request.authorization.origin(),
            )?;
            observer = Some(prepared);
            capture = Some(retained);
        }
        plan.integrity()?;
        match input.try_recv() {
            Ok(event) => {
                report.outcome = interruption(event);
                return Ok(());
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                report.outcome = OperationOutcome::CallerLost;
                return Ok(());
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
        if started.elapsed() >= SETUP_BUDGET {
            report.outcome = OperationOutcome::TimedOut;
            return Ok(());
        }
        game = Some(match &observer {
            Some(observer) => plan.spawn_observed(&report.output, observer)?,
            None => plan.spawn(&report.output)?,
        });
        let child = game.as_ref().unwrap();
        reservation.record_game(child.identity()?)?;
        if !child.suspended()? {
            return Err(SupervisorError(
                "Child suspension was not established".into(),
            ));
        }
        if started.elapsed() >= SETUP_BUDGET {
            report.outcome = OperationOutcome::TimedOut;
            return Ok(());
        }
        output.send(Reply::Started {
            attempt: report.attempt.clone(),
            game: child.pid(),
        })?;
        report.outcome = if let (Some(observer), Some(capture), Some(spec)) =
            (&mut observer, &mut capture, &request.observation)
        {
            capture.record(evidence::recorded::OwnerEvent::GameOwnedSuspended {
                pid: u64::from(child.pid()),
                identity: serde_json::to_string(&child.identity()?)?,
            })?;
            observer.start(child.pid())?;
            capture.record(evidence::recorded::OwnerEvent::WorkerStarted)?;
            if let Some(session) = &spec.session {
                observe_session(
                    input,
                    output,
                    child,
                    observer,
                    capture,
                    &report.output,
                    spec.deadline_seconds,
                    session.idle_seconds,
                )?
            } else {
                observe_worker(
                    input,
                    child,
                    observer,
                    Duration::from_secs(spec.deadline_seconds),
                )?
            }
        } else {
            observe(
                input,
                child,
                Instant::now(),
                Duration::from_millis(request.request.hold_ms),
            )?
        };
        Ok(())
    })();
    if let Err(error) = operation {
        if let Some(capture) = &mut capture
            && let Err(retention) =
                capture.record(evidence::recorded::OwnerEvent::ObservationUnavailable {
                    reason: error.to_string(),
                })
        {
            report.diagnostics.push(retention.to_string());
        }
        report.outcome = OperationOutcome::Failed(error.to_string());
    }
    let mut worker_stopped = true;
    if let Some(observer) = &mut observer {
        if session.is_some()
            && report.outcome != OperationOutcome::WorkerLost
            && let Some(capture) = &mut capture
            && let Err(error) = capture.record(evidence::recorded::OwnerEvent::WorkerStopRequested)
        {
            report.diagnostics.push(error.to_string());
        }
        if let Err(error) = observer.stop() {
            worker_stopped = false;
            report.diagnostics.push(error.to_string());
        }
        if let (Some(code), Some(capture)) = (observer.exited, &mut capture) {
            if session.is_none() && code != 0 && report.outcome == OperationOutcome::Completed {
                report.outcome = OperationOutcome::WorkerLost;
            }
            if let Err(error) =
                capture.record(evidence::recorded::OwnerEvent::WorkerExited { returncode: code })
            {
                report.diagnostics.push(error.to_string());
            }
        }
    }
    if let Some(mut child) = game {
        report.disposal = match child.dispose(DISPOSAL_BUDGET) {
            Ok(()) => OperationDisposal::Reaped,
            Err(error) => OperationDisposal::Unconfirmed(error.to_string()),
        };
        if let Some(capture) = &mut capture {
            let event = evidence::recorded::OwnerEvent::DisposalChecked {
                confirmed: report.disposal == OperationDisposal::Reaped,
                reaped_pid: u64::from(child.pid()),
                game_exit: child.exit_status(),
                remaining_identity: if report.disposal == OperationDisposal::Reaped {
                    None
                } else {
                    Some(serde_json::to_string(&child.identity().ok())?)
                },
            };
            if let Err(error) = capture.record(event) {
                report.diagnostics.push(error.to_string());
            }
        }
    }
    if let Err(error) = plan.integrity() {
        report.outcome = OperationOutcome::Failed(error.to_string());
    }
    if worker_stopped && !matches!(report.disposal, OperationDisposal::Unconfirmed(_)) {
        match reservation.disposed() {
            Ok(()) => report.reservation_resolved = true,
            Err(error) => {
                report.outcome =
                    OperationOutcome::Failed(format!("Disposal journal commit failed: {error}"))
            }
        }
    }
    if let Some(capture) = capture {
        if session.is_some() {
            let (registries, diagnostics) = capture.session_snapshot(&report.output, "final");
            report.registries = registries;
            report.diagnostics.extend(diagnostics);
        } else {
            match capture.finish(&report.output) {
                Ok(reference) => report.replay = Some(reference),
                Err(error) => report
                    .diagnostics
                    .push(format!("Capture finalization failed: {error}")),
            }
        }
    }
    if owns_output {
        retain_report(&reservation, &mut report);
    }
    Ok(report)
}

#[allow(clippy::too_many_arguments)]
fn observe_session(
    input: &Receiver<Input>,
    output: &Reporter,
    child: &binding::OwnedGame,
    observer: &mut binding::Observer,
    capture: &mut crate::capture::Capture,
    retained: &Path,
    startup_seconds: u64,
    idle_seconds: u64,
) -> Result<OperationOutcome, SupervisorError> {
    let mut deadline = Instant::now() + Duration::from_secs(startup_seconds);
    let mut snapshots = None;
    loop {
        if Instant::now() >= deadline {
            return Ok(OperationOutcome::TimedOut);
        }
        if binding::conflicting_game(Some(child.pid()))? {
            return Err(SupervisorError(
                "External game invalidated isolation".into(),
            ));
        }
        if observer.poll()? {
            return Ok(OperationOutcome::WorkerLost);
        }
        if snapshots.is_none() {
            if let Some(witness) = observer.pause_witness()? {
                child.identity()?;
                capture.record(evidence::recorded::OwnerEvent::GamePauseConfirmed {
                    pid: u64::from(child.pid()),
                    returned: witness.returned,
                })?;
                let readiness = capture.session_readiness(retained)?.ok_or_else(|| {
                    SupervisorError(
                        "Registry initialization readiness witnesses are missing or inconsistent"
                            .into(),
                    )
                })?;
                let (references, diagnostics) = capture.session_snapshot(retained, "snapshots");
                if references.is_empty() {
                    return Err(SupervisorError(format!(
                        "Session evidence retention failed: {diagnostics:?}"
                    )));
                }
                // Partial retention is explicit in per-registry replay; never revoke another answer.
                crate::capture::write_json(
                    &retained.join("snapshot-diagnostics.json"),
                    &diagnostics,
                )?;
                output.send(Reply::Paused {
                    readiness,
                    output: retained.into(),
                    registries: references.clone(),
                })?;
                snapshots = Some(references);
                deadline = Instant::now() + Duration::from_secs(idle_seconds);
            }
        } else {
            child.identity()?;
            if observer.pause_witness()?.is_none() {
                return Err(SupervisorError("Session pause witness lost".into()));
            }
        }
        match input.recv_timeout(Duration::from_millis(50)) {
            Ok(Input::Control(Control::ReadRegistry { name, request })) => {
                let Some(reference) = snapshots
                    .as_ref()
                    .and_then(|references| references.get(&name))
                else {
                    return Err(SupervisorError(
                        "Registry read before readiness or outside declared bounds".into(),
                    ));
                };
                let mut descriptor = reference.clone();
                let path = std::path::Path::new(&reference.path);
                descriptor.path = path.file_name().unwrap().to_string_lossy().into();
                let result = crate::Engine
                    .replay_registry(crate::ReplayRequest {
                        artifact_root: retained.join(path.parent().unwrap()),
                        descriptor,
                    })
                    .map_err(|error| SupervisorError(error.to_string()))?;
                if result.activation != crate::Activation::Demonstrated
                    || !matches!(
                        result.completion,
                        crate::Completion::Complete | crate::Completion::Incomplete
                    )
                {
                    return Err(SupervisorError(
                        "Unavailable registry read cannot extend session lifetime".into(),
                    ));
                }
                output.send(Reply::RegistryRead { request })?;
                deadline = Instant::now() + Duration::from_secs(idle_seconds);
            }
            Ok(Input::Control(Control::Close)) => return Ok(OperationOutcome::Completed),
            Ok(event) => return Ok(interruption(event)),
            Err(RecvTimeoutError::Disconnected) => return Ok(OperationOutcome::CallerLost),
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

fn observe_worker(
    input: &Receiver<Input>,
    child: &binding::OwnedGame,
    observer: &mut binding::Observer,
    budget: Duration,
) -> Result<OperationOutcome, SupervisorError> {
    let started = Instant::now();
    loop {
        if started.elapsed() >= budget {
            return Ok(OperationOutcome::TimedOut);
        }
        if binding::conflicting_game(Some(child.pid()))? {
            return Err(SupervisorError(
                "External game invalidated isolation".into(),
            ));
        }
        if observer.poll()? {
            return Ok(OperationOutcome::Completed);
        }
        match input.recv_timeout(Duration::from_millis(50)) {
            Ok(event) => return Ok(interruption(event)),
            Err(RecvTimeoutError::Disconnected) => return Ok(OperationOutcome::CallerLost),
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

fn prepare_fixture(
    output: &Path,
    spec: &crate::operation::ObservationSpec,
) -> Result<(), SupervisorError> {
    let profile = output.join("profile");
    for relative in [
        "mod",
        "mod/atlas_early",
        "mod/atlas_early/common",
        "mod/atlas_early/common/tradition_categories",
    ] {
        binding::private_directory(&profile.join(relative))?;
    }
    fs::write(
        profile
            .join("mod/atlas_early")
            .join(crate::capture::FIXTURE_FILE),
        &spec.fixture,
    )?;
    let mod_path = profile.join("mod/atlas_early");
    let mod_path = mod_path
        .to_str()
        .filter(|path| !path.contains(['"', '\n', '\r']))
        .ok_or_else(|| {
            SupervisorError("Profile path cannot be represented in mod descriptor".into())
        })?;
    fs::write(
        profile.join("mod/atlas_early.mod"),
        format!("name=\"Native bounded observation\"\npath=\"{mod_path}\"\n"),
    )?;
    fs::write(
        profile.join("dlc_load.json"),
        r#"{"enabled_mods":["mod/atlas_early.mod"],"disabled_dlcs":[]}"#,
    )?;
    Ok(())
}

fn retain_report(reservation: &Reservation, report: &mut AttemptReport) {
    let owner = reservation
        .snapshot()
        .and_then(|snapshot| write_new(&report.output.join("owner.json"), &snapshot));
    if let Err(error) = owner {
        report.diagnostics.push(format!("owner.json: {error}"));
    }
    let capture = operation::AttemptCapture {
        version: 4,
        build: env!("PDX_NATIVE_BUILD").into(),
        report: report.clone(),
    };
    if let Err(error) = write_new(&report.output.join("capture.json"), &capture) {
        report.diagnostics.push(format!("capture.json: {error}"));
    }
    // Write the report last so a missing capture remains visible even after controller loss.
    if let Err(error) = write_new(&report.output.join("report.json"), report) {
        report.diagnostics.push(format!("report.json: {error}"));
    }
}

fn hold_outcome(hold: Duration, elapsed: Duration) -> Option<OperationOutcome> {
    if elapsed < hold {
        return None;
    }
    Some(if hold == HOLD_LIMIT {
        OperationOutcome::TimedOut
    } else {
        OperationOutcome::Completed
    })
}

fn observe(
    input: &Receiver<Input>,
    child: &binding::OwnedGame,
    started: Instant,
    hold: Duration,
) -> Result<OperationOutcome, SupervisorError> {
    loop {
        if let Some(outcome) = hold_outcome(hold, started.elapsed()) {
            return Ok(outcome);
        }
        if binding::conflicting_game(Some(child.pid()))? {
            return Err(SupervisorError(
                "External game invalidated isolation".into(),
            ));
        }
        if !child.suspended()? {
            return Err(SupervisorError("Owned game no longer suspended".into()));
        }
        match input.recv_timeout(Duration::from_millis(100)) {
            Ok(event) => return Ok(interruption(event)),
            Err(RecvTimeoutError::Disconnected) => return Ok(OperationOutcome::CallerLost),
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}
fn interruption(event: Input) -> OperationOutcome {
    match event {
        Input::Control(Control::Cancel) => OperationOutcome::Cancelled,
        #[cfg(any(test, feature = "maintainer-tools"))]
        Input::Control(Control::WorkerLost) => OperationOutcome::WorkerLost,
        _ => OperationOutcome::CallerLost,
    }
}

fn prepare_profile(output: &Path) -> Result<(), SupervisorError> {
    binding::private_directory(&output.join("profile"))?;
    fs::write(
        output.join("profile/settings.txt"),
        "graphics={size={x=640 y=360} fullScreen=no borderless=no renderer=2}\nmaster_volume=0\nmusic_volume=0\n",
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
            OperationOutcome::Completed,
            OperationOutcome::Cancelled,
            OperationOutcome::CallerLost,
            OperationOutcome::WorkerLost,
            OperationOutcome::TimedOut,
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
            let started = if expected == OperationOutcome::TimedOut {
                Instant::now() - HOLD_LIMIT
            } else {
                Instant::now()
            };
            match expected {
                OperationOutcome::Cancelled => send.send(Input::Control(Control::Cancel)).unwrap(),
                OperationOutcome::WorkerLost => {
                    // Real worker termination precedes notification; its handle never owns game/lock.
                    let mut worker = Command::new("/bin/sleep").arg("60").spawn().unwrap();
                    worker.kill().unwrap();
                    worker.wait().unwrap();
                    send.send(Input::Control(Control::WorkerLost)).unwrap();
                }
                OperationOutcome::CallerLost => {
                    drop(send);
                }
                _ => {}
            }
            let outcome = observe(
                &input,
                &child,
                started,
                if expected == OperationOutcome::Completed {
                    Duration::ZERO
                } else if expected == OperationOutcome::TimedOut {
                    HOLD_LIMIT
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
        assert_eq!(reservation.snapshot().unwrap()["state"], "Reserved");
        drop(reservation);
        assert!(reserve(root.path(), "next", output.path()).is_err());
        assert_eq!(
            fs::read_to_string(root.path().join("partial.pending")).unwrap(),
            "interrupted write"
        );
    }

    #[test]
    fn retained_report_records_capture_failure_without_overwriting_it() {
        let root = store();
        let output = store();
        let mut reservation = reserve(root.path(), "capture-failure", output.path()).unwrap();
        reservation.disposed().unwrap();
        fs::write(output.path().join("capture.json"), "existing evidence").unwrap();
        let mut report = AttemptReport {
            origin: "unqualified-candidate".into(),
            attempt: "capture-failure".into(),
            composition: "test".into(),
            outcome: OperationOutcome::Cancelled,
            disposal: OperationDisposal::NotLaunched,
            reservation_resolved: true,
            output: output.path().into(),
            diagnostics: Vec::new(),
            replay: None,
            registries: Default::default(),
        };
        retain_report(&reservation, &mut report);
        let retained: AttemptReport =
            serde_json::from_slice(&fs::read(output.path().join("report.json")).unwrap()).unwrap();
        assert_eq!(retained.diagnostics, report.diagnostics);
        assert_eq!(retained.diagnostics.len(), 1);
        assert!(retained.diagnostics[0].contains("capture.json"));
        assert_eq!(retained.disposal, OperationDisposal::NotLaunched);
        assert_eq!(
            fs::read_to_string(output.path().join("capture.json")).unwrap(),
            "existing evidence"
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

#[cfg(test)]
mod interruption_tests {
    use super::*;
    #[test]
    fn valid_holds_complete_after_their_full_window_even_with_polling_delay() {
        let hold = Duration::from_millis(29_999);
        assert_eq!(hold_outcome(hold, hold - Duration::from_millis(1)), None);
        assert_eq!(hold_outcome(hold, hold), Some(OperationOutcome::Completed));
        assert_eq!(
            hold_outcome(hold, HOLD_LIMIT + Duration::from_millis(100)),
            Some(OperationOutcome::Completed)
        );
        assert_eq!(
            hold_outcome(HOLD_LIMIT, HOLD_LIMIT),
            Some(OperationOutcome::TimedOut)
        );
    }
    #[cfg(feature = "maintainer-tools")]
    #[test]
    fn candidate_handshake_cannot_enter_the_ordinary_owner() {
        let (send, input) = mpsc::sync_channel(1);
        let mut hello = Hello::current(Authorization::Candidate);
        hello.controller = u32::MAX;
        send.send(Input::Hello(hello)).unwrap();
        let reporter = Reporter::new(std::io::sink());
        let error = handshake(&input, &reporter, Authorization::Admitted)
            .err()
            .unwrap();
        assert!(error.to_string().contains("authorization mismatch"));
    }
    #[test]
    fn prelaunch_and_running_interruptions_keep_their_cause() {
        assert_eq!(
            interruption(Input::Control(Control::WorkerLost)),
            OperationOutcome::WorkerLost
        );
        assert_eq!(
            interruption(Input::Control(Control::Cancel)),
            OperationOutcome::Cancelled
        );
        assert_eq!(interruption(Input::Lost), OperationOutcome::CallerLost);
    }
}
