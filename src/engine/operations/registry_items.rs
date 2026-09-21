//! Registry items: from the two event streams of a session to the item names of one registry.
//!
//! The worker reads a registry's collection when the registry's initial loader returns, before
//! the engine validates the entries. [`reduce`] accepts the items only through a chain of
//! witnesses, and each broken link is a diagnostic:
//!
//! - **Activation.** The debugger held the game at its first instruction, the registry's hook
//!   was in place before the game ran, and the game process is the one that the supervisor owns.
//!   Without this, items could have loaded before anyone looked.
//! - **Sequence.** Record numbers have no hole. A hole that lies wholly inside the records of
//!   another registry does not count against this one.
//! - **Loader, owner and thread.** The loader entry, its return, the snapshot, every entry and
//!   the terminal name the same receiver object and the same game thread, in that order.
//! - **Slots.** Entry indices rise without a hole, and each key and each object occurs once.
//! - **Terminal totals.** The terminal's count equals the snapshot's count and the number of
//!   accepted entries, and its own sequence number equals the one that the worker gave it.
//! - **Worker loss.** The worker did not exit with a failure before the supervisor asked it to
//!   stop.
//!
//! No diagnostic gives a complete answer. Any diagnostic gives a partial answer that keeps the
//! accepted items, or no answer when the registry was not observed at all. An empty complete
//! answer therefore means an empty registry, never "could not look".
use super::event_stream::{OwnerEvent, WorkerEvent, WorkerRecord, single};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Where a game is paused. This is a point in initialization; no world is loaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameReadiness {
    /// Every observed registry returned from its initial loader; the game stays paused.
    PausedAfterRegistryInitialization,
    /// Only a part of the observed registries returned before the pause.
    PausedDuringRegistryInitialization,
}

/// How much of one registry's collection the session established.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Observed {
    /// Every slot was read and every witness agrees.
    Complete,
    /// The registry was observed, but a witness is missing. The accepted items are kept.
    Partial,
    /// The registry was not observed: no activation, no access, or the worker was lost.
    Unavailable,
}

/// The items of one registry, in the engine's collection order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RegistryItems {
    pub items: Vec<String>,
    pub observed: Observed,
    /// Each missing or inconsistent witness. Empty exactly when `observed` is `Complete`.
    pub diagnostics: Vec<String>,
}

