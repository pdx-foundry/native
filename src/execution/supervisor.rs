//! The supervisor process: it owns the game, the debugger worker and the host reservation of one
//! session, and it reduces the worker's event stream to answers when the game is paused.
//!
//! The order of a session: handshake, request, admission, reservation, private profile, worker
//! package, suspended game, worker, pause, answers, controls, then cleanup. Cleanup always runs:
//! stop the worker, reap the game, confirm disposal and report.
use super::{
    instances::Reservation,
    owner_events::OwnerEvents,
    run_summary::{self, PausedSummary, Phase, RunRecord},
};
use crate::{
    answer::Disposal,
    binding::{self, ExecutionPlan},
    engine::operations::{
        event_stream::{self, OwnerEvent},
        loaded_modifiers,
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
fn reader(input: impl Read + Send + 'static) -> Receiver<Input> {
    let (send, receive) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let _ = forward_input(input, &send);
        let _ = send.send(Input::Lost);
    });
    receive
}
/// Forward the controller's messages until the input or the receiver fails.
fn forward_input(mut input: impl Read, send: &SyncSender<Input>) -> Result<(), SupervisorError> {
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
    let mut run = RunRecord::new(&request);
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
        if let Some(world) = &request.world {
            prepare_world_profile(&work, world)?;
        } else {
            plan.prepare_registry_profile(&work, request.fixture.as_ref(), &registries, &content)?;
        }
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
        reservation.record_game(child.identity()?);
        if !child.suspended()? {
            return Err(SupervisorError(
                "Child suspension was not established".into(),
            ));
        }
        if started.elapsed() >= SETUP_BUDGET {
            return Ok(SessionOutcome::TimedOut);
        }
        run.complete(Phase::Setup);
        events.record(OwnerEvent::GameOwnedSuspended {
            pid: u64::from(child.pid()),
            identity: serde_json::to_string(&child.identity()?)?,
        })?;
        observer.start(child.pid())?;
        events.record(OwnerEvent::WorkerStarted)?;
        run.complete(Phase::WorkerStart);
        observe_session(
            input,
            output,
            child,
            observer,
            &mut events,
            &mut run,
            &Session {
                work_directory: &work,
                attempt: &report.attempt,
                registries: request.registries.clone(),
                fixture: request.fixture.as_ref(),
                category_fields: plan.category_fields(),
                loaded_modifiers: request.loaded_modifiers.as_deref(),
                world: request.world.as_ref(),
                world_variable_scale: plan.world_variable_scale(),
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
    run.end_session();
    finish_session(
        &mut report,
        &mut reservation,
        &mut game,
        &mut observer,
        &mut events,
        || plan.integrity().map(|_| ()),
    );
    run.end_cleanup();
    if owns_work_directory {
        write_report(&work, &reservation, &mut report, &run);
    }
    Ok(report)
}

/// Stop the worker, reap the game and resolve the reservation.
fn finish_session(
    report: &mut SessionReport,
    reservation: &mut Reservation,
    game: &mut Option<binding::OwnedGame>,
    observer: &mut Option<binding::Observer>,
    events: &mut OwnerEvents,
    integrity: impl FnOnce() -> Result<(), SupervisorError>,
) {
    let mut record = |event, diagnostics: &mut Vec<String>| {
        if let Err(error) = events.record(event) {
            diagnostics.push(error.to_string());
        }
    };
    let mut worker_stopped = true;
    if let Some(observer) = observer.as_mut() {
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
    if let Some(child) = game {
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
    if let Err(error) = integrity() {
        report.outcome = SessionOutcome::Failed(error.to_string());
    }
    if worker_stopped && !matches!(report.disposal, Disposal::Unconfirmed(_)) {
        reservation.disposed();
        report.reservation_resolved = true;
    }
}

/// The fixed facts of one session that the observation loop needs.
struct Session<'a> {
    work_directory: &'a Path,
    attempt: &'a str,
    /// Internal names of the registries that the session observes.
    registries: Vec<String>,
    fixture: Option<&'a crate::FixtureRequest>,
    /// The field tokens that the bound category fixture window reads.
    category_fields: &'a [crate::protocol::observation::FixtureFieldBinding],
    /// The registries whose item keys the modifier observation reads, when it is requested.
    loaded_modifiers: Option<&'a [String]>,
    world: Option<&'a crate::WorldRequest>,
    /// The scale that every observed world variable must carry.
    world_variable_scale: Option<u64>,
    build: crate::BuildId,
    startup: Duration,
    idle: Duration,
}

/// The loaded modifier table that the worker wrote, or `None` when it wrote none.
fn read_modifier_table(work_directory: &Path) -> Result<Option<Vec<u8>>, SupervisorError> {
    let path = work_directory.join("loaded-modifiers.json");
    if !path.try_exists()? {
        return Ok(None);
    }
    let table = files::read_bounded(&path, protocol::observation::MAX_MODIFIER_TABLE)?;
    Ok(Some(table))
}

fn worker_exit_result(
    records: &[event_stream::WorkerRecord],
) -> Result<SessionOutcome, SupervisorError> {
    for record in records {
        if let event_stream::WorkerEvent::CapabilityUnavailable { reason }
        | event_stream::WorkerEvent::NativeException { reason } = &record.event
        {
            return Err(SupervisorError(reason.clone()));
        }
    }
    Ok(SessionOutcome::WorkerLost)
}

/// Wait for the pause, reduce the worker's stream once, send the answers, then serve controls
/// until the session ends.
fn observe_session(
    input: &Receiver<Input>,
    output: &Reporter,
    child: &binding::OwnedGame,
    observer: &mut binding::Observer,
    events: &mut OwnerEvents,
    run: &mut RunRecord,
    session: &Session<'_>,
) -> Result<SessionOutcome, SupervisorError> {
    let mut deadline = Instant::now() + session.startup;
    let mut answers = None;
    let mut content_loaded = false;
    let mut checking = None;
    let mut checks_started = 0;
    loop {
        if Instant::now() >= deadline {
            return Ok(SessionOutcome::TimedOut);
        }
        if binding::conflicting_game(Some(child.pid()))? {
            return Err(SupervisorError(
                "External game invalidated isolation".into(),
            ));
        }
        let worker_exited = observer.advance_worker()?;
        if worker_exited {
            let raw = files::read_bounded(
                &session.work_directory.join("raw-trace.jsonl"),
                protocol::observation::MAX_TRACE,
            )?;
            let (records, _) = event_stream::read_worker_stream(&raw, session.attempt);
            return worker_exit_result(&records);
        }
        if answers.is_none() {
            if let Some(witness) = observer.pause_witness()? {
                if witness.state != protocol::observation::PauseState::Held {
                    return Err(SupervisorError(
                        "worker is checking before admission".into(),
                    ));
                }
                child.identity()?;
                let paused_thread = witness.thread;
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
                let readiness = registry_items::readiness(
                    &records,
                    events.all(),
                    &session.registries,
                    session.loaded_modifiers.is_some(),
                )
                        .ok_or_else(|| {
                            SupervisorError(
                                "The witnesses of the registry initialization pause are missing or inconsistent"
                                    .into(),
                            )
                        })?;
                let world = match (readiness, session.world) {
                    (crate::GameReadiness::PausedDuringRegistryInitialization, Some(_)) => {
                        return Ok(SessionOutcome::TimedOut);
                    }
                    (crate::GameReadiness::PausedInWorld, Some(request)) => {
                        let raw = files::read_bounded(
                            &session.work_directory.join("world.json"),
                            4 * 1024 * 1024,
                        )?;
                        Some(crate::engine::operations::world::answer(
                            serde_json::from_slice(&raw)?,
                            request,
                            session.world_variable_scale,
                            session.attempt,
                            child.pid(),
                            paused_thread,
                            session.build.clone(),
                        )?)
                    }
                    (crate::GameReadiness::PausedInWorld, None) | (_, Some(_)) => {
                        return Err(SupervisorError(
                            "world pause and prepared observation disagree".into(),
                        ));
                    }
                    (_, None) => None,
                };
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
                        session.category_fields,
                        &records,
                        events.all(),
                        session.build.clone(),
                    )
                });
                let modifiers = match session.loaded_modifiers {
                    Some(registries) => {
                        let table = read_modifier_table(session.work_directory)?;
                        let reduced = loaded_modifiers::reduce(
                            &records,
                            events.all(),
                            table.as_deref(),
                            session.attempt,
                            registries,
                        );
                        Some(reduced.map_err(|reason| crate::Error::Observation {
                            operation: crate::Operation::LoadedModifiers,
                            reason,
                        }))
                    }
                    None => None,
                };
                run.paused(PausedSummary::new(
                    &registries,
                    fixture.as_ref(),
                    modifiers.as_ref(),
                ));
                content_loaded = readiness == crate::GameReadiness::PausedAfterContentLoad;
                output.send(Reply::Paused {
                    readiness,
                    world: Box::new(world),
                    fixture: Box::new(fixture),
                    modifiers: Box::new(modifiers),
                    registries: registries.clone(),
                })?;
                run.complete(Phase::AwaitingPause);
                answers = Some(registries);
                deadline = Instant::now() + session.idle;
            }
        } else {
            child.identity()?;
            let witness = observer
                .pause_witness_before(deadline)?
                .ok_or_else(|| SupervisorError("Session pause witness lost".into()))?;
            match (checking, witness.state) {
                (_, protocol::observation::PauseState::Failed(reason)) => {
                    return Err(SupervisorError(reason));
                }
                (None, protocol::observation::PauseState::Held) => {}
                (Some((_, check)), protocol::observation::PauseState::Checking(active))
                    if check == active => {}
                (Some((request, check)), protocol::observation::PauseState::Held) => {
                    if let Some(result) = observer.check_reply(check)? {
                        if Instant::now() >= deadline {
                            return Ok(SessionOutcome::TimedOut);
                        }
                        output.send(Reply::ScriptChecked {
                            request,
                            result: Ok(result.answer(session.build.clone())),
                        })?;
                        checking = None;
                        deadline = Instant::now() + session.idle;
                    }
                }
                _ => return Err(SupervisorError("unexpected worker check state".into())),
            }
        }
        match input.recv_timeout(Duration::from_millis(50)) {
            Ok(Input::Control(Control::CheckScript {
                input,
                request,
                durations,
            })) => {
                if answers.is_none() || checking.is_some() {
                    return Err(SupervisorError(
                        "script check requested outside an idle pause".into(),
                    ));
                }
                if !content_loaded {
                    output.send(Reply::ScriptChecked {
                        request,
                        result: Err(crate::Error::ScriptRequest {
                            reason: "script checks require a confirmed content-loaded pause".into(),
                        }),
                    })?;
                    continue;
                }
                let check = checks_started + 1;
                match observer.prepare_check(check, &input, durations) {
                    Ok(prepared) => {
                        deadline =
                            Instant::now() + Duration::from_secs(crate::script::CHECK_SECONDS);
                        observer.start_check(&prepared)?;
                        checks_started = check;
                        checking = Some((request, check));
                    }
                    Err(error) => output.send(Reply::ScriptChecked {
                        request,
                        result: Err(error),
                    })?,
                }
            }
            Ok(Input::Control(Control::ReadWorld { request })) => {
                if answers.is_none() || session.world.is_none() || checking.is_some() {
                    return Err(SupervisorError(
                        "world read outside an idle world pause".into(),
                    ));
                }
                output.send(Reply::ObservationRead { request })?;
                deadline = Instant::now() + session.idle;
            }
            Ok(Input::Control(Control::ReadRegistry { name, request })) => {
                if checking.is_some() {
                    return Err(SupervisorError("read during script check".into()));
                }
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
                if checking.is_some() {
                    return Err(SupervisorError("read during script check".into()));
                }
                if answers.is_none() || session.fixture.is_none() {
                    return Err(SupervisorError(
                        "Fixture read without a prepared fixture at the pause".into(),
                    ));
                }
                output.send(Reply::ObservationRead { request })?;
                deadline = Instant::now() + session.idle;
            }
            Ok(Input::Control(Control::ReadModifiers { request })) => {
                if checking.is_some() {
                    return Err(SupervisorError("read during script check".into()));
                }
                if answers.is_none() || session.loaded_modifiers.is_none() {
                    return Err(SupervisorError(
                        "Modifier read without a requested modifier table at the pause".into(),
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

/// Leave the owner details, the report and the run summary in the work directory. Native keeps
/// the directory after a failure, and a caller that was lost never received the report.
fn write_report(
    work_directory: &Path,
    reservation: &Reservation,
    report: &mut SessionReport,
    run: &RunRecord,
) {
    let owner = reservation
        .snapshot()
        .and_then(|snapshot| files::write_json(&work_directory.join("owner.json"), &snapshot));
    if let Err(error) = owner {
        report.diagnostics.push(format!("owner.json: {error}"));
        report.reservation_resolved = false;
        report.outcome = SessionOutcome::Failed("Session bookkeeping failed".into());
    }
    // The report names every bookkeeping failure before it.
    if let Err(error) = files::write_json(&work_directory.join("report.json"), report) {
        report.diagnostics.push(format!("report.json: {error}"));
        report.reservation_resolved = false;
        report.outcome = SessionOutcome::Failed("Session bookkeeping failed".into());
    }
    // The summary follows the final report. It is a developer aid, so its own failure must not
    // change what `close` returns.
    if let Err(error) = run_summary::write(work_directory, report, run) {
        eprintln!("{}: {error}", run_summary::FILE);
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

/// Copy a bounded save into the private profile and select the engine's continue-save route.
fn prepare_world_profile(
    work: &Path,
    request: &crate::WorldRequest,
) -> Result<(), SupervisorError> {
    let raw = files::read_bounded(&request.save, 16 * 1024 * 1024)?;
    let profile = work.join("profile");
    let save_root = profile.join("save games");
    binding::private_directory(&save_root)?;
    let saves = save_root.join("native_world");
    binding::private_directory(&saves)?;
    files::write_new(&saves.join("fixture.sav"), &raw)?;
    files::write_json(
        &profile.join("continue_game.json"),
        &serde_json::json!({
            "title": "save games/native_world/fixture",
            "desc": "Native prepared world",
            "date": ""
        }),
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

    #[test]
    fn world_profile_preserves_source_and_supplies_every_continue_metadata_string() {
        let root = store();
        let source = root.path().join("source.sav");
        fs::write(&source, b"game-produced save").unwrap();
        let request = crate::WorldRequest {
            save: source.clone(),
            country: "Earth".into(),
            effect: String::new(),
            days: 0,
            flags: vec![],
            variables: vec![],
        };
        prepare_profile(root.path()).unwrap();
        prepare_world_profile(root.path(), &request).unwrap();
        let profile = root.path().join("profile");
        assert_eq!(fs::read(&source).unwrap(), b"game-produced save");
        assert_eq!(
            fs::read(profile.join("save games/native_world/fixture.sav")).unwrap(),
            fs::read(&source).unwrap()
        );
        let metadata: serde_json::Value =
            serde_json::from_slice(&fs::read(profile.join("continue_game.json")).unwrap()).unwrap();
        for key in ["title", "desc", "date"] {
            assert!(
                metadata[key].is_string(),
                "continue metadata requires {key}"
            );
        }
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
        reservation.record_game(child.identity().unwrap());
        assert!(child.suspended().unwrap());
        child.dispose(DISPOSAL_BUDGET).unwrap();
        assert!(binding::process_identity(child.pid()).is_err());
        reservation.disposed();
        assert_eq!(reservation.snapshot().unwrap()["state"], "disposed");
    }

    /// One session with a suspended stand-in game and a shell worker, from the worker's start to
    /// the report. The caller closes the session when it pauses.
    struct FakeSession {
        output: tempfile::TempDir,
        report: SessionReport,
        replies: Vec<Reply>,
        game_pid: u32,
        worker_exit: Option<i64>,
    }

    impl FakeSession {
        fn run(rows: &[serde_json::Value], worker: &str) -> Self {
            Self::controlled(rows, worker, close_at_pause, Duration::from_secs(3))
        }

        fn controlled(
            rows: &[serde_json::Value],
            worker: &str,
            controller: impl FnOnce(std::os::unix::net::UnixStream, SyncSender<Input>) -> Vec<Reply>
            + Send
            + 'static,
            idle: Duration,
        ) -> Self {
            use std::os::unix::net::UnixStream;

            let _guard = binding::LIFECYCLE_TEST_LOCK.lock().unwrap();
            let root = store();
            let output = store();
            let registry = "common/traditions";
            let mut reservation = reserve(root.path(), "unit", output.path()).unwrap();
            let child = binding::test_child(output.path()).unwrap();
            let game_pid = child.pid();
            reservation.record_game(child.identity().unwrap());
            assert!(child.suspended().unwrap());
            let mut game = Some(child);

            write_trace(output.path(), rows, game_pid);
            let mut observer = Some(shell_worker(output.path(), game_pid, worker, registry));
            let mut run = RunRecord::new(&SessionRequest::test());
            run.complete(Phase::Setup);
            let mut events = OwnerEvents::new(output.path());
            events
                .record(OwnerEvent::GameOwnedSuspended {
                    pid: u64::from(game_pid),
                    identity: serde_json::to_string(&game.as_ref().unwrap().identity().unwrap())
                        .unwrap(),
                })
                .unwrap();
            events.record(OwnerEvent::WorkerStarted).unwrap();
            run.complete(Phase::WorkerStart);
            let (writer, reader) = UnixStream::pair().unwrap();
            let reporter = Reporter::new(writer);
            let (send, receive) = mpsc::sync_channel(1);
            let controller = thread::spawn(move || controller(reader, send));
            let session = Session {
                work_directory: output.path(),
                attempt: "unit",
                registries: vec![registry.into()],
                fixture: None,
                category_fields: &[],
                loaded_modifiers: worker.contains("SDK_CHECK_MODE").then_some(&[][..]),
                world: None,
                world_variable_scale: None,
                build: crate::BuildId("unit".into()),
                startup: Duration::from_secs(3),
                idle,
            };
            let outcome = observe_session(
                &receive,
                &reporter,
                game.as_ref().unwrap(),
                observer.as_mut().unwrap(),
                &mut events,
                &mut run,
                &session,
            )
            .unwrap_or_else(|error| SessionOutcome::Failed(error.to_string()));
            run.end_session();
            let mut report = SessionReport {
                attempt: "unit".into(),
                outcome,
                disposal: Disposal::NotApplicable,
                reservation_resolved: false,
                diagnostics: Vec::new(),
            };
            finish_session(
                &mut report,
                &mut reservation,
                &mut game,
                &mut observer,
                &mut events,
                || Ok(()),
            );
            run.end_cleanup();
            write_report(output.path(), &reservation, &mut report, &run);
            reporter
                .send(Reply::Finished(Box::new(report.clone())))
                .unwrap();
            reporter.finish().unwrap();
            let replies = controller.join().unwrap();

            Self {
                output,
                report,
                replies,
                game_pid,
                worker_exit: observer.as_ref().unwrap().exited,
            }
        }

        fn summary(&self) -> serde_json::Value {
            let summary = fs::read(self.output.path().join(run_summary::FILE)).unwrap();

            serde_json::from_slice(&summary).unwrap()
        }
    }

    /// The worker's stream: these rows, numbered from 1, on the launch thread of `game_pid`.
    fn write_trace(directory: &Path, rows: &[serde_json::Value], game_pid: u32) {
        let mut trace = Vec::new();
        for (index, row) in rows.iter().enumerate() {
            let mut row = row.clone();
            row["seq"] = serde_json::json!(index + 1);
            row["run"] = serde_json::json!("unit");
            row["thread"] = serde_json::json!(7);
            if row["kind"] == "launch-stopped" {
                row["pid"] = serde_json::json!(game_pid);
            }
            serde_json::to_writer(&mut trace, &row).unwrap();
            trace.push(b'\n');
        }
        fs::write(directory.join("raw-trace.jsonl"), trace).unwrap();
    }

    /// A shell worker that says hello, waits for the resume grant, then runs `after_resume`.
    fn shell_worker(
        directory: &Path,
        game_pid: u32,
        after_resume: &str,
        registry: &str,
    ) -> binding::Observer {
        for name in [
            "worker.stdout",
            "worker.stderr",
            "game.stdout",
            "game.stderr",
        ] {
            if !directory.join(name).exists() {
                fs::write(directory.join(name), []).unwrap();
            }
        }
        let script = format!(
            r#"
printf '{{"version":"%s","attempt":"unit","game":%s,"worker":%s,"target":"target","source_hashes":{{}},"python":"python","lldb":"lldb","module":"module"}}' "$NATIVE_VERSION" "$GAME_PID" "$$" > hello.json.pending
mv hello.json.pending hello.json
while [ ! -f resume-granted.json ]; do sleep 0.01; done
{after_resume}
"#
        );
        let mut command = Command::new("/bin/sh");
        command
            .arg("-c")
            .arg(script)
            .current_dir(directory)
            .env("NATIVE_VERSION", crate::protocol::observation::VERSION)
            .env("GAME_PID", game_pid.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        binding::test_observer(directory, &mut command, game_pid, registry)
    }

    /// The caller: close the session when it pauses, and keep every reply until the report.
    fn close_at_pause(
        mut reader: std::os::unix::net::UnixStream,
        controls: SyncSender<Input>,
    ) -> Vec<Reply> {
        let mut replies = Vec::new();
        loop {
            let reply: Reply = protocol::read(&mut reader).unwrap();
            let finished = matches!(reply, Reply::Finished(_));
            if matches!(reply, Reply::Paused { .. }) {
                controls.send(Input::Control(Control::Close)).unwrap();
            }
            replies.push(reply);
            if finished {
                return replies;
            }
        }
    }

    #[test]
    fn script_checks_use_their_own_deadline_and_dispose_every_failed_session() {
        for (mode, count, delay, cancel) in [
            ("early", 1, 0.0, false),
            ("normal", 30, 0.0, false),
            ("normal", 2, 4.2, false),
            ("stuck", 1, 0.0, false),
            ("register-mismatch", 1, 0.0, false),
            ("worker-loss", 1, 0.0, false),
            ("stuck", 1, 0.0, true),
        ] {
            let mut rows = requested_hook_rows();
            rows.extend([
                serde_json::json!({"kind":"registry-load-returned","name":"common/traditions","owner":"0x1000"}),
                serde_json::json!({"kind":"registry-snapshot","name":"common/traditions","directory":"common/traditions","owner":"0x1000","count":0}),
                serde_json::json!({"kind":"registry-end","name":"common/traditions","owner":"0x1000","count":0,"producerLastSequence":8}),
                serde_json::json!({"kind":"session-paused","returned":["common/traditions"],"cause":"loaders-returned"}),
            ]);
            if mode != "early" {
                rows.pop();
                rows.push(serde_json::json!({"kind":"modifier-documentation-entered"}));
                rows.push(serde_json::json!({"kind":"session-paused","returned":["common/traditions"],"cause":"content-loaded"}));
            }
            let worker = format!(
                "SDK_CHECK_MODE={mode} SDK_CHECK_DELAY={delay} exec python3 - <<'PY'\n{}\nPY",
                include_str!("test_script_worker.py")
            );
            let session = FakeSession::controlled(
                &rows,
                &worker,
                move |mut reader, controls| {
                    let mut replies = Vec::new();
                    let mut sent = 0;
                    loop {
                        let reply: Reply = protocol::read(&mut reader).unwrap();
                        let next =
                            matches!(reply, Reply::Paused { .. } | Reply::ScriptChecked { .. });
                        if let Reply::ScriptChecked { result, .. } = &reply {
                            if mode == "early" {
                                assert!(matches!(result, Err(crate::Error::ScriptRequest { .. })));
                            } else {
                                assert!(result.is_ok(), "{mode}: {result:?}");
                            }
                        }
                        if next && sent < count {
                            sent += 1;
                            controls
                                .send(Input::Control(Control::CheckScript {
                                    request: sent,
                                    durations: Vec::new(),
                                    input: crate::ScriptCheck {
                                        kind: crate::DeclarationKind::Trigger,
                                        scope: crate::ScopeId("test-scope".into()),
                                        text: "always = yes".into(),
                                    },
                                }))
                                .unwrap();
                            if cancel {
                                thread::sleep(Duration::from_millis(200));
                                controls.send(Input::Control(Control::Cancel)).unwrap();
                            }
                        } else if next {
                            controls.send(Input::Control(Control::Close)).unwrap();
                        }
                        let finished = matches!(reply, Reply::Finished(_));
                        replies.push(reply);
                        if finished {
                            break;
                        }
                    }
                    replies
                },
                Duration::from_secs(1),
            );
            assert_eq!(session.report.disposal, Disposal::Confirmed, "{mode}");
            assert!(session.report.reservation_resolved, "{mode}");
            assert!(
                binding::process_identity(session.game_pid).is_err(),
                "{mode}"
            );
            if mode == "early" {
                assert_eq!(session.report.outcome, SessionOutcome::Completed);
                assert!(!session.output.path().join("check-started").exists());
            } else if mode == "normal" {
                assert_eq!(session.report.outcome, SessionOutcome::Completed);
                assert_eq!(
                    session
                        .replies
                        .iter()
                        .filter(|reply| matches!(reply, Reply::ScriptChecked { .. }))
                        .count(),
                    count as usize
                );
            } else {
                assert_ne!(session.report.outcome, SessionOutcome::Completed, "{mode}");
                assert!(
                    !session
                        .replies
                        .iter()
                        .any(|reply| matches!(reply, Reply::ScriptChecked { .. })),
                    "{mode}"
                );
            }
        }
    }

    fn requested_hook_rows() -> Vec<serde_json::Value> {
        vec![
            serde_json::json!({"kind":"hooks-requested","hooks":["registry:common/traditions"]}),
            serde_json::json!({"kind":"launch-stopped","error":"success","pid":0,"triple":"arm64-test","frames":[{"function":"_dyld_start"}]}),
            serde_json::json!({"kind":"hooks-active-before-resume","hooks":{
                "registry:common/traditions":{"enabled":true,"locations":1,"resolved":1,"hits":0}}}),
            serde_json::json!({"kind":"resume","error":"success"}),
            serde_json::json!({"kind":"registry-load-start","name":"common/traditions","directory":"common/traditions","owner":"0x1000"}),
        ]
    }

    #[test]
    fn fake_worker_session_reports_a_paused_answer_and_reaps_both_processes() {
        let registry = "common/traditions";
        let mut rows = requested_hook_rows();
        rows.extend([
            serde_json::json!({"kind":"registry-load-returned","name":registry,"owner":"0x1000"}),
            serde_json::json!({"kind":"registry-snapshot","name":registry,"directory":registry,"owner":"0x1000","count":0}),
            serde_json::json!({"kind":"registry-end","name":registry,"owner":"0x1000","count":0,"producerLastSequence":8}),
            serde_json::json!({"kind":"session-paused","returned":[registry],"cause":"loaders-returned"}),
        ]);
        let session = FakeSession::run(
            &rows,
            r#"
printf '{"attempt":"unit","game":%s,"worker":%s,"thread":7,"returned":["common/traditions"],"generation":0,"state":"held"}' "$GAME_PID" "$$" > session-paused.json.pending
mv session-paused.json.pending session-paused.json
while [ ! -f pause-check.json ]; do sleep 0.01; done
printf '{"attempt":"unit","game":%s,"worker":%s,"thread":7,"returned":["common/traditions"],"generation":1,"state":"held"}' "$GAME_PID" "$$" > session-paused.json.pending
mv session-paused.json.pending session-paused.json
exec sleep 30
"#,
        );
        let report = &session.report;
        assert_eq!(report.outcome, SessionOutcome::Completed);
        assert_eq!(report.disposal, Disposal::Confirmed);
        assert!(report.reservation_resolved);
        assert_eq!(session.worker_exit, Some(-9));
        assert!(binding::process_identity(session.game_pid).is_err());
        let saved: SessionReport =
            serde_json::from_slice(&fs::read(session.output.path().join("report.json")).unwrap())
                .unwrap();
        assert_eq!(saved.outcome, SessionOutcome::Completed);
        assert_eq!(saved.disposal, Disposal::Confirmed);
        let [paused, finished] = session.replies.as_slice() else {
            panic!(
                "expected a pause and a report: {} replies",
                session.replies.len()
            );
        };
        let Reply::Paused {
            readiness,
            registries,
            ..
        } = paused
        else {
            panic!(
                "expected pause answer: {}",
                serde_json::to_value(paused).unwrap()
            )
        };
        assert_eq!(
            *readiness,
            crate::GameReadiness::PausedAfterRegistryInitialization
        );
        assert_eq!(registries[registry].observed, Observed::Complete);
        assert!(registries[registry].items.is_empty());
        assert!(matches!(finished, Reply::Finished(_)));

        let summary = session.summary();
        let states: Vec<_> = summary["timing"]["phases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|phase| phase["state"].as_str().unwrap())
            .collect();
        assert_eq!(states, ["completed"; 4]);
        assert_eq!(summary["timing"]["last_completed_phase"], "paused");
        assert!(summary["timing"]["cleanup_milliseconds"].is_u64());
        assert_eq!(summary["outcome"], "Completed");
        assert_eq!(summary["worker"]["stream"], "intact");
        assert_eq!(summary["worker"]["last_record"]["kind"], "session-paused");
        assert_eq!(
            summary["hooks"]["reported"]["installed"],
            serde_json::json!(["registry:common/traditions"])
        );
        assert_eq!(
            summary["observations"]["registries"]["observed"][registry]["observed"],
            "Complete"
        );
        assert_eq!(summary["observations"]["fixture"], "not-requested");
    }

    #[test]
    fn a_worker_lost_before_the_pause_leaves_its_last_phase_and_record() {
        let session = FakeSession::run(&requested_hook_rows(), "exit 3");
        assert_eq!(session.report.outcome, SessionOutcome::WorkerLost);
        assert_eq!(session.report.disposal, Disposal::Confirmed);
        assert!(matches!(session.replies.as_slice(), [Reply::Finished(_)]));

        let summary = session.summary();
        let phases = summary["timing"]["phases"].as_array().unwrap();
        assert_eq!(phases[2]["phase"], "awaiting-pause");
        assert_eq!(phases[2]["state"], "interrupted");
        assert!(phases[2]["milliseconds"].is_u64());
        assert_eq!(phases[3]["state"], "not-reached");
        assert!(phases[3].get("milliseconds").is_none());
        assert_eq!(summary["timing"]["last_completed_phase"], "worker-start");
        assert!(summary["timing"]["cleanup_milliseconds"].is_u64());
        assert_eq!(summary["outcome"], "WorkerLost");
        assert_eq!(
            summary["worker"]["last_record"]["kind"],
            "registry-load-start"
        );
        assert_eq!(
            summary["observations"]["registries"]["unavailable"],
            "the session ended before its pause"
        );
        assert_eq!(summary["observations"]["modifiers"], "not-requested");
    }

    #[test]
    fn engine_failure_context_survives_worker_exit_and_confirmed_disposal() {
        let checkpoint = serde_json::json!({
            "attempt": "unit", "game": 9, "phase": "world-observation", "context": "world day 0",
            "thread": 7, "operation": "allocate-memory", "details": {},
            "elapsed_milliseconds": 400, "deadline_milliseconds": 500,
            "last_attempted_call": {"ordinal": 2, "operation": "country_name"},
            "last_completed_call": {"ordinal": 1, "operation": "local_human"},
            "hooks": [], "failure": {"kind": "allocation", "reason": "invalid allocation address",
                "details": {"size": "64", "address": "0xffffffffffffffff", "debugger_error": "success"}}
        });
        let worker = format!("cat > worker-diagnostics.json <<'JSON'\n{checkpoint}\nJSON\nexit 1");
        let session = FakeSession::run(&[], &worker);
        assert_eq!(session.report.disposal, Disposal::Confirmed);
        assert_eq!(session.report.outcome, SessionOutcome::WorkerLost);
        let summary = session.summary();
        assert_eq!(
            summary["worker_diagnostics"]["reason"],
            "allocation: invalid allocation address"
        );
        assert_eq!(
            summary["worker_diagnostics"]["context"]["failure"]["details"]["debugger_error"],
            "success"
        );
        assert!(binding::process_identity(session.game_pid).is_err());
    }

    #[test]
    fn worker_loss_during_a_call_retains_progress_without_inventing_a_cause() {
        let checkpoint = serde_json::json!({
            "attempt": "unit", "game": 9, "phase": "script-check", "context": "check 1",
            "thread": 7, "operation": "read", "details": {"expected_pc": "0x1000"},
            "elapsed_milliseconds": 20, "deadline_milliseconds": 5000,
            "last_attempted_call": {"ordinal": 2, "operation": "read"},
            "last_completed_call": {"ordinal": 1, "operation": "constructor"},
            "hooks": [], "failure": null
        });
        let worker = format!("cat > worker-diagnostics.json <<'JSON'\n{checkpoint}\nJSON\nexit 1");
        let session = FakeSession::run(&[], &worker);
        assert_eq!(session.report.outcome, SessionOutcome::WorkerLost);
        assert_eq!(session.report.disposal, Disposal::Confirmed);
        let summary = session.summary();
        assert_eq!(
            summary["worker_diagnostics"]["reason"],
            "cause unavailable; last witnessed operation: read"
        );
        assert!(summary["worker_diagnostics"]["context"]["failure"].is_null());
        assert_eq!(
            summary["worker_diagnostics"]["context"]["last_completed_call"]["operation"],
            "constructor"
        );
    }

    #[test]
    fn an_attach_timeout_reaches_the_session_report_and_disposes_the_game() {
        let session = FakeSession::run(
            &[serde_json::json!({
                "seq": 1, "run": "unit", "kind": "capability-unavailable",
                "reason": "debugger attach timed out after 15 seconds",
            })],
            "exit 1",
        );
        assert_eq!(
            session.report.outcome,
            SessionOutcome::Failed("debugger attach timed out after 15 seconds".into())
        );
        assert_eq!(session.report.disposal, Disposal::Confirmed);
        assert!(session.report.reservation_resolved);
        assert!(binding::process_identity(session.game_pid).is_err());
    }

    #[test]
    fn the_summary_follows_a_report_that_could_not_be_written() {
        let _guard = binding::LIFECYCLE_TEST_LOCK.lock().unwrap();
        let root = store();
        let output = store();
        let mut reservation = reserve(root.path(), "report", output.path()).unwrap();
        reservation.disposed();
        fs::write(output.path().join("report.json"), "existing file").unwrap();
        let mut report = SessionReport {
            attempt: "report".into(),
            outcome: SessionOutcome::Completed,
            disposal: Disposal::Confirmed,
            reservation_resolved: true,
            diagnostics: Vec::new(),
        };
        write_report(
            output.path(),
            &reservation,
            &mut report,
            &RunRecord::new(&SessionRequest::test()),
        );
        let summary: serde_json::Value =
            serde_json::from_slice(&fs::read(output.path().join(run_summary::FILE)).unwrap())
                .unwrap();

        assert!(!report.reservation_resolved);
        assert_eq!(
            summary["outcome"],
            serde_json::json!({"Failed": "Session bookkeeping failed"})
        );
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
        next.disposed();
        assert_eq!(next.snapshot().unwrap()["state"], "disposed");
    }

    #[test]
    fn the_report_names_a_file_that_could_not_be_written_and_replaces_nothing() {
        let _guard = binding::LIFECYCLE_TEST_LOCK.lock().unwrap();
        let root = store();
        let output = store();
        let mut reservation = reserve(root.path(), "report", output.path()).unwrap();
        reservation.disposed();
        fs::write(output.path().join("owner.json"), "existing file").unwrap();
        let mut report = SessionReport {
            attempt: "report".into(),
            outcome: SessionOutcome::Cancelled,
            disposal: Disposal::NotApplicable,
            reservation_resolved: true,
            diagnostics: Vec::new(),
        };
        write_report(
            output.path(),
            &reservation,
            &mut report,
            &RunRecord::new(&SessionRequest::test()),
        );
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
        let summary: serde_json::Value =
            serde_json::from_slice(&fs::read(output.path().join(run_summary::FILE)).unwrap())
                .unwrap();
        assert_eq!(
            summary["outcome"],
            serde_json::json!({"Failed": "Session bookkeeping failed"})
        );
        assert_eq!(
            summary["worker"]["stream"]["unavailable"],
            "the worker stream was never created"
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
