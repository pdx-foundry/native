//! Reduce the bounded fixture window. Native joins addresses internally, then replaces them
//! with session-local owner identities. Only witnessed read entries leave this module.
use super::event_stream::{self, OwnerEvent, WorkerEvent, WorkerRecord};
use crate::{
    Answer, Basis, BuildId, Completeness, DiagnosticCoverage, DiagnosticJoin, DiagnosticWindow,
    Error, FieldRead, FixtureDiagnostic, FixtureFieldOutcome, FixtureObservation,
    FixtureObservationKind as Kind, FixtureOwnerId, FixtureRequest, FixtureRuntime, FixtureStorage,
    Gap, GapKind, Operation, ProcessingStage, Reader, ReaderId, ReaderKind, RegistrationEntry,
    Source, StoredStringOccurrence,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Worker events inside one fixture window. Thread and sequence belong to the outer record.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) enum FixtureEvent {
    RegistrationEntry {
        ordinal: u64,
    },
    RegistrationEnd {
        count: u64,
    },
    LoadStart {
        file: String,
    },
    FieldRead {
        file: String,
        line: u64,
        field: String,
        owner: String,
        ordinal: u64,
    },
    Definition {
        file: String,
        line: u64,
        definition: String,
        owner: String,
    },
    FieldAuthority {
        question: u64,
        reader_id: Option<String>,
        reader_kind: String,
        storage_supported: bool,
        unavailable: Option<String>,
    },
    FieldStorage {
        question: u64,
        file: String,
        line: u64,
        definition: String,
        field: String,
        owner: String,
        occurrence: u64,
        value: String,
    },
    Diagnostic {
        text: String,
        stage: String,
        file: Option<String>,
        line: Option<u64>,
        definition: Option<String>,
        field: Option<String>,
        occurrence: Option<u64>,
    },
    FieldTerminal {
        question: u64,
        owner: Option<String>,
        definition_line: Option<u64>,
        reader_id: Option<String>,
        reader_kind: String,
        final_value: Option<String>,
        unavailable: Option<String>,
    },
    DiagnosticsTerminal {
        count: u64,
    },
    DiagnosticsUnavailable {
        reason: String,
    },
    LoadReturned {
        file: String,
        field_count: u64,
    },
    End {
        registrations: u64,
        field_reads: u64,
        #[serde(default)]
        field_outcomes: u64,
        #[serde(default)]
        diagnostics: u64,
        producer_last_sequence: u64,
    },
    Unavailable {
        reason: String,
    },
}

fn parse_reader_kind(kind: &str) -> ReaderKind {
    match kind {
        "Boolean" => ReaderKind::Boolean,
        "Integer" => ReaderKind::Integer,
        "FixedPoint" => ReaderKind::FixedPoint,
        "String" => ReaderKind::String,
        "Reference" => ReaderKind::Reference,
        "Block" => ReaderKind::Block,
        _ => ReaderKind::Unknown,
    }
}

pub(crate) fn reduce(
    request: &FixtureRequest,
    records: &[WorkerRecord],
    owner_events: &[OwnerEvent],
    build: BuildId,
) -> Result<Answer<FixtureObservation>, Error> {
    let mut hooks = vec!["fixture:load"];
    if request.requests(Kind::RegistrationEntries) {
        hooks.push("fixture:registration");
    }
    if request.requests(Kind::CategoryFieldReads) {
        hooks.push("fixture:field");
    }
    let diagnostics_requested = request
        .field_questions
        .iter()
        .any(|question| question.diagnostics);
    if !request.field_questions.is_empty() && request.registry() != "common/tradition_categories" {
        hooks.extend(["fixture:constructor", "fixture:reader", "fixture:member"]);
        if diagnostics_requested {
            hooks.extend(["fixture:malformed", "fixture:unexpected"]);
        }
    }
    let Some((thread, resumed)) = event_stream::activation(records, owner_events, &hooks) else {
        return Err(Error::Observation {
            operation: Operation::ObserveFixture,
            reason: "Fixture hook activation before resume was not established".into(),
        });
    };
    let mut window = Window::new(request, thread, resumed);
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
    if !window.returned {
        window.window_gap("The matching fixture loader return is missing");
    }
    for index in 0..request.field_questions.len() as u64 {
        if !window.field_terminals.contains_key(&index) {
            window.field_gap(index, "The field observation terminal is missing");
        }
        if !window.field_authorities.contains_key(&index) {
            window.field_gap(index, "The field reader authority is missing");
        }
    }
    if diagnostics_requested && window.diagnostic_terminal.is_none() {
        window.diagnostic_gap("The parser diagnostic terminal is missing");
    }
    let stopping = owner_events
        .iter()
        .any(|event| matches!(event, OwnerEvent::WorkerStopRequested));
    if !stopping
        && owner_events.iter().any(
            |event| matches!(event, OwnerEvent::WorkerExited { returncode } if *returncode != 0),
        )
    {
        window.window_gap("The observation worker was lost");
    }
    Ok(window.finish(build))
}

#[derive(Clone)]
struct DefinitionState {
    native_owner: String,
    public_owner: FixtureOwnerId,
    line: u64,
}

struct FieldAuthorityState {
    reader: Reader,
    coherent: bool,
    storage_supported: bool,
    unavailable: Option<String>,
}

struct FieldTerminalState {
    final_value: Option<String>,
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
}

enum DiagnosticTerminalState {
    Complete,
    Unavailable(String),
}

