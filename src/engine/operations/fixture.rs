//! Reduce the bounded fixture window. Native joins addresses internally, then replaces them
//! with session-local owner identities. Only witnessed read entries leave this module.
//!
//! The producer numbers the occurrences of a field. A record whose number skips ahead of the last
//! kept one stays, with a gap for the lost occurrence; a repeated or earlier number is refused.
//!
//! A repeated definition key, in one file or across files, keeps each construction. Field events
//! join the owner of the key's latest construction, and the final value is read once, after the
//! last file, from the one owner of all of them.
use super::event_stream::{self, OwnerEvent};
use crate::protocol::{
    hooks,
    observation::{
        FixtureEvent, FixtureQuestionSetup, FixtureQuestionStorage, WorkerEvent, WorkerRecord,
        diagnostic_stage,
    },
};
use crate::{
    Answer, Basis, BuildId, Completeness, DiagnosticCoverage, DiagnosticJoin, DiagnosticWindow,
    Error, FixtureDefinition, FixtureDiagnostic, FixtureFieldOutcome, FixtureObservation,
    FixtureOwnerId, FixtureParsing, FixtureRequest, FixtureStorage, FixtureValue, Gap, GapKind,
    GapSubject, Operation, ParsedFieldOccurrence, Source, StoredFieldOccurrence,
};
use std::collections::{BTreeMap, BTreeSet};

/// The fixture hooks that must be active before resume: the minimum that the request implies,
/// and every fixture hook that the worker names as requested. The worker selects its hooks from
/// the bound setup, so a hook that only that setup needs (an inline loader's return and nested
/// hooks) is required by its name.
fn required_hooks<'a>(request: &FixtureRequest, records: &'a [WorkerRecord]) -> Vec<&'a str> {
    let mut required = vec![hooks::FIXTURE_LOAD];
    let diagnostics_requested = request
        .field_questions
        .iter()
        .any(|question| question.diagnostics);
    if diagnostics_requested {
        required.extend([
            hooks::FIXTURE_LOG,
            hooks::FIXTURE_UNFORMATTED_LOG,
            hooks::FIXTURE_STREAM_LOG,
            hooks::FIXTURE_SOURCED_LOG,
        ]);
    }
    if request.window == crate::FixtureWindow::InitialFileLoadAndValidation {
        required.push(hooks::FIXTURE_VALIDATED);
    }
    if !request.field_questions.is_empty() {
        required.extend([
            hooks::FIXTURE_CONSTRUCTOR,
            hooks::FIXTURE_READER,
            hooks::FIXTURE_MEMBER,
        ]);
        if diagnostics_requested {
            required.extend([hooks::FIXTURE_MALFORMED, hooks::FIXTURE_UNEXPECTED]);
        }
    }
    let requested_by_worker = records
        .iter()
        .filter_map(|record| match &record.event {
            WorkerEvent::HooksRequested { hooks } => Some(hooks),
            _ => None,
        })
        .flatten()
        .map(String::as_str)
        .filter(|name| name.starts_with(hooks::FIXTURE));
    required.extend(requested_by_worker);
    required
}

/// Reduce the fixture window of `records`. `questions` is the setup that the supervisor resolved
/// for each of the request's questions: it states the storage of each one, so the worker does not.
pub(crate) fn reduce(
    request: &FixtureRequest,
    questions: &[FixtureQuestionSetup],
    records: &[WorkerRecord],
    owner_events: &[OwnerEvent],
    build: BuildId,
) -> Result<Answer<FixtureObservation>, Error> {
    if questions.len() != request.field_questions.len() {
        return Err(Error::Observation {
            operation: Operation::ObserveFixture,
            reason: "The fixture setup does not match the request's questions".into(),
        });
    }
    let required = required_hooks(request, records);
    let Some((thread, resumed)) = event_stream::activation(records, owner_events, &required) else {
        return Err(Error::Observation {
            operation: Operation::ObserveFixture,
            reason: "Fixture hook activation before resume was not established".into(),
        });
    };
    let mut window = Window::new(request, questions, thread, resumed);
    for record in records {
        // The terminal closes this question's window. Later registry record loss does not
        // revoke it; damaged transport still removes the terminal in read_worker_stream.
        if !window.ended && record.seq != window.next_sequence {
            window.window_gap("Observation records are missing or out of order");
        }
        window.next_sequence = record.seq.saturating_add(1);
        match &record.event {
            WorkerEvent::Fixture { event } => window.accept(record, event),
            WorkerEvent::CallbackError { .. }
            | WorkerEvent::NativeException { .. }
            | WorkerEvent::CapabilityUnavailable { .. }
            | WorkerEvent::EarlyActivationUnavailable { .. }
                if !window.ended =>
            {
                window.window_gap("The worker could not finish the fixture observation");
            }
            _ => {}
        }
    }
    if !window.ended {
        window.window_gap("The fixture terminal is missing");
    }
    for file in request.files.keys() {
        if !window.loads.contains(file) {
            window.file_gap(file, "The fixture file's load window is missing");
        }
    }
    if !window.returned {
        window.window_gap("The matching fixture loader return is missing");
    }
    for index in 0..request.field_questions.len() as u64 {
        if !window.field_terminals.contains_key(&index) {
            window.field_gap(index, "The field observation terminal is missing");
        }
    }
    if window.diagnostics_requested && window.diagnostic_terminal.is_none() {
        window.diagnostic_gap("The parser diagnostic terminal is missing");
    }
    Ok(window.finish(build))
}

struct DefinitionState {
    native_owner: String,
    public_owner: FixtureOwnerId,
    file: String,
    line: u64,
    /// The position of `file` in the load order, which orders constructions across files.
    load: usize,
}

struct FieldTerminalState {
    final_value: Option<FixtureValue>,
    unavailable: Option<String>,
}

struct RawDiagnostic {
    text: String,
    stage: String,
    file: Option<String>,
    line: Option<u64>,
    definition: Option<String>,
    field: Option<String>,
    occurrence: Option<u64>,
    parent_field: Option<String>,
}

/// How the definition, field and occurrence of a parser diagnostic join a field question.
enum QuestionLink {
    /// The diagnostic names no definition, field or occurrence.
    Unscoped,
    /// The diagnostic names a stored occurrence of this question.
    Question(u64),
    /// The diagnostic names a field in part, or names no stored occurrence of a question.
    Unmatched,
}

enum DiagnosticTerminalState {
    Complete,
    Unavailable(String),
}

fn parsed_line(occurrence: &ParsedFieldOccurrence, file: &str, line: u64) -> bool {
    file == occurrence.file
        && line >= occurrence.line
        && occurrence.return_line.is_some_and(|end| line <= end)
}

struct Window<'a> {
    request: &'a FixtureRequest,
    questions: &'a [FixtureQuestionSetup],
    thread: u64,
    resumed: u64,
    next_sequence: u64,
    /// Fixture files whose load started, in load order.
    loads: Vec<String>,
    /// The fixture file that is loading now.
    current: Option<String>,
    /// Whether every requested file has returned.
    returned: bool,
    validated: bool,
    ended: bool,
    owner_ids: BTreeMap<String, FixtureOwnerId>,
    next_owner_id: u64,
    definitions: BTreeMap<String, Vec<DefinitionState>>,
    occurrences: BTreeMap<u64, Vec<StoredFieldOccurrence>>,
    parse_occurrences: BTreeMap<u64, Vec<ParsedFieldOccurrence>>,
    parsing_terminals: BTreeMap<u64, Option<String>>,
    parsing_issues: BTreeSet<u64>,
    field_terminals: BTreeMap<u64, FieldTerminalState>,
    diagnostic_indices: BTreeMap<u64, Vec<usize>>,
    field_issues: BTreeSet<u64>,
    raw_diagnostics: Vec<RawDiagnostic>,
    diagnostics_requested: bool,
    diagnostic_terminal: Option<DiagnosticTerminalState>,
    diagnostic_issue: bool,
    stream_intact: bool,
    value: FixtureObservation,
    gaps: Vec<Gap>,
}

impl<'a> Window<'a> {
    fn new(
        request: &'a FixtureRequest,
        questions: &'a [FixtureQuestionSetup],
        thread: u64,
        resumed: u64,
    ) -> Self {
        Self {
            request,
            questions,
            thread,
            resumed,
            next_sequence: 1,
            loads: Vec::new(),
            current: None,
            returned: false,
            validated: false,
            ended: false,
            owner_ids: BTreeMap::new(),
            next_owner_id: 1,
            definitions: BTreeMap::new(),
            occurrences: BTreeMap::new(),
            parse_occurrences: BTreeMap::new(),
            parsing_terminals: BTreeMap::new(),
            parsing_issues: BTreeSet::new(),
            field_terminals: BTreeMap::new(),
            diagnostic_indices: BTreeMap::new(),
            field_issues: BTreeSet::new(),
            raw_diagnostics: Vec::new(),
            diagnostics_requested: request
                .field_questions
                .iter()
                .any(|question| question.diagnostics),
            diagnostic_terminal: None,
            diagnostic_issue: false,
            stream_intact: true,
            value: FixtureObservation::default(),
            gaps: Vec::new(),
        }
    }

    fn push_gap(&mut self, kind: GapKind, subject: Option<GapSubject>, detail: &str) {
        let gap = Gap {
            kind,
            subject,
            detail: detail.into(),
        };
        if !self.gaps.contains(&gap) {
            self.gaps.push(gap);
        }
    }

    fn window_gap(&mut self, detail: &str) {
        self.incomplete_window(GapSubject::registry(self.request.registry()), detail);
    }

    /// A window gap that one fixture file owns.
    fn file_gap(&mut self, file: &str, detail: &str) {
        self.incomplete_window(GapSubject::fixture_file(file), detail);
    }

    fn incomplete_window(&mut self, subject: GapSubject, detail: &str) {
        self.stream_intact = false;
        self.field_issues
            .extend(0..self.request.field_questions.len() as u64);
        if self.diagnostics_requested {
            self.diagnostic_issue = true;
        }
        self.push_gap(GapKind::IncompleteObservation, Some(subject), detail);
    }

    fn field_gap(&mut self, question: u64, detail: &str) {
        self.field_issues.insert(question);
        let subject = self
            .request
            .field_questions
            .get(question as usize)
            .map(|question| GapSubject::field(question.field.clone()))
            .or_else(|| Some(GapSubject::registry(self.request.registry())));
        self.push_gap(GapKind::IncompleteObservation, subject, detail);
    }

    fn diagnostic_gap(&mut self, detail: &str) {
        self.diagnostic_issue = true;
        self.push_gap(
            GapKind::IncompleteObservation,
            Some(GapSubject::registry(self.request.registry())),
            detail,
        );
    }

    fn parsing_gap(&mut self, question: u64, detail: &str) {
        self.parsing_issues.insert(question);
        let subject = self
            .request
            .field_questions
            .get(question as usize)
            .map(|question| GapSubject::field(question.field.clone()));
        self.push_gap(GapKind::IncompleteObservation, subject, detail);
    }

    fn public_owner(&mut self, native_owner: &str) -> FixtureOwnerId {
        if let Some(owner) = self.owner_ids.get(native_owner) {
            return owner.clone();
        }
        let owner = FixtureOwnerId(self.next_owner_id);
        self.next_owner_id += 1;
        self.owner_ids.insert(native_owner.into(), owner.clone());
        owner
    }

    /// Whether `line` is a line of a requested fixture file.
    fn is_fixture_line(&self, file: &str, line: u64) -> bool {
        self.request
            .files
            .get(file)
            .is_some_and(|text| line > 0 && line <= text.lines().count() as u64)
    }

    /// Whether `line` is a line of the fixture file that is loading now.
    fn is_loading_line(&self, file: &str, line: u64) -> bool {
        self.current.as_deref() == Some(file) && self.is_fixture_line(file, line)
    }

    /// Whether the last requested fixture file is loading, the window of the terminals that read
    /// each question's final value.
    fn loading_last_file(&self) -> bool {
        self.current.is_some() && self.loads.len() == self.request.files.len()
    }

    /// Whether the first fixture file started to load.
    fn started(&self) -> bool {
        !self.loads.is_empty()
    }

