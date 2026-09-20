//! `Game`: one supervised game session, or its stand-in over recorded answers.
//!
//! An independent thread (`driver`) talks to the supervisor process, so no async runtime owns
//! the game. The supervisor sends the answers once, when the game is paused; every
//! `registry_items` call returns from them.
use crate::{
    Disposal, Error, GameReadiness,
    engine::operations::registry_items::{Observed, RegistryItems},
    protocol::session::{ObservationControl, SessionOutcome, SessionRequest},
};
use std::sync::atomic::{AtomicU8, Ordering};
use std::{
    collections::BTreeMap,
    path::PathBuf,
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
    /// A deliberate fault and the content directory of the registry that receives it.
    pub(crate) fault: Option<(String, ObservationControl)>,
}
impl GameOptions {
    /// `supervisor` starts a dedicated process that calls `supervisor::serve` on its standard
    /// input and output, then exits. It must link the same Native build as the caller.
    pub fn new(supervisor: Command) -> Self {
        Self {
            supervisor,
            startup_seconds: 180,
            idle_seconds: 180,
            fault: None,
        }
    }
    /// Inject a deliberate fault into the observation of one registry, named by its content
    /// directory. Only Native's live tests use this; see `tests/live.rs`.
    #[doc(hidden)]
    pub fn fault(mut self, registry: &str, control: ObservationControl) -> Self {
        self.fault = Some((registry.trim_end_matches('/').into(), control));
        self
    }
}

/// What `start` needs besides the request: the caller's names and where answers go.
pub(crate) struct Session {
    /// Content directory of each observed registry, with its internal name.
    pub directories: BTreeMap<String, String>,
    pub build: crate::BuildId,
    /// Write every answer to this directory as it is returned.
    pub recorder: Option<Arc<PathBuf>>,
    /// Temporary directory that Native made for this session.
    pub work: PathBuf,
}

/// What the supervisor established when the game paused.
#[derive(Debug, Clone)]
struct Paused {
    readiness: GameReadiness,
    /// By internal registry name.
    registries: BTreeMap<String, RegistryItems>,
}

/// How a session ended, from the supervisor's final report.
#[derive(Debug, Clone)]
struct Finished {
    outcome: SessionOutcome,
    disposal: Disposal,
    diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct State {
    paused: Option<Paused>,
    /// `Err` when the connection to the supervisor failed; disposal is then not established.
    finished: Option<Result<Finished, Error>>,
}
enum DriverCommand {
    /// The caller answered a question about this registry; the idle time starts again.
    Read {
        name: String,
        reply: oneshot::Sender<Result<(), Error>>,
    },
}

/// An owned process paused at registry initialization, never a loaded world.
/// Drop requests cleanup. Await close for independently confirmed disposal.
#[derive(Debug)]
pub struct Game {
    commands: Option<mpsc::SyncSender<DriverCommand>>,
    stop: Arc<AtomicU8>,
    state: watch::Receiver<State>,
    paused: Paused,
    closing: bool,
    /// Content directory of each observed registry, with its internal name.
    directories: BTreeMap<String, String>,
    build: crate::BuildId,
    /// Read every answer from this directory; no process exists.
    recorded: Option<Arc<PathBuf>>,
    /// Write every answer to this directory as it is returned.
    recorder: Option<Arc<PathBuf>>,
    /// Temporary work directory that Native made. Removed after a clean close.
    pub(crate) work: Option<PathBuf>,
}
impl Game {
    /// A session over recorded answers. No supervisor or game process is started.
    pub(crate) fn recorded(directory: Arc<PathBuf>) -> Self {
        let paused = Paused {
            readiness: GameReadiness::PausedAfterRegistryInitialization,
            registries: BTreeMap::new(),
        };
        let (_, state) = watch::channel(State::default());
        Self {
            commands: None,
            stop: Arc::new(AtomicU8::new(0)),
            state,
            paused,
            closing: false,
            directories: BTreeMap::new(),
            build: crate::BuildId("recorded".into()),
            recorded: Some(directory),
            recorder: None,
            work: None,
        }
    }

