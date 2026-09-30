//! A compact developer summary of one session, written to `run-summary.json` in the work
//! directory for every session that owns one.
//!
//! The summary reports; it never decides. It projects what the other authorities established:
//! the worker's requested hooks and hook state, its stream as written, the reducers' results at
//! the pause, and the supervisor's monotonic phase clock. No answer, outcome or cleanup depends on
//! it. Lists are cut to a few samples, so a failed session stays readable at a glance.
use crate::{
    Completeness, DiagnosticCoverage, FixtureObservation, FixtureParsing, FixtureRuntime,
    FixtureStorage,
    answer::Disposal,
    engine::operations::{
        event_stream::{self, Hook, WorkerEvent, WorkerRecord},
        loaded_modifiers::ObservedModifiers,
        registry_items::{Observed, RegistryItems},
    },
    protocol::{
        self,
        session::{SessionOutcome, SessionReport, SessionRequest},
    },
    supervisor::SupervisorError,
    work_directory as files,
};
use serde::Serialize;
use std::{collections::BTreeMap, path::Path, time::Instant};

pub(super) const FILE: &str = "run-summary.json";

/// The most samples that one list keeps.
const SAMPLES: usize = 8;

/// The most characters that one sampled text keeps.
const TEXT: usize = 240;

const CLOCK: &str = "Supervisor monotonic clock, from the host reservation to the end of cleanup \
    (worker stopped, game reaped, reservation resolved). Excludes plan admission, the caller \
    handshake and the worker's own time.";

const BEFORE_PAUSE: &str = "the session ended before its pause";

/// The session phases, in the order that a session passes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Phase {
    /// Reservation to a suspended game that the supervisor owns.
    Setup,
    /// The debugger worker starts.
    WorkerStart,
    /// The worker observes the load until the supervisor witnesses its pause and answers.
    AwaitingPause,
    /// The game is held while the caller reads answers.
    Paused,
}

const PHASES: [Phase; 4] = [
    Phase::Setup,
    Phase::WorkerStart,
    Phase::AwaitingPause,
    Phase::Paused,
];

/// What the supervisor keeps for the summary while one session runs.
pub(super) struct RunRecord {
    started: Instant,
    completed: Vec<(Phase, Instant)>,
    session_ended: Option<Instant>,
    cleanup_ended: Option<Instant>,
    fixture_requested: bool,
    modifiers_requested: bool,
    paused: Option<PausedSummary>,
}

impl RunRecord {
    /// Start the clock of a session with this request.
    pub(super) fn new(request: &SessionRequest) -> Self {
        Self {
            started: Instant::now(),
            completed: Vec::new(),
            session_ended: None,
            cleanup_ended: None,
            fixture_requested: request.fixture.is_some(),
            modifiers_requested: request.loaded_modifiers.is_some(),
            paused: None,
        }
    }

    pub(super) fn complete(&mut self, phase: Phase) {
        self.completed.push((phase, Instant::now()));
    }

    /// Keep the compact projection of the answers that the supervisor sends at the pause.
    pub(super) fn paused(&mut self, summary: PausedSummary) {
        self.paused = Some(summary);
    }

    /// The session ended with any outcome; cleanup follows.
    pub(super) fn end_session(&mut self) {
        let now = Instant::now();

        // A paused session has nothing left to reach: its pause ends with the session.
        if self.phase_completed(Phase::AwaitingPause) {
            self.completed.push((Phase::Paused, now));
        }

        self.session_ended = Some(now);
    }

    pub(super) fn end_cleanup(&mut self) {
        self.cleanup_ended = Some(Instant::now());
    }

    fn phase_completed(&self, phase: Phase) -> bool {
        self.completed
            .iter()
            .any(|(completed, _)| *completed == phase)
    }

    fn timing(&self) -> Timing {
        let mut previous = self.started;
        let mut phases = Vec::new();
        let mut last_completed_phase = None;
        let mut running = true;

        for phase in PHASES {
            let completed = self
                .completed
                .iter()
                .find_map(|(completed, at)| (*completed == phase).then_some(*at));
            let time = match completed {
                Some(at) => {
                    let milliseconds = milliseconds(previous, at);

                    previous = at;
                    last_completed_phase = Some(phase);
                    PhaseTime::completed(phase, milliseconds)
                }
                None if running => {
                    running = false;
                    PhaseTime {
                        phase,
                        state: PhaseState::Interrupted,
                        milliseconds: self.session_ended.map(|end| milliseconds(previous, end)),
                    }
                }
                None => PhaseTime {
                    phase,
                    state: PhaseState::NotReached,
                    milliseconds: None,
                },
            };

            phases.push(time);
        }

        let cleanup_milliseconds = self
            .session_ended
            .zip(self.cleanup_ended)
            .map(|(end, checked)| milliseconds(end, checked));

        Timing {
            clock: CLOCK,
            phases,
            last_completed_phase,
            cleanup_milliseconds,
        }
    }

