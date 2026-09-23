mod analysis;
pub(crate) use analysis::{BoundAnalysis, NamedCandidate, VerifiedAnalysis};
mod binary;
mod compose;
mod groups;
mod installation;
mod machine;
mod platform;
mod targets;

#[cfg(test)]
mod tests;

use crate::{OpenError, UnavailableReason};

fn named_candidate<'a>(
    candidates: &'a [NamedCandidate],
    directory: &str,
) -> Result<&'a NamedCandidate, String> {
    use crate::engine::analysis::directories::Directory;
    let mut matches = candidates.iter().filter(
        |candidate| matches!(&candidate.directory, Directory::Named(name) if name == directory),
    );
    let (Some(candidate), None) = (matches.next(), matches.next()) else {
        return Err(format!("{directory}: no unique static registry candidate"));
    };
    Ok(candidate)
}

fn initial_loader(candidate: &NamedCandidate, directory: &str) -> Result<u64, String> {
    candidate
        .record
        .initial_loader
        .as_ref()
        .and_then(|address| address.strip_prefix("0x"))
        .and_then(|address| u64::from_str_radix(address, 16).ok())
        .ok_or_else(|| format!("{directory}: initial loader entry is unavailable"))
}

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
        let image = binary::identify(&bytes, installation.executable_hash())?;
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

    pub(crate) fn default_registries(&self) -> Vec<String> {
        self.operation
            .iter()
            .flat_map(|operation| {
                operation
                    .default_registries
                    .iter()
                    .map(|name| (*name).into())
            })
            .collect()
    }

    pub(crate) fn registry_bindings(
        &self,
        directories: &[String],
    ) -> Result<
        std::collections::BTreeMap<String, crate::protocol::observation::RegistryBinding>,
        String,
    > {
        let layout = self
            .operation
            .as_ref()
            .and_then(|operation| operation.registry_layout)
            .ok_or("this build has no registry observation layout")?;
        let verified = self
            .analysis
            .as_ref()
            .ok_or("this build has no static registry analysis")?
            .verified()
            .map_err(|error| error.to_string())?;
        directories
            .iter()
            .map(|directory| {
                let candidate = named_candidate(verified.named_candidates(), directory)?;
                let address = initial_loader(candidate, directory)?;
                let key_offset =
                    verified.registry_key_offset(candidate, layout.string_tag_offset());
                Ok((
                    directory.clone(),
                    groups::registry_binding(layout, directory, address, key_offset),
                ))
            })
            .collect()
    }

    pub(crate) fn has_fixture_method(&self) -> bool {
        self.operation
            .as_ref()
            .is_some_and(|operation| operation.fixture.is_some())
    }

    pub(crate) fn has_declarations_method(&self) -> bool {
        self.analysis
            .as_ref()
            .is_some_and(|analysis| analysis.has_declarations_method())
    }

    pub(crate) fn target_integrity(&self) -> Option<UnavailableReason> {
        self.installation.target_integrity()
    }

    pub(crate) fn default_content_integrity(&self) -> Option<UnavailableReason> {
        self.installation.default_content_integrity()
    }

    /// Every reason why a game session cannot start now. Empty means that it can. This may
    /// start the host's debugger tools to check them; it never starts the game.
    ///
    /// `integrity` is the caller's view of its pinned inputs; a `Native` keeps a change
    /// that it saw once.
    pub(crate) fn blocking_reasons(
        &self,
        integrity: Option<UnavailableReason>,
        check_defaults: bool,
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
        if check_defaults && let Err(reason) = &self.installation.content {
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
    pub fn registry_bindings(
        &self,
        directories: &[String],
    ) -> Result<
        std::collections::BTreeMap<String, crate::protocol::observation::RegistryBinding>,
        crate::supervisor::SupervisorError,
    > {
        self.binding
            .registry_bindings(directories)
            .map_err(crate::supervisor::SupervisorError)
    }
    fn installation(&self) -> &installation::Installation {
        &self.binding.installation
    }
    pub fn session_content(
        &self,
        directories: &[String],
    ) -> Result<installation::ContentIdentity, crate::supervisor::SupervisorError> {
        self.installation()
            .session_content(directories)
            .map_err(|reason| {
                crate::supervisor::SupervisorError(format!(
                    "Registry content unavailable: {reason:?}"
                ))
            })
    }
    pub fn session_content_unchanged(
        &self,
        directories: &[String],
        expected: &installation::ContentIdentity,
    ) -> bool {
        self.installation()
            .session_content_unchanged(directories, expected)
    }
    fn operation(&self) -> &compose::ResolvedObservation {
        self.binding
            .operation
            .as_ref()
            .expect("an opened installation has an operation")
    }
    pub fn integrity(&self) -> Result<(), crate::supervisor::SupervisorError> {
        match self.binding.target_integrity() {
            None => Ok(()),
            Some(reason) => Err(crate::supervisor::SupervisorError(format!(
                "Inputs changed: {reason:?}"
            ))),
        }
    }
    /// Refuse the session when anything blocks it. The caller asked the same question before
    /// it started this supervisor; the supervisor does not trust that answer.
    pub fn admit(&self) -> Result<(), crate::supervisor::SupervisorError> {
        let reasons = self
            .binding
            .blocking_reasons(self.binding.target_integrity(), false);
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
    HostReservation, OwnedGame, acquire_reservation, conflicting_game, prepare_owner,
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
pub(crate) fn test_observer(
    output: &std::path::Path,
    command: &mut std::process::Command,
    game: u32,
    registry: &str,
) -> Observer {
    platform::observation::test_observer(output, command, game, Some(registry))
}

#[cfg(all(test, target_os = "macos", target_arch = "aarch64"))]
pub(crate) static LIFECYCLE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub(crate) use platform::observation::Observer;

impl ExecutionPlan {
    fn fixture_question_setup(
        bindings: &crate::protocol::observation::FixtureBinding,
        fields: &[crate::Field],
        index: usize,
        question: &crate::FixtureFieldQuestion,
    ) -> crate::protocol::observation::FixtureQuestionSetup {
        let field = fields.iter().find(|field| field.name == question.field);
        let exact = bindings
            .outcome_registries
            .iter()
            .find(|binding| binding.registry == question.registry)
            .and_then(|binding| {
                binding
                    .fields
                    .iter()
                    .find(|field| field.name == question.field)
            });
        let reader_kind = field
            .map(|field| format!("{:?}", field.reader.kind))
            .unwrap_or_else(|| "Unknown".into());
        let unavailable = match (field, exact) {
            (None, _) => Some("The field is not established by registry_fields".into()),
            (Some(field), _) if field.reader.kind != crate::ReaderKind::String => Some(format!(
                "The {:?} reader has no storage decoder in this method",
                field.reader.kind
            )),
            (Some(_), None) => Some("No exact-build storage binding for this field".into()),
            _ => None,
        };
        crate::protocol::observation::FixtureQuestionSetup {
            index: index as u64,
            definition: question.definition.clone(),
            field: question.field.clone(),
            diagnostics: question.diagnostics,
            runtime: question.runtime,
            reader_id: field.and_then(|field| field.reader.id.as_ref().map(|id| id.0.clone())),
            reader_kind,
            token: exact.map(|field| field.token),
            storage_offset: exact.map(|field| field.storage_offset),
            unavailable,
        }
    }

    fn fixture_setup(
        &self,
        fixture: &crate::FixtureRequest,
    ) -> Result<crate::protocol::observation::FixtureSetup, crate::supervisor::SupervisorError>
    {
        let mut bindings = self.operation().fixture.clone().ok_or_else(|| {
            crate::supervisor::SupervisorError("No fixture binding for this build".into())
        })?;
        if !fixture.field_questions.is_empty()
            && !bindings
                .outcome_registries
                .iter()
                .any(|binding| binding.registry == fixture.registry())
        {
            let analysis = self.binding.analysis.as_ref().ok_or_else(|| {
                crate::supervisor::SupervisorError(
                    "No static reader authority for fixture questions".into(),
                )
            })?;
            let template = &bindings.outcome_registries[0];
            let Some(loader) = analysis
                .fixture_loader(fixture.registry())
                .map_err(|error| crate::supervisor::SupervisorError(error.to_string()))?
            else {
                return Err(crate::supervisor::SupervisorError(
                    "Fixture registry has no verified loader and owner boundary".into(),
                ));
            };
            let mut selected = template.clone();
            selected.registry = fixture.registry().into();
            selected.load_entry = loader.load_entry;
            selected.reader_entry = loader.reader_entry;
            selected.reader_return = loader.reader_return;
            selected.constructor_entry = loader.constructor_entry;
            selected.member_entry = loader.member_entry;
            bindings.outcome_registries.push(selected);
        }
        let fields = if fixture.field_questions.is_empty() {
            Vec::new()
        } else {
            let analysis = self.binding.analysis.as_ref().ok_or_else(|| {
                crate::supervisor::SupervisorError(
                    "No static reader authority for fixture questions".into(),
                )
            })?;
            analysis
                .registry_fields(fixture.registry())
                .map_err(|error| crate::supervisor::SupervisorError(error.to_string()))?
                .unwrap_or_default()
        };
        if !fixture.field_questions.is_empty() {
            let analysis = self.binding.analysis.as_ref().ok_or_else(|| {
                crate::supervisor::SupervisorError(
                    "No static reader authority for fixture questions".into(),
                )
            })?;
            let derived = analysis
                .fixture_string_fields(fixture.registry())
                .map_err(|error| crate::supervisor::SupervisorError(error.to_string()))?;
            if let Some(binding) = bindings
                .outcome_registries
                .iter_mut()
                .find(|binding| binding.registry == fixture.registry())
            {
                binding.fields = derived;
            }
        }
        let questions = fixture
            .field_questions
            .iter()
            .enumerate()
            .map(|(index, question)| {
                Self::fixture_question_setup(&bindings, &fields, index, question)
            })
            .collect();
        Ok(crate::protocol::observation::FixtureSetup {
            file: fixture.file().into(),
            registration_entries: fixture
                .requests(crate::FixtureObservationKind::RegistrationEntries),
            field_reads: fixture.requests(crate::FixtureObservationKind::CategoryFieldReads),
            questions,
            bindings,
        })
    }

    /// Prepare the debugger worker for one session in its work directory.
    pub(crate) fn observer(
        &self,
        work_directory: &std::path::Path,
        attempt: &str,
        request: &crate::protocol::session::SessionRequest,
        registries: &std::collections::BTreeMap<
            String,
            crate::protocol::observation::RegistryBinding,
        >,
    ) -> Result<Observer, crate::supervisor::SupervisorError> {
        self.integrity()?;
        let operation = self.operation();
        if let Some(fault) = &request.fault
            && !registries.contains_key(&fault.registry)
        {
            return Err(crate::supervisor::SupervisorError(
                "The fault names a registry that the session does not observe".into(),
            ));
        }
        (operation.strategy.prepare)(platform::ObservationSetup {
            work_directory,
            attempt,
            executable: self.installation().executable(),
            registries,
            fault: request.fault.as_ref(),
            fixture: request
                .fixture
                .as_ref()
                .map(|fixture| self.fixture_setup(fixture))
                .transpose()?,
            fixture_fault: request.fixture_fault,
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
        fixture: Option<&crate::FixtureRequest>,
        registries: &std::collections::BTreeMap<
            String,
            crate::protocol::observation::RegistryBinding,
        >,
        content: &installation::ContentIdentity,
    ) -> Result<(), crate::supervisor::SupervisorError> {
        use crate::{supervisor::SupervisorError, work_directory as files};
        self.integrity()?;
        let profile = work_directory.join("profile");
        platform::lifecycle::private_directory(&profile.join("mod"))?;
        let mount = profile.join("mod/native_registry");
        platform::lifecycle::private_directory(&mount)?;
        for registry in registries.values() {
            std::fs::create_dir_all(mount.join(&registry.directory))?;
        }
        if let Some(fixture) = fixture {
            std::fs::create_dir_all(mount.join(fixture.registry()))?;
        }
        for (relative, expected) in content {
            if relative == "launcher-settings.json"
                || fixture.is_some_and(|fixture| {
                    relative.starts_with(&format!("{}/", fixture.registry()))
                })
            {
                continue;
            }
            let source = self.installation().root().join(relative);
            let bytes = files::read_bounded(&source, 8 * 1024 * 1024)?;
            if files::sha256(&bytes) != *expected {
                return Err(SupervisorError(
                    "Registry content changed while preparing the private profile".into(),
                ));
            }
            let target = mount.join(relative);
            std::fs::create_dir_all(target.parent().unwrap())?;
            files::write_new(&target, &bytes)?;
        }
        if let Some(fixture) = fixture {
            fixture
                .validate()
                .map_err(|error| SupervisorError(error.to_string()))?;
            for (relative, text) in &fixture.files {
                files::write_new(&mount.join(relative), text.as_bytes())?;
            }
        }
        let mount = mount
            .to_str()
            .filter(|path| !path.contains(['"', '\n', '\r']))
            .ok_or_else(|| {
                SupervisorError("Profile path cannot be represented in the mod file".into())
            })?;
        let mut replaced_paths: std::collections::BTreeSet<_> = registries
            .values()
            .map(|registry| registry.directory.as_str())
            .collect();
        if let Some(fixture) = fixture {
            replaced_paths.insert(fixture.registry());
        }
        let replaced: String = replaced_paths
            .into_iter()
            .map(|directory| format!("replace_path=\"{directory}\"\n"))
            .collect();
        files::write_new(
            &profile.join("mod/native_registry.mod"),
            format!("name=\"Native pinned registries\"\npath=\"{mount}\"\n{replaced}").as_bytes(),
        )?;
        std::fs::write(
            profile.join("dlc_load.json"),
            r#"{"enabled_mods":["mod/native_registry.mod"],"disabled_dlcs":[]}"#,
        )?;
        self.integrity()?;
        Ok(())
    }
}
