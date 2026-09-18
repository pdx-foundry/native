use std::collections::BTreeSet;

use crate::records::{Descriptor, OwnerEvent, TraceEvent, TraceRecord};
use crate::store::sha256;
use crate::{
    Activation, ArtifactReference, Completion, Disposal, EvidenceReference, Gap, Observation,
    ObservationFact, ReplayResult, ResultOrigin, SubjectHandle,
};

pub(crate) fn derive(
    descriptor: &Descriptor,
    reference: &ArtifactReference,
    fixture: &str,
    trace: &[TraceRecord],
    owner: &[OwnerEvent],
) -> ReplayResult {
    let mut gaps = sequence_gaps(trace);
    let observations = observations(trace, &descriptor.trace, &reference.sha256);
    let activation = activation(trace, owner, fixture);
    if activation == Activation::NotEstablished {
        gaps.push(Gap::ActivationNotEstablished);
    }
    for record in trace {
        let reason = match &record.event {
            TraceEvent::CapabilityUnavailable { reason }
            | TraceEvent::EarlyActivationUnavailable { reason }
            | TraceEvent::NativeException { reason } => Some(reason),
            TraceEvent::CallbackError { error } => Some(error),
            _ => None,
        };
        if let Some(reason) = reason {
            gaps.push(Gap::Unavailable {
                reason: reason.clone(),
            });
        }
    }
    validate_window(trace, fixture, &mut gaps);
    let worker_lost = owner
        .iter()
        .any(|event| matches!(event, OwnerEvent::WorkerExited { returncode } if *returncode != 0));
    if worker_lost {
        gaps.push(Gap::WorkerLost);
    }
    let completion = if worker_lost {
        Completion::WorkerLost
    } else if gaps
        .iter()
        .any(|gap| matches!(gap, Gap::Unavailable { .. }))
    {
        Completion::Unavailable
    } else if gaps.is_empty() {
        Completion::Complete
    } else {
        Completion::Incomplete
    };
    let disposal = disposal(owner);
    if disposal == Disposal::Unconfirmed {
        gaps.push(Gap::DisposalUnconfirmed);
    }
    ReplayResult {
        contract: descriptor.contract.clone(), evidence_format: descriptor.format.clone(), attempt: descriptor.attempt.clone(), context: reference.sha256.clone(),
        origin: ResultOrigin::Replay, capture_origin: descriptor.origin, activation, completion, disposal,
        observations, gaps, evidence: Vec::new(),
        limits: vec!["First three registration call entries only; not the entire registry".into(),
            "Category read entries for tree_template at line 2 and traditions at line 3 in one fixture".into(),
            "No stored values, successful registration returns, validation, world state, or rule completeness established".into(),
            "Historical retained derivation only; no current target qualification or fresh disposal".into()],
    }
}

fn sequence_gaps(trace: &[TraceRecord]) -> Vec<Gap> {
    let mut expected = 1;
    let mut gaps = Vec::new();
    for record in trace {
        if record.seq != expected {
            gaps.push(Gap::Sequence {
                expected,
                found: record.seq,
            });
        }
        expected = record.seq.saturating_add(1);
    }
    gaps
}

fn observations(
    trace: &[TraceRecord],
    artifact: &ArtifactReference,
    context: &str,
) -> Vec<Observation> {
    trace
        .iter()
        .filter_map(|record| {
            let fact = match &record.event {
                TraceEvent::RegistrationObserved {
                    engine_token,
                    ordinal,
                } => ObservationFact::RegistrationEntry {
                    engine_token: *engine_token,
                    ordinal: *ordinal,
                },
                TraceEvent::FieldObserved {
                    file,
                    line,
                    field,
                    owner,
                    ordinal,
                } => ObservationFact::CategoryReadEntry {
                    file: file.clone(),
                    line: *line,
                    field: field.clone(),
                    ordinal: *ordinal,
                    owner: SubjectHandle {
                        context: context.into(),
                        identity: sha256(owner.as_bytes()),
                    },
                },
                _ => return None,
            };
            Some(Observation {
                fact,
                evidence: EvidenceReference {
                    artifact: artifact.clone(),
                    record: Some(record.seq),
                },
            })
        })
        .collect()
}

