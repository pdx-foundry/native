//! The two event streams of a live session, and the rules for reading the worker's stream.
//!
//! The debugger worker appends one JSON record for each event to `raw-trace.jsonl` in the work
//! directory. Each record carries the session's attempt identity and a sequence number that
//! starts at 1 and has no holes. The supervisor keeps its own owner events: what it did to the
//! game and worker processes, and what it confirmed about them.
//!
//! The operations in this module's siblings join the two streams. A record that is missing,
//! damaged, or from another session can only make an answer less complete, never more complete.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One record of the worker's stream.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub(crate) struct WorkerRecord {
    /// Position in the stream, from 1. A hole means that a record was lost.
    pub seq: u64,
    /// Game thread on which the event occurred, when the event has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread: Option<u64>,
    /// Attempt identity of the session that wrote the record.
    pub run: String,
    #[serde(flatten)]
    pub event: WorkerEvent,
}

/// State of one debugger hook immediately before the game resumes.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub(crate) struct Hook {
    pub enabled: bool,
    pub locations: u64,
    pub resolved: u64,
    pub hits: u64,
}

/// The top stack frame of one game thread at the suspended launch.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub(crate) struct Frame {
    pub function: String,
}

/// What the worker saw. `owner` and `object` are engine addresses as hexadecimal text; they
/// join records to each other and never leave the supervisor.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub(crate) enum WorkerEvent {
    /// Events of the bounded consumer fixture window.
    Fixture { event: super::fixture::FixtureEvent },
    /// The worker set its hooks; the debugger has not attached yet.
    HooksRequested,
    /// The debugger attached to the game while it was still suspended at its first instruction.
    LaunchStopped {
        error: String,
        pid: u64,
        triple: String,
        frames: Vec<Frame>,
    },
    /// Every hook in this map was in place before the game ran any code.
    HooksActiveBeforeResume { hooks: BTreeMap<String, Hook> },
    /// The worker let the game run.
    Resume { error: String },
    /// A registry's loader was entered. `owner` is the loader's receiver.
    RegistryLoadStart {
        name: String,
        owner: String,
        directory: String,
    },
    /// The same loader returned to its caller.
    RegistryLoadReturned { name: String, owner: String },
    /// The worker started to read the collection, which holds `count` slots.
    RegistrySnapshot {
        name: String,
        owner: String,
        directory: String,
        count: u64,
    },
    /// One slot of the collection.
    RegistryEntry {
        name: String,
        owner: String,
        index: u64,
        object: String,
        key: String,
    },
    /// The terminal of one registry: the worker's own count of entries, and the sequence number
    /// that the worker gave this record.
    RegistryEnd {
        name: String,
        owner: String,
        count: u64,
        #[serde(rename = "producerLastSequence")]
        producer_last_sequence: u64,
    },
    /// The worker could not observe this registry.
    RegistryUnavailable { name: String, reason: String },
    /// The collection was reached, but this binding cannot read its item keys.
    RegistryUnsupported { name: String, reason: String },
    /// The engine entered the function that documents its modifiers.
    ModifierDocumentationEntered,
    /// The worker read the modifier table when that function returned, and wrote it once to
    /// `loaded-modifiers.json`: `count` entries, in a file of `bytes` bytes with this SHA-256.
    ModifierTable {
        count: u64,
        bytes: u64,
        sha256: String,
    },
    /// The terminal of the modifier observation, and the sequence number that the worker gave
    /// this record.
    ModifierTableEnd {
        count: u64,
        #[serde(rename = "producerLastSequence")]
        producer_last_sequence: u64,
    },
    /// The worker could not read the modifier table.
    ModifierUnavailable { reason: String },
    /// The game is held at a safe pause, after these registries returned from their loaders.
    /// `cause` says what ended the observation and left the game held.
    SessionPaused {
        returned: Vec<String>,
        cause: PauseCause,
    },
    /// The debugger could not attach, a hook was missing or late, or the supervisor's permission
    /// to resume did not arrive.
    CapabilityUnavailable { reason: String },
    /// The game was not suspended at its first instruction when the debugger attached.
    EarlyActivationUnavailable { reason: String },
    /// A hook callback failed outside the observation of one registry.
    CallbackError {
        #[serde(default)]
        error: String,
    },
    /// The game stopped on a native exception.
    NativeException {
        #[serde(default)]
        reason: String,
    },
    /// The worker reached the point where the worker-loss fault stops it.
    WorkerLossReady,
    /// The worker ended in order.
    WorkerFinished,
}

/// Why the worker held the game where it did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum PauseCause {
    /// Every registry whose hook was active returned from its initial loader.
    LoadersReturned,
    /// The engine's modifier documentation returned, after all content loaded.
    ContentLoaded,
    /// The worker's deadline passed first, and the worker stopped the game where it was.
    Deadline,
}

/// What the supervisor did and confirmed, in order.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub(crate) enum OwnerEvent {
    /// The supervisor created the game as its own suspended child. `identity` is the process
    /// start identity, which a reused process number cannot match.
    GameOwnedSuspended { pid: u64, identity: String },
    /// The supervisor started the debugger worker.
    WorkerStarted,
    /// The worker answered a fresh pause check, so the debugger still holds the game.
    GamePauseConfirmed { pid: u64, returned: Vec<String> },
    /// The session failed in the supervisor.
    ObservationUnavailable { reason: String },
    /// The supervisor asked the worker to stop. A later nonzero exit is then not a loss.
    WorkerStopRequested,
    /// The worker process ended with this code; a negative code is a signal.
    WorkerExited { returncode: i64 },
    /// The result of the supervisor's attempt to reap the game.
    DisposalChecked {
        confirmed: bool,
        #[serde(rename = "reapedPid")]
        reaped_pid: u64,
        #[serde(rename = "gameExit")]
        game_exit: Option<i64>,
    },
}

