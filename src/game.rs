//! `Game`: one supervised game session, or its stand-in over recorded answers.
//!
//! An independent thread (`driver`) talks to the supervisor process, so no async runtime owns
//! the game. The supervisor sends the answers once, when the game is paused; every
//! `registry_items` call returns from them.
use crate::{
    Disposal, Error, GameReadiness,
    engine::operations::registry_items::{Observed, RegistryItems},
    protocol::session::{
        Fault, ObservationControl, ObservationTarget, SessionOutcome, SessionRequest,
    },
};
use std::sync::atomic::{AtomicU8, Ordering};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, mpsc},
};
use tokio::sync::{oneshot, watch};

mod driver;

/// How to start a game. The consumer supplies the supervisor process; Native supplies the rest.
#[derive(Debug)]
pub struct GameOptions {
    pub(crate) supervisor: Command,
    /// Seconds allowed for the game to reach its pause, 1 to 180. The default is 180.
    pub startup_seconds: u64,
    /// Seconds that a paused game may stay idle, 1 to 180. Each answer restarts it. The default
    /// is 180.
    pub idle_seconds: u64,
    /// A deliberate fault and the observation that receives it.
    pub(crate) fault: Option<Fault>,
    pub(crate) fixture: Option<crate::FixtureRequest>,
    pub(crate) registries: Option<Vec<String>>,
    pub(crate) loaded_modifiers: bool,
}
impl GameOptions {
    /// `supervisor` starts a dedicated process that calls `supervisor::serve` on its standard
    /// input and output, then exits. It must link the same Native build as the caller.
    pub fn new(supervisor: Command) -> Self {
        Self {
            supervisor,
            startup_seconds: crate::protocol::session::MAX_SESSION_SECONDS,
            idle_seconds: crate::protocol::session::MAX_SESSION_SECONDS,
            fault: None,
            fixture: None,
            registries: None,
            loaded_modifiers: false,
        }
    }
    /// Run the game on until all content has loaded, and read the loaded modifier table where
    /// the engine documents its modifiers. The game pauses there, before its main menu, and
    /// `Game::loaded_modifiers` answers from that table. The selected registries are still
    /// observed; one whose initial loader does not run before that point is not loaded.
    pub fn loaded_modifiers(mut self) -> Self {
        self.loaded_modifiers = true;
        self
    }
    /// Prepare one fixed fixture before launch. Registry queries observe this mounted content.
    pub fn fixture(mut self, request: crate::FixtureRequest) -> Self {
        self.fixture = Some(request);
        self
    }
    /// Select the content directories whose initial loads this session observes. Use names from
    /// `Native::registries`. Choose one or more unique names before `start_game`; an empty,
    /// duplicate or unknown selection is refused. Include the fixture's registry when using
    /// `fixture`. Without a selection, the build's defaults are observed.
    pub fn registries<I, S>(mut self, names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.registries = Some(
            names
                .into_iter()
                .map(|name| name.into().trim_end_matches('/').into())
                .collect(),
        );
        self
    }
    /// Inject a fault into a selected observation for Native's live tests. Fixture faults
    /// require a prepared fixture; modifier faults support only `WorkerLoss`.
    #[doc(hidden)]
    pub fn fault(mut self, target: ObservationTarget, control: ObservationControl) -> Self {
        let target = match target {
            ObservationTarget::Registry(directory) => {
                ObservationTarget::Registry(directory.trim_end_matches('/').into())
            }
            target => target,
        };
        self.fault = Some(Fault { target, control });
        self
    }
}

/// What `start` needs besides the request: the caller's names and where answers go.
pub(crate) struct Session {
    /// Content directory of each observed registry, with its internal name.
    pub observed: std::collections::BTreeSet<String>,
    pub build: crate::BuildId,
    /// Write every answer to this directory as it is returned.
    pub recorder: Option<Arc<PathBuf>>,
    /// Temporary directory that Native made for this session.
    pub work: PathBuf,
    pub fixture: Option<crate::FixtureRequest>,
    /// The static side of the loaded modifier join, when the session reads the table.
    pub modifiers: Option<crate::session::ModifierJoin>,
}

/// What the supervisor established when the game paused.
#[derive(Debug, Clone)]
struct Paused {
    readiness: GameReadiness,
    /// By internal registry name.
    registries: BTreeMap<String, RegistryItems>,
    fixture: Option<Result<crate::Answer<crate::FixtureObservation>, Error>>,
    modifiers:
        Option<Result<crate::engine::operations::loaded_modifiers::ObservedModifiers, Error>>,
}

