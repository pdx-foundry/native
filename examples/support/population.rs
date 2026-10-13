//! Shared comparison of serialized answers across additive serde defaults.
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// The file of a baseline directory that names its build and holds its totals.
const HEADER_FILE: &str = "header.json";

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
        remove_implementation_stamps(old);
        remove_implementation_stamps(new);
    }
    dropped
}

/// Remove the method and Native version from an answer's source. A new method revision stamps
/// every answer, so a stamp would change every subject of a baseline.
fn remove_implementation_stamps(answer: &mut Value) {
    if let Some(source) = answer.get_mut("source").and_then(Value::as_object_mut) {
        source.remove("method");
        source.remove("native_version");
    }
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

/// Replace the baseline at `directory` with one file per subject, so that changes to different
/// subjects touch different files in Git. `<subject>.json` holds the subject's comparison inputs
/// on one line, without implementation stamps, and each `/` in a subject is a directory.
/// `header.json` holds `header`: an object with the `build` and any totals of the report. The same
/// inputs always give the same bytes.
///
/// Refuses a header without a build, a subject that cannot be a relative file name, two subjects
/// that differ only in case, and an existing directory that is not empty and holds no
/// `header.json`.
pub fn write_baseline(
    directory: &Path,
    header: &Value,
    answers: &BTreeMap<String, Value>,
) -> ReportResult<()> {
    let files = baseline_files(header, answers)?;
    remove_baseline(directory)?;

    for (path, contents) in files {
        let path = directory.join(path);

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(path, contents)?;
    }

    Ok(())
}

/// The relative path and contents of each file of a baseline.
fn baseline_files(
    header: &Value,
    answers: &BTreeMap<String, Value>,
) -> ReportResult<Vec<(PathBuf, String)>> {
    text(header, "build")?;

    let mut files = vec![(
        PathBuf::from(HEADER_FILE),
        serde_json::to_string_pretty(header)? + "\n",
    )];
    let mut names = BTreeSet::from([HEADER_FILE.to_owned()]);

    for (subject, answer) in answers {
        let mut answer = answer.clone();

        if let Some(answer) = answer.get_mut("answer") {
            remove_implementation_stamps(answer);
        }

        let path = subject_file(subject)?;

        // Case-insensitive file systems, the default on macOS, would let one file overwrite another.
        if !names.insert(path.to_string_lossy().to_lowercase()) {
            return Err(format!("subject names the same file as another: {subject}").into());
        }

        files.push((path, serde_json::to_string(&answer)? + "\n"));
    }

    Ok(files)
}

/// The file of `subject`, relative to its baseline directory. A component that ends in `.json`
/// could name both a subject's directory and another subject's file.
fn subject_file(subject: &str) -> ReportResult<PathBuf> {
    let relative = subject.split('/').all(|component| {
        !component.is_empty()
            && component != "."
            && component != ".."
            && !component.contains('\\')
            && !component.to_ascii_lowercase().ends_with(".json")
    });

    if !relative {
        return Err(format!("subject cannot name a baseline file: {subject:?}").into());
    }

    Ok(PathBuf::from(format!("{subject}.json")))
}

/// Remove an earlier baseline, so that a subject that no longer exists leaves no file.
fn remove_baseline(directory: &Path) -> ReportResult<()> {
    let mut entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };

    if entries.next().is_some() && !directory.join(HEADER_FILE).is_file() {
        return Err(format!(
            "{} is not a baseline: it is not empty and has no {HEADER_FILE}",
            directory.display()
        )
        .into());
    }

    fs::remove_dir_all(directory)?;

    Ok(())
}

/// Read a population report file, or a baseline directory as its header with an `answers`
/// object.
pub fn read_report(path: &Path) -> ReportResult<Value> {
    if !path.is_dir() {
        return Ok(serde_json::from_slice(&fs::read(path)?)?);
    }

    let mut report: Value = serde_json::from_slice(&fs::read(path.join(HEADER_FILE))?)?;
    let mut answers = Map::new();
    read_subjects(path, "", &mut answers)?;

    report
        .as_object_mut()
        .ok_or(format!("expected an object in {HEADER_FILE}"))?
        .insert("answers".into(), answers.into());

    Ok(report)
}

/// Add the subject of each `.json` file under `directory`, whose subjects start with `prefix`.
fn read_subjects(
    directory: &Path,
    prefix: &str,
    answers: &mut Map<String, Value>,
) -> ReportResult<()> {
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("not a subject file: {}", path.display()))?;

        if path.is_dir() {
            read_subjects(&path, &format!("{prefix}{name}/"), answers)?;
            continue;
        }

        let Some(stem) = name.strip_suffix(".json") else {
            continue;
        };

        if prefix.is_empty() && name == HEADER_FILE {
            continue;
        }

        answers.insert(
            format!("{prefix}{stem}"),
            serde_json::from_slice(&fs::read(&path)?)?,
        );
    }

    Ok(())
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
    fn a_baseline_directory_round_trips_with_one_line_per_subject_file() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("baseline");
        let answers = BTreeMap::from([
            (
                "common/a".into(),
                json!({"answer": {"value": [1, 2], "source": {"build": "build", "method": "v1", "native_version": "1"}}}),
            ),
            (
                "common/a/nested".into(),
                json!({"error": "failed\nwith detail"}),
            ),
            ("events".into(), json!({"status": "complete"})),
        ]);
        let header = json!({"build": "build", "total": 3});
        write_baseline(&directory, &header, &answers).unwrap();
        let file = fs::read_to_string(directory.join("common/a/nested.json")).unwrap();
        assert_eq!(file, "{\"error\":\"failed\\nwith detail\"}\n");

        let report = read_report(&directory).unwrap();
        assert_eq!(report["total"], 3);
        let mut unstamped = answers.clone();
        unstamped.get_mut("common/a").unwrap()["answer"]["source"] = json!({"build": "build"});
        assert_eq!(baseline_answers(&report).unwrap().unwrap(), unstamped);

        let fewer = BTreeMap::from([("events".into(), json!({"status": "partial"}))]);
        write_baseline(&directory, &header, &fewer).unwrap();
        assert_eq!(
            baseline_answers(&read_report(&directory).unwrap())
                .unwrap()
                .unwrap(),
            fewer
        );
        assert!(!directory.join("common").exists());
    }

    #[test]
    fn a_baseline_refuses_subjects_outside_its_directory_and_other_directories() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("baseline");
        let header = json!({"build": "build"});
        for subject in [
            "",
            "../escape",
            "common//a",
            "common/./a",
            "a\\b",
            "HEADER",
            "header.json/a",
            "a.JSON",
        ] {
            let answers = BTreeMap::from([(subject.into(), json!(1))]);
            assert!(
                write_baseline(&directory, &header, &answers).is_err(),
                "{subject}"
            );
        }
        let answers =
            BTreeMap::from([("Common/a".into(), json!(1)), ("common/a".into(), json!(2))]);
        assert!(write_baseline(&directory, &header, &answers).is_err());
        assert!(write_baseline(&directory, &json!({"total": 1}), &BTreeMap::new()).is_err());
        assert!(!directory.exists());

        fs::write(root.path().join("keep.txt"), "").unwrap();
        assert!(write_baseline(root.path(), &header, &BTreeMap::new()).is_err());
        assert!(root.path().join("keep.txt").exists());
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