/// Read the worker's stream file of the session `attempt`.
///
/// A record must be one complete line of bounded length, carry this session's attempt identity,
/// and have a sequence number above zero. At the first record that breaks a rule, reading stops
/// and the records before it are kept. Every registry terminal is then removed, so that a damaged
/// stream cannot give a complete answer, even when the damage follows the terminal. The second
/// value describes the damage.
pub(crate) fn read_worker_stream(raw: &[u8], attempt: &str) -> (Vec<WorkerRecord>, Option<String>) {
    let mut records = Vec::new();
    let mut damage = None;
    for (index, line) in raw.split_inclusive(|byte| *byte == b'\n').enumerate() {
        let parsed = serde_json::from_slice::<WorkerRecord>(line);
        let valid = line.len() <= crate::protocol::observation::MAX_RECORD
            && line.ends_with(b"\n")
            && parsed
                .as_ref()
                .is_ok_and(|record| record.run == attempt && record.seq != 0);
        if !valid {
            damage = Some(format!(
                "Worker stream record {} is damaged, partial, or from another session",
                index + 1
            ));
            break;
        }
        records.extend(parsed);
    }
    if damage.is_some() {
        records.retain(|record| {
            !matches!(
                record.event,
                WorkerEvent::RegistryEnd { .. }
                    | WorkerEvent::ModifierTableEnd { .. }
                    | WorkerEvent::Fixture {
                        event: super::fixture::FixtureEvent::End { .. }
                    }
            )
        });
    }
    (records, damage)
}

/// The only record that matches, or `None` when there are none or several.
pub(crate) fn single(
    records: &[WorkerRecord],
    matches: impl Fn(&WorkerEvent) -> bool,
) -> Option<&WorkerRecord> {
    let mut found = records.iter().filter(|record| matches(&record.event));
    let record = found.next()?;
    found.next().is_none().then_some(record)
}

/// Establish the owned loader-entry stop and every required hook before resume.
/// Returns the launch thread and resume sequence; operation reducers enforce their event order.
pub(crate) fn activation(
    records: &[WorkerRecord],
    owner: &[OwnerEvent],
    required: &[&str],
) -> Option<(u64, u64)> {
    let witness = || -> Option<(u64, u64)> {
        let launch = single(records, |event| {
            matches!(event, WorkerEvent::LaunchStopped { .. })
        })?;
        let WorkerEvent::LaunchStopped {
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
            .filter_map(|event| match event {
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
        let active = single(records, |event| {
            matches!(event, WorkerEvent::HooksActiveBeforeResume { .. })
        })?;
        let WorkerEvent::HooksActiveBeforeResume { hooks } = &active.event else {
            return None;
        };
        if !required.iter().all(|name| {
            hooks.get(*name).is_some_and(|hook| {
                hook.enabled && hook.locations == 1 && hook.resolved == 1 && hook.hits == 0
            })
        }) {
            return None;
        }
        let resume = single(records, |event| matches!(event, WorkerEvent::Resume { .. }))?;
        if !matches!(&resume.event, WorkerEvent::Resume { error } if error == "success")
            || !(launch.seq < active.seq && active.seq < resume.seq)
        {
            return None;
        }
        Some((launch.thread?, resume.seq))
    };
    witness()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TERMINAL: &str = r#"{"run":"a","seq":1,"kind":"registry-end","name":"traditions","owner":"0x1000","count":0,"producerLastSequence":1}"#;

    #[test]
    fn a_damaged_tail_removes_every_terminal() {
        let whole = format!("{TERMINAL}\n");
        let (records, damage) = read_worker_stream(whole.as_bytes(), "a");
        assert_eq!(records.len(), 1);
        assert!(damage.is_none());
        let damaged = format!("{TERMINAL}\n{{\"seq\":");
        let (records, damage) = read_worker_stream(damaged.as_bytes(), "a");
        assert!(records.is_empty());
        assert!(damage.is_some());
    }

    #[test]
    fn a_record_of_another_session_or_without_a_sequence_number_stops_the_read() {
        let whole = format!("{TERMINAL}\n");
        assert!(read_worker_stream(whole.as_bytes(), "foreign").1.is_some());
        let zero = whole.replace("\"seq\":1", "\"seq\":0");
        assert!(read_worker_stream(zero.as_bytes(), "a").1.is_some());
    }

    #[test]
    fn a_sequence_hole_is_kept_for_the_reducer_and_a_partial_line_keeps_the_prefix() {
        let raw = b"{\"seq\":1,\"run\":\"a\",\"kind\":\"hooks-requested\"}\n{\"seq\":3,\"run\":\"a\",\"kind\":\"worker-finished\"}\n";
        let (records, damage) = read_worker_stream(raw, "a");
        assert!(damage.is_none());
        assert_eq!(
            records.iter().map(|record| record.seq).collect::<Vec<_>>(),
            [1, 3]
        );
        let (records, damage) = read_worker_stream(&[raw.as_slice(), b"{"].concat(), "a");
        assert_eq!(records.len(), 2);
        assert!(damage.is_some());
    }
}