    fn observations(&self) -> ObservationSummary {
        if let Some(paused) = &self.paused {
            return ObservationSummary {
                registries: RegistriesSummary::Observed(paused.registries.clone()),
                fixture: paused.fixture.clone(),
                modifiers: paused.modifiers.clone(),
            };
        }

        let fixture = if self.fixture_requested {
            FixtureSummary::Unavailable(BEFORE_PAUSE.into())
        } else {
            FixtureSummary::NotRequested
        };
        let modifiers = if self.modifiers_requested {
            ModifierSummary::Unavailable(BEFORE_PAUSE.into())
        } else {
            ModifierSummary::NotRequested
        };

        ObservationSummary {
            registries: RegistriesSummary::Unavailable(BEFORE_PAUSE.into()),
            fixture,
            modifiers,
        }
    }
}

/// The compact projection of the answers at the pause. It keeps counts and samples, never the
/// items themselves.
#[derive(Debug, Clone)]
pub(super) struct PausedSummary {
    registries: BTreeMap<String, RegistrySummary>,
    fixture: FixtureSummary,
    modifiers: ModifierSummary,
}

impl PausedSummary {
    pub(super) fn new(
        registries: &BTreeMap<String, RegistryItems>,
        fixture: Option<&Result<crate::Answer<FixtureObservation>, crate::Error>>,
        modifiers: Option<&Result<ObservedModifiers, crate::Error>>,
    ) -> Self {
        let registries = registries
            .iter()
            .map(|(name, items)| (name.clone(), RegistrySummary::new(items)))
            .collect();
        let fixture = match fixture {
            None => FixtureSummary::NotRequested,
            Some(Err(error)) => FixtureSummary::Error(cut(&error.to_string())),
            Some(Ok(answer)) => FixtureSummary::observed(answer),
        };
        let modifiers = match modifiers {
            None => ModifierSummary::NotRequested,
            Some(Err(error)) => ModifierSummary::Error(cut(&error.to_string())),
            Some(Ok(table)) => ModifierSummary::Observed {
                entries: table.entries.len(),
            },
        };

        Self {
            registries,
            fixture,
            modifiers,
        }
    }
}

/// Write the summary of a finished session. The caller reports a failure; it never changes the
/// session's outcome.
pub(super) fn write(
    work_directory: &Path,
    report: &SessionReport,
    run: &RunRecord,
) -> Result<(), SupervisorError> {
    let trace = work_directory.join("raw-trace.jsonl");
    let (records, stream) = if trace.try_exists()? {
        let raw = files::read_bounded(&trace, protocol::observation::MAX_TRACE)?;
        let (records, damage) = event_stream::parse_worker_stream(&raw, &report.attempt);
        let stream = damage.map_or(StreamState::Intact, StreamState::Damaged);

        (records, stream)
    } else {
        let stream = StreamState::Unavailable("the worker stream was never created".into());

        (Vec::new(), stream)
    };
    let summary = summarize(
        report,
        run,
        &records,
        stream,
        read_worker_diagnostics(work_directory, &report.attempt),
    );

    files::write_json(&work_directory.join(FILE), &summary)
}

fn summarize(
    report: &SessionReport,
    run: &RunRecord,
    records: &[WorkerRecord],
    stream: StreamState,
    diagnostics: Result<protocol::observation::WorkerDiagnostics, String>,
) -> RunSummary {
    RunSummary {
        attempt: report.attempt.clone(),
        outcome: report.outcome.clone(),
        disposal: report.disposal.clone(),
        timing: run.timing(),
        worker: WorkerSummary::new(records, stream),
        hooks: HookSummary::new(records),
        observations: run.observations(),
        worker_diagnostics: WorkerDiagnosticSummary::new(report, records, diagnostics),
    }
}

#[derive(Debug, Serialize)]
struct RunSummary {
    attempt: String,
    outcome: SessionOutcome,
    disposal: Disposal,
    timing: Timing,
    worker: WorkerSummary,
    hooks: HookSummary,
    observations: ObservationSummary,
    worker_diagnostics: WorkerDiagnosticSummary,
}

/// A cause comes from a worker failure, never from its disappearance alone.
#[derive(Debug, Serialize)]
struct WorkerDiagnosticSummary {
    reason: String,
    context: Option<protocol::observation::WorkerDiagnostics>,
    unavailable: Option<String>,
}

