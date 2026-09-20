use crate::{
    CapabilityReport, CapabilityRequest, ContextIdentity, ContextOrigin, OpenError, OpenRequest,
    UnavailableReason, binding::Binding, qualification,
};
use std::sync::{Arc, Mutex, OnceLock};

mod questions;

/// A pinned installation. Static queries never launch a game or probe a debugger.
/// Configure a consumer supervisor before starting an independently owned Game.
#[derive(Debug)]
pub struct Native {
    /// `None` only for recorded answers.
    binding: Option<Arc<Binding>>,
    /// Read every answer from this directory, and start no process.
    recorded: Option<Arc<std::path::PathBuf>>,
    /// Write every answer to this directory as it is returned.
    recorder: Option<Arc<std::path::PathBuf>>,
    invalidated: Arc<Mutex<Option<UnavailableReason>>>,
    pub(crate) hosting: Option<crate::game::Hosting>,
    candidates: Arc<OnceLock<Result<Vec<crate::binding::NamedCandidate>, crate::AnalysisError>>>,
}

impl Native {
    /// Pin the installation at this location: an executable, an application bundle, or an
    /// installation directory. No process starts. A build that is not in the target catalogue is
    /// refused; there is no nearest-version fallback.
    pub fn open(installation: impl Into<std::path::PathBuf>) -> Result<Self, OpenError> {
        Ok(Self::from_binding(Binding::open(OpenRequest {
            installation_hint: installation.into(),
        })?))
    }
    /// Read every answer from recorded files. No installation is opened and no process starts.
    ///
    /// Static and live questions work the same as with a real game: `start_game` returns a
    /// `Game` that reads recorded answers. A question with no file returns `Error::NotRecorded`,
    /// and every answer carries `Basis::Recorded`.
    pub fn from_recorded_answers(directory: impl Into<std::path::PathBuf>) -> Self {
        Self {
            binding: None,
            recorded: Some(Arc::new(directory.into())),
            recorder: None,
            invalidated: Arc::new(Mutex::new(None)),
            hosting: None,
            candidates: Arc::new(OnceLock::new()),
        }
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
            hosting: None,
            candidates: Arc::new(OnceLock::new()),
        }
    }
    /// The installation binding. Recorded answers have none; only the earlier hidden methods
    /// reach this without a check, and they are never used on recorded answers.
    pub(crate) fn bound(&self) -> &Arc<Binding> {
        self.binding
            .as_ref()
            .expect("recorded answers have no installation binding")
    }
    pub(crate) fn recorded(&self) -> Option<&std::path::Path> {
        self.recorded.as_deref().map(|path| path.as_path())
    }
    pub(crate) fn recorder(&self) -> Option<&std::path::Path> {
        self.recorder.as_deref().map(|path| path.as_path())
    }
    pub(crate) fn registry_names(&self) -> Vec<String> {
        self.bound().registry_names()
    }
    /// Content directory of each live registry, with its internal name.
    pub(crate) fn registry_directories(&self) -> std::collections::BTreeMap<String, String> {
        self.bound()
            .registry_names()
            .into_iter()
            .filter_map(|name| Some((self.bound().registry_directory(&name)?, name)))
            .collect()
    }
    pub(crate) fn detached_context(&self) -> Self {
        Self {
            binding: self.binding.clone(),
            recorded: self.recorded.clone(),
            recorder: self.recorder.clone(),
            invalidated: self.invalidated.clone(),
            hosting: None,
            candidates: self.candidates.clone(),
        }
    }
    #[doc(hidden)]
    /// Opaque pinned composition identity. Equality does not establish current integrity.
    pub fn identity(&self) -> ContextIdentity {
        self.bound().identity()
    }
    #[doc(hidden)]
    /// Installation input origin.
    pub fn origin(&self) -> ContextOrigin {
        self.bound().origin()
    }
    fn integrity(&self) -> Option<UnavailableReason> {
        let mut invalidated = self.invalidated.lock().expect("context integrity lock");
        if invalidated.is_none() {
            *invalidated = self.bound().integrity();
        }
        invalidated.clone()
    }
    #[doc(hidden)]
    /// Inspect one live operation. Admission may probe live prerequisites; it never launches
    /// Stellaris. Static questions need no admission.
    pub fn capability(&self, request: &CapabilityRequest) -> CapabilityReport {
        qualification::evaluate(
            &self.bound().current_inputs(),
            request,
            self.origin(),
            self.integrity(),
        )
    }
    /// The earlier configuration, with a retention directory that the caller supplies.
    #[doc(hidden)]
    pub fn with_supervisor(
        mut self,
        command: std::process::Command,
        options: crate::game::RetentionOptions,
    ) -> Result<Self, crate::GameError> {
        // Recorded answers start no process, so the supervisor is not needed.
        if self.recorded.is_none() {
            self.hosting = Some(crate::game::Hosting::new(command, options)?);
        }
        Ok(self)
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
        if let Some(directory) = &self.recorded {
            return Ok(crate::Game::recorded(directory.clone()));
        }
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let work = std::env::temp_dir().join(format!("pdx-native-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&work).map_err(|error| crate::Error::Startup {
            reason: format!("work directory: {error}"),
            disposal: crate::Disposal::NotApplicable,
        })?;
        let mut retention = crate::game::RetentionOptions::new(work.clone());
        retention.startup_seconds = options.startup_seconds;
        retention.idle_seconds = options.idle_seconds;
        let hosting = crate::game::Hosting::new(options.supervisor, retention)?;
        let mut game = crate::game::start(
            self.detached_context(),
            hosting,
            crate::operation::Authorization::Admitted,
            None,
        )
        .await?;
        game.work = Some(work);
        Ok(game)
    }
    /// The earlier start, configured by `with_supervisor`.
    #[doc(hidden)]
    pub async fn start_game_with_report(&self) -> Result<crate::Game, crate::GameError> {
        crate::game::start(
            self.detached_context(),
            self.hosting
                .clone()
                .ok_or(crate::GameError::NotConfigured)?,
            crate::operation::Authorization::Admitted,
            None,
        )
        .await
    }
    pub(crate) fn prepare_session(
        &self,
        output: std::path::PathBuf,
        options: &crate::game::RetentionOptions,
        authorization: crate::operation::Authorization,
        control: Option<(String, crate::operation::ObservationControl)>,
    ) -> Result<crate::operation::PreparedPlan, crate::GameError> {
        let names = self.bound().registry_names();
        if authorization == crate::operation::Authorization::Admitted {
            let reports: Vec<_> = names
                .iter()
                .map(|name| {
                    (
                        name.clone(),
                        self.capability(&CapabilityRequest::Registry {
                            registry: name.clone(),
                        }),
                    )
                })
                .collect();
            if !reports
                .iter()
                .any(|(_, report)| report.availability == crate::Availability::Available)
            {
                return Err(crate::GameError::Unavailable {
                    registries: reports
                        .into_iter()
                        .map(|(name, report)| (name, report.reasons))
                        .collect(),
                });
            }
        }
        if let Some(reason) = self.integrity() {
            return Err(crate::GameError::InputsChanged(reason));
        }
        let (control_registry, control) = match control {
            Some((name, control)) => (Some(name), control),
            None => (None, crate::operation::ObservationControl::Normal),
        };
        let plan = crate::operation::PlanRequest {
            request: crate::operation::AttemptRequest {
                installation_hint: self.bound().installation_hint()?,
                output,
                hold_ms: 1,
            },
            composition: self.identity().0,
            authorization,
            observation: Some(crate::operation::ObservationSpec {
                registry: None,
                fixture: String::new(),
                deadline_seconds: options.startup_seconds,
                control,
                session: Some(crate::operation::SessionSpec {
                    idle_seconds: options.idle_seconds,
                    control_registry,
                }),
            }),
        };
        plan.validate(authorization)?;
        Ok(crate::operation::PreparedPlan { request: plan })
    }
}