    fn accept(&mut self, record: &WorkerRecord, event: &FixtureEvent) {
        if let FixtureEvent::Unavailable { .. } = event {
            self.window_gap("A requested fixture observation was unavailable");
            return;
        }
        let sourced_engine_log = matches!(event, FixtureEvent::Diagnostic {
                stage, file: Some(file), definition: None, field: None, occurrence: None, ..
            } if (stage == diagnostic_stage::ENGINE_PARSER
                || (stage == diagnostic_stage::ENGINE_VALIDATION && self.validation_requested()))
                && self.request.files.contains_key(file));
        let matching_thread = record.thread == Some(self.thread)
            || (sourced_engine_log && record.thread.is_some_and(|thread| thread != 0));
        if !matching_thread || record.seq <= self.resumed || self.ended {
            self.window_gap("A fixture event has no matching activation, thread or open window");
            return;
        }
        match event {
            FixtureEvent::LoadStart { file } => self.accept_load_start(file),
            FixtureEvent::Definition {
                file,
                line,
                definition,
                owner,
            } => self.accept_definition(file, *line, definition, owner),
            FixtureEvent::FieldStorage { .. } => self.accept_field_storage(event),
            FixtureEvent::FieldParse { .. } => self.accept_field_parse(event),
            FixtureEvent::ParsingTerminal {
                question,
                count,
                unavailable,
            } => self.accept_parsing_terminal(*question, *count, unavailable),
            FixtureEvent::Diagnostic { .. } => self.accept_diagnostic(event),
            FixtureEvent::FieldTerminal { .. } => self.accept_field_terminal(event),
            FixtureEvent::DiagnosticsTerminal { count } => self.accept_diagnostics_terminal(*count),
            FixtureEvent::DiagnosticsUnavailable { reason } => {
                self.accept_diagnostics_unavailable(reason)
            }
            FixtureEvent::ValidationComplete => {
                if !self.validation_requested()
                    || !self.returned
                    || self.validated
                    || self.diagnostic_terminal.is_some()
                {
                    self.diagnostic_gap(
                        "The validation boundary has no matching completed fixture load",
                    );
                } else {
                    self.validated = true;
                }
            }
            FixtureEvent::LoadReturned { file } => self.accept_load_returned(file),
            FixtureEvent::End {
                field_outcomes,
                diagnostics,
                producer_last_sequence,
            } => self.accept_end(
                record.seq,
                *field_outcomes,
                *diagnostics,
                *producer_last_sequence,
            ),
            FixtureEvent::Unavailable { .. } => unreachable!(),
        }
    }

    fn accept_load_start(&mut self, file: &str) {
        if self.current.is_some()
            || !self.request.files.contains_key(file)
            || self.loads.iter().any(|loaded| loaded == file)
        {
            self.window_gap("The fixture loader entry is repeated or names another file");
            return;
        }
        self.loads.push(file.into());
        self.current = Some(file.into());
    }

    fn accept_definition(&mut self, file: &str, line: u64, definition: &str, owner: &str) {
        // One line can hold several definitions, so a repeat may share its line; record loss and
        // replay are the sequence check's.
        let load = self.loads.len().saturating_sub(1);
        let in_source_order = self
            .definitions
            .get(definition)
            .and_then(|constructions| constructions.last())
            .is_none_or(|last| (load, line) >= (last.load, last.line));
        // The field terminals read the final value, so a later construction would replace it unseen.
        let before_terminals = self.field_terminals.is_empty();
        let valid = self.is_loading_line(file, line)
            && super::registry_items::pointer(owner)
            && in_source_order
            && before_terminals;
        if !valid {
            self.window_gap("A definition lacks a matching file, owner, line, order or loader");
            return;
        }
        let public_owner = self.public_owner(owner);
        self.definitions
            .entry(definition.into())
            .or_default()
            .push(DefinitionState {
                native_owner: owner.into(),
                public_owner,
                file: file.into(),
                line,
                load,
            });
    }

    /// Whether `owner` constructed the latest occurrence of `definition`, which the field events
    /// that follow it read.
    fn latest_owner_is(&self, definition: &str, owner: &str) -> bool {
        self.definitions
            .get(definition)
            .and_then(|constructions| constructions.last())
            .is_some_and(|latest| latest.native_owner == owner)
    }

    fn accept_field_storage(&mut self, event: &FixtureEvent) {
        let FixtureEvent::FieldStorage {
            question,
            file,
            line,
            definition,
            field,
            owner,
            occurrence,
            value,
        } = event
        else {
            unreachable!("field-storage handler received another event")
        };
        let Some(asked) = self.request.field_questions.get(*question as usize) else {
            self.window_gap("A storage event names an unknown question");
            return;
        };
        let last_occurrence = self
            .occurrences
            .get(question)
            .and_then(|items| items.last())
            .map_or(0, |item| item.occurrence);
        let in_window = !self.field_terminals.contains_key(question);
        let in_source = self.is_loading_line(file, *line);
        let asked_field = definition == &asked.definition && field == &asked.field;
        let owner_joins = self.latest_owner_is(definition, owner);
        let decoder = self.questions[*question as usize].storage.decoder();
        let storage_supported = decoder.is_some();
        let value_matches = decoder.is_some_and(|decoder| decoder.accepts(value));
        let in_order = *occurrence > last_occurrence;

        if !(in_window
            && in_source
            && asked_field
            && owner_joins
            && storage_supported
            && value_matches
            && in_order)
        {
            self.field_gap(
                *question,
                "A stored value lacks a matching question, source, owner, order or open field window",
            );
            return;
        }
        self.occurrences
            .entry(*question)
            .or_default()
            .push(StoredFieldOccurrence {
                file: file.clone(),
                line: *line,
                occurrence: *occurrence,
                value: value.clone(),
            });
        if *occurrence - last_occurrence > 1 {
            self.field_gap(*question, "A stored occurrence is missing");
        }
    }

    fn accept_field_parse(&mut self, event: &FixtureEvent) {
        let FixtureEvent::FieldParse {
            question,
            file,
            line,
            definition,
            field,
            owner,
            occurrence,
            returned,
        } = event
        else {
            unreachable!("field-parse handler received another event")
        };
        let Some(asked) = self.request.field_questions.get(*question as usize) else {
            self.window_gap("A parser event names an unknown question");
            return;
        };
        let owner_joins = self.latest_owner_is(definition, owner);
        if !asked.parsing
            || self.parsing_terminals.contains_key(question)
            || self.field_terminals.contains_key(question)
            || !self.is_loading_line(file, *line)
            || definition != &asked.definition
            || field != &asked.field
            || !owner_joins
        {
            self.parsing_gap(
                *question,
                "A parser invocation lacks its requested source, owner or open window",
            );
            return;
        }
        let invocations = self.parse_occurrences.entry(*question).or_default();
        if *returned {
            let Some(entry) = invocations.last_mut().filter(|entry| {
                entry.occurrence == *occurrence
                    && entry.return_line.is_none()
                    && entry.file == *file
                    && *line >= entry.line
            }) else {
                self.parsing_gap(*question, "A parser return has no matching open invocation");
                return;
            };
            entry.return_line = Some(*line);
        } else {
            let last = invocations.last();
            let last_occurrence = last.map_or(0, |entry| entry.occurrence);
            if *occurrence <= last_occurrence
                || last.is_some_and(|entry| entry.return_line.is_none())
            {
                self.parsing_gap(
                    *question,
                    "Parser invocations are repeated, nested or out of order",
                );
                return;
            }
            invocations.push(ParsedFieldOccurrence {
                file: file.clone(),
                line: *line,
                occurrence: *occurrence,
                return_line: None,
            });
            if *occurrence - last_occurrence > 1 {
                self.parsing_gap(*question, "A parser invocation is missing");
            }
        }
    }

    fn accept_parsing_terminal(&mut self, question: u64, count: u64, unavailable: &Option<String>) {
        let Some(asked) = self.request.field_questions.get(question as usize) else {
            self.window_gap("A parser terminal names an unknown question");
            return;
        };
        let occurrences = self
            .parse_occurrences
            .get(&question)
            .map_or(&[][..], Vec::as_slice);
        let witnessed = if unavailable.is_some() {
            count == 0 && occurrences.is_empty()
        } else {
            self.definitions.contains_key(&asked.definition)
                && count == occurrences.len() as u64
                && occurrences.iter().all(|entry| entry.return_line.is_some())
        };
        if !asked.parsing
            || !self.loading_last_file()
            || self.parsing_terminals.contains_key(&question)
            || !witnessed
        {
            self.parsing_gap(
                question,
                "The parser terminal disagrees with its owner, invocations or window",
            );
        }
        self.parsing_terminals
            .entry(question)
            .or_insert_with(|| unavailable.clone());
    }

    fn accept_diagnostic(&mut self, event: &FixtureEvent) {
        let FixtureEvent::Diagnostic {
            text,
            stage,
            file,
            line,
            definition,
            field,
            occurrence,
            parent_field,
        } = event
        else {
            unreachable!("diagnostic handler received another event")
        };
        if !self.diagnostics_requested
            || !self.started()
            || self.validated
            || (self.returned && !self.validation_requested())
        {
            self.diagnostic_gap("A parser diagnostic is outside its requested window");
        }
        if self.diagnostic_terminal.is_some() {
            self.diagnostic_gap("A parser diagnostic arrived after its terminal");
        }
        let reader_report = matches!(
            stage.as_str(),
            diagnostic_stage::READER_MALFORMED | diagnostic_stage::READER_UNEXPECTED
        );
        let engine_log = match stage.as_str() {
            diagnostic_stage::ENGINE_PARSER => !self.returned,
            diagnostic_stage::ENGINE_VALIDATION => self.returned && self.validation_requested(),
            _ => false,
        };
        if !reader_report && !engine_log {
            self.diagnostic_gap("A parser diagnostic names an unknown engine stage");
        }
        self.raw_diagnostics.push(RawDiagnostic {
            text: text.clone(),
            stage: stage.clone(),
            file: file.clone(),
            line: *line,
            definition: definition.clone(),
            field: field.clone(),
            occurrence: *occurrence,
            parent_field: parent_field.clone(),
        });
    }

    fn accept_field_terminal(&mut self, event: &FixtureEvent) {
        let FixtureEvent::FieldTerminal {
            question,
            owner,
            final_value,
            unavailable,
        } = event
        else {
            unreachable!("field-terminal handler received another event")
        };
        let Some(asked) = self.request.field_questions.get(*question as usize) else {
            self.window_gap("A field terminal names an unknown question");
            return;
        };
        if !self.loading_last_file() || self.field_terminals.contains_key(question) {
            self.field_gap(*question, "A field terminal is early, late or repeated");
            return;
        }
        let owner_joined = owner.as_ref().is_some_and(|pointer| {
            self.definitions
                .get(&asked.definition)
                .is_some_and(|constructions| {
                    constructions
                        .iter()
                        .all(|construction| &construction.native_owner == pointer)
                })
        });
        let shape_valid = match self.questions[*question as usize].storage.decoder() {
            Some(decoder) => {
                owner_joined
                    && unavailable.is_none()
                    && final_value
                        .as_ref()
                        .is_some_and(|value| decoder.accepts(value))
            }
            None => final_value.is_none() && unavailable.is_some(),
        };
        if !shape_valid {
            self.field_gap(
                *question,
                "A field terminal lacks its required owner, storage or unavailable reason",
            );
        }
        self.field_terminals.insert(
            *question,
            FieldTerminalState {
                final_value: final_value.clone().filter(|_| shape_valid),
                unavailable: unavailable.clone(),
            },
        );
    }

    fn accept_diagnostics_terminal(&mut self, count: u64) {
        if !self.diagnostics_requested
            || !self.at_diagnostic_terminal()
            || self.diagnostic_terminal.is_some()
            || count != self.raw_diagnostics.len() as u64
        {
            self.diagnostic_gap("The diagnostic terminal disagrees with its records");
        }
        if self.diagnostic_terminal.is_none() {
            self.diagnostic_terminal = Some(DiagnosticTerminalState::Complete);
        }
    }

    fn accept_diagnostics_unavailable(&mut self, reason: &str) {
        if !self.diagnostics_requested
            || !self.at_diagnostic_terminal()
            || self.diagnostic_terminal.is_some()
            || !self.raw_diagnostics.is_empty()
        {
            self.diagnostic_gap("The unavailable diagnostic terminal is inconsistent");
        }
        if self.diagnostic_terminal.is_none() {
            self.diagnostic_terminal = Some(DiagnosticTerminalState::Unavailable(reason.into()));
        }
    }

    fn accept_load_returned(&mut self, file: &str) {
        if self.current.as_deref() != Some(file) {
            self.window_gap("The fixture return has no unique matching loader entry");
            return;
        }
        self.returned = self.loading_last_file();
        self.current = None;
    }

    fn validation_requested(&self) -> bool {
        self.request.window == crate::FixtureWindow::InitialFileLoadAndValidation
    }

    fn at_diagnostic_terminal(&self) -> bool {
        if self.validation_requested() {
            self.validated
        } else {
            self.loading_last_file()
        }
    }

    fn accept_end(
        &mut self,
        sequence: u64,
        field_outcomes: u64,
        diagnostics: u64,
        producer_last_sequence: u64,
    ) {
        if !self.returned || producer_last_sequence != sequence {
            self.window_gap("The fixture terminal disagrees with its window");
        }
        if field_outcomes != self.field_terminals.len() as u64 {
            for index in 0..self.request.field_questions.len() as u64 {
                self.field_gap(index, "The fixture terminal disagrees with field terminals");
            }
        }
        if diagnostics != self.raw_diagnostics.len() as u64
            || (self.diagnostics_requested != self.diagnostic_terminal.is_some())
            || (self.validation_requested() && !self.validated)
        {
            self.diagnostic_gap("The fixture terminal disagrees with the diagnostic window");
        }
        self.ended = true;
    }