/// How a session ended, from the supervisor's final report.
#[derive(Debug, Clone)]
struct Finished {
    outcome: SessionOutcome,
    disposal: Disposal,
    reservation_resolved: bool,
    diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct State {
    paused: Option<Paused>,
    /// `Err` when the connection to the supervisor failed; disposal is then not established.
    finished: Option<Result<Finished, Error>>,
}
enum ReadQuestion {
    Registry(String),
    Fixture,
    Modifiers,
}
enum DriverCommand {
    /// The caller answered a question about this registry; the idle time starts again.
    Read {
        question: ReadQuestion,
        reply: oneshot::Sender<Result<(), Error>>,
    },
}

#[derive(Debug)]
enum GameBackend {
    Live { recorder: Option<Arc<PathBuf>> },
    Recorded(Arc<crate::recorded::Answers>),
}

/// An owned process paused at registry initialization, or after content loads with
/// `GameOptions::loaded_modifiers`; never a loaded world.
/// Drop requests cleanup. Await close for independently confirmed disposal.
#[derive(Debug)]
pub struct Game {
    commands: Option<mpsc::SyncSender<DriverCommand>>,
    stop: Arc<AtomicU8>,
    state: watch::Receiver<State>,
    paused: Paused,
    closing: bool,
    /// Content directory of each observed registry, with its internal name.
    observed: std::collections::BTreeSet<String>,
    build: crate::BuildId,
    backend: GameBackend,
    /// Temporary work directory that Native made. `close` removes it only after a clean,
    /// confirmed session with no read error; otherwise it is kept for inspection.
    work: Option<PathBuf>,
    /// A read returned an error, other than `Error::Closed` after the caller began to close, so
    /// `close` keeps the work directory.
    read_failed: bool,
    fixture: Option<crate::FixtureRequest>,
    /// The joined loaded modifier answer, when the session reads the table.
    modifiers: Option<Result<crate::Answer<crate::LoadedModifiers>, Error>>,
}
impl Game {
    /// Return the modifiers that the engine holds after all content has loaded, with their
    /// loaded category tags.
    ///
    /// Each entry says whether the executable declares its name (`Native::modifiers`) and which
    /// family of `Native::modifier_families` gives it for a loaded item. The table is read once,
    /// where the engine documents its modifiers; every call returns it and refreshes the idle
    /// timeout, and the game is never resumed. Request it with `GameOptions::loaded_modifiers`
    /// before `Native::start_game`; otherwise this is `Error::Unsupported`.
    pub async fn loaded_modifiers(
        &mut self,
    ) -> Result<crate::Answer<crate::LoadedModifiers>, Error> {
        let result = self.answer_loaded_modifiers().await;
        self.keep_on_error(result)
    }

    async fn answer_loaded_modifiers(
        &mut self,
    ) -> Result<crate::Answer<crate::LoadedModifiers>, Error> {
        if self.closing || self.state.borrow().finished.is_some() {
            return Err(Error::Closed);
        }
        let subject = self
            .fixture
            .as_ref()
            .map(crate::FixtureRequest::recorded_subject);
        self.answer("loaded_modifiers", subject.as_deref(), async |game| {
            game.loaded_modifiers_from_game().await
        })
        .await
    }

    async fn loaded_modifiers_from_game(
        &mut self,
    ) -> Result<crate::Answer<crate::LoadedModifiers>, Error> {
        let answer = self.modifiers.clone().ok_or_else(|| Error::Unsupported {
            operation: crate::Operation::LoadedModifiers,
            reason: "this session does not read the loaded modifier table; use GameOptions::loaded_modifiers before start_game".into(),
        })?;
        self.restart_idle_time(ReadQuestion::Modifiers).await?;
        answer
    }

    /// Return the prepared fixture's read-entry observations. Every call uses the same startup
    /// observation and refreshes the idle timeout; it never resumes the game. Set the fixture
    /// with `GameOptions::fixture` before calling `Native::start_game`.
    pub async fn observe_fixture(
        &mut self,
    ) -> Result<crate::Answer<crate::FixtureObservation>, Error> {
        let result = self.answer_observe_fixture().await;
        self.keep_on_error(result)
    }

    async fn answer_observe_fixture(
        &mut self,
    ) -> Result<crate::Answer<crate::FixtureObservation>, Error> {
        if self.closing || self.state.borrow().finished.is_some() {
            return Err(Error::Closed);
        }
        let fixture = self.fixture.as_ref().ok_or_else(|| Error::FixtureRequest {
            reason: "Prepare a fixture with GameOptions::fixture before starting the game".into(),
        })?;
        let subject = fixture.recorded_subject();
        self.answer("observe_fixture", Some(&subject), async |game| {
            game.fixture_from_game().await
        })
        .await
    }

