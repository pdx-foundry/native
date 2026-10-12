//! Shared comparison of serialized answers across additive serde defaults.
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};

/// Remove only defaulted members absent in the other answer. Deserialization supplies the
/// actual type defaults; unknown JSON members stay visible and required omissions stay changes.
/// Implementation stamps do not describe an answer change; build and basis remain compared.
pub fn normalize_answers<T: DeserializeOwned + Serialize>(
    before: &mut BTreeMap<String, Value>,
    after: &mut BTreeMap<String, Value>,
) -> BTreeSet<String> {
    let mut dropped = BTreeSet::new();
    for (key, old) in before {
        let Some(new) = after.get_mut(key) else {
            continue;
        };
        let (Some(old), Some(new)) = (old.get_mut("answer"), new.get_mut("answer")) else {
            continue;
        };
        let defaults = |answer: &Value| {
            serde_json::from_value::<T>(answer.clone())
                .ok()
                .and_then(|answer| serde_json::to_value(answer).ok())
        };
        let (old_defaults, new_defaults) = (defaults(old), defaults(new));
        prune_defaults(
            old,
            new,
            old_defaults.as_ref(),
            new_defaults.as_ref(),
            "",
            &mut dropped,
        );
        for answer in [old, new] {
            if let Some(source) = answer.get_mut("source").and_then(Value::as_object_mut) {
                source.remove("method");
                source.remove("native_version");
            }
        }
    }
    dropped
}

fn prune_defaults(
    before: &mut Value,
    after: &mut Value,
    before_defaults: Option<&Value>,
    after_defaults: Option<&Value>,
    path: &str,
    dropped: &mut BTreeSet<String>,
) {
    match (before, after) {
        (Value::Object(before), Value::Object(after)) => {
            let keys: BTreeSet<_> = before.keys().chain(after.keys()).cloned().collect();
            for key in keys {
                let location = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                let old_default = before_defaults.and_then(|value| value.get(&key));
                let new_default = after_defaults.and_then(|value| value.get(&key));
                match (before.get_mut(&key), after.get_mut(&key)) {
                    (Some(old), Some(new)) => {
                        prune_defaults(old, new, old_default, new_default, &location, dropped)
                    }
                    (None, Some(new)) if old_default == Some(new) => {
                        after.remove(&key);
                        dropped.insert(location);
                    }
                    (Some(old), None) if new_default == Some(old) => {
                        before.remove(&key);
                        dropped.insert(location);
                    }
                    _ => {}
                }
            }
        }
        (Value::Array(before), Value::Array(after)) => {
            for (index, (old, new)) in before.iter_mut().zip(after).enumerate() {
                prune_defaults(
                    old,
                    new,
                    before_defaults.and_then(|value| value.get(index)),
                    after_defaults.and_then(|value| value.get(index)),
                    &format!("{path}[]"),
                    dropped,
                );
            }
        }
        _ => {}
    }
}

/// Format only comparison inputs, with one subject per line so changes stay local in Git.
pub fn format_baseline(
    build: &Value,
    answers: &BTreeMap<String, Value>,
) -> serde_json::Result<String> {
    let rows: Result<Vec<_>, _> = answers
        .iter()
        .map(|(subject, answer)| {
            Ok(format!(
                "    {}: {}",
                serde_json::to_string(subject)?,
                serde_json::to_string(answer)?
            ))
        })
        .collect::<serde_json::Result<_>>();
    Ok(format!(
        "{{\n  \"build\": {},\n  \"answers\": {{\n{}\n  }}\n}}",
        serde_json::to_string(build)?,
        rows?.join(",\n")
    ))
}

/// The result of reading a population report. An error means that the report is malformed or of
/// another kind, which is not the same as an empty report.
pub type ReportResult<T> = Result<T, Box<dyn std::error::Error>>;

/// The array at `key` of a report or row. Absent and empty are different: a report of another
/// kind has no such array.
pub fn array<'a>(value: &'a Value, key: &str) -> ReportResult<&'a Vec<Value>> {
    value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("expected an array at \"{key}\"").into())
}

/// The string at `key` of a report or row.
pub fn text<'a>(value: &'a Value, key: &str) -> ReportResult<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("expected a string at \"{key}\"").into())
}

/// Add a row under a subject that no earlier row has.
pub fn insert_unique(
    rows: &mut BTreeMap<String, Value>,
    subject: String,
    row: Value,
) -> ReportResult<()> {
    match rows.entry(subject) {
        Entry::Vacant(slot) => {
            slot.insert(row);
            Ok(())
        }
        Entry::Occupied(slot) => Err(format!("duplicate subject: {}", slot.key()).into()),
    }
}