    fn question_link(&self, raw: &RawDiagnostic) -> QuestionLink {
        let (definition, field, occurrence) = match (&raw.definition, &raw.field, raw.occurrence) {
            (None, None, None) => return QuestionLink::Unscoped,
            (Some(definition), Some(field), Some(occurrence)) => (definition, field, occurrence),
            _ => return QuestionLink::Unmatched,
        };

        self.request
            .field_questions
            .iter()
            .enumerate()
            .find(|(index, question)| {
                question.definition == *definition
                    && question.field == *field
                    && question.parent_field == raw.parent_field
                    && (self
                        .parse_occurrences
                        .get(&(*index as u64))
                        .is_some_and(|items| {
                            items.iter().any(|item| {
                                item.occurrence == occurrence
                                    && raw
                                        .file
                                        .as_ref()
                                        .zip(raw.line)
                                        .is_some_and(|(file, line)| parsed_line(item, file, line))
                            })
                        })
                        || self.occurrences.get(&(*index as u64)).is_some_and(|items| {
                            items.iter().any(|item| {
                                item.occurrence == occurrence
                                    && raw.file.as_ref() == Some(&item.file)
                                    && raw.line == Some(item.line)
                            })
                        }))
            })
            .map_or(QuestionLink::Unmatched, |(index, _)| {
                QuestionLink::Question(index as u64)
            })
    }

    fn finish_diagnostics(&mut self) {
        for mut raw in std::mem::take(&mut self.raw_diagnostics) {
            self.join_diagnostic_source(&mut raw);
            let source = match (&raw.file, raw.line) {
                (Some(file), Some(line)) if self.is_fixture_line(file, line) => {
                    Some((file.clone(), line))
                }
                _ => None,
            };
            let join = match (source, self.question_link(&raw)) {
                (Some((file, line)), QuestionLink::Question(question)) => {
                    if self.request.field_questions[question as usize].diagnostics {
                        self.diagnostic_indices
                            .entry(question)
                            .or_default()
                            .push(self.value.diagnostics.len());
                    }
                    DiagnosticJoin::Source {
                        file,
                        line,
                        definition: raw.definition.clone(),
                        field: raw.field.clone(),
                        occurrence: raw.occurrence,
                    }
                }
                (Some((file, line)), QuestionLink::Unscoped) => DiagnosticJoin::Source {
                    file,
                    line,
                    definition: None,
                    field: None,
                    occurrence: None,
                },
                (Some((file, line)), QuestionLink::Unmatched) => {
                    self.diagnostic_gap(
                        "A parser diagnostic field join lacks a witnessed occurrence",
                    );
                    DiagnosticJoin::Source {
                        file,
                        line,
                        definition: None,
                        field: None,
                        occurrence: None,
                    }
                }
                (None, _) => {
                    self.diagnostic_gap("A parser diagnostic has no valid fixture source join");
                    DiagnosticJoin::Unavailable(
                        "The diagnostic source did not join to the fixture".into(),
                    )
                }
            };
            self.value.diagnostics.push(FixtureDiagnostic {
                text: raw.text,
                stage: raw.stage,
                join,
            });
        }
        self.value.diagnostic_coverage = if !self.diagnostics_requested {
            DiagnosticCoverage::NotRequested
        } else {
            match self.diagnostic_terminal.take() {
                Some(DiagnosticTerminalState::Unavailable(reason)) => {
                    self.push_gap(
                        GapKind::OutsideMethod,
                        Some(GapSubject::registry(self.request.registry())),
                        &reason,
                    );
                    DiagnosticCoverage::Unavailable(reason)
                }
                Some(DiagnosticTerminalState::Complete)
                    if self.ended
                        && self.returned
                        && self.stream_intact
                        && !self.diagnostic_issue =>
                {
                    DiagnosticCoverage::Complete {
                        window: if self.validation_requested() {
                            DiagnosticWindow::FixtureFileLoadAndValidation
                        } else {
                            DiagnosticWindow::FixtureFileLoad
                        },
                    }
                }
                _ => {
                    self.diagnostic_gap("The parser diagnostic window did not complete intact");
                    DiagnosticCoverage::Unavailable(
                        "The parser diagnostic window did not complete intact".into(),
                    )
                }
            }
        };
    }

    fn join_diagnostic_source(&self, raw: &mut RawDiagnostic) {
        if raw.definition.is_some() || raw.field.is_some() || raw.occurrence.is_some() {
            return;
        }
        let (Some(file), Some(line)) = (&raw.file, raw.line) else {
            return;
        };
        if !self.is_fixture_line(file, line) {
            return;
        }
        let mut matches = self
            .parse_occurrences
            .iter()
            .flat_map(|(question, occurrences)| {
                occurrences.iter().filter_map(move |occurrence| {
                    parsed_line(occurrence, file, line)
                        .then_some((*question, occurrence.occurrence))
                })
            });
        let Some((index, occurrence)) = matches.next() else {
            return;
        };
        if matches.next().is_some() || self.parsing_issues.contains(&index) {
            return;
        }
        let question = &self.request.field_questions[index as usize];
        raw.definition = Some(question.definition.clone());
        raw.field = Some(question.field.clone());
        raw.parent_field = question.parent_field.clone();
        raw.occurrence = Some(occurrence);
    }

    fn finish_fields(&mut self) {
        let questions = self.questions;
        for index in 0..self.request.field_questions.len() as u64 {
            let question = self.request.field_questions[index as usize].clone();
            let terminal = self.field_terminals.remove(&index);
            let definitions: Vec<_> = self
                .definitions
                .get(&question.definition)
                .into_iter()
                .flatten()
                .map(|construction| FixtureDefinition {
                    file: construction.file.clone(),
                    line: construction.line,
                    owner: construction.public_owner.clone(),
                })
                .collect();
            let occurrences = self.occurrences.remove(&index).unwrap_or_default();
            let parsing = self.finish_parsing(index, &question);
            let storage = match &questions[index as usize].storage {
                FixtureQuestionStorage::Bound(_) => {
                    self.observed_storage(index, terminal, !definitions.is_empty(), occurrences)
                }
                FixtureQuestionStorage::Unbound(reason) => {
                    self.unbound_storage(&question, GapKind::OutsideMethod, reason)
                }
                FixtureQuestionStorage::UnresolvedReader(reason) => {
                    self.unbound_storage(&question, GapKind::UnresolvedReader, reason)
                }
            };
            self.value.field_outcomes.push(FixtureFieldOutcome {
                question,
                definitions,
                parsing,
                storage,
                diagnostics: self.diagnostic_indices.remove(&index).unwrap_or_default(),
            });
        }
    }

    /// The storage of a question that the setup bound to a destination: the stored occurrences
    /// and the file-terminal value, or the reason that none was observed.
    fn observed_storage(
        &mut self,
        index: u64,
        terminal: Option<FieldTerminalState>,
        definition_witnessed: bool,
        occurrences: Vec<StoredFieldOccurrence>,
    ) -> FixtureStorage {
        let final_value = terminal
            .as_ref()
            .and_then(|terminal| terminal.final_value.clone());
        let unavailable = terminal
            .as_ref()
            .and_then(|terminal| terminal.unavailable.clone());
        if occurrences.is_empty() && final_value.is_none() {
            let reason =
                unavailable.unwrap_or_else(|| "No storage observation was established".into());
            self.field_gap(index, &reason);
            return FixtureStorage::Unavailable(reason);
        }
        if final_value.is_none()
            && let Some(reason) = &unavailable
        {
            self.field_gap(index, reason);
        }
        let complete = self.ended
            && self.returned
            && self.stream_intact
            && !self.field_issues.contains(&index)
            && terminal.is_some()
            && final_value.is_some()
            && definition_witnessed;

        FixtureStorage::Observed {
            occurrences,
            final_value,
            completeness: if complete {
                Completeness::Complete
            } else {
                Completeness::Partial
            },
        }
    }

    /// The storage of a question that the setup bound to no destination, with the gap that says why.
    fn unbound_storage(
        &mut self,
        question: &crate::FixtureFieldQuestion,
        kind: GapKind,
        reason: &str,
    ) -> FixtureStorage {
        self.push_gap(
            kind,
            Some(GapSubject::field(question.field.clone())),
            reason,
        );
        FixtureStorage::Unavailable(reason.into())
    }

    fn finish_parsing(
        &mut self,
        index: u64,
        question: &crate::FixtureFieldQuestion,
    ) -> FixtureParsing {
        if !question.parsing {
            return FixtureParsing::NotRequested;
        }
        let terminal = self.parsing_terminals.remove(&index);
        if terminal.is_none() {
            self.parsing_gap(index, "The parser observation terminal is missing");
        }
        if let Some(Some(reason)) = terminal {
            self.parsing_gap(index, &reason);
            return FixtureParsing::Unavailable(reason);
        }
        let occurrences = self.parse_occurrences.remove(&index).unwrap_or_default();
        let complete = self.ended
            && self.returned
            && self.stream_intact
            && !self.parsing_issues.contains(&index)
            && self.definitions.contains_key(&question.definition);
        FixtureParsing::Observed {
            occurrences,
            completeness: if complete {
                Completeness::Complete
            } else {
                Completeness::Partial
            },
        }
    }

