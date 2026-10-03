//! `Game`: one supervised game session, or its stand-in over recorded answers.
//!
//! An independent thread (`driver`) talks to the supervisor process, so no async runtime owns
//! the game. The supervisor sends the answers once, when the game is paused; every
//! fixture and modifier query returns from them.
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
    /// A deliberate fault and the observation that receives it.
    pub(crate) fault: Option<Fault>,
    pub(crate) fixture: Option<crate::FixtureRequest>,
    pub(crate) loader_registry: Option<String>,
    pub(crate) loaded_modifiers: bool,
    pub(crate) keep_work_directory: bool,
}
impl GameOptions {
    /// `supervisor` starts a dedicated process that calls `supervisor::serve` on its standard
    /// input and output, then exits. It must link the same Native build as the caller.
    pub fn new(supervisor: Command) -> Self {
        Self {
            supervisor,
            startup_seconds: crate::protocol::session::MAX_SESSION_SECONDS,
            fault: None,
            fixture: None,
            loader_registry: None,
            loaded_modifiers: false,
            keep_work_directory: false,
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
    /// Prepare one fixed fixture before launch. Its registry is observed automatically.
    pub fn fixture(mut self, request: crate::FixtureRequest) -> Self {
        self.fixture = Some(request);
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
    /// Keep the work directory after every `close`, for Native's live tests. The caller then
    /// removes it.
    #[doc(hidden)]
    pub fn keep_work_directory(mut self) -> Self {
        self.keep_work_directory = true;
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
    /// `close` never removes `work`; the caller does.
    pub keep_work: bool,
    pub fixture: Option<crate::FixtureRequest>,
    /// The static side of the loaded modifier join, when the session reads the table.
    pub modifiers: Option<crate::session::ModifierJoin>,
    /// The installation whose static analysis names the duration receivers of each script check.
    pub binding: Arc<crate::binding::Binding>,
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
    CheckScript {
        input: crate::ScriptCheck,
        durations: Vec<crate::protocol::script_check::DurationReceiver>,
        reply: oneshot::Sender<Result<crate::Answer<crate::ScriptObservation>, Error>>,
    },
    /// The caller answered a question about this registry; the idle time starts again.
    Read {
        question: ReadQuestion,
        reply: oneshot::Sender<Result<(), Error>>,
    },
}

struct CheckCancellation(Option<Arc<AtomicU8>>);
impl Drop for CheckCancellation {
    fn drop(&mut self) {
        if let Some(stop) = &self.0 {
            let _ = stop.compare_exchange(0, 2, Ordering::SeqCst, Ordering::SeqCst);
        }
    }
}

#[derive(Debug)]
enum GameBackend {
    Live {
        recorder: Option<Arc<PathBuf>>,
        /// Names the duration receivers of each script check.
        binding: Option<Arc<crate::binding::Binding>>,
    },
    Recorded(Arc<crate::recorded::Answers>),
}

/// An owned process paused at registry initialization, or after content loads with
/// `GameOptions::loaded_modifiers`.
/// Drop requests cleanup. Await close for independently confirmed disposal.
#[derive(Debug)]
pub struct Game {
    script_history: String,
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
    /// The caller removes the work directory (`GameOptions::keep_work_directory`).
    keep_work: bool,
    fixture: Option<crate::FixtureRequest>,
    /// The joined loaded modifier answer, when the session reads the table.
    modifiers: Option<Result<crate::Answer<crate::LoadedModifiers>, Error>>,
}
impl Game {
    /// Read and validate one trigger or effect snippet at the loaded-content pause.
    ///
    /// Start with `GameOptions::loaded_modifiers` and use a scope from `Native::scopes`.
    /// Text is limited to 4 KiB, diagnostics to 32 occurrences, a check to five seconds, and
    /// a session to 3,000 checks. Nothing is evaluated or executed. A complete quiet answer
    /// is only an observation, never acceptance. Dropping this future after admission requests
    /// session cancellation; await `close` for disposal. A failed engine call ends the session
    /// and this method waits for its cleanup report before returning an error.
    ///
    /// ```no_run
    /// # use pdx_native::{Native, GameOptions, ScriptCheck, DeclarationKind};
    /// # async fn example(native: &Native, supervisor: std::process::Command)
    /// # -> Result<(), Box<dyn std::error::Error>> {
    /// let scopes = native.scopes()?;
    /// let country = scopes.value.types.iter().find(|scope| scope.name == "country")
    ///     .ok_or("country scope is unavailable")?;
    /// let mut game = native.start_game(GameOptions::new(supervisor).loaded_modifiers()).await?;
    /// let result = game.check_script(&ScriptCheck {
    ///     kind: DeclarationKind::Trigger,
    ///     scope: country.id.clone(),
    ///     text: "always = yes".into(),
    /// }).await;
    /// game.close().await?;
    /// let answer = result?;
    /// // Inspect coverage and all message groups; silence alone does not prove acceptance.
    /// println!("{:?}", answer);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn check_script(
        &mut self,
        input: &crate::ScriptCheck,
    ) -> Result<crate::Answer<crate::ScriptObservation>, Error> {
        if self.closing
            || self.stop.load(Ordering::SeqCst) != 0
            || self.state.borrow().finished.is_some()
        {
            return Err(Error::Closed);
        }
        input.validate()?;
        let subject = input.recorded_subject(&self.script_history);
        let result = self
            .answer("check_script", Some(&subject), async |game| {
                if game.paused.readiness != GameReadiness::AfterContentLoad {
                    return Err(Error::ScriptRequest {
                        reason: "script checks require a confirmed content-loaded pause".into(),
                    });
                }
                let durations = game.duration_receivers(input);
                let (reply, receive) = oneshot::channel();
                game.commands
                    .as_ref()
                    .ok_or(Error::Closed)?
                    .try_send(DriverCommand::CheckScript {
                        input: input.clone(),
                        durations,
                        reply,
                    })
                    .map_err(|_| Error::Supervisor("script check could not be queued".into()))?;
                let mut cancellation = CheckCancellation(Some(game.stop.clone()));
                let result = receive.await;
                cancellation.0 = None;
                match result {
                    Ok(answer) => {
                        if answer.is_ok() {
                            game.script_history = subject.clone();
                        }
                        answer
                    }
                    Err(_) => {
                        game.read_failed = true;
                        game.close().await?;
                        Err(Error::Observation {
                            operation: crate::Operation::CheckScript,
                            reason: "script check ended with session disposal".into(),
                        })
                    }
                }
            })
            .await;
        if result.is_ok() {
            self.script_history = subject;
        }
        self.keep_on_error(result)
    }
    /// The duration receivers that a live check carries; none without a static analysis.
    fn duration_receivers(
        &self,
        input: &crate::ScriptCheck,
    ) -> Vec<crate::protocol::script_check::DurationReceiver> {
        let GameBackend::Live {
            binding: Some(binding),
            ..
        } = &self.backend
        else {
            return Vec::new();
        };
        let Some(analysis) = &binding.analysis else {
            return Vec::new();
        };

        crate::session::script_durations::duration_receivers(analysis, input.kind, &input.text)
    }

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

    /// Return the prepared fixture's parser outcomes. Every call uses the same startup
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
            readiness: GameReadiness::AfterRegistryInitialization,
            registries: BTreeMap::new(),
            fixture: None,
            modifiers: None,
        };
        let (_, state) = watch::channel(State::default());
        Self {
            commands: None,
            script_history: String::new(),
            stop: Arc::new(AtomicU8::new(0)),
            state,
            paused,
            closing: false,
            observed: Default::default(),
            build: directory.build.clone(),
            backend: GameBackend::Recorded(directory),
            work: None,
            read_failed: false,
            keep_work: false,
            fixture,
            modifiers: None,
        }
    }

    /// One internal loader check. The caller owns closing the session after this read.
    pub(crate) async fn loader_items(
        &mut self,
        registry: &str,
    ) -> Result<crate::Answer<Vec<String>>, Error> {
        let result = self.registry_items_from_game(registry).await;
        self.keep_on_error(result)
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
            GameBackend::Live { recorder, .. } => recorder.clone(),
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
                operation: crate::Operation::Registries,
                reason: format!(
                    "this session does not observe {directory}; use the internal loader check"
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

    /// Request cancellation. `close` still waits for the supervisor and returns the disposal.
    #[cfg(test)]
    fn cancel(&mut self) {
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
    /// directory; after a read error, `work_directory` gives it. When its removal fails, `close`
    /// returns `Error::Cleanup` and a later `close` tries again.
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
            && !self.keep_work
            && let Some(work) = &self.work
        {
            std::fs::remove_dir_all(work).map_err(|error| Error::Cleanup {
                reason: format!("work directory not removed: {error}"),
                disposal: Disposal::Confirmed,
            })?;
            self.work = None;
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
    let operation = Operation::Registries;
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
        | Error::ScriptRequest { reason }
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
        | Error::UnknownCommand { .. }
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
                script_history: String::new(),
                commands: Some(commands),
                stop,
                state: changes,
                paused,
                closing: false,
                observed: session.observed,
                build: session.build,
                backend: GameBackend::Live {
                    recorder: session.recorder,
                    binding: Some(session.binding),
                },
                work: Some(session.work),
                read_failed: false,
                keep_work: session.keep_work,
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
            crate::FixtureRequest::field_outcomes(
                "common/tradition_categories/test.txt",
                "test = {}\n",
                [crate::FixtureFieldQuestion::new(
                    "common/tradition_categories",
                    "test",
                    "traditions",
                )
                .with_parsing()],
            );
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
            binding: None,
        };
        let lost_acknowledgement = std::thread::spawn(move || {
            let DriverCommand::Read { reply, .. } = commands.recv().unwrap() else {
                panic!("expected read command")
            };
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
            crate::FixtureRequest::field_outcomes(
                "common/tradition_categories/test.txt",
                "test = {}\n",
                [crate::FixtureFieldQuestion::new(
                    "common/tradition_categories",
                    "test",
                    "traditions",
                )
                .with_parsing()],
            );
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
            binding: None,
        };
        let acknowledgements = std::thread::spawn(move || {
            for _ in 0..2 {
                let DriverCommand::Read { question, reply } = commands.recv().unwrap() else {
                    panic!("expected read command")
                };
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
    #[tokio::test]
    async fn cancelling_a_check_future_ends_the_session_before_reuse() {
        let (mut game, commands, state) = game();
        game.paused.readiness = GameReadiness::AfterContentLoad;
        let input = crate::ScriptCheck {
            kind: crate::DeclarationKind::Trigger,
            scope: crate::ScopeId("scope".into()),
            text: "always = yes".into(),
        };
        assert!(
            tokio::time::timeout(
                std::time::Duration::from_millis(10),
                game.check_script(&input)
            )
            .await
            .is_err()
        );
        assert!(matches!(
            commands.try_recv().unwrap(),
            DriverCommand::CheckScript { .. }
        ));
        assert_eq!(game.stop.load(Ordering::SeqCst), 2);
        assert!(matches!(
            game.check_script(&input).await,
            Err(Error::Closed)
        ));
        state.send_modify(|state| state.finished = Some(Ok(finished())));
        assert_eq!(game.close().await.unwrap(), Disposal::Confirmed);
    }

    #[tokio::test]
    async fn early_pauses_reject_checks_without_queuing_engine_work() {
        let (mut game, commands, _) = game();
        let input = crate::ScriptCheck {
            kind: crate::DeclarationKind::Trigger,
            scope: crate::ScopeId("scope".into()),
            text: "always = yes".into(),
        };
        assert!(matches!(
            game.check_script(&input).await,
            Err(Error::ScriptRequest { .. })
        ));
        assert!(commands.try_recv().is_err());
        assert!(game.script_history.is_empty());
    }

    #[tokio::test]
    async fn recording_failure_preserves_completed_check_history() {
        let (mut game, commands, state) = game();
        game.paused.readiness = GameReadiness::AfterContentLoad;
        let root = tempfile::tempdir().unwrap();
        let recording = root.path().join("recording");
        std::fs::write(&recording, "block directory creation").unwrap();
        game.backend = GameBackend::Live {
            recorder: Some(Arc::new(recording.clone())),
            binding: None,
        };
        let input = crate::ScriptCheck {
            kind: crate::DeclarationKind::Trigger,
            scope: crate::ScopeId("scope".into()),
            text: "always = banana".into(),
        };
        let first = input.recorded_subject("");
        let second = input.recorded_subject(&first);
        let responder = std::thread::spawn(move || {
            for check in 1..=2 {
                let DriverCommand::CheckScript { reply, .. } = commands.recv().unwrap() else {
                    panic!("expected check");
                };
                let observation = crate::ScriptObservation {
                    check,
                    read_returned: true,
                    children: 1,
                    diagnostics: vec![],
                    foreign: vec![],
                    unjoined: vec![],
                    hooks_active: true,
                    bound_reached: false,
                    stored_durations: crate::GrammarProperty::Known(Vec::new()),
                };
                reply
                    .send(Ok(observation.answer(crate::BuildId("test".into()))))
                    .unwrap();
            }
        });
        assert!(matches!(
            game.check_script(&input).await,
            Err(Error::Recorded(_))
        ));
        assert_eq!(game.script_history, first);
        std::fs::remove_file(&recording).unwrap();
        assert_eq!(game.check_script(&input).await.unwrap().value.check, 2);
        assert_eq!(game.script_history, second);
        let recorded = crate::recorded::Answers::open(recording).unwrap();
        assert!(matches!(
            recorded.read::<crate::ScriptObservation>("check_script", Some(&first)),
            Err(Error::NotRecorded { .. })
        ));
        assert_eq!(
            recorded
                .read::<crate::ScriptObservation>("check_script", Some(&second))
                .unwrap()
                .value
                .check,
            2
        );
        responder.join().unwrap();
        state.send_modify(|state| state.finished = Some(Ok(finished())));
        game.close().await.unwrap();
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
            readiness: GameReadiness::DuringRegistryInitialization,
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
                script_history: String::new(),
                commands: Some(commands),
                stop: Arc::new(AtomicU8::new(0)),
                state: changes,
                paused,
                closing: false,
                observed: std::collections::BTreeSet::from(["common/traditions".into()]),
                build: crate::BuildId("test".into()),
                backend: GameBackend::Live {
                    recorder: None,
                    binding: None,
                },
                work: None,
                read_failed: false,
                keep_work: false,
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
            let DriverCommand::Read { question, reply } = commands.recv().unwrap() else {
                panic!("expected read command")
            };
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
    async fn a_failed_removal_names_the_work_directory_and_a_later_close_retries_it() {
        use std::os::unix::fs::PermissionsExt;
        let (mut game, _commands, state) = game();
        let root = tempfile::tempdir().unwrap();
        let work = root.path().join("session");
        std::fs::create_dir(&work).unwrap();
        game.work = Some(work.clone());
        state.send_modify(|state| state.finished = Some(Ok(finished())));
        let set_mode =
            |mode| std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(mode));

        set_mode(0o555).unwrap();
        let error = game.close().await.unwrap_err();
        set_mode(0o755).unwrap();

        let kept = format!("; work directory kept at {}", work.display());
        assert!(matches!(
            &error,
            Error::Cleanup {
                disposal: Disposal::Confirmed,
                reason,
            } if reason.starts_with("work directory not removed: ") && reason.ends_with(&kept)
        ));
        assert!(work.exists());
        assert_eq!(game.work_directory(), Some(work.as_path()));

        assert_eq!(game.close().await.unwrap(), Disposal::Confirmed);
        assert!(!work.exists());
        assert_eq!(game.work_directory(), None);
    }

    #[tokio::test]
    async fn a_kept_work_directory_survives_a_clean_close_for_a_later_check() {
        let (mut game, _commands, state) = game();
        let root = tempfile::tempdir().unwrap();
        let work = root.path().join("session");
        std::fs::create_dir(&work).unwrap();
        game.work = Some(work.clone());
        game.keep_work = true;
        state.send_modify(|state| state.finished = Some(Ok(finished())));
        assert_eq!(game.close().await.unwrap(), Disposal::Confirmed);
        assert!(work.exists());
        assert_eq!(game.work_directory(), Some(work.as_path()));
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
            binding: None,
        };
        let fixture = crate::FixtureRequest::field_outcomes(
            "common/traditions/sample.txt",
            "sample = {}",
            [crate::FixtureFieldQuestion::new(
                "common/traditions",
                "sample",
                "icon",
            )],
        );
        let subject = fixture.recorded_subject();
        game.fixture = Some(fixture);
        game.paused.fixture = Some(Err(Error::Observation {
            operation: crate::Operation::ObserveFixture,
            reason: "access failed".into(),
        }));
        let read = game.observe_fixture();
        let acknowledge = async {
            let DriverCommand::Read { reply, .. } = commands.recv().unwrap() else {
                panic!("fixture read")
            };
            reply.send(Ok(())).unwrap();
        };
        let (answer, ()) = tokio::join!(read, acknowledge);
        assert!(matches!(
            answer,
            Err(Error::Observation { reason, .. }) if reason == "access failed"
        ));
        assert!(commands.try_recv().is_err());
        assert_eq!(game.work_directory(), Some(work.as_path()));
        // The recorded answer replays the same error.
        let recorded = crate::recorded::Answers::open(recording).unwrap();
        assert!(matches!(
            recorded.read::<crate::FixtureObservation>("observe_fixture", Some(&subject)),
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
                game.loader_items("common/traditions").await,
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
            game.loader_items("common/traditions").await,
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
        let original: Result<crate::Answer<crate::LoadedModifiers>, Error> =
            Err(Error::BuildChanged);
        crate::recorded::write(
            root.path(),
            &game.build,
            "loaded_modifiers",
            None,
            &original,
        )
        .unwrap();
        game.backend = GameBackend::Live {
            recorder: Some(Arc::new(root.path().into())),
            binding: None,
        };
        game.closing = true;
        assert!(matches!(game.loaded_modifiers().await, Err(Error::Closed)));
        let recorded = crate::recorded::Answers::open(root.path().into()).unwrap();
        assert_eq!(
            recorded.read::<crate::LoadedModifiers>("loaded_modifiers", None),
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
            game.loader_items("common/traditions").await,
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
            game.loader_items("common/traditions").await,
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
            game.loader_items("common/traditions").await,
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
                game.loader_items("common/traditions").await,
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
                game.loader_items(unknown).await,
                Err(Error::Unsupported { .. })
            ));
        }
        assert!(matches!(
            commands.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        game.cancel();
        assert!(matches!(
            game.loader_items("common/traditions").await,
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
