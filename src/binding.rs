mod analysis;
pub(crate) use analysis::{BoundAnalysis, NamedCandidate};
mod binary;
mod compose;
mod groups;
mod installation;
mod machine;
mod platform;
mod targets;

#[cfg(test)]
mod tests;

use crate::qualification::{AdmissionInputs, Authority};
use crate::{ContextIdentity, ContextOrigin, OpenError, OpenRequest, UnavailableReason};

/// The only session-facing binding value. Raw target and platform descriptors stay below here.
pub(crate) struct Binding {
    inputs: AdmissionInputs,
    pub(crate) analysis: Option<std::sync::Arc<BoundAnalysis>>,
    operation: Option<compose::ResolvedObservation>,
    source: Source,
    authority: Authority,
}

#[derive(Debug)]
enum Source {
    Installation(installation::Installation),
    #[cfg(feature = "test-support")]
    Synthetic(Option<UnavailableReason>),
}

impl Binding {
    pub(crate) fn open(request: OpenRequest) -> Result<Self, OpenError> {
        let (installation, bytes) = installation::Installation::open(&request.installation_hint)?;
        let image = binary::identify(&bytes)?;
        let (inputs, operation) = compose::compose(&image, installation.content.clone())?;
        let analysis = Some(std::sync::Arc::new(compose::analysis(
            &image,
            installation.clone(),
        )?));
        Ok(Self {
            inputs,
            analysis,
            operation: Some(operation),
            source: Source::Installation(installation),
            authority: Authority::bundled(),
        })
    }

    pub(crate) fn registry_directory(&self, name: &str) -> Option<String> {
        self.operation
            .as_ref()?
            .registries
            .get(name)
            .map(|registry| registry.directory.clone())
    }

    pub(crate) fn registry_names(&self) -> Vec<String> {
        self.inputs.bounds.registries.clone()
    }

    pub(crate) fn identity(&self) -> ContextIdentity {
        ContextIdentity(self.inputs.composition.clone())
    }

    pub(crate) fn origin(&self) -> ContextOrigin {
        match self.source {
            Source::Installation(_) => ContextOrigin::Installation,
            #[cfg(feature = "test-support")]
            Source::Synthetic(_) => ContextOrigin::Synthetic,
        }
    }

    pub(crate) fn integrity(&self) -> Option<UnavailableReason> {
        match &self.source {
            Source::Installation(installation) => installation.integrity(),
            #[cfg(feature = "test-support")]
            Source::Synthetic(failure) => failure.clone(),
        }
    }

    pub(crate) fn current_inputs(&self) -> AdmissionInputs {
        let mut inputs = self.inputs.clone();
        if self.origin() == ContextOrigin::Installation && !cfg!(feature = "production") {
            inputs
                .prerequisites
                .push(UnavailableReason::ProductionFeatureRequired);
        }
        if let Some(operation) = &self.operation {
            inputs.toolchain =
                (operation.strategy.probe)().map_err(|_| UnavailableReason::PrerequisiteMissing);
        }
        inputs
    }
    pub(crate) fn authority(&self) -> &Authority {
        &self.authority
    }
}

#[cfg(feature = "test-support")]
mod synthetic;

#[cfg(feature = "test-support")]
pub(crate) use synthetic::synthetic;

pub(crate) struct ExecutionPlan {
    binding: Binding,
    admitted_tool: Option<String>,
    session_unavailable: std::collections::BTreeMap<String, String>,
}