    async fn fixture_from_game(
        &mut self,
    ) -> Result<crate::Answer<crate::FixtureObservation>, Error> {
        let answer = self.paused.fixture.clone().unwrap_or_else(|| {
            Err(Error::Observation {
                operation: crate::Operation::ObserveFixture,
                reason: "The supervisor sent no fixture observation".into(),
            })
        });
        self.restart_idle_time(ReadQuestion::Fixture).await?;
        answer
    }

    /// A session over recorded answers. No supervisor or game process is started.
    pub(crate) fn recorded(
        directory: Arc<crate::recorded::Answers>,
        fixture: Option<crate::FixtureRequest>,
    ) -> Self {
        let paused = Paused {
            readiness: GameReadiness::PausedAfterRegistryInitialization,
            registries: BTreeMap::new(),
            fixture: None,
            modifiers: None,
        };
        let (_, state) = watch::channel(State::default());
        Self {
            commands: None,
            stop: Arc::new(AtomicU8::new(0)),
            state,
            paused,
            closing: false,
            observed: Default::default(),
            build: directory.build.clone(),
            backend: GameBackend::Recorded(directory),
            work: None,
            read_failed: false,
            fixture,
            modifiers: None,
        }
    }

    /// List the item names of one registry, as the engine holds them after its initial load.
    ///
    /// The registry is named by its content directory, such as `common/traditions`. Every call
    /// returns the same startup observation; the game is never resumed. Cancelling this future
    /// leaves the session alive. Select names from `Native::registries()` with
    /// `GameOptions::registries` before starting a live game. A name not selected for this session,
    /// or one whose initial loader did not run before the pause, returns `Error::Unsupported`.
    pub async fn registry_items(
        &mut self,
        registry: &str,
    ) -> Result<crate::Answer<Vec<String>>, Error> {
        let result = self.answer_registry_items(registry).await;
        self.keep_on_error(result)
    }

    async fn answer_registry_items(
        &mut self,
        registry: &str,
    ) -> Result<crate::Answer<Vec<String>>, Error> {
        if self.closing || self.state.borrow().finished.is_some() {
            return Err(Error::Closed);
        }
        self.answer("registry_items", Some(registry), async |game| {
            game.registry_items_from_game(registry).await
        })
        .await
    }

    /// Answer from recorded files when they are the back end; otherwise read the live session,
    /// and write the result when a recorder is set.
    async fn answer<T: serde::Serialize + serde::de::DeserializeOwned>(
        &mut self,
        question: &str,
        subject: Option<&str>,
        live: impl AsyncFnOnce(&mut Self) -> Result<crate::Answer<T>, Error>,
    ) -> Result<crate::Answer<T>, Error> {
        let recorder = match &self.backend {
            GameBackend::Recorded(answers) => return answers.read(question, subject),
            GameBackend::Live { recorder } => recorder.clone(),
        };
        let answer = live(self).await;
        if let Some(directory) = &recorder {
            crate::recorded::write(directory, &self.build, question, subject, &answer)?;
        }
        answer
    }

    async fn registry_items_from_game(
        &mut self,
        registry: &str,
    ) -> Result<crate::Answer<Vec<String>>, Error> {
        let directory = registry.trim_end_matches('/');
        if !self.observed.contains(directory) {
            return Err(Error::Unsupported {
                operation: crate::Operation::RegistryItems,
                reason: format!(
                    "this session does not observe {directory}; name it in GameOptions::registries before start_game"
                ),
            });
        }
        let name = directory.to_owned();
        if self.closing || self.state.borrow().finished.is_some() {
            return Err(Error::Closed);
        }
        let answer =
            registry_items_answer(directory, self.paused.registries.get(&name), &self.build)?;
        self.restart_idle_time(ReadQuestion::Registry(name)).await?;
        Ok(answer)
    }

    /// Record a read error so that `close` keeps the work directory. The error is returned
    /// unchanged, so a recorded answer replays it exactly; `work_directory` gives the path.
    /// `Error::Closed` after the caller began to close adds nothing to inspect. `Error::Closed`
    /// because the supervisor ended the session does.
    fn keep_on_error<T>(&mut self, result: Result<T, Error>) -> Result<T, Error> {
        if let Err(error) = &result
            && !(self.closing && *error == Error::Closed)
        {
            self.read_failed = true;
        }
        result
    }

    /// The temporary work directory of a live session, or `None` for recorded answers. `close`
    /// removes it only after a clean, confirmed session with no read error; otherwise it stays for
    /// inspection.
    pub fn work_directory(&self) -> Option<&Path> {
        self.work.as_deref()
    }