/// Read comparison inputs from a compact baseline, which names its build. `None` for a full
/// report, which has its own projection.
pub fn baseline_answers(report: &Value) -> ReportResult<Option<BTreeMap<String, Value>>> {
    let Some(answers) = report.get("answers") else {
        return Ok(None);
    };
    text(report, "build")?;
    let answers = answers
        .as_object()
        .ok_or("expected an object at \"answers\"")?;
    Ok(Some(
        answers
            .iter()
            .map(|(subject, answer)| (subject.clone(), answer.clone()))
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::json;

    #[derive(Serialize, Deserialize)]
    struct Answer {
        required: String,
        #[serde(default)]
        reference: pdx_native::FieldReference,
    }
    fn report(answer: Value) -> BTreeMap<String, Value> {
        BTreeMap::from([("case".into(), json!({"answer": answer}))])
    }

    #[test]
    fn serde_defaults_are_ignored_in_both_directions_but_other_changes_remain() {
        let old = report(json!({"required": "same"}));
        let new = report(json!({"required": "same", "reference": "NotEstablished"}));
        for (mut before, mut after) in [(old.clone(), new.clone()), (new, old)] {
            assert_eq!(
                normalize_answers::<Answer>(&mut before, &mut after),
                BTreeSet::from(["reference".into()])
            );
            assert_eq!(before, after);
        }
        for changed in [
            json!({"required": "same", "reference": {"Lookups": []}}),
            json!({"reference": "NotEstablished"}),
            json!({"required": "same", "unknown": null}),
        ] {
            let mut before = report(json!({"required": "same"}));
            let mut after = report(changed);
            normalize_answers::<Answer>(&mut before, &mut after);
            assert_ne!(before, after);
        }
    }

    #[test]
    fn missing_registry_and_nested_command_properties_remain_changes() {
        let fields: Value = serde_json::from_str(include_str!(
            "../../tests/expected/m452/fields-traditions.json"
        ))
        .unwrap();
        let mut field = fields[0].clone();
        field["reference"] = serde_json::json!("NotEstablished");
        let source = json!({"build": "build", "native_version": "test", "method": "test", "basis": "StaticAnalysis"});
        let registry = json!({"value": [field.clone()], "completeness": "Complete", "gaps": [], "source": source});
        let mut old_registry = registry.clone();
        old_registry["value"][0]
            .as_object_mut()
            .unwrap()
            .remove("reference");
        let mut before = report(old_registry);
        let mut after = report(registry);
        assert_eq!(
            normalize_answers::<pdx_native::Answer<Vec<pdx_native::Field>>>(
                &mut before,
                &mut after
            ),
            BTreeSet::new()
        );
        assert_ne!(before, after);

        let grammar = json!({
            "reader": field["reader"], "child_families": "Unresolved",
            "forms": "Unresolved", "targets": "Unresolved", "durations": "Unresolved",
            "fixed_keys": {"Known": [field]}, "numeric_keys": "Unresolved", "ordering": "Unresolved"
        });
        let mut nested = grammar.clone();
        nested["numeric_keys"] = json!({"Known": grammar});
        let command =
            json!({"value": nested, "completeness": "Complete", "gaps": [], "source": source});
        let mut old_command = command.clone();
        old_command["value"]["numeric_keys"]["Known"]["fixed_keys"]["Known"][0]
            .as_object_mut()
            .unwrap()
            .remove("reference");
        let mut before = report(old_command);
        let mut after = report(command);
        assert_eq!(
            normalize_answers::<pdx_native::Answer<pdx_native::CommandGrammar>>(
                &mut before,
                &mut after
            ),
            BTreeSet::new()
        );
        assert_ne!(before, after);
    }

    #[test]
    fn implementation_stamps_do_not_change_answers_but_build_and_basis_do() {
        let mut before = report(
            json!({"required": "same", "source": {"method": "v1", "native_version": "1", "build": "A", "basis": "StaticAnalysis"}}),
        );
        let mut after = before.clone();
        after.get_mut("case").unwrap()["answer"]["source"]["method"] = json!("v2");
        after.get_mut("case").unwrap()["answer"]["source"]["native_version"] = json!("2");
        normalize_answers::<Answer>(&mut before, &mut after);
        assert_eq!(before, after);
        for (key, value) in [("build", "B"), ("basis", "Recorded")] {
            let mut changed = after.clone();
            changed.get_mut("case").unwrap()["answer"]["source"][key] = json!(value);
            normalize_answers::<Answer>(&mut before, &mut changed);
            assert_ne!(before, changed);
        }
    }

    #[test]
    fn unchanged_members_are_not_reported() {
        let mut before = report(json!({"required": "same", "reference": "NotEstablished"}));
        let mut after = before.clone();
        assert!(normalize_answers::<Answer>(&mut before, &mut after).is_empty());
    }

    #[test]
    fn baseline_round_trip_keeps_each_subject_on_one_line() {
        let answers = BTreeMap::from([
            (
                "a\"quoted".into(),
                json!({"answer": {"value": [1, 2], "gaps": []}}),
            ),
            ("z".into(), json!({"error": "failed\nwith detail"})),
        ]);
        let text = format_baseline(&json!("build"), &answers).unwrap();
        assert_eq!(text.lines().count(), answers.len() + 5);
        let parsed: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["build"], "build");
        assert_eq!(baseline_answers(&parsed).unwrap().unwrap(), answers);
        assert_eq!(baseline_answers(&json!({"cases": []})).unwrap(), None);
    }

    #[test]
    fn a_baseline_needs_its_build_and_an_answers_object() {
        assert!(baseline_answers(&json!({"answers": {}})).is_err());
        assert!(baseline_answers(&json!({"build": 7, "answers": {}})).is_err());
        assert!(baseline_answers(&json!({"build": "build", "answers": []})).is_err());
        assert_eq!(
            baseline_answers(&json!({"build": "build", "answers": {}})).unwrap(),
            Some(BTreeMap::new())
        );
    }

    #[test]
    fn rows_need_their_array_text_and_a_new_subject() {
        let row = json!({"list": [], "name": "one", "number": 1});
        assert!(array(&row, "list").unwrap().is_empty());
        assert_eq!(text(&row, "name").unwrap(), "one");
        for key in ["absent", "name", "number"] {
            assert!(array(&row, key).is_err(), "{key}");
        }
        for key in ["absent", "list", "number"] {
            assert!(text(&row, key).is_err(), "{key}");
        }
        let mut rows = BTreeMap::new();
        insert_unique(&mut rows, "one".into(), json!(1)).unwrap();
        assert!(insert_unique(&mut rows, "one".into(), json!(2)).is_err());
        assert_eq!(rows["one"], json!(1));
    }
}
