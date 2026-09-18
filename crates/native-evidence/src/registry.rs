//! Read-only registry snapshot derivation. This module cannot launch a game.
use crate::recorded::{OwnerEvent, TraceEvent, TraceRecord};
use crate::store::{ArtifactStore, sha256};
use crate::{
    Activation, ArtifactReference, CaptureOrigin, Completion, Disposal, EvidenceReference,
    ReplayError, ResultOrigin, SubjectHandle,
};
use serde::Serialize;
use std::collections::BTreeSet;

/// Retained registry snapshot format, independent of the historical read-entry format.
pub const FORMAT: &str = "pdx-native/registry-snapshot-v1";
/// Entry keys at the selected registry's initial loader return, before validation.
pub const CONTRACT: &str = "pdx-native/registry-keys-after-initial-load-v1";

/// One named object in the engine's collection at the snapshot boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegistryEntry {
    /// Engine-defined identifier; no name is inferred from a configuration file.
    pub key: String,
    /// Opaque identity scoped to this evidence context, not a portable memory address.
    pub subject: SubjectHandle,
}

/// Opaque retained provenance; keep it with the answer rather than interpreting it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegistryProvenance {
    descriptor: ArtifactReference,
    evidence: Vec<EvidenceReference>,
}

/// Engine collection keys, including explicit partial and unavailable outcomes.
#[derive(Debug, Clone, Serialize)]
pub struct RegistryResult {
    /// Stable public name of the queried registry.
    pub registry: String,
    /// Registered items in engine collection order. Partial results retain established items.
    #[serde(rename = "registeredItems")]
    pub registered_items: Vec<RegistryEntry>,
    /// Whether every slot in the collection was captured at the stated boundary.
    pub completion: Completion,
    /// Whether required hooks were installed before engine execution resumed.
    pub activation: Activation,
    /// Independent historical cleanup result, not inferred from completion.
    pub disposal: Disposal,
    /// Live admitted capture or replay of retained bytes.
    pub origin: ResultOrigin,
    /// Captured engine evidence or authored synthetic records.
    pub capture_origin: CaptureOrigin,
    /// Retained evidence identities, opaque to callers.
    pub provenance: RegistryProvenance,
    /// Missing or inconsistent evidence; never silently interpreted as an empty registry.
    pub diagnostics: Vec<String>,
    /// Scope of this answer; completeness never implies field or rule coverage.
    pub limits: Vec<String>,
}

/// Verify all pinned artifacts and derive a registry answer without an installation.
pub fn replay(
    store: &ArtifactStore,
    reference: &ArtifactReference,
) -> Result<RegistryResult, ReplayError> {
    let retained = crate::replay::load(store, reference, FORMAT, CONTRACT)?;
    validate_content_copies(store, &retained)?;
    let request = &retained.request;
    let name = request
        .observations
        .first()
        .and_then(|value| value.strip_prefix("registry:"));
    let Some(name) = name.filter(|name| !name.is_empty()) else {
        return Err(ReplayError::Malformed {
            path: retained.descriptor.request.path.clone(),
            reason: "Expected one registry request".into(),
        });
    };
    if request.observations.len() != 1
        || !request.fixtures.is_empty()
        || !(1..=180).contains(&request.deadline_seconds)
    {
        return Err(ReplayError::Malformed {
            path: retained.descriptor.request.path.clone(),
            reason: "Invalid registry request".into(),
        });
    }
    let (entries, activation, mut completion, mut diagnostics) =
        derive(name, reference, &retained.trace, &retained.owner);
    diagnostics.extend(
        retained
            .provenance_gaps
            .iter()
            .map(|gap| format!("{gap:?}")),
    );
    if !diagnostics.is_empty() && completion == Completion::Complete {
        completion = Completion::Incomplete;
    }
    Ok(RegistryResult {
        registry: name.into(), registered_items: entries, activation, completion,
        disposal: crate::stream::disposal(&retained.owner), origin: ResultOrigin::Replay,
        capture_origin: retained.descriptor.origin,
        provenance: RegistryProvenance { descriptor: reference.clone(), evidence: retained.evidence },
        diagnostics,
        limits: vec!["Collection keys at the initial loader return, before post-load validation; no field values, rules, or gameplay availability established".into(),
            "Private copies of pinned installed tradition files replace the two registry directories; DLC additions, user mods, and later registry changes are outside this answer".into()],
    })
}

