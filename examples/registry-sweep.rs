//! The automatic rate of the registry sweep (SDK-553), from recorded answers and the public API
//! only. The classification rules are in `docs/native/method-authoring.md#report-the-registry-sweep`.
//!
//! `registry-sweep --record DIRECTORY INSTALLATION` records `registries`, and `registry_fields`,
//! `derived_names` and `modifier_families` for each listed registry, into a new directory.
//!
//! `registry-sweep [--baseline] DIRECTORY` reads that directory through
//! `Native::from_recorded_answers` and writes the report, or only its comparison inputs.
//!
//! `registry-sweep --diff BEFORE AFTER` compares two reports or baselines and writes each
//! registry and relationship whose statuses changed.
// The report shares only the baseline format and its reader.
#[allow(dead_code)]
#[path = "support/population.rs"]
mod population;

use pdx_native::{
    AcceptedCategories, Answer, BlockFamily, Completeness, DerivedName, EmptyKey, EntryContext,
    EntryScope, Error, Field, FieldCondition, FieldMembers, FieldReadOutcome, FieldReference, Gap,
    GapKind, GapSubject, GrammarProperty, KeyMatch, LookupStage, MissingResult, ModifierFamily,
    Native, ReaderKind, ReferenceTarget, RepeatBehavior, ValueShape, WeightBlock, WeightOtherKeys,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Instant;

const USAGE: &str = "usage: registry-sweep --record DIRECTORY INSTALLATION | registry-sweep [--baseline] DIRECTORY | registry-sweep --diff BEFORE AFTER";

const FIELDS: &str = "fields";
const READER_KINDS: &str = "reader_kinds";
const SHAPES_AND_CONDITIONS: &str = "shapes_and_conditions";
const REFERENCES: &str = "references";
const NUMERICS: &str = "numerics";
const WEIGHTS: &str = "weights";
const NAMING_RULES: &str = "naming_rules";
const SCOPE_CONTEXT: &str = "scope_context";

/// The outcome of one unit of one relationship.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
enum Status {
    Complete,
    Partial,
    Failed,
    Unsupported,
    /// The recording holds no answer; not a method failure.
    Missing,
    /// The method did not establish whether the relationship applies.
    Undetermined,
}

impl Status {
    /// Complete when every part is complete, failed when every part failed, otherwise partial.
    fn of_parts(parts: &[Self]) -> Self {
        if parts.iter().all(|part| *part == Self::Complete) {
            Self::Complete
        } else if parts.iter().all(|part| *part == Self::Failed) {
            Self::Failed
        } else {
            Self::Partial
        }
    }

    fn established(established: bool) -> Self {
        if established {
            Self::Complete
        } else {
            Self::Failed
        }
    }

    fn in_denominator(self) -> bool {
        self != Self::Missing
    }
}

/// What one recorded question gave.
enum Outcome<'a, T> {
    Answer(&'a Answer<T>),
    Failed(String),
    Unsupported(String),
    Missing,
}

impl<'a, T> Outcome<'a, T> {
    /// Refuses an unreadable recording, which is no answer.
    fn of(result: &'a Result<Answer<T>, Error>) -> Result<Self, Error> {
        Ok(match result {
            Ok(answer) => Self::Answer(answer),
            Err(Error::NotRecorded { .. }) => Self::Missing,
            Err(Error::Unsupported { reason, .. }) => Self::Unsupported(reason.clone()),
            Err(Error::Recorded(reason)) => return Err(Error::Recorded(reason.clone())),
            Err(error) => Self::Failed(error.to_string()),
        })
    }

    fn answer(&self) -> Option<&'a Answer<T>> {
        match self {
            Self::Answer(answer) => Some(answer),
            _ => None,
        }
    }
}

/// A registry relationship's status, with the reason of a failed or unsupported one.
struct RegistryStatus {
    status: Status,
    reason: Option<String>,
}

impl RegistryStatus {
    fn new(status: Status) -> Self {
        Self {
            status,
            reason: None,
        }
    }

