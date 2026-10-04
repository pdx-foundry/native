//! Live tests: each case starts the real game through the public API and checks the answers
//! and the cleanup.
//!
//! The cases are ignored by default. To run them, name the installation and use the `cargo live`
//! alias for `cargo test --release --test live -- --ignored`:
//!
//! ```text
//! STELLARIS_PATH=/path/to/Stellaris cargo live
//! STELLARIS_PATH=/path/to/Stellaris cargo live missing_hook
//! ```
//!
//! A word after `cargo live` selects the cases whose name contains it. A case takes about 35
//! seconds; the full set takes longer as cases are added.
//!
//! Every session keeps its work directory, `$TMPDIR/pdx-native-<pid>-<nanos>`, until its case
//! ends. A passing case removes it. A failing case keeps it, also when a check fails after a clean
//! `close`, and prints its path with `session/run-summary.json`: the outcome, the phase times,
//! the hooks, the worker stream's last record and holes, and each observation's compact result.
//! The same directory holds the raw trace, owner events, worker and game output and the private
//! profile's engine logs. A later passing case never removes an earlier case's directory.
//!
//! This file has its own `main` (`harness = false` in `Cargo.toml`) for two reasons. The
//! supervisor is this executable with the `--supervisor` argument, and the standard harness
//! writes to the standard output that the supervisor protocol owns. Only one Native-owned game
//! may run on a host, so the cases run one at a time.
//!
//! The fault cases use the hidden `GameOptions::fault`. A fault applies to one registry; the
//! other registry must stay complete. The tests stop only their own unrelated sentinel process. They check that every
//! game, supervisor and worker-group process that a case started is gone when the case ends.
use pdx_native::internals::{ObservationControl as Fault, ObservationTarget};
use pdx_native::{
    Answer, Basis, Completeness, Disposal, Error, Game, GameOptions, GapKind, Native,
};
use std::{
    collections::BTreeSet,
    fmt::Write as _,
    process::Command,
    time::{Duration, Instant},
};

const TRADITIONS: &str = "common/traditions";
const CATEGORIES: &str = "common/tradition_categories";
const ASCENSION_PERKS: &str = "common/ascension_perks";
const RELICS: &str = "common/relics";
/// The registries whose database generators register modifier families (SDK-540), with the
/// item counts of the catalogued M451-hotfix build.
const GENERATOR_REGISTRIES: [(&str, usize); 6] = [
    ("common/buildings", 498),
    ("common/bypass", 10),
    ("common/districts", 147),
    ("common/megastructures", 164),
    ("common/situations", 90),
    ("common/zones", 146),
];

type Outcome = Result<(), Box<dyn std::error::Error>>;

#[path = "live/scoped_numeric.rs"]
mod scoped_numeric;

#[path = "live/numeric.rs"]
mod numeric_conversion;
#[path = "live/script.rs"]
mod script_checks;
#[path = "live/durations.rs"]
mod stored_durations;

// Public in this test binary so unused static comparison entry points do not trigger dead-code warnings.
#[path = "parity/comparison.rs"]
pub mod comparison;

/// What the registry that receives a fault must give.
#[derive(Clone, Copy)]
enum Expect {
    /// An `Error::Observation`: the items could not be observed.
    NoAnswer,
    /// A `Partial` answer with an `IncompleteObservation` gap.
    PartialAnswer,
}

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    // The supervisor role. Nothing else may write to the standard output in this role.
    if arguments
        .first()
        .is_some_and(|argument| argument == "--supervisor")
    {
        if let Err(error) = pdx_native::supervisor::serve(std::io::stdin(), std::io::stdout()) {
            eprintln!("supervisor: {error}");
            std::process::exit(1);
        }
        return;
    }
    let filter = arguments.iter().find(|argument| !argument.starts_with('-'));
    let cases: Vec<_> = cases()
        .into_iter()
        .filter(|(name, _)| filter.is_none_or(|filter| name.contains(filter.as_str())))
        .collect();
    if arguments.iter().any(|argument| argument == "--list") {
        for (name, _) in &cases {
            println!("{name}: test");
        }
        return;
    }
    if !arguments.iter().any(|argument| argument == "--ignored") {
        println!(
            "{} live cases ignored: they require STELLARIS_PATH and `--ignored`",
            cases.len()
        );
        if let Err(error) = failed_cases_keep_their_work_directories() {
            println!("work directory retention ... FAILED: {error}");
            std::process::exit(1);
        }
        println!("work directory retention ... ok");
        if let Err(error) = cleanup_gate_checks() {
            println!("cleanup gate ... FAILED: {error}");
            std::process::exit(1);
        }
        println!("cleanup gate ... ok");
        return;
    }
    let installation = std::env::var_os("STELLARIS_PATH")
        .expect("STELLARIS_PATH names the Stellaris installation");
    let native = Native::open(installation).expect("the installed build is in the catalogue");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let mut failed = Vec::new();
    println!("running {} live cases, one at a time", cases.len());
    for (name, case) in cases {
        let before = game_processes(&processes().expect("process inventory"));
        if !before.is_empty() {
            // Never start a second game, and never touch a game that is not ours.
            println!("test {name} ... not run: Stellaris already runs {before:?}");
            failed.push(name);
            break;
        }
        let earlier_work = work_directories().expect("work directory inventory");
        let started = Instant::now();
        let isolation =
            Isolation::begin().expect("ordinary profile and unrelated process baseline");
        let outcome = runtime.block_on(run(&native, &case));
        let isolation_result = isolation.finish();
        let cleanup = processes_are_gone(&before, &earlier_work);
        let result = outcome
            .and(isolation_result)
            .and(cleanup)
            .and_then(|()| remove_work_directories(&earlier_work));
        let seconds = started.elapsed().as_secs();
        match result {
            Ok(()) => println!("test {name} ... ok ({seconds} s)"),
            Err(error) => {
                println!("test {name} ... FAILED ({seconds} s): {error}");
                for line in kept_work_directories(&earlier_work) {
                    println!("  {line}");
                }
                failed.push(name);
                // A later case cannot start while a process of this one remains.
                if processes_are_gone(&before, &earlier_work).is_err() {
                    break;
                }
            }
        }
    }
    if !failed.is_empty() {
        println!("failed: {failed:?}");
        std::process::exit(1);
    }
}

enum Case {
    ScriptNumericStored,
    StoredDurations,
    ScriptArguments,
    ScriptAttribution,
    ScriptDeepNesting,
    ScriptAccessFailure,
    LoaderFixture,
    LoadedModifierKeyLayouts,
    Fixture(Fault),
    FixtureTimeout,
    FixtureRefusal,
    FixtureOutcome(FixtureOutcomeCase),
    FixtureTransfer,
    FixtureRelicPortrait,
    FixtureBlockParsing,
    FixtureReadScope,
    FixtureModifierBlock,
    FixtureNumeric {
        registry: &'static str,
        integer: Option<&'static str>,
        fixed: &'static str,
        fractional_final: i64,
    },
    FixtureNestedNumeric(Fault),
    NumericConversionMatrix,
    ScopedNumericMatrix,
    ScopedNumericWorkerLoss,
    /// Validation samples of one block field, each in its own definition of one fixture file.
    FixtureValidation {
        field: &'static str,
        samples: Vec<ValidationSample>,
    },
    /// Validation samples of commands whose answers must be `Complete`. The samples come from
    /// the grammar that the method established; a failed sample contradicts the grammar.
    FixtureArgument {
        field: &'static str,
        commands: Vec<&'static str>,
        samples: Vec<ValidationSample>,
    },
    StartupTimeout,
    DropWithoutClose,
    Fault {
        registry: &'static str,
        other: &'static str,
        control: Fault,
        expect: Expect,
    },
    LoadedModifiers,
    LoadedModifiersWorkerLoss,
    LoadedModifiersMissingRegistryHook,
    WorkerLoss {
        registry: &'static str,
        control: Fault,
    },
}

/// One validation sample: a child command in a block field, and the engine stage whose
/// source-located diagnostic rejects it, or `None` when the engine accepts it.
struct ValidationSample {
    name: String,
    child: String,
    stage: Option<&'static str>,
}

impl ValidationSample {
    fn new(name: impl Into<String>, child: impl Into<String>, stage: Option<&'static str>) -> Self {
        Self {
            name: name.into(),
            child: child.into(),
            stage,
        }
    }

    fn definition(&self) -> String {
        format!("native_validation_{}", self.name)
    }

    /// The sample's definition in a fixture file. The child is at `CHILD`.
    fn definition_lines(&self, field: &str) -> [String; Self::LINES] {
        [
            format!("{} = {{", self.definition()),
            format!(" {field} = {{"),
            format!("  {}", self.child),
            " }".into(),
            "}".into(),
        ]
    }

    const LINES: usize = 5;
    const CHILD: usize = 2;

    /// The one-based line of the child of the sample at `index` in a file of definitions.
    fn child_line(index: usize) -> u64 {
        (index * Self::LINES + Self::CHILD + 1) as u64
    }

    /// The index of the sample whose definition holds this one-based line.
    fn index_at(line: u64) -> Option<usize> {
        let offset = usize::try_from(line.checked_sub(1)?).ok()?;
        Some(offset / Self::LINES)
    }
}

#[derive(Clone, Copy)]
enum FixtureOutcomeCase {
    Valid,
    Omitted,
    Repeated,
    Malformed,
    UnknownField,
    SameOwner,
    DiagnosticsNotRequested,
    MaximumQuestions,
    UnrelatedDefinitions,
}