    /// List the item names of one registry, as the engine holds them after its initial load.
    ///
    /// The registry is named by its content directory, such as `common/traditions`. Every call
    /// returns the same startup observation; the game is never resumed. Cancelling this future
    /// leaves the session alive.
    pub async fn registry_items(
        &mut self,
        registry: &str,
    ) -> Result<crate::Answer<Vec<String>>, crate::Error> {
        use crate::Error;
        if let Some(directory) = &self.recorded {
            if self.closing {
                return Err(Error::Closed);
            }
            return crate::recorded::read(directory, "registry_items", Some(registry));
        }
        let answer = self.registry_items_from_game(registry).await;
        if let Some(directory) = &self.recorder {
            crate::recorded::write(directory, "registry_items", Some(registry), &answer)?;
        }
        answer
    }

    async fn registry_items_from_game(
        &mut self,
        registry: &str,
    ) -> Result<crate::Answer<Vec<String>>, Error> {
        use crate::{Answer, Basis, Completeness, Gap, GapKind, Operation, Source};
        let operation = Operation::RegistryItems;
        let directory = registry.trim_end_matches('/');
        // Item observation covers only the registries that the build's live recipe binds. Another
        // name can be a real registry, so this is not `UnknownRegistry`.
        let Some(name) = self.directories.get(directory).cloned() else {
            let covered: Vec<_> = self.directories.keys().cloned().collect();
            return Err(Error::Unsupported {
                operation,
                reason: format!("item observation covers only: {}", covered.join(", ")),
            });
        };
        if self.closing || self.state.borrow().finished.is_some() {
            return Err(Error::Closed);
        }
        let observed = self
            .paused
            .registries
            .get(&name)
            .filter(|items| items.observed != Observed::Unavailable)
            .cloned();
        let Some(observed) = observed else {
            let diagnostics = self.paused.registries.get(&name);
            return Err(Error::Observation {
                operation,
                reason: diagnostics.map_or_else(
                    || "The supervisor sent no observation of this registry".into(),
                    |items| items.diagnostics.join("; "),
                ),
            });
        };
        self.restart_idle_time(name).await?;
        let complete = observed.observed == Observed::Complete;
        Ok(Answer {
            value: observed.items,
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
                    subject: Some(directory.into()),
                    detail: "The engine collection was not read to its end.".into(),
                }]
            },
            source: Source::new(
                self.build.clone(),
                "registry-items/v1",
                Basis::LiveObservation,
            ),
        })
    }

    /// Tell the supervisor that the caller got an answer, and wait for its acknowledgement. The
    /// supervisor ends a session that stays idle.
    async fn restart_idle_time(&mut self, name: String) -> Result<(), Error> {
        let (reply, receive) = oneshot::channel();
        self.commands
            .as_ref()
            .ok_or(Error::Closed)?
            .try_send(DriverCommand::Read { name, reply })
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => {
                    Error::Supervisor("Too many pending registry reads".into())
                }
                mpsc::TrySendError::Disconnected(_) => Error::Closed,
            })?;
        receive
            .await
            .map_err(|_| Error::Supervisor("Registry read acknowledgement lost".into()))?
    }

    /// The witnessed initialization pause. This does not advertise gameplay readiness.
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
    /// was polled. The temporary work directory is removed after a confirmed disposal; after any
    /// other result it is kept for inspection.
    pub async fn close(&mut self) -> Result<Disposal, Error> {
        if self.recorded.is_some() {
            self.closing = true;
            return Ok(Disposal::NotApplicable);
        }
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
        if finished.disposal == Disposal::Confirmed
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

/// Start the driver thread and wait for the pause. A session that ends before its pause is a
/// failed start, whatever its outcome.
pub(crate) async fn start(
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
            return Ok(Game {
                commands: Some(commands),
                stop,
                state: changes,
                paused,
                closing: false,
                directories: session.directories,
                build: session.build,
                recorded: None,
                recorder: session.recorder,
                work: Some(session.work),
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
    fn finished() -> Finished {
        Finished {
            outcome: SessionOutcome::Completed,
            disposal: Disposal::Confirmed,
            diagnostics: Vec::new(),
        }
    }
    fn game() -> (Game, mpsc::Receiver<DriverCommand>, watch::Sender<State>) {
        let paused = Paused {
            readiness: GameReadiness::PausedDuringRegistryInitialization,
            registries: BTreeMap::new(),
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
                directories: BTreeMap::from([("common/traditions".into(), "traditions".into())]),
                build: crate::BuildId("test".into()),
                recorded: None,
                recorder: None,
                work: None,
            },
            receive,
            state,
        )
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
            "traditions".into(),
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
