//! The loaded modifier table: the modifiers that the engine holds when it documents them, after
//! all content has loaded.
//!
//! `CGameApplication::InitGame` calls the engine's modifier documentation once, after the
//! databases and their post-inits. When that function returns, the worker reads the whole table
//! and, for each requested registry, the item keys of its database. It writes them once to
//! `loaded-modifiers.json`, and its stream carries the file's size and SHA-256 and a terminal.
//!
//! The table is accepted only when every witness agrees: the hook was active before resume, the
//! documentation was entered and returned on the launch thread, the stream has no hole up to the
//! terminal, the file is the one that the stream names, and every name is distinct. Otherwise the
//! reason is returned and no table is.
use super::event_stream::{self, OwnerEvent, WorkerEvent, WorkerRecord, single};
use crate::protocol::observation::{ModifierEntry, ModifierTable, RegistryKeys};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::protocol::hooks::MODIFIERS_DOCUMENTATION as HOOK;

/// The accepted table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ObservedModifiers {
    /// Each entry's name and category mask, in table order.
    pub entries: Vec<ModifierEntry>,
    /// The item keys of each requested registry, or why they could not be read.
    pub registries: BTreeMap<String, RegistryKeys>,
}

/// Join the stream, the owner events and the table file of the session `attempt`. `registries`
/// are the registries whose keys the session requested.
pub(crate) fn reduce(
    records: &[WorkerRecord],
    owner: &[OwnerEvent],
    file: Option<&[u8]>,
    attempt: &str,
    registries: &[String],
) -> Result<ObservedModifiers, String> {
    if let Some(reason) = records.iter().find_map(|record| match &record.event {
        WorkerEvent::ModifierUnavailable { reason } => Some(reason),
        _ => None,
    }) {
        return Err(reason.clone());
    }
    let (thread, resumed) = event_stream::activation(records, owner, &[HOOK])
        .ok_or("The modifier hook was not active before the game resumed")?;
    let entered = single(records, |event| {
        matches!(event, WorkerEvent::ModifierDocumentationEntered)
    })
    .ok_or("The engine's modifier documentation was not entered exactly once")?;
    let table = single(records, |event| {
        matches!(event, WorkerEvent::ModifierTable { .. })
    })
    .ok_or("The modifier table was not read")?;
    let end = single(records, |event| {
        matches!(event, WorkerEvent::ModifierTableEnd { .. })
    })
    .ok_or("The modifier table terminal is missing")?;
    let (
        WorkerEvent::ModifierTable {
            count,
            bytes,
            sha256,
        },
        WorkerEvent::ModifierTableEnd {
            count: end_count,
            producer_last_sequence,
        },
    ) = (&table.event, &end.event)
    else {
        unreachable!("selected by kind");
    };
    if [entered.thread, table.thread, end.thread] != [Some(thread); 3]
        || !(resumed < entered.seq && entered.seq < table.seq && table.seq < end.seq)
        || *producer_last_sequence != end.seq
        || end_count != count
    {
        return Err("The modifier table witnesses disagree".into());
    }
    let continuous = records
        .iter()
        .take_while(|record| record.seq <= end.seq)
        .enumerate()
        .all(|(index, record)| record.seq == index as u64 + 1);
    if !continuous {
        return Err("Worker records before the modifier terminal are missing".into());
    }
    if records[..end.seq as usize].iter().any(|record| {
        matches!(
            record.event,
            WorkerEvent::CallbackError { .. } | WorkerEvent::NativeException { .. }
        )
    }) {
        return Err("The worker failed before the modifier terminal".into());
    }
    let file = file.ok_or("The modifier table file is missing")?;
    if file.len() as u64 != *bytes || crate::work_directory::sha256(file) != *sha256 {
        return Err("The modifier table file differs from the file that the worker wrote".into());
    }
    let table: ModifierTable = serde_json::from_slice(file)
        .map_err(|error| format!("The modifier table file is invalid: {error}"))?;
    if table.attempt != attempt || table.entries.len() as u64 != *count {
        return Err("The modifier table file belongs to another observation".into());
    }
    let names: BTreeSet<_> = table.entries.iter().map(|entry| &entry.name).collect();
    if names.len() != table.entries.len() {
        return Err("The modifier table names a modifier more than once".into());
    }
    if !table
        .registries
        .keys()
        .eq(registries.iter().collect::<BTreeSet<_>>())
    {
        return Err("The modifier table's registries differ from the request".into());
    }
    Ok(ObservedModifiers {
        entries: table.entries,
        registries: table.registries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ATTEMPT: &str = "a";
    const THREAD: u64 = 7;

    fn table() -> Vec<u8> {
        let table = ModifierTable {
            attempt: ATTEMPT.into(),
            entries: vec![
                ModifierEntry {
                    name: "pop_happiness".into(),
                    mask: 1,
                },
                ModifierEntry {
                    name: "planet_building_a_build_speed_mult".into(),
                    mask: 0x4000_0000,
                },
            ],
            registries: BTreeMap::from([(
                "common/buildings".into(),
                RegistryKeys::Keys(vec!["building_a".into()]),
            )]),
        };
        let mut bytes = serde_json::to_vec(&table).unwrap();
        bytes.push(b'\n');
        bytes
    }

    /// The worker's stream of a complete observation, as raw lines.
    fn stream(file: &[u8]) -> Vec<String> {
        let sha = crate::work_directory::sha256(file);
        let hooks =
            format!(r#"{{"{HOOK}":{{"enabled":true,"locations":1,"resolved":1,"hits":0}}}}"#);
        [
            r#""kind":"hooks-requested""#.to_owned(),
            format!(
                r#""kind":"launch-stopped","error":"success","pid":42,"triple":"arm64-apple-macosx","frames":[{{"function":"_dyld_start"}}],"thread":{THREAD}"#
            ),
            format!(r#""kind":"hooks-active-before-resume","hooks":{hooks}"#),
            r#""kind":"resume","error":"success""#.to_owned(),
            format!(r#""kind":"modifier-documentation-entered","thread":{THREAD}"#),
            format!(
                r#""kind":"modifier-table","count":2,"bytes":{},"sha256":"{sha}","thread":{THREAD}"#,
                file.len()
            ),
            format!(
                r#""kind":"modifier-table-end","count":2,"producerLastSequence":7,"thread":{THREAD}"#
            ),
            format!(r#""kind":"session-paused","returned":[],"cause":"content-loaded","thread":{THREAD}"#),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, body)| format!(r#"{{"seq":{},"run":"{ATTEMPT}",{body}}}"#, index + 1))
        .collect()
    }

    fn owner() -> Vec<OwnerEvent> {
        vec![OwnerEvent::GameOwnedSuspended {
            pid: 42,
            identity: "started".into(),
        }]
    }

    fn observe(lines: &[String], file: Option<&[u8]>) -> Result<ObservedModifiers, String> {
        let raw: String = lines.iter().map(|line| format!("{line}\n")).collect();
        let (records, _) = event_stream::read_worker_stream(raw.as_bytes(), ATTEMPT);
        reduce(
            &records,
            &owner(),
            file,
            ATTEMPT,
            &["common/buildings".into()],
        )
    }

    #[test]
    fn a_complete_stream_and_its_file_give_the_table() {
        let file = table();
        let observed = observe(&stream(&file), Some(&file)).unwrap();
        assert_eq!(observed.entries.len(), 2);
        assert_eq!(
            observed.registries["common/buildings"],
            RegistryKeys::Keys(vec!["building_a".into()])
        );
    }

    #[test]
    fn a_missing_or_damaged_terminal_gives_no_table() {
        let file = table();
        let mut lines = stream(&file);
        lines.remove(6);
        assert!(observe(&lines, Some(&file)).is_err());
        // Damage after the terminal removes the terminal as the stream is read.
        let mut damaged = stream(&file);
        damaged.push("{\"seq\":".into());
        let raw: String = damaged.iter().map(|line| format!("{line}\n")).collect();
        let (records, damage) = event_stream::read_worker_stream(raw.as_bytes(), ATTEMPT);
        assert!(damage.is_some());
        assert!(
            reduce(
                &records,
                &owner(),
                Some(&file),
                ATTEMPT,
                &["common/buildings".into()]
            )
            .is_err()
        );
    }

    #[test]
    fn a_hole_before_the_terminal_or_a_foreign_record_gives_no_table() {
        let file = table();
        // Only the first record is lost; every other witness is present.
        let mut hole = stream(&file);
        hole.remove(0);
        assert!(observe(&hole, Some(&file)).is_err());
        let mut foreign = stream(&file);
        foreign[3] = foreign[3].replace("\"run\":\"a\"", "\"run\":\"b\"");
        assert!(observe(&foreign, Some(&file)).is_err());
    }

    #[test]
    fn the_file_must_be_the_one_that_the_stream_names() {
        let file = table();
        let lines = stream(&file);
        assert!(observe(&lines, None).is_err());
        let mut changed = file.clone();
        changed[10] ^= 1;
        assert!(observe(&lines, Some(&changed)).is_err());
        let other = String::from_utf8(file.clone())
            .unwrap()
            .replace("\"attempt\":\"a\"", "\"attempt\":\"b\"")
            .into_bytes();
        assert!(observe(&stream(&other), Some(&other)).is_err());
    }

    #[test]
    fn a_repeated_name_or_an_unrequested_registry_gives_no_table() {
        let repeated = String::from_utf8(table())
            .unwrap()
            .replace("planet_building_a_build_speed_mult", "pop_happiness")
            .into_bytes();
        assert!(observe(&stream(&repeated), Some(&repeated)).is_err());
        let other = String::from_utf8(table())
            .unwrap()
            .replace("common/buildings", "common/zones")
            .into_bytes();
        assert!(observe(&stream(&other), Some(&other)).is_err());
    }

    #[test]
    fn a_worker_report_or_a_missing_hook_gives_its_reason() {
        let file = table();
        let mut lines = stream(&file);
        lines.insert(
            5,
            r#"{"seq":6,"run":"a","kind":"modifier-unavailable","reason":"lexer token lookup is not current"}"#
                .into(),
        );
        assert_eq!(
            observe(&lines, Some(&file)).unwrap_err(),
            "lexer token lookup is not current"
        );
        let mut late = stream(&file);
        late[2] = late[2].replace("\"hits\":0", "\"hits\":1");
        assert!(observe(&late, Some(&file)).is_err());
    }
}