fn cases() -> Vec<(String, Case)> {
    let mut cases = vec![
        ("script_numeric".to_owned(), Case::ScriptNumericStored),
        ("stored_durations".to_owned(), Case::StoredDurations),
        ("script_arguments".to_owned(), Case::ScriptArguments),
        ("script_attribution".to_owned(), Case::ScriptAttribution),
        ("script_deep_nesting".to_owned(), Case::ScriptDeepNesting),
        (
            "script_access_failure".to_owned(),
            Case::ScriptAccessFailure,
        ),
        ("loader_fixture".to_owned(), Case::LoaderFixture),
        ("fixture_read_scope".to_owned(), Case::FixtureReadScope),
        (
            "loaded_modifier_key_layouts".to_owned(),
            Case::LoadedModifierKeyLayouts,
        ),
        ("loaded_modifiers".to_owned(), Case::LoadedModifiers),
        (
            "loaded_modifiers_worker_loss".to_owned(),
            Case::LoadedModifiersWorkerLoss,
        ),
        (
            "loaded_modifiers_missing_registry_hook".to_owned(),
            Case::LoadedModifiersMissingRegistryHook,
        ),
        ("startup_timeout".to_owned(), Case::StartupTimeout),
        ("drop_without_close".to_owned(), Case::DropWithoutClose),
    ];
    let faults = [
        ("missing_hook", Fault::MissingHook, Expect::NoAnswer),
        ("late_hook", Fault::LateHook, Expect::NoAnswer),
        ("access_failure", Fault::AccessFailure, Expect::NoAnswer),
        (
            "dropped_record",
            Fault::DroppedRecord,
            Expect::PartialAnswer,
        ),
        (
            "missing_terminal",
            Fault::MissingTerminal,
            Expect::PartialAnswer,
        ),
    ];
    for (registry, other) in [(TRADITIONS, CATEGORIES)] {
        let short = registry.rsplit('/').next().unwrap();
        for (name, control, expect) in faults {
            let case = Case::Fault {
                registry,
                other,
                control,
                expect,
            };
            cases.push((format!("{name}_in_{short}"), case));
        }
        cases.push((
            format!("worker_loss_in_{short}"),
            Case::WorkerLoss {
                registry,
                control: Fault::WorkerLoss,
            },
        ));
    }
    cases.push((
        "worker_loss_before_activation".into(),
        Case::WorkerLoss {
            registry: TRADITIONS,
            control: Fault::WorkerLossBeforeActivation,
        },
    ));
    for (name, control) in [
        ("normal", Fault::Normal),
        ("missing_hook", Fault::MissingHook),
        ("late_hook", Fault::LateHook),
        ("dropped_record", Fault::DroppedRecord),
        ("missing_terminal", Fault::MissingTerminal),
        ("worker_loss", Fault::WorkerLoss),
        ("access_failure", Fault::AccessFailure),
    ] {
        cases.push((format!("fixture_{name}"), Case::Fixture(control)));
    }
    cases.push((
        "fixture_transfer_string_reader".into(),
        Case::FixtureTransfer,
    ));
    cases.push((
        "fixture_transfer_relic_portrait".into(),
        Case::FixtureRelicPortrait,
    ));
    cases.push(("fixture_block_parsing".into(), Case::FixtureBlockParsing));
    cases.push(("fixture_modifier_block".into(), Case::FixtureModifierBlock));
    cases.push((
        "fixture_numeric_megastructures".into(),
        Case::FixtureNumeric {
            registry: "common/megastructures",
            integer: Some("sensor_range"),
            fixed: "build_time",
            fractional_final: 100_000,
        },
    ));
    cases.push((
        "fixture_numeric_armies".into(),
        Case::FixtureNumeric {
            registry: "common/armies",
            integer: None,
            fixed: "war_exhaustion",
            fractional_final: -123_456,
        },
    ));
    cases.push((
        "fixture_numeric_nested_projects".into(),
        Case::FixtureNestedNumeric(Fault::Normal),
    ));
    cases.push((
        "fixture_numeric_nested_worker_loss".into(),
        Case::FixtureNestedNumeric(Fault::WorkerLoss),
    ));
    cases.push((
        "fixture_numeric_conversion_matrix".into(),
        Case::NumericConversionMatrix,
    ));
    cases.push((
        "fixture_scoped_numeric_matrix".into(),
        Case::ScopedNumericMatrix,
    ));
    cases.push((
        "fixture_scoped_numeric_worker_loss".into(),
        Case::ScopedNumericWorkerLoss,
    ));
    let parser_log = Some("engine-parser-log");
    let validation_log = Some("engine-validation-log");
    let mut triggers = vec![
        ValidationSample::new("valid", "always = yes", None),
        ValidationSample::new("wrong_scope", "is_planet_class = pc_barren", parser_log),
        ValidationSample::new("unknown", "native_unknown_trigger = yes", validation_log),
    ];
    for (name, accepted, rejected) in [
        (
            "and",
            "and = { always = yes }",
            "and = { is_planet_class = pc_barren }",
        ),
        (
            "or",
            "or = { always = yes }",
            "or = { is_planet_class = pc_barren }",
        ),
        (
            "not",
            "not = { always = no }",
            "not = { is_planet_class = pc_barren }",
        ),
        (
            "if",
            "if = { limit = { always = yes } always = yes }",
            "if = { limit = { always = yes } is_planet_class = pc_barren }",
        ),
        (
            "else_if",
            "if = { limit = { always = no } always = yes } else_if = { limit = { always = yes } always = yes }",
            "if = { limit = { always = no } always = yes } else_if = { limit = { always = yes } is_planet_class = pc_barren }",
        ),
        (
            "else",
            "if = { limit = { always = no } always = yes } else = { always = yes }",
            "if = { limit = { always = no } always = yes } else = { is_planet_class = pc_barren }",
        ),
    ] {
        triggers.push(ValidationSample::new(
            format!("{name}_accepted"),
            accepted,
            None,
        ));
        triggers.push(ValidationSample::new(
            format!("{name}_rejected"),
            rejected,
            parser_log,
        ));
    }
    for (name, child) in [
        ("empty_limit", "if = { limit = { } always = yes }"),
        ("missing_limit", "if = { always = yes }"),
        (
            "repeated_limit",
            "if = { limit = { always = yes } limit = { always = no } always = yes }",
        ),
        (
            "late_limit",
            "if = { always = yes limit = { always = yes } }",
        ),
    ] {
        triggers.push(ValidationSample::new(format!("edge_{name}"), child, None));
    }
    let mut effects = vec![
        ValidationSample::new("valid", "set_country_flag = native_fixture_flag", None),
        ValidationSample::new("wrong_scope", "change_pc = pc_barren", parser_log),
        ValidationSample::new("unknown", "native_unknown_effect = yes", validation_log),
    ];
    for (name, accepted, rejected) in [
        (
            "if",
            "if = { limit = { always = yes } set_country_flag = native_fixture_flag }",
            "if = { limit = { always = yes } native_unknown_effect = yes }",
        ),
        (
            "else_if",
            "if = { limit = { always = no } set_country_flag = native_fixture_flag } else_if = { limit = { always = yes } set_country_flag = native_fixture_flag }",
            "if = { limit = { always = no } set_country_flag = native_fixture_flag } else_if = { limit = { always = yes } native_unknown_effect = yes }",
        ),
        (
            "else",
            "if = { limit = { always = no } set_country_flag = native_fixture_flag } else = { set_country_flag = native_fixture_flag }",
            "if = { limit = { always = no } set_country_flag = native_fixture_flag } else = { native_unknown_effect = yes }",
        ),
        (
            "hidden_effect",
            "hidden_effect = { set_country_flag = native_fixture_flag }",
            "hidden_effect = { native_unknown_effect = yes }",
        ),
        (
            "random_list",
            "random_list = { 10 = { set_country_flag = native_first } 90 = { set_country_flag = native_second } }",
            "random_list = { 10 = { native_unknown_effect = yes } }",
        ),
        (
            "every_owned_planet",
            "every_owned_planet = { limit = { always = yes } set_planet_flag = native_fixture_flag }",
            "every_owned_planet = { limit = { always = yes } native_unknown_effect = yes }",
        ),
    ] {
        effects.push(ValidationSample::new(
            format!("{name}_accepted"),
            accepted,
            None,
        ));
        effects.push(ValidationSample::new(
            format!("{name}_rejected"),
            rejected,
            validation_log,
        ));
    }
    for (name, child) in [
        (
            "empty_limit",
            "if = { limit = { } set_country_flag = native_fixture_flag }",
        ),
        (
            "missing_limit",
            "if = { set_country_flag = native_fixture_flag }",
        ),
        (
            "repeated_limit",
            "if = { limit = { always = yes } limit = { always = no } set_country_flag = native_fixture_flag }",
        ),
        (
            "late_limit",
            "if = { set_country_flag = native_fixture_flag limit = { always = yes } }",
        ),
        (
            "else_first",
            "if = { limit = { } else = { set_country_flag = native_fixture_flag } }",
        ),
        (
            "else_after_effect",
            "if = { limit = { } set_country_flag = native_first else = { set_country_flag = native_second } }",
        ),
        (
            "else_after_if",
            "if = { limit = { } if = { limit = { } set_country_flag = native_first } else = { set_country_flag = native_second } }",
        ),
        (
            "weighted_zero",
            "random_list = { 0 = { set_country_flag = native_first } 10 = { set_country_flag = native_second } }",
        ),
    ] {
        effects.push(ValidationSample::new(format!("edge_{name}"), child, None));
    }
    cases.push((
        "fixture_control_triggers".into(),
        Case::FixtureValidation {
            field: "potential",
            samples: triggers,
        },
    ));
    cases.push((
        "fixture_control_effects".into(),
        Case::FixtureValidation {
            field: "on_enabled",
            samples: effects,
        },
    ));
    // A reader report or a malformed block can upset the parsing of the definitions after it, so
    // each of these samples keeps its own session.
    for (name, child, stage) in [
        (
            "weighted_nonnumeric",
            "random_list = { not_a_weight = { set_country_flag = native_fixture_flag } }",
            Some("reader-unexpected-report"),
        ),
        (
            "malformed",
            "if = { limit = yes set_country_flag = native_fixture_flag }",
            validation_log,
        ),
    ] {
        cases.push((
            format!("fixture_control_edge_effect_{name}"),
            Case::FixtureValidation {
                field: "on_enabled",
                samples: vec![ValidationSample::new(name, child, stage)],
            },
        ));
    }
    cases.extend(argument_cases().into_iter().filter(|(name, _)| {
        !name.ends_with("_unknown_key")
            || name == "fixture_argument_get_councilor_level_unknown_key"
    }));
    cases.push((
        "fixture_argument_technology_reference".into(),
        Case::FixtureArgument {
            field: "potential",
            commands: vec![],
            samples: vec![
                ValidationSample::new(
                    "missing_technology",
                    "has_technology = native_missing_technology",
                    Some("engine-validation-log"),
                ),
                ValidationSample::new(
                    "installed_technology",
                    "has_technology = tech_lasers_1",
                    None,
                ),
            ],
        },
    ));
    cases.push(("fixture_timeout".into(), Case::FixtureTimeout));
    cases.push(("fixture_refusal".into(), Case::FixtureRefusal));
    for (name, case) in [
        ("valid", FixtureOutcomeCase::Valid),
        ("omitted", FixtureOutcomeCase::Omitted),
        ("repeated", FixtureOutcomeCase::Repeated),
        ("malformed", FixtureOutcomeCase::Malformed),
        ("unknown_field", FixtureOutcomeCase::UnknownField),
        ("same_owner", FixtureOutcomeCase::SameOwner),
        (
            "diagnostics_not_requested",
            FixtureOutcomeCase::DiagnosticsNotRequested,
        ),
        ("maximum_questions", FixtureOutcomeCase::MaximumQuestions),
        (
            "unrelated_definitions",
            FixtureOutcomeCase::UnrelatedDefinitions,
        ),
    ] {
        cases.push((
            format!("fixture_outcome_{name}"),
            Case::FixtureOutcome(case),
        ));
    }
    cases
}

async fn run(native: &Native, case: &Case) -> Outcome {
    match *case {
        Case::FixtureBlockParsing => fixture_block_parsing(native).await,
        Case::FixtureReadScope => fixture_read_scope(native).await,
        Case::FixtureModifierBlock => fixture_modifier_block(native).await,
        Case::FixtureNumeric {
            registry,
            integer,
            fixed,
            fractional_final,
        } => fixture_numeric(registry, integer, fixed, fractional_final).await,
        Case::FixtureNestedNumeric(control) => fixture_nested_numeric(control).await,
        Case::NumericConversionMatrix => numeric_conversion::matrix().await,
        Case::ScopedNumericMatrix => scoped_numeric::matrix().await,
        Case::ScopedNumericWorkerLoss => scoped_numeric::worker_loss().await,
        Case::FixtureValidation { field, ref samples } => {
            fixture_validation(native, field, samples).await
        }
        Case::FixtureArgument {
            field,
            ref commands,
            ref samples,
        } => fixture_argument(native, field, commands, samples).await,
        Case::ScriptNumericStored => script_numeric::stored(native).await,
        Case::StoredDurations => stored_durations::stored(native).await,
        Case::ScriptArguments => script_checks::arguments(native).await,
        Case::ScriptAttribution => script_checks::attribution(native).await,
        Case::ScriptDeepNesting => script_checks::deep_nesting(native).await,
        Case::ScriptAccessFailure => script_checks::access_failure(native).await,
        Case::LoaderFixture => loader_fixture(native).await,
        Case::LoadedModifierKeyLayouts => loaded_modifier_key_layouts(native).await,
        Case::LoadedModifiers => loaded_modifiers(native).await,
        Case::LoadedModifiersWorkerLoss => loaded_modifiers_worker_loss(native).await,
        Case::LoadedModifiersMissingRegistryHook => {
            loaded_modifiers_missing_registry_hook(native).await
        }
        Case::Fixture(control) => fixture_case(control).await,
        Case::FixtureTimeout => fixture_timeout(native).await,
        Case::FixtureRefusal => fixture_refusal(native).await,
        Case::FixtureOutcome(case) => fixture_outcome(case).await,
        Case::FixtureTransfer => fixture_transfer(native).await,
        Case::FixtureRelicPortrait => fixture_relic_portrait(native).await,
        Case::StartupTimeout => startup_timeout(native).await,
        Case::DropWithoutClose => drop_without_close(native).await,
        Case::Fault {
            registry,
            other,
            control,
            expect,
        } => fault(native, registry, other, control, expect).await,
        Case::WorkerLoss { registry, control } => worker_loss(native, registry, control).await,
    }
}

/// The SDK-548 fixture sample, `tests/population/m451-hotfix/command-fixture-sample.json`.
/// Accepted samples cover each form, value alternative and fixed key of a command. Rejected
/// samples use only a rejection that the method established on every path. A rejected key gives
/// a reader report that can upset the definitions after it, so it keeps its own session.
fn argument_cases() -> Vec<(String, Case)> {
    let parser_log = Some("engine-parser-log");
    let unexpected = Some("reader-unexpected-report");
    let mut cases = Vec::new();

    let blocks = [
        (
            "potential",
            "get_councilor_level",
            "type = councilor_curator_archivist",
        ),
        (
            "potential",
            "is_ai_ship_role",
            "ship_size = corvette role = explosive",
        ),
        (
            "potential",
            "has_completed_event_chain_counter",
            "event_chain = nomad_star_journal_chain counter = star_journal_progress",
        ),
        (
            "on_enabled",
            "activate_saved_leader",
            "key = native_fixture_leader add_to_owned = yes effect = { }",
        ),
        (
            "on_enabled",
            "store_country_backup_data",
            "name = yes flag = yes government = yes room = yes ethics = yes",
        ),
        (
            "on_enabled",
            "leave_alliance",
            "override_requirements = yes apply_opinion_penalty = no",
        ),
        (
            "on_enabled",
            "reset_event_chain_counter",
            "event_chain = nomad_star_journal_chain counter = star_journal_progress",
        ),
        (
            "on_enabled",
            "create_ship_design",
            "random_existing_design = corvette design = \"NAME_Sky_Dragon_Baby\"",
        ),
    ];
    let booleans = [
        "stop_crisis_sound",
        "set_advisor_active",
        "remove_from_galactic_community",
        "set_galactic_defense_force",
        "set_council_emergency_measures",
        "clear_ai_starbase_shields_ratio",
        "downgrade_all_buildings",
        "open_shroud_tab",
        "run_ai_strategic_war_data",
        "unlock_council_selection",
        "reset_policy_cooldowns",
        "add_to_galactic_community_no_message",
    ];

    for field in ["potential", "on_enabled"] {
        let mut commands = Vec::new();
        let mut samples = Vec::new();
        for &(_, command, keys) in blocks.iter().filter(|block| block.0 == field) {
            commands.push(command);
            samples.push(ValidationSample::new(
                format!("{command}_keys"),
                format!("{command} = {{ {keys} }}"),
                None,
            ));
        }
        if field == "on_enabled" {
            for command in booleans {
                commands.push(command);
                samples.push(ValidationSample::new(
                    format!("{command}_yes"),
                    format!("{command} = yes"),
                    None,
                ));
                samples.push(ValidationSample::new(
                    format!("{command}_not_boolean"),
                    format!("{command} = native_not_boolean"),
                    parser_log,
                ));
            }
        }
        cases.push((
            format!("fixture_argument_{field}"),
            Case::FixtureArgument {
                field,
                commands,
                samples,
            },
        ));
    }
    for (field, command, _) in blocks {
        cases.push((
            format!("fixture_argument_{command}_unknown_key"),
            Case::FixtureArgument {
                field,
                commands: vec![command],
                samples: vec![ValidationSample::new(
                    "unknown_key",
                    format!("{command} = {{ native_unknown_key = yes }}"),
                    unexpected,
                )],
            },
        ));
    }
    cases
}

