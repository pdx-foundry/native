//! `Native`: one pinned installation, or one directory of recorded answers.
use crate::{OpenError, UnavailableReason, binding::Binding};
use std::sync::{Arc, Mutex};

mod callbacks;
mod defines;
mod families;
mod language;
mod localization;
pub(crate) mod questions;

#[derive(Debug)]
enum Backend {
    Live(Arc<Binding>),
    Recorded(Arc<crate::recorded::Answers>),
}

/// A pinned installation. Static questions never start a game. `start_game` starts a game that
/// an independent supervisor process owns.
#[derive(Debug)]
pub struct Native {
    backend: Backend,
    /// Write every answer to this directory as it is returned.
    recorder: Option<Arc<std::path::PathBuf>>,
    /// The first executable change and the first default-content change seen by this context.
    /// Each stays invalidated even when the original bytes return.
    target_invalidated: Arc<Mutex<Option<UnavailableReason>>>,
    default_invalidated: Arc<Mutex<Option<UnavailableReason>>>,
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
            backend: Backend::Recorded(Arc::new(crate::recorded::Answers::open(directory.into())?)),
            recorder: None,
            target_invalidated: Arc::new(Mutex::new(None)),
            default_invalidated: Arc::new(Mutex::new(None)),
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
            backend: Backend::Live(Arc::new(binding)),
            recorder: None,
            target_invalidated: Arc::new(Mutex::new(None)),
            default_invalidated: Arc::new(Mutex::new(None)),
        }
    }
    /// The installation binding. Recorded answers have none; every public method answers from
    /// the recorded files before it reaches this.
    pub(crate) fn bound(&self) -> &Arc<Binding> {
        match &self.backend {
            Backend::Live(binding) => binding,
            Backend::Recorded(_) => panic!("recorded answers have no installation binding"),
        }
    }
    pub(crate) fn recorded(&self) -> Option<&crate::recorded::Answers> {
        match &self.backend {
            Backend::Recorded(answers) => Some(answers),
            Backend::Live(_) => None,
        }
    }
    pub(crate) fn recorder(&self) -> Option<&std::path::Path> {
        self.recorder.as_deref().map(|path| path.as_path())
    }
    fn target_integrity(&self) -> Option<UnavailableReason> {
        let mut invalidated = self
            .target_invalidated
            .lock()
            .expect("target integrity lock");
        if invalidated.is_none() {
            *invalidated = self.bound().target_integrity();
        }
        invalidated.clone()
    }
    fn integrity(&self) -> Option<UnavailableReason> {
        if let Some(reason) = self.target_integrity() {
            return Some(reason);
        }
        let mut invalidated = self
            .default_invalidated
            .lock()
            .expect("default content integrity lock");
        if invalidated.is_none() {
            *invalidated = self.bound().default_content_integrity();
        }
        invalidated.clone()
    }
    /// Every reason why a game session cannot start now. This may start the host's debugger
    /// tools to check them; it never starts the game.
    pub(crate) fn blocking_reasons(&self) -> Vec<UnavailableReason> {
        self.bound().blocking_reasons(self.integrity(), true)
    }
    /// Method and host availability without a particular content selection.
    pub(crate) fn selected_blocking_reasons(&self) -> Vec<UnavailableReason> {
        self.bound()
            .blocking_reasons(self.target_integrity(), false)
    }
    /// Start a supervised game and wait until it is paused after its registries load. The game
    /// never loads a world. With recorded answers, no process starts; the fixture selects its recording and launch options are ignored.
    ///
    /// Dropping this future requests cleanup. No async runtime owns the process: an independent
    /// thread and the supervisor do, so cleanup continues if the caller is lost.
    pub async fn start_game(
        &self,
        options: crate::GameOptions,
    ) -> Result<crate::Game, crate::Error> {
        use crate::{Disposal, Error, Operation};
        if let Some(fixture) = &options.fixture {
            fixture.validate()?;
        }
        if let Backend::Recorded(answers) = &self.backend {
            return Ok(crate::Game::recorded(answers.clone(), options.fixture));
        }
        if !(1..=crate::protocol::session::MAX_SESSION_SECONDS).contains(&options.startup_seconds)
            || !(1..=crate::protocol::session::MAX_SESSION_SECONDS).contains(&options.idle_seconds)
        {
            return Err(Error::Startup {
                reason: "Startup and idle budgets must be 1 to 180 seconds".into(),
                disposal: Disposal::NotApplicable,
            });
        }
        if options.fixture.is_some() && !self.bound().has_fixture_method() {
            return Err(Error::Unsupported {
                operation: Operation::ObserveFixture,
                reason: "this build has no fixture observation recipe".into(),
            });
        }
        let reasons = if options.registries.is_some() {
            self.selected_blocking_reasons()
        } else {
            self.blocking_reasons()
        };
        if reasons.contains(&UnavailableReason::TargetChanged) {
            return Err(Error::BuildChanged);
        }
        if !reasons.is_empty() {
            return Err(Error::Unsupported {
                operation: if options.fixture.is_some() {
                    Operation::ObserveFixture
                } else {
                    Operation::RegistryItems
                },
                reason: format!("{reasons:?}"),
            });
        }
        let registries = options
            .registries
            .clone()
            .unwrap_or_else(|| self.bound().default_registries());
        if let Some(fixture) = &options.fixture
            && !registries.iter().any(|name| name == fixture.registry())
        {
            return Err(Error::FixtureRequest {
                reason: format!(
                    "select {} in GameOptions::registries to observe this fixture",
                    fixture.registry()
                ),
            });
        }
        let known: std::collections::BTreeSet<_> = self
            .registries()?
            .value
            .into_iter()
            .map(|registry| registry.name)
            .collect();
        for registry in &registries {
            if !known.contains(registry) {
                return Err(Error::UnknownRegistry {
                    name: registry.clone(),
                });
            }
        }
        let fault = options
            .fault
            .map(|(directory, control)| crate::protocol::session::Fault {
                registry: directory,
                control,
            });
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let work = std::env::temp_dir().join(format!("pdx-native-{}-{id}", std::process::id()));
        let request = crate::protocol::session::SessionRequest {
            installation: self.bound().installation_location(),
            build: self.bound().build().into(),
            // The supervisor creates this directory; `work` holds nothing else.
            work_directory: work.join("session"),
            startup_seconds: options
                .fixture
                .as_ref()
                .map_or(options.startup_seconds, |fixture| {
                    options.startup_seconds.min(fixture.deadline_seconds)
                }),
            idle_seconds: options.idle_seconds,
            registries: registries.clone(),
            fault,
            fixture: options.fixture.clone(),
            fixture_fault: options.fixture_fault,
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
            observed: registries.into_iter().collect(),
            build: self.build(),
            recorder: self.recorder.clone(),
            work,
            fixture: options.fixture,
        };
        crate::game::start(options.supervisor, request, session).await
    }
}
