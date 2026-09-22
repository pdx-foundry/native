//! Frozen-method sweep for the milestone 2 registry-field review.
//! Run with the exact supported executable and redirect stdout to a retained JSON file.
use pdx_native::{GapKind, Native, ReaderKind};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let installation = std::env::args_os()
        .nth(1)
        .ok_or("usage: registry_field_sweep <installation>")?;
    let native = Native::open(installation)?;
    let started = Instant::now();
    let registries = native.registries()?;
    let mut cases = Vec::with_capacity(registries.value.len());
    let mut reader_fields: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut success = 0;
    let mut failure = 0;
    let mut unresolved = 0;
    let mut fields_found = 0;
    let mut unresolved_paths = 0;
    let mut known_identities = 0;
    let mut unknown_identities = 0;
    let mut known_kinds = 0;
    let mut unknown_kinds = 0;
    let mut field_method: Option<String> = None;

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
                success += 1;
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
                    if let Some(identity) = &field.reader.id {
                        known_identities += 1;
                        reader_fields
                            .entry(serde_json::to_value(identity)?.as_str().unwrap().into())
                            .or_default()
                            .push(name);
                    } else {
                        unknown_identities += 1;
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
        .map(|(id, fields)| (id, json!({ "count": fields.len(), "fields": fields })))
        .collect();
    let field_method = field_method.ok_or("No registry field method was established")?;
    let output = json!({
        "method": field_method,
        "build": native.build(),
        "elapsed_ms": started.elapsed().as_millis(),
        "registry_count": registries.value.len(),
        "summary": {
            "successful_queries": success,
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
        "cases": cases,
    });
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
