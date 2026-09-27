//! Unfiltered command grammar answers and developer diagnostics for one supported build.
//! Counts are operation answers per unique (family, name), never paths or registration sites.
use pdx_native::internals::command_grammar_stops::{self, Chain, GrammarResult, Run};
use pdx_native::internals::inspect::{Image, read_image};
use pdx_native::internals::reference_readers::{self, Initialization, ReferenceFacts};
use pdx_native::internals::registry_field_stops::Stop;
use pdx_native::{
    Answer, CommandGrammar, Completeness, DeclarationKind, EmptyKey, FieldReference,
    GrammarProperty, KeyMatch, LookupStage, MissingResult, Native, ReferenceTarget,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let output = match args.as_slice() {
        [flag, before, after] if flag == "--diff" => {
            let before = serde_json::from_slice(&std::fs::read(before)?)?;
            let after = serde_json::from_slice(&std::fs::read(after)?)?;
            diff(&before, &after)
        }
        [installation] => population(installation)?,
        _ => return Err("usage: command-population INSTALLATION | --diff BEFORE AFTER".into()),
    };
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

fn population(installation: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let native = Native::open(installation)?;
    let bytes = read_image(installation.as_ref())?;
    let image = Image::read(&bytes)?;
    let started = std::time::Instant::now();
    let references = reference_readers::run(&native)?;
    let mut inventories = Vec::new();
    for kind in [DeclarationKind::Effect, DeclarationKind::Trigger] {
        let mut report = Report::default();
        let mut initialization = InitializationTally::default();
        let population = command_grammar_stops::population(&native, kind, |name, run| {
            let diagnostics = diagnostics(&image, &run);
            initialization.add(name, &run, &references);
            report.add(name, run.answer, &run.chain, diagnostics);
        })?;
        inventories.push(json!({
            "kind": kind,
            "denominator": "unique named command operation answers; unknown observations are separate",
            "full_denominator_known": population.unknown_registrations.is_empty() && population.inventory_gaps.is_empty(),
            "unknown_registrations": population.unknown_registrations,
            "inventory_gaps": population.inventory_gaps,
            "totals": report.totals,
            "failure_shapes": report.failure_shapes,
            "stop_groups": report.stop_groups,
            "initialization_lookups": initialization.report(),
            "cases": report.cases,
        }));
    }
    Ok(
        json!({"build": native.build(), "elapsed_ms": started.elapsed().as_millis(), "inventories": inventories}),
    )
}

#[derive(Default, serde::Serialize)]
struct Totals {
    named_commands: usize,
    complete: usize,
    partial: usize,
    // Input errors abort the inventory; individual named runs always return an answer.
    failed: usize,
    receiver_join_failed: usize,
}

#[derive(Default)]
struct Report {
    totals: Totals,
    failure_shapes: BTreeMap<String, BTreeSet<String>>,
    stop_groups: BTreeMap<String, BTreeSet<String>>,
    cases: Vec<Value>,
}

impl Report {
    fn add(
        &mut self,
        name: &str,
        answer: Answer<CommandGrammar>,
        chain: &Chain,
        diagnostics: Value,
    ) {
        self.totals.named_commands += 1;
        if let Some(groups) = diagnostics["stop_groups"].as_object() {
            for shape in groups.keys() {
                self.stop_groups
                    .entry(shape.clone())
                    .or_default()
                    .insert(name.into());
            }
        }
        let status = match answer.completeness {
            Completeness::Complete => {
                self.totals.complete += 1;
                "complete"
            }
            Completeness::Partial => {
                self.totals.partial += 1;
                "partial"
            }
        };
        if chain.receiver.is_none() || chain.stopped_at == Some("reader slots and bodies") {
            self.totals.receiver_join_failed += 1;
        }
        for gap in &answer.gaps {
            self.failure_shapes
                .entry(gap.detail.clone())
                .or_default()
                .insert(name.into());
        }
        self.cases.push(
            json!({"name": name, "status": status, "answer": answer, "diagnostics": diagnostics}),
        );
    }
}

/// Each named command by what its receiver's initializer establishes. A lookup joins a child key
/// when that key's answer holds an owner-initialization lookup; complete when every property of
/// that lookup is established.
#[derive(Default)]
struct InitializationTally {
    without_lookup: usize,
    complete: Vec<String>,
    partial: Vec<String>,
    without_authored_field: Vec<String>,
    failed: BTreeMap<&'static str, Vec<String>>,
    unknown_initializer: Vec<String>,
}

impl InitializationTally {
    fn add(&mut self, name: &str, run: &Run, references: &ReferenceFacts) {
        let Ok(result) = &run.result else {
            self.failed
                .entry("receiver-join")
                .or_default()
                .push(name.into());
            return;
        };
        let initialization = result
            .initializer
            .as_ref()
            .ok()
            .and_then(|initializer| references.initializers.get(initializer));
        match initialization {
            None => self.unknown_initializer.push(name.into()),
            Some(Initialization::NoLookup) => self.without_lookup += 1,
            Some(Initialization::Unresolved(stop)) => self
                .failed
                .entry(stop.reason)
                .or_default()
                .push(name.into()),
            Some(Initialization::Lookup(_)) => match joined_lookup(&run.answer) {
                Some(true) => self.complete.push(name.into()),
                Some(false) => self.partial.push(name.into()),
                None => self.without_authored_field.push(name.into()),
            },
        }
    }

    fn report(self) -> Value {
        let failed: usize = self.failed.values().map(Vec::len).sum();

        json!({
            "without_lookup": self.without_lookup,
            "joined_complete": self.complete.len(),
            "joined_partial": self.partial.len(),
            "without_authored_field": self.without_authored_field.len(),
            "failed": failed,
            "unknown_initializer": self.unknown_initializer.len(),
            "complete_commands": self.complete,
            "partial_commands": self.partial,
            "commands_without_authored_field": self.without_authored_field,
            "failure_shapes": self.failed,
            "unknown_initializer_commands": self.unknown_initializer,
        })
    }
}

/// Whether the owner-initialization lookup that a child key joins is fully established, or
/// `None` when no child key joins one.
fn joined_lookup(answer: &Answer<CommandGrammar>) -> Option<bool> {
    let GrammarProperty::Partial(keys) = &answer.value.fixed_keys else {
        return None;
    };
    let lookups: Vec<_> = keys
        .iter()
        .filter_map(|key| match &key.reference {
            FieldReference::Lookups(lookups) => Some(lookups),
            _ => None,
        })
        .flatten()
        .filter(|lookup| lookup.stage == LookupStage::OwnerInitialization)
        .collect();
    if lookups.is_empty() {
        return None;
    }

    Some(lookups.iter().all(|lookup| {
        matches!(lookup.target, ReferenceTarget::Registry { .. })
            && lookup.key_match != KeyMatch::Unresolved
            && lookup.empty_key != EmptyKey::Unresolved
            && lookup.on_missing != MissingResult::Unresolved
    }))
}

/// Internal failures retain locations and group by reason, instruction, obstacle and function.
fn diagnostics(image: &Image, run: &Run) -> Value {
    let mut stops = Vec::new();
    let mut delegates = Vec::new();
    match &run.result {
        Ok(result) => grammar_diagnostics(image, result, "root", &mut stops, &mut delegates),
        Err(unresolved) => stops.push(stop_case(
            image,
            run.chain.stopped_at.unwrap_or("grammar"),
            unresolved.reason,
            unresolved.stop,
        )),
    }
    let mut groups = BTreeMap::<String, usize>::new();
    for stop in &stops {
        let key = json!({"reason": stop["reason"], "operation": stop["operation"], "obstacle": stop["obstacle"], "function": stop["function"]}).to_string();
        *groups.entry(key).or_default() += 1;
    }
    json!({"chain": run.chain, "delegates": delegates, "stops": stops, "stop_groups": groups})
}

fn grammar_diagnostics(
    image: &Image,
    result: &GrammarResult,
    path: &str,
    stops: &mut Vec<Value>,
    delegates: &mut Vec<Value>,
) {
    delegates.push(json!({"path": path, "reader": result.reader_name, "member": result.member_name, "calls": result.delegates}));
    for unresolved in &result.stops {
        stops.push(stop_case(image, path, unresolved.reason, unresolved.stop));
    }
    for gap in &result.fields.gaps {
        let stop = stop_case(image, path, &gap.reason, gap.stop);
        if !stops.contains(&stop) {
            stops.push(stop);
        }
    }
    if let Some(numeric) = &result.numeric {
        grammar_diagnostics(image, numeric, &format!("{path}.numeric"), stops, delegates);
    }
}

fn stop_case(image: &Image, stage: &str, reason: &str, stop: Option<Stop>) -> Value {
    match stop {
        Some(stop) => {
            let placed = image.place_stop(stop);
            json!({"stage": stage, "reason": reason, "stop": stop, "function": placed.function,
                "operation": placed.row.map(|row| row.operation), "obstacle": stop.obstacle})
        }
        None => json!({"stage": stage, "reason": reason, "location": "no instruction located"}),
    }
}

/// Ignore timing and addresses. Include inventory uncertainty as well as answer and status changes.
fn normalized(report: &Value) -> BTreeMap<String, Value> {
    let mut result = BTreeMap::new();
    for inventory in report["inventories"].as_array().into_iter().flatten() {
        let kind = inventory["kind"].as_str().unwrap_or_default();
        let mut unknown_reasons: Vec<_> = inventory["unknown_registrations"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|entry| entry["reason"].clone())
            .collect();
        unknown_reasons.sort_by_key(Value::to_string);
        result.insert(format!("{kind}/inventory"), json!({"gaps": inventory["inventory_gaps"], "unknown_reasons": unknown_reasons, "full_denominator_known": inventory["full_denominator_known"]}));
        for case in inventory["cases"].as_array().into_iter().flatten() {
            let name = case["name"].as_str().unwrap_or_default();
            result.insert(
                format!("{kind}/command/{name}"),
                json!({"status": case["status"], "answer": case["answer"], "error": case["error"]}),
            );
        }
    }
    result
}

fn diff(before: &Value, after: &Value) -> Value {
    let before = normalized(before);
    let after = normalized(after);
    let keys: BTreeSet<_> = before.keys().chain(after.keys()).collect();
    let changes: Vec<_> = keys
        .into_iter()
        .filter(|key| before.get(*key) != after.get(*key))
        .map(|key| json!({"subject": key, "before": before.get(key), "after": after.get(key)}))
        .collect();
    json!({"changed": changes.len(), "changes": changes})
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdx_native::{
        Basis, BlockFamily, BuildId, Gap, GapKind, GrammarProperty, Reader, ReaderId, ReaderKind,
        Source,
    };

    fn answer(completeness: Completeness, joined: bool) -> Answer<CommandGrammar> {
        let gaps = if completeness == Completeness::Partial {
            vec![Gap {
                kind: GapKind::UnresolvedReader,
                subject: None,
                detail: "factory-return".into(),
            }]
        } else {
            vec![]
        };
        Answer {
            value: CommandGrammar {
                reader: Reader {
                    id: joined.then(|| {
                        serde_json::from_value::<ReaderId>(json!("authored-reader")).unwrap()
                    }),
                    kind: ReaderKind::Unknown,
                    family: BlockFamily::Unknown,
                },
                child_families: GrammarProperty::Unresolved,
                fixed_keys: GrammarProperty::Unresolved,
                numeric_keys: GrammarProperty::Unresolved,
                ordering: GrammarProperty::Unresolved,
            },
            completeness,
            gaps,
            source: Source {
                build: serde_json::from_value::<BuildId>(json!("authored-build")).unwrap(),
                native_version: "test".into(),
                method: "authored".into(),
                basis: Basis::StaticAnalysis,
            },
        }
    }

    fn joined_chain() -> Chain {
        Chain {
            receiver: Some(1),
            read: Some(2),
            member: Some(3),
            ..Chain::default()
        }
    }

    #[test]
    fn grammar_name_failures_do_not_count_as_receiver_failures() {
        let mut report = Report::default();
        for reason in ["command-member-name", "command-reader-name"] {
            let mut answer = answer(Completeness::Partial, false);
            answer.gaps[0].detail = reason.into();
            let chain = Chain {
                stopped_at: Some("grammar"),
                ..joined_chain()
            };
            report.add(reason, answer, &chain, Value::Null);
        }
        assert_eq!(report.totals.receiver_join_failed, 0);
        assert_eq!(report.totals.partial, 2);
        assert!(
            report
                .cases
                .iter()
                .all(|case| case["answer"]["value"]["reader"]["id"].is_null())
        );

        let missing_body = Chain {
            stopped_at: Some("reader slots and bodies"),
            ..joined_chain()
        };
        report.add(
            "missing-body",
            answer(Completeness::Partial, false),
            &missing_body,
            Value::Null,
        );
        assert_eq!(report.totals.receiver_join_failed, 1);
        let missing_receiver = Chain {
            stopped_at: Some("factory receiver"),
            ..Chain::default()
        };
        report.add(
            "missing-receiver",
            answer(Completeness::Partial, false),
            &missing_receiver,
            Value::Null,
        );
        assert_eq!(report.totals.receiver_join_failed, 2);
    }

    fn report(cases: Vec<Value>) -> Value {
        json!({"inventories": [{"kind": "Effect", "unknown_registrations": [], "inventory_gaps": [], "full_denominator_known": true, "cases": cases}]})
    }

    #[test]
    fn mixed_answers_keep_failed_joins_in_totals_and_group_each_name_once() {
        let mut report = Report::default();
        report.add(
            "complete",
            answer(Completeness::Complete, true),
            &joined_chain(),
            Value::Null,
        );
        let mut partial = answer(Completeness::Partial, false);
        partial.gaps.push(partial.gaps[0].clone());
        report.add(
            "unresolved",
            partial,
            &Chain {
                stopped_at: Some("factory receiver"),
                ..Chain::default()
            },
            json!({"stop_groups": {"factory-return/no-instruction": 2}}),
        );
        report.add(
            "partial",
            answer(Completeness::Partial, true),
            &joined_chain(),
            Value::Null,
        );
        assert_eq!(
            (
                report.totals.named_commands,
                report.totals.complete,
                report.totals.partial,
                report.totals.failed,
                report.totals.receiver_join_failed
            ),
            (3, 1, 2, 0, 1)
        );
        assert_eq!(
            report.failure_shapes["factory-return"],
            BTreeSet::from(["partial".into(), "unresolved".into()])
        );
        assert_eq!(
            report.stop_groups["factory-return/no-instruction"],
            BTreeSet::from(["unresolved".into()])
        );
        assert_eq!(
            report.cases[1]["answer"]["value"]["fixed_keys"],
            "Unresolved"
        );
        assert_eq!(report.cases[2]["status"], "partial");
    }

    #[test]
    fn normalized_diff_ignores_diagnostics_but_detects_answers_gaps_status_and_inventory() {
        let before = report(vec![
            json!({"name": "sample", "status": "partial", "answer": answer(Completeness::Partial, false)}),
        ]);
        let mut after = before.clone();
        after["elapsed_ms"] = json!(100);
        after["inventories"][0]["cases"][0]["diagnostics"] = json!({"address": 123});
        assert_eq!(diff(&before, &after)["changed"], 0);
        for (field, value) in [
            ("status", json!("failed")),
            ("answer", json!(answer(Completeness::Complete, true))),
            ("error", json!("failure")),
        ] {
            let mut changed = after.clone();
            changed["inventories"][0]["cases"][0][field] = value;
            assert_eq!(diff(&before, &changed)["changed"], 1);
        }
        after["inventories"][0]["cases"][0]["answer"]["gaps"][0]["detail"] = json!("another stop");
        assert_eq!(diff(&before, &after)["changed"], 1);
        let mut unknown = before.clone();
        unknown["inventories"][0]["unknown_registrations"] =
            json!([{"reason": "runtime token", "instruction": 1}]);
        unknown["inventories"][0]["full_denominator_known"] = json!(false);
        assert_eq!(diff(&before, &unknown)["changed"], 1);
        let empty = report(vec![]);
        assert_eq!(diff(&before, &empty)["changed"], 1);
        assert_eq!(diff(&empty, &before)["changed"], 1);
    }
}
