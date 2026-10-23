mod analysis;
pub(crate) use analysis::{
    BlockFacts, BoundAnalysis, FixtureRegistryFields, NamedCandidate, PreparedDurations,
    VerifiedAnalysis, loader_names, scoped_operand_decoder, unique_named_candidate,
};
mod binary;
mod compose;
mod groups;
pub mod inspect;
mod installation;
mod machine;
mod platform;
mod requirements;
mod targets;

#[cfg(test)]
mod tests;

use crate::{OpenError, UnavailableReason};

/// Returns after content validation; the loaded modifier table is complete there.
const MODIFIER_LOG_DEFINITIONS: &str = "CModifier::LogDefinitions()";

fn named_candidate<'a>(
    candidates: &'a [NamedCandidate],
    directory: &str,
) -> Result<&'a NamedCandidate, String> {
    unique_named_candidate(candidates, directory)
        .ok_or_else(|| format!("{directory}: no unique static registry candidate"))
}

/// The database instance of a registry whose keys a session reads, and its key offset. A
/// registry without an instance symbol gets no key read, with the reason, and the rest of the
/// table binding stands.
fn registry_instance(
    instance: Result<u64, String>,
    key_offset: Result<u64, String>,
) -> (u64, Result<u64, String>) {
    match instance {
        Ok(instance) => (instance, key_offset),
        Err(reason) => (
            0,
            Err(format!(
                "the registry's database instance is not established: {reason}"
            )),
        ),
    }
}

fn initial_loader(candidate: &NamedCandidate, directory: &str) -> Result<u64, String> {
    candidate
        .record
        .initial_loader
        .ok_or_else(|| format!("{directory}: initial loader entry is unavailable"))
}

/// The content directories that the private profile's mod replaces: each observed registry's
/// directory and the fixture's registry.
fn replaced_directories<'a>(
    registries: &'a std::collections::BTreeMap<
        String,
        crate::protocol::observation::RegistryBinding,
    >,
    fixture: Option<&'a crate::FixtureRequest>,
) -> std::collections::BTreeSet<&'a str> {
    registries
        .values()
        .map(|registry| registry.directory.as_str())
        .chain(fixture.map(crate::FixtureRequest::registry))
        .collect()
}

/// Whether the private profile copies this pinned file. The launcher settings are pinned but
/// are not registry content, and the fixture's files replace its whole registry directory.
fn copies_pinned_file(relative: &str, fixture: Option<&crate::FixtureRequest>) -> bool {
    let fixture_content =
        fixture.is_some_and(|fixture| relative.starts_with(&format!("{}/", fixture.registry())));

    relative != "launcher-settings.json" && !fixture_content
}

/// The mod descriptor that mounts `mount` in place of each replaced directory. Descriptor values
/// are quoted without escapes, so a mount path with a quote or a line break is refused.
fn registry_mod_descriptor(
    mount: &std::path::Path,
    replaced: &std::collections::BTreeSet<&str>,
) -> Result<String, crate::supervisor::SupervisorError> {
    let mount = mount
        .to_str()
        .filter(|path| !path.contains(['"', '\n', '\r']))
        .ok_or_else(|| {
            crate::supervisor::SupervisorError(
                "Profile path cannot be represented in the mod file".into(),
            )
        })?;
    let replace_paths: String = replaced
        .iter()
        .map(|directory| format!("replace_path=\"{directory}\"\n"))
        .collect();

    Ok(format!(
        "name=\"Native pinned registries\"\npath=\"{mount}\"\n{replace_paths}"
    ))
}

/// One pinned installation with the implementation that its exact build selects. This is the
/// only binding value that the session and the supervisor see; target records and platform
/// leaves stay below here.
pub(crate) struct Binding {
    pub(crate) analysis: std::sync::Arc<BoundAnalysis>,
    /// `None` only in tests that bind an authored installation.
    operation: Option<compose::ResolvedObservation>,
    installation: installation::Installation,
    /// The first executable change that a static read or an integrity check saw, shared with
    /// `analysis`. It stays, even when the original bytes come back.
    invalidated: std::sync::Arc<std::sync::Mutex<Option<UnavailableReason>>>,
}