impl ExecutionPlan {
    pub fn open(hint: &std::path::Path) -> Result<Self, crate::supervisor::SupervisorError> {
        platform::lifecycle::available()?;
        let binding = Binding::open(OpenRequest {
            installation_hint: hint.into(),
        })
        .map_err(|error| crate::supervisor::SupervisorError(error.to_string()))?;
        let plan = Self {
            binding,
            admitted_tool: None,
            session_unavailable: Default::default(),
        };
        plan.integrity()?;
        Ok(plan)
    }
    pub fn composition(&self) -> &str {
        &self.binding.inputs.composition
    }
    fn installation(&self) -> &installation::Installation {
        match &self.binding.source {
            Source::Installation(installation) => installation,
            #[cfg(feature = "test-support")]
            Source::Synthetic(_) => unreachable!("execution cannot use synthetic inputs"),
        }
    }
    fn operation(&self) -> &compose::ResolvedObservation {
        self.binding
            .operation
            .as_ref()
            .expect("installed operation")
    }
    pub fn integrity(&self) -> Result<(), crate::supervisor::SupervisorError> {
        match self.binding.integrity() {
            None => Ok(()),
            Some(reason) => Err(crate::supervisor::SupervisorError(format!(
                "Inputs changed: {reason:?}"
            ))),
        }
    }
    pub fn admit(&mut self, registry: &str) -> Result<(), crate::supervisor::SupervisorError> {
        let inputs = self.binding.current_inputs();
        let report = crate::qualification::evaluate(
            &inputs,
            self.binding.authority(),
            &crate::CapabilityRequest::Registry {
                registry: registry.into(),
            },
            self.binding.origin(),
            self.binding.integrity(),
        );
        if report.availability != crate::Availability::Available {
            return Err(crate::supervisor::SupervisorError(format!(
                "Live admission refused: {:?}",
                report.reasons
            )));
        }
        self.validate_observation_content()?;
        self.admitted_tool = inputs.toolchain.ok();
        Ok(())
    }
    pub fn admit_session(&mut self) -> Result<(), crate::supervisor::SupervisorError> {
        let mut admitted = false;
        for name in self.binding.registry_names() {
            match self.admit(&name) {
                Ok(()) => admitted = true,
                Err(error) => {
                    self.session_unavailable.insert(name, error.to_string());
                }
            }
        }
        if !admitted {
            return Err(crate::supervisor::SupervisorError(format!(
                "Session unavailable: {:?}",
                self.session_unavailable
            )));
        }
        Ok(())
    }

    #[cfg(feature = "maintainer-tools")]
    pub fn probe(&self) -> Result<(), crate::supervisor::SupervisorError> {
        (self.operation().strategy.probe)().map(|_| ())
    }
    pub fn spawn(
        &self,
        output: &std::path::Path,
    ) -> Result<OwnedGame, crate::supervisor::SupervisorError> {
        self.integrity()?;
        platform::lifecycle::spawn(
            self.installation().executable(),
            self.installation().root(),
            output,
            self.operation().machine.spawn_preference,
        )
    }
}

