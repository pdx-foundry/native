//! `Native`: one pinned installation, or one directory of recorded answers.
use crate::{OpenError, UnavailableReason, binding::Binding};
use std::sync::{Arc, Mutex};

pub(crate) use loaded_modifiers::ModifierJoin;

mod callbacks;
mod defines;
mod families;
mod fields;
mod language;
mod loaded_modifiers;
mod localization;
pub(crate) mod questions;
pub mod registry_field_stops;

#[derive(Debug)]
enum Backend {
    Live {
        binding: Arc<Binding>,
        /// Write every answer to this directory as it is returned.
        recorder: Option<Arc<std::path::PathBuf>>,
    },
    Recorded(Arc<crate::recorded::Answers>),
}

/// A pinned installation. Static questions never start a game. `start_game` starts a game that
/// an independent supervisor process owns.
#[derive(Debug)]
pub struct Native {
    backend: Backend,
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
            target_invalidated: Arc::new(Mutex::new(None)),
            default_invalidated: Arc::new(Mutex::new(None)),
        })
    }
    /// Write every answer, and every error, to this directory as it is returned. A later
    /// `from_recorded_answers` on the same directory then gives the same answers without a game.
    /// Recorded answers are not written again.
    pub fn record_answers_to(mut self, directory: impl Into<std::path::PathBuf>) -> Self {
        if let Backend::Live { recorder, .. } = &mut self.backend {
            *recorder = Some(Arc::new(directory.into()));
        }
        self
    }
    pub(crate) fn from_binding(binding: Binding) -> Self {
        Self {
            backend: Backend::Live {
                binding: Arc::new(binding),
                recorder: None,
            },
            target_invalidated: Arc::new(Mutex::new(None)),
            default_invalidated: Arc::new(Mutex::new(None)),
        }
    }
    /// The installation binding. Recorded answers have none; every public method answers from
    /// the recorded files before it reaches this.
    pub(crate) fn bound(&self) -> &Arc<Binding> {
        match &self.backend {
            Backend::Live { binding, .. } => binding,
            Backend::Recorded(_) => panic!("recorded answers have no installation binding"),
        }
    }
    fn target_integrity(&self, binding: &Binding) -> Option<UnavailableReason> {
        let mut invalidated = self
            .target_invalidated
            .lock()
            .expect("target integrity lock");
        if invalidated.is_none() {
            *invalidated = binding.target_integrity();
        }
        invalidated.clone()
    }
    fn integrity(&self, binding: &Binding) -> Option<UnavailableReason> {
        if let Some(reason) = self.target_integrity(binding) {
            return Some(reason);
        }
        let mut invalidated = self
            .default_invalidated
            .lock()
            .expect("default content integrity lock");
        if invalidated.is_none() {
            *invalidated = binding.default_content_integrity();
        }
        invalidated.clone()
    }
    /// Every reason why a game session cannot start now. This may start the host's debugger
    /// tools to check them; it never starts the game.
    pub(crate) fn blocking_reasons(&self, binding: &Binding) -> Vec<UnavailableReason> {
        binding.blocking_reasons(self.integrity(binding), true)
    }
    /// Method and host availability without a particular content selection.
    pub(crate) fn selected_blocking_reasons(&self, binding: &Binding) -> Vec<UnavailableReason> {
        binding.blocking_reasons(self.target_integrity(binding), false)
    }
    /// Start a supervised game and wait until it is paused after its registries load, or after
    /// all content loads with `GameOptions::loaded_modifiers`. The game never loads a world. With recorded answers, no process starts; the fixture selects its recording and launch options are ignored.
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
        let (binding, recorder) = match &self.backend {
            Backend::Recorded(answers) => {
                return Ok(crate::Game::recorded(answers.clone(), options.fixture));
            }
            Backend::Live { binding, recorder } => (binding, recorder),
        };
        if !(1..=crate::protocol::session::MAX_SESSION_SECONDS).contains(&options.startup_seconds)
            || !(1..=crate::protocol::session::MAX_SESSION_SECONDS).contains(&options.idle_seconds)
        {
            return Err(Error::Startup {
                reason: "Startup and idle budgets must be 1 to 180 seconds".into(),
                disposal: Disposal::NotApplicable,
            });
        }
        if options.fixture.is_some() && !binding.has_fixture_method() {
            return Err(Error::Unsupported {
                operation: Operation::ObserveFixture,
                reason: "this build has no fixture observation recipe".into(),
            });
        }
        let reasons = if options.registries.is_some() {
            self.selected_blocking_reasons(binding)
        } else {
            self.blocking_reasons(binding)
        };
        if reasons.contains(&UnavailableReason::TargetChanged) {
            return Err(Error::BuildChanged);
        }
        if !reasons.is_empty() {
            return Err(Error::Unsupported {
                operation: if options.loaded_modifiers {
                    Operation::LoadedModifiers
                } else if options.fixture.is_some() {
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
            .unwrap_or_else(|| binding.default_registries());
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
        let modifiers = if options.loaded_modifiers {
            if !binding.has_modifier_table_method() {
                return Err(Error::Unsupported {
                    operation: Operation::LoadedModifiers,
                    reason: "this build has no loaded modifier table recipe".into(),
                });
            }
            Some(self.modifier_join(options.fixture.as_ref())?)
        } else {
            None
        };
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let work = std::env::temp_dir().join(format!("pdx-native-{}-{id}", std::process::id()));
        let request = session_request(binding, &options, &registries, &work, modifiers.as_ref());
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
            recorder: recorder.clone(),
            work,
            fixture: options.fixture,
            modifiers,
        };
        crate::game::start(options.supervisor, request, session).await
    }
}

/// The supervisor's request for one live session. A fixture's deadline also bounds startup.
fn session_request(
    binding: &Binding,
    options: &crate::GameOptions,
    registries: &[String],
    work: &std::path::Path,
    modifiers: Option<&ModifierJoin>,
) -> crate::protocol::session::SessionRequest {
    let startup_seconds = options
        .fixture
        .as_ref()
        .map_or(options.startup_seconds, |fixture| {
            options.startup_seconds.min(fixture.deadline_seconds)
        });

    crate::protocol::session::SessionRequest {
        installation: binding.installation_location(),
        build: binding.build().into(),
        // The supervisor creates this directory; `work` holds nothing else.
        work_directory: work.join("session"),
        startup_seconds,
        idle_seconds: options.idle_seconds,
        registries: registries.to_vec(),
        fault: options.fault.clone(),
        fixture: options.fixture.clone(),
        loaded_modifiers: modifiers.map(ModifierJoin::registries),
    }
}