struct Window<'a> {
    request: &'a FixtureRequest,
    thread: u64,
    resumed: u64,
    next_sequence: u64,
    loading: bool,
    returned: bool,
    registrations_ended: bool,
    ended: bool,
    owner: Option<String>,
    last_field_ordinal: u64,
    owner_ids: BTreeMap<String, FixtureOwnerId>,
    next_owner_id: u64,
    definitions: BTreeMap<String, DefinitionState>,
    field_authorities: BTreeMap<u64, FieldAuthorityState>,
    occurrences: BTreeMap<u64, Vec<StoredStringOccurrence>>,
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
    fn new(request: &'a FixtureRequest, thread: u64, resumed: u64) -> Self {
        Self {
            request,
            thread,
            resumed,
            next_sequence: 1,
            loading: false,
            returned: false,
            registrations_ended: false,
            ended: false,
            owner: None,
            last_field_ordinal: 0,
            owner_ids: BTreeMap::new(),
            next_owner_id: 1,
            definitions: BTreeMap::new(),
            field_authorities: BTreeMap::new(),
            occurrences: BTreeMap::new(),
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

    fn push_gap(&mut self, kind: GapKind, subject: Option<String>, detail: &str) {
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
        self.stream_intact = false;
        self.field_issues
            .extend(0..self.request.field_questions.len() as u64);
        if self.diagnostics_requested {
            self.diagnostic_issue = true;
        }
        self.push_gap(
            GapKind::IncompleteObservation,
            Some(self.request.file().into()),
            detail,
        );
    }

    fn field_gap(&mut self, question: u64, detail: &str) {
        self.field_issues.insert(question);
        let subject = self
            .request
            .field_questions
            .get(question as usize)
            .map(|question| question.field.clone())
            .or_else(|| Some(self.request.file().into()));
        self.push_gap(GapKind::IncompleteObservation, subject, detail);
    }

    fn diagnostic_gap(&mut self, detail: &str) {
        self.diagnostic_issue = true;
        self.push_gap(
            GapKind::IncompleteObservation,
            Some(self.request.file().into()),
            detail,
        );
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

    fn accept(&mut self, record: &WorkerRecord, event: &FixtureEvent) {
        if let FixtureEvent::Unavailable { .. } = event {
            self.window_gap("A requested fixture observation was unavailable");
            return;
        }
        if record.thread != Some(self.thread) || record.seq <= self.resumed || self.ended {
            self.window_gap("A fixture event has no matching activation, thread or open window");
            return;
        }
        match event {
            FixtureEvent::RegistrationEntry { ordinal } => self.accept_registration_entry(*ordinal),
            FixtureEvent::RegistrationEnd { count } => self.accept_registration_end(*count),
            FixtureEvent::LoadStart { file } => self.accept_load_start(file),
            FixtureEvent::FieldRead {
                file,
                line,
                field,
                owner,
                ordinal,
            } => self.accept_field_read(file, *line, field, owner, *ordinal),
            FixtureEvent::Definition {
                file,
                line,
                definition,
                owner,
            } => self.accept_definition(file, *line, definition, owner),
            FixtureEvent::FieldAuthority {
                question,
                reader_id,
                reader_kind,
                storage_supported,
                unavailable,
            } => self.accept_field_authority(
                *question,
                reader_id,
                reader_kind,
                *storage_supported,
                unavailable,
            ),
            FixtureEvent::FieldStorage { .. } => self.accept_field_storage(event),
            FixtureEvent::Diagnostic { .. } => self.accept_diagnostic(event),
            FixtureEvent::FieldTerminal { .. } => self.accept_field_terminal(event),
            FixtureEvent::DiagnosticsTerminal { count } => self.accept_diagnostics_terminal(*count),
            FixtureEvent::DiagnosticsUnavailable { reason } => {
                self.accept_diagnostics_unavailable(reason)
            }
            FixtureEvent::LoadReturned { file, field_count } => {
                self.accept_load_returned(file, *field_count)
            }
            FixtureEvent::End {
                registrations,
                field_reads,
                field_outcomes,
                diagnostics,
                producer_last_sequence,
            } => self.accept_end(
                record.seq,
                *registrations,
                *field_reads,
                *field_outcomes,
                *diagnostics,
                *producer_last_sequence,
            ),
            FixtureEvent::Unavailable { .. } => unreachable!(),
        }
    }

    fn accept_registration_entry(&mut self, ordinal: u64) {
        let expected = self
            .value
            .registration_entries
            .last()
            .map_or(1, |entry| entry.ordinal + 1);
        if !self.request.requests(Kind::RegistrationEntries)
            || self.loading
            || self.registrations_ended
            || !(1..=3).contains(&ordinal)
            || ordinal < expected
        {
            self.window_gap("Invalid registration entry order");
            return;
        }
        if ordinal != expected {
            self.window_gap("A registration entry is missing");
        }
        self.value.registration_entries.push(RegistrationEntry {
            ordinal,
            stage: ProcessingStage::RegistrationEntry,
        });
    }

    fn accept_registration_end(&mut self, count: u64) {
        if !self.request.requests(Kind::RegistrationEntries)
            || self.registrations_ended
            || self.loading
            || count != 3
            || self.value.registration_entries.len() != 3
        {
            self.window_gap("The registration terminal disagrees with its entries");
        }
        self.registrations_ended = true;
    }

    fn accept_load_start(&mut self, file: &str) {
        if self.loading || file != self.request.file() {
            self.window_gap("The fixture loader entry is repeated or names another file");
            return;
        }
        if self.request.requests(Kind::RegistrationEntries) && !self.registrations_ended {
            self.window_gap("Registration did not finish before the fixture load");
        }
        self.loading = true;
    }

    fn accept_field_read(&mut self, file: &str, line: u64, field: &str, owner: &str, ordinal: u64) {
        let valid = self.request.requests(Kind::CategoryFieldReads)
            && self.loading
            && !self.returned
            && file == self.request.file()
            && line > 0
            && line <= self.request.files[file].lines().count() as u64
            && matches!(field, "tree_template" | "traditions")
            && super::registry_items::pointer(owner)
            && self.owner.as_ref().is_none_or(|expected| expected == owner)
            && ordinal > self.last_field_ordinal
            && ordinal <= 2;
        if !valid {
            self.window_gap("A field read lacks a matching file, owner, line, order or loader");
            return;
        }
        if ordinal != self.last_field_ordinal + 1 {
            self.window_gap("A field read is missing");
        }
        self.last_field_ordinal = ordinal;
        self.owner = Some(owner.into());
        let public_owner = self.public_owner(owner);
        self.value.field_reads.push(FieldRead {
            file: file.into(),
            line,
            field: field.into(),
            owner: public_owner,
            stage: ProcessingStage::FieldReadEntry,
        });
    }

    fn accept_definition(&mut self, file: &str, line: u64, definition: &str, owner: &str) {
        let valid = self.loading
            && !self.returned
            && file == self.request.file()
            && line > 0
            && line <= self.request.files[file].lines().count() as u64
            && super::registry_items::pointer(owner)
            && !self.definitions.contains_key(definition);
        if !valid {
            self.window_gap("A definition lacks a matching file, owner, line or loader");
            return;
        }
        let public_owner = self.public_owner(owner);
        self.definitions.insert(
            definition.into(),
            DefinitionState {
                native_owner: owner.into(),
                public_owner,
                line,
            },
        );
    }

    fn accept_field_authority(
        &mut self,
        question: u64,
        reader_id: &Option<String>,
        reader_kind: &str,
        storage_supported: bool,
        unavailable: &Option<String>,
    ) {
        let Some(_) = self.request.field_questions.get(question as usize) else {
            self.window_gap("A field authority names an unknown question");
            return;
        };
        if !self.loading || self.returned || self.field_authorities.contains_key(&question) {
            self.field_gap(question, "A field authority is late or repeated");
            return;
        }
        let reader = Reader {
            id: reader_id.clone().map(ReaderId),
            kind: parse_reader_kind(reader_kind),
        };
        let coherent = if storage_supported {
            reader.kind == ReaderKind::String && reader.id.is_some() && unavailable.is_none()
        } else {
            unavailable.is_some()
        };
        if !coherent {
            self.field_gap(question, "The field authority is internally inconsistent");
        }
        self.field_authorities.insert(
            question,
            FieldAuthorityState {
                reader,
                coherent,
                storage_supported: storage_supported && coherent,
                unavailable: unavailable.clone(),
            },
        );
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
        let expected = self
            .occurrences
            .get(question)
            .map_or(1, |items| items.len() as u64 + 1);
        let valid = self.loading
            && !self.returned
            && file == self.request.file()
            && definition == &asked.definition
            && field == &asked.field
            && self
                .definitions
                .get(definition)
                .is_some_and(|known| &known.native_owner == owner)
            && self
                .field_authorities
                .get(question)
                .is_some_and(|authority| authority.storage_supported)
            && !self.field_terminals.contains_key(question)
            && *line > 0
            && *line <= self.request.files[file].lines().count() as u64
            && *occurrence == expected;
        if !valid {
            self.field_gap(
                *question,
                "A stored value lacks a matching question, source, owner, order or open field window",
            );
            return;
        }
        self.occurrences
            .entry(*question)
            .or_default()
            .push(StoredStringOccurrence {
                line: *line,
                occurrence: *occurrence,
                value: value.clone(),
            });
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
        } = event
        else {
            unreachable!("diagnostic handler received another event")
        };
        if !self.diagnostics_requested || !self.loading || self.returned {
            self.diagnostic_gap("A parser diagnostic is outside its requested window");
        }
        if self.diagnostic_terminal.is_some() {
            self.diagnostic_gap("A parser diagnostic arrived after its terminal");
        }
        if !matches!(
            stage.as_str(),
            "reader-malformed-report" | "reader-unexpected-report"
        ) {
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
        });
    }

    fn accept_field_terminal(&mut self, event: &FixtureEvent) {
        let FixtureEvent::FieldTerminal {
            question,
            owner,
            definition_line,
            reader_id,
            reader_kind,
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
        if !self.loading || self.returned || self.field_terminals.contains_key(question) {
            self.field_gap(*question, "A field terminal is late or repeated");
            return;
        }
        let authority_matches = self
            .field_authorities
            .get(question)
            .is_some_and(|authority| {
                authority.reader.kind == parse_reader_kind(reader_kind)
                    && authority.reader.id.as_ref().map(|id| &id.0) == reader_id.as_ref()
            });
        if !authority_matches {
            self.field_gap(
                *question,
                "A field terminal disagrees with reader authority",
            );
        }
        let storage_supported = self
            .field_authorities
            .get(question)
            .is_some_and(|authority| authority.storage_supported);
        let owner_joined = owner.as_ref().is_some_and(|pointer| {
            self.definitions
                .get(&asked.definition)
                .is_some_and(|known| {
                    &known.native_owner == pointer && Some(known.line) == *definition_line
                })
        });
        let shape_valid = if storage_supported {
            owner_joined && final_value.is_some() && unavailable.is_none()
        } else {
            final_value.is_none() && unavailable.is_some()
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
                final_value: (authority_matches && owner_joined)
                    .then(|| final_value.clone())
                    .flatten(),
                unavailable: unavailable.clone(),
            },
        );
    }

    fn accept_diagnostics_terminal(&mut self, count: u64) {
        if !self.diagnostics_requested
            || !self.loading
            || self.returned
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
            || !self.loading
            || self.returned
            || self.diagnostic_terminal.is_some()
            || !self.raw_diagnostics.is_empty()
        {
            self.diagnostic_gap("The unavailable diagnostic terminal is inconsistent");
        }
        if self.diagnostic_terminal.is_none() {
            self.diagnostic_terminal = Some(DiagnosticTerminalState::Unavailable(reason.into()));
        }
    }

    fn accept_load_returned(&mut self, file: &str, field_count: u64) {
        if !self.loading || self.returned || file != self.request.file() {
            self.window_gap("The fixture return has no unique matching loader entry");
            return;
        }
        if field_count != self.value.field_reads.len() as u64 {
            self.window_gap("The loader return disagrees with the field reads");
        }
        self.returned = true;
    }

    fn accept_end(
        &mut self,
        sequence: u64,
        registrations: u64,
        field_reads: u64,
        field_outcomes: u64,
        diagnostics: u64,
        producer_last_sequence: u64,
    ) {
        if !self.returned
            || producer_last_sequence != sequence
            || registrations != self.value.registration_entries.len() as u64
            || field_reads != self.value.field_reads.len() as u64
            || (self.request.requests(Kind::RegistrationEntries) && !self.registrations_ended)
        {
            self.window_gap("The fixture terminal disagrees with its window");
        }
        if field_outcomes != self.field_terminals.len() as u64 {
            for index in 0..self.request.field_questions.len() as u64 {
                self.field_gap(index, "The fixture terminal disagrees with field terminals");
            }
        }
        if diagnostics != self.raw_diagnostics.len() as u64
            || (self.diagnostics_requested != self.diagnostic_terminal.is_some())
        {
            self.diagnostic_gap("The fixture terminal disagrees with the diagnostic window");
        }
        self.ended = true;
    }

    fn finish_diagnostics(&mut self) {
        for raw in std::mem::take(&mut self.raw_diagnostics) {
            let source = match (&raw.file, raw.line) {
                (Some(file), Some(line))
                    if file == self.request.file()
                        && line > 0
                        && line <= self.request.files[file].lines().count() as u64 =>
                {
                    Some((file.clone(), line))
                }
                _ => None,
            };
            let linked_question = match (&raw.definition, &raw.field, raw.occurrence) {
                (None, None, None) => Some(None),
                (Some(definition), Some(field), Some(occurrence)) => self
                    .request
                    .field_questions
                    .iter()
                    .enumerate()
                    .find(|(index, question)| {
                        question.definition == *definition
                            && question.field == *field
                            && self.occurrences.get(&(*index as u64)).is_some_and(|items| {
                                items.iter().any(|item| {
                                    item.occurrence == occurrence && raw.line == Some(item.line)
                                })
                            })
                    })
                    .map(|(index, _)| Some(index as u64)),
                _ => None,
            };
            let join = match (source, linked_question) {
                (Some((file, line)), Some(question)) => {
                    if let Some(question) = question {
                        let asked = &self.request.field_questions[question as usize];
                        if asked.diagnostics {
                            self.diagnostic_indices
                                .entry(question)
                                .or_default()
                                .push(self.value.diagnostics.len());
                        }
                    }
                    DiagnosticJoin::Source {
                        file,
                        line,
                        definition: raw.definition.clone(),
                        field: raw.field.clone(),
                        occurrence: raw.occurrence,
                    }
                }
                (Some((file, line)), None) => {
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
                        Some(self.request.registry().into()),
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
                        window: DiagnosticWindow::FixtureFileLoad,
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

    fn finish_fields(&mut self) {
        for index in 0..self.request.field_questions.len() as u64 {
            let question = self.request.field_questions[index as usize].clone();
            let authority = self.field_authorities.remove(&index);
            let terminal = self.field_terminals.remove(&index);
            let definition = self.definitions.get(&question.definition).cloned();
            let occurrences = self.occurrences.remove(&index).unwrap_or_default();
            let reader = authority.as_ref().map_or(
                Reader {
                    id: None,
                    kind: ReaderKind::Unknown,
                },
                |authority| authority.reader.clone(),
            );
            let storage = if authority
                .as_ref()
                .is_some_and(|authority| authority.storage_supported)
            {
                let final_value = terminal
                    .as_ref()
                    .and_then(|terminal| terminal.final_value.clone());
                if !occurrences.is_empty() || final_value.is_some() {
                    let complete = self.ended
                        && self.returned
                        && self.stream_intact
                        && !self.field_issues.contains(&index)
                        && terminal.is_some()
                        && final_value.is_some()
                        && definition.is_some();
                    FixtureStorage::String {
                        occurrences,
                        final_value,
                        completeness: if complete {
                            Completeness::Complete
                        } else {
                            Completeness::Partial
                        },
                    }
                } else {
                    let reason = terminal
                        .as_ref()
                        .and_then(|terminal| terminal.unavailable.clone())
                        .unwrap_or_else(|| "No String storage observation was established".into());
                    self.field_gap(index, &reason);
                    FixtureStorage::Unavailable(reason)
                }
            } else {
                let reason = authority
                    .as_ref()
                    .and_then(|authority| authority.unavailable.clone())
                    .or_else(|| {
                        terminal
                            .as_ref()
                            .and_then(|terminal| terminal.unavailable.clone())
                    })
                    .unwrap_or_else(|| "The field reader authority was not established".into());
                let kind = match authority.as_ref() {
                    None => GapKind::IncompleteObservation,
                    Some(authority) if !authority.coherent => GapKind::IncompleteObservation,
                    Some(authority) if authority.reader.kind == ReaderKind::Unknown => {
                        GapKind::UnresolvedReader
                    }
                    Some(_) => GapKind::OutsideMethod,
                };
                self.push_gap(kind, Some(question.field.clone()), &reason);
                FixtureStorage::Unavailable(reason)
            };
            let runtime = if question.runtime {
                let reason = "Runtime is outside the initial file-load method";
                self.push_gap(GapKind::OutsideMethod, Some(question.field.clone()), reason);
                FixtureRuntime::Unavailable(reason.into())
            } else {
                FixtureRuntime::NotRequested
            };
            self.value.field_outcomes.push(FixtureFieldOutcome {
                question,
                file: self.request.file().into(),
                owner: definition
                    .as_ref()
                    .map(|definition| definition.public_owner.clone()),
                definition_line: definition.as_ref().map(|definition| definition.line),
                reader,
                storage,
                diagnostics: self.diagnostic_indices.remove(&index).unwrap_or_default(),
                runtime,
            });
        }
    }

    fn finish(mut self, build: BuildId) -> Answer<FixtureObservation> {
        self.finish_diagnostics();
        self.finish_fields();
        Answer {
            completeness: if self.gaps.is_empty() {
                Completeness::Complete
            } else {
                Completeness::Partial
            },
            value: self.value,
            gaps: self.gaps,
            source: Source::new(build, "observe-fixture/v1", Basis::LiveObservation),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    const FILE: &str = "common/tradition_categories/atlas.txt";
    fn request() -> FixtureRequest {
        FixtureRequest::new(
            FILE,
            "atlas = {\n tree_template = \"template\"\n traditions = {}\n}\n",
        )
    }
    fn events() -> Vec<Value> {
        let hook = json!({"enabled":true,"locations":1,"resolved":1,"hits":0});
        let fixture = |event: Value| json!({"kind":"fixture", "event":event});
        vec![
            json!({"kind":"launch-stopped", "error":"success", "pid":42, "triple":"arm64-macos", "frames":[{"function":"_dyld_start"}]}),
            json!({"kind":"hooks-active-before-resume", "hooks":{"fixture:registration":hook, "fixture:load":hook, "fixture:field":hook}}),
            json!({"kind":"resume","error":"success"}),
            fixture(json!({"kind":"registration-entry","ordinal":1})),
            fixture(json!({"kind":"registration-entry","ordinal":2})),
            fixture(json!({"kind":"registration-entry","ordinal":3})),
            fixture(json!({"kind":"registration-end","count":3})),
            fixture(json!({"kind":"load-start","file":FILE})),
            fixture(json!({"kind":"field-read","file":FILE,"line":2,"field":"tree_template","owner":"0x2000","ordinal":1})),
            fixture(json!({"kind":"field-read","file":FILE,"line":3,"field":"traditions","owner":"0x2000","ordinal":2})),
            fixture(json!({"kind":"load-returned","file":FILE,"field_count":2})),
            fixture(json!({"kind":"end","registrations":3,"field_reads":2,"producer_last_sequence":12})),
        ].into_iter().enumerate().map(|(index, mut event)| {
            event["seq"] = json!(index + 1); event["thread"] = json!(7); event["run"] = json!("attempt"); event
        }).collect()
    }
    fn records(events: Vec<Value>) -> Vec<WorkerRecord> {
        events
            .into_iter()
            .map(|event| serde_json::from_value(event).unwrap())
            .collect()
    }

    fn field_request(runtime: bool) -> FixtureRequest {
        let mut question =
            crate::FixtureFieldQuestion::new("common/traditions", "sample", "unlocks_agenda");
        question.runtime = runtime;
        FixtureRequest::field_outcomes(
            "common/traditions/sample.txt",
            "sample = {\n unlocks_agenda = \"one\"\n unlocks_agenda = \"two\"\n}\n",
            [question],
        )
    }

    fn field_request_without_diagnostics() -> FixtureRequest {
        let mut request = field_request(false);
        request.field_questions[0].diagnostics = false;
        request
    }

    fn field_events() -> Vec<Value> {
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
                "fixture:unexpected":hook
            }}),
            json!({"kind":"resume","error":"success"}),
            fixture(json!({"kind":"load-start","file":"common/traditions/sample.txt"})),
            fixture(
                json!({"kind":"field-authority","question":0,"reader_id":"325efaa17499c32d","reader_kind":"String","storage_supported":true,"unavailable":null}),
            ),
            fixture(
                json!({"kind":"definition","file":"common/traditions/sample.txt","line":1,"definition":"sample","owner":"0x2000"}),
            ),
            fixture(
                json!({"kind":"field-storage","question":0,"file":"common/traditions/sample.txt","line":2,"definition":"sample","field":"unlocks_agenda","owner":"0x2000","occurrence":1,"value":"one"}),
            ),
            fixture(
                json!({"kind":"diagnostic","text":"malformed value","stage":"reader-malformed-report","file":"common/traditions/sample.txt","line":2,"definition":"sample","field":"unlocks_agenda","occurrence":1}),
            ),
            fixture(
                json!({"kind":"field-storage","question":0,"file":"common/traditions/sample.txt","line":3,"definition":"sample","field":"unlocks_agenda","owner":"0x2000","occurrence":2,"value":"two"}),
            ),
            fixture(
                json!({"kind":"field-terminal","question":0,"owner":"0x2000","definition_line":1,"reader_id":"325efaa17499c32d","reader_kind":"String","final_value":"two","unavailable":null}),
            ),
            fixture(json!({"kind":"diagnostics-terminal","count":1})),
            fixture(
                json!({"kind":"load-returned","file":"common/traditions/sample.txt","field_count":0}),
            ),
            Value::Null,
        ];
        let last = events.len();
        events[last - 1] = fixture(json!({"kind":"end","registrations":0,"field_reads":0,
            "field_outcomes":1,"diagnostics":1,"producer_last_sequence":last}));
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

    fn renumber(events: &mut [Value]) {
        let last = events.len();
        for (index, event) in events.iter_mut().enumerate() {
            event["seq"] = json!(index + 1);
            if event.pointer("/event/kind").and_then(Value::as_str) == Some("end") {
                event["event"]["producer_last_sequence"] = json!(last);
            }
        }
    }

    fn unavailable_field_events(reader_kind: &str) -> Vec<Value> {
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
                    event["event"]["reader_id"] = Value::Null;
                    event["event"]["reader_kind"] = json!(reader_kind);
                    event["event"]["final_value"] = Value::Null;
                    event["event"]["unavailable"] = json!("Storage decoder unavailable");
                }
                Some("field-authority") => {
                    event["event"]["reader_id"] = Value::Null;
                    event["event"]["reader_kind"] = json!(reader_kind);
                    event["event"]["storage_supported"] = json!(false);
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

    fn category_outcome_events() -> Vec<Value> {
        let hook = json!({"enabled":true,"locations":1,"resolved":1,"hits":0});
        let fixture = |event: Value| json!({"kind":"fixture", "event":event});
        let mut events = vec![
            json!({"kind":"launch-stopped", "error":"success", "pid":42, "triple":"arm64-macos", "frames":[{"function":"_dyld_start"}]}),
            json!({"kind":"hooks-active-before-resume", "hooks":{"fixture:load":hook}}),
            json!({"kind":"resume","error":"success"}),
            fixture(json!({"kind":"load-start","file":"common/tradition_categories/sample.txt"})),
            fixture(
                json!({"kind":"field-authority","question":0,"reader_id":"325efaa17499c32d","reader_kind":"String","storage_supported":false,"unavailable":"No exact-build storage binding for this field"}),
            ),
            fixture(
                json!({"kind":"field-terminal","question":0,"owner":null,"definition_line":null,"reader_id":"325efaa17499c32d","reader_kind":"String","final_value":null,"unavailable":"No exact-build storage binding for this field"}),
            ),
            fixture(
                json!({"kind":"diagnostics-unavailable","reason":"Parser diagnostics are outside this registry binding"}),
            ),
            fixture(
                json!({"kind":"load-returned","file":"common/tradition_categories/sample.txt","field_count":0}),
            ),
            Value::Null,
        ];
        let last = events.len();
        events[last - 1] = fixture(json!({"kind":"end","registrations":0,"field_reads":0,
            "field_outcomes":1,"diagnostics":0,"producer_last_sequence":last}));
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
        let mut events = field_events();
        events[4] = json!({"kind":"fixture","event":{"kind":"field-authority","question":0,
            "reader_id":"325efaa17499c32d","reader_kind":"String","storage_supported":true,
            "unavailable":null},"thread":7,"run":"attempt"});
        events[6] = json!({"kind":"fixture","event":{"kind":"field-storage","question":0,
            "file":"common/traditions/sample.txt","line":2,"definition":"sample",
            "field":"custom_tooltip","owner":"0x2000","occurrence":1,"value":"tip"},
            "thread":7,"run":"attempt"});
        events.remove(7);
        events[7] = json!({"kind":"fixture","event":{"kind":"field-storage","question":1,
            "file":"common/traditions/sample.txt","line":3,"definition":"sample",
            "field":"unlocks_agenda","owner":"0x2000","occurrence":1,"value":"agenda"},
            "thread":7,"run":"attempt"});
        events[8] = json!({"kind":"fixture","event":{"kind":"field-terminal","question":0,
            "owner":"0x2000","definition_line":1,"reader_id":"325efaa17499c32d",
            "reader_kind":"String","final_value":"tip","unavailable":null},
            "thread":7,"run":"attempt"});
        events.insert(
            5,
            json!({"kind":"fixture","event":{"kind":"field-authority","question":1,
            "reader_id":"325efaa17499c32d","reader_kind":"String","storage_supported":true,
            "unavailable":null},"thread":7,"run":"attempt"}),
        );
        events.insert(
            10,
            json!({"kind":"fixture","event":{"kind":"field-terminal","question":1,
            "owner":"0x2000","definition_line":1,"reader_id":"325efaa17499c32d",
            "reader_kind":"String","final_value":"agenda","unavailable":null},
            "thread":7,"run":"attempt"}),
        );
        for event in &mut events {
            if event.pointer("/event/kind").and_then(Value::as_str) == Some("diagnostics-terminal")
            {
                event["event"]["count"] = json!(0);
            }
            if event.pointer("/event/kind").and_then(Value::as_str) == Some("end") {
                event["event"]["field_outcomes"] = json!(2);
                event["event"]["diagnostics"] = json!(0);
            }
        }
        renumber(&mut events);
        events
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
                    event["event"]["definition_line"] = json!(4);
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
            &request(),
            &records(events),
            &owner(),
            BuildId("build".into()),
        )
        .unwrap()
    }

    #[test]
    fn complete_window_keeps_source_lines_and_replaces_addresses_with_owner_identity() {
        let answer = answer(events());
        assert_eq!(answer.completeness, Completeness::Complete);
        assert!(answer.gaps.is_empty());
        assert_eq!(
            answer
                .value
                .registration_entries
                .iter()
                .map(|entry| entry.ordinal)
                .collect::<Vec<_>>(),
            [1, 2, 3]
        );
        let reads = &answer.value.field_reads;
        assert_eq!(
            reads
                .iter()
                .map(|read| (read.file.as_str(), read.line, read.field.as_str()))
                .collect::<Vec<_>>(),
            [(FILE, 2, "tree_template"), (FILE, 3, "traditions")]
        );
        assert_eq!(reads[0].owner, reads[1].owner);
        assert_eq!(reads[0].stage, ProcessingStage::FieldReadEntry);
        assert!(!serde_json::to_string(&answer).unwrap().contains("0x2000"));
        let mut relocated = events();
        for index in [8, 9] {
            relocated[index]["event"]["owner"] = json!("0x8000");
        }
        assert_eq!(answer, self::answer(relocated));
    }

    #[test]
    fn every_activation_fact_is_required_before_any_complete_answer() {
        for (index, pointer, value) in [
            (0, "/pid", json!(99)),
            (0, "/error", json!("failed")),
            (0, "/triple", json!("x86_64-macos")),
            (0, "/thread", json!(0)),
            (0, "/frames/0/function", json!("main")),
            (1, "/hooks/fixture:field/enabled", json!(false)),
            (1, "/hooks/fixture:load/locations", json!(0)),
            (1, "/hooks/fixture:registration/resolved", json!(2)),
            (1, "/hooks/fixture:field/hits", json!(1)),
            (2, "/error", json!("failed")),
            (2, "/seq", json!(1)),
        ] {
            let mut changed = events();
            *changed[index].pointer_mut(pointer).unwrap() = value;
            assert!(
                matches!(
                    reduce(&request(), &records(changed), &owner(), BuildId("b".into())),
                    Err(Error::Observation { .. })
                ),
                "{pointer}"
            );
        }
        for index in [0, 1, 2] {
            let mut changed = events();
            changed.remove(index);
            assert!(reduce(&request(), &records(changed), &owner(), BuildId("b".into())).is_err());
        }
        assert!(reduce(&request(), &records(events()), &[], BuildId("b".into())).is_err());
    }

    #[test]
    fn bad_joins_counts_order_and_terminals_never_complete() {
        for (index, pointer, value) in [
            (3, "/thread", json!(8)),
            (3, "/event/ordinal", json!(0)),
            (6, "/event/count", json!(2)),
            (7, "/event/file", json!("foreign.txt")),
            (8, "/event/owner", json!("0x0")),
            (9, "/event/owner", json!("0x3000")),
            (8, "/event/file", json!("foreign.txt")),
            (8, "/event/line", json!(0)),
            (8, "/event/line", json!(100)),
            (8, "/event/field", json!("unbound")),
            (9, "/event/ordinal", json!(1)),
            (9, "/thread", json!(8)),
            (10, "/event/file", json!("foreign.txt")),
            (10, "/event/field_count", json!(1)),
            (11, "/event/registrations", json!(2)),
            (11, "/event/field_reads", json!(1)),
            (11, "/event/producer_last_sequence", json!(11)),
        ] {
            let mut changed = events();
            *changed[index].pointer_mut(pointer).unwrap() = value;
            let answer = answer(changed);
            assert_eq!(
                answer.completeness,
                Completeness::Partial,
                "{index} {pointer}"
            );
            assert!(!answer.gaps.is_empty());
        }
        for index in 3..12 {
            let mut changed = events();
            changed.remove(index);
            assert_eq!(
                answer(changed).completeness,
                Completeness::Partial,
                "missing {index}"
            );
        }
        for (left, right) in [(3, 7), (7, 8), (9, 10), (10, 11)] {
            let mut changed = events();
            changed.swap(left, right);
            assert_eq!(answer(changed).completeness, Completeness::Partial);
        }
    }

    #[test]
    fn record_loss_and_missing_terminal_keep_the_established_reads() {
        let mut dropped = events();
        dropped.remove(8);
        let partial = answer(dropped);
        assert_eq!(partial.completeness, Completeness::Partial);
        assert_eq!(partial.value.field_reads.len(), 1);
        assert_eq!(partial.value.field_reads[0].field, "traditions");
        let mut no_terminal = events();
        no_terminal.pop();
        assert_eq!(answer(no_terminal).value.field_reads.len(), 2);
        let mut lost = owner();
        lost.push(OwnerEvent::WorkerExited { returncode: -9 });
        assert_eq!(
            reduce(&request(), &records(events()), &lost, BuildId("b".into()))
                .unwrap()
                .completeness,
            Completeness::Partial
        );
    }

    #[test]
    fn later_registry_record_loss_does_not_revoke_a_completed_fixture_window() {
        let expected = answer(events());
        let mut later_loss = events();
        later_loss.push(json!({"kind":"registry-entry", "run":"attempt", "seq":14,
            "thread":7, "name":"tradition_categories", "owner":"0x4000",
            "index":1, "object":"0x8000", "key":"other"}));
        later_loss.push(json!({"kind":"callback-error", "run":"attempt", "seq":15,
            "error":"a later observation failed"}));
        assert_eq!(answer(later_loss), expected);
    }

    #[test]
    fn damaged_tail_invalidates_fixture_terminal_even_after_a_complete_window() {
        let mut raw = events()
            .into_iter()
            .map(|event| format!("{event}\n"))
            .collect::<String>();
        raw.push('{');
        let (records, damage) = event_stream::read_worker_stream(raw.as_bytes(), "attempt");
        assert!(damage.is_some());
        let answer = reduce(&request(), &records, &owner(), BuildId("b".into())).unwrap();
        assert_eq!(answer.completeness, Completeness::Partial);
        assert_eq!(answer.value.field_reads.len(), 2);
    }

    #[test]
    fn field_storage_diagnostics_and_runtime_are_independent() {
        let complete = reduce(
            &field_request(false),
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
        assert_eq!(outcome.runtime, FixtureRuntime::NotRequested);
        assert!(matches!(
            &outcome.storage,
            FixtureStorage::String { occurrences, final_value, completeness }
                if occurrences.iter().map(|item| item.value.as_str()).collect::<Vec<_>>() == ["one", "two"]
                    && final_value.as_deref() == Some("two")
                    && *completeness == Completeness::Complete
        ));

        let runtime = reduce(
            &field_request(true),
            &records(field_events()),
            &owner(),
            BuildId("b".into()),
        )
        .unwrap();
        assert_eq!(runtime.completeness, Completeness::Partial);
        assert!(matches!(
            runtime.value.field_outcomes[0].runtime,
            FixtureRuntime::Unavailable(_)
        ));
        assert!(
            runtime
                .gaps
                .iter()
                .any(|gap| gap.kind == GapKind::OutsideMethod)
        );

        let mut missing_diagnostic_terminal = field_events();
        missing_diagnostic_terminal.remove(10);
        let partial = reduce(
            &field_request(false),
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
        unjoined_diagnostic[7]["event"]["file"] = json!("foreign.txt");
        unjoined_diagnostic[7]["event"]["line"] = json!(0);
        let partial = reduce(
            &field_request(false),
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
        inconsistent_subjoin[7]["event"]["line"] = json!(3);
        let partial = reduce(
            &field_request(false),
            &records(inconsistent_subjoin),
            &owner(),
            BuildId("b".into()),
        )
        .unwrap();
        assert_eq!(partial.completeness, Completeness::Partial);
        assert_eq!(
            partial.value.diagnostics[0].join,
            DiagnosticJoin::Source {
                file: "common/traditions/sample.txt".into(),
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
        for (reader_kind, gap_kind, expected_kind) in [
            ("Unknown", GapKind::UnresolvedReader, ReaderKind::Unknown),
            ("Block", GapKind::OutsideMethod, ReaderKind::Block),
        ] {
            let answer = reduce(
                &field_request(false),
                &records(unavailable_field_events(reader_kind)),
                &owner(),
                BuildId("b".into()),
            )
            .unwrap();
            assert_eq!(answer.completeness, Completeness::Partial);
            assert_eq!(answer.value.field_outcomes[0].reader.kind, expected_kind);
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
        let answer = reduce(&request, &records(events), &owner(), BuildId("b".into())).unwrap();
        assert_eq!(answer.completeness, Completeness::Partial);
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

        let unsupported = reduce(
            &category_outcome_request(),
            &records(category_outcome_events()),
            &owner(),
            BuildId("b".into()),
        )
        .unwrap();
        assert_eq!(unsupported.completeness, Completeness::Partial);
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
        wrong_count[10]["event"]["count"] = json!(2);
        cases.push(wrong_count);
        let mut duplicate_terminal = field_events();
        duplicate_terminal.insert(11, duplicate_terminal[10].clone());
        renumber(&mut duplicate_terminal);
        cases.push(duplicate_terminal);
        let mut premature_terminal = field_events();
        premature_terminal.swap(7, 10);
        renumber(&mut premature_terminal);
        cases.push(premature_terminal);
        let mut missing_return = field_events();
        missing_return.remove(11);
        renumber(&mut missing_return);
        cases.push(missing_return);
        let mut missing_end = field_events();
        missing_end.pop();
        cases.push(missing_end);
        let mut diagnostic_after_terminal = field_events();
        diagnostic_after_terminal.insert(11, diagnostic_after_terminal[7].clone());
        diagnostic_after_terminal.last_mut().unwrap()["event"]["diagnostics"] = json!(2);
        renumber(&mut diagnostic_after_terminal);
        cases.push(diagnostic_after_terminal);
        for events in cases {
            let answer = reduce(
                &field_request(false),
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
        let answer = reduce(
            &field_request(false),
            &records,
            &owner(),
            BuildId("b".into()),
        )
        .unwrap();
        assert!(matches!(
            answer.value.diagnostic_coverage,
            DiagnosticCoverage::Unavailable(_)
        ));
        assert!(matches!(
            answer.value.field_outcomes[0].storage,
            FixtureStorage::String {
                final_value: Some(_),
                completeness: Completeness::Partial,
                ..
            }
        ));
    }

    #[test]
    fn storage_authority_lifecycle_and_runtime_fail_independently() {
        let mut wrong_authority = field_events();
        wrong_authority[9]["event"]["reader_id"] = Value::Null;
        let answer = reduce(
            &field_request(false),
            &records(wrong_authority),
            &owner(),
            BuildId("b".into()),
        )
        .unwrap();
        assert!(matches!(
            answer.value.field_outcomes[0].storage,
            FixtureStorage::String {
                final_value: None,
                completeness: Completeness::Partial,
                ..
            }
        ));

        let mut missing_final = field_events();
        missing_final[9]["event"]["final_value"] = Value::Null;
        missing_final[9]["event"]["unavailable"] = json!("Final storage read failed");
        let answer = reduce(
            &field_request(false),
            &records(missing_final),
            &owner(),
            BuildId("b".into()),
        )
        .unwrap();
        assert!(matches!(
            answer.value.field_outcomes[0].storage,
            FixtureStorage::String {
                ref occurrences,
                final_value: None,
                completeness: Completeness::Partial,
            } if occurrences.len() == 2
        ));

        let mut storage_after_terminal = field_events();
        storage_after_terminal.insert(10, storage_after_terminal[8].clone());
        storage_after_terminal[10]["event"]["occurrence"] = json!(3);
        renumber(&mut storage_after_terminal);
        let answer = reduce(
            &field_request(false),
            &records(storage_after_terminal),
            &owner(),
            BuildId("b".into()),
        )
        .unwrap();
        assert_eq!(answer.completeness, Completeness::Partial);

        let mut missing_terminal = field_events();
        missing_terminal.remove(9);
        renumber(&mut missing_terminal);
        let answer = reduce(
            &field_request(true),
            &records(missing_terminal),
            &owner(),
            BuildId("b".into()),
        )
        .unwrap();
        assert!(matches!(
            answer.value.field_outcomes[0].runtime,
            FixtureRuntime::Unavailable(_)
        ));
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
            expected.value.field_outcomes[0].owner,
            expected.value.field_outcomes[1].owner
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
            different.value.field_outcomes[0].owner,
            different.value.field_outcomes[1].owner
        );
    }

    #[test]
    fn repair_regressions_require_independent_terminals_and_partial_preservation() {
        let mut entry_only = events();
        entry_only.insert(
            11,
            json!({"kind":"fixture", "event":{"kind":"diagnostics-terminal","count":0},
                "thread":7,"run":"attempt"}),
        );
        renumber(&mut entry_only);
        assert!(!matches!(
            answer(entry_only).value.diagnostic_coverage,
            DiagnosticCoverage::Complete { .. }
        ));

        let mut missing_field_terminal = field_events();
        missing_field_terminal.remove(9);
        renumber(&mut missing_field_terminal);
        let partial = reduce(
            &field_request(false),
            &records(missing_field_terminal),
            &owner(),
            BuildId("b".into()),
        )
        .unwrap();
        assert_eq!(partial.completeness, Completeness::Partial);
        assert!(matches!(
            &partial.value.field_outcomes[0].storage,
            FixtureStorage::String {
                occurrences,
                final_value: None,
                completeness: Completeness::Partial,
            } if occurrences.len() == 2
        ));

        let mut duplicate_field_terminal = field_events();
        duplicate_field_terminal.insert(10, duplicate_field_terminal[9].clone());
        renumber(&mut duplicate_field_terminal);
        let partial = reduce(
            &field_request(false),
            &records(duplicate_field_terminal),
            &owner(),
            BuildId("b".into()),
        )
        .unwrap();
        assert_eq!(partial.completeness, Completeness::Partial);
        assert!(matches!(
            &partial.value.field_outcomes[0].storage,
            FixtureStorage::String { occurrences, .. } if occurrences.len() == 2
        ));

        let mut missing_end = field_events();
        missing_end.pop();
        let partial = reduce(
            &field_request(false),
            &records(missing_end),
            &owner(),
            BuildId("b".into()),
        )
        .unwrap();
        assert_eq!(partial.completeness, Completeness::Partial);
        assert_eq!(partial.value.field_outcomes.len(), 1);

        let mut wrong_diagnostic_count = field_events();
        wrong_diagnostic_count[10]["event"]["count"] = json!(2);
        let partial = reduce(
            &field_request(false),
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
}