    /// The baseline form, such as `failed: the method failed: …`.
    fn text(&self) -> String {
        let status = serde_json::to_value(self.status).expect("statuses serialize");
        let status = status.as_str().expect("a status is a string");
        match &self.reason {
            Some(reason) => format!("{status}: {reason}"),
            None => status.to_owned(),
        }
    }
}

/// Complete with an answer and no gap that can hide a field.
fn fields_status(outcome: &Outcome<Vec<Field>>) -> RegistryStatus {
    match outcome {
        Outcome::Answer(answer) => {
            let hides_a_field = answer.gaps.iter().any(|gap| {
                matches!(
                    gap.kind,
                    GapKind::UnresolvedPath
                        | GapKind::UnnamedField
                        | GapKind::UnresolvedReader
                        | GapKind::UnreadableInput
                ) && matches!(gap.subject, None | Some(GapSubject::Registry { .. }))
            });
            RegistryStatus::new(if hides_a_field {
                Status::Partial
            } else {
                Status::Complete
            })
        }
        other => error_status(other),
    }
}

/// Complete when both naming questions answer completely, failed when neither answers.
fn naming_status<A, B>(derived: &Outcome<A>, families: &Outcome<B>) -> RegistryStatus {
    match (derived, families) {
        (Outcome::Missing, _) | (_, Outcome::Missing) => RegistryStatus::new(Status::Missing),
        (Outcome::Answer(derived), Outcome::Answer(families)) => {
            let complete = [derived.completeness, families.completeness]
                .iter()
                .all(|completeness| *completeness == Completeness::Complete);
            RegistryStatus::new(if complete {
                Status::Complete
            } else {
                Status::Partial
            })
        }
        (Outcome::Answer(_), error) => RegistryStatus {
            status: Status::Partial,
            reason: error_status(error).reason,
        },
        (error, Outcome::Answer(_)) => RegistryStatus {
            status: Status::Partial,
            reason: error_status(error).reason,
        },
        (Outcome::Unsupported(_), Outcome::Unsupported(_)) | (Outcome::Failed(_), _) => {
            error_status(derived)
        }
        (_, failed) => error_status(failed),
    }
}

/// The status of a question that gave no answer.
fn error_status<T>(outcome: &Outcome<T>) -> RegistryStatus {
    let (status, reason) = match outcome {
        Outcome::Answer(_) => unreachable!("an answer has no error status"),
        Outcome::Failed(reason) => (Status::Failed, Some(reason.clone())),
        Outcome::Unsupported(reason) => (Status::Unsupported, Some(reason.clone())),
        Outcome::Missing => (Status::Missing, None),
    };
    RegistryStatus { status, reason }
}

/// The field relationship that a gap of this kind leaves partial when it names the field.
fn gap_relationship(kind: GapKind) -> Option<&'static str> {
    match kind {
        GapKind::UnresolvedReader => Some(READER_KINDS),
        GapKind::UnresolvedStorage => Some(SHAPES_AND_CONDITIONS),
        GapKind::ReaderSemantics => Some(REFERENCES),
        GapKind::NumericConversion => Some(NUMERICS),
        GapKind::UnresolvedPath => Some(SCOPE_CONTEXT),
        _ => None,
    }
}

/// The status of each field relationship that applies to `field`, a root field of an answer
/// with these gaps.
fn field_statuses(field: &Field, gaps: &[Gap]) -> BTreeMap<&'static str, Status> {
    let mut statuses = property_statuses(field);
    for gap in gaps.iter().filter(|gap| gap.kind != GapKind::OutsideMethod) {
        let relationship = match &gap.subject {
            Some(GapSubject::Field { name }) if *name == field.name => gap_relationship(gap.kind),
            Some(GapSubject::KeyPath { path }) if path[0] == field.name => Some(WEIGHTS),
            _ => None,
        };
        if let Some(status) = relationship.and_then(|relationship| statuses.get_mut(relationship))
            && *status == Status::Complete
        {
            *status = Status::Partial;
        }
    }
    statuses
}

