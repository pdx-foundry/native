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
use std::collections::BTreeMap;

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
        file: String,
        line: u64,
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

fn pending_outcome(question: &crate::FixtureFieldQuestion, file: &str) -> FixtureFieldOutcome {
    FixtureFieldOutcome {
        question: question.clone(),
        file: file.into(),
        owner: None,
        definition_line: None,
        reader: Reader {
            id: None,
            kind: ReaderKind::Unknown,
        },
        storage: FixtureStorage::Unavailable("Storage terminal pending".into()),
        diagnostics: Vec::new(),
        runtime: FixtureRuntime::NotRequested,
    }
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
    if !request.field_questions.is_empty() && request.registry() == "common/traditions" {
        hooks.extend([
            "fixture:constructor",
            "fixture:reader",
            "fixture:member",
            "fixture:malformed",
        ]);
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
            window.gap("Observation records are missing or out of order");
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
                window.gap("The worker could not finish the fixture observation");
            }
            _ => {}
        }
    }
    if !window.ended {
        window.gap("The fixture terminal is missing");
    }
    if !window.returned {
        window.gap("The matching fixture loader return is missing");
    }
    if window.value.field_outcomes.len() != request.field_questions.len() {
        window.gap("One or more requested field terminals are missing");
        for question in &request.field_questions {
            if !window
                .value
                .field_outcomes
                .iter()
                .any(|outcome| outcome.question == *question)
            {
                let mut outcome = pending_outcome(question, request.file());
                outcome.storage =
                    FixtureStorage::Unavailable("The field observation terminal is missing".into());
                window.value.field_outcomes.push(outcome);
            }
        }
    }
    if !request.field_questions.is_empty() && !window.diagnostics_terminal {
        window.gap("The parser diagnostic terminal is missing");
        window.value.diagnostic_coverage =
            DiagnosticCoverage::Unavailable("The parser diagnostic window did not complete".into());
    }
    let stopping = owner_events
        .iter()
        .any(|event| matches!(event, OwnerEvent::WorkerStopRequested));
    if !stopping
        && owner_events.iter().any(
            |event| matches!(event, OwnerEvent::WorkerExited { returncode } if *returncode != 0),
        )
    {
        window.gap("The observation worker was lost");
    }
    Ok(Answer {
        value: window.value,
        completeness: if window.gaps.is_empty() {
            Completeness::Complete
        } else {
            Completeness::Partial
        },
        gaps: window.gaps,
        source: Source::new(build, "observe-fixture/v1", Basis::LiveObservation),
    })
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
    definitions: BTreeMap<String, (String, u64)>,
    occurrences: BTreeMap<u64, Vec<StoredStringOccurrence>>,
    field_terminals: BTreeMap<u64, FixtureFieldOutcome>,
    diagnostics_terminal: bool,
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
            definitions: BTreeMap::new(),
            occurrences: BTreeMap::new(),
            field_terminals: BTreeMap::new(),
            diagnostics_terminal: false,
            value: FixtureObservation::default(),
            gaps: Vec::new(),
        }
    }

    fn gap(&mut self, detail: &str) {
        if !self.gaps.iter().any(|gap| gap.detail == detail) {
            self.gaps.push(Gap {
                kind: GapKind::IncompleteObservation,
                subject: Some(self.request.file().into()),
                detail: detail.into(),
            });
        }
    }

    fn accept(&mut self, record: &WorkerRecord, event: &FixtureEvent) {
        if let FixtureEvent::Unavailable { .. } = event {
            self.gap("A requested fixture observation was unavailable");
            return;
        }
        if record.thread != Some(self.thread) || record.seq <= self.resumed || self.ended {
            self.gap("A fixture event has no matching activation, thread or open window");
            return;
        }
        match event {
            FixtureEvent::RegistrationEntry { ordinal } => {
                let expected = self
                    .value
                    .registration_entries
                    .last()
                    .map_or(1, |entry| entry.ordinal + 1);
                if !self.request.requests(Kind::RegistrationEntries)
                    || self.loading
                    || self.registrations_ended
                    || !(1..=3).contains(ordinal)
                    || *ordinal < expected
                {
                    self.gap("Invalid registration entry order");
                    return;
                }
                if *ordinal != expected {
                    self.gap("A registration entry is missing");
                }
                self.value.registration_entries.push(RegistrationEntry {
                    ordinal: *ordinal,
                    stage: ProcessingStage::RegistrationEntry,
                });
            }
            FixtureEvent::RegistrationEnd { count } => {
                if !self.request.requests(Kind::RegistrationEntries)
                    || self.registrations_ended
                    || self.loading
                    || *count != 3
                    || self.value.registration_entries.len() != 3
                {
                    self.gap("The registration terminal disagrees with its entries");
                }
                self.registrations_ended = true;
            }
            FixtureEvent::LoadStart { file } => {
                if self.loading || file != self.request.file() {
                    self.gap("The fixture loader entry is repeated or names another file");
                    return;
                }
                if self.request.requests(Kind::RegistrationEntries) && !self.registrations_ended {
                    self.gap("Registration did not finish before the fixture load");
                }
                self.loading = true;
            }
            FixtureEvent::FieldRead {
                file,
                line,
                field,
                owner,
                ordinal,
            } => {
                let valid = self.request.requests(Kind::CategoryFieldReads)
                    && self.loading
                    && !self.returned
                    && file == self.request.file()
                    && *line > 0
                    && *line <= self.request.files[file].lines().count() as u64
                    && matches!(field.as_str(), "tree_template" | "traditions")
                    && super::registry_items::pointer(owner)
                    && self.owner.as_ref().is_none_or(|expected| expected == owner)
                    && *ordinal > self.last_field_ordinal
                    && *ordinal <= 2;
                if !valid {
                    self.gap("A field read lacks a matching file, owner, line, order or loader");
                    return;
                }
                if *ordinal != self.last_field_ordinal + 1 {
                    self.gap("A field read is missing");
                }
                self.last_field_ordinal = *ordinal;
                self.owner = Some(owner.clone());
                self.value.field_reads.push(FieldRead {
                    file: file.clone(),
                    line: *line,
                    field: field.clone(),
                    owner: FixtureOwnerId(1),
                    stage: ProcessingStage::FieldReadEntry,
                });
            }
            FixtureEvent::Definition {
                file,
                line,
                definition,
                owner,
            } => {
                let valid = self.loading
                    && !self.returned
                    && file == self.request.file()
                    && *line > 0
                    && *line <= self.request.files[file].lines().count() as u64
                    && super::registry_items::pointer(owner)
                    && !self.definitions.contains_key(definition);
                if !valid {
                    self.gap("A definition lacks a matching file, owner, line or loader");
                } else {
                    self.definitions
                        .insert(definition.clone(), (owner.clone(), *line));
                }
            }
            FixtureEvent::FieldStorage {
                question,
                file,
                line,
                definition,
                field,
                owner,
                occurrence,
                value,
            } => {
                let Some(asked) = self.request.field_questions.get(*question as usize) else {
                    self.gap("A storage event names an unknown question");
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
                        .is_some_and(|(known, _)| known == owner)
                    && *line > 0
                    && *line <= self.request.files[file].lines().count() as u64
                    && *occurrence == expected;
                if !valid {
                    self.gap("A stored value lacks a matching question, source, owner or order");
                } else {
                    self.occurrences
                        .entry(*question)
                        .or_default()
                        .push(StoredStringOccurrence {
                            line: *line,
                            occurrence: *occurrence,
                            value: value.clone(),
                        });
                }
            }
            FixtureEvent::Diagnostic {
                text,
                stage,
                file,
                line,
                definition,
                field,
                occurrence,
            } => {
                let source_valid = file == self.request.file()
                    && *line > 0
                    && *line <= self.request.files[file].lines().count() as u64;
                let join = if source_valid {
                    DiagnosticJoin::Source {
                        file: file.clone(),
                        line: *line,
                        definition: definition.clone(),
                        field: field.clone(),
                        occurrence: *occurrence,
                    }
                } else {
                    self.gap("A parser diagnostic has no valid fixture source join");
                    DiagnosticJoin::Unavailable(
                        "The diagnostic source did not join to the fixture".into(),
                    )
                };
                let index = self.value.diagnostics.len();
                self.value.diagnostics.push(FixtureDiagnostic {
                    text: text.clone(),
                    stage: stage.clone(),
                    join,
                });
                if let (Some(definition), Some(field)) = (definition, field) {
                    for (question, asked) in self.request.field_questions.iter().enumerate() {
                        if asked.diagnostics
                            && &asked.definition == definition
                            && &asked.field == field
                        {
                            // The final outcome is built at its terminal; retain indices by a
                            // temporary sentinel entry in the map.
                            self.field_terminals
                                .entry(question as u64)
                                .or_insert_with(|| pending_outcome(asked, self.request.file()))
                                .diagnostics
                                .push(index);
                        }
                    }
                }
            }
            FixtureEvent::FieldTerminal {
                question,
                owner,
                definition_line,
                reader_id,
                reader_kind,
                final_value,
                unavailable,
            } => {
                let Some(asked) = self.request.field_questions.get(*question as usize) else {
                    self.gap("A field terminal names an unknown question");
                    return;
                };
                let diagnostics = self
                    .field_terminals
                    .remove(question)
                    .map(|outcome| outcome.diagnostics)
                    .unwrap_or_default();
                let owner_joined = owner.as_ref().is_some_and(|pointer| {
                    self.definitions
                        .get(&asked.definition)
                        .is_some_and(|(known, line)| {
                            known == pointer && Some(*line) == *definition_line
                        })
                });
                let storage = match (final_value, unavailable, owner_joined) {
                    (Some(final_value), None, true) => FixtureStorage::String {
                        occurrences: self.occurrences.remove(question).unwrap_or_default(),
                        final_value: final_value.clone(),
                    },
                    (None, Some(reason), _) => {
                        let kind = match reader_kind.as_str() {
                            "String" => GapKind::IncompleteObservation,
                            "Unknown" => GapKind::UnresolvedReader,
                            _ => GapKind::OutsideMethod,
                        };
                        self.gaps.push(Gap {
                            kind,
                            subject: Some(asked.field.clone()),
                            detail: reason.clone(),
                        });
                        FixtureStorage::Unavailable(reason.clone())
                    }
                    _ => {
                        self.gap("A field terminal lacks its constructor owner or source join");
                        FixtureStorage::Unavailable(
                            "The field terminal did not join to its constructor".into(),
                        )
                    }
                };
                let runtime = if asked.runtime {
                    self.gaps.push(Gap {
                        kind: GapKind::OutsideMethod,
                        subject: Some(asked.field.clone()),
                        detail: "Runtime is outside the initial file-load method".into(),
                    });
                    FixtureRuntime::Unavailable(
                        "Runtime is outside the initial file-load method".into(),
                    )
                } else {
                    FixtureRuntime::NotRequested
                };
                let reader = Reader {
                    id: reader_id.clone().map(ReaderId),
                    kind: parse_reader_kind(reader_kind),
                };
                let owner = owner_joined.then_some(FixtureOwnerId(*question + 1));
                self.field_terminals.insert(
                    *question,
                    FixtureFieldOutcome {
                        question: asked.clone(),
                        file: self.request.file().into(),
                        owner,
                        definition_line: *definition_line,
                        reader,
                        storage,
                        diagnostics,
                        runtime,
                    },
                );
            }
            FixtureEvent::DiagnosticsTerminal { count } => {
                if self.diagnostics_terminal || *count != self.value.diagnostics.len() as u64 {
                    self.gap("The diagnostic terminal disagrees with its records");
                }
                self.diagnostics_terminal = true;
                self.value.diagnostic_coverage = DiagnosticCoverage::Complete {
                    window: DiagnosticWindow::FixtureFileLoad,
                };
            }
            FixtureEvent::LoadReturned { file, field_count } => {
                if !self.loading || self.returned || file != self.request.file() {
                    self.gap("The fixture return has no unique matching loader entry");
                    return;
                }
                if *field_count != self.value.field_reads.len() as u64 {
                    self.gap("The loader return disagrees with the field reads");
                }
                self.returned = true;
            }
            FixtureEvent::End {
                registrations,
                field_reads,
                field_outcomes,
                diagnostics,
                producer_last_sequence,
            } => {
                if !self.returned
                    || *producer_last_sequence != record.seq
                    || *registrations != self.value.registration_entries.len() as u64
                    || *field_reads != self.value.field_reads.len() as u64
                    || *field_outcomes != self.field_terminals.len() as u64
                    || *diagnostics != self.value.diagnostics.len() as u64
                    || (!self.request.field_questions.is_empty() && !self.diagnostics_terminal)
                    || (self.request.requests(Kind::RegistrationEntries)
                        && !self.registrations_ended)
                {
                    self.gap("The fixture terminal disagrees with its window");
                }
                self.value.field_outcomes = (0..self.request.field_questions.len() as u64)
                    .filter_map(|index| self.field_terminals.remove(&index))
                    .collect();
                self.ended = true;
            }
            FixtureEvent::Unavailable { .. } => unreachable!(),
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
                "fixture:malformed":hook
            }}),
            json!({"kind":"resume","error":"success"}),
            fixture(json!({"kind":"load-start","file":"common/traditions/sample.txt"})),
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
                Some("diagnostics-terminal") => event["event"]["count"] = json!(0),
                Some("end") => event["event"]["diagnostics"] = json!(0),
                _ => {}
            }
        }
        let last = events.len();
        for (index, event) in events.iter_mut().enumerate() {
            event["seq"] = json!(index + 1);
            if event.pointer("/event/kind").and_then(Value::as_str) == Some("end") {
                event["event"]["producer_last_sequence"] = json!(last);
            }
        }
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
            FixtureStorage::String { occurrences, final_value }
                if occurrences.iter().map(|item| item.value.as_str()).collect::<Vec<_>>() == ["one", "two"]
                    && final_value == "two"
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
        missing_diagnostic_terminal.remove(9);
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
        unjoined_diagnostic[6]["event"]["file"] = json!("foreign.txt");
        unjoined_diagnostic[6]["event"]["line"] = json!(0);
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
}