/// Each command's grammar must be `Complete` before its samples can test it.
async fn fixture_argument(
    native: &Native,
    field: &str,
    commands: &[&str],
    samples: &[ValidationSample],
) -> Outcome {
    let kind = if field == "potential" {
        pdx_native::DeclarationKind::Trigger
    } else {
        pdx_native::DeclarationKind::Effect
    };
    for command in commands {
        let answer = native.command_grammar(kind, command)?;
        if answer.completeness != Completeness::Complete {
            return Err(format!("{command} is not complete: {:?}", answer.gaps).into());
        }
    }
    script_checks::paired_file(native, field, samples).await
}

async fn fixture_outcome(case: FixtureOutcomeCase) -> Outcome {
    let request = fixture_outcome_request(case);
    let recorded = tempfile::tempdir()?;
    let native = Native::open(std::env::var_os("STELLARIS_PATH").unwrap())?
        .record_answers_to(recorded.path());
    let mut game = native
        .start_game(options().fixture(request.clone()))
        .await?;
    let mut result = async {
        let answer = game.observe_fixture().await?;
        assert_fixture_outcome(case, &answer)?;
        assert_recorded_fixture(recorded.path(), request, &answer).await?;
        Ok(())
    }
    .await;
    and_close(&mut result, &mut game).await;
    result
}

async fn fixture_transfer(native: &Native) -> Outcome {
    use pdx_native::{FixtureFieldQuestion, FixtureRequest, FixtureStorage};

    let request = FixtureRequest::field_outcomes(
        "common/ascension_perks/native_transfer.txt",
        "native_transfer = {\n custom_tooltip = \"transfer_tip\"\n unlocks_agenda = \"transfer_agenda\"\n}\n",
        [
            FixtureFieldQuestion::new(ASCENSION_PERKS, "native_transfer", "custom_tooltip"),
            FixtureFieldQuestion::new(ASCENSION_PERKS, "native_transfer", "unlocks_agenda"),
        ],
    );
    let mut game = native.start_game(options().fixture(request)).await?;
    let mut result = async {
        let answer = game.observe_fixture().await?;
        if answer.completeness != Completeness::Complete {
            return Err(format!("transfer was incomplete: {answer:?}").into());
        }
        let [tooltip, agenda] = answer.value.field_outcomes.as_slice() else {
            return Err(format!("transfer outcomes: {answer:?}").into());
        };
        if tooltip.owner.is_none() || tooltip.owner != agenda.owner {
            return Err(format!("transfer owner join: {answer:?}").into());
        }
        for (outcome, expected) in [(tooltip, "transfer_tip"), (agenda, "transfer_agenda")] {
            let FixtureStorage::Observed {
                occurrences,
                final_value,
                completeness: Completeness::Complete,
            } = &outcome.storage
            else {
                return Err(format!("transfer storage: {answer:?}").into());
            };
            if occurrences.len() != 1
                || occurrences[0].value != pdx_native::FixtureValue::String(expected.into())
                || final_value.as_ref() != Some(&pdx_native::FixtureValue::String(expected.into()))
            {
                return Err(format!("transfer value: {answer:?}").into());
            }
        }
        Ok(())
    }
    .await;
    and_close(&mut result, &mut game).await;
    result
}

async fn fixture_relic_portrait(native: &Native) -> Outcome {
    use pdx_native::{FixtureFieldQuestion, FixtureRequest, FixtureStorage};

    let request = FixtureRequest::field_outcomes(
        "common/relics/native_transfer.txt",
        "native_transfer = {\n portrait = \"transfer_relic_portrait\"\n}\n",
        [FixtureFieldQuestion::new(
            RELICS,
            "native_transfer",
            "portrait",
        )],
    );
    let mut game = native.start_game(options().fixture(request)).await?;
    let mut result = async {
        let answer = game.observe_fixture().await?;
        if answer.completeness != Completeness::Complete {
            return Err(format!("novel field was incomplete: {answer:?}").into());
        }
        let [outcome] = answer.value.field_outcomes.as_slice() else {
            return Err(format!("novel field outcomes: {answer:?}").into());
        };
        if outcome.owner.is_none() {
            return Err(format!("novel field owner: {answer:?}").into());
        }
        let FixtureStorage::Observed {
            occurrences,
            final_value,
            completeness: Completeness::Complete,
        } = &outcome.storage
        else {
            return Err(format!("novel field storage: {answer:?}").into());
        };
        if occurrences.len() != 1
            || occurrences[0].value
                != pdx_native::FixtureValue::String("transfer_relic_portrait".into())
            || final_value.as_ref()
                != Some(&pdx_native::FixtureValue::String(
                    "transfer_relic_portrait".into(),
                ))
        {
            return Err(format!("novel field value: {answer:?}").into());
        }
        Ok(())
    }
    .await;
    and_close(&mut result, &mut game).await;
    result
}

type NumericExpectations = std::collections::BTreeMap<
    (String, String),
    (
        Vec<pdx_native::StoredFieldOccurrence>,
        pdx_native::FixtureValue,
    ),
>;

fn numeric_fixture(
    registry: &str,
    integer: Option<&str>,
    fixed: &str,
    fractional_final: i64,
) -> Result<(pdx_native::FixtureRequest, NumericExpectations), std::fmt::Error> {
    use pdx_native::{FixtureFieldQuestion, FixtureRequest, FixtureValue, StoredFieldOccurrence};

    let mut text = String::new();
    let mut questions = Vec::new();
    let mut expected = std::collections::BTreeMap::new();
    for definition in [
        "numeric_boundary",
        "numeric_fractional",
        "numeric_malformed",
    ] {
        writeln!(text, "{definition} = {{")?;
        for (field, is_integer) in integer
            .into_iter()
            .map(|name| (name, true))
            .chain([(fixed, false)])
        {
            let (inputs, value) = match (definition, is_integer) {
                ("numeric_boundary", true) => (vec!["2147483647"], FixtureValue::Integer(i32::MAX)),
                ("numeric_boundary", false) => (
                    vec!["92233720368547.75807"],
                    FixtureValue::FixedPoint {
                        raw: i64::MAX,
                        scale: 100_000,
                    },
                ),
                ("numeric_fractional", true) => (vec!["-1.234567"], FixtureValue::Integer(-1)),
                ("numeric_fractional", false) => (
                    vec!["-1.234567"],
                    FixtureValue::FixedPoint {
                        raw: -123_456,
                        scale: 100_000,
                    },
                ),
                (_, true) => (vec!["7", "not_a_number"], FixtureValue::Integer(7)),
                (_, false) => (
                    vec!["7", "not_a_number"],
                    FixtureValue::FixedPoint {
                        raw: 700_000,
                        scale: 100_000,
                    },
                ),
            };
            let mut occurrences = Vec::new();
            for (index, input) in inputs.iter().enumerate() {
                let line = text.lines().count() as u64 + 1;
                writeln!(text, " {field} = {input}")?;
                occurrences.push(StoredFieldOccurrence {
                    line,
                    occurrence: index as u64 + 1,
                    value: value.clone(),
                });
            }
            questions.push(FixtureFieldQuestion::new(registry, definition, field).with_parsing());
            let final_value = if definition == "numeric_fractional" && !is_integer {
                FixtureValue::FixedPoint {
                    raw: fractional_final,
                    scale: 100_000,
                }
            } else {
                value
            };
            expected.insert(
                (definition.to_owned(), field.to_owned()),
                (occurrences, final_value),
            );
        }
        writeln!(text, "}}")?;
    }
    let request =
        FixtureRequest::field_outcomes(format!("{registry}/native_numeric.txt"), text, questions);
    Ok((request, expected))
}

async fn fixture_numeric(
    registry: &str,
    integer: Option<&str>,
    fixed: &str,
    fractional_final: i64,
) -> Outcome {
    use pdx_native::{DiagnosticCoverage, DiagnosticWindow, FixtureParsing, FixtureStorage};

    let (request, expected) = numeric_fixture(registry, integer, fixed, fractional_final)?;
    let recorded = tempfile::tempdir()?;
    let native = Native::open(std::env::var_os("STELLARIS_PATH").unwrap())?
        .record_answers_to(recorded.path());
    let mut game = native
        .start_game(options().fixture(request.clone()))
        .await?;
    let mut result = async {
        let answer = game.observe_fixture().await?;
        if answer.completeness != Completeness::Complete
            || !answer.gaps.is_empty()
            || answer.value.diagnostic_coverage
                != (DiagnosticCoverage::Complete {
                    window: DiagnosticWindow::FixtureFileLoad,
                })
        {
            return Err(format!("numeric observation incomplete: {answer:?}").into());
        }
        for outcome in &answer.value.field_outcomes {
            let (occurrences, final_value) = &expected[&(
                outcome.question.definition.clone(),
                outcome.question.field.clone(),
            )];
            let expected_storage = FixtureStorage::Observed {
                occurrences: occurrences.clone(),
                final_value: Some(final_value.clone()),
                completeness: Completeness::Complete,
            };
            if outcome.owner.is_none() || outcome.storage != expected_storage {
                return Err(format!(
                    "numeric storage differs: expected {expected_storage:?}; got {outcome:?}"
                )
                .into());
            }
            let FixtureParsing::Observed {
                occurrences: parsed,
                completeness: Completeness::Complete,
            } = &outcome.parsing
            else {
                return Err(format!("numeric parsing incomplete: {outcome:?}").into());
            };
            if parsed.len() != occurrences.len()
                || parsed.iter().any(|item| item.return_line.is_none())
            {
                return Err(format!("numeric parser occurrences: {outcome:?}").into());
            }
            let malformed = outcome.question.definition == "numeric_malformed";
            if outcome.diagnostics.len() != usize::from(malformed) {
                return Err(format!("numeric diagnostics differ: {answer:?}").into());
            }
            if malformed {
                let diagnostic = &answer.value.diagnostics[outcome.diagnostics[0]];
                if diagnostic.text != "Malformed token"
                    || diagnostic.stage != "reader-malformed-report"
                {
                    return Err(format!("numeric malformed diagnostic: {diagnostic:?}").into());
                }
            }
        }
        if answer.value.field_outcomes.len() != expected.len() {
            return Err(format!("missing numeric outcomes: {answer:?}").into());
        }
        assert_recorded_fixture(recorded.path(), request, &answer).await?;
        Ok(())
    }
    .await;
    and_close(&mut result, &mut game).await;
    result
}

async fn fixture_block_parsing(native: &Native) -> Outcome {
    use pdx_native::{FixtureFieldQuestion, FixtureParsing, FixtureRequest, FixtureStorage};

    let request = FixtureRequest::field_outcomes(
        "common/traditions/native_blocks.txt",
        "native_blocks = {\n potential = {\n  and = { always = yes }\n }\n potential = { always = no }\n}\n",
        [FixtureFieldQuestion::new(TRADITIONS, "native_blocks", "potential").with_parsing()],
    );
    let mut game = native.start_game(options().fixture(request)).await?;
    let mut result = async {
        let answer = game.observe_fixture().await?;
        let [outcome] = answer.value.field_outcomes.as_slice() else {
            return Err(format!("block outcomes: {answer:?}").into());
        };
        let FixtureParsing::Observed {
            occurrences,
            completeness: Completeness::Complete,
        } = &outcome.parsing
        else {
            return Err(format!("block parser observation: {answer:?}").into());
        };
        if occurrences.len() != 2
            || occurrences[0].line != 2
            || occurrences[0].return_line.is_none()
            || occurrences[1].line != 5
            || occurrences[1].return_line.is_none()
            || outcome.owner.is_none()
            || !matches!(outcome.storage, FixtureStorage::Unavailable(_))
            || answer
                .gaps
                .iter()
                .any(|gap| gap.kind == GapKind::IncompleteObservation)
        {
            return Err(format!("block parser joins: {answer:?}").into());
        }
        Ok(())
    }
    .await;
    and_close(&mut result, &mut game).await;
    result
}

