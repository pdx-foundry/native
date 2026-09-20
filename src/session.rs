//! `Native`: one pinned installation, or one directory of recorded answers.
use crate::{OpenError, UnavailableReason, binding::Binding};
use std::sync::{Arc, Mutex, OnceLock};

mod questions;

/// A pinned installation. Static questions never start a game. `start_game` starts a game that
/// an independent supervisor process owns.
#[derive(Debug)]
pub struct Native {
    /// `None` only for recorded answers.
    binding: Option<Arc<Binding>>,
    /// Read every answer from this directory, and start no process.
    recorded: Option<Arc<crate::recorded::Answers>>,
    /// Write every answer to this directory as it is returned.
    recorder: Option<Arc<std::path::PathBuf>>,
    /// The first change of the executable or the pinned content that this `Native` saw. It
    /// stays, even when the original bytes come back.
    invalidated: Arc<Mutex<Option<UnavailableReason>>>,
    candidates: Arc<OnceLock<Result<Vec<crate::binding::NamedCandidate>, crate::AnalysisError>>>,
}

impl Native {
    /// Pin the installation at this location: an executable, an application bundle, or an
    /// installation directory. No process starts. A build that is not in the target catalogue is
    /// refused; there is no nearest-version fallback.
    pub fn open(installation: impl Into<std::path::PathBuf>) -> Result<Self, OpenError> {
        Ok(Self::from_binding(Binding::open(&installation.into())?))
    }
    /// Read every answer from recorded files. No installation is opened and no process starts.
    ///
    /// Static and live questions work the same as with a real game: `start_game` returns a
    /// `Game` that reads recorded answers. A question with no file returns `Error::NotRecorded`,
    /// and every answer carries `Basis::Recorded`. `build.json` must contain the original
    /// serialized `BuildId`; missing or invalid metadata returns `Error::Recorded`.
    pub fn from_recorded_answers(
        directory: impl Into<std::path::PathBuf>,
    ) -> Result<Self, crate::Error> {
        Ok(Self {
            binding: None,
            recorded: Some(Arc::new(crate::recorded::Answers::open(directory.into())?)),
            recorder: None,
            invalidated: Arc::new(Mutex::new(None)),
            candidates: Arc::new(OnceLock::new()),
        })
    }
    /// Write every answer, and every error, to this directory as it is returned. A later
    /// `from_recorded_answers` on the same directory then gives the same answers without a game.
    pub fn record_answers_to(mut self, directory: impl Into<std::path::PathBuf>) -> Self {
        self.recorder = Some(Arc::new(directory.into()));
        self
    }
    pub(crate) fn from_binding(binding: Binding) -> Self {
        Self {
            binding: Some(Arc::new(binding)),
            recorded: None,
            recorder: None,
            invalidated: Arc::new(Mutex::new(None)),
            candidates: Arc::new(OnceLock::new()),
        }
    }
    /// The installation binding. Recorded answers have none; every public method answers from
    /// the recorded files before it reaches this.
    pub(crate) fn bound(&self) -> &Arc<Binding> {
        self.binding
            .as_ref()
            .expect("recorded answers have no installation binding")
    }
    pub(crate) fn recorded(&self) -> Option<&crate::recorded::Answers> {
        self.recorded.as_deref()
    }
    pub(crate) fn recorder(&self) -> Option<&std::path::Path> {
        self.recorder.as_deref().map(|path| path.as_path())
    }
    /// Content directory of each registry that a game session observes, with its internal name.
    pub(crate) fn registry_directories(&self) -> std::collections::BTreeMap<String, String> {
        self.bound()
            .registry_names()
            .into_iter()
            .filter_map(|name| Some((self.bound().registry_directory(&name)?, name)))
            .collect()
    }
    fn integrity(&self) -> Option<UnavailableReason> {
        let mut invalidated = self.invalidated.lock().expect("context integrity lock");
        if invalidated.is_none() {
            *invalidated = self.bound().integrity();
        }
        invalidated.clone()
    }
    /// Every reason why a game session cannot start now. This may start the host's debugger
    /// tools to check them; it never starts the game.
    pub(crate) fn blocking_reasons(&self) -> Vec<UnavailableReason> {
        self.bound().blocking_reasons(self.integrity())
    }
    /// Start a supervised game and wait until it is paused after its registries load. The game
    /// never loads a world. With recorded answers, no process starts and the options are ignored.
    ///
    /// Dropping this future requests cleanup. No async runtime owns the process: an independent
    /// thread and the supervisor do, so cleanup continues if the caller is lost.
    pub async fn start_game(
        &self,
        options: crate::GameOptions,
    ) -> Result<crate::Game, crate::Error> {
        use crate::{Disposal, Error, Operation};
        if let Some(directory) = &self.recorded {
            return Ok(crate::Game::recorded(directory.clone()));
        }
        let reasons = self.blocking_reasons();
        if reasons.contains(&UnavailableReason::TargetChanged) {
            return Err(Error::BuildChanged);
        }
        if !reasons.is_empty() {
            return Err(Error::Unsupported {
                operation: Operation::RegistryItems,
                reason: format!("{reasons:?}"),
            });
        }
        // The supervisor knows a registry by its internal name.
        let directories = self.registry_directories();
        let fault = match options.fault {
            Some((directory, control)) => {
                let registry = directories.get(&directory).cloned();
                let registry = registry.ok_or(Error::UnknownRegistry { name: directory })?;
                Some(crate::protocol::session::Fault { registry, control })
            }
            None => None,
        };
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let work = std::env::temp_dir().join(format!("pdx-native-{}-{id}", std::process::id()));
        let request = crate::protocol::session::SessionRequest {
            installation: self.bound().installation_location(),
            build: self.bound().build().into(),
            // The supervisor creates this directory; `work` holds nothing else.
            work_directory: work.join("session"),
            startup_seconds: options.startup_seconds,
            idle_seconds: options.idle_seconds,
            fault,
        };
        request.validate().map_err(|error| Error::Startup {
            reason: error.to_string(),
            disposal: Disposal::NotApplicable,
        })?;
        std::fs::create_dir_all(&work).map_err(|error| Error::Startup {
            reason: format!("work directory: {error}"),
            disposal: Disposal::NotApplicable,
        })?;
        let session = crate::game::Session {
            directories,
            build: self.build(),
            recorder: self.recorder.clone(),
            work,
        };
        crate::game::start(options.supervisor, request, session).await
    }
}