/// The status of each field relationship that applies to `field`, from its properties alone.
fn property_statuses(field: &Field) -> BTreeMap<&'static str, Status> {
    let unknown_kind = field.reader.kind == ReaderKind::Unknown;
    let mut statuses = BTreeMap::from([
        (READER_KINDS, reader_kind_status(field)),
        (SHAPES_AND_CONDITIONS, shape_status(field)),
        (SCOPE_CONTEXT, scope_status(field)),
    ]);
    let reads_reference = field.reader.kind == ReaderKind::Reference
        || field.read.iter().any(|alternative| {
            matches!(&alternative.outcome, FieldReadOutcome::Read { reader, .. } if reader.kind == ReaderKind::Reference)
        });
    if reads_reference {
        statuses.insert(REFERENCES, reference_status(&field.reference));
    } else if unknown_kind {
        statuses.insert(REFERENCES, Status::Undetermined);
    }
    if matches!(
        field.reader.kind,
        ReaderKind::Integer
            | ReaderKind::FixedPoint
            | ReaderKind::Float
            | ReaderKind::ScopedNumeric
    ) {
        statuses.insert(NUMERICS, numeric_status(&field.reader.numeric));
    } else if unknown_kind {
        statuses.insert(NUMERICS, Status::Undetermined);
    }
    match &field.members {
        FieldMembers::WeightBlock(block) => {
            statuses.insert(WEIGHTS, weight_status(block));
        }
        _ if field.reader.family == BlockFamily::Weight => {
            statuses.insert(WEIGHTS, Status::Failed);
        }
        _ if unknown_kind
            || (field.reader.kind == ReaderKind::Block
                && field.reader.family == BlockFamily::Unknown) =>
        {
            statuses.insert(WEIGHTS, Status::Undetermined);
        }
        _ => {}
    }
    statuses
}

fn reader_kind_status(field: &Field) -> Status {
    Status::of_parts(&[
        Status::established(field.reader.id.is_some()),
        Status::established(field.reader.kind != ReaderKind::Unknown),
    ])
}

fn shape_status(field: &Field) -> Status {
    let value = field.shape.value != ValueShape::Unknown;
    let repeat = field.shape.repeat != RepeatBehavior::Unknown;
    let outcomes = field
        .read
        .iter()
        .filter(|alternative| !matches!(alternative.outcome, FieldReadOutcome::Unresolved))
        .count();
    let conditions = field
        .read
        .iter()
        .all(|alternative| resolved(&alternative.condition));
    if value && repeat && conditions && outcomes == field.read.len() {
        Status::Complete
    } else if !value && !repeat && outcomes == 0 {
        Status::Failed
    } else {
        Status::Partial
    }
}

/// Whether a condition has no unresolved term.
fn resolved(condition: &FieldCondition) -> bool {
    match condition {
        FieldCondition::Unresolved => false,
        FieldCondition::All(terms) => terms.iter().all(resolved),
        _ => true,
    }
}

fn reference_status(reference: &FieldReference) -> Status {
    let FieldReference::Lookups(lookups) = reference else {
        return Status::Failed;
    };
    let targeted = lookups
        .iter()
        .filter(|lookup| !matches!(lookup.target, ReferenceTarget::Unresolved))
        .count();
    let established = lookups.iter().all(|lookup| {
        lookup.stage != LookupStage::Unresolved
            && lookup.key_match != KeyMatch::Unresolved
            && lookup.empty_key != EmptyKey::Unresolved
            && lookup.on_missing != MissingResult::Unresolved
    });
    match targeted {
        0 => Status::Failed,
        count if count == lookups.len() && established => Status::Complete,
        _ => Status::Partial,
    }
}

/// The `Partial` wrapper of a conversion does not count: a `NumericConversion` gap on the field
/// makes it partial (`docs/native/numeric-conversion.md`).
fn numeric_status<T>(numeric: &GrammarProperty<Option<T>>) -> Status {
    match numeric {
        GrammarProperty::Known(Some(_)) | GrammarProperty::Partial(Some(_)) => Status::Complete,
        GrammarProperty::Known(None)
        | GrammarProperty::Partial(None)
        | GrammarProperty::Unresolved => Status::Failed,
    }
}

fn weight_status(block: &WeightBlock) -> Status {
    if weight_established(block) {
        Status::Complete
    } else {
        Status::Partial
    }
}