/// One initial reader invocation covers the fixed keys and both entry forms. Deferred
/// completion and runtime application are outside this observation.
async fn fixture_modifier_block(native: &Native) -> Outcome {
    use pdx_native::{
        DiagnosticCoverage, DiagnosticWindow, FieldMembers, FixtureFieldQuestion, FixtureParsing,
        FixtureRequest, GrammarProperty,
    };
    let text = r#"native_modifier_block = {
 modifier = {
  name = native_modifier_block
  data = 1
  icon = mod_country_resource_max_add
  icon_frame = 1
  custom_tooltip = native_modifier_tooltip
  show_only_custom_tooltip = no
  important = no
  hide_from_country_list = no
  key = native_modifier_block
  divide_over_pop_groups = no
  apply_modifier_to_other_planets = gave_up_pop
  description = native_modifier_description
  description_parameters = { }
  country_resource_max_add = 1
  pop_job_amenities_mult = 0.1
  gave_up_pop = 1
 }
}
"#;
    let fields = native.registry_fields(TRADITIONS)?;
    let field = fields
        .value
        .iter()
        .find(|field| field.name == "modifier")
        .ok_or("modifier missing")?;
    let FieldMembers::ModifierBlock(block) = &field.members else {
        return Err("modifier grammar missing".into());
    };
    let (GrammarProperty::Known(keys) | GrammarProperty::Partial(keys)) = &block.fixed_keys else {
        return Err("modifier keys unresolved".into());
    };
    for key in keys {
        if !text.contains(&format!("\n  {} =", key.name)) {
            return Err(format!("fixture omits reported key {}", key.name).into());
        }
    }
    let request = FixtureRequest::field_outcomes(
        "common/traditions/native_modifier_block.txt",
        text,
        [
            FixtureFieldQuestion::new(TRADITIONS, "native_modifier_block", "modifier")
                .with_parsing(),
        ],
    );
    let mut game = native.start_game(options().fixture(request)).await?;
    let mut result = async {
        let answer = game.observe_fixture().await?;
        let [outcome] = answer.value.field_outcomes.as_slice() else {
            return Err(format!("modifier outcomes: {answer:?}").into());
        };
        let FixtureParsing::Observed {
            occurrences,
            completeness: Completeness::Complete,
        } = &outcome.parsing
        else {
            return Err(format!("modifier parser observation: {answer:?}").into());
        };
        if occurrences.len() != 1
            || occurrences[0].return_line.is_none()
            || outcome.owner.is_none()
            || !answer.value.diagnostics.is_empty()
            || !matches!(
                answer.value.diagnostic_coverage,
                DiagnosticCoverage::Complete {
                    window: DiagnosticWindow::FixtureFileLoad
                }
            )
            || answer
                .gaps
                .iter()
                .any(|gap| gap.kind == GapKind::IncompleteObservation)
        {
            return Err(format!("modifier reader did not complete cleanly: {answer:?}").into());
        }
        Ok(())
    }
    .await;
    and_close(&mut result, &mut game).await;
    result
}

/// Check the engine-selected read scope at file load, before post-load validation.
async fn fixture_read_scope(native: &Native) -> Outcome {
    use pdx_native::{
        DiagnosticCoverage, DiagnosticJoin, DiagnosticWindow, FixtureFieldQuestion, FixtureParsing,
        FixtureRequest, GrammarProperty, ReadScope,
    };
    let file = "common/traditions/native_read_scope.txt";
    let samples = [
        (
            "native_scope_trigger",
            "potential",
            "is_planet_class = pc_barren",
        ),
        ("native_scope_effect", "on_enabled", "change_pc = pc_barren"),
    ];
    let fields = native.registry_fields(TRADITIONS)?;
    for (_, name, _) in samples {
        let field = fields
            .value
            .iter()
            .find(|field| field.name == name)
            .ok_or("missing scope field")?;
        if !matches!(&field.read_scope, GrammarProperty::Known(scopes)
            if matches!(scopes.as_slice(), [ReadScope::Types(types)] if types.len() == 1 && types[0].name == "country"))
        {
            return Err(format!(
                "{name}: unexpected static read scope {:?}",
                field.read_scope
            )
            .into());
        }
    }
    let text = samples
        .iter()
        .map(|(definition, field, child)| {
            format!("{definition} = {{\n {field} = {{\n  {child}\n }}\n}}\n")
        })
        .collect::<String>();
    let questions = samples.iter().map(|(definition, field, _)| {
        FixtureFieldQuestion::new(TRADITIONS, *definition, *field).with_parsing()
    });
    let request = FixtureRequest::field_outcomes(file, text, questions);
    let mut game = native.start_game(options().fixture(request)).await?;
    let mut result = async {
        let answer = game.observe_fixture().await?;
        if answer.value.diagnostic_coverage != (DiagnosticCoverage::Complete { window: DiagnosticWindow::FixtureFileLoad }) {
            return Err(format!("read scope diagnostic coverage: {:?}", answer.gaps).into());
        }
        for (index, (definition, field, _)) in samples.iter().enumerate() {
            let parsed = answer.value.field_outcomes.iter().any(|outcome| outcome.question.definition == *definition
                && outcome.question.field == *field
                && matches!(&outcome.parsing, FixtureParsing::Observed { completeness: Completeness::Complete, occurrences } if occurrences.len() == 1));
            let diagnosed = answer.value.diagnostics.iter().any(|diagnostic| diagnostic.stage == "engine-parser-log"
                && diagnostic.text.contains("Current Scope: country")
                && matches!(&diagnostic.join, DiagnosticJoin::Source { file: source, line, .. } if source == file && *line == ValidationSample::child_line(index)));
            if !parsed || !diagnosed { return Err(format!("{field}: parsed={parsed}, country diagnostic={diagnosed}: {:?}", answer.value.diagnostics).into()); }
        }
        Ok(())
    }.await;
    and_close(&mut result, &mut game).await;
    result
}

/// Validate every sample in one session, so the session pays for loading all content once.
async fn fixture_validation(native: &Native, field: &str, samples: &[ValidationSample]) -> Outcome {
    use pdx_native::{FixtureFieldQuestion, FixtureRequest};

    let file = format!("common/traditions/native_validation_{field}.txt");
    let text: String = samples
        .iter()
        .flat_map(|sample| sample.definition_lines(field))
        .map(|line| line + "\n")
        .collect();
    let questions = samples.iter().map(|sample| {
        FixtureFieldQuestion::new(TRADITIONS, sample.definition(), field).with_parsing()
    });
    let request = FixtureRequest::field_outcomes(&file, text, questions).through_validation();
    let mut game = native.start_game(options().fixture(request)).await?;
    let mut result = async {
        let answer = game.observe_fixture().await?;
        let failures = validation_failures(field, &file, samples, &answer);
        if failures.is_empty() {
            return Ok(());
        }

        let count = samples.len();
        Err(format!(
            "{} problems across {count} samples: {}",
            failures.len(),
            failures.join("; ")
        )
        .into())
    }
    .await;
    and_close(&mut result, &mut game).await;
    result
}

/// Each sample is checked alone. An accepted sample has no diagnostic on its lines, and a
/// rejected one has a diagnostic of its stage on its child's line. A diagnostic that no sample
/// owns is a failure too.
fn validation_failures(
    field: &str,
    file: &str,
    samples: &[ValidationSample],
    answer: &Answer<pdx_native::FixtureObservation>,
) -> Vec<String> {
    use pdx_native::{DiagnosticCoverage, DiagnosticJoin, DiagnosticWindow, FixtureParsing};

    let observation = &answer.value;
    if !matches!(
        observation.diagnostic_coverage,
        DiagnosticCoverage::Complete {
            window: DiagnosticWindow::FixtureFileLoadAndValidation
        }
    ) {
        return vec![format!("validation coverage: {answer:?}")];
    }

    let expected_family = if field == "potential" {
        pdx_native::BlockFamily::Trigger
    } else {
        pdx_native::BlockFamily::Effect
    };
    let source_line = |join: &DiagnosticJoin| match join {
        DiagnosticJoin::Source {
            file: source, line, ..
        } if source == file => Some(*line),
        _ => None,
    };
    let mut failures = Vec::new();

    for (index, sample) in samples.iter().enumerate() {
        let outcome = observation
            .field_outcomes
            .iter()
            .find(|outcome| outcome.question.definition == sample.definition());
        let parsed = outcome.is_some_and(|outcome| {
            matches!(&outcome.parsing,
                FixtureParsing::Observed { completeness: Completeness::Complete, occurrences }
                    if occurrences.len() == 1)
                && outcome.reader.family == expected_family
        });
        if !parsed {
            failures.push(format!("{}: parsing or reader: {outcome:?}", sample.name));
        }

        let Some(stage) = sample.stage else {
            continue;
        };
        let rejected = observation.diagnostics.iter().any(|diagnostic| {
            diagnostic.stage == stage
                && source_line(&diagnostic.join) == Some(ValidationSample::child_line(index))
                && (sample.name != "wrong_scope"
                    || diagnostic.text.contains("Current Scope: country"))
        });
        if !rejected {
            failures.push(format!(
                "{}: no {stage} diagnostic on its line",
                sample.name
            ));
        }
    }

    for diagnostic in &observation.diagnostics {
        let owner = source_line(&diagnostic.join)
            .and_then(ValidationSample::index_at)
            .and_then(|index| samples.get(index));
        match owner {
            Some(sample) if sample.stage.is_some() => {}
            Some(sample) => {
                failures.push(format!(
                    "{}: accepted sample has {diagnostic:?}",
                    sample.name
                ));
            }
            None => failures.push(format!("diagnostic owned by no sample: {diagnostic:?}")),
        }
    }

    failures
}

fn fixture_outcome_request(case: FixtureOutcomeCase) -> pdx_native::FixtureRequest {
    use pdx_native::{FixtureFieldQuestion, FixtureRequest};

    if matches!(case, FixtureOutcomeCase::SameOwner) {
        return FixtureRequest::field_outcomes(
            "common/traditions/native_fixture.txt",
            "native_fixture_tradition = {\n custom_tooltip = \"tip\"\n unlocks_agenda = \"agenda\"\n}\n",
            [
                FixtureFieldQuestion::new(TRADITIONS, "native_fixture_tradition", "custom_tooltip"),
                FixtureFieldQuestion::new(TRADITIONS, "native_fixture_tradition", "unlocks_agenda"),
            ],
        );
    }
    if matches!(case, FixtureOutcomeCase::MaximumQuestions) {
        return maximum_questions_request();
    }
    if matches!(case, FixtureOutcomeCase::UnrelatedDefinitions) {
        return unrelated_definitions_request();
    }
    let body = match case {
        FixtureOutcomeCase::Valid | FixtureOutcomeCase::DiagnosticsNotRequested => {
            " unlocks_agenda = \"agenda_one\"\n"
        }
        FixtureOutcomeCase::Omitted => "",
        FixtureOutcomeCase::Repeated => {
            " unlocks_agenda = \"agenda_one\"\n unlocks_agenda = \"agenda_two\"\n"
        }
        FixtureOutcomeCase::Malformed => " unlocks_agenda = \"agenda\nbroken\"\n",
        FixtureOutcomeCase::UnknownField => {
            " unlocks_agenda = \"agenda_one\"\n this_is_an_unknown_field = { broken = yes }\n"
        }
        FixtureOutcomeCase::SameOwner
        | FixtureOutcomeCase::MaximumQuestions
        | FixtureOutcomeCase::UnrelatedDefinitions => unreachable!(),
    };
    let mut question =
        FixtureFieldQuestion::new(TRADITIONS, "native_fixture_tradition", "unlocks_agenda");
    if matches!(case, FixtureOutcomeCase::DiagnosticsNotRequested) {
        question.diagnostics = false;
    }
    FixtureRequest::field_outcomes(
        "common/traditions/native_fixture.txt",
        format!("native_fixture_tradition = {{\n{body}}}\n"),
        [question],
    )
}

fn maximum_questions_request() -> pdx_native::FixtureRequest {
    use pdx_native::{FixtureFieldQuestion, FixtureRequest};

    let mut text = String::new();
    let mut questions = Vec::new();
    for index in 0..32 {
        let definition = format!("native_fixture_tradition_{index:02}");
        writeln!(text, "{definition} = {{").unwrap();
        writeln!(text, " unlocks_agenda = \"agenda_{index:02}\"").unwrap();
        writeln!(text, "}}").unwrap();
        let mut question = FixtureFieldQuestion::new(TRADITIONS, definition, "unlocks_agenda");
        question.diagnostics = false;
        questions.push(question);
    }
    FixtureRequest::field_outcomes("common/traditions/native_fixture.txt", text, questions)
}