    /// Tell the supervisor that the caller got an answer, and wait for its acknowledgement. The
    /// supervisor ends a session that stays idle.
    async fn restart_idle_time(&mut self, question: ReadQuestion) -> Result<(), Error> {
        let (reply, receive) = oneshot::channel();
        self.commands
            .as_ref()
            .ok_or(Error::Closed)?
            .try_send(DriverCommand::Read { question, reply })
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => {
                    Error::Supervisor("Too many pending observation reads".into())
                }
                mpsc::TrySendError::Disconnected(_) => Error::Closed,
            })?;
        receive
            .await
            .map_err(|_| Error::Supervisor("Observation read acknowledgement lost".into()))?
    }

    /// Where the game is paused: after its registries load, or after all content loads with
    /// `GameOptions::loaded_modifiers`. No world is loaded; this does not advertise gameplay
    /// readiness.
    pub fn readiness(&self) -> GameReadiness {
        self.paused.readiness
    }
    /// Request cancellation. `close` still waits for the supervisor and returns the disposal.
    pub fn cancel(&mut self) {
        if !self.closing {
            self.closing = true;
            let _ = self
                .stop
                .compare_exchange(0, 2, Ordering::SeqCst, Ordering::SeqCst);
        }
    }
    /// Close the session and wait until the supervisor reports whether the game is gone.
    ///
    /// Repeated calls give the same result. Cleanup continues if this future is dropped after it
    /// was polled. Failed session cleanup returns `Error::Cleanup`, with the witnessed disposal.
    /// The temporary work directory is removed only after a clean, confirmed disposal of a
    /// session that the caller ended, with no read error. An error from `close` names the kept
    /// directory; after a read error, `work_directory` gives it.
    pub async fn close(&mut self) -> Result<Disposal, Error> {
        if matches!(self.backend, GameBackend::Recorded(_)) {
            self.closing = true;
            return Ok(Disposal::NotApplicable);
        }
        let result = self.finish().await;
        match &self.work {
            Some(work) => result.map_err(|error| name_kept_work(error, work)),
            None => result,
        }
    }

    async fn finish(&mut self) -> Result<Disposal, Error> {
        if !self.closing {
            self.closing = true;
            let _ = self
                .stop
                .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst);
        }
        let finished = loop {
            if let Some(result) = self.state.borrow().finished.clone() {
                break result?;
            }
            self.state
                .changed()
                .await
                .map_err(|_| Error::Supervisor("Session owner thread lost".into()))?;
        };
        if matches!(finished.outcome, SessionOutcome::Failed(_))
            || !finished.reservation_resolved
            || !finished.diagnostics.is_empty()
        {
            return Err(Error::Cleanup {
                reason: format!(
                    "{:?}; {}",
                    finished.outcome,
                    finished.diagnostics.join("; ")
                ),
                disposal: finished.disposal,
            });
        }
        // A session that the supervisor ended (time out, worker or caller loss) is kept.
        let caller_ended = matches!(
            finished.outcome,
            SessionOutcome::Completed | SessionOutcome::Cancelled
        );
        if finished.disposal == Disposal::Confirmed
            && caller_ended
            && !self.read_failed
            && let Some(work) = self.work.take()
        {
            let _ = std::fs::remove_dir_all(work);
        }
        Ok(finished.disposal)
    }
}
impl Drop for Game {
    fn drop(&mut self) {
        self.commands.take();
    }
}

/// The answer for one observed registry, or why it has none: no usable observation, a loader
/// that did not run before the pause, or an item layout that the binding cannot read.
fn registry_items_answer(
    directory: &str,
    observation: Option<&RegistryItems>,
    build: &crate::BuildId,
) -> Result<crate::Answer<Vec<String>>, Error> {
    use crate::{Answer, Basis, Completeness, Gap, GapKind, GapSubject, Operation, Source};
    let operation = Operation::RegistryItems;
    let Some(observed) = observation.filter(|items| items.observed != Observed::Unavailable) else {
        return Err(Error::Observation {
            operation,
            reason: observation.map_or_else(
                || "The supervisor sent no observation of this registry".into(),
                |items| items.diagnostics.join("; "),
            ),
        });
    };
    if observed.observed == Observed::NotLoaded {
        return Err(Error::Unsupported {
            operation,
            reason: observed.diagnostics.first().cloned().unwrap_or_else(|| {
                format!("the initial loader of {directory} did not run before the session paused")
            }),
        });
    }
    if observed.observed == Observed::Unsupported {
        return Err(Error::Unsupported {
            operation,
            reason: format!(
                "item observation of {directory} is unavailable: {}",
                observed
                    .diagnostics
                    .first()
                    .map(String::as_str)
                    .unwrap_or("unsupported item layout")
            ),
        });
    }
    let complete = observed.observed == Observed::Complete;
    Ok(Answer {
        value: observed.items.clone(),
        completeness: if complete {
            Completeness::Complete
        } else {
            Completeness::Partial
        },
        gaps: if complete {
            Vec::new()
        } else {
            vec![Gap {
                kind: GapKind::IncompleteObservation,
                subject: Some(GapSubject::registry(directory)),
                detail: "The engine collection was not read to its end.".into(),
            }]
        },
        source: Source::new(build.clone(), "registry-items/v1", Basis::LiveObservation),
    })
}