impl Binding {
    pub(crate) fn installation_hint(
        &self,
    ) -> Result<std::path::PathBuf, crate::supervisor::SupervisorError> {
        match &self.source {
            Source::Installation(installation) => Ok(installation.locator().into()),
            #[cfg(feature = "test-support")]
            Source::Synthetic(_) => Err(crate::supervisor::SupervisorError(
                "Synthetic contexts cannot launch".into(),
            )),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Machine {
    pub revision: String,
    pub architecture: String,
    pub spawn_preference: i32,
    pub registers: std::collections::BTreeMap<String, String>,
}

impl std::fmt::Debug for Binding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Binding")
            .field("inputs", &self.inputs)
            .finish_non_exhaustive()
    }
}

pub(crate) use platform::lifecycle::{
    HostReservation, OwnedGame, acquire_reservation, conflicting_game, open_record, prepare_owner,
    private_directory, process_identity,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProcessIdentity {
    pub pid: u32,
    pub started_seconds: u64,
    pub started_microseconds: u64,
}

#[cfg(all(test, target_os = "macos", target_arch = "aarch64"))]
pub(crate) fn test_reservation(
    root: &std::path::Path,
) -> Result<HostReservation, crate::supervisor::SupervisorError> {
    platform::lifecycle::acquire_at(root)
}

#[cfg(all(test, target_os = "macos", target_arch = "aarch64"))]
pub(crate) fn test_child(
    output: &std::path::Path,
) -> Result<OwnedGame, crate::supervisor::SupervisorError> {
    platform::lifecycle::spawn(
        std::path::Path::new("/bin/sleep"),
        std::path::Path::new("/"),
        output,
        machine::resolve(object::Architecture::Aarch64)
            .unwrap()
            .spawn_preference,
    )
}

#[cfg(all(test, target_os = "macos", target_arch = "aarch64"))]
pub(crate) static LIFECYCLE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub(crate) use platform::observation::Observer;

impl ExecutionPlan {
    pub(crate) fn observer(
        &self,
        output: &std::path::Path,
        attempt: &str,
        spec: &crate::operation::ObservationSpec,
        origin: &str,
    ) -> Result<(Observer, crate::capture::Capture), crate::supervisor::SupervisorError> {
        self.integrity()?;
        self.validate_observation_content()?;
        let operation = self.operation();
        let identity = serde_json::json!({
            "origin": origin, "attempt": attempt, "composition": self.composition(),
            "target": operation.image.executable, "slice": operation.image.slice,
            "machine": operation.machine, "architecture": operation.machine.architecture,
            "build": env!("PDX_NATIVE_BUILD"), "implementation": env!("PDX_NATIVE_OPERATION"),
            "compiler": env!("PDX_NATIVE_COMPILER"), "profile": env!("PDX_NATIVE_PROFILE"),
            "registries": operation.registries.keys().collect::<Vec<_>>(),
            "method": if spec.session.is_some() { operation.session_method } else if spec.registry.is_some() { operation.method } else { operation.early_method }, "strategy": operation.strategy.revision, "control": spec.control,
            "bindings": operation.bindings,
        });
        (operation.strategy.prepare)(platform::ObservationSetup {
            output,
            attempt,
            spec,
            executable: self.installation().executable(),
            identity,
            content: self
                .binding
                .inputs
                .content
                .as_ref()
                .map_err(|_| crate::supervisor::SupervisorError("Content unavailable".into()))?,
            expected_tool: self.admitted_tool.as_deref(),
            registry: spec
                .registry
                .as_ref()
                .map(|name| {
                    operation.registries.get(name).ok_or_else(|| {
                        crate::supervisor::SupervisorError("Unsupported registry".into())
                    })
                })
                .transpose()?,
            session: spec.session.as_ref().map(|session| {
                crate::protocol::observation::SessionBindings {
                    registries: operation
                        .registries
                        .iter()
                        .filter(|(name, _)| !self.session_unavailable.contains_key(*name))
                        .map(|(name, binding)| (name.clone(), binding.clone()))
                        .collect(),
                    unavailable: self.session_unavailable.clone(),
                    control_registry: session.control_registry.clone(),
                }
            }),
            bindings: &operation.bindings,
            machine: &operation.machine,
            package: &operation.strategy.package,
        })
    }
    pub(crate) fn spawn_observed(
        &self,
        output: &std::path::Path,
        observer: &Observer,
    ) -> Result<OwnedGame, crate::supervisor::SupervisorError> {
        self.integrity()?;
        platform::lifecycle::spawn_guarded(
            self.installation().executable(),
            self.installation().root(),
            output,
            self.operation().machine.spawn_preference,
            Some(&observer.guard()),
        )
    }
    pub(crate) fn prepare_registry_profile(
        &self,
        output: &std::path::Path,
    ) -> Result<(), crate::supervisor::SupervisorError> {
        use crate::{capture, supervisor::SupervisorError};
        self.integrity()?;
        self.validate_observation_content()?;
        let profile = output.join("profile");
        platform::lifecycle::private_directory(&profile.join("mod"))?;
        let mount = profile.join("mod/native_registry");
        platform::lifecycle::private_directory(&mount)?;
        for (relative, expected) in &self.operation().content {
            if !relative.starts_with("common/") {
                continue;
            }
            let source = self.installation().root().join(relative);
            let bytes = capture::read_bounded(&source, 1024 * 1024)?;
            if capture::hash(&bytes) != *expected {
                return Err(SupervisorError(
                    "Registry content changed while preparing the private profile".into(),
                ));
            }
            let target = mount.join(relative);
            std::fs::create_dir_all(target.parent().unwrap())?;
            capture::write_new(&target, &bytes)?;
        }
        let mount = mount
            .to_str()
            .filter(|path| !path.contains(['"', '\n', '\r']))
            .ok_or_else(|| {
                SupervisorError("Profile path cannot be represented in mod descriptor".into())
            })?;
        capture::write_new(&profile.join("mod/native_registry.mod"), format!("name=\"Native pinned registries\"\npath=\"{mount}\"\nreplace_path=\"common/traditions\"\nreplace_path=\"common/tradition_categories\"\n").as_bytes())?;
        std::fs::write(
            profile.join("dlc_load.json"),
            r#"{"enabled_mods":["mod/native_registry.mod"],"disabled_dlcs":[]}"#,
        )?;
        Ok(())
    }

    pub(crate) fn validate_observation_content(
        &self,
    ) -> Result<(), crate::supervisor::SupervisorError> {
        if self.binding.inputs.content.as_ref().ok() != Some(&self.operation().content) {
            return Err(crate::supervisor::SupervisorError(
                "Observation content does not match the selected recipe".into(),
            ));
        }
        Ok(())
    }
}
