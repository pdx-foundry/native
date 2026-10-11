//! The messages of one game session between the caller and its supervisor: the request, the
//! controls that follow it, what the session observed at its pause, and the supervisor's final
//! report.
//!
//! [`ObservationControl`] holds the deliberate faults that Native's live tests inject. A request
//! carries one fault together with the observation that receives it.
use super::observation::{ModifierEntry, RegistryKeys};
use crate::{answer::Disposal, supervisor::SupervisorError};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

pub(crate) const MAX_SESSION_SECONDS: u64 = 180;

/// What the caller asks its supervisor to do: start one game and hold it at its pause.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionRequest {
    /// The installation that the caller opened. The supervisor opens it again.
    pub installation: PathBuf,
    /// Identity of the game build that the caller opened. The supervisor must find the same.
    pub build: String,
    /// A new directory for the private game profile and the session's files.
    pub work_directory: PathBuf,
    /// Seconds allowed for the game to reach its pause, 1 to `MAX_SESSION_SECONDS`.
    pub startup_seconds: u64,
    /// Seconds that the paused game may stay idle, 1 to `MAX_SESSION_SECONDS`.
    pub idle_seconds: u64,
    /// Content directories selected for this session.
    pub registries: Vec<String>,
    /// A deliberate fault, for Native's live tests.
    pub fault: Option<Fault>,
    /// Consumer fixture, mounted before launch.
    pub fixture: Option<crate::FixtureRequest>,
    /// Read the loaded modifier table, and the item keys of these registries, where the engine
    /// documents its modifiers; the session pauses there.
    pub loaded_modifiers: Option<Vec<String>>,
}

/// A deliberate fault and the observation that receives it.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Fault {
    pub target: ObservationTarget,
    pub control: ObservationControl,
}

/// The observation that receives a deliberate fault in Native's live tests.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum ObservationTarget {
    /// A selected registry, named by its content directory.
    Registry(String),
    /// The prepared fixture.
    Fixture,
    /// The requested loaded modifier table.
    Modifiers,
    /// A command check at the loaded-content pause.
    ScriptChecks,
}

impl SessionRequest {
    pub(crate) fn validate(&self) -> Result<(), SupervisorError> {
        if let Some(fixture) = &self.fixture {
            fixture
                .validate()
                .map_err(|error| SupervisorError(error.to_string()))?;
        }
        if !self.work_directory.is_absolute()
            || !(1..=MAX_SESSION_SECONDS).contains(&self.startup_seconds)
            || !(1..=MAX_SESSION_SECONDS).contains(&self.idle_seconds)
        {
            return Err(SupervisorError(format!(
                "Expected an absolute work directory and budgets of 1 to {MAX_SESSION_SECONDS} seconds"
            )));
        }
        let valid_registry_directory = |name: &str| {
            name.split('/').all(|segment| {
                !segment.is_empty()
                    && segment.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'
                    })
            })
        };
        // The supervisor checks each name against the registries it discovers in the opened
        // executable, so the number of names follows the build.
        let valid_unique_registry_directories = |names: &[String]| {
            names.iter().all(|name| valid_registry_directory(name))
                && names
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    == names.len()
        };
        if self.registries.is_empty() || !valid_unique_registry_directories(&self.registries) {
            return Err(SupervisorError(
                "Expected one or more unique registry content directories".into(),
            ));
        }
        if self
            .loaded_modifiers
            .as_ref()
            .is_some_and(|names| !valid_unique_registry_directories(names))
        {
            return Err(SupervisorError(
                "Expected unique modifier family registries".into(),
            ));
        }
        // Fixture directories can have inline loaders outside the template registry inventory.
        // The supervisor binds their loader and owner against the opened executable.
        if let Some(fault) = &self.fault {
            let valid = fault.control != ObservationControl::Normal
                && match &fault.target {
                    ObservationTarget::Registry(registry) => self.registries.contains(registry),
                    ObservationTarget::Fixture => self.fixture.is_some(),
                    ObservationTarget::ScriptChecks => {
                        self.loaded_modifiers.is_some()
                            && fault.control == ObservationControl::AccessFailure
                    }
                    ObservationTarget::Modifiers => {
                        self.loaded_modifiers.is_some()
                            && fault.control == ObservationControl::WorkerLoss
                    }
                };
            if !valid {
                return Err(SupervisorError(
                    "The fault requires a selected observation and a supported fault kind".into(),
                ));
            }
        }
        Ok(())
    }
}