fn unrelated_definitions_request() -> pdx_native::FixtureRequest {
    use pdx_native::{FixtureFieldQuestion, FixtureRequest};

    let mut text = String::new();
    for index in 0..257 {
        writeln!(text, "native_fixture_before_{index:03} = {{}}").unwrap();
    }
    for _ in 0..2 {
        writeln!(text, "native_fixture_repeat = {{}}").unwrap();
    }
    writeln!(text, "native_fixture_target = {{").unwrap();
    writeln!(text, " unlocks_agenda = \"target_agenda\"").unwrap();
    writeln!(text, "}}").unwrap();
    writeln!(
        text,
        "native_fixture_unrelated_diagnostic = {{ this_is_an_unknown_field = {{ broken = yes }} }}"
    )
    .unwrap();
    for index in 0..257 {
        writeln!(text, "native_fixture_after_{index:03} = {{}}").unwrap();
    }
    for _ in 0..2 {
        writeln!(text, "native_fixture_repeat = {{}}").unwrap();
    }
    FixtureRequest::field_outcomes(
        "common/traditions/native_fixture.txt",
        text,
        [FixtureFieldQuestion::new(
            TRADITIONS,
            "native_fixture_target",
            "unlocks_agenda",
        )],
    )
}

fn assert_fixture_outcome(
    case: FixtureOutcomeCase,
    answer: &Answer<pdx_native::FixtureObservation>,
) -> Outcome {
    use pdx_native::{DiagnosticCoverage, DiagnosticWindow};

    if answer.completeness != Completeness::Complete {
        return Err(format!("fixture completeness: {answer:?}").into());
    }
    let expected_coverage = match case {
        FixtureOutcomeCase::DiagnosticsNotRequested | FixtureOutcomeCase::MaximumQuestions => {
            DiagnosticCoverage::NotRequested
        }
        _ => DiagnosticCoverage::Complete {
            window: DiagnosticWindow::FixtureFileLoad,
        },
    };
    if answer.value.diagnostic_coverage != expected_coverage {
        return Err(format!("diagnostic coverage: {answer:?}").into());
    }

    match case {
        FixtureOutcomeCase::MaximumQuestions => assert_maximum_questions(answer),
        FixtureOutcomeCase::UnrelatedDefinitions => assert_unrelated_definitions(answer),
        FixtureOutcomeCase::SameOwner => assert_same_owner(answer),
        FixtureOutcomeCase::Valid
        | FixtureOutcomeCase::Omitted
        | FixtureOutcomeCase::Repeated
        | FixtureOutcomeCase::Malformed
        | FixtureOutcomeCase::UnknownField
        | FixtureOutcomeCase::DiagnosticsNotRequested => assert_agenda_outcome(case, answer),
    }
}

/// Checks two fields of one tradition, which share its owner and definition line.
fn assert_same_owner(answer: &Answer<pdx_native::FixtureObservation>) -> Outcome {
    let [tooltip, agenda] = answer.value.field_outcomes.as_slice() else {
        return Err(format!("same-owner outcomes: {answer:?}").into());
    };
    if tooltip.owner.is_none()
        || tooltip.owner != agenda.owner
        || tooltip.definition_line != Some(1)
        || agenda.definition_line != Some(1)
    {
        return Err(format!("same-owner identity: {answer:?}").into());
    }
    assert_string_storage(tooltip, &[(2, 1, "tip")], Some("tip"))?;
    assert_string_storage(agenda, &[(3, 1, "agenda")], Some("agenda"))
}

/// Checks the one `unlocks_agenda` question of the single-tradition cases: its identity, stored
/// values and parser diagnostics.
fn assert_agenda_outcome(
    case: FixtureOutcomeCase,
    answer: &Answer<pdx_native::FixtureObservation>,
) -> Outcome {
    use pdx_native::{DiagnosticJoin, ReaderKind};

    let outcome = answer
        .value
        .field_outcomes
        .first()
        .ok_or("missing field outcome")?;
    if outcome.question.field != "unlocks_agenda"
        || outcome.owner.is_none()
        || outcome.definition_line != Some(1)
        || outcome.reader.kind != ReaderKind::String
    {
        return Err(format!("field outcome identity: {answer:?}").into());
    }
    match case {
        FixtureOutcomeCase::Valid
        | FixtureOutcomeCase::DiagnosticsNotRequested
        | FixtureOutcomeCase::UnknownField => {
            assert_string_storage(outcome, &[(2, 1, "agenda_one")], Some("agenda_one"))?;
        }
        FixtureOutcomeCase::Omitted => assert_string_storage(outcome, &[], Some(""))?,
        FixtureOutcomeCase::Repeated => assert_string_storage(
            outcome,
            &[(2, 1, "agenda_one"), (3, 2, "agenda_two")],
            Some("agenda_two"),
        )?,
        FixtureOutcomeCase::Malformed => {
            assert_string_storage(
                outcome,
                &[(3, 1, "Unreadable String")],
                Some("Unreadable String"),
            )?;
        }
        FixtureOutcomeCase::SameOwner
        | FixtureOutcomeCase::MaximumQuestions
        | FixtureOutcomeCase::UnrelatedDefinitions => unreachable!(),
    }
    match case {
        FixtureOutcomeCase::Malformed => {
            let [diagnostic] = answer.value.diagnostics.as_slice() else {
                return Err(format!("malformed diagnostics: {answer:?}").into());
            };
            if diagnostic.text != "Malformed token"
                || diagnostic.stage != "reader-malformed-report"
                || outcome.diagnostics != [0]
                || !matches!(
                    &diagnostic.join,
                    DiagnosticJoin::Source { file, line: 3, definition: Some(definition),
                        field: Some(field), occurrence: Some(1) }
                        if file == "common/traditions/native_fixture.txt"
                            && definition == "native_fixture_tradition"
                            && field == "unlocks_agenda"
                )
            {
                return Err(format!("malformed diagnostic join: {answer:?}").into());
            }
        }
        FixtureOutcomeCase::UnknownField => {
            let [diagnostic] = answer.value.diagnostics.as_slice() else {
                return Err(format!("unexpected-field diagnostics: {answer:?}").into());
            };
            if diagnostic.text != "Unexpected token"
                || diagnostic.stage != "reader-unexpected-report"
                || !outcome.diagnostics.is_empty()
                || !matches!(
                    &diagnostic.join,
                    DiagnosticJoin::Source { file, line: 3, definition: None,
                        field: None, occurrence: None }
                        if file == "common/traditions/native_fixture.txt"
                )
            {
                return Err(format!("unexpected-field diagnostic join: {answer:?}").into());
            }
        }
        _ if !answer.value.diagnostics.is_empty() => {
            return Err(format!("unexpected parser diagnostics: {answer:?}").into());
        }
        _ => {}
    }
    Ok(())
}

fn assert_maximum_questions(answer: &Answer<pdx_native::FixtureObservation>) -> Outcome {
    use pdx_native::{DiagnosticCoverage, ReaderKind};

    if answer.completeness != Completeness::Complete
        || answer.value.diagnostic_coverage != DiagnosticCoverage::NotRequested
        || !answer.gaps.is_empty()
        || answer.value.field_outcomes.len() != 32
    {
        return Err(format!("maximum field questions: {answer:?}").into());
    }
    let expected_reader = answer.value.field_outcomes[0].reader.clone();
    if expected_reader.kind != ReaderKind::String || expected_reader.id.is_none() {
        return Err(format!("maximum field reader: {answer:?}").into());
    }
    for (index, outcome) in answer.value.field_outcomes.iter().enumerate() {
        let definition = format!("native_fixture_tradition_{index:02}");
        let value = format!("agenda_{index:02}");
        let definition_line = index as u64 * 3 + 1;
        let field_line = definition_line + 1;
        if outcome.question.definition != definition
            || outcome.question.field != "unlocks_agenda"
            || outcome.owner.is_none()
            || outcome.definition_line != Some(definition_line)
            || outcome.reader != expected_reader
        {
            return Err(format!("maximum field outcome {index}: {answer:?}").into());
        }
        assert_string_storage(outcome, &[(field_line, 1, &value)], Some(&value))?;
    }
    Ok(())
}

fn assert_unrelated_definitions(answer: &Answer<pdx_native::FixtureObservation>) -> Outcome {
    use pdx_native::{DiagnosticCoverage, DiagnosticJoin, DiagnosticWindow, ReaderKind};

    let [outcome] = answer.value.field_outcomes.as_slice() else {
        return Err(format!("unrelated definition outcome: {answer:?}").into());
    };
    let [diagnostic] = answer.value.diagnostics.as_slice() else {
        return Err(format!("unrelated definition diagnostic: {answer:?}").into());
    };
    if answer.completeness != Completeness::Complete
        || answer.value.diagnostic_coverage
            != (DiagnosticCoverage::Complete {
                window: DiagnosticWindow::FixtureFileLoad,
            })
        || !answer.gaps.is_empty()
        || outcome.question.definition != "native_fixture_target"
        || outcome.owner.is_none()
        || outcome.definition_line != Some(260)
        || outcome.reader.kind != ReaderKind::String
        || outcome.reader.id.is_none()
        || !outcome.diagnostics.is_empty()
        || diagnostic.text != "Unexpected token"
        || diagnostic.stage != "reader-unexpected-report"
        || !matches!(
            &diagnostic.join,
            DiagnosticJoin::Source {
                file,
                line: 263,
                definition: None,
                field: None,
                occurrence: None,
            } if file == "common/traditions/native_fixture.txt"
        )
    {
        return Err(format!("unrelated definitions: {answer:?}").into());
    }
    assert_string_storage(outcome, &[(261, 1, "target_agenda")], Some("target_agenda"))
}

fn assert_string_storage(
    outcome: &pdx_native::FixtureFieldOutcome,
    expected: &[(u64, u64, &str)],
    expected_final: Option<&str>,
) -> Outcome {
    let pdx_native::FixtureStorage::Observed {
        occurrences,
        final_value,
        completeness,
    } = &outcome.storage
    else {
        return Err(format!("field storage unavailable: {outcome:?}").into());
    };
    let actual = occurrences
        .iter()
        .map(|occurrence| {
            (
                occurrence.line,
                occurrence.occurrence,
                occurrence.value.clone(),
            )
        })
        .collect::<Vec<_>>();
    let expected: Vec<_> = expected
        .iter()
        .map(|(line, ordinal, value)| {
            (
                *line,
                *ordinal,
                pdx_native::FixtureValue::String((*value).into()),
            )
        })
        .collect();
    if actual != expected
        || *final_value
            != expected_final.map(|value| pdx_native::FixtureValue::String(value.into()))
        || *completeness != Completeness::Complete
    {
        return Err(format!("field storage values: {outcome:?}").into());
    }
    Ok(())
}

async fn assert_recorded_fixture(
    directory: &std::path::Path,
    request: pdx_native::FixtureRequest,
    answer: &Answer<pdx_native::FixtureObservation>,
) -> Outcome {
    let recorded_native = Native::from_recorded_answers(directory)?;
    let mut recorded_game = recorded_native
        .start_game(GameOptions::new(Command::new("must-not-start")).fixture(request))
        .await?;
    let mut expected = answer.clone();
    expected.source.basis = Basis::Recorded;
    if recorded_game.observe_fixture().await? != expected
        || recorded_game.close().await? != Disposal::NotApplicable
    {
        return Err("recorded field outcome differs".into());
    }
    Ok(())
}

/// Every live session keeps its work directory; the harness removes it when the case passes.
fn options() -> GameOptions {
    let mut supervisor = Command::new(std::env::current_exe().expect("test executable path"));
    supervisor.arg("--supervisor");
    GameOptions::new(supervisor).keep_work_directory()
}

fn fixture_request() -> pdx_native::FixtureRequest {
    pdx_native::FixtureRequest::field_outcomes(
        "common/tradition_categories/atlas.txt",
        "atlas_early_category = {\n tree_template = \"atlas_early_template\"\n traditions = { }\n}\n",
        ["tree_template", "traditions"].map(|field| {
            let mut question =
                pdx_native::FixtureFieldQuestion::new(CATEGORIES, "atlas_early_category", field)
                    .with_parsing();
            question.diagnostics = false;
            question
        }),
    )
}

async fn fixture_case(control: Fault) -> Outcome {
    use pdx_native::{Operation, Support};
    let recorded = tempfile::tempdir()?;
    let native = Native::open(std::env::var_os("STELLARIS_PATH").unwrap())?
        .record_answers_to(recorded.path());
    if native.supports(Operation::ObserveFixture) != Support::Supported {
        return Err("fixture operation is not supported".into());
    }
    let request = fixture_request();
    let mut prepared = options().fixture(request.clone());
    if control != Fault::Normal {
        prepared = prepared.fault(ObservationTarget::Fixture, control);
    }
    let started = native.start_game(prepared).await;
    if control == Fault::WorkerLoss {
        return match started {
            Err(Error::Startup {
                disposal: Disposal::Confirmed,
                reason,
            }) if reason.contains("WorkerLost") => Ok(()),
            Ok(mut game) => {
                let _ = game.close().await;
                Err("fixture worker loss unexpectedly started a session".into())
            }
            Err(error) => Err(format!("fixture worker loss: {error:?}").into()),
        };
    }
    let mut game = started?;
    let mut result = async {
        let first = game.observe_fixture().await;
        assert_first_fixture_result(control, &first)?;
        for _ in 0..2 {
            if game.observe_fixture().await != first {
                return Err("fixture answer changed after repeated query".into());
            }
        }
        assert_fixture_replay(recorded.path(), &request, first).await?;
        Ok(())
    }
    .await;
    and_close(&mut result, &mut game).await;
    if result.is_ok() && !matches!(game.observe_fixture().await, Err(Error::Closed)) {
        return Err("closed fixture session still answered".into());
    }
    result
}