impl WorkerDiagnosticSummary {
    fn new(
        report: &SessionReport,
        records: &[WorkerRecord],
        diagnostics: Result<protocol::observation::WorkerDiagnostics, String>,
    ) -> Self {
        let (context, unavailable) = match diagnostics {
            Ok(context) => (Some(context), None),
            Err(reason) => (None, Some(cut(&reason))),
        };
        let cause = context
            .as_ref()
            .and_then(|context| context.failure.as_ref());
        let reason = if let Some(cause) = cause {
            format!("{}: {}", cause.kind, cause.reason)
        } else if let Some(reason) = records.iter().find_map(|record| match &record.event {
            WorkerEvent::CapabilityUnavailable { reason }
            | WorkerEvent::NativeException { reason } => Some(reason),
            WorkerEvent::CallbackError { error } => Some(error),
            _ => None,
        }) {
            cut(reason)
        } else if matches!(
            report.outcome,
            SessionOutcome::WorkerLost | SessionOutcome::TimedOut | SessionOutcome::Failed(_)
        ) {
            let operation = context
                .as_ref()
                .map(|context| context.operation.clone())
                .or_else(|| records.last().map(|record| record_kind(&record.event)));
            match operation {
                Some(operation) => {
                    format!("cause unavailable; last witnessed operation: {operation}")
                }
                None => "cause unavailable; no worker operation witnessed".into(),
            }
        } else {
            "no worker failure reported".into()
        };

        Self {
            reason: cut(&reason),
            context,
            unavailable,
        }
    }
}

fn read_worker_diagnostics(
    directory: &Path,
    attempt: &str,
) -> Result<protocol::observation::WorkerDiagnostics, String> {
    let path = directory.join("worker-diagnostics.json");
    let bytes = files::read_bounded(&path, protocol::observation::MAX_WORKER_DIAGNOSTICS)
        .map_err(|error| format!("worker checkpoint unavailable: {error}"))?;
    let mut context: protocol::observation::WorkerDiagnostics = serde_json::from_slice(&bytes)
        .map_err(|error| format!("worker checkpoint malformed: {error}"))?;
    if context.attempt != attempt {
        return Err("worker checkpoint belongs to another attempt".into());
    }
    context.phase = cut(&context.phase);
    context.context = context.context.map(|context| cut(&context));
    context.operation = cut(&context.operation);
    for call in [
        &mut context.last_attempted_call,
        &mut context.last_completed_call,
    ]
    .into_iter()
    .flatten()
    {
        call.operation = cut(&call.operation);
    }
    context.hooks.truncate(SAMPLES);
    for hook in &mut context.hooks {
        hook.name = cut(&hook.name);
    }
    context.details = bounded_details(context.details);
    if let Some(failure) = &mut context.failure {
        failure.kind = cut(&failure.kind);
        failure.reason = cut(&failure.reason);
        failure.details = bounded_details(std::mem::take(&mut failure.details));
    }
    Ok(context)
}

fn bounded_details(details: BTreeMap<String, String>) -> BTreeMap<String, String> {
    details
        .into_iter()
        .take(16)
        .map(|(key, value)| (cut(&key), cut(&value)))
        .collect()
}

#[derive(Debug, Serialize)]
struct Timing {
    /// The clock and its measurement boundary.
    clock: &'static str,
    phases: Vec<PhaseTime>,
    last_completed_phase: Option<Phase>,
    /// From the end of the session to the end of cleanup.
    cleanup_milliseconds: Option<u64>,
}

#[derive(Debug, Serialize)]
struct PhaseTime {
    phase: Phase,
    state: PhaseState,
    #[serde(skip_serializing_if = "Option::is_none")]
    milliseconds: Option<u64>,
}

