use super::*;
use crate::EvidenceReference;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct HistoricalRun {
    pub reference: DiscoveryRun,
    pub trace: Vec<Value>,
    pub table: Vec<Value>,
    pub result: Value,
    pub manifest: Value,
}
fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("")
}
fn address(value: &Value) -> Option<u64> {
    u64::from_str_radix(value.as_str()?.strip_prefix("0x")?, 16).ok()
}
fn witness(run: &HistoricalRun, row: &Value) -> EvidenceReference {
    EvidenceReference {
        artifact: run.reference.trace.clone(),
        record: row["seq"].as_u64(),
    }
}
fn gap(
    result: &mut RegistryDiscoveryResult,
    run: &HistoricalRun,
    row: &Value,
    subject: Option<RegistrySubject>,
    kind: DiscoveryGapKind,
    reason: &str,
) {
    result.gaps.push(DiscoveryGap {
        kind,
        subject,
        reason: reason.into(),
        evidence: witness(run, row),
    });
}
fn handle(
    result: &mut RegistryDiscoveryResult,
    objects: &mut BTreeMap<(String, String), RegistrySubject>,
    kind: &str,
    address: &str,
) -> RegistrySubject {
    objects
        .entry((kind.into(), address.into()))
        .or_insert_with(|| {
            let handle = RegistrySubject {
                scope: result.scope.clone(),
                ordinal: result.subject_count,
            };
            result.subject_count += 1;
            handle
        })
        .clone()
}
pub(super) fn append(
    result: &mut RegistryDiscoveryResult,
    input: &StaticInput,
    records: &[CandidateRecord],
    rows: &[SchedulerRow],
    runs: &[HistoricalRun],
) {
    let by_db: BTreeMap<_, _> = records
        .iter()
        .enumerate()
        .map(|(i, r)| (r.database.as_str(), i))
        .collect();
    let mut observed = BTreeSet::new();
    for run in runs {
        let first = run.trace.first().unwrap_or(&Value::Null);
        let activated = first["atDyldEntry"] == true
            && first["error"] == "success"
            && first["hooks"].as_object().is_some_and(|hooks| {
                !hooks.is_empty() && hooks.values().all(|h| h["resolved"] == 1 && h["hits"] == 0)
            });
        let sequence = run
            .trace
            .iter()
            .enumerate()
            .all(|(i, r)| r["seq"].as_u64() == Some(i as u64 + 1));
        let complete = run.trace.iter().any(|r| r["kind"] == "stream-end")
            && [
                "completed",
                "sequenceContiguous",
                "binaryUnchanged",
                "contentUnchanged",
            ]
            .iter()
            .all(|k| run.result[k] == true);
        let target = &run.manifest["target"];
        let matching = target["executableSha256"] == result.descriptor.provenance.executable
            && target["sliceSha256"] == result.descriptor.provenance.slice;
        if !activated || !sequence || !complete || !matching {
            gap(
                result,
                run,
                first,
                None,
                DiscoveryGapKind::HistoricalIntegrity,
                "historical activation, sequence, completion or exact target is not established",
            );
            continue;
        }
        let table_matches = run.table.len() == rows.len()
            && rows.iter().zip(&run.table).all(|(s, t)| {
                t["index"].as_u64() == Some(s.index as u64)
                    && t["name"].as_str() == s.name.as_deref()
                    && t["slots"].as_array().is_some_and(|slots| {
                        slots.len() == 5
                            && slots.iter().zip(&s.values[1..]).all(|(slot, value)| {
                                if *value == Some(0) {
                                    slot.is_null()
                                } else {
                                    address(&slot["address"]) == *value && value.is_some()
                                }
                            })
                    })
            });
        if !table_matches {
            gap(
                result,
                run,
                first,
                None,
                DiscoveryGapKind::HistoricalIntegrity,
                "retained startup table differs from static scheduling records",
            );
        }
        let mut objects = BTreeMap::new();
        let mut loaders: BTreeMap<String, (&Value, EvidenceReference)> = BTreeMap::new();
        let mut roots: BTreeMap<String, &Value> = BTreeMap::new();
        let mut enumerations: BTreeMap<String, &Value> = BTreeMap::new();
        let mut custom_phase = None;
        let mut key_phase = false;
        let mut custom_file: Option<&Value> = None;
        let mut custom_start: Option<&Value> = None;
        let mut custom_key_end: Option<&Value> = None;
        for event in &run.trace {
            let db = text(event, "database");
            let candidate = by_db.get(db).copied();
            let subject = candidate.map(|i| result.candidates[i].subject.clone());
            match text(event, "kind") {
                "vfs-end" => {
                    if event["count"].as_u64().is_some_and(|n| n > 0) {
                        enumerations.insert(text(event, "path").into(), event);
                    }
                }
                "load-file" => {
                    let directory = text(event, "directory");
                    let receiver = address(&event["receiver"]);
                    let valid = receiver.is_some_and(|a| a != 0)
                        && !directory.is_empty()
                        && text(event, "file").starts_with(&format!("{directory}/"))
                        && event["vtable"]["symbol"] == format!("vtable for {db}");
                    if !valid {
                        gap(
                            result,
                            run,
                            event,
                            subject,
                            DiscoveryGapKind::OwnerJoin,
                            "loader receiver, concrete database or directory join failed",
                        );
                        loaders.remove(db);
                        continue;
                    }
                    if let Some(i) = candidate {
                        observed.insert(i);
                        let evidence = witness(run, event);
                        loaders.insert(db.into(), (event, evidence.clone()));
                        let loader =
                            handle(result, &mut objects, "loader", text(event, "receiver"));
                        result.relationships.push(RegistryRelationship {
                            loader: Some(loader),
                            owner: None,
                            subject,
                            directory: Some(directory.into()),
                            key: None,
                            basis: DiscoveryBasis::HistoricalLoader,
                            evidence: vec![evidence],
                        });
                    }
                }
                "file-end" => {
                    loaders.remove(db);
                }
                "constructor-key" => {
                    if !text(event, "key").is_empty()
                        && address(&event["owner"]).is_some_and(|a| a != 0)
                    {
                        roots.insert(text(event, "owner").into(), event);
                    }
                }
                "owner-reader" => {
                    let Some(i) = candidate else {
                        gap(
                            result,
                            run,
                            event,
                            None,
                            DiscoveryGapKind::OwnerJoin,
                            "reader has no template candidate",
                        );
                        continue;
                    };
                    let root = roots.get(text(event, "top")).copied();
                    let loader = loaders.get(db);
                    let enumeration = loader
                        .and_then(|(loader, _)| {
                            enumerations.get(text(loader, "directory")).copied()
                        })
                        .filter(|event| {
                            let caller = text(&event["caller"], "symbol");
                            caller.contains(&format!("<{db},"))
                                || caller.contains(&format!("{db}::"))
                        });
                    let owner = &records[i].owner_candidate;
                    let vtable =
                        address(&event["vtable"]["address"]).and_then(|a| input.vtables.get(&a));
                    let adjusted = address(&event["receiver"])
                        .zip(vtable)
                        .and_then(|(a, v)| a.checked_add_signed(v.offset_to_top));
                    let valid = enumeration.is_some()
                        && root.is_some_and(|r| {
                            !text(event, "key").is_empty()
                                && r["key"] == event["key"]
                                && r["database"] == event["database"]
                                && r["file"] == event["file"]
                        })
                        && loader.is_some_and(|(r, _)| r["file"] == event["file"])
                        && adjusted.is_some()
                        && adjusted == address(&event["top"])
                        && event["vtable"]["symbol"] == format!("vtable for {owner}")
                        && vtable.is_some_and(|v| {
                            v.owner == *owner
                                && Some(v.member) == address(&event["memberFunction"]["address"])
                        })
                        && text(&event["memberFunction"], "symbol")
                            .ends_with(&format!("{owner}::ReadMember(CReader&, int)"));
                    if !valid {
                        gap(
                            result,
                            run,
                            event,
                            subject,
                            DiscoveryGapKind::OwnerJoin,
                            "reader occurrence lacks a matching root key, loader file, concrete owner, base adjustment or dispatch slot",
                        );
                        continue;
                    }
                    let (loader, loader_evidence) = loader.unwrap();
                    let loader_handle =
                        handle(result, &mut objects, "loader", text(loader, "receiver"));
                    let owner_handle = handle(result, &mut objects, "owner", text(event, "top"));
                    result.relationships.push(RegistryRelationship {
                        loader: Some(loader_handle),
                        owner: Some(owner_handle),
                        subject,
                        directory: Some(text(loader, "directory").into()),
                        key: Some(text(event, "key").into()),
                        basis: DiscoveryBasis::HistoricalOwner,
                        evidence: vec![
                            loader_evidence.clone(),
                            witness(run, enumeration.unwrap()),
                            witness(run, root.unwrap()),
                            witness(run, event),
                            EvidenceReference {
                                artifact: result.descriptor.input.clone(),
                                record: None,
                            },
                        ],
                    });
                }
                "custom-file" => {
                    custom_file = Some(event);
                }
                "custom-start" => {
                    custom_start = Some(event);
                    custom_file = None;
                    custom_phase = event["mode"].as_u64();
                    if custom_phase == Some(1) {
                        key_phase = true;
                    }
                }
                "custom-end" => {
                    if custom_phase == Some(1) && event["mode"] == 1 {
                        custom_key_end = Some(event);
                    }
                    custom_phase = None;
                }
                "custom-owner-reader" => {
                    let vtable =
                        address(&event["vtable"]["address"]).and_then(|a| input.vtables.get(&a));
                    let valid = key_phase
                        && custom_key_end.is_some()
                        && custom_file.is_some_and(|file| file["file"] == event["file"])
                        && text(&event["readFunction"], "symbol").starts_with("CPdxModifier<")
                        && custom_phase == Some(0)
                        && !text(event, "key").is_empty()
                        && text(event, "file").starts_with("common/static_modifiers/")
                        && address(&event["owner"]).is_some_and(|a| a != 0)
                        && event["vtable"]["symbol"] == "vtable for CStaticModifier"
                        && vtable.is_some_and(|v| {
                            v.owner == "CStaticModifier"
                                && Some(v.member) == address(&event["memberFunction"]["address"])
                        })
                        && event["memberFunction"]["symbol"]
                            == "CStaticModifier::ReadMember(CReader&, int)";
                    if valid {
                        let owner_handle =
                            handle(result, &mut objects, "owner", text(event, "owner"));
                        result.relationships.push(RegistryRelationship {
                            loader: None,
                            owner: Some(owner_handle),
                            subject: None,
                            directory: Some("common/static_modifiers".into()),
                            key: Some(text(event, "key").into()),
                            basis: DiscoveryBasis::HistoricalCustomOwner,
                            evidence: vec![
                                witness(run, custom_key_end.unwrap()),
                                witness(run, custom_start.unwrap()),
                                witness(run, custom_file.unwrap()),
                                witness(run, event),
                                EvidenceReference {
                                    artifact: result.descriptor.input.clone(),
                                    record: None,
                                },
                            ],
                        });
                    } else {
                        gap(
                            result,
                            run,
                            event,
                            None,
                            DiscoveryGapKind::OwnerJoin,
                            "custom owner, read phase or shared dispatch join failed",
                        );
                    }
                }
                "callback-error" | "missing-hook" | "exception" => {
                    gap(
                        result,
                        run,
                        event,
                        None,
                        DiscoveryGapKind::HistoricalIntegrity,
                        "retained observer reported failure",
                    );
                }
                _ => {}
            }
        }
    }
    for (i, candidate) in result.candidates.iter().enumerate() {
        if !observed.contains(&i) {
            result.gaps.push(DiscoveryGap {
                kind: DiscoveryGapKind::UnobservedCandidate,
                subject: Some(candidate.subject.clone()),
                reason: if runs.is_empty() {
                    "no live ownership evidence supplied"
                } else {
                    "candidate loader was not observed in the retained qualified window"
                }
                .into(),
                evidence: candidate.evidence.clone(),
            });
        }
    }
}
