//! Async consumer sessions; independent threads and the external supervisor own process lifetime.
use crate::{
    ArtifactReference, GameReadiness, RegistryError, RegistryResult, ReplayRequest, operation,
};
use serde::Serialize;
use std::sync::atomic::{AtomicU8, Ordering};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    process::Command,
    sync::{Arc, Mutex, mpsc},
};
use tokio::sync::{oneshot, watch};

mod driver;

/// Session retention and deadlines. Cleanup has its own independent bounded budget.
#[derive(Debug, Clone)]
pub struct GameOptions {
    /// Existing absolute directory for new immutable attempts.
    pub retention_directory: PathBuf,
    /// Initialization budget in seconds, 1–180; the constructor defaults to 180.
    pub startup_seconds: u64,
    /// Idle budget in seconds, 1–180, reset by successful registry reads.
    pub idle_seconds: u64,
}
impl GameOptions {
    /// Use 180-second startup and idle budgets.
    pub fn new(retention_directory: PathBuf) -> Self {
        Self {
            retention_directory,
            startup_seconds: 180,
            idle_seconds: 180,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Hosting {
    command: Arc<Mutex<Command>>,
    options: GameOptions,
}
impl Hosting {
    pub(crate) fn new(command: Command, mut options: GameOptions) -> Result<Self, GameError> {
        if !options.retention_directory.is_absolute()
            || !options.retention_directory.is_dir()
            || !(1..=180).contains(&options.startup_seconds)
            || !(1..=180).contains(&options.idle_seconds)
        {
            return Err(GameError::InvalidOptions(
                "Expected an existing absolute retention directory and 1–180 second budgets".into(),
            ));
        }
        options.retention_directory = options
            .retention_directory
            .canonicalize()
            .map_err(|error| GameError::InvalidOptions(error.to_string()))?;
        Ok(Self {
            command: Arc::new(Mutex::new(command)),
            options,
        })
    }
}

/// Session startup or transport failure. Only a retained owner report can confirm disposal.
#[derive(Debug, Clone, Serialize)]
pub enum GameError {
    /// No consumer supervisor has been configured.
    NotConfigured,
    /// Retention or deadline configuration is invalid.
    InvalidOptions(String),
    /// None of the declared registries is currently admitted.
    Unavailable {
        /// Per-registry admission failures.
        registries: BTreeMap<String, Vec<crate::UnavailableReason>>,
    },
    /// The pinned installation changed or became unreadable.
    InputsChanged(crate::UnavailableReason),
    /// Startup ended without a safe usable pause. Partial results and cleanup remain available.
    StartupFailed(Box<GameReport>),
    /// Supervisor transport failed; cleanup confirmation is unavailable.
    Supervisor(String),
}
impl std::fmt::Display for GameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Game session failed: {self:?}")
    }
}
impl std::error::Error for GameError {}
impl From<crate::supervisor::SupervisorError> for GameError {
    fn from(error: crate::supervisor::SupervisorError) -> Self {
        Self::Supervisor(error.to_string())
    }
}

/// Availability of one registry, independent of the other registries in a session.
#[derive(Debug, Clone, Serialize)]
pub enum RegistryAvailability {
    /// A readable snapshot exists; its completeness is reported separately.
    Available {
        /// Complete or partial collection evidence.
        completion: crate::Completion,
    },
    /// Admission, activation, access, or evidence retention did not establish this answer.
    Unavailable {
        /// Specific observation or retention gaps.
        diagnostics: Vec<String>,
    },
}

/// Final session results and independently established process disposal.
#[derive(Debug, Clone, Serialize)]
pub struct GameReport {
    /// Pinned composition identity.
    pub context: crate::ContextIdentity,
    /// Unique supervised attempt.
    pub attempt: String,
    /// Last established initialization readiness, if any.
    pub readiness: Option<GameReadiness>,
    /// Why the session ended; independent of item completeness.
    pub outcome: crate::OperationOutcome,
    /// Independent owner disposal confirmation.
    pub disposal: crate::OperationDisposal,
    /// Whether the durable host reservation was resolved.
    pub reservation_resolved: bool,
    /// Each registry's retained observations or evidence-finalization failure.
    pub registries: BTreeMap<String, Result<RegistryResult, String>>,
    /// Final immutable per-registry replay references.
    pub replay: BTreeMap<String, ReplayRequest>,
    /// Retained attempt directory, including the owner report.
    pub retained: PathBuf,
    /// Additional retention and supervisor failures.
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone)]
struct Paused {
    readiness: GameReadiness,
    registries: BTreeMap<String, Result<RegistryResult, String>>,
    replay: BTreeMap<String, ReplayRequest>,
}
#[derive(Debug, Clone, Default)]
struct State {
    paused: Option<Paused>,
    finished: Option<Result<GameReport, GameError>>,
}
enum DriverCommand {
    Read {
        name: String,
        reply: oneshot::Sender<Result<(), RegistryError>>,
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
}
impl Game {
    /// List the item names of one registry, as the engine holds them after its initial load.
    ///
    /// The registry is named by its content directory, such as `common/traditions`. Every call
    /// returns the same startup observation; the game is never resumed. Cancelling this future
    /// leaves the session alive.
    pub async fn registry_items(
        &mut self,
        registry: &str,
    ) -> Result<crate::Answer<Vec<String>>, crate::Error> {
        use crate::{Answer, Basis, Completeness, Error, Gap, GapKind, Operation, Source};
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
        let result = self
            .get_registry_items(&name)
            .await
            .map_err(|error| match error {
                RegistryError::Closed => Error::Closed,
                RegistryError::Supervisor(reason) => Error::Supervisor(reason),
                RegistryError::Unsupported { .. } => Error::UnknownRegistry {
                    name: registry.into(),
                },
                RegistryError::Unavailable { reasons } => Error::Observation {
                    operation,
                    reason: format!("{reasons:?}"),
                },
                RegistryError::ObservationUnavailable { diagnostics, .. } => Error::Observation {
                    operation,
                    reason: diagnostics.join("; "),
                },
            })?;
        let complete = result.completion == crate::Completion::Complete;
        Ok(Answer {
            value: result
                .registered_items
                .into_iter()
                .map(|item| item.key)
                .collect(),
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

    /// The witnessed initialization pause. This does not advertise gameplay readiness.
    pub fn readiness(&self) -> GameReadiness {
        self.paused.readiness
    }
    #[doc(hidden)]
    /// Inspect every declared registry independently, without extending the idle deadline.
    pub fn registry_availability(&self) -> BTreeMap<String, RegistryAvailability> {
        self.paused
            .registries
            .iter()
            .map(|(name, result)| (name.clone(), availability(result)))
            .collect()
    }
    #[doc(hidden)]
    /// Immutable startup replay references. Replaying these does not confirm later cleanup.
    pub fn replay_references(&self) -> &BTreeMap<String, ReplayRequest> {
        &self.paused.replay
    }
    #[doc(hidden)]
    /// Read the same initial-loader snapshot on every call. No game execution is resumed.
    /// Cancelling this future leaves the session alive.
    pub async fn get_registry_items(
        &mut self,
        name: &str,
    ) -> Result<RegistryResult, RegistryError> {
        if self.closing || self.state.borrow().finished.is_some() {
            return Err(RegistryError::Closed);
        }
        let result =
            self.paused
                .registries
                .get(name)
                .ok_or_else(|| RegistryError::Unsupported {
                    registry: name.into(),
                })?;
        if let RegistryAvailability::Unavailable { diagnostics } = availability(result) {
            return Err(RegistryError::ObservationUnavailable {
                registry: name.into(),
                diagnostics,
            });
        }
        let result = result.as_ref().expect("available snapshot").clone();
        let (reply, receive) = oneshot::channel();
        self.commands
            .as_ref()
            .ok_or(RegistryError::Closed)?
            .try_send(DriverCommand::Read {
                name: name.into(),
                reply,
            })
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => {
                    RegistryError::Supervisor("Too many pending registry reads".into())
                }
                mpsc::TrySendError::Disconnected(_) => RegistryError::Closed,
            })?;
        receive.await.map_err(|_| {
            RegistryError::Supervisor("Registry read acknowledgement lost".into())
        })??;
        Ok(result)
    }
    /// Request cancellation. Close still returns the independent final cleanup report.
    pub fn cancel(&mut self) {
        if !self.closing {
            self.closing = true;
            let _ = self
                .stop
                .compare_exchange(0, 2, Ordering::SeqCst, Ordering::SeqCst);
        }
    }
    /// Close the session and await independent disposal. Repeated calls return the same report.
    /// Cleanup continues if this future is cancelled after it has been polled.
    pub async fn close(&mut self) -> Result<GameReport, GameError> {
        if !self.closing {
            self.closing = true;
            let _ = self
                .stop
                .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst);
        }
        loop {
            if let Some(result) = self.state.borrow().finished.clone() {
                return result;
            }
            self.state
                .changed()
                .await
                .map_err(|_| GameError::Supervisor("Session owner thread lost".into()))?;
        }
    }
}
impl Drop for Game {
    fn drop(&mut self) {
        self.commands.take();
    }
}
fn availability(result: &Result<RegistryResult, String>) -> RegistryAvailability {
    match result {
        Ok(result)
            if result.activation == crate::Activation::Demonstrated
                && matches!(
                    result.completion,
                    crate::Completion::Complete | crate::Completion::Incomplete
                ) =>
        {
            RegistryAvailability::Available {
                completion: result.completion,
            }
        }
        Ok(result) => RegistryAvailability::Unavailable {
            diagnostics: result.diagnostics.clone(),
        },
        Err(reason) => RegistryAvailability::Unavailable {
            diagnostics: vec![reason.clone()],
        },
    }
}

pub(crate) async fn start(
    context: crate::Native,
    hosting: Hosting,
    authorization: operation::Authorization,
    control: Option<(String, operation::ObservationControl)>,
) -> Result<Game, GameError> {
    let directories = context.registry_directories();
    let build = context.build();
    let (commands, receive) = mpsc::sync_channel(16);
    let stop = Arc::new(AtomicU8::new(0));
    let owner_stop = stop.clone();
    let (state, mut changes) = watch::channel(State::default());
    std::thread::Builder::new()
        .name("native-game-owner".into())
        .spawn(move || {
            driver::run(
                context,
                hosting,
                authorization,
                control,
                receive,
                owner_stop,
                state,
            );
        })
        .map_err(|error| GameError::Supervisor(error.to_string()))?;
    // The only command sender stays in this future until ownership moves into Game.
    loop {
        let current = changes.borrow().clone();
        if let Some(finished) = current.finished {
            return match finished {
                Ok(report) => Err(GameError::StartupFailed(Box::new(report))),
                Err(error) => Err(error),
            };
        }
        if let Some(paused) = current.paused {
            return Ok(Game {
                commands: Some(commands),
                stop,
                state: changes,
                paused,
                closing: false,
                directories,
                build,
            });
        }
        changes
            .changed()
            .await
            .map_err(|_| GameError::Supervisor("Session owner thread lost".into()))?;
    }
}

fn replay_requests(
    output: &std::path::Path,
    references: &BTreeMap<String, ArtifactReference>,
) -> Result<BTreeMap<String, ReplayRequest>, GameError> {
    references
        .iter()
        .map(|(name, reference)| {
            let path = std::path::Path::new(&reference.path);
            if path.is_absolute()
                || path
                    .components()
                    .any(|part| !matches!(part, std::path::Component::Normal(_)))
            {
                return Err(GameError::Supervisor(
                    "Invalid session evidence path".into(),
                ));
            }
            let mut descriptor = reference.clone();
            descriptor.path = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| GameError::Supervisor("Missing descriptor filename".into()))?
                .into();
            Ok((
                name.clone(),
                ReplayRequest {
                    artifact_root: output.join(path.parent().unwrap()),
                    descriptor,
                },
            ))
        })
        .collect()
}
fn replay_results(
    requests: &BTreeMap<String, ReplayRequest>,
    admitted: bool,
) -> BTreeMap<String, Result<RegistryResult, String>> {
    requests
        .iter()
        .map(|(name, request)| {
            let result = crate::Engine
                .replay_registry(request.clone())
                .map_err(|error| error.to_string())
                .and_then(|mut result| {
                    if result.registry != *name {
                        return Err("Registry descriptor does not match the requested name".into());
                    }
                    if admitted {
                        result.origin = crate::ResultOrigin::Live;
                    }
                    Ok(result)
                });
            (name.clone(), result)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn report() -> GameReport {
        GameReport {
            context: crate::ContextIdentity("test".into()),
            attempt: "test".into(),
            readiness: Some(GameReadiness::PausedDuringRegistryInitialization),
            outcome: crate::OperationOutcome::Completed,
            disposal: crate::OperationDisposal::Reaped,
            reservation_resolved: true,
            registries: BTreeMap::new(),
            replay: BTreeMap::new(),
            retained: std::env::temp_dir(),
            diagnostics: Vec::new(),
        }
    }
    fn game() -> (Game, mpsc::Receiver<DriverCommand>, watch::Sender<State>) {
        let paused = Paused {
            readiness: GameReadiness::PausedDuringRegistryInitialization,
            registries: BTreeMap::new(),
            replay: BTreeMap::new(),
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
            },
            receive,
            state,
        )
    }
    #[cfg(unix)]
    #[test]
    fn retention_alias_is_canonicalized_before_the_owner_report_join() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("captures");
        std::fs::create_dir(&target).unwrap();
        let alias = root.path().join("alias");
        std::os::unix::fs::symlink(&target, &alias).unwrap();
        let hosting =
            Hosting::new(Command::new("must-not-start"), GameOptions::new(alias)).unwrap();
        assert_eq!(
            hosting.options.retention_directory,
            target.canonicalize().unwrap()
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
            game.get_registry_items("traditions").await,
            Err(RegistryError::Closed)
        ));
        state.send_modify(|state| state.finished = Some(Ok(report())));
        let first = game.close().await.unwrap();
        let second = game.close().await.unwrap();
        assert_eq!(
            serde_json::to_value(first).unwrap(),
            serde_json::to_value(second).unwrap()
        );
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
    async fn unknown_names_do_not_send_reads_or_extend_idle_lifetime() {
        let (mut game, commands, _) = game();
        assert!(matches!(
            game.get_registry_items("unknown").await,
            Err(RegistryError::Unsupported { .. })
        ));
        assert!(matches!(
            commands.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
    }
    #[tokio::test]
    async fn registry_items_names_registries_by_directory_and_sends_no_read_for_other_names() {
        let (mut game, commands, _) = game();
        for unknown in ["traditions", "common/nothing"] {
            assert!(matches!(
                game.registry_items(unknown).await,
                Err(crate::Error::Unsupported { .. })
            ));
        }
        assert!(matches!(
            commands.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        game.cancel();
        assert!(matches!(
            game.registry_items("common/traditions").await,
            Err(crate::Error::Closed)
        ));
    }
    #[test]
    fn runtime_shutdown_drops_the_lease_without_requiring_async_cleanup() {
        let (game, commands, _) = game();
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