/// Reduce the streams to the items of the registry `name`.
pub(crate) fn reduce(name: &str, records: &[WorkerRecord], owner: &[OwnerEvent]) -> RegistryItems {
    let activated = activated(records, owner, name);
    let mut diagnostics = Vec::new();
    if !activated {
        diagnostics.push("Registry hook activation not established".into());
    }
    let mut expected = 1;
    let mut loading = None;
    let mut snapshot = None;
    let mut ended = false;
    let mut returned = false;
    let mut items = Vec::new();
    let mut next_index = 0;
    let mut keys = BTreeSet::new();
    let mut objects = BTreeSet::new();
    let mut unavailable = false;
    for (position, record) in records.iter().enumerate() {
        // A hole wholly inside the records of another registry does not count against this one.
        let hole_elsewhere = position > 0
            && registry_name(&records[position - 1].event)
                .zip(registry_name(&record.event))
                .is_some_and(|(before, after)| before == after && before != name);
        let hole_in_fixture = position > 0
            && matches!(records[position - 1].event, WorkerEvent::Fixture { .. })
            && matches!(record.event, WorkerEvent::Fixture { .. });
        if record.seq != expected && !hole_elsewhere && !hole_in_fixture {
            diagnostics.push(format!(
                "Sequence gap: expected {expected}, found {}",
                record.seq
            ));
        }
        expected = record.seq.saturating_add(1);
        if registry_name(&record.event).is_some_and(|found| found != name) {
            continue;
        }
        match &record.event {
            WorkerEvent::RegistryLoadStart {
                owner, directory, ..
            } => {
                if loading.is_some()
                    || snapshot.is_some()
                    || ended
                    || directory != &format!("common/{name}")
                    || !pointer(owner)
                    || record.thread.unwrap_or(0) == 0
                {
                    diagnostics.push("Invalid or repeated registry loader".into());
                } else {
                    loading = Some((owner.as_str(), record.thread));
                }
            }
            WorkerEvent::RegistryLoadReturned { owner, .. } => {
                if returned
                    || !loading.is_some_and(|(receiver, thread)| {
                        receiver == owner && thread == record.thread
                    })
                {
                    diagnostics
                        .push("Registry return lacks a unique matching loader witness".into());
                }
                returned = true;
            }
            WorkerEvent::RegistrySnapshot {
                owner,
                directory,
                count,
                ..
            } => {
                if !loading.is_some_and(|(expected_owner, thread)| {
                    expected_owner == owner && thread == record.thread
                }) || !returned
                    || snapshot.is_some()
                    || ended
                    || directory != &format!("common/{name}")
                    || !pointer(owner)
                    || *count > 100_000
                    || record.thread.unwrap_or(0) == 0
                {
                    diagnostics.push("Invalid or repeated registry snapshot".into());
                } else {
                    snapshot = Some((owner.as_str(), *count, record.thread));
                }
            }
            WorkerEvent::RegistryEntry {
                owner,
                index,
                object,
                key,
                ..
            } => {
                let valid = snapshot.is_some_and(|(expected_owner, count, thread)| {
                    owner == expected_owner && *index < count && thread == record.thread
                }) && !ended
                    && *index >= next_index
                    && pointer(object)
                    && !key.is_empty()
                    && key.len() < 4095
                    && !key.contains('\0')
                    && keys.insert(key.clone())
                    && objects.insert(object.clone());
                if !valid {
                    diagnostics.push(
                        "Registry entry lacks a unique slot, owner, key, or thread witness".into(),
                    );
                    continue;
                }
                if *index != next_index {
                    diagnostics.push("Registry slot missing".into());
                }
                next_index = index.saturating_add(1);
                items.push(key.clone());
            }
            WorkerEvent::RegistryEnd {
                owner,
                count,
                producer_last_sequence,
                ..
            } => {
                if ended
                    || *producer_last_sequence != record.seq
                    || *count != items.len() as u64
                    || !snapshot.is_some_and(|(expected_owner, expected_count, thread)| {
                        owner == expected_owner
                            && *count == expected_count
                            && thread == record.thread
                    })
                {
                    diagnostics.push("Registry terminal disagrees with its snapshot".into());
                }
                ended = true;
            }
            WorkerEvent::RegistryUnavailable { reason, .. } => {
                unavailable = true;
                diagnostics.push(reason.clone());
            }
            // A failure after this registry's terminal does not take its answer back.
            WorkerEvent::CapabilityUnavailable { reason }
            | WorkerEvent::EarlyActivationUnavailable { reason }
            | WorkerEvent::NativeException { reason } => {
                if !ended {
                    unavailable = true;
                    diagnostics.push(reason.clone());
                }
            }
            WorkerEvent::CallbackError { error } => {
                if !ended {
                    unavailable = true;
                    diagnostics.push(error.clone());
                }
            }
            WorkerEvent::Fixture { .. }
            | WorkerEvent::HooksRequested
            | WorkerEvent::LaunchStopped { .. }
            | WorkerEvent::HooksActiveBeforeResume { .. }
            | WorkerEvent::Resume { .. }
            | WorkerEvent::SessionPaused { .. }
            | WorkerEvent::WorkerFinished
            | WorkerEvent::WorkerLossReady => {}
        }
    }
    if !ended {
        diagnostics.push("Registry terminal missing".into());
        for event in owner {
            if let OwnerEvent::ObservationUnavailable { reason } = event {
                unavailable = true;
                diagnostics.push(reason.clone());
            }
        }
    }
    let stop_requested = owner
        .iter()
        .any(|event| matches!(event, OwnerEvent::WorkerStopRequested));
    let worker_lost = !stop_requested
        && owner.iter().any(
            |event| matches!(event, OwnerEvent::WorkerExited { returncode } if *returncode != 0),
        );
    if worker_lost && !diagnostics.is_empty() {
        diagnostics.push("The observation worker was lost".into());
    }
    let observed = if diagnostics.is_empty() {
        Observed::Complete
    } else if worker_lost || unavailable || !activated {
        Observed::Unavailable
    } else {
        Observed::Partial
    };
    RegistryItems {
        items,
        observed,
        diagnostics,
    }
}

