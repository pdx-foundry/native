//! Report every numeric registry field and its shared conversion, including unresolved joins.
use pdx_native::internals::{numeric_readers, registry_field_stops};
use pdx_native::{GapKind, GapSubject, GrammarProperty, Native, ReaderKind};
use serde_json::json;
use std::collections::BTreeMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let native = Native::open(std::env::var_os("STELLARIS_PATH").ok_or("set STELLARIS_PATH")?)?;
    let facts = numeric_readers::run(&native)?;
    let mut fields = Vec::new();
    let mut failures = Vec::new();
    let mut counts = BTreeMap::<&str, usize>::new();
    let registries = native.registries()?;
    for registry in &registries.value {
        let run = match registry_field_stops::run(&native, &registry.name) {
            Ok(run) => run,
            Err(error) => {
                failures.push(json!({"registry": registry.name, "error": error}));
                continue;
            }
        };
        for field in &run.answer.value {
            let raw = run
                .result
                .fields
                .iter()
                .find(|candidate| candidate.name == field.name);
            let callees: Vec<_> = raw
                .into_iter()
                .flat_map(|candidate| &candidate.readers)
                .filter_map(|join| match join {
                    registry_field_stops::ReaderJoin::Joined { callee, .. }
                        if facts.readers.contains_key(callee) =>
                    {
                        Some(callee.as_str())
                    }
                    _ => None,
                })
                .collect();
            if callees.is_empty()
                && !matches!(
                    field.reader.kind,
                    ReaderKind::Integer | ReaderKind::FixedPoint | ReaderKind::Float
                )
            {
                continue;
            }
            // The conversion keeps its `Partial` wrapper by design; a typed gap on the field, not
            // the wrapper, makes the answer partial.
            let subject = Some(GapSubject::Field {
                name: field.name.clone(),
            });
            let limited = run
                .answer
                .gaps
                .iter()
                .any(|gap| gap.kind == GapKind::NumericConversion && gap.subject == subject);
            let status = match &field.reader.numeric {
                GrammarProperty::Known(Some(_)) | GrammarProperty::Partial(Some(_)) if !limited => {
                    "complete"
                }
                GrammarProperty::Known(Some(_)) | GrammarProperty::Partial(Some(_)) => "partial",
                _ => "failed",
            };
            *counts.entry(status).or_default() += 1;
            let reasons: Vec<_> = callees
                .iter()
                .flat_map(|callee| &facts.readers[*callee].gaps)
                .map(|gap| gap.reason)
                .collect();
            let boundary: Vec<_> = callees
                .iter()
                .flat_map(|callee| &facts.readers[*callee].boundary)
                .map(|gap| gap.reason)
                .collect();
            fields.push(json!({"registry": registry.name, "field": field.name,
                "status": status, "reader": field.reader, "callees": callees, "reasons": reasons,
                "boundary": boundary}));
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({"build": native.build(),
        "registries": registries.value.len(), "registry_completeness": registries.completeness,
        "counts": counts, "failures": failures, "facts": facts, "fields": fields}))?
    );
    Ok(())
}
