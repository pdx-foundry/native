//! `Native`: one pinned installation, or one directory of recorded answers.
use crate::{OpenError, UnavailableReason, binding::Binding};
use std::sync::{Arc, Mutex};

pub(crate) use loaded_modifiers::ModifierJoin;

mod callbacks;
pub mod command_grammar_stops;
mod container_masks;
mod defines;
pub mod duration_groups;
mod durations;
pub mod dynamic_name_commands;
mod dynamic_names;
mod families;
mod field_entries;
mod fields;
pub(crate) mod grammar;
mod language;
mod loaded_modifiers;
mod localization;
mod modifier_blocks;
mod modifier_nodes;
mod names;
mod numeric;
pub mod numeric_readers;
pub(crate) mod questions;
mod read_scope;
pub mod reference_readers;
pub mod registry_field_stops;
mod scoped_numeric;
pub(crate) mod script_durations;
pub mod target_getters;
mod triggered_modifiers;
mod weight_blocks;

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
    /// The first default-content change seen by this context. It stays invalidated even when
    /// the original bytes return. The binding keeps the first executable change.
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
    fn integrity(&self, binding: &Binding) -> Option<UnavailableReason> {
        if let Some(reason) = binding.target_integrity() {
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
        binding.blocking_reasons(binding.target_integrity(), false)
    }
    /// Start a supervised game and wait until it is paused after its registries load, or after
    /// all content loads with `GameOptions::loaded_modifiers`. With recorded answers, no process
    /// starts; the prepared fixture selects its recording.
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
        if !(1..=crate::protocol::session::MAX_SESSION_SECONDS).contains(&options.startup_seconds) {
            return Err(Error::Startup {
                reason: "Startup budget must be 1 to 180 seconds".into(),
                disposal: Disposal::NotApplicable,
            });
        }
        if options.fixture.is_some() && !binding.has_fixture_method() {
            return Err(Error::Unsupported {
                operation: Operation::ObserveFixture,
                reason: "this build has no fixture observation recipe".into(),
            });
        }
        let reasons = if options.loader_registry.is_some() || options.fixture.is_some() {
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
                    Operation::Registries
                },
                reason: format!("{reasons:?}"),
            });
        }
        if options.loaded_modifiers
            && binding.has_script_check_method()
            && let Some(analysis) = &binding.analysis
        {
            // A failure leaves every check's duration table empty, so its answer is partial.
            let _ = analysis.prepare_script_durations();
        }
        let mut registries = match &options.loader_registry {
            Some(registry) => vec![registry.clone()],
            None => binding.default_registries(),
        };
        if let Some(fixture) = &options.fixture
            && !binding
                .analysis
                .as_ref()
                .is_some_and(|analysis| analysis.has_inline_fixture(fixture.registry()))
            && !registries.iter().any(|name| name == fixture.registry())
        {
            registries.push(fixture.registry().into());
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
            keep_work: options.keep_work_directory,
            fixture: options.fixture,
            modifiers,
            binding: binding.clone(),
        };
        crate::game::start(options.supervisor, request, session).await
    }
}

/// The supervisor's request for one live session, with a fixed idle timeout.
fn session_request(
    binding: &Binding,
    options: &crate::GameOptions,
    registries: &[String],
    work: &std::path::Path,
    modifiers: Option<&ModifierJoin>,
) -> crate::protocol::session::SessionRequest {
    crate::protocol::session::SessionRequest {
        installation: binding.installation_location(),
        build: binding.build().into(),
        // The supervisor creates this directory; `work` holds nothing else.
        work_directory: work.join("session"),
        startup_seconds: options.startup_seconds,
        idle_seconds: crate::protocol::session::MAX_SESSION_SECONDS,
        registries: registries.to_vec(),
        fault: options.fault.clone(),
        fixture: options.fixture.clone(),
        loaded_modifiers: modifiers.map(ModifierJoin::registries),
    }
}

/// Run one bounded loader-rule control for Native's development tests.
/// The result is an initial-load key observation; no recording or persistent game is returned.
pub async fn check_registry_load(
    native: &Native,
    mut options: crate::GameOptions,
    registry: &str,
) -> Result<crate::Answer<Vec<String>>, crate::Error> {
    if matches!(native.backend, Backend::Recorded(_)) {
        return Err(crate::Error::Unsupported {
            operation: crate::Operation::Registries,
            reason: "loader controls require a live installation".into(),
        });
    }
    if let Some(fixture) = &options.fixture {
        fixture.validate()?;
    }
    let registry = registry.trim_end_matches('/');
    if options.loaded_modifiers
        || options
            .fixture
            .as_ref()
            .is_some_and(|fixture| fixture.registry() != registry)
    {
        return Err(crate::Error::FixtureRequest {
            reason: "loader controls observe one registry and its optional fixture".into(),
        });
    }
    options.loader_registry = Some(registry.into());
    let mut game = native.start_game(options).await?;
    let result = game.loader_items(registry).await;
    game.close().await?;
    result
}
