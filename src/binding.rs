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

pub(crate) use installation::ContentIdentity;

use crate::{OpenError, UnavailableReason};

/// One pinned installation with the implementation that its exact build selects. This is the
/// only binding value that the session and the supervisor see; target records and platform
/// leaves stay below here.
pub(crate) struct Binding {
    pub(crate) analysis: Option<std::sync::Arc<BoundAnalysis>>,
    /// `None` only in tests that bind an authored installation.
    operation: Option<compose::ResolvedObservation>,
    installation: installation::Installation,
}

impl Binding {
    pub(crate) fn open(installation: &std::path::Path) -> Result<Self, OpenError> {
        let (installation, bytes) = installation::Installation::open(installation)?;
        let image = binary::identify(&bytes)?;
        let operation = compose::compose(&image)?;
        let analysis = Some(std::sync::Arc::new(compose::analysis(
            &image,
            installation.clone(),
        )?));
        Ok(Self {
            analysis,
            operation: Some(operation),
            installation,
        })
    }

    /// Identity of the exact game build: the SHA-256 of the executable file.
    pub(crate) fn build(&self) -> &str {
        self.installation.executable_hash()
    }

    /// The location that a supervisor opens to bind the same installation.
    pub(crate) fn installation_location(&self) -> std::path::PathBuf {
        self.installation.locator().into()
    }

    /// Internal names of the registries that a game session observes.
    pub(crate) fn registry_names(&self) -> Vec<String> {
        self.operation
            .iter()
            .flat_map(|operation| operation.registries.keys().cloned())
            .collect()
    }

    pub(crate) fn registry_directory(&self, name: &str) -> Option<String> {
        self.operation
            .as_ref()?
            .registries
            .get(name)
            .map(|registry| registry.directory.clone())
    }

    /// Whether the executable or the pinned content changed since `open`.
    pub(crate) fn integrity(&self) -> Option<UnavailableReason> {
        self.installation.integrity()
    }

    /// Every reason why a game session cannot start now. Empty means that it can. This may
    /// start the host's debugger tools to check them; it never starts the game.
    ///
    /// `integrity` is the caller's view of [`Binding::integrity`]; a `Native` keeps a change
    /// that it saw once.
    pub(crate) fn blocking_reasons(
        &self,
        integrity: Option<UnavailableReason>,
    ) -> Vec<UnavailableReason> {
        let mut reasons = Vec::new();
        let mut add = |reason: UnavailableReason| {
            if !reasons.contains(&reason) {
                reasons.push(reason);
            }
        };
        if let Some(operation) = &self.operation {
            operation
                .host_prerequisites()
                .into_iter()
                .for_each(&mut add);
        }
        integrity.into_iter().for_each(&mut add);
        if let Err(reason) = &self.installation.content {
            add(reason.clone());
        }
        if let Some(operation) = &self.operation
            && (operation.strategy.probe)().is_err()
        {
            add(UnavailableReason::PrerequisiteMissing);
        }
        reasons
    }
}

/// The supervisor's view of a binding: what it needs to start and observe one game.
pub(crate) struct ExecutionPlan {
    binding: Binding,
}