impl Binding {
    pub(crate) fn has_script_check_method(&self) -> bool {
        self.operation
            .as_ref()
            .is_some_and(|operation| operation.script_checks.is_some())
    }

    fn script_check_binding(&self) -> Option<crate::protocol::script_check::ScriptCheckBinding> {
        self.operation
            .as_ref()
            .and_then(|operation| operation.script_checks.clone())
    }

    /// The scope bit of each scope identity of this build, which the supervisor puts in a check
    /// request. The worker does not see them. A scope analysis that fails is a reason, so it
    /// rejects the checks and nothing else.
    fn script_scopes(&self) -> Result<std::collections::BTreeMap<String, u64>, String> {
        let input = self
            .analysis
            .scope_input()
            .map_err(|error| error.to_string())?;
        let scopes =
            crate::engine::analysis::scopes::scopes(&input).map_err(|error| error.to_string())?;

        Ok(scopes
            .scopes
            .iter()
            .map(|(scope, _)| {
                (
                    crate::session::questions::scope_id(scope).0,
                    1u64 << scope.bit,
                )
            })
            .collect())
    }
    pub(crate) fn open(installation: &std::path::Path) -> Result<Self, OpenError> {
        let (installation, bytes) = installation::Installation::open(installation)?;
        let image = binary::identify(&bytes, installation.executable_hash())?;
        let invalidated = std::sync::Arc::new(std::sync::Mutex::new(None));
        let composition = compose::compose(&image, installation, invalidated.clone())?;
        Ok(Self {
            analysis: std::sync::Arc::new(composition.analysis),
            operation: Some(composition.operation),
            installation: composition.installation,
            invalidated,
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

    /// Whether a definition of `registry` must contain the root field `field`, by the build's
    /// census. `Unknown` when the build has none.
    pub(crate) fn field_requirement(&self, registry: &str, field: &str) -> crate::FieldRequirement {
        self.operation
            .as_ref()
            .and_then(|operation| operation.field_requirements)
            .map_or(crate::FieldRequirement::Unknown, |census| {
                census.requirement(registry, field)
            })
    }

    /// The build's reader export, parsed on first use, or why it could not be read. `None` when
    /// the build has no build directory.
    pub(crate) fn reader_export(
        &self,
    ) -> Option<&Result<crate::session::reader_export::ReaderExport, String>> {
        let operation = self.operation.as_ref()?;
        let files = operation.reader_export?;

        Some(
            operation
                .parsed_export
                .get_or_init(|| crate::session::reader_export::ReaderExport::parse(files)),
        )
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
            .verified()
            .map_err(|error| error.to_string())?;
        directories
            .iter()
            .map(|directory| {
                let candidate = named_candidate(verified.named_candidates(), directory)?;
                let address = initial_loader(candidate, directory)?;
                let key_offset = verified.registry_key_offset(
                    &candidate.record.owner_candidate,
                    layout.string_tag_offset(),
                );
                Ok((
                    directory.clone(),
                    groups::registry_binding(layout, directory, address, key_offset),
                ))
            })
            .collect()
    }

    /// Where this session reads the loaded modifier table, and the database instance of each
    /// registry in `registries` whose item keys it reads there. A registry whose own loader builds
    /// a database with no stated item layout gets no key read, with the reason.
    pub(crate) fn modifier_table_binding(
        &self,
        registries: &[String],
    ) -> Result<crate::protocol::observation::ModifierTableBinding, String> {
        let operation = self.operation.as_ref();
        let Some(layout) = operation.and_then(|operation| operation.registry_layout) else {
            return Err("this build has no loaded modifier table layout".into());
        };
        let loader_databases = operation.map_or(&[][..], |operation| operation.loader_databases);
        let verified = self
            .analysis
            .verified()
            .map_err(|error| error.to_string())?;
        let table = verified.modifier_table(layout.string_tag_offset())?;
        let mut instances = std::collections::BTreeMap::new();
        for directory in registries {
            // A template candidate's database is a `TGameDatabase`; a custom loader's is its own.
            let database = match unique_named_candidate(verified.named_candidates(), directory) {
                Some(candidate) => {
                    let record = &candidate.record;
                    let instance = format!("TGameDatabase<{}>::_pInstance", record.database);
                    Some((record.owner_candidate.as_str(), instance, layout.items()))
                }
                None => {
                    let Some(Ok(registry)) = verified.named_registry(directory) else {
                        return Err(format!("{directory}: no unique static registry candidate"));
                    };
                    loader_databases
                        .iter()
                        .find(|database| database.class == registry.database)
                        .map(|database| {
                            let instance = format!("{}::_pInstance", database.class);
                            (registry.owner, instance, database.items)
                        })
                }
            };
            let Some((owner, instance, items)) = database else {
                let unread = "the registry's own loader builds a database whose item layout is not stated, so its item keys are not read";
                instances.insert(directory.clone(), (0, layout.items(), Err(unread.into())));
                continue;
            };

            let key_offset = verified.registry_key_offset(owner, layout.string_tag_offset());
            let (instance, key_offset) = registry_instance(verified.symbol(&instance), key_offset);
            instances.insert(directory.clone(), (instance, items, key_offset));
        }
        Ok(groups::modifier_table_binding(
            table.layout,
            layout,
            groups::ModifierTableSymbols {
                documentation_entry: table.documentation_entry,
                definitions: table.definitions,
                registries: instances,
            },
        ))
    }

    pub(crate) fn has_modifier_table_method(&self) -> bool {
        self.operation
            .as_ref()
            .is_some_and(|operation| operation.registry_layout.is_some())
    }

    pub(crate) fn has_fixture_method(&self) -> bool {
        self.operation
            .as_ref()
            .is_some_and(|operation| operation.fixture.is_some())
    }

    pub(crate) fn target_integrity(&self) -> Option<UnavailableReason> {
        let mut invalidated = self.invalidated.lock().expect("target integrity lock");
        if invalidated.is_none() {
            *invalidated = self.installation.target_integrity();
        }
        invalidated.clone()
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
        let binding = Binding::open(installation)
            .map_err(|error| crate::supervisor::SupervisorError(error.to_string()))?;
        Ok(Self { binding })
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

impl std::fmt::Debug for Binding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Binding")
            .field("installation", &self.installation.locator())
            .field("build", &self.build())
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
        fields: &[crate::Field],
        fixture_fields: &[crate::protocol::observation::FixtureOutcomeFieldBinding],
        index: usize,
        question: &crate::FixtureFieldQuestion,
    ) -> crate::protocol::observation::FixtureQuestionSetup {
        use crate::protocol::observation::{FixtureQuestionSetup, FixtureQuestionStorage};

        let field = fields
            .iter()
            .find(|field| question.parent_field.is_none() && field.name == question.field);
        let exact = fixture_fields
            .iter()
            .find(|field| question.parent_field.is_none() && field.name == question.field);
        let storage = FixtureQuestionStorage::resolve(field.map(|field| &field.reader), exact);
        let mut setup = FixtureQuestionSetup::new(index, question, storage);
        setup.token = exact.map(|exact| exact.token);

        setup
    }

    /// Derive the fixture registry's loader hooks and resolve each field question against the
    /// storage that its reader arguments prove.
    fn ordinary_fixture(
        analysis: &BoundAnalysis,
        fixture: &crate::FixtureRequest,
    ) -> Result<
        (
            crate::protocol::observation::FixtureOutcomeRegistryBinding,
            Vec<crate::protocol::observation::FixtureQuestionSetup>,
        ),
        crate::supervisor::SupervisorError,
    > {
        use crate::supervisor::SupervisorError;

        let analysis_error = |error: crate::AnalysisError| SupervisorError(error.to_string());
        let registry = fixture.registry();
        let Some(loader) = analysis.fixture_loader(registry).map_err(analysis_error)? else {
            return Err(SupervisorError(
                "Fixture registry has no verified loader and owner boundary".into(),
            ));
        };
        let FixtureRegistryFields { fields, bindings } = analysis
            .fixture_registry_fields(registry)
            .map_err(analysis_error)?;
        let questions = fixture
            .field_questions
            .iter()
            .enumerate()
            .map(|(index, question)| {
                Self::fixture_question_setup(&fields, &bindings, index, question)
            })
            .collect();

        Ok((loader.outcome_binding(), questions))
    }

    /// Resolve the fixture request for the worker and for the reducer of its answer.
    pub(crate) fn fixture_setup(
        &self,
        fixture: &crate::FixtureRequest,
    ) -> Result<crate::protocol::observation::FixtureSetup, crate::supervisor::SupervisorError>
    {
        use crate::supervisor::SupervisorError;

        let bindings = self
            .operation()
            .fixture
            .clone()
            .ok_or_else(|| SupervisorError("No fixture binding for this build".into()))?;
        let analysis = &self.binding.analysis;
        let validation_complete_entry =
            if fixture.window == crate::FixtureWindow::InitialFileLoadAndValidation {
                if bindings.validation.is_none() {
                    return Err(SupervisorError(
                        "No fixture validation binding for this build".into(),
                    ));
                }
                let verified = analysis
                    .verified()
                    .map_err(|error| SupervisorError(error.to_string()))?;
                Some(
                    verified
                        .symbol(MODIFIER_LOG_DEFINITIONS)
                        .map_err(SupervisorError)?,
                )
            } else {
                None
            };
        let (registry, questions) =
            match analysis.inline_fixture(fixture).map_err(SupervisorError)? {
                Some(inline) => inline,
                None => Self::ordinary_fixture(analysis, fixture)?,
            };

        Ok(crate::protocol::observation::FixtureSetup {
            files: fixture.files.keys().cloned().collect(),
            validation_complete_entry,
            questions,
            registry,
            bindings,
        })
    }

    /// Prepare the debugger worker for one session in its work directory. `fixture` is the setup
    /// of the request's fixture, from `fixture_setup`.
    pub(crate) fn observer(
        &self,
        work_directory: &std::path::Path,
        attempt: &str,
        request: &crate::protocol::session::SessionRequest,
        fixture: Option<crate::protocol::observation::FixtureSetup>,
        registries: &std::collections::BTreeMap<
            String,
            crate::protocol::observation::RegistryBinding,
        >,
    ) -> Result<Observer, crate::supervisor::SupervisorError> {
        let operation = self.operation();
        let script_checks = request
            .loaded_modifiers
            .as_ref()
            .and_then(|_| self.binding.script_check_binding());
        let script_scopes = if script_checks.is_some() {
            self.binding.script_scopes()
        } else {
            Ok(Default::default())
        };
        (operation.strategy.prepare)(platform::ObservationSetup {
            work_directory,
            attempt,
            executable: self.installation().executable(),
            executable_hash: self.build(),
            registries,
            fault: request.fault.as_ref(),
            fixture,
            modifiers: request
                .loaded_modifiers
                .as_ref()
                .map(|registries| {
                    self.binding
                        .modifier_table_binding(registries)
                        .map_err(crate::supervisor::SupervisorError)
                })
                .transpose()?,
            startup_seconds: request.startup_seconds,
            machine: &operation.machine,
            package: &operation.strategy.package,
            script_checks,
            script_scopes,
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
    /// whatever mods the user has. The installed content must still be the pinned `content` when
    /// the profile is ready.
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

        for (relative, expected) in content
            .iter()
            .filter(|(relative, _)| copies_pinned_file(relative, fixture))
        {
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
            for (relative, text) in &fixture.files {
                files::write_new(&mount.join(relative), text.as_bytes())?;
            }
        }

        files::write_new(
            &profile.join("mod/native_registry.mod"),
            registry_mod_descriptor(&mount, &replaced_directories(registries, fixture))?.as_bytes(),
        )?;
        files::write_new(
            &profile.join("dlc_load.json"),
            br#"{"enabled_mods":["mod/native_registry.mod"],"disabled_dlcs":[]}"#,
        )?;

        let directories: Vec<String> = registries.keys().cloned().collect();
        if !self
            .installation()
            .session_content_unchanged(&directories, content)
        {
            return Err(SupervisorError(
                "Registry content changed during profile preparation".into(),
            ));
        }

        Ok(())
    }
}