fn single(trace: &[TraceRecord], predicate: impl Fn(&TraceEvent) -> bool) -> Option<&TraceRecord> {
    let mut selected = trace.iter().filter(|record| predicate(&record.event));
    let record = selected.next()?;
    if selected.next().is_some() {
        return None;
    }
    Some(record)
}

fn activation(trace: &[TraceRecord], owner: &[OwnerEvent], fixture: &str) -> Activation {
    if activation_witnesses(trace, owner, fixture).is_some() {
        Activation::Demonstrated
    } else {
        Activation::NotEstablished
    }
}

fn activation_witnesses(trace: &[TraceRecord], owner: &[OwnerEvent], fixture: &str) -> Option<()> {
    let start = single(trace, |event| {
        matches!(event, TraceEvent::LaunchStopped { .. })
    })?;
    let active = single(trace, |event| {
        matches!(event, TraceEvent::HooksActiveBeforeResume { .. })
    })?;
    let resume = single(trace, |event| matches!(event, TraceEvent::Resume { .. }))?;
    let registration_phase = single(
        trace,
        |event| matches!(event, TraceEvent::PhaseReached { phase, .. } if phase == "effect-registration"),
    )?;
    let parse = single(
        trace,
        |event| matches!(event, TraceEvent::PhaseReached { phase, .. } if phase == "fixture-file-parse"),
    )?;
    let finish = single(trace, |event| {
        matches!(event, TraceEvent::PhaseComplete { .. })
    })?;
    let end = single(trace, |event| matches!(event, TraceEvent::StreamEnd { .. }))?;
    let TraceEvent::LaunchStopped {
        error,
        pid,
        triple,
        frames,
    } = &start.event
    else {
        return None;
    };
    let owned = owner
        .iter()
        .filter_map(|event| match event {
            OwnerEvent::GameOwnedSuspended { pid, identity } if !identity.is_empty() => Some(*pid),
            _ => None,
        })
        .collect::<Vec<_>>();
    if error != "success"
        || *pid == 0
        || owned != [*pid]
        || !triple.starts_with("arm64-")
        || frames.first().map(|frame| frame.function.as_str()) != Some("_dyld_start")
    {
        return None;
    }
    let TraceEvent::HooksActiveBeforeResume { hooks } = &active.event else {
        return None;
    };
    if !["registration", "load-file", "field"].iter().all(|name| {
        hooks.get(*name).is_some_and(|hook| {
            hook.enabled && hook.locations == 1 && hook.resolved == 1 && hook.hits == 0
        })
    }) {
        return None;
    }
    if !matches!(&resume.event, TraceEvent::Resume { error } if error == "success") {
        return None;
    }
    if !matches!(&registration_phase.event, TraceEvent::PhaseReached { stack, .. }
        if stack.iter().any(|frame| frame.contains("findAndRunAllInitializers")))
    {
        return None;
    }
    if !matches!(&parse.event, TraceEvent::PhaseReached { file: Some(file), .. } if file == fixture)
        || !matches!(&finish.event, TraceEvent::PhaseComplete { phase, file, .. } if phase == "fixture-file-parse" && file == fixture)
    {
        return None;
    }
    if !(start.seq < active.seq
        && active.seq < resume.seq
        && resume.seq < registration_phase.seq
        && registration_phase.seq < parse.seq
        && parse.seq < finish.seq
        && finish.seq < end.seq)
    {
        return None;
    }
    // Record loss after parse entry affects completion, while the earlier activation witnesses survive.
    if trace
        .iter()
        .take_while(|record| record.seq <= parse.seq)
        .enumerate()
        .any(|(index, record)| record.seq != index as u64 + 1)
    {
        return None;
    }
    let registrations: Vec<_> = trace
        .iter()
        .filter(|record| matches!(record.event, TraceEvent::RegistrationObserved { .. }))
        .collect();
    if registrations.len() != 3
        || !registrations
            .iter()
            .all(|record| registration_phase.seq < record.seq && record.seq < parse.seq)
    {
        return None;
    }
    if !trace.iter().filter(|record| matches!(record.event, TraceEvent::FieldObserved { .. })).all(|record| {
        parse.seq < record.seq && record.seq < finish.seq
            && matches!(&record.event, TraceEvent::FieldObserved { file, .. } if file == fixture)
    }) { return None; }
    Some(())
}

