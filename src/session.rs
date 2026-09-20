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
    binding: Arc<Binding>,
    invalidated: Arc<Mutex<Option<UnavailableReason>>>,
    pub(crate) hosting: Option<crate::game::Hosting>,
    candidates: Arc<OnceLock<Result<Vec<crate::binding::NamedCandidate>, crate::AnalysisError>>>,
}

/// Installation context retained for source compatibility with capability and replay callers.
pub type EngineContext = Native;

pub(crate) fn open(request: OpenRequest) -> Result<Native, OpenError> {
    Native::open(request)
}

impl Native {
    /// Pin an installation without starting a process.
    pub fn open(request: OpenRequest) -> Result<Self, OpenError> {
        Ok(Self::from_binding(Binding::open(request)?))
    }
    pub(crate) fn from_binding(binding: Binding) -> Self {
        Self {
            binding: Arc::new(binding),
            invalidated: Arc::new(Mutex::new(None)),
            hosting: None,
            candidates: Arc::new(OnceLock::new()),
        }
    }
    pub(crate) fn registry_names(&self) -> Vec<String> {
        self.binding.registry_names()
    }
    /// Content directory of each live registry, with its internal name.
    pub(crate) fn registry_directories(&self) -> std::collections::BTreeMap<String, String> {
        self.binding
            .registry_names()
            .into_iter()
            .filter_map(|name| Some((self.binding.registry_directory(&name)?, name)))
            .collect()
    }
    pub(crate) fn detached_context(&self) -> Self {
        Self {
            binding: self.binding.clone(),
            invalidated: self.invalidated.clone(),
            hosting: None,
            candidates: self.candidates.clone(),
        }
    }
    /// Opaque pinned composition identity. Equality does not establish current integrity.
    pub fn identity(&self) -> ContextIdentity {
        self.binding.identity()
    }
    /// Installation or authored synthetic input origin.
    pub fn origin(&self) -> ContextOrigin {
        self.binding.origin()
    }
    fn integrity(&self) -> Option<UnavailableReason> {
        let mut invalidated = self.invalidated.lock().expect("context integrity lock");
        if invalidated.is_none() {
            *invalidated = self.binding.integrity();
        }
        invalidated.clone()
    }
    /// Inspect one live operation. Admission may probe live prerequisites; it never launches
    /// Stellaris. Static questions need no admission.
    pub fn capability(&self, request: &CapabilityRequest) -> CapabilityReport {
        qualification::evaluate(
            &self.binding.current_inputs(),
            request,
            self.origin(),
            self.integrity(),
        )
    }
    /// Describe a declared registry without live admission, debugger access, or a game process.
    pub fn get_registry(
        &self,
        name: &str,
    ) -> Result<crate::RegistryDescription, crate::RegistryError> {
        let directory = self.binding.registry_directory(name).ok_or_else(|| {
            crate::RegistryError::Unsupported {
                registry: name.into(),
            }
        })?;
        if let Some(reason) = self.integrity() {
            return Err(crate::RegistryError::Unavailable {
                reasons: vec![reason],
            });
        }
        Ok(crate::RegistryDescription {
            name: name.into(), content_directory: directory, context: self.identity(), origin: self.origin(),
            reader_discovery: crate::DiscoveryStatus::Unknown { reason: "Reader discovery has not been qualified".into() },
            field_discovery: crate::DiscoveryStatus::Unknown { reason: "Field discovery has not been qualified".into() },
            limits: vec!["Target-declared registry metadata; no complete reader, field schema, or registered items established".into()],
        })
    }
    /// Configure a dedicated direct child calling supervisor::serve on private stdin/stdout.
    /// See examples/live.rs for the supervisor role and async session flow.
    pub fn with_supervisor(
        mut self,
        command: std::process::Command,
        options: crate::GameOptions,
    ) -> Result<Self, crate::GameError> {
        self.hosting = Some(crate::game::Hosting::new(command, options)?);
        Ok(self)
    }
    /// Start a paused registry initialization session, never a loaded world.
    /// Dropping this future requests independent cleanup. No async runtime owns the process.
    pub async fn start_game(&self) -> Result<crate::Game, crate::GameError> {
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
        options: &crate::GameOptions,
        authorization: crate::operation::Authorization,
        control: Option<(String, crate::operation::ObservationControl)>,
    ) -> Result<crate::operation::PreparedPlan, crate::GameError> {
        let names = self.binding.registry_names();
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
                installation_hint: self.binding.installation_hint()?,
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