/// Add where the kept work directory is to an error's reason. An error without a reason is
/// returned unchanged.
fn name_kept_work(mut error: Error, work: &Path) -> Error {
    match &mut error {
        Error::Unsupported { reason, .. }
        | Error::FixtureRequest { reason }
        | Error::Method(reason)
        | Error::Observation { reason, .. }
        | Error::Startup { reason, .. }
        | Error::Cleanup { reason, .. }
        | Error::Recorded(reason)
        | Error::Supervisor(reason) => {
            reason.push_str(&format!("; work directory kept at {}", work.display()));
        }
        Error::BuildChanged
        | Error::UnknownRegistry { .. }
        | Error::Closed
        | Error::NotRecorded { .. } => {}
    }
    error
}

/// Start the driver thread and wait for the pause. A session that ends before its pause is a
/// failed start, whatever its outcome. The work directory is kept and its error names it.
pub(crate) async fn start(
    supervisor: Command,
    request: SessionRequest,
    session: Session,
) -> Result<Game, Error> {
    let work = session.work.clone();
    start_until_paused(supervisor, request, session)
        .await
        .map_err(|error| name_kept_work(error, &work))
}

async fn start_until_paused(
    supervisor: Command,
    request: SessionRequest,
    session: Session,
) -> Result<Game, Error> {
    let timing = driver::Timing {
        startup_seconds: request.startup_seconds,
        idle_seconds: request.idle_seconds,
    };
    let (commands, receive) = mpsc::sync_channel(16);
    let stop = Arc::new(AtomicU8::new(0));
    let owner_stop = stop.clone();
    let (state, mut changes) = watch::channel(State::default());
    std::thread::Builder::new()
        .name("native-game-owner".into())
        .spawn(move || {
            driver::run(supervisor, request, timing, receive, owner_stop, state);
        })
        .map_err(|error| Error::Supervisor(error.to_string()))?;
    // The only command sender stays in this future until ownership moves into Game.
    loop {
        let current = changes.borrow().clone();
        if let Some(finished) = current.finished {
            let finished = finished?;
            return Err(Error::Startup {
                reason: format!(
                    "{:?}; {}",
                    finished.outcome,
                    finished.diagnostics.join("; ")
                ),
                disposal: finished.disposal,
            });
        }
        if let Some(paused) = current.paused {
            let modifiers = session.modifiers.map(|join| match &paused.modifiers {
                Some(Ok(observed)) => Ok(join.assemble(observed, session.build.clone())),
                Some(Err(error)) => Err(error.clone()),
                None => Err(Error::Observation {
                    operation: crate::Operation::LoadedModifiers,
                    reason: "The supervisor sent no modifier table".into(),
                }),
            });
            return Ok(Game {
                commands: Some(commands),
                stop,
                state: changes,
                paused,
                closing: false,
                observed: session.observed,
                build: session.build,
                backend: GameBackend::Live {
                    recorder: session.recorder,
                },
                work: Some(session.work),
                read_failed: false,
                fixture: session.fixture,
                modifiers,
            });
        }
        changes
            .changed()
            .await
            .map_err(|_| Error::Supervisor("Session owner thread lost".into()))?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fixture_acknowledgement_loss_is_recorded_as_the_returned_error() {
        let (mut game, commands, _state) = game();
        let fixture =
            crate::FixtureRequest::new("common/tradition_categories/test.txt", "test = {}\n");
        let subject = fixture.recorded_subject();
        game.fixture = Some(fixture);
        game.paused.fixture = Some(Ok(crate::Answer {
            value: crate::FixtureObservation::default(),
            completeness: crate::Completeness::Complete,
            gaps: vec![],
            source: crate::Source::new(
                game.build.clone(),
                "test/v1",
                crate::Basis::LiveObservation,
            ),
        }));
        let root = tempfile::tempdir().unwrap();
        game.backend = GameBackend::Live {
            recorder: Some(Arc::new(root.path().into())),
        };
        let lost_acknowledgement = std::thread::spawn(move || {
            let DriverCommand::Read { reply, .. } = commands.recv().unwrap();
            drop(reply);
        });
        let error = game.observe_fixture().await.unwrap_err();
        assert!(matches!(error, Error::Supervisor(_)));
        lost_acknowledgement.join().unwrap();
        let recording = crate::recorded::Answers::open(root.path().into()).unwrap();
        assert_eq!(
            recording.read::<crate::FixtureObservation>("observe_fixture", Some(&subject)),
            Err(error)
        );
    }

    #[tokio::test]
    async fn fixture_reads_refresh_idle_time_and_record_errors_without_resuming() {
        let (mut game, commands, state) = game();
        assert!(matches!(
            game.observe_fixture().await,
            Err(Error::FixtureRequest { .. })
        ));
        assert!(commands.try_recv().is_err());
        let fixture =
            crate::FixtureRequest::new("common/tradition_categories/test.txt", "test = {}\n");
        let subject = fixture.recorded_subject();
        game.fixture = Some(fixture);
        let error = Error::Observation {
            operation: crate::Operation::ObserveFixture,
            reason: "Missing fixture hook".into(),
        };
        game.paused.fixture = Some(Err(error.clone()));
        let root = tempfile::tempdir().unwrap();
        game.backend = GameBackend::Live {
            recorder: Some(Arc::new(root.path().into())),
        };
        let acknowledgements = std::thread::spawn(move || {
            for _ in 0..2 {
                let DriverCommand::Read { question, reply } = commands.recv().unwrap();
                assert!(matches!(question, ReadQuestion::Fixture));
                reply.send(Ok(())).unwrap();
            }
        });
        for _ in 0..2 {
            assert_eq!(game.observe_fixture().await, Err(error.clone()));
        }
        acknowledgements.join().unwrap();
        let recording = crate::recorded::Answers::open(root.path().into()).unwrap();
        assert_eq!(
            recording.read::<crate::FixtureObservation>("observe_fixture", Some(&subject)),
            Err(error)
        );
        state.send_modify(|state| state.finished = Some(Ok(finished())));
        assert_eq!(game.observe_fixture().await, Err(Error::Closed));
    }
    fn finished() -> Finished {
        Finished {
            outcome: SessionOutcome::Completed,
            disposal: Disposal::Confirmed,
            reservation_resolved: true,
            diagnostics: Vec::new(),
        }
    }
    fn game() -> (Game, mpsc::Receiver<DriverCommand>, watch::Sender<State>) {
        let paused = Paused {
            readiness: GameReadiness::PausedDuringRegistryInitialization,
            registries: BTreeMap::new(),
            fixture: None,
            modifiers: None,
        };
        let (commands, receive) = mpsc::sync_channel(16);
        let (state, changes) = watch::channel(State {
            paused: Some(paused.clone()),
            finished: None,
        });
        (
            Game {
                commands: Some(commands),
                stop: Arc::new(AtomicU8::new(0)),
                state: changes,
                paused,
                closing: false,
                observed: std::collections::BTreeSet::from(["common/traditions".into()]),
                build: crate::BuildId("test".into()),
                backend: GameBackend::Live { recorder: None },
                work: None,
                read_failed: false,
                fixture: None,
                modifiers: None,
            },
            receive,
            state,
        )
    }

    #[tokio::test]
    async fn loaded_modifiers_need_the_option_and_refresh_idle_time_when_read() {
        let (mut game, commands, _state) = game();
        assert!(matches!(
            game.loaded_modifiers().await,
            Err(Error::Unsupported { .. })
        ));
        assert!(commands.try_recv().is_err());
        let answer = crate::Answer {
            value: crate::LoadedModifiers {
                modifiers: Vec::new(),
                registry_items: BTreeMap::new(),
                content: crate::LoadedContent::Installation,
            },
            completeness: crate::Completeness::Complete,
            gaps: vec![],
            source: crate::Source::new(
                game.build.clone(),
                "test/v1",
                crate::Basis::LiveObservation,
            ),
        };
        game.modifiers = Some(Ok(answer.clone()));
        let acknowledgement = std::thread::spawn(move || {
            let DriverCommand::Read { question, reply } = commands.recv().unwrap();
            assert!(matches!(question, ReadQuestion::Modifiers));
            reply.send(Ok(())).unwrap();
        });
        assert_eq!(game.loaded_modifiers().await, Ok(answer));
        acknowledgement.join().unwrap();
    }

    #[tokio::test]
    async fn failed_cleanup_keeps_diagnostics_even_when_the_game_is_gone() {
        for failure in ["reservation", "outcome", "diagnostics"] {
            let (mut game, _commands, state) = game();
            let root = tempfile::tempdir().unwrap();
            let work = root.path().join("session");
            std::fs::create_dir(&work).unwrap();
            let log = work.join("report.json");
            std::fs::write(&log, "cleanup diagnostics").unwrap();
            game.work = Some(work.clone());
            let mut report = finished();
            match failure {
                "reservation" => report.reservation_resolved = false,
                "outcome" => {
                    report.outcome = SessionOutcome::Failed("Session bookkeeping failed".into())
                }
                _ => report.diagnostics.push("worker stop failed".into()),
            }
            state.send_modify(|state| state.finished = Some(Ok(report.clone())));
            let error = game.close().await.unwrap_err();
            let kept = format!("; work directory kept at {}", work.display());
            assert!(matches!(
                &error,
                Error::Cleanup {
                    disposal: Disposal::Confirmed,
                    reason,
                } if reason.ends_with(&kept)
            ));
            assert_eq!(game.close().await.unwrap_err(), error);
            assert_eq!(
                std::fs::read_to_string(&log).unwrap(),
                "cleanup diagnostics"
            );
            assert_eq!(game.work.as_ref(), Some(&work));
        }
    }

    #[tokio::test]
    async fn clean_close_removes_its_temporary_directory() {
        let (mut game, _commands, state) = game();
        let root = tempfile::tempdir().unwrap();
        let work = root.path().join("session");
        std::fs::create_dir(&work).unwrap();
        game.work = Some(work.clone());
        state.send_modify(|state| state.finished = Some(Ok(finished())));
        assert_eq!(game.close().await.unwrap(), Disposal::Confirmed);
        assert!(!work.exists());
    }

    #[tokio::test]
    async fn a_read_error_keeps_the_work_directory_after_a_clean_close() {
        let (mut game, commands, state) = game();
        let root = tempfile::tempdir().unwrap();
        let work = root.path().join("session");
        std::fs::create_dir(&work).unwrap();
        game.work = Some(work.clone());
        let recording = root.path().join("recorded");
        game.backend = GameBackend::Live {
            recorder: Some(Arc::new(recording.clone())),
        };
        game.paused.registries.insert(
            "common/traditions".into(),
            RegistryItems {
                items: Vec::new(),
                observed: Observed::Unavailable,
                diagnostics: vec!["access failed".into()],
            },
        );
        assert!(matches!(
            game.registry_items("common/traditions").await,
            Err(Error::Observation { reason, .. }) if reason == "access failed"
        ));
        assert!(commands.try_recv().is_err());
        assert_eq!(game.work_directory(), Some(work.as_path()));
        // The recorded answer replays the same error.
        let recorded = crate::recorded::Answers::open(recording).unwrap();
        assert!(matches!(
            recorded.read::<Vec<String>>("registry_items", Some("common/traditions")),
            Err(Error::Observation { reason, .. }) if reason == "access failed"
        ));
        state.send_modify(|state| state.finished = Some(Ok(finished())));
        assert_eq!(game.close().await.unwrap(), Disposal::Confirmed);
        assert!(work.exists());
        assert_eq!(game.work_directory(), Some(work.as_path()));
    }

    #[tokio::test]
    async fn a_session_the_supervisor_ended_keeps_the_work_directory() {
        for outcome in [SessionOutcome::TimedOut, SessionOutcome::WorkerLost] {
            let (mut game, _commands, state) = game();
            let root = tempfile::tempdir().unwrap();
            let work = root.path().join("session");
            std::fs::create_dir(&work).unwrap();
            game.work = Some(work.clone());
            let report = Finished {
                outcome,
                ..finished()
            };
            state.send_modify(|state| state.finished = Some(Ok(report)));
            assert_eq!(
                game.registry_items("common/traditions").await,
                Err(Error::Closed)
            );
            assert_eq!(game.close().await.unwrap(), Disposal::Confirmed);
            assert!(work.exists());
        }
    }

    #[tokio::test]
    async fn a_read_after_close_does_not_keep_the_work_directory() {
        let (mut game, _commands, state) = game();
        let root = tempfile::tempdir().unwrap();
        let work = root.path().join("session");
        std::fs::create_dir(&work).unwrap();
        game.work = Some(work.clone());
        game.cancel();
        assert_eq!(
            game.registry_items("common/traditions").await,
            Err(Error::Closed)
        );
        state.send_modify(|state| state.finished = Some(Ok(finished())));
        assert_eq!(game.close().await.unwrap(), Disposal::Confirmed);
        assert!(!work.exists());
    }

    #[test]
    fn only_errors_with_a_reason_name_the_kept_work_directory() {
        let work = Path::new("/tmp/pdx-native-1-2");
        let startup = Error::Startup {
            reason: "worker lost".into(),
            disposal: Disposal::NotApplicable,
        };
        assert_eq!(
            name_kept_work(startup, work),
            Error::Startup {
                reason: "worker lost; work directory kept at /tmp/pdx-native-1-2".into(),
                disposal: Disposal::NotApplicable,
            }
        );
        for error in [
            Error::BuildChanged,
            Error::Closed,
            Error::UnknownRegistry {
                name: "common/nothing".into(),
            },
            Error::NotRecorded {
                question: "registry_items".into(),
            },
        ] {
            assert_eq!(name_kept_work(error.clone(), work), error);
        }
    }

    #[tokio::test]
    async fn a_read_after_close_does_not_overwrite_a_recorded_answer() {
        let (mut game, _commands, _state) = game();
        let root = tempfile::tempdir().unwrap();
        let original: Result<crate::Answer<Vec<String>>, Error> = Err(Error::BuildChanged);
        crate::recorded::write(
            root.path(),
            &game.build,
            "registry_items",
            Some("common/traditions"),
            &original,
        )
        .unwrap();
        game.backend = GameBackend::Live {
            recorder: Some(Arc::new(root.path().into())),
        };
        game.closing = true;
        assert!(matches!(
            game.registry_items("common/traditions").await,
            Err(Error::Closed)
        ));
        let recorded = crate::recorded::Answers::open(root.path().into()).unwrap();
        assert_eq!(
            recorded.read::<Vec<String>>("registry_items", Some("common/traditions")),
            original
        );
    }

    #[tokio::test]
    async fn cancelled_close_keeps_cleanup_requested_and_close_is_idempotent() {
        let (mut game, commands, state) = game();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(10), game.close())
                .await
                .is_err()
        );
        assert_eq!(game.stop.load(Ordering::SeqCst), 1);
        assert!(matches!(
            game.registry_items("common/traditions").await,
            Err(Error::Closed)
        ));
        state.send_modify(|state| state.finished = Some(Ok(finished())));
        let first = game.close().await.unwrap();
        let second = game.close().await.unwrap();
        assert_eq!(first, Disposal::Confirmed);
        assert_eq!(first, second);
        assert!(commands.try_recv().is_err());
    }
    #[tokio::test]
    async fn cancellation_and_drop_do_not_claim_disposal() {
        let (mut game, commands, state) = game();
        game.cancel();
        assert_eq!(game.stop.load(Ordering::SeqCst), 2);
        assert!(state.borrow().finished.is_none());
        drop(game);
        assert!(matches!(
            commands.try_recv(),
            Err(mpsc::TryRecvError::Disconnected)
        ));
    }
    #[tokio::test]
    async fn a_lost_supervisor_connection_is_an_error_and_never_a_disposal() {
        let (mut game, _commands, state) = game();
        state.send_modify(|state| {
            state.finished = Some(Err(Error::Supervisor("connection lost".into())))
        });
        assert!(matches!(game.close().await, Err(Error::Supervisor(_))));
    }
    #[tokio::test]
    async fn a_registry_without_an_observation_gives_an_error_and_sends_no_read() {
        let (mut game, commands, _state) = game();
        assert!(matches!(
            game.registry_items("common/traditions").await,
            Err(Error::Observation { .. })
        ));
        game.paused.registries.insert(
            "common/traditions".into(),
            RegistryItems {
                items: vec!["kept".into()],
                observed: Observed::Unavailable,
                diagnostics: vec!["access failed".into()],
            },
        );
        assert!(matches!(
            game.registry_items("common/traditions").await,
            Err(Error::Observation { reason, .. }) if reason == "access failed"
        ));
        assert!(matches!(
            commands.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
    }
    #[tokio::test]
    async fn an_unloaded_or_unsupported_layout_is_precisely_unavailable() {
        let (mut game, commands, _state) = game();
        for observed in [Observed::NotLoaded, Observed::Unsupported] {
            game.paused.registries.insert(
                "common/traditions".into(),
                RegistryItems {
                    items: Vec::new(),
                    observed,
                    diagnostics: vec!["unavailable key layout".into()],
                },
            );
            assert!(matches!(
                game.registry_items("common/traditions").await,
                Err(Error::Unsupported { .. })
            ));
        }
        assert!(matches!(
            commands.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
    }
    #[tokio::test]
    async fn registry_items_names_registries_by_directory_and_sends_no_read_for_other_names() {
        let (mut game, commands, _state) = game();
        for unknown in ["traditions", "common/nothing"] {
            assert!(matches!(
                game.registry_items(unknown).await,
                Err(Error::Unsupported { .. })
            ));
        }
        assert!(matches!(
            commands.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        game.cancel();
        assert!(matches!(
            game.registry_items("common/traditions").await,
            Err(Error::Closed)
        ));
    }
    #[test]
    fn runtime_shutdown_drops_the_lease_without_requiring_async_cleanup() {
        let (game, commands, _state) = game();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        runtime.block_on(async {
            tokio::spawn(async move {
                let _game = game;
                std::future::pending::<()>().await;
            });
            tokio::task::yield_now().await;
        });
        drop(runtime);
        assert!(matches!(
            commands.recv_timeout(std::time::Duration::from_secs(1)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
    }
}
