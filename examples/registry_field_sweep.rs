//! Baseline of registry fields and reader kinds on one supported executable.
//! Run unchanged across the inventory and redirect stdout to a development report.
use pdx_native::{Completeness, GapKind, Native, ReaderKind};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::Instant;

struct ReaderFields {
    kind: ReaderKind,
    fields: Vec<String>,
    registries: BTreeMap<String, Completeness>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let installation = std::env::args_os()
        .nth(1)
        .ok_or("usage: registry_field_sweep <installation>")?;
    let native = Native::open(installation)?;
    let started = Instant::now();
    let registries = native.registries()?;
    let mut cases = Vec::with_capacity(registries.value.len());
    let mut reader_fields: BTreeMap<String, ReaderFields> = BTreeMap::new();
    let mut complete = 0;
    let mut partial = 0;
    let mut failure = 0;
    let mut unresolved = 0;
    let mut fields_found = 0;
    let mut unresolved_paths = 0;
    let mut known_identities = 0;
    let mut unknown_identities = 0;
    let mut known_kinds = 0;
    let mut unknown_kinds = 0;
    let mut field_method: Option<String> = None;
    let mut fields_by_kind: BTreeMap<ReaderKind, usize> = BTreeMap::new();
    let mut missing_reader_fields = Vec::new();
    let mut failure_shapes: BTreeMap<String, Vec<Value>> = BTreeMap::new();

    for registry in &registries.value {
        let query_started = Instant::now();
        match native.registry_fields(&registry.name) {
            Ok(answer) => {
                match &field_method {
                    Some(method) if method != &answer.source.method => {
                        return Err("Registry field methods differ within one sweep".into());
                    }
                    None => field_method = Some(answer.source.method.clone()),
                    _ => {}
                }
                match answer.completeness {
                    Completeness::Complete => complete += 1,
                    Completeness::Partial => partial += 1,
                }
                for gap in &answer.gaps {
                    if gap.kind != GapKind::OutsideMethod {
                        failure_shapes
                            .entry(gap.detail.clone())
                            .or_default()
                            .push(json!({
                                "registry": registry.name,
                                "kind": gap.kind,
                                "subject": gap.subject,
                            }));
                    }
                }
                let path_gaps = answer
                    .gaps
                    .iter()
                    .filter(|gap| gap.kind == GapKind::UnresolvedPath)
                    .count();
                unresolved_paths += path_gaps;
                if path_gaps > 0 {
                    unresolved += 1;
                }
                fields_found += answer.value.len();
                for field in &answer.value {
                    let name = format!("{}#{}", registry.name, field.name);
                    *fields_by_kind.entry(field.reader.kind).or_default() += 1;
                    if let Some(identity) = &field.reader.id {
                        known_identities += 1;
                        let id = serde_json::to_value(identity)?.as_str().unwrap().to_owned();
                        let reader = reader_fields.entry(id).or_insert_with(|| ReaderFields {
                            kind: field.reader.kind,
                            fields: Vec::new(),
                            registries: BTreeMap::new(),
                        });
                        reader
                            .registries
                            .insert(registry.name.clone(), answer.completeness);
                        reader.fields.push(name);
                    } else {
                        unknown_identities += 1;
                        missing_reader_fields.push(name);
                    }
                    if field.reader.kind == ReaderKind::Unknown {
                        unknown_kinds += 1;
                    } else {
                        known_kinds += 1;
                    }
                }
                cases.push(json!({
                    "registry": registry.name,
                    "elapsed_ms": query_started.elapsed().as_millis(),
                    "answer": answer,
                }));
            }
            Err(error) => {
                failure += 1;
                failure_shapes
                    .entry(error.to_string())
                    .or_default()
                    .push(json!({
                        "registry": registry.name,
                        "error": error,
                    }));
                cases.push(json!({
                    "registry": registry.name,
                    "elapsed_ms": query_started.elapsed().as_millis(),
                    "error": error,
                }));
            }
        }
    }
    let readers: BTreeMap<_, Value> = reader_fields
        .into_iter()
        .map(|(id, reader)| {
            let complete = reader
                .registries
                .values()
                .filter(|status| **status == Completeness::Complete)
                .count();
            let report = json!({
                "kind": reader.kind,
                "count": reader.fields.len(),
                "fields": reader.fields,
                "registry_answers": {
                    "complete": complete,
                    "partial": reader.registries.len() - complete,
                    "failed": 0,
                },
            });
            (id, report)
        })
        .collect();
    let field_method = field_method.ok_or("No registry field method was established")?;
    let output = json!({
        "method": field_method,
        "build": native.build(),
        "elapsed_ms": started.elapsed().as_millis(),
        "registry_count": registries.value.len(),
        "registry_inventory": registries,
        "summary": {
            "complete_queries": complete,
            "partial_queries": partial,
            "failed_queries": failure,
            "queries_with_unresolved_paths": unresolved,
            "fields_found": fields_found,
            "unresolved_paths": unresolved_paths,
            "known_reader_identities": known_identities,
            "unknown_reader_identities": unknown_identities,
            "known_reader_kinds": known_kinds,
            "unknown_reader_kinds": unknown_kinds,
            "distinct_known_readers": readers.len(),
        },
        "readers": readers,
        "fields_by_reader_kind": fields_by_kind,
        "fields_without_reader_identity": missing_reader_fields,
        "failure_shapes": failure_shapes,
        "report_limits": {
            "failure_shapes": "Grouped by current public gap detail; internal stop diagnostics await SDK-581/SDK-588.",
            "reader_registry_answers": "Completeness of registry answers containing this reader, not completeness of the reader's full semantics. Failed queries cannot be assigned to a reader.",
        },
        "cases": cases,
    });
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