/// Every property of the block and of each nested weight key is established.
fn weight_established(block: &WeightBlock) -> bool {
    let GrammarProperty::Known(keys) = &block.fixed_keys else {
        return false;
    };
    let nested = keys.iter().all(|key| match &key.members {
        FieldMembers::WeightBlock(nested) => weight_established(nested),
        _ => true,
    });
    let repeat = match &block.operations {
        GrammarProperty::Known(operations) => {
            operations.is_empty() || block.operation_repeat != RepeatBehavior::Unknown
        }
        _ => false,
    };
    nested
        && repeat
        && matches!(block.scalar, GrammarProperty::Known(_))
        && matches!(
            block.other_keys,
            WeightOtherKeys::Rejected | WeightOtherKeys::Triggers(GrammarProperty::Known(_))
        )
}

fn scope_status(field: &Field) -> Status {
    let mut parts = vec![
        match field.read_scope {
            GrammarProperty::Known(_) => Status::Complete,
            GrammarProperty::Partial(_) => Status::Partial,
            GrammarProperty::Unresolved => Status::Failed,
        },
        Status::established(!matches!(
            field.accepted_categories,
            AcceptedCategories::Unresolved
        )),
    ];
    if matches!(
        field.reader.family,
        BlockFamily::Trigger | BlockFamily::Effect | BlockFamily::Weight
    ) {
        parts.push(match field.entry_contexts.as_slice() {
            [] => Status::Failed,
            contexts if contexts.iter().all(established_context) => Status::Complete,
            _ => Status::Partial,
        });
    }
    Status::of_parts(&parts)
}

fn established_context(context: &EntryContext) -> bool {
    [&context.this, &context.root]
        .into_iter()
        .chain(&context.from)
        .chain(&context.prev)
        .all(|scope| !matches!(scope, EntryScope::Unresolved))
}

/// Counts of units by status, with the rate over the denominator.
#[derive(Default)]
struct Counts(BTreeMap<Status, usize>);

impl Counts {
    fn add(&mut self, status: Status) {
        *self.0.entry(status).or_default() += 1;
    }

    fn report(&self) -> Value {
        let denominator: usize = self
            .0
            .iter()
            .filter(|(status, _)| status.in_denominator())
            .map(|(_, count)| count)
            .sum();
        let complete = self.0.get(&Status::Complete).copied().unwrap_or_default();
        let mut report = serde_json::to_value(&self.0).expect("counts serialize");
        report["denominator"] = json!(denominator);
        report["rate"] = if denominator == 0 {
            Value::Null
        } else {
            json!(((complete as f64 / denominator as f64) * 10_000.0).round() / 10_000.0)
        };
        report
    }
}

/// The report state, gathered registry by registry.
#[derive(Default)]
struct Sweep {
    totals: BTreeMap<&'static str, Counts>,
    /// Each registry's comparison inputs: its registry statuses and its field counts.
    registries: BTreeMap<String, Value>,
    /// Field counts by reader identity, then relationship.
    readers: BTreeMap<String, BTreeMap<&'static str, Counts>>,
    /// Each field's statuses, by `registry#field`.
    fields: BTreeMap<String, BTreeMap<&'static str, Status>>,
    /// Registries whose `registry_fields` question has no answer, so no field units.
    registries_without_field_units: Counts,
    gap_shapes: BTreeMap<String, BTreeSet<String>>,
    gap_counts: BTreeMap<String, usize>,
    errors: BTreeMap<String, BTreeSet<String>>,
}