/// Check category source joins and parser returns independently of storage support.
fn assert_first_fixture_result(
    control: Fault,
    first: &Result<Answer<pdx_native::FixtureObservation>, Error>,
) -> Outcome {
    use pdx_native::{FixtureParsing, Operation};
    match (control, first) {
        (
            Fault::MissingHook | Fault::LateHook,
            Err(Error::Observation {
                operation: Operation::ObserveFixture,
                ..
            }),
        ) => {}
        (Fault::DroppedRecord | Fault::MissingTerminal | Fault::AccessFailure, Ok(answer)) => {
            if answer.completeness != Completeness::Partial
                || !answer
                    .gaps
                    .iter()
                    .any(|gap| gap.kind == GapKind::IncompleteObservation)
            {
                return Err(
                    format!("{control:?}: expected partial fixture answer: {answer:?}").into(),
                );
            }
        }
        (Fault::Normal, Ok(answer)) => {
            let outcomes = &answer.value.field_outcomes;
            if answer.source.basis != Basis::LiveObservation
                || outcomes.len() != 2
                || outcomes[0].owner.is_none()
                || outcomes[0].owner != outcomes[1].owner
            {
                return Err(format!("category owner join: {answer:?}").into());
            }
            for (field, line) in [("tree_template", 2), ("traditions", 3)] {
                let outcome = outcomes
                    .iter()
                    .find(|outcome| outcome.question.field == field)
                    .ok_or("category field outcome missing")?;
                if outcome.file != "common/tradition_categories/atlas.txt"
                    || outcome.question.definition != "atlas_early_category"
                    || outcome.definition_line != Some(1)
                {
                    return Err(format!("category source join: {outcome:?}").into());
                }
                match &outcome.parsing {
                    FixtureParsing::Observed {
                        occurrences,
                        completeness: Completeness::Complete,
                    } if occurrences.len() == 1
                        && occurrences[0].line == line
                        && occurrences[0].occurrence == 1
                        && occurrences[0].return_line.is_some() => {}
                    other => {
                        return Err(format!("category parser did not return: {other:?}").into());
                    }
                }
            }
        }
        (_, answer) => {
            return Err(format!("{control:?}: unexpected fixture result: {answer:?}").into());
        }
    }
    if let Ok(answer) = first
        && answer.value.diagnostic_coverage != pdx_native::DiagnosticCoverage::NotRequested
    {
        return Err(format!("unrequested diagnostic coverage: {answer:?}").into());
    }
    Ok(())
}

/// Replays the recorded fixture result without a game, and checks that a changed fixture file
/// has no recorded answer.
async fn assert_fixture_replay(
    directory: &std::path::Path,
    request: &pdx_native::FixtureRequest,
    first: Result<Answer<pdx_native::FixtureObservation>, Error>,
) -> Outcome {
    let recorded_native = Native::from_recorded_answers(directory)?;
    let mut recorded_game = recorded_native
        .start_game(GameOptions::new(Command::new("must-not-start")).fixture(request.clone()))
        .await?;
    let expected = first.map(|mut answer| {
        answer.source.basis = Basis::Recorded;
        answer
    });
    if recorded_game.observe_fixture().await != expected {
        return Err("recorded fixture differs from live answer".into());
    }
    if recorded_game.close().await? != Disposal::NotApplicable {
        return Err("recorded fixture started a game".into());
    }
    let mut changed = request.clone();
    changed.files.values_mut().next().unwrap().push('\n');
    let mut absent = recorded_native
        .start_game(GameOptions::new(Command::new("must-not-start")).fixture(changed))
        .await?;
    if !matches!(
        absent.observe_fixture().await,
        Err(Error::NotRecorded { .. })
    ) {
        return Err("different fixture used another file's answer".into());
    }
    absent.close().await?;
    Ok(())
}

async fn fixture_timeout(native: &Native) -> Outcome {
    let mut options = options().fixture(fixture_request());
    options.startup_seconds = 1;
    match native.start_game(options).await {
        Err(Error::Startup {
            disposal: Disposal::Confirmed,
            reason,
        }) if reason.contains("TimedOut") => Ok(()),
        Ok(mut game) => {
            let _ = game.close().await;
            Err("fixture startup deadline was not enforced".into())
        }
        Err(error) => Err(format!("fixture timeout: {error:?}").into()),
    }
}

async fn fixture_refusal(native: &Native) -> Outcome {
    let before = work_directories()?;
    let mut request = fixture_request();
    request.files = [("../fixture.txt".into(), "x = {}".into())].into();
    match native.start_game(options().fixture(request)).await {
        Err(Error::FixtureRequest { .. }) => {}
        Ok(mut game) => {
            let _ = game.close().await;
            return Err("unsupported fixture launched".into());
        }
        Err(error) => return Err(format!("fixture refusal: {error:?}").into()),
    }
    if work_directories()? != before {
        return Err("refused fixture created session state".into());
    }
    Ok(())
}

/// An unrelated owned child must survive Native's cleanup, and the ordinary game profile must
/// remain byte-for-byte unchanged. The sentinel is reaped before the supervisor-leak check.
struct Isolation {
    sentinel: std::process::Child,
    profile: std::collections::BTreeMap<std::path::PathBuf, String>,
}
impl Isolation {
    fn begin() -> Result<Self, Box<dyn std::error::Error>> {
        let profile = profile_snapshot()?;
        let sentinel = Command::new("/bin/sleep").arg("3600").spawn()?;
        Ok(Self { sentinel, profile })
    }
    fn finish(mut self) -> Outcome {
        if self.sentinel.try_wait()?.is_some() {
            return Err("Native stopped an unrelated process".into());
        }
        if profile_snapshot()? != self.profile {
            return Err("Native changed the ordinary profile".into());
        }
        Ok(())
    }
}
impl Drop for Isolation {
    fn drop(&mut self) {
        let _ = self.sentinel.kill();
        let _ = self.sentinel.wait();
    }
}

fn profile_snapshot()
-> Result<std::collections::BTreeMap<std::path::PathBuf, String>, Box<dyn std::error::Error>> {
    use sha2::{Digest, Sha256};
    use std::{
        collections::BTreeMap,
        fs,
        io::Read,
        path::{Path, PathBuf},
    };
    fn visit(path: &Path, snapshot: &mut BTreeMap<PathBuf, String>) -> std::io::Result<()> {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        let value = if metadata.is_symlink() {
            format!("link:{:?}", fs::read_link(path)?)
        } else if metadata.is_dir() {
            for child in fs::read_dir(path)? {
                visit(&child?.path(), snapshot)?;
            }
            "directory".into()
        } else {
            let mut digest = Sha256::new();
            let mut file = fs::File::open(path)?;
            let mut buffer = [0; 65536];
            loop {
                let read = file.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                digest.update(&buffer[..read]);
            }
            format!("{:x}", digest.finalize())
        };
        snapshot.insert(
            path.into(),
            format!(
                "{value}:{:?}:{:?}",
                metadata.permissions(),
                metadata.modified()?
            ),
        );
        Ok(())
    }
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME is missing")?);
    let mut snapshot = BTreeMap::new();
    for relative in [
        "Documents/Paradox Interactive/Stellaris",
        "Library/Application Support/Paradox Interactive/Stellaris",
    ] {
        visit(&home.join(relative), &mut snapshot)?;
    }
    Ok(snapshot)
}

/// The item count of a complete live answer.
fn complete(answer: &Answer<Vec<String>>, registry: &str) -> Result<usize, String> {
    if answer.completeness != Completeness::Complete || !answer.gaps.is_empty() {
        return Err(format!(
            "{registry}: expected a complete answer: {:?}",
            answer.gaps
        ));
    }
    if answer.source.basis != Basis::LiveObservation {
        return Err(format!("{registry}: basis {:?}", answer.source.basis));
    }
    Ok(answer.value.len())
}

/// Compare a few live key layouts with independent top-level keys in the selected game files.
fn source_keys_match(keys: &[String], registry: &str) -> Outcome {
    let root = std::path::PathBuf::from(std::env::var_os("STELLARIS_PATH").unwrap());
    let mut source = BTreeSet::new();
    for entry in std::fs::read_dir(root.join(registry))? {
        let path = entry?.path();
        if path.extension().is_none_or(|extension| extension != "txt") {
            continue;
        }
        let text = std::fs::read_to_string(path)?;
        source.extend(text.lines().filter_map(source_key).map(str::to_owned));
    }
    let observed: BTreeSet<_> = keys.iter().cloned().collect();
    if observed != source {
        return Err(format!("{registry}: live keys differ from top-level source keys").into());
    }
    Ok(())
}

/// The top-level key that a source line assigns, if its name uses only lowercase letters, digits
/// and underscores. An indented or commented line assigns none.
fn source_key(line: &str) -> Option<&str> {
    if line.starts_with([' ', '\t', '#']) {
        return None;
    }

    let (key, _) = line.split_once('=')?;
    let key = key.trim();
    let is_plain_name = !key.is_empty()
        && key
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');

    is_plain_name.then_some(key)
}

async fn close_confirmed(game: &mut Game) -> Outcome {
    let disposal = game.close().await?;
    if disposal != Disposal::Confirmed {
        return Err(format!("disposal: {disposal:?}").into());
    }
    // A later check of this case may still fail, so its diagnostics must outlive the close.
    if !game.work_directory().is_some_and(std::path::Path::exists) {
        return Err("a confirmed close removed the kept work directory".into());
    }
    // A repeated close gives the same result, and a closed game answers nothing.
    if game.close().await? != Disposal::Confirmed {
        return Err("the second close gave another disposal".into());
    }
    match game.observe_fixture().await {
        Err(Error::Closed) => Ok(()),
        other => Err(format!("after close: {other:?}").into()),
    }
}

/// The loaded tags of the five declared names that content registers again (M451-hotfix).
const RE_REGISTERED: [(&str, &[&str]); 5] = [
    ("terraforming_cost_mult", &["Planets", "AI Economy"]),
    (
        "starbase_shipyard_build_cost_mult",
        &["Starbases", "AI Economy"],
    ),
    (
        "starbase_shipyard_artificial_build_cost_mult",
        &SHIP_TAGS_WITH_ECONOMY,
    ),
    (
        "starbase_shipyard_space_fauna_build_cost_mult",
        &SHIP_TAGS_WITH_ECONOMY,
    ),
    ("gdf_ship_alloys_cost_mult", &SHIP_TAGS_WITH_ECONOMY),
];
const SHIP_TAGS_WITH_ECONOMY: [&str; 7] = [
    "Orbital Stations",
    "Space Stations",
    "Military Ships",
    "Civilian Ships",
    "Science Ships",
    "Transport Ships",
    "AI Economy",
];
/// Entries of the loaded table on M451-hotfix with installed content (45,578, as on M45-release in SDK-488).
const LOADED_MODIFIERS: usize = 45_578;

async fn loaded_modifiers(native: &Native) -> Outcome {
    use pdx_native::{DeclaredTags, LoadedContent};
    let declared = native.modifiers()?.value;
    let earlier_work = work_directories()?;
    let mut game = native.start_game(options().loaded_modifiers()).await?;
    let mut result = async {
        let answer = game.loaded_modifiers().await?;
        if answer.source.basis != Basis::LiveObservation
            || answer.value.content != LoadedContent::Installation
        {
            return Err(format!("source or content: {:?}", answer.source).into());
        }
        let loaded = &answer.value.modifiers;
        if loaded.len() != LOADED_MODIFIERS {
            return Err(format!("{} loaded modifiers", loaded.len()).into());
        }
        let by_name: std::collections::BTreeMap<_, _> = loaded
            .iter()
            .map(|modifier| (modifier.name.as_str(), modifier))
            .collect();
        for declaration in &declared {
            if !by_name
                .get(declaration.name.as_str())
                .is_some_and(|modifier| modifier.declared)
            {
                return Err(format!("{} is not loaded as declared", declaration.name).into());
            }
        }
        if loaded.iter().filter(|modifier| modifier.declared).count() != declared.len() {
            return Err("a loaded name is marked declared without a declaration".into());
        }
        for (name, tags) in RE_REGISTERED {
            let expected = DeclaredTags::Listed(tags.iter().map(|tag| tag.to_string()).collect());
            let static_tags = &declared
                .iter()
                .find(|declaration| declaration.name == name)
                .ok_or(name)?
                .category_tags;
            if by_name[name].category_tags != expected || *static_tags == expected {
                return Err(format!("{name}: loaded {:?}", by_name[name].category_tags).into());
            }
        }
        let capital = by_name
            .get("planet_building_capital_build_speed_mult")
            .ok_or("no building family name")?;
        if capital.declared
            || !capital.generated_by.iter().any(|generated| {
                generated.registry == "common/buildings" && generated.item == "building_capital"
            })
        {
            return Err(format!("building family: {capital:?}").into());
        }
        // SDK-566: families that item post-read code and shared helpers register.
        for (name, registry, item) in [
            ("job_miner_add", "common/pop_jobs", "miner"),
            (
                "district_mining_max_add",
                "common/districts",
                "district_mining",
            ),
            (
                "category_computing_research_speed_mult",
                "common/technology/category",
                "computing",
            ),
        ] {
            let modifier = by_name.get(name).ok_or(name)?;
            if modifier.declared
                || !modifier
                    .generated_by
                    .iter()
                    .any(|generated| generated.registry == registry && generated.item == item)
            {
                return Err(format!("{name}: {modifier:?}").into());
            }
        }
        engine_log_agrees(loaded, &earlier_work)?;
        let generated = loaded
            .iter()
            .filter(|modifier| !modifier.generated_by.is_empty())
            .count();
        let unexplained = loaded
            .iter()
            .filter(|modifier| !modifier.declared && modifier.generated_by.is_empty())
            .count();
        println!(
            "  loaded {}, declared {}, generated {generated}, unexplained {unexplained}; gaps {:?}",
            loaded.len(),
            declared.len(),
            answer.gaps
        );
        if game.loaded_modifiers().await? != answer {
            return Err("a repeated read gave another answer".into());
        }
        Ok(())
    }
    .await;
    and_close(&mut result, &mut game).await;
    result
}