    fn finish(mut self, build: BuildId) -> Answer<FixtureObservation> {
        self.finish_diagnostics();
        self.finish_fields();
        self.value.loaded_files = std::mem::take(&mut self.loads);
        Answer {
            completeness: Completeness::from_gaps(&self.gaps),
            value: self.value,
            gaps: self.gaps,
            source: Source::new(build, "observe-fixture/v9", Basis::LiveObservation),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::observation::{FixtureStorageBinding, FixtureStorageDecoder};
    use serde_json::{Value, json};

    const SAMPLE: &str = "common/traditions/sample.txt";

    /// Parse authored worker events as worker records.
    fn records(events: Vec<Value>) -> Vec<WorkerRecord> {
        events
            .into_iter()
            .map(|event| serde_json::from_value(event).unwrap())
            .collect()
    }

    /// A destination that reads `decoder`, for a question whose reader is established.
    fn bound(decoder: FixtureStorageDecoder) -> FixtureQuestionStorage {
        FixtureQuestionStorage::Bound(FixtureStorageBinding {
            offset: 0x10,
            decoder,
        })
    }

    /// `reduce` with `storage` as the setup of every question.
    fn reduce_with(
        request: &FixtureRequest,
        storage: FixtureQuestionStorage,
        records: &[WorkerRecord],
        owner_events: &[OwnerEvent],
        build: BuildId,
    ) -> Result<Answer<FixtureObservation>, Error> {
        let questions: Vec<_> = request
            .field_questions
            .iter()
            .enumerate()
            .map(|(index, question)| FixtureQuestionSetup::new(index, question, storage.clone()))
            .collect();

        super::reduce(request, &questions, records, owner_events, build)
    }

    /// `reduce` with a string destination for every question, which most tests ask for.
    fn reduce(
        request: &FixtureRequest,
        records: &[WorkerRecord],
        owner_events: &[OwnerEvent],
        build: BuildId,
    ) -> Result<Answer<FixtureObservation>, Error> {
        reduce_with(
            request,
            bound(FixtureStorageDecoder::String),
            records,
            owner_events,
            build,
        )
    }

    /// The setup of a block field: its reader is established, and no storage decoder is bound.
    fn block_storage() -> FixtureQuestionStorage {
        FixtureQuestionStorage::Unbound("No block storage decoder".into())
    }

    fn field_request() -> FixtureRequest {
        let question =
            crate::FixtureFieldQuestion::new("common/traditions", "sample", "unlocks_agenda");
        FixtureRequest::field_outcomes(
            "common/traditions/sample.txt",
            "sample = {\n unlocks_agenda = \"one\"\n unlocks_agenda = \"two\"\n}\n",
            [question],
        )
    }

    fn field_request_without_diagnostics() -> FixtureRequest {
        let mut request = field_request();
        request.field_questions[0].diagnostics = false;
        request
    }

    fn block_parsing_request() -> FixtureRequest {
        FixtureRequest::field_outcomes(
            "common/traditions/sample.txt",
            "sample = {\n potential = {\n  and = { always = yes }\n }\n}\n",
            [
                crate::FixtureFieldQuestion::new("common/traditions", "sample", "potential")
                    .with_parsing(),
            ],
        )
    }

    fn block_parsing_events() -> Vec<Value> {
        field_outcome_session(
            vec![
                json!({"kind":"load-start","file":"common/traditions/sample.txt"}),
                json!({"kind":"definition","file":"common/traditions/sample.txt","line":1,"definition":"sample","owner":"0x2000"}),
                json!({"kind":"field-parse","question":0,"file":"common/traditions/sample.txt","line":2,"definition":"sample","field":"potential","owner":"0x2000","occurrence":1,"returned":false}),
                json!({"kind":"diagnostic","text":"Rejected nested child","stage":"reader-unexpected-report","file":"common/traditions/sample.txt","line":3,"definition":"sample","field":"potential","occurrence":1}),
                json!({"kind":"field-parse","question":0,"file":"common/traditions/sample.txt","line":4,"definition":"sample","field":"potential","owner":"0x2000","occurrence":1,"returned":true}),
                json!({"kind":"field-terminal","question":0,"owner":null,"final_value":null,"unavailable":"No block storage decoder"}),
                json!({"kind":"parsing-terminal","question":0,"count":1,"unavailable":null}),
                json!({"kind":"diagnostics-terminal","count":1}),
                json!({"kind":"load-returned","file":"common/traditions/sample.txt"}),
            ],
            json!({"kind":"end","field_outcomes":1,"diagnostics":1}),
        )
    }

    fn block_parsing_answer(events: Vec<Value>) -> Answer<FixtureObservation> {
        reduce_with(
            &block_parsing_request(),
            block_storage(),
            &records(events),
            &owner(),
            BuildId("build".into()),
        )
        .unwrap()
    }

    fn validation_events() -> Vec<Value> {
        let mut events = block_parsing_events();
        let hooks = &mut events[1]["hooks"];
        hooks["fixture:log"] = hooks["fixture:malformed"].clone();
        hooks["fixture:unformatted-log"] = hooks["fixture:malformed"].clone();
        hooks["fixture:stream-log"] = hooks["fixture:malformed"].clone();
        hooks["fixture:sourced-log"] = hooks["fixture:malformed"].clone();
        hooks["fixture:validated"] = hooks["fixture:malformed"].clone();
        let diagnostic = events
            .iter_mut()
            .find(|event| event["event"]["kind"] == "diagnostic")
            .unwrap();
        diagnostic["event"]["stage"] = json!("engine-validation-log");
        for key in ["definition", "field", "occurrence"] {
            diagnostic["event"][key] = Value::Null;
        }
        let diagnostic = events.remove(
            events
                .iter()
                .position(|event| event["event"]["kind"] == "diagnostic")
                .unwrap(),
        );
        let terminal = events.remove(
            events
                .iter()
                .position(|event| event["event"]["kind"] == "diagnostics-terminal")
                .unwrap(),
        );
        let end = events.pop().unwrap();
        events.push(diagnostic);
        let mut validated = terminal.clone();
        validated["event"] = json!({"kind":"validation-complete"});
        events.extend([validated, terminal, end]);
        renumber(&mut events);
        events
    }

    fn validation_answer(events: Vec<Value>) -> Answer<FixtureObservation> {
        reduce_with(
            &block_parsing_request().through_validation(),
            block_storage(),
            &records(events),
            &owner(),
            BuildId("build".into()),
        )
        .unwrap()
    }

    #[test]
    fn deferred_diagnostics_join_to_witnessed_nested_parsing() {
        let answer = validation_answer(validation_events());
        assert_eq!(answer.value.diagnostics[0].stage, "engine-validation-log");
        assert!(matches!(
            answer.value.diagnostics[0].join,
            DiagnosticJoin::Source {
                line: 3,
                occurrence: Some(1),
                ..
            }
        ));
        assert!(matches!(
            answer.value.diagnostic_coverage,
            DiagnosticCoverage::Complete {
                window: DiagnosticWindow::FixtureFileLoadAndValidation
            }
        ));
    }

    #[test]
    fn engine_parser_logs_join_during_initial_file_load_on_either_thread() {
        for thread in [7, 8] {
            let mut events = block_parsing_events();
            let diagnostic = events
                .iter_mut()
                .find(|event| event["event"]["kind"] == "diagnostic")
                .unwrap();
            diagnostic["thread"] = json!(thread);
            diagnostic["event"]["stage"] = json!("engine-parser-log");
            for key in ["definition", "field", "occurrence"] {
                diagnostic["event"][key] = Value::Null;
            }
            let answer = block_parsing_answer(events.clone());
            assert_eq!(
                answer.value.diagnostic_coverage,
                DiagnosticCoverage::Complete {
                    window: DiagnosticWindow::FixtureFileLoad
                }
            );
            assert!(matches!(
                &answer.value.diagnostics[0].join,
                DiagnosticJoin::Source {
                    line: 3,
                    occurrence: Some(1),
                    ..
                }
            ));
            let diagnostic = events
                .iter_mut()
                .find(|event| event["event"]["kind"] == "diagnostic")
                .unwrap();
            diagnostic["event"]["stage"] = json!("engine-validation-log");
            assert!(!matches!(
                block_parsing_answer(events).value.diagnostic_coverage,
                DiagnosticCoverage::Complete { .. }
            ));
        }
        for hook in [
            "fixture:log",
            "fixture:unformatted-log",
            "fixture:stream-log",
            "fixture:sourced-log",
        ] {
            let mut events = block_parsing_events();
            events[1]["hooks"].as_object_mut().unwrap().remove(hook);
            assert!(
                reduce_with(
                    &block_parsing_request(),
                    block_storage(),
                    &records(events),
                    &owner(),
                    BuildId("build".into())
                )
                .is_err()
            );
        }
    }

    #[test]
    fn engine_diagnostics_keep_background_threads_without_relaxing_parser_ownership() {
        for deferred in [false, true] {
            let mut events = validation_events();
            let index = events
                .iter()
                .position(|event| event["event"]["kind"] == "diagnostic")
                .unwrap();
            let mut diagnostic = events.remove(index);
            diagnostic["thread"] = json!(8);
            if deferred {
                events.insert(index, diagnostic);
            } else {
                diagnostic["event"]["stage"] = json!("engine-parser-log");
                let at = events
                    .iter()
                    .position(|event| event["event"]["kind"] == "load-returned")
                    .unwrap();
                events.insert(at, diagnostic);
            }
            renumber(&mut events);
            let answer = validation_answer(events);
            assert_eq!(answer.value.diagnostics.len(), 1);
            assert!(matches!(
                answer.value.diagnostics[0].join,
                DiagnosticJoin::Source {
                    line: 3,
                    occurrence: Some(1),
                    ..
                }
            ));
            assert!(matches!(
                answer.value.diagnostic_coverage,
                DiagnosticCoverage::Complete { .. }
            ));
        }
        for (kind, pointer, value) in [
            ("diagnostic", "/thread", json!(0)),
            ("diagnostic", "/thread", Value::Null),
            ("diagnostic", "/event/file", json!("other.txt")),
            (
                "diagnostic",
                "/event/stage",
                json!("reader-malformed-report"),
            ),
            ("diagnostic", "/event/occurrence", json!(1)),
            ("diagnostic", "/event/line", Value::Null),
            ("load-start", "/thread", json!(8)),
            ("field-parse", "/thread", json!(8)),
            ("validation-complete", "/thread", json!(8)),
            ("diagnostics-terminal", "/thread", json!(8)),
        ] {
            let mut events = validation_events();
            let event = events
                .iter_mut()
                .find(|event| event["event"]["kind"] == kind)
                .unwrap();
            event["thread"] = json!(8);
            *event.pointer_mut(pointer).unwrap() = value;
            let answer = validation_answer(events);
            assert!(
                !matches!(
                    answer.value.diagnostic_coverage,
                    DiagnosticCoverage::Complete { .. }
                ),
                "{kind} {pointer}"
            );
        }
    }

    #[test]
    fn validation_requires_hooks_boundaries_and_intact_diagnostics() {
        for missing in [
            "validation-complete",
            "diagnostics-terminal",
            "load-returned",
            "end",
        ] {
            let mut events = validation_events();
            events.retain(|event| event["event"]["kind"] != missing);
            renumber(&mut events);
            let answer = validation_answer(events);
            assert!(
                !matches!(
                    answer.value.diagnostic_coverage,
                    DiagnosticCoverage::Complete { .. }
                ),
                "{missing}"
            );
        }
        for hook in [
            "fixture:log",
            "fixture:unformatted-log",
            "fixture:stream-log",
            "fixture:sourced-log",
            "fixture:validated",
            "fixture:malformed",
            "fixture:unexpected",
        ] {
            let mut events = validation_events();
            events[1]["hooks"].as_object_mut().unwrap().remove(hook);
            assert!(
                reduce_with(
                    &block_parsing_request().through_validation(),
                    block_storage(),
                    &records(events),
                    &owner(),
                    BuildId("build".into())
                )
                .is_err(),
                "{hook}"
            );
        }
        for (kind, key, value) in [
            ("diagnostic", "line", Value::Null),
            ("diagnostic", "stage", json!("engine-parser-log")),
            ("diagnostics-terminal", "count", json!(0)),
        ] {
            let mut events = validation_events();
            let event = events
                .iter_mut()
                .find(|event| event["event"]["kind"] == kind)
                .unwrap();
            event["event"][key] = value;
            let answer = validation_answer(events);
            assert!(
                !matches!(
                    answer.value.diagnostic_coverage,
                    DiagnosticCoverage::Complete { .. }
                ),
                "{kind} {key}"
            );
        }
    }

    #[test]
    fn block_parsing_is_independent_of_storage_and_keeps_nested_diagnostics() {
        let answer = block_parsing_answer(block_parsing_events());
        let field = &answer.value.field_outcomes[0];
        assert_eq!(
            field.parsing,
            FixtureParsing::Observed {
                occurrences: vec![ParsedFieldOccurrence {
                    file: SAMPLE.into(),
                    line: 2,
                    occurrence: 1,
                    return_line: Some(4)
                }],
                completeness: Completeness::Complete,
            }
        );
        assert!(matches!(field.storage, FixtureStorage::Unavailable(_)));
        assert_eq!(field.diagnostics, [0]);
        assert!(matches!(
            answer.value.diagnostics[0].join,
            DiagnosticJoin::Source {
                line: 3,
                occurrence: Some(1),
                ..
            }
        ));
        assert!(matches!(
            answer.value.diagnostic_coverage,
            DiagnosticCoverage::Complete { .. }
        ));
    }

    #[test]
    fn block_parsing_requires_paired_invocations_and_matching_terminals() {
        for kind in [
            "field-parse",
            "parsing-terminal",
            "definition",
            "load-returned",
            "end",
        ] {
            let mut events = block_parsing_events();
            let index = events
                .iter()
                .position(|event| event["event"]["kind"] == kind)
                .unwrap();
            events.remove(index);
            renumber(&mut events);
            let answer = block_parsing_answer(events);
            assert!(
                !matches!(
                    answer.value.field_outcomes[0].parsing,
                    FixtureParsing::Observed {
                        completeness: Completeness::Complete,
                        ..
                    }
                ),
                "{kind}"
            );
        }
        for (kind, key, value) in [
            ("field-parse", "owner", json!("0x3000")),
            ("field-parse", "occurrence", json!(2)),
            ("field-parse", "line", json!(99)),
            ("parsing-terminal", "count", json!(0)),
        ] {
            let mut events = block_parsing_events();
            let event = events
                .iter_mut()
                .find(|event| event["event"]["kind"] == kind)
                .unwrap();
            event["event"][key] = value;
            let answer = block_parsing_answer(events);
            assert!(
                !matches!(
                    answer.value.field_outcomes[0].parsing,
                    FixtureParsing::Observed {
                        completeness: Completeness::Complete,
                        ..
                    }
                ),
                "{kind} {key}"
            );
        }
        let mut events = block_parsing_events();
        let index = events
            .iter()
            .position(|event| event["event"]["returned"] == true)
            .unwrap();
        events.remove(index);
        renumber(&mut events);
        let answer = block_parsing_answer(events);
        assert!(matches!(
            answer.value.field_outcomes[0].parsing,
            FixtureParsing::Observed {
                completeness: Completeness::Partial,
                ..
            }
        ));
    }

    /// Worker records for an owned session that activates every field-outcome hook, then emits
    /// `window` and the terminal `end`. The terminal gains its producer's last sequence.
    fn field_outcome_session(window: Vec<Value>, mut end: Value) -> Vec<Value> {
        let hook = json!({"enabled":true,"locations":1,"resolved":1,"hits":0});
        let fixture = |event: Value| json!({"kind":"fixture", "event":event});
        let mut events = vec![
            json!({"kind":"launch-stopped", "error":"success", "pid":42, "triple":"arm64-macos", "frames":[{"function":"_dyld_start"}]}),
            json!({"kind":"hooks-active-before-resume", "hooks":{
                "fixture:load":hook,
                "fixture:constructor":hook,
                "fixture:reader":hook,
                "fixture:member":hook,
                "fixture:malformed":hook,
                "fixture:unexpected":hook,
                "fixture:log":hook,
                "fixture:unformatted-log":hook,
                "fixture:stream-log":hook,
                "fixture:sourced-log":hook
            }}),
            json!({"kind":"resume","error":"success"}),
        ];
        events.extend(window.into_iter().map(fixture));
        end["producer_last_sequence"] = json!(events.len() + 1);
        events.push(fixture(end));

        events
            .into_iter()
            .enumerate()
            .map(|(index, mut event)| {
                event["seq"] = json!(index + 1);
                event["thread"] = json!(7);
                event["run"] = json!("attempt");
                event
            })
            .collect()
    }

    /// Row numbers, from 0: 3 load start, 4 definition, 5 and 7 storage, 6 diagnostic, 8 field
    /// terminal, 9 diagnostic terminal, 10 return and 11 end.
    fn field_events() -> Vec<Value> {
        field_outcome_session(
            vec![
                json!({"kind":"load-start","file":"common/traditions/sample.txt"}),
                json!({"kind":"definition","file":"common/traditions/sample.txt","line":1,"definition":"sample","owner":"0x2000"}),
                json!({"kind":"field-storage","question":0,"file":"common/traditions/sample.txt","line":2,"definition":"sample","field":"unlocks_agenda","owner":"0x2000","occurrence":1,"value":{"String":"one"}}),
                json!({"kind":"diagnostic","text":"malformed value","stage":"reader-malformed-report","file":"common/traditions/sample.txt","line":2,"definition":"sample","field":"unlocks_agenda","occurrence":1}),
                json!({"kind":"field-storage","question":0,"file":"common/traditions/sample.txt","line":3,"definition":"sample","field":"unlocks_agenda","owner":"0x2000","occurrence":2,"value":{"String":"two"}}),
                json!({"kind":"field-terminal","question":0,"owner":"0x2000","final_value":{"String":"two"},"unavailable":null}),
                json!({"kind":"diagnostics-terminal","count":1}),
                json!({"kind":"load-returned","file":"common/traditions/sample.txt"}),
            ],
            json!({"kind":"end","field_outcomes":1,"diagnostics":1}),
        )
    }

    fn renumber(events: &mut [Value]) {
        let last = events.len();
        for (index, event) in events.iter_mut().enumerate() {
            event["seq"] = json!(index + 1);
            if event.pointer("/event/kind").and_then(Value::as_str) == Some("end") {
                event["event"]["producer_last_sequence"] = json!(last);
            }
        }
    }

    fn unavailable_field_events() -> Vec<Value> {
        let mut events = field_events()
            .into_iter()
            .filter(|event| {
                !matches!(
                    event.pointer("/event/kind").and_then(Value::as_str),
                    Some("field-storage" | "diagnostic")
                )
            })
            .collect::<Vec<_>>();
        for event in &mut events {
            match event.pointer("/event/kind").and_then(Value::as_str) {
                Some("field-terminal") => {
                    event["event"]["final_value"] = Value::Null;
                    event["event"]["unavailable"] = json!("Storage decoder unavailable");
                }
                Some("diagnostics-terminal") => event["event"]["count"] = json!(0),
                Some("end") => event["event"]["diagnostics"] = json!(0),
                _ => {}
            }
        }
        renumber(&mut events);
        events
    }

    fn no_diagnostic_events() -> Vec<Value> {
        let mut events = field_events()
            .into_iter()
            .filter(|event| {
                !matches!(
                    event.pointer("/event/kind").and_then(Value::as_str),
                    Some("diagnostic" | "diagnostics-terminal")
                )
            })
            .collect::<Vec<_>>();
        events[1]["hooks"]
            .as_object_mut()
            .unwrap()
            .remove("fixture:malformed");
        events[1]["hooks"]
            .as_object_mut()
            .unwrap()
            .remove("fixture:unexpected");
        for event in &mut events {
            if event.pointer("/event/kind").and_then(Value::as_str) == Some("end") {
                event["event"]["diagnostics"] = json!(0);
            }
        }
        renumber(&mut events);
        events
    }

    fn category_outcome_request() -> FixtureRequest {
        FixtureRequest::field_outcomes(
            "common/tradition_categories/sample.txt",
            "sample = { tree_template = \"example\" }\n",
            [crate::FixtureFieldQuestion::new(
                "common/tradition_categories",
                "sample",
                "tree_template",
            )],
        )
    }

    /// The setup of a tradition category's field: its reader is established, and no exact-build
    /// storage is bound.
    fn category_storage() -> FixtureQuestionStorage {
        FixtureQuestionStorage::Unbound("No exact-build storage binding for this field".into())
    }

    fn category_outcome_events() -> Vec<Value> {
        field_outcome_session(
            vec![
                json!({"kind":"load-start","file":"common/tradition_categories/sample.txt"}),
                json!({"kind":"field-terminal","question":0,"owner":null,"final_value":null,"unavailable":"No exact-build storage binding for this field"}),
                json!({"kind":"diagnostics-unavailable","reason":"Parser diagnostics are outside this registry binding"}),
                json!({"kind":"load-returned","file":"common/tradition_categories/sample.txt"}),
            ],
            json!({"kind":"end","field_outcomes":1,"diagnostics":0}),
        )
    }

    fn two_field_request() -> FixtureRequest {
        FixtureRequest::field_outcomes(
            "common/traditions/sample.txt",
            "sample = {\n custom_tooltip = \"tip\"\n unlocks_agenda = \"agenda\"\n}\n",
            [
                crate::FixtureFieldQuestion::new("common/traditions", "sample", "custom_tooltip"),
                crate::FixtureFieldQuestion::new("common/traditions", "sample", "unlocks_agenda"),
            ],
        )
    }

    fn two_field_events() -> Vec<Value> {
        let storage = |question: u64, line: u64, field: &str, value: &str| json!({"kind":"field-storage","question":question,"file":"common/traditions/sample.txt","line":line,"definition":"sample","field":field,"owner":"0x2000","occurrence":1,"value":{"String":value}});
        let terminal = |question: u64, value: &str| json!({"kind":"field-terminal","question":question,"owner":"0x2000","final_value":{"String":value},"unavailable":null});
        field_outcome_session(
            vec![
                json!({"kind":"load-start","file":"common/traditions/sample.txt"}),
                json!({"kind":"definition","file":"common/traditions/sample.txt","line":1,"definition":"sample","owner":"0x2000"}),
                storage(0, 2, "custom_tooltip", "tip"),
                storage(1, 3, "unlocks_agenda", "agenda"),
                terminal(0, "tip"),
                terminal(1, "agenda"),
                json!({"kind":"diagnostics-terminal","count":0}),
                json!({"kind":"load-returned","file":"common/traditions/sample.txt"}),
            ],
            json!({"kind":"end","field_outcomes":2,"diagnostics":0}),
        )
    }

    fn different_owner_request() -> FixtureRequest {
        FixtureRequest::field_outcomes(
            "common/traditions/sample.txt",
            "first = {\n custom_tooltip = \"tip\"\n}\nsecond = {\n unlocks_agenda = \"agenda\"\n}\n",
            [
                crate::FixtureFieldQuestion::new("common/traditions", "first", "custom_tooltip"),
                crate::FixtureFieldQuestion::new("common/traditions", "second", "unlocks_agenda"),
            ],
        )
    }

    fn different_owner_events() -> Vec<Value> {
        let mut events = two_field_events();
        for event in &mut events {
            match event.pointer("/event/kind").and_then(Value::as_str) {
                Some("definition") => event["event"]["definition"] = json!("first"),
                Some("field-storage") => {
                    if event["event"]["question"] == json!(0) {
                        event["event"]["definition"] = json!("first");
                    } else {
                        event["event"]["line"] = json!(5);
                        event["event"]["definition"] = json!("second");
                        event["event"]["owner"] = json!("0x3000");
                    }
                }
                Some("field-terminal") if event["event"]["question"] == json!(1) => {
                    event["event"]["owner"] = json!("0x3000");
                }
                _ => {}
            }
        }
        let second_storage = events
            .iter()
            .position(|event| {
                event.pointer("/event/kind").and_then(Value::as_str) == Some("field-storage")
                    && event.pointer("/event/question").and_then(Value::as_u64) == Some(1)
            })
            .unwrap();
        events.insert(
            second_storage,
            json!({"kind":"fixture","event":{"kind":"definition",
                "file":"common/traditions/sample.txt","line":4,"definition":"second",
                "owner":"0x3000"},"thread":7,"run":"attempt"}),
        );
        renumber(&mut events);
        events
    }
    fn owner() -> Vec<OwnerEvent> {
        vec![OwnerEvent::GameOwnedSuspended {
            pid: 42,
            identity: "owned-process".into(),
        }]
    }
    fn answer(events: Vec<Value>) -> Answer<FixtureObservation> {
        reduce(
            &field_request(),
            &records(events),
            &owner(),
            BuildId("build".into()),
        )
        .unwrap()
    }

    #[test]
    fn category_field_questions_require_the_outcome_hooks() {
        for hook in [
            "fixture:constructor",
            "fixture:reader",
            "fixture:member",
            "fixture:malformed",
            "fixture:unexpected",
        ] {
            let mut events = category_outcome_events();
            events[1]["hooks"].as_object_mut().unwrap().remove(hook);

            let result = reduce_with(
                &category_outcome_request(),
                category_storage(),
                &records(events),
                &owner(),
                BuildId("b".into()),
            );
            assert!(result.is_err(), "{hook}");
        }
    }

    #[test]
    fn every_activation_fact_is_required_before_any_complete_answer() {
        for (index, pointer, value) in [
            (0, "/pid", json!(99)),
            (0, "/error", json!("failed")),
            (0, "/thread", json!(0)),
            (1, "/hooks/fixture:member/enabled", json!(false)),
            (1, "/hooks/fixture:load/locations", json!(0)),
            (1, "/hooks/fixture:constructor/resolved", json!(2)),
            (1, "/hooks/fixture:member/hits", json!(1)),
            (2, "/error", json!("failed")),
            (2, "/seq", json!(1)),
        ] {
            let mut changed = field_events();
            *changed[index].pointer_mut(pointer).unwrap() = value;
            assert!(
                matches!(
                    reduce(
                        &field_request(),
                        &records(changed),
                        &owner(),
                        BuildId("b".into())
                    ),
                    Err(Error::Observation { .. })
                ),
                "{pointer}"
            );
        }
        for index in [0, 1, 2] {
            let mut changed = field_events();
            changed.remove(index);
            assert!(
                reduce(
                    &field_request(),
                    &records(changed),
                    &owner(),
                    BuildId("b".into())
                )
                .is_err()
            );
        }
        assert!(
            reduce(
                &field_request(),
                &records(field_events()),
                &[],
                BuildId("b".into())
            )
            .is_err()
        );
    }

    /// The hooks that `field_request` requires before resume.
    const MINIMUM_HOOKS: [&str; 10] = [
        "fixture:load",
        "fixture:constructor",
        "fixture:reader",
        "fixture:member",
        "fixture:malformed",
        "fixture:unexpected",
        "fixture:log",
        "fixture:unformatted-log",
        "fixture:stream-log",
        "fixture:sourced-log",
    ];

    /// `field_events` with the worker's `hooks-requested` record first. Rows shift by one: 1
    /// launch stop, 2 hooks in place before resume and 3 resume.
    fn with_requested_hooks(requested: &[&str]) -> Vec<Value> {
        let mut events = field_events();
        events.insert(
            0,
            json!({"kind":"hooks-requested", "hooks":requested, "run":"attempt"}),
        );
        renumber(&mut events);
        events
    }

    fn activation_fails(events: Vec<Value>) -> bool {
        reduce(
            &field_request(),
            &records(events),
            &owner(),
            BuildId("b".into()),
        )
        .is_err()
    }

    #[test]
    fn a_setup_that_does_not_match_the_requests_questions_gives_no_answer() {
        let request = two_field_request();
        let events = records(two_field_events());
        let one_question: Vec<_> = request.field_questions[..1]
            .iter()
            .map(|question| {
                FixtureQuestionSetup::new(0, question, bound(FixtureStorageDecoder::String))
            })
            .collect();
        assert!(matches!(
            super::reduce(
                &request,
                &one_question,
                &events,
                &owner(),
                BuildId("b".into())
            ),
            Err(Error::Observation { .. })
        ));
        assert!(reduce(&request, &events, &owner(), BuildId("b".into())).is_ok());
    }

    #[test]
    fn a_minimum_hook_that_the_worker_does_not_name_is_still_required() {
        let requested: Vec<_> = MINIMUM_HOOKS
            .into_iter()
            .filter(|name| *name != "fixture:member")
            .collect();
        let mut events = with_requested_hooks(&requested);
        assert_eq!(answer(events.clone()), answer(field_events()));

        events[2]["hooks"]
            .as_object_mut()
            .unwrap()
            .remove("fixture:member");
        assert!(activation_fails(events));
    }

    #[test]
    fn a_hook_that_only_the_worker_names_must_be_active_before_resume() {
        let hook = json!({"enabled":true, "locations":1, "resolved":1, "hits":0});
        let mut requested = MINIMUM_HOOKS.to_vec();
        requested.extend([
            "fixture:return",
            "fixture:nested:4660",
            "registry:common/traditions",
        ]);
        let mut events = with_requested_hooks(&requested);
        events[2]["hooks"]["fixture:return"] = hook.clone();
        events[2]["hooks"]["fixture:nested:4660"] = hook;
        // Only a fixture hook binds the fixture answer; the registry hook has no state here.
        assert_eq!(answer(events.clone()), answer(field_events()));

        for name in ["fixture:return", "fixture:nested:4660"] {
            let mut absent = events.clone();
            absent[2]["hooks"].as_object_mut().unwrap().remove(name);
            assert!(activation_fails(absent), "{name} absent");

            for (field, value) in [
                ("enabled", json!(false)),
                ("locations", json!(0)),
                ("resolved", json!(2)),
                ("hits", json!(1)),
            ] {
                let mut inactive = events.clone();
                inactive[2]["hooks"][name][field] = value;
                assert!(activation_fails(inactive), "{name} {field}");
            }
        }
    }

    #[test]
    fn later_registry_record_loss_does_not_revoke_a_completed_fixture_window() {
        let expected = answer(field_events());
        let mut later_loss = field_events();
        later_loss.push(json!({"kind":"registry-entry", "run":"attempt", "seq":15,
            "thread":7, "name":"tradition_categories", "owner":"0x4000",
            "index":1, "object":"0x8000", "key":"other"}));
        later_loss.push(json!({"kind":"callback-error", "run":"attempt", "seq":16,
            "error":"a later observation failed"}));
        assert_eq!(answer(later_loss), expected);
    }

    fn parsed_field_request() -> FixtureRequest {
        let mut request = field_request();
        request.field_questions[0] = request.field_questions[0].clone().with_parsing();
        request
    }

    /// Two occurrences of one field, each with its parser entry, storage and return, and a
    /// diagnostic on the second. Row numbers, from 0: 5 to 7 are occurrence 1 (entry, storage,
    /// return), 8 to 11 are occurrence 2 (entry, storage, diagnostic, return), 12 the field
    /// terminal and 13 the parser terminal.
    fn parsed_field_events() -> Vec<Value> {
        let source = "common/traditions/sample.txt";
        let parse = |line, occurrence, returned| json!({"kind":"field-parse","question":0,"file":source,"line":line,"definition":"sample","field":"unlocks_agenda","owner":"0x2000","occurrence":occurrence,"returned":returned});
        let storage = |line, occurrence, value| json!({"kind":"field-storage","question":0,"file":source,"line":line,"definition":"sample","field":"unlocks_agenda","owner":"0x2000","occurrence":occurrence,"value":{"String":value}});
        field_outcome_session(
            vec![
                json!({"kind":"load-start","file":source}),
                json!({"kind":"definition","file":source,"line":1,"definition":"sample","owner":"0x2000"}),
                parse(2, 1, false),
                storage(2, 1, "one"),
                parse(2, 1, true),
                parse(3, 2, false),
                storage(3, 2, "two"),
                json!({"kind":"diagnostic","text":"malformed value","stage":"reader-malformed-report","file":source,"line":3,"definition":"sample","field":"unlocks_agenda","occurrence":2}),
                parse(3, 2, true),
                json!({"kind":"field-terminal","question":0,"owner":"0x2000","final_value":{"String":"two"},"unavailable":null}),
                json!({"kind":"parsing-terminal","question":0,"count":2,"unavailable":null}),
                json!({"kind":"diagnostics-terminal","count":1}),
                json!({"kind":"load-returned","file":source}),
            ],
            json!({"kind":"end","field_outcomes":1,"diagnostics":1}),
        )
    }

    fn parsed_field_answer(events: Vec<Value>) -> Answer<FixtureObservation> {
        reduce(
            &parsed_field_request(),
            &records(events),
            &owner(),
            BuildId("b".into()),
        )
        .unwrap()
    }

    /// The stored and parsed occurrence numbers that the answer keeps for the one question.
    fn kept_occurrences(answer: &Answer<FixtureObservation>) -> (Vec<u64>, Vec<u64>) {
        let outcome = &answer.value.field_outcomes[0];
        let (
            FixtureStorage::Observed {
                occurrences: stored,
                ..
            },
            FixtureParsing::Observed {
                occurrences: parsed,
                ..
            },
        ) = (&outcome.storage, &outcome.parsing)
        else {
            panic!("{outcome:?}")
        };

        (
            stored.iter().map(|item| item.occurrence).collect(),
            parsed.iter().map(|item| item.occurrence).collect(),
        )
    }

    #[test]
    fn a_lost_first_occurrence_keeps_the_later_one_with_its_parser_pair_and_terminal() {
        let complete = parsed_field_answer(parsed_field_events());
        assert_eq!(complete.completeness, Completeness::Complete);
        assert_eq!(kept_occurrences(&complete), (vec![1, 2], vec![1, 2]));

        // The dropped-record fault leaves out the storage and the parser entry of occurrence 1,
        // and the stream keeps their sequence numbers.
        let mut dropped = parsed_field_events();
        dropped.drain(5..7);
        let partial = parsed_field_answer(dropped);

        assert_eq!(partial.completeness, Completeness::Partial);
        let outcome = &partial.value.field_outcomes[0];
        assert_eq!(
            outcome.storage,
            FixtureStorage::Observed {
                occurrences: vec![StoredFieldOccurrence {
                    file: SAMPLE.into(),
                    line: 3,
                    occurrence: 2,
                    value: FixtureValue::String("two".into()),
                }],
                final_value: Some(FixtureValue::String("two".into())),
                completeness: Completeness::Partial,
            }
        );
        assert_eq!(
            outcome.parsing,
            FixtureParsing::Observed {
                occurrences: vec![ParsedFieldOccurrence {
                    file: SAMPLE.into(),
                    line: 3,
                    occurrence: 2,
                    return_line: Some(3),
                }],
                completeness: Completeness::Partial,
            }
        );
        assert_eq!(outcome.diagnostics, [0]);
    }

    #[test]
    fn an_occurrence_number_that_skips_ahead_is_kept_with_a_gap() {
        // Without the sequence gap of a dropped record, only the occurrence number shows the hole.
        let mut skipped = parsed_field_events();
        skipped.drain(5..8);
        renumber(&mut skipped);
        let partial = parsed_field_answer(skipped);

        assert_eq!(kept_occurrences(&partial), (vec![2], vec![2]));
        let outcome = &partial.value.field_outcomes[0];
        assert!(matches!(
            &outcome.storage,
            FixtureStorage::Observed {
                completeness: Completeness::Partial,
                ..
            }
        ));
        for detail in [
            "A stored occurrence is missing",
            "A parser invocation is missing",
        ] {
            assert!(
                partial.gaps.iter().any(|gap| gap.detail == detail),
                "{detail}"
            );
        }
    }

    #[test]
    fn a_repeated_or_earlier_occurrence_number_is_refused() {
        for (row, occurrence, kept) in [
            // Occurrence 2 repeats occurrence 1.
            (9, 1, (vec![1], vec![1, 2])),
            (8, 1, (vec![1, 2], vec![1])),
            // The first kept occurrence skips ahead to 3, and the next one is earlier.
            (6, 3, (vec![3], vec![1, 2])),
            (5, 3, (vec![1, 2], vec![3])),
        ] {
            let mut events = parsed_field_events();
            events[row]["event"]["occurrence"] = json!(occurrence);
            let answer = parsed_field_answer(events);

            assert_eq!(kept_occurrences(&answer), kept, "row {row}");
            assert_eq!(answer.completeness, Completeness::Partial, "row {row}");
        }
    }

    #[test]
    fn damaged_tail_invalidates_fixture_terminal_even_after_a_complete_window() {
        let mut raw = field_events()
            .into_iter()
            .map(|event| format!("{event}\n"))
            .collect::<String>();
        raw.push('{');
        let (records, damage) = event_stream::read_worker_stream(raw.as_bytes(), "attempt");
        assert!(damage.is_some());
        let answer = reduce(&field_request(), &records, &owner(), BuildId("b".into())).unwrap();
        assert_eq!(answer.completeness, Completeness::Partial);
        assert_eq!(answer.value.field_outcomes.len(), 1);
        assert!(matches!(
            answer.value.field_outcomes[0].storage,
            FixtureStorage::Observed { .. }
        ));
    }

    #[test]
    fn field_storage_and_diagnostics_are_independent() {
        let complete = reduce(
            &field_request(),
            &records(field_events()),
            &owner(),
            BuildId("b".into()),
        )
        .unwrap();
        assert_eq!(complete.completeness, Completeness::Complete);
        assert_eq!(
            complete.value.diagnostic_coverage,
            DiagnosticCoverage::Complete {
                window: DiagnosticWindow::FixtureFileLoad,
            }
        );
        assert_eq!(complete.value.diagnostics.len(), 1);
        let outcome = &complete.value.field_outcomes[0];
        assert_eq!(outcome.diagnostics, [0]);
        assert!(matches!(
            &outcome.storage,
            FixtureStorage::Observed { occurrences, final_value, completeness }
                if occurrences.iter().map(|item| item.value.clone()).collect::<Vec<_>>() == [FixtureValue::String("one".into()), FixtureValue::String("two".into())]
                    && final_value.as_ref() == Some(&FixtureValue::String("two".into()))
                    && *completeness == Completeness::Complete
        ));

        let mut missing_diagnostic_terminal = field_events();
        missing_diagnostic_terminal.remove(9);
        let partial = reduce(
            &field_request(),
            &records(missing_diagnostic_terminal),
            &owner(),
            BuildId("b".into()),
        )
        .unwrap();
        assert_eq!(partial.completeness, Completeness::Partial);
        assert_eq!(partial.value.diagnostics.len(), 1);
        assert!(matches!(
            partial.value.diagnostic_coverage,
            DiagnosticCoverage::Unavailable(_)
        ));

        let mut unjoined_diagnostic = field_events();
        unjoined_diagnostic[6]["event"]["file"] = json!("foreign.txt");
        unjoined_diagnostic[6]["event"]["line"] = json!(0);
        let partial = reduce(
            &field_request(),
            &records(unjoined_diagnostic),
            &owner(),
            BuildId("b".into()),
        )
        .unwrap();
        assert_eq!(partial.completeness, Completeness::Partial);
        assert_eq!(partial.value.diagnostics.len(), 1);
        assert!(matches!(
            partial.value.diagnostics[0].join,
            DiagnosticJoin::Unavailable(_)
        ));

        let mut inconsistent_subjoin = field_events();
        inconsistent_subjoin[6]["event"]["line"] = json!(3);
        let partial = reduce(
            &field_request(),
            &records(inconsistent_subjoin),
            &owner(),
            BuildId("b".into()),
        )
        .unwrap();
        assert_eq!(partial.completeness, Completeness::Partial);
        assert_eq!(
            partial.value.diagnostics[0].join,
            DiagnosticJoin::Source {
                file: SAMPLE.into(),
                line: 3,
                definition: None,
                field: None,
                occurrence: None,
            }
        );
        assert!(matches!(
            partial.value.diagnostic_coverage,
            DiagnosticCoverage::Unavailable(_)
        ));
    }

    #[test]
    fn unsupported_reader_kinds_return_typed_storage_gaps() {
        let reason = "Storage decoder unavailable";
        for (storage, gap_kind, completeness) in [
            (
                FixtureQuestionStorage::UnresolvedReader(reason.into()),
                GapKind::UnresolvedReader,
                Completeness::Partial,
            ),
            (
                FixtureQuestionStorage::Unbound(reason.into()),
                GapKind::OutsideMethod,
                Completeness::Complete,
            ),
        ] {
            let answer = reduce_with(
                &field_request(),
                storage,
                &records(unavailable_field_events()),
                &owner(),
                BuildId("b".into()),
            )
            .unwrap();
            assert_eq!(answer.completeness, completeness);
            assert!(matches!(
                answer.value.field_outcomes[0].storage,
                FixtureStorage::Unavailable(ref reason) if reason == "Storage decoder unavailable"
            ));
            assert!(answer.gaps.iter().any(|gap| gap.kind == gap_kind));
        }
    }

    #[test]
    fn established_string_without_a_decoder_is_outside_the_method() {
        let mut request = category_outcome_request();
        request.field_questions[0].diagnostics = false;
        let mut events = category_outcome_events();
        events.retain(|event| {
            event.pointer("/event/kind").and_then(Value::as_str) != Some("diagnostics-unavailable")
        });
        renumber(&mut events);
        let answer = reduce_with(
            &request,
            category_storage(),
            &records(events),
            &owner(),
            BuildId("b".into()),
        )
        .unwrap();
        assert_eq!(answer.completeness, Completeness::Complete);
        assert!(
            answer
                .gaps
                .iter()
                .any(|gap| gap.kind == GapKind::OutsideMethod)
        );
        assert!(
            answer
                .gaps
                .iter()
                .all(|gap| gap.kind != GapKind::IncompleteObservation)
        );
    }

    #[test]
    fn diagnostics_require_requested_supported_intact_terminals() {
        let not_requested = reduce(
            &field_request_without_diagnostics(),
            &records(no_diagnostic_events()),
            &owner(),
            BuildId("b".into()),
        )
        .unwrap();
        assert_eq!(
            not_requested.value.diagnostic_coverage,
            DiagnosticCoverage::NotRequested
        );
        assert!(not_requested.value.diagnostics.is_empty());

        let unsupported = reduce_with(
            &category_outcome_request(),
            category_storage(),
            &records(category_outcome_events()),
            &owner(),
            BuildId("b".into()),
        )
        .unwrap();
        assert_eq!(unsupported.completeness, Completeness::Complete);
        assert!(matches!(
            unsupported.value.diagnostic_coverage,
            DiagnosticCoverage::Unavailable(_)
        ));
        assert!(
            unsupported
                .gaps
                .iter()
                .any(|gap| gap.kind == GapKind::OutsideMethod)
        );

        let mut cases = Vec::new();
        let mut wrong_count = field_events();
        wrong_count[9]["event"]["count"] = json!(2);
        cases.push(wrong_count);
        let mut duplicate_terminal = field_events();
        duplicate_terminal.insert(10, duplicate_terminal[9].clone());
        renumber(&mut duplicate_terminal);
        cases.push(duplicate_terminal);
        let mut premature_terminal = field_events();
        premature_terminal.swap(6, 9);
        renumber(&mut premature_terminal);
        cases.push(premature_terminal);
        let mut missing_return = field_events();
        missing_return.remove(10);
        renumber(&mut missing_return);
        cases.push(missing_return);
        let mut missing_end = field_events();
        missing_end.pop();
        cases.push(missing_end);
        let mut diagnostic_after_terminal = field_events();
        diagnostic_after_terminal.insert(10, diagnostic_after_terminal[6].clone());
        diagnostic_after_terminal.last_mut().unwrap()["event"]["diagnostics"] = json!(2);
        renumber(&mut diagnostic_after_terminal);
        cases.push(diagnostic_after_terminal);
        for events in cases {
            let answer = reduce(
                &field_request(),
                &records(events),
                &owner(),
                BuildId("b".into()),
            )
            .unwrap();
            assert_eq!(answer.completeness, Completeness::Partial);
            assert!(matches!(
                answer.value.diagnostic_coverage,
                DiagnosticCoverage::Unavailable(_)
            ));
        }

        let mut raw = field_events()
            .into_iter()
            .map(|event| format!("{event}\n"))
            .collect::<String>();
        raw.push('{');
        let (records, damage) = event_stream::read_worker_stream(raw.as_bytes(), "attempt");
        assert!(damage.is_some());
        let answer = reduce(&field_request(), &records, &owner(), BuildId("b".into())).unwrap();
        assert!(matches!(
            answer.value.diagnostic_coverage,
            DiagnosticCoverage::Unavailable(_)
        ));
        assert!(matches!(
            answer.value.field_outcomes[0].storage,
            FixtureStorage::Observed {
                final_value: Some(_),
                completeness: Completeness::Partial,
                ..
            }
        ));
    }

    #[test]
    fn storage_and_lifecycle_fail_independently() {
        let mut missing_final = field_events();
        missing_final[8]["event"]["final_value"] = Value::Null;
        missing_final[8]["event"]["unavailable"] = json!("Final storage read failed");
        let answer = reduce(
            &field_request(),
            &records(missing_final),
            &owner(),
            BuildId("b".into()),
        )
        .unwrap();
        assert!(matches!(
            answer.value.field_outcomes[0].storage,
            FixtureStorage::Observed {
                ref occurrences,
                final_value: None,
                completeness: Completeness::Partial,
            } if occurrences.len() == 2
        ));

        let mut storage_after_terminal = field_events();
        storage_after_terminal.insert(9, storage_after_terminal[7].clone());
        storage_after_terminal[9]["event"]["occurrence"] = json!(3);
        renumber(&mut storage_after_terminal);
        let answer = reduce(
            &field_request(),
            &records(storage_after_terminal),
            &owner(),
            BuildId("b".into()),
        )
        .unwrap();
        assert_eq!(answer.completeness, Completeness::Partial);

        let mut missing_terminal = field_events();
        missing_terminal.remove(8);
        renumber(&mut missing_terminal);
        let answer = reduce(
            &field_request(),
            &records(missing_terminal),
            &owner(),
            BuildId("b".into()),
        )
        .unwrap();
        assert_eq!(answer.completeness, Completeness::Partial);
    }

    #[test]
    fn owner_identity_follows_the_engine_object_and_ignores_addresses() {
        let expected = reduce(
            &two_field_request(),
            &records(two_field_events()),
            &owner(),
            BuildId("b".into()),
        )
        .unwrap();
        assert_eq!(expected.completeness, Completeness::Complete);
        assert_eq!(
            expected.value.field_outcomes[0].definitions,
            expected.value.field_outcomes[1].definitions
        );
        let mut relocated = two_field_events();
        for event in &mut relocated {
            if event.pointer("/event/owner").and_then(Value::as_str) == Some("0x2000") {
                event["event"]["owner"] = json!("0x8000");
            }
        }
        let relocated = reduce(
            &two_field_request(),
            &records(relocated),
            &owner(),
            BuildId("b".into()),
        )
        .unwrap();
        assert_eq!(relocated, expected);

        let different = reduce(
            &different_owner_request(),
            &records(different_owner_events()),
            &owner(),
            BuildId("b".into()),
        )
        .unwrap();
        assert_eq!(different.completeness, Completeness::Complete);
        assert_ne!(
            different.value.field_outcomes[0].definitions,
            different.value.field_outcomes[1].definitions
        );
    }

    #[test]
    fn repair_regressions_require_independent_terminals_and_partial_preservation() {
        let mut entry_only = field_events();
        entry_only.insert(
            10,
            json!({"kind":"fixture", "event":{"kind":"diagnostics-terminal","count":0},
                "thread":7,"run":"attempt"}),
        );
        renumber(&mut entry_only);
        assert!(!matches!(
            answer(entry_only).value.diagnostic_coverage,
            DiagnosticCoverage::Complete { .. }
        ));

        let mut missing_field_terminal = field_events();
        missing_field_terminal.remove(8);
        renumber(&mut missing_field_terminal);
        let partial = reduce(
            &field_request(),
            &records(missing_field_terminal),
            &owner(),
            BuildId("b".into()),
        )
        .unwrap();
        assert_eq!(partial.completeness, Completeness::Partial);
        assert!(matches!(
            &partial.value.field_outcomes[0].storage,
            FixtureStorage::Observed {
                occurrences,
                final_value: None,
                completeness: Completeness::Partial,
            } if occurrences.len() == 2
        ));

        let mut duplicate_field_terminal = field_events();
        duplicate_field_terminal.insert(9, duplicate_field_terminal[8].clone());
        renumber(&mut duplicate_field_terminal);
        let partial = reduce(
            &field_request(),
            &records(duplicate_field_terminal),
            &owner(),
            BuildId("b".into()),
        )
        .unwrap();
        assert_eq!(partial.completeness, Completeness::Partial);
        assert!(matches!(
            &partial.value.field_outcomes[0].storage,
            FixtureStorage::Observed { occurrences, .. } if occurrences.len() == 2
        ));

        let mut missing_end = field_events();
        missing_end.pop();
        let partial = reduce(
            &field_request(),
            &records(missing_end),
            &owner(),
            BuildId("b".into()),
        )
        .unwrap();
        assert_eq!(partial.completeness, Completeness::Partial);
        assert_eq!(partial.value.field_outcomes.len(), 1);

        let mut wrong_diagnostic_count = field_events();
        wrong_diagnostic_count[9]["event"]["count"] = json!(2);
        let partial = reduce(
            &field_request(),
            &records(wrong_diagnostic_count),
            &owner(),
            BuildId("b".into()),
        )
        .unwrap();
        assert_eq!(partial.completeness, Completeness::Partial);
        assert!(!matches!(
            partial.value.diagnostic_coverage,
            DiagnosticCoverage::Complete { .. }
        ));
    }
    #[test]
    fn nested_diagnostics_require_the_matching_parent_selector() {
        let mut request = field_request();
        request.field_questions[0].parent_field = Some("requirements".into());
        let run =
            |events| reduce(&request, &records(events), &owner(), BuildId("b".into())).unwrap();
        let mut events = field_events();
        for row in &mut events {
            if row["event"]["kind"] == "diagnostic" {
                row["event"]["parent_field"] = json!("requirements");
            }
        }
        let answer = run(events.clone());
        assert_eq!(answer.completeness, Completeness::Complete);
        assert_eq!(answer.value.field_outcomes[0].diagnostics, vec![0]);
        for row in &mut events {
            if row["event"]["kind"] == "diagnostic" {
                row["event"]["parent_field"] = json!("other");
            }
        }
        let answer = run(events);
        assert_eq!(answer.completeness, Completeness::Partial);
        assert!(answer.value.field_outcomes[0].diagnostics.is_empty());
    }

    #[test]
    fn numeric_storage_keeps_exact_values_and_rejects_wrong_types_and_scales() {
        for (decoder, first, last) in [
            (
                json!("Integer"),
                json!({"Integer": i32::MIN}),
                json!({"Integer": i32::MAX}),
            ),
            (
                json!({"FixedPoint":{"scale":100000}}),
                json!({"FixedPoint":{"raw":i64::MIN,"scale":100000}}),
                json!({"FixedPoint":{"raw":125000,"scale":100000}}),
            ),
            (
                json!({"FixedPoint":{"scale":32768}}),
                json!({"FixedPoint":{"raw":i64::MAX,"scale":32768}}),
                json!({"FixedPoint":{"raw":-40960,"scale":32768}}),
            ),
        ] {
            let mut events = field_events();
            events[5]["event"]["value"] = first.clone();
            events[7]["event"]["value"] = last.clone();
            events[8]["event"]["final_value"] = last.clone();
            let run = |events| {
                reduce_with(
                    &field_request(),
                    bound(serde_json::from_value(decoder.clone()).unwrap()),
                    &records(events),
                    &owner(),
                    BuildId("b".into()),
                )
                .unwrap()
            };
            let answer = run(events.clone());
            assert_eq!(answer.completeness, Completeness::Complete);
            let FixtureStorage::Observed {
                occurrences,
                final_value,
                completeness,
            } = &answer.value.field_outcomes[0].storage
            else {
                panic!("{answer:?}")
            };
            assert_eq!(*completeness, Completeness::Complete);
            assert_eq!(serde_json::to_value(&occurrences[0].value).unwrap(), first);
            assert_eq!(serde_json::to_value(final_value).unwrap(), last);
            assert_eq!(answer.value.diagnostics.len(), 1);
            for wrong in [
                json!({"String":"wrong"}),
                json!({"FixedPoint":{"raw":1,"scale":0}}),
                json!({"FixedPoint":{"raw":1,"scale":42}}),
            ] {
                let mut invalid = events.clone();
                invalid[7]["event"]["value"] = wrong.clone();
                invalid[8]["event"]["final_value"] = wrong;
                let answer = run(invalid);
                let FixtureStorage::Observed {
                    occurrences,
                    final_value,
                    completeness,
                } = &answer.value.field_outcomes[0].storage
                else {
                    panic!("{answer:?}")
                };
                assert_eq!(occurrences.len(), 1);
                assert!(final_value.is_none());
                assert_eq!(*completeness, Completeness::Partial);
            }
            let mut missing = events.clone();
            missing.retain(|row| row["event"]["kind"] != "field-terminal");
            renumber(&mut missing);
            let answer = run(missing);
            assert!(matches!(&answer.value.field_outcomes[0].storage,
                FixtureStorage::Observed { occurrences, final_value: None, completeness: Completeness::Partial } if occurrences.len() == 2));
        }
    }
    #[test]
    fn scoped_storage_preserves_independent_state_and_rejects_lost_evidence() {
        use crate::protocol::observation::{ScopedLiteralDecoder, ScopedStorageLayout};
        for literal in [
            ScopedLiteralDecoder::Integer,
            ScopedLiteralDecoder::FixedPoint { scale: 100_000 },
        ] {
            let decoder = FixtureStorageDecoder::ScopedNumeric {
                literal,
                layout: ScopedStorageLayout {
                    literal: 0x200,
                    location: 0x1d8,
                    trigger: 0x1a0,
                    script_value: 0x1a8,
                    modifier: 0x198,
                    modifier_unset: 0x7fffffff,
                    variable: 0x1b0,
                },
            };
            let value = |number| {
                json!({"ScopedNumeric":{
                    "literal":match literal {
                        ScopedLiteralDecoder::Integer => json!({"Integer":number}),
                        ScopedLiteralDecoder::FixedPoint { scale } => json!({"FixedPoint":{"raw":number,"scale":scale}}),
                    },
                    "has_source_location":true,"has_trigger":false,"has_script_value":true,
                    "has_modifier":false,"variable":""
                }})
            };
            let mut events = field_events();
            events[5]["event"]["value"] = value(0);
            events[7]["event"]["value"] = value(9);
            events[8]["event"]["final_value"] = value(9);
            let run = |events| {
                reduce_with(
                    &field_request(),
                    bound(decoder),
                    &records(events),
                    &owner(),
                    BuildId("b".into()),
                )
                .unwrap()
            };
            let answer = run(events.clone());
            assert_eq!(answer.completeness, Completeness::Complete);
            let FixtureStorage::Observed {
                occurrences,
                final_value,
                ..
            } = &answer.value.field_outcomes[0].storage
            else {
                panic!("{answer:?}");
            };
            assert_eq!(
                serde_json::to_value(&occurrences[1].value).unwrap(),
                value(9)
            );
            assert_eq!(serde_json::to_value(final_value).unwrap(), value(9));
            let mut lost = events.clone();
            lost.remove(5);
            assert_eq!(run(lost).completeness, Completeness::Partial);
            let mut wrong = events;
            wrong[7]["event"]["value"] = json!({"Integer":9});
            assert_eq!(run(wrong).completeness, Completeness::Partial);
        }
    }

    /// A key repeated three times, the last two on one line; the last occurrence omits the field.
    fn repeated_key_request() -> FixtureRequest {
        let mut question =
            crate::FixtureFieldQuestion::new("common/traditions", "sample", "unlocks_agenda");
        question.diagnostics = false;
        FixtureRequest::field_outcomes(
            "common/traditions/sample.txt",
            "sample = {\n unlocks_agenda = \"one\"\n}\nsample = { unlocks_agenda = \"two\" } sample = {}\n",
            [question],
        )
    }

    /// Row numbers, from 0: 3 load start, 4, 6 and 8 the definitions, 5 and 7 storage, 9 the
    /// terminal, 10 return and 11 end.
    fn repeated_key_events() -> Vec<Value> {
        let file = "common/traditions/sample.txt";
        let definition = |line| json!({"kind":"definition","file":file,"line":line,"definition":"sample","owner":"0x2000"});
        let storage = |line, occurrence, value| json!({"kind":"field-storage","question":0,"file":file,"line":line,"definition":"sample","field":"unlocks_agenda","owner":"0x2000","occurrence":occurrence,"value":{"String":value}});
        field_outcome_session(
            vec![
                json!({"kind":"load-start","file":file}),
                definition(1),
                storage(2, 1, "one"),
                definition(4),
                storage(4, 2, "two"),
                definition(4),
                json!({"kind":"field-terminal","question":0,"owner":"0x2000","final_value":{"String":""},"unavailable":null}),
                json!({"kind":"load-returned","file":file}),
            ],
            json!({"kind":"end","field_outcomes":1,"diagnostics":0}),
        )
    }

    fn repeated_key_answer(events: Vec<Value>) -> Answer<FixtureObservation> {
        reduce(
            &repeated_key_request(),
            &records(events),
            &owner(),
            BuildId("b".into()),
        )
        .unwrap()
    }

    #[test]
    fn a_repeated_key_keeps_each_construction_and_reads_the_final_value_once() {
        let answer = repeated_key_answer(repeated_key_events());

        assert_eq!(answer.completeness, Completeness::Complete);
        let outcome = &answer.value.field_outcomes[0];
        let owner = FixtureOwnerId(1);
        assert_eq!(
            outcome.definitions,
            [1, 4, 4].map(|line| FixtureDefinition {
                file: SAMPLE.into(),
                line,
                owner: owner.clone(),
            })
        );
        assert_eq!(
            outcome.storage,
            FixtureStorage::Observed {
                occurrences: vec![
                    StoredFieldOccurrence {
                        file: SAMPLE.into(),
                        line: 2,
                        occurrence: 1,
                        value: FixtureValue::String("one".into()),
                    },
                    StoredFieldOccurrence {
                        file: SAMPLE.into(),
                        line: 4,
                        occurrence: 2,
                        value: FixtureValue::String("two".into()),
                    },
                ],
                final_value: Some(FixtureValue::String(String::new())),
                completeness: Completeness::Complete,
            }
        );
    }

    #[test]
    fn a_repeated_key_out_of_order_after_its_terminal_or_with_a_lost_record_is_partial() {
        let mut earlier_line = repeated_key_events();
        earlier_line[8]["event"]["line"] = json!(2);
        let mut after_terminal = repeated_key_events();
        after_terminal.swap(8, 9);
        renumber(&mut after_terminal);
        let mut lost = repeated_key_events();
        lost.remove(6);

        for (case, events) in [
            ("earlier line", earlier_line),
            ("after terminal", after_terminal),
            ("lost record", lost),
        ] {
            let answer = repeated_key_answer(events);

            assert_eq!(answer.completeness, Completeness::Partial, "{case}");
        }
    }

    #[test]
    fn a_repeated_key_with_several_owners_keeps_its_values_and_the_reason_for_no_final_value() {
        let reason =
            "The repeated definition key has several owners; the stored one is not established";
        let mut events = repeated_key_events();
        for row in [6, 7, 8] {
            events[row]["event"]["owner"] = json!("0x3000");
        }
        events[9]["event"] = json!({"kind":"field-terminal","question":0,"owner":null,"final_value":null,"unavailable":reason});

        let answer = repeated_key_answer(events);

        assert_eq!(answer.completeness, Completeness::Partial);
        assert!(answer.gaps.iter().any(|gap| gap.detail == reason));
        let outcome = &answer.value.field_outcomes[0];
        assert_eq!(
            outcome
                .definitions
                .iter()
                .map(|definition| definition.owner.clone())
                .collect::<Vec<_>>(),
            [FixtureOwnerId(1), FixtureOwnerId(2), FixtureOwnerId(2)]
        );
        let FixtureStorage::Observed {
            occurrences,
            final_value: None,
            completeness: Completeness::Partial,
        } = &outcome.storage
        else {
            panic!("storage: {:?}", outcome.storage);
        };
        assert_eq!(occurrences.len(), 2);
    }

    const FIRST: &str = "common/traditions/a.txt";
    const SECOND: &str = "common/traditions/b.txt";

    /// One key in two files: the second file's definition replaces the first.
    fn two_file_request() -> FixtureRequest {
        FixtureRequest::field_outcomes(
            FIRST,
            "sample = {\n unlocks_agenda = \"one\"\n}\n",
            [
                crate::FixtureFieldQuestion::new("common/traditions", "sample", "unlocks_agenda")
                    .with_parsing(),
            ],
        )
        .with_file(SECOND, "sample = {\n unlocks_agenda = \"two\"\n}\n")
    }

    /// The window of one file: its definition at line 1 and its field at line 2.
    fn file_window(file: &str, occurrence: u64, value: &str) -> Vec<Value> {
        let parse = |returned| json!({"kind":"field-parse","question":0,"file":file,"line":2,"definition":"sample","field":"unlocks_agenda","owner":"0x2000","occurrence":occurrence,"returned":returned});
        vec![
            json!({"kind":"load-start","file":file}),
            json!({"kind":"definition","file":file,"line":1,"definition":"sample","owner":"0x2000"}),
            parse(false),
            parse(true),
            json!({"kind":"field-storage","question":0,"file":file,"line":2,"definition":"sample","field":"unlocks_agenda","owner":"0x2000","occurrence":occurrence,"value":{"String":value}}),
        ]
    }

    /// The last file also logs the repeated key and ends every terminal before its return.
    fn two_file_events() -> Vec<Value> {
        let mut window = file_window(FIRST, 1, "one");
        window.push(json!({"kind":"load-returned","file":FIRST}));
        window.extend(file_window(SECOND, 2, "two"));
        window.extend([
            json!({"kind":"diagnostic","text":"Object with key: sample already exists","stage":"engine-parser-log","file":SECOND,"line":1,"definition":null,"field":null,"occurrence":null}),
            json!({"kind":"field-terminal","question":0,"owner":"0x2000","final_value":{"String":"two"},"unavailable":null}),
            json!({"kind":"parsing-terminal","question":0,"count":2,"unavailable":null}),
            json!({"kind":"diagnostics-terminal","count":1}),
            json!({"kind":"load-returned","file":SECOND}),
        ]);

        field_outcome_session(
            window,
            json!({"kind":"end","field_outcomes":1,"diagnostics":1}),
        )
    }

    fn two_file_answer(request: &FixtureRequest, events: Vec<Value>) -> Answer<FixtureObservation> {
        reduce(request, &records(events), &owner(), BuildId("b".into())).unwrap()
    }

    fn position(events: &[Value], kind: &str, file: &str) -> usize {
        events
            .iter()
            .position(|event| event["event"]["kind"] == kind && event["event"]["file"] == file)
            .unwrap()
    }

    #[test]
    fn two_files_report_their_load_order_and_attribute_each_occurrence_to_its_file() {
        let answer = two_file_answer(&two_file_request(), two_file_events());

        assert_eq!(answer.completeness, Completeness::Complete, "{answer:?}");
        assert_eq!(answer.value.loaded_files, [FIRST, SECOND]);
        let outcome = &answer.value.field_outcomes[0];
        assert_eq!(
            outcome.definitions,
            [FIRST, SECOND].map(|file| FixtureDefinition {
                file: file.into(),
                line: 1,
                owner: FixtureOwnerId(1),
            })
        );
        assert_eq!(
            outcome.storage,
            FixtureStorage::Observed {
                occurrences: [(FIRST, 1, "one"), (SECOND, 2, "two")]
                    .map(|(file, occurrence, value)| StoredFieldOccurrence {
                        file: file.into(),
                        line: 2,
                        occurrence,
                        value: FixtureValue::String(value.into()),
                    })
                    .to_vec(),
                final_value: Some(FixtureValue::String("two".into())),
                completeness: Completeness::Complete,
            }
        );
        assert_eq!(
            outcome.parsing,
            FixtureParsing::Observed {
                occurrences: [(FIRST, 1), (SECOND, 2)]
                    .map(|(file, occurrence)| ParsedFieldOccurrence {
                        file: file.into(),
                        line: 2,
                        occurrence,
                        return_line: Some(2),
                    })
                    .to_vec(),
                completeness: Completeness::Complete,
            }
        );
        assert!(matches!(
            &answer.value.diagnostics[0].join,
            DiagnosticJoin::Source { file, line: 1, .. } if file == SECOND
        ));
    }

    #[test]
    fn a_missing_second_load_window_gives_a_partial_answer_that_names_the_file() {
        let mut events = two_file_events();
        events.truncate(position(&events, "load-start", SECOND));
        renumber(&mut events);

        let answer = two_file_answer(&two_file_request(), events);

        assert_eq!(answer.completeness, Completeness::Partial);
        assert_eq!(answer.value.loaded_files, [FIRST]);
        assert!(answer.gaps.iter().any(|gap| {
            gap.subject == Some(GapSubject::fixture_file(SECOND))
                && gap.detail == "The fixture file's load window is missing"
        }));
        let FixtureStorage::Observed {
            final_value: None,
            completeness: Completeness::Partial,
            ..
        } = &answer.value.field_outcomes[0].storage
        else {
            panic!("storage: {answer:?}");
        };
    }

    #[test]
    fn events_outside_the_loading_file_or_terminals_before_the_last_file_are_partial() {
        let mut other_file = two_file_events();
        let definition = position(&other_file, "definition", SECOND);
        other_file[definition]["event"]["file"] = json!(FIRST);

        let mut early_terminal = two_file_events();
        let terminal = early_terminal
            .iter()
            .position(|event| event["event"]["kind"] == "field-terminal")
            .unwrap();
        let terminal = early_terminal.remove(terminal);
        early_terminal.insert(position(&early_terminal, "load-returned", FIRST), terminal);
        renumber(&mut early_terminal);

        // The first file's invocation never returns; a return with its number in the second file
        // must not close it.
        let mut crossed_return = two_file_events();
        let first_return = crossed_return
            .iter()
            .position(|event| event["event"]["returned"] == true)
            .unwrap();
        let mut moved = crossed_return.remove(first_return);
        moved["event"]["file"] = json!(SECOND);
        crossed_return.insert(position(&crossed_return, "definition", SECOND) + 1, moved);
        renumber(&mut crossed_return);

        for (case, events) in [
            ("other file", other_file),
            ("early terminal", early_terminal),
            ("crossed return", crossed_return),
        ] {
            let answer = two_file_answer(&two_file_request(), events);

            assert_eq!(answer.completeness, Completeness::Partial, "{case}");
        }
    }

    #[test]
    fn validation_keeps_a_late_diagnostic_from_the_first_file_after_both_files_return() {
        let mut events = two_file_events();
        let hooks = &mut events[1]["hooks"];
        hooks["fixture:validated"] = hooks["fixture:malformed"].clone();
        let log = events
            .iter()
            .position(|event| event["event"]["kind"] == "diagnostic")
            .unwrap();
        let mut diagnostic = events.remove(log);
        diagnostic["event"]["stage"] = json!("engine-validation-log");
        diagnostic["event"]["file"] = json!(FIRST);
        diagnostic["event"]["line"] = json!(2);
        let terminal = events
            .iter()
            .position(|event| event["event"]["kind"] == "diagnostics-terminal")
            .unwrap();
        let mut terminal = events.remove(terminal);
        let end = events.pop().unwrap();
        let mut validated = terminal.clone();
        validated["event"] = json!({"kind":"validation-complete"});
        terminal["event"] = json!({"kind":"diagnostics-terminal","count":1});
        events.extend([diagnostic, validated, terminal, end]);
        renumber(&mut events);

        let answer = two_file_answer(&two_file_request().through_validation(), events);

        assert_eq!(answer.completeness, Completeness::Complete, "{answer:?}");
        assert_eq!(
            answer.value.diagnostic_coverage,
            DiagnosticCoverage::Complete {
                window: DiagnosticWindow::FixtureFileLoadAndValidation
            }
        );
        assert!(matches!(
            &answer.value.diagnostics[0].join,
            DiagnosticJoin::Source { file, line: 2, occurrence: Some(1), .. } if file == FIRST
        ));
    }
}