impl Sweep {
    /// Add one registry's outcomes of the three registry questions.
    fn add(
        &mut self,
        registry: &str,
        fields: Outcome<Vec<Field>>,
        derived: Outcome<Vec<DerivedName>>,
        families: Outcome<Vec<ModifierFamily>>,
    ) {
        self.add_failure_shapes(registry, "registry_fields", &fields);
        self.add_failure_shapes(registry, "derived_names", &derived);
        self.add_failure_shapes(registry, "modifier_families", &families);

        let mut row = serde_json::Map::new();
        for (relationship, status) in [
            (FIELDS, fields_status(&fields)),
            (NAMING_RULES, naming_status(&derived, &families)),
        ] {
            self.totals
                .entry(relationship)
                .or_default()
                .add(status.status);
            row.insert(relationship.into(), json!(status.text()));
        }

        let mut counts: BTreeMap<&'static str, Counts> = BTreeMap::new();
        let (answered, gaps) = match fields.answer() {
            Some(answer) => (answer.value.as_slice(), answer.gaps.as_slice()),
            None => {
                self.registries_without_field_units
                    .add(fields_status(&fields).status);
                (&[][..], &[][..])
            }
        };
        for field in answered {
            let statuses = field_statuses(field, gaps);
            let by_reader = self.readers.entry(reader_name(field)).or_default();
            for (relationship, status) in &statuses {
                counts.entry(relationship).or_default().add(*status);
                by_reader.entry(relationship).or_default().add(*status);
                self.totals.entry(relationship).or_default().add(*status);
            }
            self.fields
                .insert(format!("{registry}#{}", field.name), statuses);
        }
        for (relationship, counts) in counts {
            row.insert(relationship.into(), counts.report());
        }
        self.registries.insert(registry.to_owned(), row.into());
    }

    fn add_failure_shapes<T>(&mut self, registry: &str, operation: &str, outcome: &Outcome<T>) {
        match outcome {
            Outcome::Answer(answer) => {
                for gap in answer
                    .gaps
                    .iter()
                    .filter(|gap| gap.kind != GapKind::OutsideMethod)
                {
                    let shape = format!(
                        "{operation} | {:?}: {}",
                        gap.kind,
                        without_digits(&gap.detail)
                    );
                    *self.gap_counts.entry(shape.clone()).or_default() += 1;
                    self.gap_shapes
                        .entry(shape)
                        .or_default()
                        .insert(registry.to_owned());
                }
            }
            Outcome::Failed(reason) | Outcome::Unsupported(reason) => {
                self.errors
                    .entry(format!("{operation} | {reason}"))
                    .or_default()
                    .insert(registry.to_owned());
            }
            Outcome::Missing => {
                self.errors
                    .entry(format!("{operation} | not recorded"))
                    .or_default()
                    .insert(registry.to_owned());
            }
        }
    }

    fn report(self, build: Value, registries: Value) -> Value {
        let counts = |groups: BTreeMap<&'static str, Counts>| -> BTreeMap<&'static str, Value> {
            groups
                .into_iter()
                .map(|(relationship, counts)| (relationship, counts.report()))
                .collect()
        };
        let gap_shapes: BTreeMap<_, _> = self
            .gap_shapes
            .into_iter()
            .map(|(shape, registries)| {
                let count = self.gap_counts[&shape];
                (shape, json!({ "gaps": count, "registries": registries }))
            })
            .collect();
        json!({
            "build": build,
            "registry_inventory": registries,
            "relationships": counts(self.totals),
            "registries_without_field_units": self.registries_without_field_units.0,
            "registries": self.registries,
            "readers": self
                .readers
                .into_iter()
                .map(|(reader, groups)| (reader, counts(groups)))
                .collect::<BTreeMap<_, _>>(),
            "fields": self.fields,
            "failure_shapes": { "gaps": gap_shapes, "errors": self.errors },
        })
    }
}

/// The field's reader identity, or `none`. `ReaderId` has no string accessor; it serializes as
/// its text.
fn reader_name(field: &Field) -> String {
    let Some(id) = &field.reader.id else {
        return "none".to_owned();
    };
    match serde_json::to_value(id) {
        Ok(Value::String(name)) => name,
        _ => unreachable!("a reader identity serializes as a string"),
    }
}

