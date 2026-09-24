//! Live tests: each case starts the real game through the public API and checks the answers
//! and the cleanup.
//!
//! The cases are ignored by default. To run them, name the installation and pass `--ignored`:
//!
//! ```text
//! STELLARIS_PATH=/path/to/Stellaris cargo test --release --test live -- --ignored
//! STELLARIS_PATH=/path/to/Stellaris cargo test --release --test live -- --ignored missing_hook
//! ```
//!
//! A word after `--ignored` selects the cases whose name contains it. A case takes about 35
//! seconds; the full set takes longer as cases are added.
//!
//! This file has its own `main` (`harness = false` in `Cargo.toml`) for two reasons. The
//! supervisor is this executable with the `--supervisor` argument, and the standard harness
//! writes to the standard output that the supervisor protocol owns. Only one Native-owned game
//! may run on a host, so the cases run one at a time.
//!
//! The fault cases use the hidden `GameOptions::fault`. A fault applies to one registry; the
//! other registry must stay complete. The tests stop only their own unrelated sentinel process. They check that every
//! game and supervisor process that a case started is gone when the case ends.
use pdx_native::internals::ObservationControl as Fault;
use pdx_native::{
    Answer, Basis, Completeness, Disposal, Error, Game, GameOptions, GameReadiness, GapKind, Native,
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
const MAP_GALAXY: &str = "map/galaxy";
const CIVICS: &str = "common/governments/civics";
const GAME_SCENARIOS: &str = "common/game_scenarios";
const MAP_MODES: &str = "common/map_modes";
/// The registries whose database generators register modifier families (SDK-540), with the
/// item counts of the catalogued M45 build.
const GENERATOR_REGISTRIES: [(&str, usize); 6] = [
    ("common/buildings", 498),
    ("common/bypass", 10),
    ("common/districts", 147),
    ("common/megastructures", 164),
    ("common/situations", 90),
    ("common/zones", 146),
];
/// Item counts of the catalogued M45 build.
const ITEM_COUNTS: [(&str, usize); 3] =
    [(TRADITIONS, 234), (CATEGORIES, 33), (ASCENSION_PERKS, 49)];

type Outcome = Result<(), Box<dyn std::error::Error>>;

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
        let before = game_processes();
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
        let cleanup = processes_are_gone(&before);
        let result = outcome
            .and(isolation_result)
            .and(cleanup)
            .and_then(|()| remove_work_directories(&earlier_work));
        let seconds = started.elapsed().as_secs();
        match result {
            Ok(()) => println!("test {name} ... ok ({seconds} s)"),
            Err(error) => {
                println!("test {name} ... FAILED ({seconds} s): {error}");
                failed.push(name);
                // A later case cannot start while a process of this one remains.
                if processes_are_gone(&before).is_err() {
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
    Normal,
    InvalidSelection,
    OutsideCommon,
    LateOnly,
    NonstandardKey,
    GeneratorRegistries,
    RecordedRoundTrip,
    Fixture(Fault),
    FixtureOutsideSelection,
    FixtureSelection(pdx_native::FixtureObservationKind),
    FixtureRegistrationDropped,
    FixtureLaterRegistryDropped,
    FixtureTimeout,
    FixtureRefusal,
    FixtureOutcome(FixtureOutcomeCase),
    FixtureTransfer,
    FixtureRelicPortrait,
    StartupTimeout,
    Cancel,
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

#[derive(Clone, Copy)]
enum FixtureOutcomeCase {
    Valid,
    Omitted,
    Repeated,
    Malformed,
    Runtime,
    UnknownField,
    SameOwner,
    CategoryUnsupported,
    CategoryStorageUnsupported,
    DiagnosticsNotRequested,
    MaximumQuestions,
    UnrelatedDefinitions,
}

fn cases() -> Vec<(String, Case)> {
    let mut cases = vec![
        ("normal".to_owned(), Case::Normal),
        ("loaded_modifiers".to_owned(), Case::LoadedModifiers),
        (
            "loaded_modifiers_worker_loss".to_owned(),
            Case::LoadedModifiersWorkerLoss,
        ),
        (
            "loaded_modifiers_missing_registry_hook".to_owned(),
            Case::LoadedModifiersMissingRegistryHook,
        ),
        ("invalid_selection".to_owned(), Case::InvalidSelection),
        ("outside_common".to_owned(), Case::OutsideCommon),
        ("late_only".to_owned(), Case::LateOnly),
        ("nonstandard_key".to_owned(), Case::NonstandardKey),
        ("generator_registries".to_owned(), Case::GeneratorRegistries),
        ("recorded_round_trip".to_owned(), Case::RecordedRoundTrip),
        ("startup_timeout".to_owned(), Case::StartupTimeout),
        ("cancel".to_owned(), Case::Cancel),
        ("drop_without_close".to_owned(), Case::DropWithoutClose),
        (
            "fixture_outside_selection_is_rejected".to_owned(),
            Case::FixtureOutsideSelection,
        ),
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
        "fixture_registration_only".into(),
        Case::FixtureSelection(pdx_native::FixtureObservationKind::RegistrationEntries),
    ));
    cases.push((
        "fixture_transfer_string_reader".into(),
        Case::FixtureTransfer,
    ));
    cases.push((
        "fixture_transfer_relic_portrait".into(),
        Case::FixtureRelicPortrait,
    ));
    cases.push((
        "fixture_field_reads_only".into(),
        Case::FixtureSelection(pdx_native::FixtureObservationKind::CategoryFieldReads),
    ));
    cases.push((
        "fixture_registration_dropped_record".into(),
        Case::FixtureRegistrationDropped,
    ));
    cases.push((
        "fixture_later_registry_dropped_record".into(),
        Case::FixtureLaterRegistryDropped,
    ));
    cases.push(("fixture_timeout".into(), Case::FixtureTimeout));
    cases.push(("fixture_refusal".into(), Case::FixtureRefusal));
    for (name, case) in [
        ("valid", FixtureOutcomeCase::Valid),
        ("omitted", FixtureOutcomeCase::Omitted),
        ("repeated", FixtureOutcomeCase::Repeated),
        ("malformed", FixtureOutcomeCase::Malformed),
        ("runtime", FixtureOutcomeCase::Runtime),
        ("unknown_field", FixtureOutcomeCase::UnknownField),
        ("same_owner", FixtureOutcomeCase::SameOwner),
        (
            "category_unsupported",
            FixtureOutcomeCase::CategoryUnsupported,
        ),
        (
            "category_storage_unsupported",
            FixtureOutcomeCase::CategoryStorageUnsupported,
        ),
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
        Case::Normal => normal(native).await,
        Case::LoadedModifiers => loaded_modifiers(native).await,
        Case::LoadedModifiersWorkerLoss => loaded_modifiers_worker_loss(native).await,
        Case::LoadedModifiersMissingRegistryHook => {
            loaded_modifiers_missing_registry_hook(native).await
        }
        Case::InvalidSelection => invalid_selection(native).await,
        Case::GeneratorRegistries => generator_registries(native).await,
        Case::OutsideCommon => outside_common(native).await,
        Case::LateOnly => late_only(native).await,
        Case::NonstandardKey => nonstandard_key(native).await,
        Case::RecordedRoundTrip => recorded_round_trip(native).await,
        Case::Fixture(control) => fixture_case(control, None).await,
        Case::FixtureOutsideSelection => fixture_outside_selection(native).await,
        Case::FixtureSelection(kind) => fixture_case(Fault::Normal, Some(kind)).await,
        Case::FixtureRegistrationDropped => {
            fixture_case(
                Fault::DroppedRecord,
                Some(pdx_native::FixtureObservationKind::RegistrationEntries),
            )
            .await
        }
        Case::FixtureLaterRegistryDropped => fixture_later_registry_dropped(native).await,
        Case::FixtureTimeout => fixture_timeout(native).await,
        Case::FixtureRefusal => fixture_refusal(native).await,
        Case::FixtureOutcome(case) => fixture_outcome(case).await,
        Case::FixtureTransfer => fixture_transfer(native).await,
        Case::FixtureRelicPortrait => fixture_relic_portrait(native).await,
        Case::StartupTimeout => startup_timeout(native).await,
        Case::Cancel => cancel(native).await,
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
        let expected_counts = match case {
            FixtureOutcomeCase::CategoryUnsupported
            | FixtureOutcomeCase::CategoryStorageUnsupported => (234, 1),
            FixtureOutcomeCase::MaximumQuestions => (32, 33),
            FixtureOutcomeCase::UnrelatedDefinitions => (517, 33),
            _ => (1, 33),
        };
        if complete(&game.registry_items(TRADITIONS).await?, TRADITIONS)? != expected_counts.0
            || complete(&game.registry_items(CATEGORIES).await?, CATEGORIES)? != expected_counts.1
        {
            return Err("fixture did not replace only its selected registry".into());
        }
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
    let mut game = native
        .start_game(options().registries([ASCENSION_PERKS]).fixture(request))
        .await?;
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
            let FixtureStorage::String {
                occurrences,
                final_value,
                completeness: Completeness::Complete,
            } = &outcome.storage
            else {
                return Err(format!("transfer storage: {answer:?}").into());
            };
            if occurrences.len() != 1
                || occurrences[0].value != expected
                || final_value.as_deref() != Some(expected)
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
    let mut game = native
        .start_game(options().registries([RELICS]).fixture(request))
        .await?;
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
        let FixtureStorage::String {
            occurrences,
            final_value,
            completeness: Completeness::Complete,
        } = &outcome.storage
        else {
            return Err(format!("novel field storage: {answer:?}").into());
        };
        if occurrences.len() != 1
            || occurrences[0].value != "transfer_relic_portrait"
            || final_value.as_deref() != Some("transfer_relic_portrait")
        {
            return Err(format!("novel field value: {answer:?}").into());
        }
        Ok(())
    }
    .await;
    and_close(&mut result, &mut game).await;
    result
}

fn fixture_outcome_request(case: FixtureOutcomeCase) -> pdx_native::FixtureRequest {
    use pdx_native::{FixtureFieldQuestion, FixtureRequest};

    if matches!(
        case,
        FixtureOutcomeCase::CategoryUnsupported | FixtureOutcomeCase::CategoryStorageUnsupported
    ) {
        let mut question =
            FixtureFieldQuestion::new(CATEGORIES, "native_fixture_category", "tree_template");
        if matches!(case, FixtureOutcomeCase::CategoryStorageUnsupported) {
            question.diagnostics = false;
        }
        return FixtureRequest::field_outcomes(
            "common/tradition_categories/native_fixture.txt",
            "native_fixture_category = {\n tree_template = \"bad\nvalue\"\n}\n",
            [question],
        );
    }
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
        FixtureOutcomeCase::Valid
        | FixtureOutcomeCase::Runtime
        | FixtureOutcomeCase::DiagnosticsNotRequested => " unlocks_agenda = \"agenda_one\"\n",
        FixtureOutcomeCase::Omitted => "",
        FixtureOutcomeCase::Repeated => {
            " unlocks_agenda = \"agenda_one\"\n unlocks_agenda = \"agenda_two\"\n"
        }
        FixtureOutcomeCase::Malformed => " unlocks_agenda = \"agenda\nbroken\"\n",
        FixtureOutcomeCase::UnknownField => {
            " unlocks_agenda = \"agenda_one\"\n this_is_an_unknown_field = { broken = yes }\n"
        }
        FixtureOutcomeCase::SameOwner
        | FixtureOutcomeCase::CategoryUnsupported
        | FixtureOutcomeCase::CategoryStorageUnsupported
        | FixtureOutcomeCase::MaximumQuestions
        | FixtureOutcomeCase::UnrelatedDefinitions => unreachable!(),
    };
    let mut question =
        FixtureFieldQuestion::new(TRADITIONS, "native_fixture_tradition", "unlocks_agenda");
    if matches!(case, FixtureOutcomeCase::Runtime) {
        question = question.with_runtime();
    }
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
    use pdx_native::{
        DiagnosticCoverage, DiagnosticJoin, DiagnosticWindow, FixtureRuntime, FixtureStorage,
        ReaderKind,
    };

    let expected_completeness = if matches!(
        case,
        FixtureOutcomeCase::Runtime
            | FixtureOutcomeCase::CategoryUnsupported
            | FixtureOutcomeCase::CategoryStorageUnsupported
    ) {
        Completeness::Partial
    } else {
        Completeness::Complete
    };
    if answer.completeness != expected_completeness {
        return Err(format!("fixture completeness: {answer:?}").into());
    }
    let expected_coverage = match case {
        FixtureOutcomeCase::DiagnosticsNotRequested => {
            answer.value.diagnostic_coverage == DiagnosticCoverage::NotRequested
        }
        FixtureOutcomeCase::CategoryStorageUnsupported | FixtureOutcomeCase::MaximumQuestions => {
            answer.value.diagnostic_coverage == DiagnosticCoverage::NotRequested
        }
        _ => {
            answer.value.diagnostic_coverage
                == (DiagnosticCoverage::Complete {
                    window: DiagnosticWindow::FixtureFileLoad,
                })
        }
    };
    if !expected_coverage {
        return Err(format!("diagnostic coverage: {answer:?}").into());
    }
    if matches!(case, FixtureOutcomeCase::CategoryUnsupported) {
        let outcome = answer
            .value
            .field_outcomes
            .first()
            .ok_or("missing category outcome")?;
        let [diagnostic] = answer.value.diagnostics.as_slice() else {
            return Err(format!("missing category diagnostic: {answer:?}").into());
        };
        if !matches!(outcome.storage, FixtureStorage::Unavailable(_))
            || !answer
                .gaps
                .iter()
                .any(|gap| gap.kind == GapKind::OutsideMethod)
            || diagnostic.text != "Malformed token"
            || diagnostic.stage != "reader-malformed-report"
            || !matches!(&diagnostic.join,
                DiagnosticJoin::Source { file, line: 3, definition: None, field: None, occurrence: None }
                if file == "common/tradition_categories/native_fixture.txt")
        {
            return Err(format!("unsupported category outcome: {answer:?}").into());
        }
        return Ok(());
    }
    if matches!(case, FixtureOutcomeCase::CategoryStorageUnsupported) {
        let outcome = answer
            .value
            .field_outcomes
            .first()
            .ok_or("missing category storage outcome")?;
        if outcome.reader.kind != ReaderKind::String
            || outcome.reader.id.is_none()
            || !matches!(outcome.storage, FixtureStorage::Unavailable(_))
            || !answer
                .gaps
                .iter()
                .any(|gap| gap.kind == GapKind::OutsideMethod)
            || answer
                .gaps
                .iter()
                .any(|gap| gap.kind == GapKind::IncompleteObservation)
        {
            return Err(format!("unsupported category storage: {answer:?}").into());
        }
        return Ok(());
    }
    if matches!(case, FixtureOutcomeCase::MaximumQuestions) {
        return assert_maximum_questions(answer);
    }
    if matches!(case, FixtureOutcomeCase::UnrelatedDefinitions) {
        return assert_unrelated_definitions(answer);
    }
    if matches!(case, FixtureOutcomeCase::SameOwner) {
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
        assert_string_storage(agenda, &[(3, 1, "agenda")], Some("agenda"))?;
        return Ok(());
    }
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
        | FixtureOutcomeCase::Runtime
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
        | FixtureOutcomeCase::CategoryUnsupported
        | FixtureOutcomeCase::CategoryStorageUnsupported
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
    if matches!(case, FixtureOutcomeCase::Runtime) {
        if !matches!(outcome.runtime, FixtureRuntime::Unavailable(_))
            || !answer
                .gaps
                .iter()
                .any(|gap| gap.kind == GapKind::OutsideMethod)
        {
            return Err(format!("runtime outcome: {answer:?}").into());
        }
    } else if outcome.runtime != FixtureRuntime::NotRequested {
        return Err("unrequested runtime was not kept distinct".into());
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
    let pdx_native::FixtureStorage::String {
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
                occurrence.value.as_str(),
            )
        })
        .collect::<Vec<_>>();
    if actual != expected
        || final_value.as_deref() != expected_final
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

fn options() -> GameOptions {
    let mut supervisor = Command::new(std::env::current_exe().expect("test executable path"));
    supervisor.arg("--supervisor");
    GameOptions::new(supervisor)
}

fn fixture_request() -> pdx_native::FixtureRequest {
    pdx_native::FixtureRequest::new(
        "common/tradition_categories/atlas.txt",
        "atlas_early_category = {\n tree_template = \"atlas_early_template\"\n traditions = { }\n}\n",
    )
}

async fn fixture_case(
    control: Fault,
    selection: Option<pdx_native::FixtureObservationKind>,
) -> Outcome {
    use pdx_native::{Operation, ProcessingStage, Support};
    let recorded = tempfile::tempdir()?;
    let native = Native::open(std::env::var_os("STELLARIS_PATH").unwrap())?
        .record_answers_to(recorded.path());
    if native.supports(Operation::ObserveFixture) != Support::Supported {
        return Err("fixture operation is not supported".into());
    }
    let mut request = fixture_request();
    if let Some(kind) = selection {
        request.observations = vec![kind];
    }
    let mut prepared = options().fixture(request.clone());
    if control != Fault::Normal {
        prepared = prepared.fixture_fault(control);
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
        match (&control, &first) {
            (
                Fault::MissingHook | Fault::LateHook,
                Err(Error::Observation {
                    operation: Operation::ObserveFixture,
                    ..
                }),
            ) => {}
            (Fault::DroppedRecord | Fault::MissingTerminal | Fault::AccessFailure, Ok(answer)) => {
                if answer.completeness != Completeness::Partial || answer.gaps.is_empty() {
                    return Err(format!(
                        "{control:?}: expected partial fixture answer: {answer:?}"
                    )
                    .into());
                }
                let registration_only =
                    selection == Some(pdx_native::FixtureObservationKind::RegistrationEntries);
                let expected_registrations = if registration_only && control == Fault::DroppedRecord
                {
                    2
                } else {
                    3
                };
                let expected_reads = if registration_only {
                    0
                } else if control == Fault::DroppedRecord {
                    1
                } else {
                    2
                };
                if answer.value.registration_entries.len() != expected_registrations
                    || answer.value.field_reads.len() != expected_reads
                {
                    return Err(format!("{control:?}: established entries lost: {answer:?}").into());
                }
            }
            (Fault::Normal, Ok(answer)) => {
                let reads = &answer.value.field_reads;
                let expected_registrations: &[u64] = if request
                    .observations
                    .contains(&pdx_native::FixtureObservationKind::RegistrationEntries)
                {
                    &[1, 2, 3]
                } else {
                    &[]
                };
                let expected_reads: &[(&str, u64, &str)] = if request
                    .observations
                    .contains(&pdx_native::FixtureObservationKind::CategoryFieldReads)
                {
                    &[
                        ("common/tradition_categories/atlas.txt", 2, "tree_template"),
                        ("common/tradition_categories/atlas.txt", 3, "traditions"),
                    ]
                } else {
                    &[]
                };
                if answer.completeness != Completeness::Complete
                    || !answer.gaps.is_empty()
                    || answer.source.basis != Basis::LiveObservation
                    || answer
                        .value
                        .registration_entries
                        .iter()
                        .map(|entry| entry.ordinal)
                        .collect::<Vec<_>>()
                        != expected_registrations
                    || reads
                        .iter()
                        .map(|read| (read.file.as_str(), read.line, read.field.as_str()))
                        .collect::<Vec<_>>()
                        != expected_reads
                    || (reads.len() == 2 && reads[0].owner != reads[1].owner)
                    || reads
                        .iter()
                        .any(|read| read.stage != ProcessingStage::FieldReadEntry)
                {
                    return Err(format!("normal fixture: {answer:?}").into());
                }
            }
            (_, answer) => {
                return Err(format!("{control:?}: unexpected fixture result: {answer:?}").into());
            }
        }
        if let Ok(answer) = &first
            && answer.value.diagnostic_coverage != pdx_native::DiagnosticCoverage::NotRequested
        {
            return Err(format!("entry-only diagnostic coverage: {answer:?}").into());
        }
        for _ in 0..2 {
            let categories = game.registry_items(CATEGORIES).await?;
            if complete(&categories, CATEGORIES)? != 1
                || categories.value != ["atlas_early_category"]
            {
                return Err(format!("mounted categories: {categories:?}").into());
            }
            if complete(&game.registry_items(TRADITIONS).await?, TRADITIONS)? != 234 {
                return Err("fixture altered the pinned traditions".into());
            }
            if game.observe_fixture().await != first {
                return Err("fixture read changed after registry query".into());
            }
        }
        let recorded_native = Native::from_recorded_answers(recorded.path())?;
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
    .await;
    and_close(&mut result, &mut game).await;
    if result.is_ok() && !matches!(game.observe_fixture().await, Err(Error::Closed)) {
        return Err("closed fixture session still answered".into());
    }
    result
}

async fn fixture_outside_selection(native: &Native) -> Outcome {
    match native
        .start_game(
            options()
                .registries([MAP_GALAXY])
                .fixture(fixture_request()),
        )
        .await
    {
        Err(Error::FixtureRequest { reason }) if reason.contains("GameOptions::registries") => {
            Ok(())
        }
        other => Err(format!("fixture outside registry selection: {other:?}").into()),
    }
}

async fn fixture_timeout(native: &Native) -> Outcome {
    let mut request = fixture_request();
    request.deadline_seconds = 1;
    match native.start_game(options().fixture(request)).await {
        Err(Error::Startup {
            disposal: Disposal::Confirmed,
            reason,
        }) if reason.contains("TimedOut") => Ok(()),
        Ok(mut game) => {
            let _ = game.close().await;
            Err("fixture deadline was not enforced".into())
        }
        Err(error) => Err(format!("fixture timeout: {error:?}").into()),
    }
}

async fn fixture_later_registry_dropped(native: &Native) -> Outcome {
    let mut game = native
        .start_game(
            options()
                .fixture(fixture_request())
                .fault(CATEGORIES, Fault::DroppedRecord),
        )
        .await?;
    let mut result = async {
        let fixture = game.observe_fixture().await?;
        if fixture.completeness != Completeness::Complete
            || !fixture.gaps.is_empty()
            || fixture.value.registration_entries.len() != 3
            || fixture.value.field_reads.len() != 2
        {
            return Err(
                format!("later registry loss changed completed fixture: {fixture:?}").into(),
            );
        }
        let categories = game.registry_items(CATEGORIES).await?;
        if categories.completeness != Completeness::Partial || categories.gaps.is_empty() {
            return Err(format!("registry fault did not take effect: {categories:?}").into());
        }
        if game.observe_fixture().await? != fixture {
            return Err("fixture changed after registry query".into());
        }
        Ok(())
    }
    .await;
    and_close(&mut result, &mut game).await;
    result
}

async fn fixture_refusal(native: &Native) -> Outcome {
    let before = work_directories()?;
    let request = pdx_native::FixtureRequest::new("common/unsupported/fixture.txt", "x = {}");
    match native.start_game(options().fixture(request)).await {
        Err(Error::FixtureRequest { reason })
            if reason.contains("requires a common/tradition_categories fixture") => {}
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
fn source_keys_match(answer: &Answer<Vec<String>>, registry: &str) -> Outcome {
    let root = std::path::PathBuf::from(std::env::var_os("STELLARIS_PATH").unwrap());
    let mut source = BTreeSet::new();
    for entry in std::fs::read_dir(root.join(registry))? {
        let path = entry?.path();
        if path.extension().is_none_or(|extension| extension != "txt") {
            continue;
        }
        for line in std::fs::read_to_string(path)?.lines() {
            if line.starts_with([' ', '\t', '#']) {
                continue;
            }
            if let Some((key, _)) = line.split_once('=') {
                let key = key.trim();
                if !key.is_empty()
                    && key.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'
                    })
                {
                    source.insert(key.to_owned());
                }
            }
        }
    }
    let observed: BTreeSet<_> = answer.value.iter().cloned().collect();
    if observed != source {
        return Err(format!("{registry}: live keys differ from top-level source keys").into());
    }
    Ok(())
}

async fn close_confirmed(game: &mut Game) -> Outcome {
    let disposal = game.close().await?;
    if disposal != Disposal::Confirmed {
        return Err(format!("disposal: {disposal:?}").into());
    }
    // A repeated close gives the same result, and a closed game answers nothing.
    if game.close().await? != Disposal::Confirmed {
        return Err("the second close gave another disposal".into());
    }
    match game.registry_items(TRADITIONS).await {
        Err(Error::Closed) => Ok(()),
        other => Err(format!("after close: {other:?}").into()),
    }
}

async fn normal(native: &Native) -> Outcome {
    let mut game = native
        .start_game(options().registries([TRADITIONS, CATEGORIES, ASCENSION_PERKS]))
        .await?;
    let readiness = game.readiness();
    let mut result = async {
        if readiness != GameReadiness::PausedAfterRegistryInitialization {
            return Err(format!("readiness: {readiness:?}").into());
        }
        // Both orders, so that each registry is read after the other.
        for (registry, count) in ITEM_COUNTS.into_iter().chain(ITEM_COUNTS.into_iter().rev()) {
            let first = game.registry_items(registry).await?;
            if complete(&first, registry)? != count {
                return Err(format!("{registry}: {} items", first.value.len()).into());
            }
            if registry == ASCENSION_PERKS {
                source_keys_match(&first, registry)?;
            }
            if game.registry_items(registry).await? != first {
                return Err(format!("{registry}: a repeated read gave another answer").into());
            }
        }
        match game.registry_items("common/agendas").await {
            Err(Error::Unsupported { .. }) => {}
            other => {
                return Err(format!("a registry outside the discovery method: {other:?}").into());
            }
        }
        match game.registry_items("common/ethics").await {
            Err(Error::Unsupported { reason, .. })
                if reason.contains("GameOptions::registries") =>
            {
                Ok(())
            }
            other => Err(format!("a listed but unselected registry: {other:?}").into()),
        }
    }
    .await;
    and_close(&mut result, &mut game).await;
    result
}

/// The loaded tags of the five declared names that content registers again (M45-release).
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
/// Entries of the loaded table on M45-release with installed content (SDK-488: 45,578).
const LOADED_MODIFIERS: usize = 45_578;

async fn loaded_modifiers(native: &Native) -> Outcome {
    use pdx_native::{DeclaredTags, LoadedContent};
    let declared = native.modifiers()?.value;
    let mut game = native.start_game(options().loaded_modifiers()).await?;
    let readiness = game.readiness();
    let mut result = async {
        if readiness != GameReadiness::PausedAfterContentLoad {
            return Err(format!("readiness: {readiness:?}").into());
        }
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
        engine_log_agrees(loaded)?;
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
        complete(&game.registry_items(TRADITIONS).await?, TRADITIONS)?;
        Ok(())
    }
    .await;
    and_close(&mut result, &mut game).await;
    result
}

/// Compare every name and tag list with the modifier documentation that the engine itself wrote
/// in the session's private profile. Only this test reads the log.
fn engine_log_agrees(loaded: &[pdx_native::LoadedModifier]) -> Outcome {
    let logs: Vec<_> = work_directories()?
        .into_iter()
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
                .registries([TRADITIONS])
                .fault(TRADITIONS, Fault::MissingHook)
                .loaded_modifiers(),
        )
        .await?;
    let readiness = game.readiness();
    let mut result = async {
        if readiness != GameReadiness::PausedAfterContentLoad {
            return Err(format!("readiness: {readiness:?}").into());
        }
        let loaded = game.loaded_modifiers().await?;
        if loaded.value.modifiers.len() != LOADED_MODIFIERS {
            return Err(format!("{} loaded modifiers", loaded.value.modifiers.len()).into());
        }
        match game.registry_items(TRADITIONS).await {
            Err(Error::Observation { .. }) => Ok(()),
            other => Err(format!("a registry without its hook: {other:?}").into()),
        }
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
                .modifier_fault(Fault::WorkerLoss),
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

async fn invalid_selection(native: &Native) -> Outcome {
    match native
        .start_game(options().registries(["common/no_such_registry"]))
        .await
    {
        Err(Error::UnknownRegistry { .. }) => {}
        other => return Err(format!("unknown registry: {other:?}").into()),
    }
    match native
        .start_game(options().registries([TRADITIONS, TRADITIONS]))
        .await
    {
        Err(Error::Startup {
            disposal: Disposal::NotApplicable,
            ..
        }) => Ok(()),
        other => Err(format!("duplicate registry: {other:?}").into()),
    }
}

async fn outside_common(native: &Native) -> Outcome {
    let mut game = native
        .start_game(options().registries([MAP_GALAXY, CIVICS, TRADITIONS]))
        .await?;
    let mut result = async {
        let galaxy = game.registry_items(MAP_GALAXY).await?;
        if complete(&galaxy, MAP_GALAXY)? != 10 {
            return Err(format!("{MAP_GALAXY}: {} items", galaxy.value.len()).into());
        }
        source_keys_match(&galaxy, MAP_GALAXY)?;
        let civics = game.registry_items(CIVICS).await?;
        if complete(&civics, CIVICS)? != 358 {
            return Err("civic count changed".into());
        }
        source_keys_match(&civics, CIVICS)?;
        let traditions = game.registry_items(TRADITIONS).await?;
        if complete(&traditions, TRADITIONS)? != 234 {
            return Err("tradition control changed".into());
        }
        Ok(())
    }
    .await;
    and_close(&mut result, &mut game).await;
    result
}

/// A registry whose loader never runs before the startup deadline: the worker stops the game
/// there, and the answer says that the deadline, not another loader, ended the session.
async fn late_only(native: &Native) -> Outcome {
    let mut options = options().registries([GAME_SCENARIOS]);
    options.startup_seconds = 60;
    let mut game = native.start_game(options).await?;
    let mut result = async {
        if game.readiness() != GameReadiness::PausedDuringRegistryInitialization {
            return Err("late-only session did not pause during initialization".into());
        }
        match game.registry_items(GAME_SCENARIOS).await {
            Err(Error::Unsupported { reason, .. })
                if reason.contains("initial loader") && reason.contains("startup deadline") =>
            {
                Ok(())
            }
            other => Err(format!("late-only registry: {other:?}").into()),
        }
    }
    .await;
    and_close(&mut result, &mut game).await;
    result
}

/// `common/map_modes` keeps its key at `+0x18`; its loader runs before the pause (SDK-573).
async fn nonstandard_key(native: &Native) -> Outcome {
    let mut game = native
        .start_game(options().registries([MAP_MODES, TRADITIONS]))
        .await?;
    let mut result = async {
        if game.readiness() != GameReadiness::PausedAfterRegistryInitialization {
            return Err(format!("readiness: {:?}", game.readiness()).into());
        }
        let answer = game.registry_items(MAP_MODES).await?;
        if complete(&answer, MAP_MODES)? != 8 {
            return Err(format!("{MAP_MODES}: {} items", answer.value.len()).into());
        }
        source_keys_match(&answer, MAP_MODES)
    }
    .await;
    and_close(&mut result, &mut game).await;
    result
}

/// The six generator registries load on the launch thread before the pause (SDK-573).
async fn generator_registries(native: &Native) -> Outcome {
    let mut game = native
        .start_game(options().registries(GENERATOR_REGISTRIES.map(|(name, _)| name)))
        .await?;
    let mut result = async {
        if game.readiness() != GameReadiness::PausedAfterRegistryInitialization {
            return Err(format!("readiness: {:?}", game.readiness()).into());
        }
        for (registry, count) in GENERATOR_REGISTRIES {
            let answer = game.registry_items(registry).await?;
            if complete(&answer, registry)? != count {
                return Err(format!("{registry}: {} items", answer.value.len()).into());
            }
        }
        Ok(())
    }
    .await;
    and_close(&mut result, &mut game).await;
    result
}

async fn recorded_round_trip(_native: &Native) -> Outcome {
    let directory = tempfile::tempdir()?;
    let real = Native::open(std::env::var_os("STELLARIS_PATH").unwrap())?
        .record_answers_to(directory.path());
    let discovered = real.registries()?;
    let selected = [TRADITIONS, CATEGORIES, ASCENSION_PERKS];
    let mut game = real.start_game(options().registries(selected)).await?;
    let mut original = Vec::new();
    let mut result = async {
        for name in selected {
            original.push((name, game.registry_items(name).await?));
        }
        Ok(())
    }
    .await;
    and_close(&mut result, &mut game).await;
    result?;
    let recorded = Native::from_recorded_answers(directory.path())?;
    let mut again = recorded.registries()?;
    again.source.basis = discovered.source.basis;
    if again != discovered {
        return Err("recorded registry list differs".into());
    }
    let mut game = recorded
        .start_game(GameOptions::new(Command::new("must-not-start")))
        .await?;
    for (name, original) in original {
        let mut again = game.registry_items(name).await?;
        again.source.basis = original.source.basis;
        if again != original {
            return Err(format!("recorded {name} differs").into());
        }
    }
    if game.close().await? != Disposal::NotApplicable {
        return Err("recorded close started a process".into());
    }
    Ok(())
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
    other: &'static str,
    control: Fault,
    expect: Expect,
) -> Outcome {
    let mut game = native
        .start_game(
            options()
                .registries([registry, other])
                .fault(registry, control),
        )
        .await?;
    let readiness = game.readiness();
    let mut result = async {
        // A hook fault stops the game before the faulted registry returns from its load.
        let expected_readiness = match control {
            Fault::MissingHook | Fault::LateHook => {
                GameReadiness::PausedDuringRegistryInitialization
            }
            _ => GameReadiness::PausedAfterRegistryInitialization,
        };
        if readiness != expected_readiness {
            return Err(format!("readiness: {readiness:?}").into());
        }
        // Both orders: a failed read must not change the other registry's answer.
        for name in [registry, other, other, registry] {
            let answer = game.registry_items(name).await;
            if name == other {
                let answer = answer?;
                if complete(&answer, name)? == 0 {
                    return Err(format!("{name}: no items").into());
                }
                continue;
            }
            match (expect, answer) {
                (Expect::NoAnswer, Err(Error::Observation { .. })) => {}
                (Expect::PartialAnswer, Ok(answer))
                    if answer.completeness == Completeness::Partial
                        && answer
                            .gaps
                            .iter()
                            .any(|gap| gap.kind == GapKind::IncompleteObservation) => {}
                (_, answer) => return Err(format!("{name} with {control:?}: {answer:?}").into()),
            }
        }
        Ok(())
    }
    .await;
    and_close(&mut result, &mut game).await;
    result
}

/// The supervisor stops the debugger worker while the faulted registry loads. The game never
/// reaches its pause, so there is no `Game`; the start fails and the game is still reaped.
async fn worker_loss(native: &Native, registry: &'static str, control: Fault) -> Outcome {
    let other = if registry == TRADITIONS {
        CATEGORIES
    } else {
        TRADITIONS
    };
    match native
        .start_game(
            options()
                .registries([registry, other])
                .fault(registry, control),
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

async fn cancel(native: &Native) -> Outcome {
    let mut game = native.start_game(options()).await?;
    game.cancel();
    let mut result = match game.registry_items(TRADITIONS).await {
        Err(Error::Closed) => Ok(()),
        other => Err(format!("after cancel: {other:?}").into()),
    };
    and_close(&mut result, &mut game).await;
    result
}

/// The caller forgets to close. The supervisor sees its control input end and reaps the game.
async fn drop_without_close(native: &Native) -> Outcome {
    let game = native.start_game(options()).await?;
    if game_processes().is_empty() {
        return Err("no game process after start".into());
    }
    drop(game);
    Ok(())
}

/// Wait until every game process that started after `before`, and every child of this process,
/// is gone. This only looks; it never signals a process.
fn processes_are_gone(before: &BTreeSet<u32>) -> Outcome {
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let games: Vec<_> = game_processes().difference(before).copied().collect();
        let children = child_processes();
        if games.is_empty() && children.is_empty() {
            break;
        }
        if Instant::now() >= deadline {
            return Err(format!("still running: games {games:?}, children {children:?}").into());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Ok(())
}

/// Native removes its work directory after a confirmed `close`, and keeps it after a failed
/// start or a drop. A case that passed needs no inspection, so remove what it left.
fn remove_work_directories(earlier: &BTreeSet<std::path::PathBuf>) -> Outcome {
    for path in work_directories()?.difference(earlier) {
        std::fs::remove_dir_all(path)?;
    }
    Ok(())
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

/// Every process on the host with `(pid, parent pid, executable path)`.
fn processes() -> Vec<(u32, u32, String)> {
    let output = Command::new("ps")
        .args(["-axo", "pid=,ppid=,comm="])
        .output()
        .expect("ps");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let (pid, rest) = line.trim_start().split_once(char::is_whitespace)?;
            let (parent, command) = rest.trim_start().split_once(char::is_whitespace)?;
            Some((
                pid.parse().ok()?,
                parent.parse().ok()?,
                command.trim().to_owned(),
            ))
        })
        .collect()
}

/// Processes whose executable is the game.
fn game_processes() -> BTreeSet<u32> {
    processes()
        .into_iter()
        .filter(|(_, _, command)| {
            std::path::Path::new(command)
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case("stellaris"))
        })
        .map(|(pid, _, _)| pid)
        .collect()
}

/// Children of this process: the supervisors. `ps` itself has exited when its output is read.
fn child_processes() -> Vec<u32> {
    let this = std::process::id();
    processes()
        .into_iter()
        .filter(|(_, parent, command)| *parent == this && !command.ends_with("ps"))
        .map(|(pid, _, _)| pid)
        .collect()
}