/// An engine address as the worker writes it: hexadecimal, not null, aligned to a pointer.
pub(super) fn pointer(value: &str) -> bool {
    value
        .strip_prefix("0x")
        .and_then(|value| u64::from_str_radix(value, 16).ok())
        .is_some_and(|value| value != 0 && value % 8 == 0)
}

fn registry_name(event: &WorkerEvent) -> Option<&str> {
    match event {
        WorkerEvent::RegistryLoadStart { name, .. }
        | WorkerEvent::RegistrySnapshot { name, .. }
        | WorkerEvent::RegistryEntry { name, .. }
        | WorkerEvent::RegistryEnd { name, .. }
        | WorkerEvent::RegistryLoadReturned { name, .. }
        | WorkerEvent::RegistryUnavailable { name, .. } => Some(name),
        _ => None,
    }
}

/// The registry loader must run on the activated thread after resume.
fn activated(records: &[WorkerRecord], owner: &[OwnerEvent], registry: &str) -> bool {
    let hook = format!("registry:{registry}");
    let Some((thread, resumed)) = super::event_stream::activation(records, owner, &[&hook]) else {
        return false;
    };
    !records.iter().any(|record| {
        matches!(
            record.event,
            WorkerEvent::RegistrySnapshot { .. } | WorkerEvent::RegistryLoadStart { .. }
        ) && (record.seq <= resumed || record.thread != Some(thread))
    })
}