fn validate_content_copies(
    store: &ArtifactStore,
    retained: &crate::replay::VerifiedAttempt,
) -> Result<(), ReplayError> {
    let malformed = |reason: &str| ReplayError::Malformed {
        path: retained.descriptor.manifest.path.clone(),
        reason: reason.into(),
    };
    let references = &retained.evidence;
    let manifest = references
        .iter()
        .find(|reference| reference.artifact.path == "producer-content.json")
        .ok_or_else(|| malformed("Registry content manifest missing"))?;
    let content: std::collections::BTreeMap<String, String> =
        serde_json::from_slice(&store.read(&manifest.artifact)?)
            .map_err(|error| malformed(&error.to_string()))?;
    let prefix = "profile/mod/native_registry/";
    let copies: std::collections::BTreeMap<_, _> = references
        .iter()
        .filter_map(|reference| {
            reference
                .artifact
                .path
                .strip_prefix(prefix)
                .map(|path| (path, reference.artifact.sha256.as_str()))
        })
        .collect();
    let expected: std::collections::BTreeMap<_, _> = content
        .iter()
        .filter(|(path, _)| path.starts_with("common/"))
        .map(|(path, hash)| (path.as_str(), hash.as_str()))
        .collect();
    if copies != expected {
        return Err(malformed(
            "Private registry content differs from pinned installed inputs",
        ));
    }
    Ok(())
}