/// What the caller sends after the request.
#[derive(Serialize, Deserialize)]
pub(crate) enum Control {
    /// Read and validate a snippet at the loaded-content pause.
    CheckScript {
        input: crate::ScriptCheck,
        request: u64,
        /// The receivers of the commands that the text names.
        durations: Vec<crate::protocol::script_check::DurationReceiver>,
    },
    /// End the session because the caller cancelled.
    Cancel,
    /// End the session in order.
    Close,
    /// The caller answered a question about this registry, so the idle time starts again.
    ReadRegistry { name: String, request: u64 },
    /// The caller read its prepared fixture observation.
    ReadFixture { request: u64 },
    /// The caller read the loaded modifier table.
    ReadModifiers { request: u64 },
}

/// Why a session ended. This says nothing about disposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum SessionOutcome {
    /// The caller closed the session.
    Completed,
    /// The caller cancelled.
    Cancelled,
    /// The startup or idle time elapsed.
    TimedOut,
    /// The control channel closed or became invalid.
    CallerLost,
    /// The observation worker exited before the supervisor stopped it.
    WorkerLost,
    /// Launch, isolation, integrity, or storage failed.
    Failed(String),
}

/// The supervisor's final report on one session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionReport {
    /// Identity of this session in its report and the worker's stream.
    pub attempt: String,
    pub outcome: SessionOutcome,
    /// Whether the supervisor reaped the game. Independent of the outcome.
    pub disposal: Disposal,
    /// Whether cleanup and required bookkeeping completed.
    pub reservation_resolved: bool,
    /// Failures that did not decide the outcome.
    pub diagnostics: Vec<String>,
}

/// Where the supervised game is held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum GameReadiness {
    /// Every observed registry returned from its initial loader; the game stays paused.
    AfterRegistryInitialization,
    /// Only a part of the observed registries returned before the pause.
    DuringRegistryInitialization,
    /// All content has loaded: the game is held where the engine documents its modifiers,
    /// before it shows its main menu. Requested with `GameOptions::loaded_modifiers`.
    AfterContentLoad,
}

/// How much of one registry's collection the session established.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Observed {
    /// Every slot was read and every witness agrees.
    Complete,
    /// The registry was observed, but a witness is missing. The accepted items are kept.
    Partial,
    /// The hook was active, but this loader did not return before the session paused. The
    /// diagnostic says what ended the session first.
    NotLoaded,
    /// The loader ran, but the binding cannot read this registry's item layout.
    Unsupported,
    /// The registry was not observed: no activation or no access.
    Unavailable,
}

impl Observed {
    /// Whether the registry has an answer for the caller: its items were read, with or without
    /// a gap.
    pub(crate) fn has_answer(self) -> bool {
        matches!(self, Self::Complete | Self::Partial)
    }
}

/// The items of one registry, in the engine's collection order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RegistryItems {
    pub items: Vec<String>,
    pub observed: Observed,
    /// Each missing or inconsistent witness. Empty exactly when `observed` is `Complete`.
    pub diagnostics: Vec<String>,
}

/// The accepted table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ObservedModifiers {
    /// Each entry's name and category mask, in table order.
    pub entries: Vec<ModifierEntry>,
    /// The item keys of each requested registry, or why they could not be read.
    pub registries: BTreeMap<String, RegistryKeys>,
}

/// A deliberate fault for Native's live tests, applied to the target of `GameOptions::fault`.
#[doc(hidden)]
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum ObservationControl {
    /// No fault.
    #[default]
    Normal,
    /// Do not set the targeted observation's hook.
    MissingHook,
    /// Set the targeted hook, but leave it disabled when the game resumes.
    LateHook,
    /// Omit one targeted observation record, leaving its sequence gap.
    DroppedRecord,
    /// Do not write the targeted observation's terminal record.
    MissingTerminal,
    /// Make a read of game memory fail in the targeted observation.
    AccessFailure,
    /// Stop the debugger worker during the targeted observation.
    WorkerLoss,
    /// Stop the debugger worker before it attaches or activates a hook.
    WorkerLossBeforeActivation,
}