fn validate_window(trace: &[TraceRecord], fixture: &str, gaps: &mut Vec<Gap>) {
    let terminals: Vec<_> = trace
        .iter()
        .filter(|record| matches!(record.event, TraceEvent::StreamEnd { .. }))
        .collect();
    if terminals.is_empty() {
        gaps.push(Gap::MissingTerminal);
        return;
    }
    if terminals.len() != 1 {
        integrity(gaps, "multiple observation terminals");
        return;
    }
    let end = terminals[0];
    let TraceEvent::StreamEnd {
        producer_last_sequence,
        producer_field_count,
        registrations: expected_registrations,
    } = end.event
    else {
        return;
    };
    let registrations: Vec<_> = trace
        .iter()
        .filter_map(|record| match record.event {
            TraceEvent::RegistrationObserved { ordinal, .. } => Some((record.seq, ordinal)),
            _ => None,
        })
        .collect();
    let fields: Vec<_> = trace
        .iter()
        .filter_map(|record| match &record.event {
            TraceEvent::FieldObserved {
                file,
                line,
                field,
                owner,
                ordinal,
            } => Some((record.seq, file, *line, field, owner, *ordinal)),
            _ => None,
        })
        .collect();
    if producer_last_sequence != end.seq
        || producer_field_count != fields.len() as u64
        || expected_registrations != registrations.len() as u64
    {
        integrity(gaps, "terminal sequence or observation totals differ");
    }
    if registrations.iter().any(|(seq, _)| *seq >= end.seq)
        || fields.iter().any(|(seq, ..)| *seq >= end.seq)
    {
        integrity(gaps, "observations follow the terminal");
    }
    if registrations
        .iter()
        .map(|(_, ordinal)| *ordinal)
        .collect::<Vec<_>>()
        != [1, 2, 3]
    {
        integrity(gaps, "bounded registration ordinals differ");
    }
    let marker = single(trace, |event| {
        matches!(event, TraceEvent::RegistrationWindowComplete { .. })
    });
    if !marker.is_some_and(|record| matches!(record.event, TraceEvent::RegistrationWindowComplete { observed: 3 })
        && registrations.last().is_some_and(|(seq, _)| *seq < record.seq) && record.seq < end.seq
        && single(trace, |event| matches!(event, TraceEvent::PhaseReached { phase, .. } if phase == "fixture-file-parse")).is_some_and(|parse| record.seq < parse.seq)) {
        integrity(gaps, "registration window marker missing or inconsistent");
    }
    if fields
        .iter()
        .map(|(_, file, line, field, _, ordinal)| (file.as_str(), *line, field.as_str(), *ordinal))
        .collect::<Vec<_>>()
        != [
            (fixture, 2, "tree_template", 1),
            (fixture, 3, "traditions", 2),
        ]
    {
        integrity(gaps, "bounded category field reads differ");
    }
    let owners: BTreeSet<_> = fields
        .iter()
        .map(|(_, _, _, _, owner, _)| owner.as_str())
        .collect();
    if !fields.is_empty() && (owners.len() != 1 || owners.contains("")) {
        gaps.push(Gap::OwnerJoin);
    }
    let finish = single(trace, |event| {
        matches!(event, TraceEvent::PhaseComplete { .. })
    });
    if !finish.is_some_and(|record| matches!(&record.event, TraceEvent::PhaseComplete { phase, file, producer_field_count }
        if phase == "fixture-file-parse" && file == fixture && *producer_field_count == fields.len() as u64)
        && record.seq < end.seq) {
        integrity(gaps, "fixture parse completion count or ordering differs");
    }
}

fn integrity(gaps: &mut Vec<Gap>, reason: &str) {
    gaps.push(Gap::WindowIntegrity {
        reason: reason.into(),
    });
}

fn disposal(owner: &[OwnerEvent]) -> Disposal {
    let owned: Vec<_> = owner
        .iter()
        .filter_map(|event| match event {
            OwnerEvent::GameOwnedSuspended { pid, identity } => Some((*pid, identity)),
            _ => None,
        })
        .collect();
    if owned.len() == 1
        && owned[0].0 != 0
        && !owned[0].1.is_empty()
        && matches!(owner.last(), Some(OwnerEvent::DisposalChecked {
        confirmed: true, reaped_pid, game_exit: Some(_), remaining_identity: None,
    }) if *reaped_pid == owned[0].0)
    {
        return Disposal::Confirmed;
    }
    Disposal::Unconfirmed
}