/// Compare every name and tag list with the modifier documentation that the engine itself wrote
/// in the session's private profile. Only this test reads the log.
fn engine_log_agrees(
    loaded: &[pdx_native::LoadedModifier],
    earlier_work: &BTreeSet<std::path::PathBuf>,
) -> Outcome {
    let logs: Vec<_> = work_directories()?
        .difference(earlier_work)
        .map(|work| work.join("session/profile/logs/script_documentation/modifiers.log"))
        .filter(|path| path.exists())
        .collect();
    let [log] = logs.as_slice() else {
        return Err(format!("expected one engine modifier log: {logs:?}").into());
    };
    let text = std::fs::read_to_string(log)?;
    let logged: Vec<(&str, Vec<&str>)> = text
        .lines()
        .filter_map(|line| line.strip_prefix("- "))
        .filter_map(|line| line.split_once(", Category: "))
        .map(|(name, tags)| {
            (
                name,
                tags.split(", ").filter(|tag| !tag.is_empty()).collect(),
            )
        })
        .collect();
    if logged.len() != loaded.len() {
        return Err(format!("engine log has {} entries", logged.len()).into());
    }
    for ((name, tags), modifier) in logged.iter().zip(loaded) {
        let pdx_native::DeclaredTags::Listed(loaded_tags) = &modifier.category_tags else {
            return Err(format!("{}: unresolved tags", modifier.name).into());
        };
        if *name != modifier.name || *tags != *loaded_tags {
            return Err(format!("{name} {tags:?} differs from {modifier:?}").into());
        }
    }
    Ok(())
}

/// The only selected registry has no hook; the modifier hook alone still owns the pause.
async fn loaded_modifiers_missing_registry_hook(native: &Native) -> Outcome {
    let mut game = native
        .start_game(
            options()
                .fault(
                    ObservationTarget::Registry(TRADITIONS.into()),
                    Fault::MissingHook,
                )
                .loaded_modifiers(),
        )
        .await?;
    let mut result = async {
        let loaded = game.loaded_modifiers().await?;
        if loaded.value.modifiers.len() != LOADED_MODIFIERS {
            return Err(format!("{} loaded modifiers", loaded.value.modifiers.len()).into());
        }
        if !loaded.value.registry_items.contains_key("common/buildings") {
            return Err("modifier keys were lost with an unrelated loader hook".into());
        }
        Ok(())
    }
    .await;
    and_close(&mut result, &mut game).await;
    result
}

async fn loaded_modifiers_worker_loss(native: &Native) -> Outcome {
    match native
        .start_game(
            options()
                .loaded_modifiers()
                .fault(ObservationTarget::Modifiers, Fault::WorkerLoss),
        )
        .await
    {
        Err(Error::Startup {
            disposal: Disposal::Confirmed,
            reason,
        }) if reason.contains("WorkerLost") => Ok(()),
        Err(error) => Err(format!("expected a worker-loss startup error: {error:?}").into()),
        Ok(mut game) => {
            let _ = game.close().await;
            Err("the game started although its worker was lost".into())
        }
    }
}

/// Always close, so that a failed check leaves no game. The first failure is the one reported.
async fn and_close(result: &mut Outcome, game: &mut Game) {
    let closed = close_confirmed(game).await;
    if result.is_ok() {
        *result = closed;
    }
}

async fn fault(
    native: &Native,
    registry: &'static str,
    _other: &'static str,
    control: Fault,
    expect: Expect,
) -> Outcome {
    let result = pdx_native::internals::check_registry_load(
        native,
        options().fault(ObservationTarget::Registry(registry.into()), control),
        registry,
    )
    .await;
    match (expect, result) {
        (
            Expect::NoAnswer,
            Err(Error::Startup {
                disposal: Disposal::Confirmed,
                reason,
            }),
        ) if matches!(control, Fault::MissingHook | Fault::LateHook)
            && reason.contains("WorkerLost") =>
        {
            Ok(())
        }
        (Expect::NoAnswer, Err(Error::Observation { .. })) if control == Fault::AccessFailure => {
            Ok(())
        }
        (Expect::PartialAnswer, Ok(answer))
            if answer.completeness == Completeness::Partial
                && answer
                    .gaps
                    .iter()
                    .any(|gap| gap.kind == GapKind::IncompleteObservation) =>
        {
            Ok(())
        }
        (_, answer) => Err(format!("{registry} with {control:?}: {answer:?}").into()),
    }
}

/// The supervisor stops the debugger worker while the faulted registry loads. The game never
/// reaches its pause, so there is no `Game`; the start fails and the game is still reaped.
async fn worker_loss(native: &Native, registry: &'static str, control: Fault) -> Outcome {
    match native
        .start_game(options().fault(ObservationTarget::Registry(registry.into()), control))
        .await
    {
        Err(Error::Startup {
            disposal: Disposal::Confirmed,
            reason,
        }) if reason.contains("WorkerLost") => Ok(()),
        Err(error) => Err(format!("expected a worker-loss startup error: {error:?}").into()),
        Ok(mut game) => {
            let _ = game.close().await;
            Err("the game started although its worker was lost".into())
        }
    }
}

async fn startup_timeout(native: &Native) -> Outcome {
    let mut options = options();
    options.startup_seconds = 1;
    match native.start_game(options).await {
        Err(Error::Startup {
            disposal: Disposal::Confirmed,
            reason,
        }) if reason.contains("TimedOut") => Ok(()),
        Err(error) => Err(format!("expected a startup timeout: {error:?}").into()),
        Ok(mut game) => {
            let _ = game.close().await;
            Err("the game started within one second".into())
        }
    }
}

/// The caller forgets to close. The supervisor sees its control input end and reaps the game.
async fn drop_without_close(native: &Native) -> Outcome {
    let game = native.start_game(options()).await?;
    if game_processes(&processes()?).is_empty() {
        return Err("no game process after start".into());
    }
    drop(game);
    Ok(())
}

/// Wait until every game process that started after `before`, every child of this process, and
/// every process that a session of this case recorded as its game or in its worker's process
/// group is gone. A worker or `debugserver` that outlives its supervisor is no longer a child of
/// this process. This only looks; it never signals a process.
fn processes_are_gone(
    before: &BTreeSet<u32>,
    earlier_work: &BTreeSet<std::path::PathBuf>,
) -> Outcome {
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        // The supervisor writes the game's identity when its session ends, so read it each time.
        let owned = owned_identities(earlier_work)?;
        let inventory = processes()?;
        let games: Vec<_> = game_processes(&inventory)
            .difference(before)
            .copied()
            .collect();
        let children = child_processes(&inventory);
        let sessions = owned_processes(&inventory, &owned);
        if games.is_empty() && children.is_empty() && sessions.is_empty() {
            break;
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "still running: games {games:?}, children {children:?}, session processes {sessions:?}"
            )
            .into());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Ok(())
}

/// The game PIDs and worker process groups that the sessions of a case recorded in their work
/// directories. A session that did not reach its game or its worker has no record of it.
#[derive(Default)]
struct OwnedIdentities {
    games: BTreeSet<u32>,
    worker_groups: BTreeSet<u32>,
}

/// Read `owner.json` and `worker-owned.json` of each work directory made after `earlier`.
fn owned_identities(
    earlier: &BTreeSet<std::path::PathBuf>,
) -> Result<OwnedIdentities, Box<dyn std::error::Error>> {
    let mut owned = OwnedIdentities::default();
    for work in work_directories()?.difference(earlier) {
        let session = work.join("session");
        if let Some(owner) = optional_json(&session.join("owner.json"))? {
            match &owner["game"] {
                serde_json::Value::Null => {}
                game => {
                    owned
                        .games
                        .insert(identity_pid(game).ok_or("owner.json: invalid game")?);
                }
            }
        }
        if let Some(worker) = optional_json(&session.join("worker-owned.json"))? {
            owned
                .worker_groups
                .insert(identity_pid(&worker).ok_or("worker-owned.json: invalid pid")?);
        }
    }
    Ok(owned)
}

fn identity_pid(identity: &serde_json::Value) -> Option<u32> {
    identity["pid"].as_u64()?.try_into().ok()
}

/// The JSON file at `path`, or `None` when the session did not write it.
fn optional_json(
    path: &std::path::Path,
) -> Result<Option<serde_json::Value>, Box<dyn std::error::Error>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(
            serde_json::from_slice(&bytes)
                .map_err(|error| format!("{}: {error}", path.display()))?,
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("{}: {error}", path.display()).into()),
    }
}

/// The processes that are a recorded game, or members of a recorded worker's process group. The
/// worker leads its own group, and the `debugserver` launcher joins it.
fn owned_processes(inventory: &[Process], owned: &OwnedIdentities) -> Vec<u32> {
    inventory
        .iter()
        .filter(|process| {
            owned.games.contains(&process.pid) || owned.worker_groups.contains(&process.group)
        })
        .map(|process| process.pid)
        .collect()
}

/// Every live session keeps its work directory, and Native also keeps it after a failed start or
/// a drop. A case that passed needs no inspection, so remove what it left.
fn remove_work_directories(earlier: &BTreeSet<std::path::PathBuf>) -> Outcome {
    for path in work_directories()?.difference(earlier) {
        std::fs::remove_dir_all(path)?;
    }
    Ok(())
}

/// Read only the summaries created by this case, after its session has closed.
fn diagnostic_summaries(
    earlier: &BTreeSet<std::path::PathBuf>,
) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
    let current = work_directories()?;
    current
        .difference(earlier)
        .map(|work| {
            let bytes = std::fs::read(work.join("session/run-summary.json"))?;
            Ok(serde_json::from_slice(&bytes)?)
        })
        .collect()
}

/// A held worker has no active call deadline, even after successful command checks.
fn check_held_diagnostics(earlier: &BTreeSet<std::path::PathBuf>) -> Outcome {
    let summaries = diagnostic_summaries(earlier)?;
    let [summary] = summaries.as_slice() else {
        return Err("expected one completed session summary".into());
    };
    let context = &summary["worker_diagnostics"]["context"];
    if context["phase"] != "held"
        || !context["deadline_milliseconds"].is_null()
        || !context["failure"].is_null()
    {
        return Err(
            format!("held worker retained an active deadline or failure: {context}").into(),
        );
    }
    Ok(())
}