impl ExecutionPlan {
    pub fn open(
        installation: &std::path::Path,
    ) -> Result<Self, crate::supervisor::SupervisorError> {
        platform::lifecycle::available()?;
        let binding = Binding::open(installation)
            .map_err(|error| crate::supervisor::SupervisorError(error.to_string()))?;
        let plan = Self { binding };
        plan.integrity()?;
        Ok(plan)
    }
    pub fn build(&self) -> &str {
        self.binding.build()
    }
    pub fn registry_names(&self) -> Vec<String> {
        self.binding.registry_names()
    }
    fn installation(&self) -> &installation::Installation {
        &self.binding.installation
    }
    fn operation(&self) -> &compose::ResolvedObservation {
        self.binding
            .operation
            .as_ref()
            .expect("an opened installation has an operation")
    }
    pub fn integrity(&self) -> Result<(), crate::supervisor::SupervisorError> {
        match self.binding.integrity() {
            None => Ok(()),
            Some(reason) => Err(crate::supervisor::SupervisorError(format!(
                "Inputs changed: {reason:?}"
            ))),
        }
    }
    /// Refuse the session when anything blocks it. The caller asked the same question before
    /// it started this supervisor; the supervisor does not trust that answer.
    pub fn admit(&self) -> Result<(), crate::supervisor::SupervisorError> {
        let reasons = self.binding.blocking_reasons(self.binding.integrity());
        if !reasons.is_empty() {
            return Err(crate::supervisor::SupervisorError(format!(
                "A game session cannot start: {reasons:?}"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Machine {
    pub architecture: String,
    pub spawn_preference: i32,
    pub registers: std::collections::BTreeMap<String, String>,
}

impl std::fmt::Debug for Binding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Binding")
            .field("installation", &self.installation)
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
    /// Prepare the debugger worker for one session in its work directory.
    pub(crate) fn observer(
        &self,
        work_directory: &std::path::Path,
        attempt: &str,
        request: &crate::protocol::session::SessionRequest,
    ) -> Result<Observer, crate::supervisor::SupervisorError> {
        self.integrity()?;
        let operation = self.operation();
        if let Some(fault) = &request.fault
            && !operation.registries.contains_key(&fault.registry)
        {
            return Err(crate::supervisor::SupervisorError(
                "The fault names a registry that the session does not observe".into(),
            ));
        }
        (operation.strategy.prepare)(platform::ObservationSetup {
            work_directory,
            attempt,
            executable: self.installation().executable(),
            registries: &operation.registries,
            fault: request.fault.as_ref(),
            startup_seconds: request.startup_seconds,
            machine: &operation.machine,
            package: &operation.strategy.package,
        })
    }
    pub(crate) fn spawn_observed(
        &self,
        work_directory: &std::path::Path,
        observer: &Observer,
    ) -> Result<OwnedGame, crate::supervisor::SupervisorError> {
        self.integrity()?;
        platform::lifecycle::spawn_guarded(
            self.installation().executable(),
            self.installation().root(),
            work_directory,
            self.operation().machine.spawn_preference,
            Some(&observer.guard()),
        )
    }
    /// Give the private game profile a mod that replaces the observed registries' directories
    /// with copies of the pinned installed files. The session then observes known content,
    /// whatever mods the user has.
    pub(crate) fn prepare_registry_profile(
        &self,
        work_directory: &std::path::Path,
    ) -> Result<(), crate::supervisor::SupervisorError> {
        use crate::{supervisor::SupervisorError, work_directory as files};
        self.integrity()?;
        let profile = work_directory.join("profile");
        platform::lifecycle::private_directory(&profile.join("mod"))?;
        let mount = profile.join("mod/native_registry");
        platform::lifecycle::private_directory(&mount)?;
        for (relative, expected) in &self.operation().content {
            if !relative.starts_with("common/") {
                continue;
            }
            let source = self.installation().root().join(relative);
            let bytes = files::read_bounded(&source, 1024 * 1024)?;
            if files::sha256(&bytes) != *expected {
                return Err(SupervisorError(
                    "Registry content changed while preparing the private profile".into(),
                ));
            }
            let target = mount.join(relative);
            std::fs::create_dir_all(target.parent().unwrap())?;
            files::write_new(&target, &bytes)?;
        }
        let mount = mount
            .to_str()
            .filter(|path| !path.contains(['"', '\n', '\r']))
            .ok_or_else(|| {
                SupervisorError("Profile path cannot be represented in the mod file".into())
            })?;
        let replaced: String = self
            .operation()
            .registries
            .values()
            .map(|registry| format!("replace_path=\"{}\"\n", registry.directory))
            .collect();
        files::write_new(
            &profile.join("mod/native_registry.mod"),
            format!("name=\"Native pinned registries\"\npath=\"{mount}\"\n{replaced}").as_bytes(),
        )?;
        std::fs::write(
            profile.join("dlc_load.json"),
            r#"{"enabled_mods":["mod/native_registry.mod"],"disabled_dlcs":[]}"#,
        )?;
        Ok(())
    }
}