/// `text` with each run of digits replaced by `N`.
fn without_digits(text: &str) -> String {
    let mut normalized = String::with_capacity(text.len());
    for character in text.chars() {
        if !character.is_ascii_digit() {
            normalized.push(character);
        } else if !normalized.ends_with('N') {
            normalized.push('N');
        }
    }
    normalized
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    match arguments.as_slice() {
        [flag, directory, installation] if flag == "--record" => record(directory, installation),
        [flag, before, after] if flag == "--diff" => {
            let report = diff(&read_report(before)?, &read_report(after)?)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
        [flag, directory] if flag == "--baseline" => {
            let report = sweep(Path::new(directory))?;
            println!(
                "{}",
                population::format_baseline(&report["build"], &comparison_inputs(&report)?)?
            );
            Ok(())
        }
        [directory] => {
            let report = sweep(Path::new(directory))?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
        _ => Err(USAGE.into()),
    }
}

/// Record the questions that the report reads, errors included.
fn record(directory: &str, installation: &str) -> Result<(), Box<dyn std::error::Error>> {
    if Path::new(directory).exists() {
        return Err(format!("{directory} exists; record into a new directory").into());
    }
    let started = Instant::now();
    let native = Native::open(installation)?.record_answers_to(directory);
    let registries = native.registries()?.value;
    for registry in &registries {
        for result in [
            native.registry_fields(&registry.name).map(drop),
            native.derived_names(&registry.name).map(drop),
            native.modifier_families(&registry.name).map(drop),
        ] {
            if let Err(error @ Error::Recorded(_)) = result {
                return Err(error.into());
            }
        }
    }
    eprintln!(
        "recorded {} registries in {:.1} s",
        registries.len(),
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

fn sweep(directory: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let native = Native::from_recorded_answers(directory)?;
    let registries = native.registries()?;
    let mut sweep = Sweep::default();
    for registry in &registries.value {
        let name = &registry.name;
        let fields = native.registry_fields(name);
        let derived = native.derived_names(name);
        let families = native.modifier_families(name);
        sweep.add(
            name,
            Outcome::of(&fields)?,
            Outcome::of(&derived)?,
            Outcome::of(&families)?,
        );
    }
    Ok(sweep.report(json!(native.build()), serde_json::to_value(&registries)?))
}

fn read_report(path: &str) -> Result<Value, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

/// Each registry's statuses, from a baseline or a full report.
fn comparison_inputs(report: &Value) -> population::ReportResult<BTreeMap<String, Value>> {
    if let Some(answers) = population::baseline_answers(report)? {
        return Ok(answers);
    }
    let registries = report
        .get("registries")
        .and_then(Value::as_object)
        .ok_or("expected an object at \"registries\"")?;
    Ok(registries
        .iter()
        .map(|(registry, row)| (registry.clone(), row.clone()))
        .collect())
}

/// Each registry and relationship whose statuses differ between two reports.
fn diff(before: &Value, after: &Value) -> population::ReportResult<Value> {
    let before = comparison_inputs(before).map_err(|error| format!("before report: {error}"))?;
    let after = comparison_inputs(after).map_err(|error| format!("after report: {error}"))?;
    let registries: BTreeSet<_> = before.keys().chain(after.keys()).collect();
    let mut changes = Vec::new();
    let mut changed = BTreeSet::new();
    for registry in registries {
        let (old, new) = (before.get(registry), after.get(registry));
        let relationships: BTreeSet<_> = [old, new]
            .into_iter()
            .flatten()
            .filter_map(Value::as_object)
            .flat_map(|row| row.keys())
            .collect();
        for relationship in relationships {
            let old = old.and_then(|row| row.get(relationship));
            let new = new.and_then(|row| row.get(relationship));
            if old != new {
                changed.insert(registry);
                changes.push(json!({
                    "registry": registry,
                    "relationship": relationship,
                    "before": old,
                    "after": new,
                }));
            }
        }
    }
    Ok(json!({ "changed_registries": changed.len(), "changes": changes }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn source() -> Value {
        json!({ "build": "example-build", "native_version": "0.1.0", "method": "example/v1", "basis": "StaticAnalysis" })
    }

    fn answer(value: Value, completeness: &str, gaps: Value) -> Value {
        json!({ "Ok": { "value": value, "completeness": completeness, "gaps": gaps, "source": source() } })
    }

    /// A field whose every property is established, as a scalar integer.
    fn established_field(name: &str) -> Value {
        let numeric = json!({ "representation": { "Known": "Integer" }, "width_bits": { "Known": 32 },
            "signedness": { "Known": "Signed" }, "scale": { "Known": null }, "literal_syntax": "Unresolved",
            "accepted_range": "Unresolved" });
        let reader = json!({ "id": "integer", "kind": "Integer", "family": "NotApplicable",
            "numeric": { "Known": numeric }, "scoped_operand": { "Known": null } });
        let shape = json!({ "value": "Scalar", "repeat": "Replace" });
        json!({ "name": name, "reader": reader, "shape": shape,
            "read": [{ "condition": "Always", "outcome": { "Read": { "reader": reader, "shape": shape } } }],
            "members": "None", "domain": "Unknown", "uses": [], "reference": "NotEstablished",
            "entry_contexts": [], "read_scope": { "Known": [] }, "accepted_categories": "NotApplicable" })
    }

    fn write(root: &Path, file: &str, value: Value) {
        let path = root.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
    }

    fn write_registry(root: &Path, registry: &str, fields: Value, derived: Value, families: Value) {
        write(root, &format!("registry_fields/{registry}.json"), fields);
        write(root, &format!("derived_names/{registry}.json"), derived);
        write(
            root,
            &format!("modifier_families/{registry}.json"),
            families,
        );
    }

    /// One registry for each control, written by hand.
    fn recording() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let path = root.path();
        let names = [
            "common/empty",
            "common/failed",
            "common/missing",
            "common/mixed",
            "common/unknown",
        ];
        let registries: Vec<_> = names.iter().map(|name| json!({ "name": name })).collect();
        write(path, "build.json", json!("example-build"));
        write(
            path,
            "registries.json",
            answer(json!(registries), "Complete", json!([])),
        );
        let none = || answer(json!([]), "Complete", json!([]));

        let mut mixed = established_field("cost");
        mixed["reader"]["numeric"] = json!({ "Partial": mixed["reader"]["numeric"]["Known"] });
        let mut potential = established_field("potential");
        potential["reader"] = json!({ "id": "trigger", "kind": "Block", "family": "Trigger",
            "numeric": { "Known": null }, "scoped_operand": { "Known": null } });
        potential["shape"]["value"] = json!("Block");
        potential["entry_contexts"] = json!([{ "this": "SelfLink", "root": "SelfLink", "from": ["SelfLink"], "prev": ["SelfLink"] }]);
        let gaps = json!([
            { "kind": "OutsideMethod", "subject": { "kind": "registry", "name": "common/mixed" }, "detail": "Outside." },
            { "kind": "NumericConversion", "subject": { "kind": "field", "name": "cost" }, "detail": "Range." },
            { "kind": "UnresolvedCondition", "subject": { "kind": "field", "name": "cost" }, "detail": "Use-time context." },
            { "kind": "UnresolvedPath", "subject": { "kind": "field", "name": "potential" }, "detail": "More paths (path-limit)." },
        ]);
        let unsupported = json!({ "Err": { "Unsupported": { "operation": "DerivedNames", "reason": "no names here" } } });
        write_registry(
            path,
            "common/mixed",
            answer(json!([mixed, potential]), "Partial", gaps),
            unsupported,
            none(),
        );

        write_registry(path, "common/empty", none(), none(), none());

        let mut unknown = established_field("icon");
        unknown["reader"] = json!({ "id": null, "kind": "Unknown", "family": "Unknown", "numeric": "Unresolved", "scoped_operand": "Unresolved" });
        unknown["shape"] = json!({ "value": "Unknown", "repeat": "Unknown" });
        unknown["read"] = json!([{ "condition": "Unresolved", "outcome": "Unresolved" }]);
        unknown["read_scope"] = json!("Unresolved");
        unknown["accepted_categories"] = json!("Unresolved");
        let gaps = json!([
            { "kind": "UnresolvedReader", "subject": { "kind": "field", "name": "icon" }, "detail": "No reader on 2 paths." },
            { "kind": "UnnamedField", "subject": { "kind": "registry", "name": "common/unknown" }, "detail": "3 paths have no name." },
        ]);
        let partial_names = answer(
            json!([]),
            "Partial",
            json!([{ "kind": "UnresolvedPath", "subject": null, "detail": "Stopped." }]),
        );
        write_registry(
            path,
            "common/unknown",
            answer(json!([unknown]), "Partial", gaps),
            partial_names,
            none(),
        );

        let stopped = json!({ "Err": { "Method": "stopped" } });
        write_registry(path, "common/failed", stopped.clone(), stopped, none());
        root
    }

    #[test]
    fn each_control_has_the_statuses_of_its_rules() {
        let root = recording();
        let report = sweep(root.path()).unwrap();
        let registry = |name: &str| &report["registries"][name];

        assert_eq!(
            registry("common/mixed")["fields"],
            "complete",
            "an OutsideMethod gap keeps the field set complete"
        );
        assert_eq!(
            registry("common/mixed")["naming_rules"],
            "partial: no names here"
        );
        let mixed = &report["fields"]["common/mixed#cost"];
        assert_eq!(
            *mixed,
            json!({ "reader_kinds": "complete", "shapes_and_conditions": "complete", "numerics": "partial", "scope_context": "complete" })
        );
        assert_eq!(
            report["fields"]["common/mixed#potential"]["scope_context"], "partial",
            "a path-limit gap leaves a resolved context partial"
        );

        assert_eq!(
            *registry("common/empty"),
            json!({ "fields": "complete", "naming_rules": "complete" })
        );

        assert_eq!(
            registry("common/unknown")["fields"],
            "partial",
            "an unnamed path can hold a field"
        );
        assert_eq!(
            report["fields"]["common/unknown#icon"],
            json!({ "reader_kinds": "failed", "shapes_and_conditions": "failed", "references": "undetermined",
                "numerics": "undetermined", "weights": "undetermined", "scope_context": "failed" })
        );
        assert_eq!(report["readers"]["none"]["reader_kinds"]["failed"], 1);

        assert_eq!(
            registry("common/failed")["fields"],
            "failed: the method failed: stopped"
        );
        assert_eq!(
            registry("common/failed")["naming_rules"],
            "partial: the method failed: stopped"
        );
        assert_eq!(
            report["failure_shapes"]["errors"]["registry_fields | the method failed: stopped"],
            json!(["common/failed"])
        );
        assert_eq!(
            report["failure_shapes"]["gaps"]["registry_fields | UnnamedField: N paths have no name."],
            json!({ "gaps": 1, "registries": ["common/unknown"] })
        );

        assert_eq!(
            *registry("common/missing"),
            json!({ "fields": "missing", "naming_rules": "missing" })
        );
        assert_eq!(
            report["relationships"]["fields"],
            json!({ "complete": 2, "partial": 1, "failed": 1, "missing": 1, "denominator": 4, "rate": 0.5 })
        );
        assert_eq!(
            report["relationships"]["numerics"],
            json!({ "partial": 1, "undetermined": 1, "denominator": 2, "rate": 0.0 })
        );
    }

    #[test]
    fn an_unreadable_recording_is_refused() {
        let root = recording();
        write(
            root.path(),
            "registry_fields/common/empty.json",
            json!({ "Ok": "not an answer" }),
        );
        assert!(sweep(root.path()).is_err());
    }

    #[test]
    fn the_baseline_is_stable_and_the_diff_names_each_changed_relationship() {
        let root = recording();
        let report = sweep(root.path()).unwrap();
        let baseline = |report: &Value| {
            population::format_baseline(&report["build"], &comparison_inputs(report).unwrap())
                .unwrap()
        };
        let text = baseline(&report);
        assert_eq!(text, baseline(&sweep(root.path()).unwrap()));
        let parsed: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(diff(&parsed, &report).unwrap()["changed_registries"], 0);

        fs::remove_file(root.path().join("modifier_families/common/empty.json")).unwrap();
        let changed = diff(&parsed, &sweep(root.path()).unwrap()).unwrap();
        assert_eq!(
            changed,
            json!({ "changed_registries": 1, "changes": [{ "registry": "common/empty",
                "relationship": "naming_rules", "before": "complete", "after": "missing" }] })
        );
        assert!(diff(&parsed, &json!({ "cases": [] })).is_err());
    }
}