/// Name each work directory that a failed case kept, with its run summary's outcome and last
/// completed phase, or say that it has none.
fn kept_work_directories(earlier: &BTreeSet<std::path::PathBuf>) -> Vec<String> {
    let current = match work_directories() {
        Ok(current) => current,
        Err(error) => return vec![format!("work directories unknown: {error}")],
    };

    current
        .difference(earlier)
        .map(|work| {
            let path = work.join("session/run-summary.json");
            let summary = std::fs::read(&path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
            let Some(summary) = summary else {
                return format!("kept {}; no run summary", work.display());
            };

            format!(
                "kept {}; run summary {}: outcome {}, last completed phase {}, reason {}",
                work.display(),
                path.display(),
                summary["outcome"],
                summary["timing"]["last_completed_phase"],
                summary["worker_diagnostics"]["reason"],
            )
        })
        .collect()
}

/// A case that fails after a confirmed close keeps its work directories and names each run
/// summary, and a later passing case removes only its own directory. Uses stand-in directories
/// with this process's prefix; starts no game.
fn failed_cases_keep_their_work_directories() -> Outcome {
    // A unique run keeps stale stand-ins of an earlier process with this number out of the check.
    let run = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let make = |name: &str| -> std::io::Result<std::path::PathBuf> {
        let work = std::env::temp_dir().join(format!(
            "pdx-native-{}-retention-{run}-{name}",
            std::process::id()
        ));
        std::fs::create_dir_all(work.join("session"))?;
        Ok(work)
    };
    let before = work_directories()?;
    let summarized = make("summarized")?;
    let summary = summarized.join("session/run-summary.json");
    std::fs::write(
        &summary,
        r#"{"outcome":"Completed","timing":{"last_completed_phase":"paused"}}"#,
    )?;
    let unsummarized = make("unsummarized")?;
    let report = kept_work_directories(&before);

    let after_failure = work_directories()?;
    let passed = make("passed")?;
    let removal = remove_work_directories(&after_failure);
    let earlier_kept = summary.exists() && unsummarized.exists();
    let own_removed = !passed.exists();
    for work in [&summarized, &unsummarized, &passed] {
        if work.exists() {
            std::fs::remove_dir_all(work)?;
        }
    }
    removal?;

    let expected = [
        format!(
            "kept {}; run summary {}: outcome \"Completed\", last completed phase \"paused\", reason null",
            summarized.display(),
            summary.display()
        ),
        format!("kept {}; no run summary", unsummarized.display()),
    ];
    if report != expected {
        return Err(format!("failed case report: {report:?}").into());
    }
    if !earlier_kept {
        return Err("a later passing case removed an earlier failed case's directory".into());
    }
    if !own_removed {
        return Err("a passing case kept its own work directory".into());
    }
    Ok(())
}

/// A failed or incomplete process inventory fails the gate, and a process left in a recorded
/// worker's group, or recorded as the game, is found after its parent has exited and it is no
/// longer a child of this process. Uses a stand-in work directory and a compiled stub, because
/// macOS kills a copied Apple binary; starts no game.
fn cleanup_gate_checks() -> Outcome {
    use std::os::unix::process::{CommandExt, ExitStatusExt};
    let ps = |status: i32, stdout: &str| std::process::Output {
        status: std::process::ExitStatus::from_raw(status),
        stdout: stdout.into(),
        stderr: Vec::new(),
    };
    if process_inventory(&ps(1 << 8, "    1     0     1 /sbin/launchd\n")).is_ok() {
        return Err("a failed process inventory was accepted".into());
    }
    for row in [
        "    1     0 /sbin/launchd",
        "    1     0     x /sbin/launchd",
        "    1     0     1",
    ] {
        if process_inventory(&ps(0, &format!("{row}\n"))).is_ok() {
            return Err(format!("a malformed inventory row was accepted: {row:?}").into());
        }
    }
    // The supervisor's own platform is macOS; elsewhere only the inventory rules apply.
    if !cfg!(target_os = "macos") {
        return Ok(());
    }

    let tools = tempfile::tempdir()?;
    let source = tools.path().join("stub.c");
    let stub = tools.path().join("stub");
    // The child closes its copy of the output, so that reading its PID does not wait for it. It
    // ends itself after a minute if this check stops before it kills the child.
    std::fs::write(
        &source,
        "#include <stdio.h>\n#include <unistd.h>\n\
         int main(void) {\n\
             pid_t child = fork();\n\
             if (child == 0) { close(STDOUT_FILENO); sleep(60); return 0; }\n\
             printf(\"%d\\n\", child);\n\
             return child < 0;\n\
         }\n",
    )?;
    let compiled = Command::new("cc")
        .arg(&source)
        .arg("-o")
        .arg(&stub)
        .status()?;
    if !compiled.success() {
        return Err(format!("stub compilation failed: {compiled}").into());
    }
    let leader = Command::new(&stub)
        .process_group(0)
        .stdout(std::process::Stdio::piped())
        .spawn()?;
    let worker = leader.id();
    let output = leader.wait_with_output()?;
    let left: u32 = String::from_utf8(output.stdout)?.trim().parse()?;
    let _left = KillOnDrop(left);

    let run = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let earlier = work_directories()?;
    let work = std::env::temp_dir().join(format!(
        "pdx-native-{}-cleanup-gate-{run}",
        std::process::id()
    ));
    std::fs::create_dir_all(work.join("session"))?;
    let detected = |file: &str, record: String| -> Result<bool, Box<dyn std::error::Error>> {
        std::fs::write(work.join("session").join(file), record)?;
        let inventory = processes()?;
        let found = owned_processes(&inventory, &owned_identities(&earlier)?) == [left]
            && !child_processes(&inventory).contains(&left);
        std::fs::remove_file(work.join("session").join(file))?;
        Ok(found)
    };
    let worker_group = detected(
        "worker-owned.json",
        format!(r#"{{"pid":{worker},"started_seconds":0,"started_microseconds":0}}"#),
    );
    let game = detected(
        "owner.json",
        format!(r#"{{"game":{{"pid":{left},"started_seconds":0,"started_microseconds":0}}}}"#),
    );
    std::fs::remove_dir_all(&work)?;
    if !worker_group? {
        return Err("a process left in a recorded worker group was not found".into());
    }
    if !game? {
        return Err("a process recorded as the game was not found".into());
    }
    Ok(())
}

/// Kill the process with this PID when dropped. Only for a process that this file started.
struct KillOnDrop(u32);
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = Command::new("kill")
            .args(["-KILL", &self.0.to_string()])
            .status();
    }
}

/// Keep a failed case's diagnostics when a later case succeeds.
fn work_directories() -> std::io::Result<BTreeSet<std::path::PathBuf>> {
    let prefix = format!("pdx-native-{}-", std::process::id());
    std::fs::read_dir(std::env::temp_dir())?
        .filter_map(|entry| match entry {
            Ok(entry) if entry.file_name().to_string_lossy().starts_with(&prefix) => {
                Some(Ok(entry.path()))
            }
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .collect()
}

/// One row of the host's process inventory.
struct Process {
    pid: u32,
    parent: u32,
    group: u32,
    /// The executable path.
    command: String,
}

/// Every process on the host.
fn processes() -> Result<Vec<Process>, Box<dyn std::error::Error>> {
    let output = Command::new("ps")
        .args(["-axo", "pid=,ppid=,pgid=,comm="])
        .output()?;
    Ok(process_inventory(&output)?)
}

/// A failed `ps` or an incomplete row fails the inventory, so that it never reads as "nothing
/// running".
fn process_inventory(output: &std::process::Output) -> Result<Vec<Process>, String> {
    if !output.status.success() {
        return Err(format!("process inventory failed: {}", output.status));
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(process_row)
        .collect()
}

fn process_row(line: &str) -> Result<Process, String> {
    let malformed = || format!("malformed process inventory row {line:?}");
    let mut rest = line;
    let mut number = || -> Result<u32, String> {
        let (field, after) = rest
            .trim_start()
            .split_once(char::is_whitespace)
            .ok_or_else(malformed)?;
        rest = after;
        field.parse().map_err(|_| malformed())
    };
    let pid = number()?;
    let parent = number()?;
    let group = number()?;
    let command = rest.trim();
    if command.is_empty() {
        return Err(malformed());
    }

    Ok(Process {
        pid,
        parent,
        group,
        command: command.to_owned(),
    })
}

/// Processes whose executable is the game.
fn game_processes(inventory: &[Process]) -> BTreeSet<u32> {
    inventory
        .iter()
        .filter(|process| {
            std::path::Path::new(&process.command)
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case("stellaris"))
        })
        .map(|process| process.pid)
        .collect()
}

/// Children of this process: the supervisors. `ps` itself has exited when its output is read.
fn child_processes(inventory: &[Process]) -> Vec<u32> {
    let this = std::process::id();
    inventory
        .iter()
        .filter(|process| process.parent == this && !process.command.ends_with("ps"))
        .map(|process| process.pid)
        .collect()
}

async fn fixture_nested_numeric(control: Fault) -> Outcome {
    use pdx_native::{
        FixtureFieldQuestion, FixtureParsing, FixtureRequest, FixtureStorage, FixtureValue,
        StoredFieldOccurrence,
    };
    use std::fmt::Write;
    let registry = "common/special_projects";
    let mut text = String::new();
    let mut questions = Vec::new();
    let mut expected = std::collections::BTreeMap::new();
    for (key, inputs, values) in [
        (
            "nested_boundary",
            vec!["-281474976710656.0"],
            vec![i64::MIN],
        ),
        ("nested_fractional", vec!["-1.25"], vec![-40960]),
        (
            "nested_malformed",
            vec!["7", "not_a_number"],
            vec![229376, 0],
        ),
    ] {
        writeln!(text, "special_project = {{\n requirements = {{")?;
        let mut occurrences = Vec::new();
        for (index, (input, raw)) in inputs.iter().zip(&values).enumerate() {
            let line = text.lines().count() as u64 + 1;
            writeln!(text, "  fleet_power = {input}")?;
            occurrences.push(StoredFieldOccurrence {
                line,
                occurrence: index as u64 + 1,
                value: FixtureValue::FixedPoint {
                    raw: *raw,
                    scale: 32768,
                },
            });
        }
        writeln!(text, " }}\n key = {key}\n}}")?;
        questions.push(
            FixtureFieldQuestion::new(registry, key, "fleet_power")
                .with_parent_field("requirements")
                .with_parsing(),
        );
        expected.insert(key.to_owned(), occurrences);
    }
    let request =
        FixtureRequest::field_outcomes(format!("{registry}/native_nested.txt"), text, questions);
    let recorded = tempfile::tempdir()?;
    let native = Native::open(std::env::var_os("STELLARIS_PATH").unwrap())?
        .record_answers_to(recorded.path());
    let mut prepared = options().fixture(request.clone());
    if control != Fault::Normal {
        prepared = prepared.fault(ObservationTarget::Fixture, control);
    }
    let started = native.start_game(prepared).await;
    if control == Fault::WorkerLoss {
        return match started {
            Err(Error::Startup {
                disposal: Disposal::Confirmed,
                reason,
            }) if reason.contains("WorkerLost") => Ok(()),
            Ok(mut game) => {
                let _ = game.close().await;
                Err("nested fixture worker loss unexpectedly started a session".into())
            }
            Err(error) => Err(format!("nested fixture worker loss: {error:?}").into()),
        };
    }
    let mut game = started?;
    let mut result = async {
        let answer = game.observe_fixture().await?;
        if answer.completeness != Completeness::Complete || !answer.gaps.is_empty() {
            return Err(format!("nested numeric observation incomplete: {answer:?}").into());
        }
        if !matches!(answer.value.diagnostic_coverage, pdx_native::DiagnosticCoverage::Complete {
            window: pdx_native::DiagnosticWindow::FixtureFileLoad
        }) || !answer.value.diagnostics.is_empty() {
            return Err(format!("nested diagnostic coverage differs: {answer:?}").into());
        }
        for outcome in &answer.value.field_outcomes {
            let occurrences = &expected[&outcome.question.definition];
            let storage = FixtureStorage::Observed { occurrences: occurrences.clone(),
                final_value: occurrences.last().map(|item| item.value.clone()), completeness: Completeness::Complete };
            if outcome.storage != storage || outcome.owner.is_none() {
                return Err(format!("nested storage differs: expected {storage:?}; got {outcome:?}").into());
            }
            if !matches!(&outcome.parsing, FixtureParsing::Observed { occurrences: parsed, completeness: Completeness::Complete } if parsed.len() == occurrences.len() && parsed.iter().zip(occurrences).all(|(parsed, stored)| parsed.line == stored.line && parsed.occurrence == stored.occurrence && parsed.return_line == Some(stored.line))) {
                return Err(format!("nested parsing incomplete: {outcome:?}").into());
            }
        }
        if answer.value.field_outcomes.len() != expected.len() { return Err("missing nested field outcomes".into()); }
        assert_recorded_fixture(recorded.path(), request, &answer).await?;
        Ok(())
    }.await;
    and_close(&mut result, &mut game).await;
    result
}

#[path = "live/script_numeric.rs"]
mod script_numeric;

/// The internal loader control keeps fixture replacement testable for SDK-552.
async fn loader_fixture(native: &Native) -> Outcome {
    let answer = pdx_native::internals::check_registry_load(
        native,
        options().fixture(fixture_request()),
        CATEGORIES,
    )
    .await?;
    if complete(&answer, CATEGORIES)? != 1 || answer.value != ["atlas_early_category"] {
        return Err(format!("fixture loader keys: {answer:?}").into());
    }
    Ok(())
}

/// Loaded modifier keys cover generator registries and the nonstandard +0x18 bypass key.
async fn loaded_modifier_key_layouts(native: &Native) -> Outcome {
    let mut game = native.start_game(options().loaded_modifiers()).await?;
    let mut result = async {
        let answer = game.loaded_modifiers().await?;
        for (registry, count) in GENERATOR_REGISTRIES {
            let keys = answer
                .value
                .registry_items
                .get(registry)
                .ok_or_else(|| format!("no loaded keys for {registry}: {:?}", answer.gaps))?;
            if keys.len() != count {
                return Err(format!("{registry}: {} keys, expected {count}", keys.len()).into());
            }
            if registry == "common/bypass" {
                source_keys_match(keys, registry)?;
            }
        }
        Ok(())
    }
    .await;
    and_close(&mut result, &mut game).await;
    result
}