/// Where the game is paused, or `None` when the pause witnesses are missing or disagree.
///
/// The worker's pause record and the supervisor's pause confirmation must name the same
/// registries and the owned game. Each named registry needs its activation witness, and one
/// loader entry and one loader return on the pause thread, in order, before the pause. Whether
/// the items of a registry are complete is a separate question.
pub(crate) fn readiness(
    records: &[WorkerRecord],
    owner: &[OwnerEvent],
    declared: &[String],
) -> Option<GameReadiness> {
    let pause = single(records, |event| {
        matches!(event, WorkerEvent::SessionPaused { .. })
    })?;
    let WorkerEvent::SessionPaused { returned } = &pause.event else {
        return None;
    };
    if returned.is_empty()
        || returned.iter().collect::<BTreeSet<_>>().len() != returned.len()
        || !returned.iter().all(|name| declared.contains(name))
    {
        return None;
    }
    let owned = owner.iter().find_map(|event| match event {
        OwnerEvent::GameOwnedSuspended { pid, .. } => Some(*pid),
        _ => None,
    })?;
    let confirmations: Vec<_> = owner
        .iter()
        .filter_map(|event| match event {
            OwnerEvent::GamePauseConfirmed { pid, returned } => Some((*pid, returned)),
            _ => None,
        })
        .collect();
    if confirmations != [(owned, returned)] {
        return None;
    }
    for name in returned {
        if !activated(records, owner, name) {
            return None;
        }
        let entry = single(
            records,
            |event| matches!(event, WorkerEvent::RegistryLoadStart { name: found, .. } if found == name),
        )?;
        let end = single(
            records,
            |event| matches!(event, WorkerEvent::RegistryLoadReturned { name: found, .. } if found == name),
        )?;
        let (
            WorkerEvent::RegistryLoadStart {
                owner: receiver,
                directory,
                ..
            },
            WorkerEvent::RegistryLoadReturned {
                owner: returned_receiver,
                ..
            },
        ) = (&entry.event, &end.event)
        else {
            return None;
        };
        if receiver != returned_receiver
            || !pointer(receiver)
            || *directory != format!("common/{name}")
            || entry.thread != end.thread
            || end.thread != pause.thread
            || !(entry.seq < end.seq && end.seq < pause.seq)
        {
            return None;
        }
    }
    Some(if returned.len() == declared.len() {
        GameReadiness::PausedAfterRegistryInitialization
    } else {
        GameReadiness::PausedDuringRegistryInitialization
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    const TRADITIONS: &str = "traditions";
    const CATEGORIES: &str = "tradition_categories";

    /// A session that observed two registries. Row numbers, from 0: 0 to 3 are the activation,
    /// then each registry has a loader entry, a return, a snapshot, its entries and a terminal.
    /// `traditions` starts at row 4 with the given keys; the categories hold one item.
    fn session(keys: &[&str]) -> (Vec<WorkerRecord>, Vec<OwnerEvent>, Vec<String>) {
        let names = vec![TRADITIONS.to_string(), CATEGORIES.to_string()];
        let mut rows = vec![
            json!({"kind":"hooks-requested"}),
            json!({"kind":"launch-stopped","error":"success","pid":10,"triple":"arm64-test","frames":[{"function":"_dyld_start"}]}),
            json!({"kind":"hooks-active-before-resume","hooks":{
                "registry:traditions":{"enabled":true,"locations":1,"resolved":1,"hits":0},
                "registry:tradition_categories":{"enabled":true,"locations":1,"resolved":1,"hits":0}}}),
            json!({"kind":"resume","error":"success"}),
        ];
        for (name, owner, keys) in [
            (TRADITIONS, "0x1000", keys),
            (CATEGORIES, "0x3000", &["category"][..]),
        ] {
            rows.extend([
                json!({"kind":"registry-load-start","name":name,"directory":format!("common/{name}"),"owner":owner}),
                json!({"kind":"registry-load-returned","name":name,"owner":owner}),
                json!({"kind":"registry-snapshot","name":name,"directory":format!("common/{name}"),"owner":owner,"count":keys.len()}),
            ]);
            for (index, key) in keys.iter().enumerate() {
                let object = format!(
                    "{:#x}",
                    u64::from_str_radix(&owner[2..], 16).unwrap() + 0x800 + index as u64 * 8
                );
                rows.push(json!({"kind":"registry-entry","name":name,"owner":owner,"index":index,"object":object,"key":key}));
            }
            rows.push(json!({"kind":"registry-end","name":name,"owner":owner,"count":keys.len(),"producerLastSequence":rows.len()+1}));
        }
        rows.push(json!({"kind":"session-paused","returned":names}));
        let records = rows
            .into_iter()
            .enumerate()
            .map(|(index, mut row)| {
                row["run"] = json!("unit");
                row["thread"] = json!(7);
                row["seq"] = json!(index + 1);
                serde_json::from_value(row).unwrap()
            })
            .collect();
        let owner = vec![
            OwnerEvent::GameOwnedSuspended {
                pid: 10,
                identity: "owned".into(),
            },
            OwnerEvent::GamePauseConfirmed {
                pid: 10,
                returned: names.clone(),
            },
        ];
        (records, owner, names)
    }

    fn change(records: &mut [WorkerRecord], index: usize, field: &str, value: Value) {
        let mut row = serde_json::to_value(&records[index]).unwrap();
        row[field] = value;
        records[index] = serde_json::from_value(row).unwrap();
    }

    #[test]
    fn both_registries_reduce_independently_and_in_collection_order() {
        let (records, owner, names) = session(&["first", "second"]);
        let traditions = reduce(TRADITIONS, &records, &owner);
        assert_eq!(traditions.items, ["first", "second"]);
        assert_eq!(traditions.observed, Observed::Complete);
        assert!(traditions.diagnostics.is_empty());
        let categories = reduce(CATEGORIES, &records, &owner);
        assert_eq!(categories.items, ["category"]);
        assert_eq!(categories.observed, Observed::Complete);
        assert_eq!(
            readiness(&records, &owner, &names),
            Some(GameReadiness::PausedAfterRegistryInitialization)
        );
    }

    #[test]
    fn an_empty_registry_is_complete_only_with_activation_snapshot_and_terminal() {
        let (mut records, owner, _) = session(&[]);
        let empty = reduce(TRADITIONS, &records, &owner);
        assert!(empty.items.is_empty());
        assert_eq!(empty.observed, Observed::Complete);
        records.remove(7);
        assert_ne!(
            reduce(TRADITIONS, &records, &owner).observed,
            Observed::Complete
        );
        assert_eq!(
            reduce(TRADITIONS, &[], &owner).observed,
            Observed::Unavailable
        );
    }

    #[test]
    fn names_counts_owners_threads_and_slots_must_join() {
        // Rows: 4 loader entry, 5 return, 6 snapshot, 7 and 8 entries, 9 terminal.
        for (row, field, value) in [
            (4, "directory", json!("common/other")),
            (4, "owner", json!("0x0")),
            (5, "owner", json!("0x2000")),
            (5, "thread", json!(8)),
            (6, "directory", json!("common/other")),
            (6, "owner", json!("0x0")),
            (6, "count", json!(100001)),
            (7, "owner", json!("0x2000")),
            (7, "index", json!(1)),
            (7, "key", json!("")),
            (7, "thread", json!(8)),
            (7, "object", json!("0x0")),
            (8, "key", json!("first")),
            (8, "object", json!("0x1800")),
            (9, "count", json!(3)),
            (9, "owner", json!("0x2000")),
            (9, "producerLastSequence", json!(11)),
        ] {
            let (mut records, owner, _) = session(&["first", "second"]);
            change(&mut records, row, field, value);
            assert_ne!(
                reduce(TRADITIONS, &records, &owner).observed,
                Observed::Complete,
                "{row} {field}"
            );
            assert_eq!(
                reduce(CATEGORIES, &records, &owner).observed,
                Observed::Complete,
                "{row} {field}"
            );
        }
    }

    #[test]
    fn a_loader_on_another_thread_breaks_the_activation_of_every_registry() {
        let (mut records, owner, _) = session(&["first"]);
        change(&mut records, 4, "thread", json!(8));
        for name in [TRADITIONS, CATEGORIES] {
            assert_eq!(
                reduce(name, &records, &owner).observed,
                Observed::Unavailable
            );
        }
    }

    #[test]
    fn a_snapshot_before_the_loader_return_is_refused() {
        let (mut records, owner, _) = session(&["first"]);
        records.swap(5, 6);
        for (index, record) in records.iter_mut().enumerate() {
            record.seq = index as u64 + 1;
        }
        assert_ne!(
            reduce(TRADITIONS, &records, &owner).observed,
            Observed::Complete
        );
    }

    #[test]
    fn a_dropped_record_keeps_the_other_items_and_is_partial() {
        let (mut records, owner, names) = session(&["first", "second", "third"]);
        records.remove(8);
        let traditions = reduce(TRADITIONS, &records, &owner);
        assert_eq!(traditions.observed, Observed::Partial);
        assert_eq!(traditions.items, ["first", "third"]);
        // The hole lies inside the traditions records, so the categories stay complete.
        assert_eq!(
            reduce(CATEGORIES, &records, &owner).observed,
            Observed::Complete
        );
        assert_eq!(
            readiness(&records, &owner, &names),
            Some(GameReadiness::PausedAfterRegistryInitialization)
        );
    }

    #[test]
    fn a_hole_between_two_registries_counts_against_both() {
        let (mut records, owner, _) = session(&["first"]);
        // Remove the traditions terminal: the hole has traditions before it and categories after.
        records.remove(8);
        assert_eq!(
            reduce(TRADITIONS, &records, &owner).observed,
            Observed::Partial
        );
        assert_eq!(
            reduce(CATEGORIES, &records, &owner).observed,
            Observed::Partial
        );
    }

    #[test]
    fn a_missing_terminal_is_partial() {
        let (mut records, owner, _) = session(&["first"]);
        records.retain(
            |record| !matches!(&record.event, WorkerEvent::RegistryEnd { name, .. } if name == CATEGORIES),
        );
        let categories = reduce(CATEGORIES, &records, &owner);
        assert_eq!(categories.observed, Observed::Partial);
        assert_eq!(categories.items, ["category"]);
    }

    #[test]
    fn early_or_missing_activation_cannot_establish_a_registry() {
        for row in [1, 2, 3] {
            let (mut records, owner, names) = session(&["first"]);
            records.remove(row);
            assert_eq!(
                reduce(TRADITIONS, &records, &owner).observed,
                Observed::Unavailable
            );
            assert_eq!(readiness(&records, &owner, &names), None);
        }
        // A loader entry with a sequence number before the resume.
        let (mut records, owner, _) = session(&["first"]);
        change(&mut records, 4, "seq", json!(1));
        assert_eq!(
            reduce(TRADITIONS, &records, &owner).observed,
            Observed::Unavailable
        );
        // The hook of one registry was not in place; the other registry is not affected.
        let (mut records, owner, _) = session(&["first"]);
        let WorkerEvent::HooksActiveBeforeResume { hooks } = &mut records[2].event else {
            unreachable!()
        };
        hooks.remove("registry:traditions");
        assert_eq!(
            reduce(TRADITIONS, &records, &owner).observed,
            Observed::Unavailable
        );
        assert_eq!(
            reduce(CATEGORIES, &records, &owner).observed,
            Observed::Complete
        );
        // The game that the debugger held is not the game that the supervisor owns.
        let (records, mut owner, _) = session(&["first"]);
        owner[0] = OwnerEvent::GameOwnedSuspended {
            pid: 11,
            identity: "owned".into(),
        };
        assert_eq!(
            reduce(TRADITIONS, &records, &owner).observed,
            Observed::Unavailable
        );
    }

    #[test]
    fn an_unavailable_registry_does_not_erase_another_answer() {
        let (mut records, owner, _) = session(&["first"]);
        for record in &mut records {
            if matches!(&record.event, WorkerEvent::RegistryEntry { name, .. } if name == CATEGORIES)
            {
                record.event = WorkerEvent::RegistryUnavailable {
                    name: CATEGORIES.into(),
                    reason: "access failed".into(),
                };
            }
        }
        assert_eq!(
            reduce(TRADITIONS, &records, &owner).observed,
            Observed::Complete
        );
        let categories = reduce(CATEGORIES, &records, &owner);
        assert_eq!(categories.observed, Observed::Unavailable);
        assert!(
            categories
                .diagnostics
                .contains(&"access failed".to_string())
        );
    }

    #[test]
    fn readiness_requires_matching_owner_and_unique_loader_return_witnesses() {
        let (records, owner, names) = session(&["first"]);
        let mut no_confirmation = owner.clone();
        no_confirmation.pop();
        assert_eq!(readiness(&records, &no_confirmation, &names), None);
        let mut no_return = records.clone();
        no_return.retain(|record| !matches!(&record.event, WorkerEvent::RegistryLoadReturned { name, .. } if name == TRADITIONS));
        assert_eq!(readiness(&no_return, &owner, &names), None);
        let mut duplicate = records.clone();
        duplicate.insert(6, records[5].clone());
        assert_eq!(readiness(&duplicate, &owner, &names), None);
        let mut wrong_game = owner.clone();
        wrong_game[1] = OwnerEvent::GamePauseConfirmed {
            pid: 11,
            returned: names.clone(),
        };
        assert_eq!(readiness(&records, &wrong_game, &names), None);
    }

    #[test]
    fn a_pause_after_a_part_of_the_registries_is_a_pause_during_initialization() {
        let (mut records, mut owner, names) = session(&["first"]);
        let returned = vec![CATEGORIES.to_string()];
        records.last_mut().unwrap().event = WorkerEvent::SessionPaused {
            returned: returned.clone(),
        };
        owner[1] = OwnerEvent::GamePauseConfirmed { pid: 10, returned };
        assert_eq!(
            readiness(&records, &owner, &names),
            Some(GameReadiness::PausedDuringRegistryInitialization)
        );
    }

    #[test]
    fn a_later_failure_keeps_a_completed_answer_and_a_lost_worker_gives_none() {
        let (records, mut owner, _) = session(&["first"]);
        owner.push(OwnerEvent::ObservationUnavailable {
            reason: "later session failure".into(),
        });
        owner.push(OwnerEvent::WorkerExited { returncode: -9 });
        assert_eq!(
            reduce(TRADITIONS, &records, &owner).observed,
            Observed::Complete
        );
        // The worker was lost before the terminal of the categories.
        let mut cut = records.clone();
        cut.truncate(records.len() - 2);
        let categories = reduce(CATEGORIES, &cut, &owner);
        assert_eq!(categories.observed, Observed::Unavailable);
        assert_eq!(categories.items, ["category"]);
        // An exit after the supervisor asked the worker to stop is not a loss.
        owner.remove(2);
        owner.insert(2, OwnerEvent::WorkerStopRequested);
        assert_eq!(reduce(CATEGORIES, &cut, &owner).observed, Observed::Partial);
    }
}
