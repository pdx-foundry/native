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
use super::event_stream::{OwnerEvent, PauseCause, WorkerEvent, WorkerRecord, single};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Where a game is paused. This is a point in initialization; no world is loaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameReadiness {
    /// Every observed registry returned from its initial loader; the game stays paused.
    PausedAfterRegistryInitialization,
    /// Only a part of the observed registries returned before the pause.
    PausedDuringRegistryInitialization,
    /// All content has loaded: the game is held where the engine documents its modifiers,
    /// before it shows its main menu. Requested with `GameOptions::loaded_modifiers`.
    PausedAfterContentLoad,
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
    let mut unsupported = false;
    let mut saw_registry = false;
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
        saw_registry |= registry_name(&record.event) == Some(name);
        match &record.event {
            WorkerEvent::RegistryLoadStart {
                owner, directory, ..
            } => {
                if loading.is_some()
                    || snapshot.is_some()
                    || ended
                    || directory != name
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
                    || directory != name
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
                let slot_witnessed = snapshot.is_some_and(|(expected_owner, count, thread)| {
                    owner == expected_owner && *index < count && thread == record.thread
                }) && !ended
                    && *index >= next_index;
                let key_readable = !key.is_empty() && key.len() < 4095 && !key.contains('\0');
                let witnessed = slot_witnessed && pointer(object) && key_readable;

                // The key is claimed before the object: a new key stays claimed when its object
                // repeats, and an object stays unclaimed when its key repeats.
                let claimed =
                    witnessed && keys.insert(key.clone()) && objects.insert(object.clone());

                if !claimed {
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
            WorkerEvent::RegistryUnsupported { reason, .. } => {
                unsupported = true;
                diagnostics.insert(0, reason.clone());
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
            | WorkerEvent::ModifierDocumentationEntered
            | WorkerEvent::ModifierTable { .. }
            | WorkerEvent::ModifierTableEnd { .. }
            | WorkerEvent::ModifierUnavailable { .. }
            | WorkerEvent::HooksRequested
            | WorkerEvent::LaunchStopped { .. }
            | WorkerEvent::HooksActiveBeforeResume { .. }
            | WorkerEvent::Resume { .. }
            | WorkerEvent::SessionPaused { .. }
            | WorkerEvent::WorkerFinished
            | WorkerEvent::WorkerLossReady => {}
        }
    }
    let pause_cause = records.iter().find_map(|record| match record.event {
        WorkerEvent::SessionPaused { cause, .. } => Some(cause),
        _ => None,
    });
    let session_failed = owner
        .iter()
        .any(|event| matches!(event, OwnerEvent::ObservationUnavailable { .. }));
    if let Some(cause) = pause_cause
        && activated
        && !saw_registry
        && diagnostics.is_empty()
        && !session_failed
    {
        return RegistryItems {
            items: Vec::new(),
            observed: Observed::NotLoaded,
            diagnostics: vec![not_loaded_reason(name, cause)],
        };
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
    } else if unsupported {
        Observed::Unsupported
    } else {
        Observed::Partial
    };
    RegistryItems {
        items,
        observed,
        diagnostics,
    }
}

/// Why the loader of `name` was not observed, from what ended the session first.
fn not_loaded_reason(name: &str, cause: PauseCause) -> String {
    match cause {
        PauseCause::Deadline => format!(
            "the initial loader of {name} did not run before the session's startup deadline stopped the game; the game had not reached it"
        ),
        PauseCause::LoadersReturned => format!(
            "the initial loader of {name} did not run before the other selected loaders returned and the session paused; late and on-demand loaders are outside this method"
        ),
        PauseCause::ContentLoaded => format!(
            "the initial loader of {name} did not run before all content loaded; late and on-demand loaders are outside this method"
        ),
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
        | WorkerEvent::RegistryUnavailable { name, .. }
        | WorkerEvent::RegistryUnsupported { name, .. } => Some(name),
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
/// loader entry and one loader return on the pause thread, in order, before the pause. The
/// pause's cause must agree with the stream: a pause after the loaders returned names every
/// activated registry, and a pause after content loaded follows the documentation entry. Whether
/// the items of a registry are complete is a separate question.
pub(crate) fn readiness(
    records: &[WorkerRecord],
    owner: &[OwnerEvent],
    declared: &[String],
    loaded_modifiers: bool,
) -> Option<GameReadiness> {
    let pause = single(records, |event| {
        matches!(event, WorkerEvent::SessionPaused { .. })
    })?;
    let WorkerEvent::SessionPaused { returned, cause } = &pause.event else {
        return None;
    };
    if returned.iter().collect::<BTreeSet<_>>().len() != returned.len()
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
            || directory != name
            || entry.thread != end.thread
            || end.thread != pause.thread
            || !(entry.seq < end.seq && end.seq < pause.seq)
        {
            return None;
        }
    }
    let documented = single(records, |event| {
        matches!(event, WorkerEvent::ModifierDocumentationEntered)
    })
    .is_some_and(|entered| entered.thread == pause.thread && entered.seq < pause.seq);
    match cause {
        PauseCause::ContentLoaded => {
            (loaded_modifiers && documented).then_some(GameReadiness::PausedAfterContentLoad)
        }
        PauseCause::LoadersReturned => {
            let active_loader_missing = declared
                .iter()
                .any(|name| !returned.contains(name) && activated(records, owner, name));
            if active_loader_missing {
                return None;
            }
            Some(if returned.len() == declared.len() {
                GameReadiness::PausedAfterRegistryInitialization
            } else {
                GameReadiness::PausedDuringRegistryInitialization
            })
        }
        PauseCause::Deadline => Some(GameReadiness::PausedDuringRegistryInitialization),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    const TRADITIONS: &str = "common/traditions";
    const CATEGORIES: &str = "common/tradition_categories";

    /// A session that observed two registries. Row numbers, from 0: 0 to 3 are the activation,
    /// then each registry has a loader entry, a return, a snapshot, its entries and a terminal.
    /// `traditions` starts at row 4 with the given keys; the categories hold one item.
    fn session(keys: &[&str]) -> (Vec<WorkerRecord>, Vec<OwnerEvent>, Vec<String>) {
        let names = vec![TRADITIONS.to_string(), CATEGORIES.to_string()];
        let mut rows = vec![
            json!({"kind":"hooks-requested"}),
            json!({"kind":"launch-stopped","error":"success","pid":10,"triple":"arm64-test","frames":[{"function":"_dyld_start"}]}),
            json!({"kind":"hooks-active-before-resume","hooks":{
                "registry:common/traditions":{"enabled":true,"locations":1,"resolved":1,"hits":0},
                "registry:common/tradition_categories":{"enabled":true,"locations":1,"resolved":1,"hits":0}}}),
            json!({"kind":"resume","error":"success"}),
        ];
        for (name, owner, keys) in [
            (TRADITIONS, "0x1000", keys),
            (CATEGORIES, "0x3000", &["category"][..]),
        ] {
            rows.extend([
                json!({"kind":"registry-load-start","name":name,"directory":name,"owner":owner}),
                json!({"kind":"registry-load-returned","name":name,"owner":owner}),
                json!({"kind":"registry-snapshot","name":name,"directory":name,"owner":owner,"count":keys.len()}),
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
        rows.push(json!({"kind":"session-paused","returned":names,"cause":"loaders-returned"}));
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
            readiness(&records, &owner, &names, false),
            Some(GameReadiness::PausedAfterRegistryInitialization)
        );
    }

    #[test]
    fn nested_and_outside_common_directories_are_registry_identities() {
        let (mut records, mut owner, mut names) = session(&["first"]);
        for (old, new) in [
            (TRADITIONS, "map/galaxy"),
            (CATEGORIES, "common/governments/civics"),
        ] {
            for record in &mut records {
                let json = serde_json::to_string(record).unwrap().replace(old, new);
                *record = serde_json::from_str(&json).unwrap();
            }
            for event in &mut owner {
                let json = serde_json::to_string(event).unwrap().replace(old, new);
                *event = serde_json::from_str(&json).unwrap();
            }
            for name in &mut names {
                if name == old {
                    *name = new.into();
                }
            }
        }
        for name in &names {
            assert_eq!(reduce(name, &records, &owner).observed, Observed::Complete);
        }
        assert_eq!(
            readiness(&records, &owner, &names, false),
            Some(GameReadiness::PausedAfterRegistryInitialization)
        );
    }

    /// Turn the session's pause into one that the worker's deadline caused, with `returned`.
    fn pause_at_deadline(records: &mut [WorkerRecord], owner: &mut [OwnerEvent], names: &[&str]) {
        let returned: Vec<String> = names.iter().map(|name| (*name).into()).collect();
        for record in records.iter_mut() {
            if matches!(record.event, WorkerEvent::SessionPaused { .. }) {
                record.event = WorkerEvent::SessionPaused {
                    returned: returned.clone(),
                    cause: PauseCause::Deadline,
                };
            }
        }
        for event in owner.iter_mut() {
            if let OwnerEvent::GamePauseConfirmed {
                returned: confirmed,
                ..
            } = event
            {
                *confirmed = returned.clone();
            }
        }
    }

    #[test]
    fn a_selected_loader_that_never_runs_is_not_an_empty_complete_answer() {
        let (mut records, mut owner, names) = session(&["first"]);
        records.retain(|record| registry_name(&record.event) != Some(CATEGORIES));
        for (index, record) in records.iter_mut().enumerate() {
            record.seq = index as u64 + 1;
        }
        pause_at_deadline(&mut records, &mut owner, &[TRADITIONS]);
        assert_eq!(
            readiness(&records, &owner, &names, false),
            Some(GameReadiness::PausedDuringRegistryInitialization)
        );
        let absent = reduce(CATEGORIES, &records, &owner);
        assert_eq!(absent.observed, Observed::NotLoaded);
        assert!(absent.items.is_empty());
        assert!(absent.diagnostics[0].contains("startup deadline"));
    }

    #[test]
    fn a_pause_after_the_loaders_returned_must_name_every_active_loader() {
        // The categories hook was active, but the worker claims that the loaders returned
        // without it: the witnesses disagree, and no readiness follows.
        let (mut records, mut owner, names) = session(&["first"]);
        records.retain(|record| registry_name(&record.event) != Some(CATEGORIES));
        for (index, record) in records.iter_mut().enumerate() {
            record.seq = index as u64 + 1;
            if let WorkerEvent::SessionPaused { returned, .. } = &mut record.event {
                *returned = vec![TRADITIONS.into()];
            }
        }
        for event in &mut owner {
            if let OwnerEvent::GamePauseConfirmed { returned, .. } = event {
                *returned = vec![TRADITIONS.into()];
            }
        }
        assert_eq!(readiness(&records, &owner, &names, false), None);
        // Without the categories hook, the same pause is consistent: only the active loader
        // had to return.
        let WorkerEvent::HooksActiveBeforeResume { hooks } = &mut records[2].event else {
            unreachable!()
        };
        hooks.remove("registry:common/tradition_categories");
        assert_eq!(
            readiness(&records, &owner, &names, false),
            Some(GameReadiness::PausedDuringRegistryInitialization)
        );
        assert_eq!(
            reduce(CATEGORIES, &records, &owner).observed,
            Observed::Unavailable
        );
    }

    #[test]
    fn the_not_loaded_reason_says_what_ended_the_session() {
        let (mut records, owner, _) = session(&["first"]);
        records.retain(|record| registry_name(&record.event) != Some(CATEGORIES));
        for (index, record) in records.iter_mut().enumerate() {
            record.seq = index as u64 + 1;
        }
        let pause = records.len() - 1;
        for (cause, expected) in [
            (PauseCause::Deadline, "startup deadline"),
            (
                PauseCause::LoadersReturned,
                "other selected loaders returned",
            ),
            (PauseCause::ContentLoaded, "before all content loaded"),
        ] {
            records[pause].event = WorkerEvent::SessionPaused {
                returned: vec![TRADITIONS.into()],
                cause,
            };
            let absent = reduce(CATEGORIES, &records, &owner);
            assert_eq!(absent.observed, Observed::NotLoaded, "{cause:?}");
            assert!(absent.diagnostics[0].contains(expected), "{cause:?}");
        }
    }

    #[test]
    fn a_pause_after_content_loaded_needs_the_documentation_witness() {
        let (mut records, owner, names) = session(&["first"]);
        let pause = records.len() - 1;
        records[pause].event = WorkerEvent::SessionPaused {
            returned: names.clone(),
            cause: PauseCause::ContentLoaded,
        };
        assert_eq!(readiness(&records, &owner, &names, true), None);
        let mut entered = records[pause].clone();
        entered.event = WorkerEvent::ModifierDocumentationEntered;
        records.insert(pause, entered);
        for (index, record) in records.iter_mut().enumerate() {
            record.seq = index as u64 + 1;
        }
        assert_eq!(
            readiness(&records, &owner, &names, true),
            Some(GameReadiness::PausedAfterContentLoad)
        );
        // The same pause without the requested table is not a content-load pause.
        assert_eq!(readiness(&records, &owner, &names, false), None);
    }

    #[test]
    fn an_unestablished_key_layout_refuses_item_names() {
        let (mut records, owner, names) = session(&[]);
        records.retain(|record| {
            !matches!(
                &record.event,
                WorkerEvent::RegistrySnapshot { name, .. } | WorkerEvent::RegistryEnd { name, .. }
                    if name == TRADITIONS
            )
        });
        let pause = records
            .iter()
            .position(|record| matches!(record.event, WorkerEvent::SessionPaused { .. }))
            .unwrap();
        let mut unsupported = records[pause].clone();
        unsupported.event = WorkerEvent::RegistryUnsupported {
            name: TRADITIONS.into(),
            reason: "item key storage was not established".into(),
        };
        records.insert(pause, unsupported);
        for (index, record) in records.iter_mut().enumerate() {
            record.seq = index as u64 + 1;
        }

        assert_eq!(
            readiness(&records, &owner, &names, false),
            Some(GameReadiness::PausedAfterRegistryInitialization)
        );
        let items = reduce(TRADITIONS, &records, &owner);
        assert_eq!(items.observed, Observed::Unsupported);
        assert!(items.items.is_empty());
        assert!(items.diagnostics[0].contains("item key storage was not established"));
    }

    #[test]
    fn a_pause_with_no_returned_loaders_keeps_the_selected_registry_unsupported() {
        let (mut records, mut owner, _) = session(&["first"]);
        records.retain(|record| registry_name(&record.event).is_none());
        for (index, record) in records.iter_mut().enumerate() {
            record.seq = index as u64 + 1;
        }
        pause_at_deadline(&mut records, &mut owner, &[]);
        assert_eq!(
            readiness(&records, &owner, &[TRADITIONS.into()], false),
            Some(GameReadiness::PausedDuringRegistryInitialization)
        );
        assert_eq!(
            reduce(TRADITIONS, &records, &owner).observed,
            Observed::NotLoaded
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
    fn a_refused_entry_keeps_its_new_key_claimed_but_not_its_object() {
        // Rows 7 to 9 are the entries, with objects 0x1800, 0x1808 and 0x1810.
        let (mut records, owner, _) = session(&["first", "second", "third"]);
        change(&mut records, 8, "object", json!("0x1800"));
        change(&mut records, 9, "key", json!("second"));
        assert_eq!(reduce(TRADITIONS, &records, &owner).items, ["first"]);

        let (mut records, owner, _) = session(&["first", "second", "third"]);
        change(&mut records, 8, "key", json!("first"));
        change(&mut records, 9, "object", json!("0x1808"));
        assert_eq!(
            reduce(TRADITIONS, &records, &owner).items,
            ["first", "third"]
        );
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
            readiness(&records, &owner, &names, false),
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
            assert_eq!(readiness(&records, &owner, &names, false), None);
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
        hooks.remove("registry:common/traditions");
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
        assert_eq!(readiness(&records, &no_confirmation, &names, false), None);
        let mut no_return = records.clone();
        no_return.retain(|record| !matches!(&record.event, WorkerEvent::RegistryLoadReturned { name, .. } if name == TRADITIONS));
        assert_eq!(readiness(&no_return, &owner, &names, false), None);
        let mut duplicate = records.clone();
        duplicate.insert(6, records[5].clone());
        assert_eq!(readiness(&duplicate, &owner, &names, false), None);
        let mut wrong_game = owner.clone();
        wrong_game[1] = OwnerEvent::GamePauseConfirmed {
            pid: 11,
            returned: names.clone(),
        };
        assert_eq!(readiness(&records, &wrong_game, &names, false), None);
    }

    #[test]
    fn a_pause_after_a_part_of_the_registries_is_a_pause_during_initialization() {
        let (mut records, mut owner, names) = session(&["first"]);
        pause_at_deadline(&mut records, &mut owner, &[CATEGORIES]);
        assert_eq!(
            readiness(&records, &owner, &names, false),
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