#[cfg(test)]
impl SessionRequest {
    /// A valid request for tests that start no game.
    pub(crate) fn test() -> Self {
        Self {
            installation: "/absent".into(),
            build: "test".into(),
            work_directory: std::env::temp_dir().join("unused-native-test"),
            startup_seconds: MAX_SESSION_SECONDS,
            idle_seconds: MAX_SESSION_SECONDS,
            registries: vec!["common/traditions".into()],
            fault: None,
            fixture: None,
            loaded_modifiers: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_needs_an_absolute_work_directory_and_valid_budgets() {
        assert!(SessionRequest::test().validate().is_ok());
        let mut relative = SessionRequest::test();
        relative.work_directory = "relative".into();
        assert!(relative.validate().is_err());
        for seconds in [0, 181] {
            let mut startup = SessionRequest::test();
            startup.startup_seconds = seconds;
            assert!(startup.validate().is_err());
            let mut idle = SessionRequest::test();
            idle.idle_seconds = seconds;
            assert!(idle.validate().is_err());
        }
        for names in [
            vec!["common/traditions", "common/traditions"],
            vec!["common/../traditions"],
            vec![""],
        ] {
            let mut invalid = SessionRequest::test();
            invalid.registries = names.into_iter().map(String::from).collect();
            assert!(invalid.validate().is_err());
        }
    }

    #[test]
    fn fixture_directory_binding_is_separate_from_registry_selection() {
        let mut request = SessionRequest::test();
        request.fixture =
            Some(crate::FixtureRequest::field_outcomes(
                "common/special_projects/sample.txt",
                "special_project = { key = sample requirements = { fleet_power = 1 } }",
                [crate::FixtureFieldQuestion::new(
                    "common/special_projects",
                    "sample",
                    "fleet_power",
                )
                .with_parent_field("requirements")],
            ));
        assert!(request.validate().is_ok());
    }

    #[test]
    fn faults_require_a_selected_target_and_supported_kind() {
        let controls = [
            ObservationControl::Normal,
            ObservationControl::MissingHook,
            ObservationControl::LateHook,
            ObservationControl::DroppedRecord,
            ObservationControl::MissingTerminal,
            ObservationControl::AccessFailure,
            ObservationControl::WorkerLoss,
            ObservationControl::WorkerLossBeforeActivation,
        ];
        for target in [
            ObservationTarget::Registry("common/traditions".into()),
            ObservationTarget::Registry("common/unselected".into()),
            ObservationTarget::Fixture,
            ObservationTarget::Modifiers,
            ObservationTarget::ScriptChecks,
        ] {
            for selected in [false, true] {
                for control in controls {
                    let mut request = SessionRequest::test();
                    if selected {
                        request
                            .registries
                            .push("common/tradition_categories".into());
                        request.fixture = Some(crate::FixtureRequest::field_outcomes(
                            "common/tradition_categories/atlas.txt",
                            "atlas = {}",
                            [crate::FixtureFieldQuestion::new(
                                "common/tradition_categories",
                                "example",
                                "traditions",
                            )
                            .with_parsing()],
                        ));
                        request.loaded_modifiers = Some(Vec::new());
                    }
                    request.fault = Some(Fault {
                        target: target.clone(),
                        control,
                    });
                    let expected = match &target {
                        ObservationTarget::Registry(registry) => {
                            request.registries.contains(registry)
                                && control != ObservationControl::Normal
                        }
                        ObservationTarget::Fixture => {
                            selected && control != ObservationControl::Normal
                        }
                        ObservationTarget::ScriptChecks => {
                            selected && control == ObservationControl::AccessFailure
                        }
                        ObservationTarget::Modifiers => {
                            selected && control == ObservationControl::WorkerLoss
                        }
                    };
                    assert_eq!(
                        request.validate().is_ok(),
                        expected,
                        "{target:?} {control:?} selected={selected}"
                    );
                }
            }
        }
    }

    #[test]
    fn obsolete_or_ambiguous_fault_fields_are_rejected() {
        for field in ["fixture_fault", "modifier_fault", "registry"] {
            let mut unknown = serde_json::to_value(SessionRequest::test()).unwrap();
            unknown[field] = "worker-loss".into();
            assert!(serde_json::from_value::<SessionRequest>(unknown).is_err());
        }
        let ambiguous = serde_json::json!({
            "target": {"registry": "common/traditions", "fixture": null},
            "control": "worker-loss"
        });
        assert!(serde_json::from_value::<Fault>(ambiguous).is_err());
    }

    #[test]
    fn the_selection_size_follows_the_build_and_names_stay_unique() {
        let names: Vec<String> = (0..200).map(|index| format!("common/r{index}")).collect();
        let mut large = SessionRequest::test();
        large.registries = names.clone();
        large.loaded_modifiers = Some(names.clone());
        assert!(large.validate().is_ok());

        let mut empty = SessionRequest::test();
        empty.registries = Vec::new();
        assert!(empty.validate().is_err());

        let mut repeated_modifiers = SessionRequest::test();
        repeated_modifiers.loaded_modifiers =
            Some(vec!["common/zones".into(), "common/zones".into()]);
        assert!(repeated_modifiers.validate().is_err());

        let mut repeated = large;
        repeated.registries.push(names[0].clone());
        assert!(repeated.validate().is_err());
    }
}
