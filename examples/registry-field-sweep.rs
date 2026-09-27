//! Baseline of registry fields and reader kinds on one supported executable.
//! Run unchanged across the inventory and redirect stdout to a development report.
//!
//! `registry-field-sweep INSTALLATION` writes the report. It runs the method once per registry and
//! derives the public answer from that run, as `Native::registry_fields` does. Failures are grouped
//! twice: by public gap detail, and by the method's internal stop (instruction kind and obstacle,
//! then function). Addresses in the stop groups are for development only.
//!
//! `registry-field-sweep --diff BEFORE AFTER` compares the normalized answers of two reports and
//! writes the registries whose answer changed. Two runs on the same build give an empty diff.
#[path = "support/population.rs"]
mod population;

use pdx_native::internals::inspect::{Image, read_image};
use pdx_native::internals::reference_readers;
use pdx_native::internals::registry_field_stops::{self, FieldGap};
use pdx_native::{
    Answer, BuildId, Completeness, EmptyKey, Error, Field, FieldReadOutcome, FieldReference,
    GapKind, GapSubject, KeyMatch, LookupStage, MissingResult, Native, ReaderKind, ReferenceTarget,
    Registry,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

const USAGE: &str = "usage: registry-field-sweep [--baseline] INSTALLATION | registry-field-sweep --diff BEFORE AFTER";

struct ReaderFields {
    kind: ReaderKind,
    fields: Vec<String>,
    registries: BTreeMap<String, Completeness>,
}

/// One internal gap of one registry, placed in the image.
struct StopCase {
    registry: String,
    gap: FieldGap,
    /// The text symbol that holds the stop instruction.
    function: Option<String>,
    /// The mnemonic of the stop instruction.
    operation: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let output = match arguments.as_slice() {
        [flag, before, after] if flag == "--diff" => {
            diff(&read_report(before)?, &read_report(after)?)
        }
        [installation] => sweep(installation)?,
        [flag, installation] if flag == "--baseline" => sweep(installation)?,
        _ => return Err(USAGE.into()),
    };
    if arguments.first().is_some_and(|flag| flag == "--baseline") {
        println!(
            "{}",
            population::format_baseline(&output["build"], &normalized_cases(&output))?
        );
        return Ok(());
    }
    if let Some(members) = output.get("defaulted_members").and_then(Value::as_array)
        && !members.is_empty()
    {
        eprintln!(
            "ignored serde-default members: {}",
            members
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

fn read_report(path: &str) -> Result<Value, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

fn sweep(installation: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let native = Native::open(installation)?;
    let bytes = read_image(installation.as_ref())?;
    let image = Image::read(&bytes)?;
    let started = Instant::now();
    let registries = native.registries()?;
    let mut report = SweepReport::default();

    for registry in &registries.value {
        let query_started = Instant::now();
        let run = registry_field_stops::run(&native, &registry.name);
        let elapsed_ms = query_started.elapsed().as_millis();

        match run {
            Ok(registry_field_stops::Run { answer, result }) => {
                let stops = place_gaps(&image, &registry.name, result.gaps);
                report.add_answer(&registry.name, answer, stops, elapsed_ms)?;
            }
            Err(error) => report.add_failure(&registry.name, error, elapsed_ms),
        }
    }

    let readers = reader_census(&native)?;
    report.into_json(
        native.build(),
        started.elapsed().as_millis(),
        registries,
        readers,
    )
}

/// Every reference reader in the executable, counted by form and by what the method established.
fn reader_census(native: &Native) -> Result<Value, Box<dyn std::error::Error>> {
    let facts = reference_readers::run(native)?;
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut unresolved: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (callee, fact) in &facts.readers {
        let form = reference_readers::reader(callee).map(|reader| reader.form);
        let status = match (&fact.lookup, &fact.directory) {
            (Ok(lookup), Some(_)) if lookup.key_match.is_some() => "complete".to_owned(),
            (Ok(_), _) => "partial".to_owned(),
            (Err(stop), _) => {
                unresolved
                    .entry(stop.reason.to_owned())
                    .or_default()
                    .push(fact.database.clone());
                "failed".to_owned()
            }
        };
        *counts.entry(format!("{form:?}.{status}")).or_default() += 1;
        if fact.directory.is_none() {
            unresolved
                .entry("no-content-directory".into())
                .or_default()
                .push(fact.database.clone());
        }
    }

    Ok(json!({
        "readers": facts.readers.len(),
        "by_form_and_status": counts,
        "unresolved": unresolved,
        "initializers": initializer_census(&facts),
    }))
}

/// Every owner initializer in the executable, by what the method established about its lookup.
/// Complete: a qualified shape, a content directory and a key match. Failed: the initializer
/// searches a database, but no qualified shape established the lookup.
fn initializer_census(facts: &reference_readers::ReferenceFacts) -> Value {
    use reference_readers::Initialization;

    let mut no_lookup = 0;
    let mut complete = Vec::new();
    let mut partial: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut failed: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (name, initialization) in &facts.initializers {
        match initialization {
            Initialization::NoLookup => no_lookup += 1,
            Initialization::Lookup(found) => match (&found.directory, found.lookup.key_match) {
                (Some(_), Some(_)) => complete.push(name.as_str()),
                (None, _) => partial
                    .entry("no-content-directory")
                    .or_default()
                    .push(name),
                (Some(_), None) => partial
                    .entry("search-not-qualified")
                    .or_default()
                    .push(name),
            },
            Initialization::Unresolved(stop) => failed.entry(stop.reason).or_default().push(name),
        }
    }
    let count = |groups: &BTreeMap<&str, Vec<&str>>| groups.values().map(Vec::len).sum::<usize>();

    json!({
        "initializers": facts.initializers.len(),
        "without_lookup": no_lookup,
        "complete": complete.len(),
        "partial": count(&partial),
        "failed": count(&failed),
        "complete_initializers": complete,
        "partial_shapes": partial,
        "failure_shapes": failed,
    })
}

/// The report state gathered from each registry's query, in registry order.
#[derive(Default)]
struct SweepReport {
    /// The one method that gave every answer. A failed query has no method.
    field_method: Option<String>,
    complete_queries: usize,
    partial_queries: usize,
    failed_queries: usize,
    queries_with_unresolved_paths: usize,
    unresolved_paths: usize,
    /// Fields with a reader identity, by that identity.
    readers: BTreeMap<String, ReaderFields>,
    fields_by_reader_kind: BTreeMap<ReaderKind, usize>,
    field_shapes: BTreeMap<String, usize>,
    field_families: BTreeMap<String, usize>,
    fields_without_reader_identity: Vec<String>,
    failure_shapes: BTreeMap<String, Vec<Value>>,
    references: ReferenceTally,
    stop_cases: Vec<StopCase>,
    cases: Vec<Value>,
}

impl SweepReport {
    /// Add one registry's answer and its placed internal gaps. Fails when the answer's method
    /// differs from an earlier answer's.
    fn add_answer(
        &mut self,
        registry: &str,
        answer: Answer<Vec<Field>>,
        stops: Vec<StopCase>,
        elapsed_ms: u128,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match &self.field_method {
            Some(method) if method != &answer.source.method => {
                return Err("Registry field methods differ within one sweep".into());
            }
            None => self.field_method = Some(answer.source.method.clone()),
            _ => {}
        }

        match answer.completeness {
            Completeness::Complete => self.complete_queries += 1,
            Completeness::Partial => self.partial_queries += 1,
        }

        for gap in &answer.gaps {
            if gap.kind != GapKind::OutsideMethod {
                self.failure_shapes
                    .entry(gap.detail.clone())
                    .or_default()
                    .push(json!({
                        "registry": registry,
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
        self.unresolved_paths += path_gaps;
        if path_gaps > 0 {
            self.queries_with_unresolved_paths += 1;
        }

        self.references.add(registry, &answer, &answer.value, "");
        count_shapes(&answer.value, &mut self.field_shapes, false);
        count_families(&answer.value, &mut self.field_families, "root");
        for field in &answer.value {
            let name = format!("{registry}#{}", field.name);
            *self
                .fields_by_reader_kind
                .entry(field.reader.kind)
                .or_default() += 1;

            let Some(identity) = &field.reader.id else {
                self.fields_without_reader_identity.push(name);
                continue;
            };
            let id = serde_json::to_value(identity)?.as_str().unwrap().to_owned();
            let reader = self.readers.entry(id).or_insert_with(|| ReaderFields {
                kind: field.reader.kind,
                fields: Vec::new(),
                registries: BTreeMap::new(),
            });
            reader
                .registries
                .insert(registry.to_owned(), answer.completeness);
            reader.fields.push(name);
        }

        self.stop_cases.extend(stops);
        self.cases.push(json!({
            "registry": registry,
            "status": status(Some(answer.completeness)),
            "elapsed_ms": elapsed_ms,
            "answer": answer,
        }));
        Ok(())
    }

    fn add_failure(&mut self, registry: &str, error: Error, elapsed_ms: u128) {
        self.failed_queries += 1;
        self.failure_shapes
            .entry(error.to_string())
            .or_default()
            .push(json!({
                "registry": registry,
                "error": error,
            }));
        self.cases.push(json!({
            "registry": registry,
            "status": status(None),
            "elapsed_ms": elapsed_ms,
            "error": error,
        }));
    }

    /// The report. Fails when no query gave an answer, because then no method was established.
    fn into_json(
        self,
        build: BuildId,
        elapsed_ms: u128,
        registries: Answer<Vec<Registry>>,
        reference_readers: Value,
    ) -> Result<Value, Box<dyn std::error::Error>> {
        let field_method = self
            .field_method
            .ok_or("No registry field method was established")?;
        let known_identities: usize = self
            .readers
            .values()
            .map(|reader| reader.fields.len())
            .sum();
        let unknown_identities = self.fields_without_reader_identity.len();
        let fields_found = known_identities + unknown_identities;
        let unknown_kinds = self
            .fields_by_reader_kind
            .get(&ReaderKind::Unknown)
            .copied()
            .unwrap_or_default();
        let readers: BTreeMap<_, Value> = self
            .readers
            .into_iter()
            .map(|(id, reader)| (id, reader.report()))
            .collect();

        Ok(json!({
            "method": field_method,
            "build": build,
            "elapsed_ms": elapsed_ms,
            "registry_count": registries.value.len(),
            "registry_inventory": registries,
            "summary": {
                "complete_queries": self.complete_queries,
                "partial_queries": self.partial_queries,
                "failed_queries": self.failed_queries,
                "queries_with_unresolved_paths": self.queries_with_unresolved_paths,
                "fields_found": fields_found,
                "unresolved_paths": self.unresolved_paths,
                "known_reader_identities": known_identities,
                "unknown_reader_identities": unknown_identities,
                "known_reader_kinds": fields_found - unknown_kinds,
                "unknown_reader_kinds": unknown_kinds,
                "distinct_known_readers": readers.len(),
            },
            "readers": readers,
            "fields_by_reader_kind": self.fields_by_reader_kind,
            "field_shapes": self.field_shapes,
            "field_families": self.field_families,
            "fields_without_reader_identity": self.fields_without_reader_identity,
            "failure_shapes": self.failure_shapes,
            "references": {
                "fields": self.references.complete + self.references.partial + self.references.failed,
                "complete": self.references.complete,
                "partial": self.references.partial,
                "failed": self.references.failed,
                "failure_shapes": self.references.shapes,
                "readers": reference_readers,
            },
            "stop_shapes": stop_shapes(&self.stop_cases),
            "report_limits": {
                "failure_shapes": "Grouped by public gap detail.",
                "stop_shapes": "Every internal gap of the registry field method. A gap with a stop is grouped by the stop instruction's mnemonic, the method's reason and the obstacle, then by the function that holds the instruction; one without a stop by its kind and reason. One stopped path can also leave an unresolved-token-path gap without a stop.",
                "reader_registry_answers": "Completeness of registry answers containing this reader, not completeness of the reader's full semantics. Failed queries cannot be assigned to a reader.",
                "references": "Root and nested fields with a read alternative whose reader is a reference reader. Complete: every lookup names a registry and every lookup property is established. Failed: no lookup names a registry. Readers counts every reference reader in the executable, joined or not.",
            },
            "cases": self.cases,
        }))
    }
}

/// Fields read by a reference reader, by how much of their lookup is established.
#[derive(Default)]
struct ReferenceTally {
    complete: usize,
    partial: usize,
    failed: usize,
    /// Field names by the gap that says what is missing.
    shapes: BTreeMap<String, Vec<String>>,
}

impl ReferenceTally {
    fn add(&mut self, registry: &str, answer: &Answer<Vec<Field>>, fields: &[Field], parent: &str) {
        for field in fields {
            let path = format!("{parent}{}", field.name);
            if let pdx_native::FieldMembers::Fields(children) = &field.members {
                self.add(registry, answer, children, &format!("{path}."));
            }
            let reads_reference = field.read.iter().any(|alternative| {
                matches!(&alternative.outcome, FieldReadOutcome::Read { reader, .. } if reader.kind == ReaderKind::Reference)
            });
            if !reads_reference {
                continue;
            }

            let lookups = match &field.reference {
                FieldReference::Lookups(lookups) => lookups.as_slice(),
                _ => &[],
            };
            let named = lookups
                .iter()
                .filter(|lookup| matches!(lookup.target, ReferenceTarget::Registry { .. }))
                .count();
            let established = lookups.iter().all(|lookup| {
                lookup.stage != LookupStage::Unresolved
                    && lookup.key_match != KeyMatch::Unresolved
                    && lookup.empty_key != EmptyKey::Unresolved
                    && lookup.on_missing != MissingResult::Unresolved
            });
            let name = format!("{registry}#{path}");
            if !lookups.is_empty() && named == lookups.len() && established {
                self.complete += 1;
                continue;
            }
            if named == 0 {
                self.failed += 1;
            } else {
                self.partial += 1;
            }
            let reason = answer
                .gaps
                .iter()
                .find(|gap| {
                    gap.kind == GapKind::ReaderSemantics
                        && matches!(&gap.subject, Some(GapSubject::Field { name }) if name == &path)
                })
                .map_or("no lookup is established".to_owned(), |gap| {
                    gap.detail.clone()
                });
            self.shapes.entry(reason).or_default().push(name);
        }
    }
}

fn count_families(fields: &[Field], counts: &mut BTreeMap<String, usize>, level: &str) {
    for field in fields {
        *counts
            .entry(format!("{level}.{:?}", field.reader.family))
            .or_default() += 1;
        if let pdx_native::FieldMembers::Fields(children) = &field.members {
            count_families(children, counts, "nested");
        }
    }
}

fn count_shapes(fields: &[Field], counts: &mut BTreeMap<String, usize>, nested: bool) {
    for field in fields {
        let level = if nested { "nested" } else { "root" };
        *counts
            .entry(format!("{level}.value.{:?}", field.shape.value))
            .or_default() += 1;
        *counts
            .entry(format!("{level}.repeat.{:?}", field.shape.repeat))
            .or_default() += 1;
        *counts.entry(format!("{level}.uses")).or_default() += field.uses.len();
        for alternative in &field.read {
            let condition = match &alternative.condition {
                pdx_native::FieldCondition::Always => "always",
                pdx_native::FieldCondition::FieldZero { .. } => "field",
                _ => "unresolved-or-composite",
            };
            *counts
                .entry(format!("{level}.read.{condition}"))
                .or_default() += 1;
        }
        if let pdx_native::FieldMembers::Fields(children) = &field.members {
            count_shapes(children, counts, true);
        }
    }
}

impl ReaderFields {
    /// The reader's fields and the completeness of the registry answers that hold them. A failed
    /// query holds no fields, so it never counts here.
    fn report(self) -> Value {
        let complete = self
            .registries
            .values()
            .filter(|status| **status == Completeness::Complete)
            .count();
        json!({
            "kind": self.kind,
            "count": self.fields.len(),
            "fields": self.fields,
            "registry_answers": {
                "complete": complete,
                "partial": self.registries.len() - complete,
                "failed": 0,
            },
        })
    }
}

/// Place each internal gap's stop, if it has one, in the image.
fn place_gaps(image: &Image, registry: &str, gaps: Vec<FieldGap>) -> Vec<StopCase> {
    gaps.into_iter()
        .map(|gap| {
            let placed = gap.stop.map(|stop| image.place_stop(stop));
            StopCase {
                registry: registry.into(),
                function: placed.as_ref().and_then(|placed| placed.function.clone()),
                operation: placed.and_then(|placed| placed.row.map(|row| row.operation)),
                gap,
            }
        })
        .collect()
}

/// A registry's status in the report, from its answer's completeness or `None` for an error. A
/// partial answer is never complete.
fn status(completeness: Option<Completeness>) -> &'static str {
    match completeness {
        Some(Completeness::Complete) => "complete",
        Some(Completeness::Partial) => "partial",
        None => "failed",
    }
}

/// Group internal gaps by the stop's instruction kind and obstacle, then by function.
fn stop_shapes(cases: &[StopCase]) -> Value {
    let mut shapes: BTreeMap<String, BTreeMap<String, Vec<Value>>> = BTreeMap::new();
    for case in cases {
        let (shape, function) = match &case.gap.stop {
            Some(stop) => (
                format!(
                    "{} | {}: {}",
                    case.operation
                        .as_deref()
                        .unwrap_or("outside the text section"),
                    case.gap.reason,
                    stop.obstacle
                ),
                case.function
                    .clone()
                    .unwrap_or_else(|| "no text symbol".into()),
            ),
            None => (
                format!("{:?}: {}", case.gap.kind, case.gap.reason),
                "no stop".into(),
            ),
        };
        shapes
            .entry(shape)
            .or_default()
            .entry(function)
            .or_default()
            .push(json!({
                "registry": case.registry,
                "kind": format!("{:?}", case.gap.kind),
                "path": case.gap.path,
                "instruction": case.gap.stop.map(|stop| format!("{:#x}", stop.instruction)),
            }));
    }

    shapes
        .into_iter()
        .map(|(shape, functions)| {
            let count: usize = functions.values().map(Vec::len).sum();
            (shape, json!({ "count": count, "functions": functions }))
        })
        .collect::<serde_json::Map<_, _>>()
        .into()
}

/// The registries whose normalized answer or error differs between two reports. Elapsed time
/// and the other report fields are ignored.
fn diff(before: &Value, after: &Value) -> Value {
    let mut before = normalized_cases(before);
    let mut after = normalized_cases(after);
    let dropped = population::normalize_answers::<Answer<Vec<Field>>>(&mut before, &mut after);
    let registries: BTreeSet<_> = before.keys().chain(after.keys()).collect();

    let changed: Vec<Value> = registries
        .into_iter()
        .filter(|registry| before.get(*registry) != after.get(*registry))
        .map(|registry| {
            let (old, new) = (before.get(registry), after.get(registry));
            let old_fields = field_names(old);
            let new_fields = field_names(new);
            json!({
                "registry": registry,
                "before": old.map(|case| &case["status"]),
                "after": new.map(|case| &case["status"]),
                "added_fields": new_fields.difference(&old_fields).collect::<Vec<_>>(),
                "removed_fields": old_fields.difference(&new_fields).collect::<Vec<_>>(),
            })
        })
        .collect();

    json!({ "changed_registries": changed.len(), "registries": changed , "defaulted_members": dropped })
}

/// Each registry's status with its normalized answer or error.
fn normalized_cases(report: &Value) -> BTreeMap<String, Value> {
    if let Some(answers) = population::baseline_answers(report) {
        return answers;
    }
    report["cases"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|case| {
            let registry = case["registry"].as_str().unwrap_or_default().to_owned();
            let normalized = json!({
                "status": case["status"],
                "answer": case["answer"],
                "error": case["error"],
            });
            (registry, normalized)
        })
        .collect()
}

fn field_names(case: Option<&Value>) -> BTreeSet<String> {
    case.and_then(|case| case["answer"]["value"].as_array())
        .into_iter()
        .flatten()
        .filter_map(|field| field["name"].as_str().map(str::to_owned))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdx_native::internals::registry_field_stops::{
        FieldGapKind, Obstacle, Unknown, Unresolved,
    };

    fn report(cases: Value) -> Value {
        json!({ "elapsed_ms": 1, "cases": cases })
    }

    fn case(registry: &str, fields: &[&str], reader: &str, status: &str) -> Value {
        let value: Vec<_> = fields
            .iter()
            .map(|name| json!({ "name": name, "reader": { "id": reader, "kind": "Integer" } }))
            .collect();
        json!({
            "registry": registry,
            "status": status,
            "elapsed_ms": 10,
            "answer": { "value": value, "completeness": status, "gaps": [] },
        })
    }

    fn answer<T: serde::de::DeserializeOwned>(
        method: &str,
        completeness: &str,
        value: Value,
        gaps: Value,
    ) -> Answer<T> {
        serde_json::from_value(json!({
            "value": value,
            "completeness": completeness,
            "gaps": gaps,
            "source": {
                "build": "build",
                "native_version": "0",
                "method": method,
                "basis": "StaticAnalysis",
            },
        }))
        .unwrap()
    }

    #[test]
    fn compact_baseline_and_full_report_have_the_same_comparison_inputs() {
        let full = report(json!([case("common/a", &["x"], "r1", "complete")]));
        let text = population::format_baseline(&json!("build"), &normalized_cases(&full)).unwrap();
        let baseline: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(normalized_cases(&full), normalized_cases(&baseline));
        assert_eq!(diff(&baseline, &full)["changed_registries"], 0);
        let mut changed = baseline.clone();
        let answers = changed["answers"].as_object_mut().unwrap();
        answers.values_mut().next().unwrap()["status"] = json!("changed");
        assert_eq!(diff(&baseline, &changed)["changed_registries"], 1);
    }

    #[test]
    fn the_summary_counts_each_query_and_field_once() {
        let mut report = SweepReport::default();
        let fields = json!([
            { "name": "cost", "reader": { "id": "r1", "kind": "Integer" }, "shape": {"value": "Unknown", "repeat": "Unknown"}, "read": [{"condition": "Unresolved", "outcome": "Unresolved"}], "members": "Unresolved", "domain": "Unknown", "default": "Unknown", "uses": [] },
            { "name": "icon", "reader": { "id": null, "kind": "Unknown" }, "shape": {"value": "Unknown", "repeat": "Unknown"}, "read": [{"condition": "Unresolved", "outcome": "Unresolved"}], "members": "Unresolved", "domain": "Unknown", "default": "Unknown", "uses": [] },
        ]);
        let gaps = json!([{ "kind": "UnresolvedPath", "subject": null, "detail": "path 1" }]);
        report
            .add_answer(
                "common/a",
                answer("fields/v1", "Partial", fields, gaps),
                Vec::new(),
                1,
            )
            .unwrap();
        let fields = json!([
            { "name": "cost", "reader": { "id": "r1", "kind": "Integer" }, "shape": {"value": "Unknown", "repeat": "Unknown"}, "read": [{"condition": "Unresolved", "outcome": "Unresolved"}], "members": "Unresolved", "domain": "Unknown", "default": "Unknown", "uses": [] },
        ]);
        report
            .add_answer(
                "common/b",
                answer("fields/v1", "Complete", fields, json!([])),
                Vec::new(),
                1,
            )
            .unwrap();
        report.add_failure("common/c", Error::Method("stopped".into()), 1);

        let registries = answer("registries", "Complete", json!([]), json!([]));
        let json = report
            .into_json(registries.source.build.clone(), 1, registries, json!({}))
            .unwrap();

        assert_eq!(
            json["summary"],
            json!({
                "complete_queries": 1,
                "partial_queries": 1,
                "failed_queries": 1,
                "queries_with_unresolved_paths": 1,
                "fields_found": 3,
                "unresolved_paths": 1,
                "known_reader_identities": 2,
                "unknown_reader_identities": 1,
                "known_reader_kinds": 2,
                "unknown_reader_kinds": 1,
                "distinct_known_readers": 1,
            })
        );
        assert_eq!(
            json["readers"]["r1"]["registry_answers"],
            json!({ "complete": 1, "partial": 1, "failed": 0 })
        );
        assert_eq!(
            json["fields_without_reader_identity"],
            json!(["common/a#icon"])
        );
    }

    #[test]
    fn a_sweep_answered_by_two_methods_is_refused() {
        let mut report = SweepReport::default();
        let first = answer("fields/v1", "Complete", json!([]), json!([]));
        let second = answer("fields/v2", "Complete", json!([]), json!([]));

        report.add_answer("common/a", first, Vec::new(), 1).unwrap();

        assert!(
            report
                .add_answer("common/b", second, Vec::new(), 1)
                .is_err()
        );
    }

    #[test]
    fn a_partial_answer_is_never_complete() {
        assert_eq!(status(Some(Completeness::Complete)), "complete");
        assert_eq!(status(Some(Completeness::Partial)), "partial");
        assert_eq!(status(None), "failed");
    }

    #[test]
    fn two_reports_that_differ_only_in_time_have_an_empty_diff() {
        let before = report(json!([case("common/a", &["x"], "r1", "complete")]));
        let mut after = before.clone();
        after["elapsed_ms"] = json!(99);
        after["cases"][0]["elapsed_ms"] = json!(99);

        assert_eq!(
            diff(&before, &after),
            json!({ "changed_registries": 0, "registries": [], "defaulted_members": [] })
        );
    }

    #[test]
    fn a_diff_names_each_changed_answer_and_its_fields() {
        let before = report(json!([
            case("common/fields", &["kept", "removed"], "r1", "complete"),
            case("common/reader", &["x"], "r1", "complete"),
            case("common/status", &["x"], "r1", "complete"),
            case("common/gone", &["x"], "r1", "complete"),
            case("common/failed", &["x"], "r1", "complete"),
        ]));
        let mut failed = json!({ "registry": "common/failed", "status": "failed" });
        failed["error"] = json!({ "Method": "stopped" });
        let mut gaps = case("common/gaps", &["x"], "r1", "partial");
        gaps["answer"]["gaps"] = json!([{ "kind": "UnresolvedPath" }]);
        let after = report(json!([
            case("common/fields", &["kept", "added"], "r1", "complete"),
            case("common/reader", &["x"], "r2", "complete"),
            case("common/status", &["x"], "r1", "partial"),
            failed,
            gaps,
        ]));

        let changed = diff(&before, &after);
        let registries: Vec<_> = changed["registries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|case| case["registry"].as_str().unwrap())
            .collect();
        assert_eq!(
            registries,
            [
                "common/failed",
                "common/fields",
                "common/gaps",
                "common/gone",
                "common/reader",
                "common/status"
            ]
        );
        let fields = &changed["registries"][1];
        assert_eq!(fields["added_fields"], json!(["added"]));
        assert_eq!(fields["removed_fields"], json!(["removed"]));
        assert_eq!(changed["registries"][0]["after"], json!("failed"));
        assert_eq!(changed["registries"][3]["after"], Value::Null);
    }

    #[test]
    fn stops_group_by_instruction_kind_and_obstacle_then_function() {
        let flags = |registry: &str, function: &str, instruction| StopCase {
            registry: registry.into(),
            gap: FieldGap {
                path: Some(0),
                ..FieldGap::unresolved(
                    FieldGapKind::ReaderJoin,
                    Unresolved::at(
                        "flags",
                        instruction,
                        0x1000,
                        Obstacle::Unknown(Unknown::Flags),
                    ),
                )
            },
            function: Some(function.into()),
            operation: Some("b.hi".into()),
        };
        let cases = [
            flags("common/a", "A::ReadMember", 0x1004),
            flags("common/b", "B::ReadMember", 0x2004),
            flags("common/c", "B::ReadMember", 0x2008),
            StopCase {
                registry: "common/a".into(),
                gap: FieldGap::new(FieldGapKind::TokenTable, "conflicting names for token 5"),
                function: None,
                operation: None,
            },
        ];

        let shapes = stop_shapes(&cases);
        let flags = &shapes["b.hi | flags: flags unknown"];
        assert_eq!(flags["count"], json!(3));
        assert_eq!(
            flags["functions"]["B::ReadMember"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            flags["functions"]["A::ReadMember"][0]["instruction"],
            json!("0x1004")
        );
        assert_eq!(
            shapes["TokenTable: conflicting names for token 5"]["count"],
            json!(1)
        );
    }
}