impl PhaseTime {
    fn completed(phase: Phase, milliseconds: u64) -> Self {
        Self {
            phase,
            state: PhaseState::Completed,
            milliseconds: Some(milliseconds),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum PhaseState {
    Completed,
    /// The phase was running when the session ended.
    Interrupted,
    NotReached,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
enum StreamState {
    Intact,
    /// Reading stopped at a damaged, partial or foreign record; the records before it count.
    Damaged(String),
    Unavailable(String),
}

#[derive(Debug, Serialize)]
struct WorkerSummary {
    stream: StreamState,
    records: usize,
    last_record: Option<LastRecord>,
    /// Each place where a sequence number is not the one after its predecessor. A hole does not
    /// say how many records were lost.
    sequence_discontinuities: Sample<Discontinuity>,
}

impl WorkerSummary {
    fn new(records: &[WorkerRecord], stream: StreamState) -> Self {
        let last_record = records.last().map(|record| LastRecord {
            seq: record.seq,
            kind: record_kind(&record.event),
        });

        Self {
            stream,
            records: records.len(),
            last_record,
            sequence_discontinuities: Sample::new(discontinuities(records)),
        }
    }
}

#[derive(Debug, Serialize)]
struct LastRecord {
    seq: u64,
    kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Discontinuity {
    expected: u64,
    found: u64,
}

fn discontinuities(records: &[WorkerRecord]) -> Vec<Discontinuity> {
    let mut expected = 1;
    let mut found = Vec::new();

    for record in records {
        if record.seq != expected {
            found.push(Discontinuity {
                expected,
                found: record.seq,
            });
        }

        expected = record.seq + 1;
    }

    found
}

/// The record's `kind`, as the worker wrote it.
fn record_kind(event: &WorkerEvent) -> String {
    let kind = serde_json::to_value(event)
        .ok()
        .and_then(|value| value.get("kind")?.as_str().map(str::to_owned));

    kind.unwrap_or_else(|| "unknown".into())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
enum HookSummary {
    /// The hooks the worker requested, and the state of each before the game resumed.
    Reported {
        requested: Vec<String>,
        installed: Vec<String>,
        missing: Vec<MissingHook>,
    },
    /// The worker recorded no hook state; `requested` is what it asked for, if anything.
    Unavailable {
        reason: String,
        requested: Vec<String>,
    },
}

impl HookSummary {
    fn new(records: &[WorkerRecord]) -> Self {
        let requested = records.iter().find_map(|record| match &record.event {
            WorkerEvent::HooksRequested { hooks } => Some(hooks.clone()),
            _ => None,
        });
        let Some(requested) = requested else {
            return Self::Unavailable {
                reason: "the worker stopped before it requested hooks".into(),
                requested: Vec::new(),
            };
        };
        let states = records.iter().find_map(|record| match &record.event {
            WorkerEvent::HooksActiveBeforeResume { hooks } => Some(hooks),
            _ => None,
        });
        let Some(states) = states else {
            return Self::Unavailable {
                reason: "the worker stopped before its hook state record".into(),
                requested,
            };
        };
        let (installed, missing) = requested.iter().partition::<Vec<_>, _>(|name| {
            states.get(*name).is_some_and(Hook::active_before_resume)
        });
        let missing = missing
            .into_iter()
            .map(|name| MissingHook {
                hook: name.clone(),
                state: HookState::of(states.get(name)),
            })
            .collect();

        Self::Reported {
            installed: installed.into_iter().cloned().collect(),
            requested,
            missing,
        }
    }
}

#[derive(Debug, Serialize)]
struct MissingHook {
    hook: String,
    state: HookState,
}

/// Why a requested hook was not active before resume.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum HookState {
    Absent,
    Disabled,
    /// Not at exactly one resolved location.
    Unresolved,
    HitBeforeResume,
}

impl HookState {
    fn of(hook: Option<&Hook>) -> Self {
        match hook {
            None => Self::Absent,
            Some(hook) if !hook.enabled => Self::Disabled,
            Some(hook) if hook.locations != 1 || hook.resolved != 1 => Self::Unresolved,
            Some(_) => Self::HitBeforeResume,
        }
    }
}

#[derive(Debug, Serialize)]
struct ObservationSummary {
    registries: RegistriesSummary,
    fixture: FixtureSummary,
    modifiers: ModifierSummary,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
enum RegistriesSummary {
    Observed(BTreeMap<String, RegistrySummary>),
    Unavailable(String),
}

#[derive(Debug, Clone, Serialize)]
struct RegistrySummary {
    observed: Observed,
    items: usize,
    diagnostics: Sample<String>,
}

impl RegistrySummary {
    fn new(items: &RegistryItems) -> Self {
        Self {
            observed: items.observed,
            items: items.items.len(),
            diagnostics: Sample::texts(&items.diagnostics),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
enum FixtureSummary {
    NotRequested,
    Unavailable(String),
    /// The reducer returned no answer.
    Error(String),
    Observed {
        completeness: Completeness,
        gaps: Sample<String>,
        diagnostic_coverage: DiagnosticCoverage,
        questions: Vec<QuestionSummary>,
    },
}

impl FixtureSummary {
    fn observed(answer: &crate::Answer<FixtureObservation>) -> Self {
        let observation = &answer.value;
        let gaps: Vec<_> = answer
            .gaps
            .iter()
            .map(|gap| format!("{:?}: {}", gap.kind, gap.detail))
            .collect();
        let questions = observation
            .field_outcomes
            .iter()
            .map(|outcome| QuestionSummary::new(outcome, observation))
            .collect();

        Self::Observed {
            completeness: answer.completeness,
            gaps: Sample::texts(&gaps),
            diagnostic_coverage: observation.diagnostic_coverage.clone(),
            questions,
        }
    }
}

/// The four dimensions of one field question, each reported on its own.
#[derive(Debug, Clone, Serialize)]
struct QuestionSummary {
    question: String,
    parsing: ParsingSummary,
    storage: StorageSummary,
    /// The question's joined diagnostics, counted by the engine stage that reported them.
    validation: BTreeMap<String, usize>,
    runtime: FixtureRuntime,
}

impl QuestionSummary {
    fn new(outcome: &crate::FixtureFieldOutcome, observation: &FixtureObservation) -> Self {
        let question = &outcome.question;
        let mut validation = BTreeMap::new();

        for diagnostic in outcome
            .diagnostics
            .iter()
            .filter_map(|index| observation.diagnostics.get(*index))
        {
            *validation.entry(diagnostic.stage.clone()).or_default() += 1;
        }

        Self {
            question: format!(
                "{}/{}.{}",
                question.registry, question.definition, question.field
            ),
            parsing: ParsingSummary::new(&outcome.parsing),
            storage: StorageSummary::new(&outcome.storage),
            validation,
            runtime: outcome.runtime.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
enum ParsingSummary {
    NotRequested,
    Unavailable(String),
    Observed {
        entries: usize,
        /// Entries whose return was not witnessed.
        unmatched_entries: usize,
        completeness: Completeness,
    },
}

impl ParsingSummary {
    fn new(parsing: &FixtureParsing) -> Self {
        match parsing {
            FixtureParsing::NotRequested => Self::NotRequested,
            FixtureParsing::Unavailable(reason) => Self::Unavailable(cut(reason)),
            FixtureParsing::Observed {
                occurrences,
                completeness,
            } => Self::Observed {
                entries: occurrences.len(),
                unmatched_entries: occurrences
                    .iter()
                    .filter(|occurrence| occurrence.return_line.is_none())
                    .count(),
                completeness: *completeness,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
enum StorageSummary {
    Unavailable(String),
    Observed {
        occurrences: usize,
        completeness: Completeness,
    },
}

impl StorageSummary {
    fn new(storage: &FixtureStorage) -> Self {
        match storage {
            FixtureStorage::Unavailable(reason) => Self::Unavailable(cut(reason)),
            FixtureStorage::Observed {
                occurrences,
                completeness,
                ..
            } => Self::Observed {
                occurrences: occurrences.len(),
                completeness: *completeness,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
enum ModifierSummary {
    NotRequested,
    Unavailable(String),
    /// The reducer returned no table.
    Error(String),
    Observed {
        entries: usize,
    },
}

/// The first few items of a list, and how many were left out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Sample<T> {
    shown: Vec<T>,
    omitted: usize,
}

impl<T> Sample<T> {
    fn new(mut items: Vec<T>) -> Self {
        let omitted = items.len().saturating_sub(SAMPLES);

        items.truncate(SAMPLES);
        Self {
            shown: items,
            omitted,
        }
    }
}

impl Sample<String> {
    fn texts(texts: &[String]) -> Self {
        Self::new(texts.iter().map(|text| cut(text)).collect())
    }
}

/// `text`, cut to `TEXT` characters.
fn cut(text: &str) -> String {
    if text.chars().count() <= TEXT {
        return text.to_owned();
    }

    let kept: String = text.chars().take(TEXT).collect();

    format!("{kept}…")
}

fn milliseconds(from: Instant, to: Instant) -> u64 {
    u64::try_from(to.saturating_duration_since(from).as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Answer, Basis, DiagnosticJoin, DiagnosticWindow, FixtureDiagnostic, FixtureFieldOutcome,
        FixtureFieldQuestion, Gap, GapKind, ParsedFieldOccurrence, Reader, ReaderKind, Source,
        StoredFieldOccurrence,
    };
    use serde_json::{Value, json};
    use std::time::Duration;

    fn checkpoint() -> Value {
        json!({
            "attempt": "unit", "game": 9, "phase": "world-update", "context": "world day 2",
            "thread": 7, "operation": "fast_forward", "details": {"expected_pc": "0x1000"},
            "elapsed_milliseconds": 400, "deadline_milliseconds": 500,
            "last_attempted_call": {"ordinal": 2, "operation": "fast_forward"},
            "last_completed_call": {"ordinal": 1, "operation": "fast_forward"},
            "hooks": [], "failure": null
        })
    }

    #[test]
    fn interrupted_call_reports_unknown_cause_and_distinct_completion() {
        let directory = tempfile::tempdir().unwrap();
        files::write_json(
            &directory.path().join("worker-diagnostics.json"),
            &checkpoint(),
        )
        .unwrap();
        let context = read_worker_diagnostics(directory.path(), "unit");
        let report = SessionReport {
            attempt: "unit".into(),
            outcome: SessionOutcome::WorkerLost,
            disposal: Disposal::Confirmed,
            reservation_resolved: true,
            diagnostics: vec![],
        };
        let summary =
            serde_json::to_value(WorkerDiagnosticSummary::new(&report, &[], context)).unwrap();
        assert_eq!(
            summary["reason"],
            "cause unavailable; last witnessed operation: fast_forward"
        );
        assert_eq!(summary["context"]["last_attempted_call"]["ordinal"], 2);
        assert_eq!(summary["context"]["last_completed_call"]["ordinal"], 1);
        assert!(summary["context"]["failure"].is_null());
    }

    #[test]
    fn checkpoint_damage_is_unavailable_and_never_prevents_the_summary() {
        let directory = tempfile::tempdir().unwrap();
        assert!(read_worker_diagnostics(directory.path(), "unit").is_err());
        let path = directory.path().join("worker-diagnostics.json");
        for bytes in [
            b"{".to_vec(),
            vec![b' '; protocol::observation::MAX_WORKER_DIAGNOSTICS + 1],
        ] {
            std::fs::write(&path, bytes).unwrap();
            assert!(read_worker_diagnostics(directory.path(), "unit").is_err());
        }
        std::fs::write(&path, serde_json::to_vec(&checkpoint()).unwrap()).unwrap();
        assert!(
            read_worker_diagnostics(directory.path(), "foreign")
                .unwrap_err()
                .contains("another attempt")
        );
    }

    #[test]
    fn checkpoint_text_and_hook_samples_are_bounded() {
        let directory = tempfile::tempdir().unwrap();
        let mut context = checkpoint();
        context["operation"] = json!("x".repeat(1000));
        context["hooks"] = json!(
            (0..20)
                .map(|index| json!({
                    "name": index.to_string(), "enabled": false, "locations": 0, "resolved": 0,
                }))
                .collect::<Vec<_>>()
        );
        files::write_json(&directory.path().join("worker-diagnostics.json"), &context).unwrap();
        let context = read_worker_diagnostics(directory.path(), "unit").unwrap();
        assert_eq!(context.hooks.len(), SAMPLES);
        assert_eq!(context.operation.chars().count(), TEXT + 1);
    }

    /// Worker records with these sequence numbers and events.
    fn records(rows: &[(u64, Value)]) -> Vec<WorkerRecord> {
        rows.iter()
            .map(|(seq, row)| {
                let mut row = row.clone();
                row["seq"] = json!(seq);
                row["run"] = json!("unit");
                serde_json::from_value(row).unwrap()
            })
            .collect()
    }

    fn finished() -> Value {
        json!({"kind":"worker-finished"})
    }

    fn hook(enabled: bool, locations: u64, hits: u64) -> Value {
        json!({"enabled":enabled,"locations":locations,"resolved":locations,"hits":hits})
    }

    #[test]
    fn hook_states_name_why_each_requested_hook_was_not_active() {
        let records = records(&[
            (
                1,
                json!({"kind":"hooks-requested","hooks":["active","late","twice","hit","missing"]}),
            ),
            (
                2,
                json!({"kind":"hooks-active-before-resume","hooks":{
                    "active":hook(true, 1, 0),
                    "late":hook(false, 1, 0),
                    "twice":hook(true, 2, 0),
                    "hit":hook(true, 1, 1)}}),
            ),
        ]);

        assert_eq!(
            serde_json::to_value(HookSummary::new(&records)).unwrap(),
            json!({"reported":{
                "requested":["active","late","twice","hit","missing"],
                "installed":["active"],
                "missing":[
                    {"hook":"late","state":"disabled"},
                    {"hook":"twice","state":"unresolved"},
                    {"hook":"hit","state":"hit-before-resume"},
                    {"hook":"missing","state":"absent"}]}})
        );
    }

    #[test]
    fn hook_states_are_unavailable_when_the_worker_recorded_none() {
        let requested_only = records(&[(1, json!({"kind":"hooks-requested","hooks":["a"]}))]);

        assert_eq!(
            serde_json::to_value(HookSummary::new(&requested_only)).unwrap(),
            json!({"unavailable":{
                "reason":"the worker stopped before its hook state record",
                "requested":["a"]}})
        );
        assert_eq!(
            serde_json::to_value(HookSummary::new(&[])).unwrap(),
            json!({"unavailable":{
                "reason":"the worker stopped before it requested hooks",
                "requested":[]}})
        );
    }

    #[test]
    fn a_sequence_hole_is_reported_where_it_is_without_a_lost_count() {
        let records = records(&[(1, finished()), (2, finished()), (5, finished())]);
        let summary = WorkerSummary::new(&records, StreamState::Intact);

        assert_eq!(
            serde_json::to_value(summary).unwrap(),
            json!({
                "stream":"intact",
                "records":3,
                "last_record":{"seq":5,"kind":"worker-finished"},
                "sequence_discontinuities":{"shown":[{"expected":3,"found":5}],"omitted":0}})
        );
    }

    #[test]
    fn many_holes_keep_a_few_samples_and_count_the_rest() {
        let rows: Vec<_> = (0..11).map(|index| (index * 2 + 1, finished())).collect();
        let summary = WorkerSummary::new(&records(&rows), StreamState::Intact);

        assert_eq!(summary.sequence_discontinuities.shown.len(), SAMPLES);
        assert_eq!(summary.sequence_discontinuities.omitted, 2);
        assert_eq!(
            summary.sequence_discontinuities.shown[0],
            Discontinuity {
                expected: 2,
                found: 3
            }
        );
    }

    #[test]
    fn a_damaged_tail_after_a_terminal_invents_no_hole() {
        let raw = concat!(
            r#"{"seq":1,"run":"unit","kind":"hooks-requested","hooks":[]}"#,
            "\n",
            r#"{"seq":2,"run":"unit","kind":"registry-end","name":"common/traditions","owner":"0x1000","count":0,"producerLastSequence":2}"#,
            "\n",
            r#"{"seq":3,"run":"unit","kind":"worker-finished"}"#,
            "\n",
            r#"{"seq":4,"#,
        );
        let (stripped, _) = event_stream::read_worker_stream(raw.as_bytes(), "unit");
        let (written, damage) = event_stream::parse_worker_stream(raw.as_bytes(), "unit");
        let summary = WorkerSummary::new(
            &written,
            damage.map_or(StreamState::Intact, StreamState::Damaged),
        );

        assert_eq!(discontinuities(&stripped).len(), 1);
        assert_eq!(summary.records, 3);
        assert!(summary.sequence_discontinuities.shown.is_empty());
        assert_eq!(
            serde_json::to_value(&summary.stream).unwrap(),
            json!({"damaged":"Worker stream record 4 is damaged, partial, or from another session"})
        );
    }

    #[test]
    fn registry_diagnostics_are_sampled_and_cut() {
        let mut diagnostics: Vec<String> = (0..9)
            .map(|index| format!("Sequence gap {index}"))
            .collect();
        diagnostics[0] = "x".repeat(TEXT + 10);
        let items = RegistryItems {
            items: vec!["one".into(), "two".into()],
            observed: Observed::Partial,
            diagnostics,
        };
        let summary = RegistrySummary::new(&items);

        assert_eq!(summary.observed, Observed::Partial);
        assert_eq!(summary.items, 2);
        assert_eq!(summary.diagnostics.shown.len(), SAMPLES);
        assert_eq!(summary.diagnostics.omitted, 1);
        assert_eq!(
            summary.diagnostics.shown[0],
            format!("{}…", "x".repeat(TEXT))
        );
        assert_eq!(summary.diagnostics.shown[1], "Sequence gap 1");
    }

    fn outcome(
        field: &str,
        parsing: FixtureParsing,
        storage: FixtureStorage,
        diagnostics: Vec<usize>,
        runtime: FixtureRuntime,
    ) -> FixtureFieldOutcome {
        FixtureFieldOutcome {
            question: FixtureFieldQuestion::new("common/tradition_categories", "atlas", field),
            file: "common/tradition_categories/atlas.txt".into(),
            owner: None,
            definition_line: None,
            reader: Reader {
                numeric: crate::GrammarProperty::Unresolved,
                scoped_operand: crate::GrammarProperty::Unresolved,
                id: None,
                kind: ReaderKind::Unknown,
                family: crate::BlockFamily::Unknown,
            },
            parsing,
            storage,
            diagnostics,
            runtime,
        }
    }

    fn diagnostic(stage: &str) -> FixtureDiagnostic {
        FixtureDiagnostic {
            text: "unexpected token".into(),
            stage: stage.into(),
            join: DiagnosticJoin::Unavailable("no line".into()),
        }
    }

    fn fixture_answer(coverage: DiagnosticCoverage) -> Answer<FixtureObservation> {
        let occurrence = |occurrence, return_line| ParsedFieldOccurrence {
            line: 2,
            occurrence,
            return_line,
        };
        let parsed = outcome(
            "tree_template",
            FixtureParsing::Observed {
                occurrences: vec![occurrence(1, Some(2)), occurrence(2, None)],
                completeness: Completeness::Partial,
            },
            FixtureStorage::Unavailable("no storage decoder".into()),
            vec![0, 1],
            FixtureRuntime::NotRequested,
        );
        let stored = outcome(
            "traditions",
            FixtureParsing::NotRequested,
            FixtureStorage::Observed {
                occurrences: vec![StoredFieldOccurrence {
                    line: 3,
                    occurrence: 1,
                    value: crate::FixtureValue::String("x".into()),
                }],
                final_value: Some(crate::FixtureValue::String("x".into())),
                completeness: Completeness::Complete,
            },
            vec![],
            FixtureRuntime::Unavailable("outside the initial load".into()),
        );

        Answer {
            value: FixtureObservation {
                registration_entries: vec![],
                field_reads: vec![],
                field_outcomes: vec![parsed, stored],
                diagnostics: vec![
                    diagnostic("engine-parser-log"),
                    diagnostic("engine-validation-log"),
                ],
                diagnostic_coverage: coverage,
            },
            completeness: Completeness::Partial,
            gaps: vec![Gap {
                kind: GapKind::IncompleteObservation,
                subject: None,
                detail: "A parser return has no matching open invocation".into(),
            }],
            source: Source::new(
                crate::BuildId("unit".into()),
                "unit/v1",
                Basis::LiveObservation,
            ),
        }
    }

    #[test]
    fn fixture_dimensions_are_reported_separately() {
        let window = DiagnosticWindow::FixtureFileLoadAndValidation;
        let answer = fixture_answer(DiagnosticCoverage::Complete { window });

        assert_eq!(
            serde_json::to_value(FixtureSummary::observed(&answer)).unwrap(),
            json!({"observed":{
                "completeness":"Partial",
                "gaps":{"shown":["IncompleteObservation: A parser return has no matching open invocation"],"omitted":0},
                "diagnostic_coverage":{"Complete":{"window":"FixtureFileLoadAndValidation"}},
                "questions":[
                    {"question":"common/tradition_categories/atlas.tree_template",
                     "parsing":{"observed":{"entries":2,"unmatched_entries":1,"completeness":"Partial"}},
                     "storage":{"unavailable":"no storage decoder"},
                     "validation":{"engine-parser-log":1,"engine-validation-log":1},
                     "runtime":"NotRequested"},
                    {"question":"common/tradition_categories/atlas.traditions",
                     "parsing":"not-requested",
                     "storage":{"observed":{"occurrences":1,"completeness":"Complete"}},
                     "validation":{},
                     "runtime":{"Unavailable":"outside the initial load"}}]}})
        );
    }

    #[test]
    fn equal_diagnostic_counts_from_different_windows_differ() {
        let summary = |window| {
            let answer = fixture_answer(DiagnosticCoverage::Complete { window });

            serde_json::to_value(FixtureSummary::observed(&answer)).unwrap()
        };

        assert_ne!(
            summary(DiagnosticWindow::FixtureFileLoad),
            summary(DiagnosticWindow::FixtureFileLoadAndValidation)
        );
    }

    #[test]
    fn a_missing_answer_differs_from_one_not_requested() {
        let registries = BTreeMap::new();
        let error: Result<_, _> = Err(crate::Error::Observation {
            operation: crate::Operation::ObserveFixture,
            reason: "Fixture hook activation before resume was not established".into(),
        });
        let table = Ok(ObservedModifiers {
            entries: vec![],
            registries: BTreeMap::new(),
        });
        let paused = PausedSummary::new(&registries, Some(&error), Some(&table));
        let absent = PausedSummary::new(&registries, None, None);

        assert!(
            matches!(paused.fixture, FixtureSummary::Error(reason) if reason.contains("activation"))
        );
        assert!(matches!(
            paused.modifiers,
            ModifierSummary::Observed { entries: 0 }
        ));
        assert!(matches!(absent.fixture, FixtureSummary::NotRequested));
        assert!(matches!(absent.modifiers, ModifierSummary::NotRequested));
    }

    /// A record whose phases completed at these offsets, in milliseconds, from its start.
    fn run(
        completed: &[(Phase, u64)],
        session_ended: Option<u64>,
        cleanup_ended: Option<u64>,
    ) -> RunRecord {
        let started = Instant::now();
        let at = |milliseconds| started + Duration::from_millis(milliseconds);

        RunRecord {
            started,
            completed: completed
                .iter()
                .map(|(phase, offset)| (*phase, at(*offset)))
                .collect(),
            session_ended: session_ended.map(at),
            cleanup_ended: cleanup_ended.map(at),
            fixture_requested: true,
            modifiers_requested: false,
            paused: None,
        }
    }

    #[test]
    fn the_running_phase_is_interrupted_and_cleanup_is_timed_apart() {
        let timing = run(&[(Phase::Setup, 5)], Some(20), Some(50)).timing();

        assert_eq!(
            serde_json::to_value(timing).unwrap(),
            json!({
                "clock":CLOCK,
                "phases":[
                    {"phase":"setup","state":"completed","milliseconds":5},
                    {"phase":"worker-start","state":"interrupted","milliseconds":15},
                    {"phase":"awaiting-pause","state":"not-reached"},
                    {"phase":"paused","state":"not-reached"}],
                "last_completed_phase":"setup",
                "cleanup_milliseconds":30})
        );
    }

    #[test]
    fn a_timeout_in_setup_differs_from_one_while_awaiting_the_pause() {
        let setup = run(&[], Some(30_000), Some(30_010)).timing();
        let observing = run(
            &[(Phase::Setup, 800), (Phase::WorkerStart, 850)],
            Some(170_850),
            Some(172_000),
        )
        .timing();

        assert_eq!(setup.phases[0].state, PhaseState::Interrupted);
        assert_eq!(setup.phases[0].milliseconds, Some(30_000));
        assert_eq!(setup.last_completed_phase, None);
        assert_eq!(observing.phases[2].state, PhaseState::Interrupted);
        assert_eq!(observing.phases[2].milliseconds, Some(170_000));
        assert_eq!(observing.last_completed_phase, Some(Phase::WorkerStart));
        assert_eq!(observing.cleanup_milliseconds, Some(1_150));
    }

    #[test]
    fn a_paused_session_completes_its_pause_when_it_ends() {
        let mut record = run(
            &[
                (Phase::Setup, 1),
                (Phase::WorkerStart, 2),
                (Phase::AwaitingPause, 3),
            ],
            None,
            None,
        );
        record.end_session();
        record.end_cleanup();
        let timing = record.timing();

        assert!(
            timing
                .phases
                .iter()
                .all(|phase| phase.state == PhaseState::Completed)
        );
        assert_eq!(timing.last_completed_phase, Some(Phase::Paused));
    }

    #[test]
    fn before_the_pause_requested_observations_are_unavailable() {
        let observations = serde_json::to_value(run(&[], None, None).observations()).unwrap();

        assert_eq!(
            observations,
            json!({
                "registries":{"unavailable":BEFORE_PAUSE},
                "fixture":{"unavailable":BEFORE_PAUSE},
                "modifiers":"not-requested"})
        );
    }
}