fn derive(
    name: &str,
    reference: &ArtifactReference,
    trace: &[TraceRecord],
    owner: &[OwnerEvent],
) -> (Vec<RegistryEntry>, Activation, Completion, Vec<String>) {
    let activation = activation(trace, owner);
    let mut diagnostics = Vec::new();
    if activation != Activation::Demonstrated {
        diagnostics.push("Registry hook activation not established".into());
    }
    let mut expected = 1;
    let mut loading = None;
    let mut snapshot = None;
    let mut ended = false;
    let mut entries = Vec::new();
    let mut next_index = 0;
    let mut keys = BTreeSet::new();
    let mut objects = BTreeSet::new();
    let mut unavailable = false;
    for record in trace {
        if record.seq != expected {
            diagnostics.push(format!(
                "Sequence gap: expected {expected}, found {}",
                record.seq
            ));
        }
        expected = record.seq.saturating_add(1);
        match &record.event {
            TraceEvent::RegistryLoadStart {
                name: found,
                owner,
                directory,
            } => {
                if loading.is_some()
                    || snapshot.is_some()
                    || ended
                    || found != name
                    || directory != &format!("common/{name}")
                    || !pointer(owner)
                    || record.thread.unwrap_or(0) == 0
                {
                    diagnostics.push("Invalid or repeated registry loader".into());
                } else {
                    loading = Some((owner.as_str(), record.thread));
                }
            }
            TraceEvent::RegistrySnapshot {
                name: found,
                owner,
                directory,
                count,
            } => {
                if !loading.is_some_and(|(expected_owner, thread)| {
                    expected_owner == owner && thread == record.thread
                }) || snapshot.is_some()
                    || ended
                    || found != name
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
            TraceEvent::RegistryEntry {
                name: found,
                owner,
                index,
                object,
                key,
            } => {
                let valid = snapshot.is_some_and(|(expected_owner, count, thread)| {
                    owner == expected_owner && *index < count && thread == record.thread
                }) && !ended
                    && found == name
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
                entries.push(RegistryEntry {
                    key: key.clone(),
                    subject: SubjectHandle {
                        context: reference.sha256.clone(),
                        identity: sha256(format!("{name}:{object}").as_bytes()),
                    },
                });
            }
            TraceEvent::RegistryEnd {
                name: found,
                owner,
                count,
                producer_last_sequence,
            } => {
                if ended
                    || found != name
                    || *producer_last_sequence != record.seq
                    || *count != entries.len() as u64
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
            TraceEvent::CapabilityUnavailable { reason }
            | TraceEvent::EarlyActivationUnavailable { reason }
            | TraceEvent::NativeException { reason } => {
                unavailable = true;
                diagnostics.push(reason.clone());
            }
            TraceEvent::CallbackError { error } => {
                unavailable = true;
                diagnostics.push(error.clone());
            }
            TraceEvent::HooksRequested
            | TraceEvent::LaunchStopped { .. }
            | TraceEvent::HooksActiveBeforeResume { .. }
            | TraceEvent::Resume { .. }
            | TraceEvent::WorkerFinished
            | TraceEvent::WorkerDispose
            | TraceEvent::WorkerLossReady => {}
            _ => diagnostics.push("Unexpected read-entry event in a registry capture".into()),
        }
    }
    if !ended {
        diagnostics.push("Registry terminal missing".into());
    }
    for event in owner {
        if let OwnerEvent::ObservationUnavailable { reason } = event {
            unavailable = true;
            diagnostics.push(reason.clone());
        }
    }
    let worker_lost = owner
        .iter()
        .any(|event| matches!(event, OwnerEvent::WorkerExited { returncode } if *returncode != 0));
    let completion = if diagnostics.is_empty() {
        Completion::Complete
    } else if worker_lost {
        Completion::WorkerLost
    } else if unavailable {
        Completion::Unavailable
    } else {
        Completion::Incomplete
    };
    (entries, activation, completion, diagnostics)
}

fn pointer(value: &str) -> bool {
    value
        .strip_prefix("0x")
        .and_then(|value| u64::from_str_radix(value, 16).ok())
        .is_some_and(|value| value != 0 && value % 8 == 0)
}

fn activation(trace: &[TraceRecord], owner: &[OwnerEvent]) -> Activation {
    let witness = || -> Option<()> {
        let mut launches = trace
            .iter()
            .filter(|row| matches!(row.event, TraceEvent::LaunchStopped { .. }));
        let launch = launches.next()?;
        if launches.next().is_some() {
            return None;
        }
        let TraceEvent::LaunchStopped {
            error,
            pid,
            triple,
            frames,
        } = &launch.event
        else {
            return None;
        };
        let owned: Vec<_> = owner
            .iter()
            .filter_map(|row| match row {
                OwnerEvent::GameOwnedSuspended { pid, identity } if !identity.is_empty() => {
                    Some(*pid)
                }
                _ => None,
            })
            .collect();
        if error != "success"
            || *pid == 0
            || owned != [*pid]
            || !triple.starts_with("arm64-")
            || !frames.iter().any(|frame| frame.function == "_dyld_start")
            || launch.thread.unwrap_or(0) == 0
        {
            return None;
        }
        let mut active = trace
            .iter()
            .filter(|row| matches!(row.event, TraceEvent::HooksActiveBeforeResume { .. }));
        let active_row = active.next()?;
        if active.next().is_some() {
            return None;
        }
        let TraceEvent::HooksActiveBeforeResume { hooks } = &active_row.event else {
            return None;
        };
        if hooks.len() != 1
            || !hooks.get("registry").is_some_and(|hook| {
                hook.enabled && hook.locations == 1 && hook.resolved == 1 && hook.hits == 0
            })
        {
            return None;
        }
        let mut resumes = trace
            .iter()
            .filter(|row| matches!(row.event, TraceEvent::Resume { .. }));
        let resume = resumes.next()?;
        if resumes.next().is_some()
            || !matches!(&resume.event, TraceEvent::Resume { error } if error == "success")
            || !(launch.seq < active_row.seq && active_row.seq < resume.seq)
        {
            return None;
        }
        if trace.iter().any(|row| {
            matches!(
                row.event,
                TraceEvent::RegistrySnapshot { .. } | TraceEvent::RegistryLoadStart { .. }
            ) && (row.seq <= resume.seq || row.thread != launch.thread)
        }) {
            return None;
        }
        Some(())
    };
    if witness().is_some() {
        Activation::Demonstrated
    } else {
        Activation::NotEstablished
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    fn fixture(keys: &[&str]) -> (ArtifactReference, Vec<TraceRecord>, Vec<OwnerEvent>) {
        let mut rows = vec![
            json!({"kind":"hooks-requested"}),
            json!({"kind":"launch-stopped","error":"success","pid":10,"triple":"arm64-test","frames":[{"function":"_dyld_start"}]}),
            json!({"kind":"hooks-active-before-resume","hooks":{"registry":{"enabled":true,"locations":1,"resolved":1,"hits":0}}}),
            json!({"kind":"resume","error":"success"}),
            json!({"kind":"registry-load-start","name":"traditions","directory":"common/traditions","owner":"0x1000"}),
            json!({"kind":"registry-snapshot","name":"traditions","directory":"common/traditions","owner":"0x1000","count":keys.len()}),
        ];
        for (index, key) in keys.iter().enumerate() {
            rows.push(json!({"kind":"registry-entry","name":"traditions","owner":"0x1000","index":index,"object":format!("0x{:x}",0x2000 + index * 8),"key":key}));
        }
        rows.push(json!({"kind":"registry-end","name":"traditions","owner":"0x1000","count":keys.len(),"producerLastSequence":rows.len()+1}));
        let trace = rows
            .into_iter()
            .enumerate()
            .map(|(index, mut row)| {
                row["run"] = json!("unit");
                row["seq"] = json!(index + 1);
                row["thread"] = json!(7);
                serde_json::from_value(row).unwrap()
            })
            .collect();
        let owner = vec![OwnerEvent::GameOwnedSuspended {
            pid: 10,
            identity: "owned".into(),
        }];
        (
            ArtifactReference {
                path: "descriptor.json".into(),
                sha256: "a".repeat(64),
                bytes: 1,
            },
            trace,
            owner,
        )
    }
    fn change(trace: &mut [TraceRecord], index: usize, field: &str, value: Value) {
        let mut row = serde_json::to_value(&trace[index]).unwrap();
        row[field] = value;
        trace[index] = serde_json::from_value(row).unwrap();
    }
    #[test]
    fn empty_is_complete_only_with_activation_snapshot_and_terminal() {
        let (reference, mut trace, owner) = fixture(&[]);
        let (entries, _, completion, _) = derive("traditions", &reference, &trace, &owner);
        assert!(entries.is_empty());
        assert_eq!(completion, Completion::Complete);
        trace.pop();
        assert_eq!(
            derive("traditions", &reference, &trace, &owner).2,
            Completion::Incomplete
        );
        assert_eq!(
            derive("traditions", &reference, &[], &owner).2,
            Completion::Incomplete
        );
    }
    #[test]
    fn names_counts_owners_threads_and_slots_must_join() {
        for (row, field, value) in [
            (5, "name", json!("technology")),
            (5, "directory", json!("common/other")),
            (5, "owner", json!("0x0")),
            (5, "count", json!(100001)),
            (6, "owner", json!("0x3000")),
            (6, "index", json!(1)),
            (6, "key", json!("")),
            (6, "thread", json!(8)),
            (6, "object", json!("0x0")),
            (7, "key", json!("first")),
            (7, "object", json!("0x2000")),
            (8, "count", json!(3)),
            (8, "producerLastSequence", json!(10)),
        ] {
            let (reference, mut trace, owner) = fixture(&["first", "second"]);
            change(&mut trace, row, field, value);
            assert_ne!(
                derive("traditions", &reference, &trace, &owner).2,
                Completion::Complete,
                "{row} {field}"
            );
        }
    }
    #[test]
    fn missing_record_retains_other_established_entries_without_completeness() {
        let (reference, mut trace, owner) = fixture(&["first", "second", "third"]);
        trace.remove(7);
        let (entries, _, completion, _) = derive("traditions", &reference, &trace, &owner);
        assert_eq!(completion, Completion::Incomplete);
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.key.as_str())
                .collect::<Vec<_>>(),
            ["first", "third"]
        );
    }
    #[test]
    fn early_or_missing_activation_cannot_establish_a_registry() {
        for row in [1, 2, 3] {
            let (reference, mut trace, owner) = fixture(&["first"]);
            trace.remove(row);
            assert_eq!(
                derive("traditions", &reference, &trace, &owner).1,
                Activation::NotEstablished
            );
        }
        let (reference, mut trace, owner) = fixture(&["first"]);
        change(&mut trace, 4, "seq", json!(1));
        assert_eq!(
            derive("traditions", &reference, &trace, &owner).1,
            Activation::NotEstablished
        );
    }
}
