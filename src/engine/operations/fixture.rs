//! Reduce the bounded fixture window. Native joins addresses internally, then replaces them
//! with session-local owner identities. Only witnessed read entries leave this module.
use super::event_stream::{self, OwnerEvent, WorkerEvent, WorkerRecord};
use crate::{
    Answer, Basis, BuildId, Completeness, Error, FieldRead, FixtureObservation,
    FixtureObservationKind as Kind, FixtureOwnerId, FixtureRequest, Gap, GapKind, Operation,
    ProcessingStage, RegistrationEntry, Source,
};
use serde::{Deserialize, Serialize};

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
    LoadReturned {
        file: String,
        field_count: u64,
    },
    End {
        registrations: u64,
        field_reads: u64,
        producer_last_sequence: u64,
    },
    Unavailable {
        reason: String,
    },
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
    let Some((thread, resumed)) = event_stream::activation(records, owner_events, &hooks) else {
        return Err(Error::Observation {
            operation: Operation::ObserveFixture,
            reason: "Fixture hook activation before resume was not established".into(),
        });
    };
    let mut window = Window::new(request, thread, resumed);
    for record in records {
        if record.seq != window.next_sequence {
            window.gap("Observation records are missing or out of order");
        }
        window.next_sequence = record.seq.saturating_add(1);
        match &record.event {
            WorkerEvent::Fixture { event } => window.accept(record, event),
            WorkerEvent::CallbackError { .. }
            | WorkerEvent::NativeException { .. }
            | WorkerEvent::CapabilityUnavailable { .. }
            | WorkerEvent::EarlyActivationUnavailable { .. } => {
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
                producer_last_sequence,
            } => {
                if !self.returned
                    || *producer_last_sequence != record.seq
                    || *registrations != self.value.registration_entries.len() as u64
                    || *field_reads != self.value.field_reads.len() as u64
                    || (self.request.requests(Kind::RegistrationEntries)
                        && !self.registrations_ended)
                {
                    self.gap("The fixture terminal disagrees with its window");
                }
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
}
