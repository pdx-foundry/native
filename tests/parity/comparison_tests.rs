use super::comparison::*;
use pdx_native::BuildId;
use serde_json::{Value, json};

fn build(name: &str) -> BuildId {
    serde_json::from_value(json!(name)).unwrap()
}

fn compare(file: &str, reviewed: &Value, candidate: &Value) -> Report {
    compare_static(
        &build("hotfix"),
        file,
        &serde_json::to_vec(reviewed).unwrap(),
        &serde_json::to_vec(candidate).unwrap(),
    )
}

fn commands(identity: &str) -> Value {
    json!({"effect/sample": {
        "source": {"build": identity, "native_version": "0.1.0", "method": "command-grammar/v2", "basis": "StaticAnalysis"},
        "value": {"forms": ["Literal", "Block"]}, "completeness": "Complete", "gaps": []
    }})
}

#[test]
fn unchanged_output_has_no_notices() {
    for (file, value) in [
        ("command-grammars.json", commands("hotfix")),
        ("registries.json", json!(["one", "two"])),
    ] {
        let report = compare(file, &value, &value);
        assert!(report.passes());
        assert!(report.differences.is_empty());
    }
}

#[test]
fn build_difference_is_permitted_but_other_provenance_is_strict() {
    let reviewed = commands("release");
    let candidate = commands("hotfix");
    let report = compare("command-grammars.json", &reviewed, &candidate);
    assert!(report.passes());
    assert_eq!(report.differences.len(), 1);
    let difference = &report.differences[0];
    assert_eq!(difference.path, "/effect~1sample/source/build");
    assert_eq!(difference.category, Category::Provenance);
    assert_eq!(difference.status, Status::Pass);
    assert_eq!(difference.reviewed, Some(json!("release")));
    assert_eq!(difference.candidate, Some(json!("hotfix")));
    for key in ["method", "basis", "native_version"] {
        let mut changed = candidate.clone();
        changed["effect/sample"]["source"][key] = if key == "basis" {
            json!("Recorded")
        } else {
            json!("changed")
        };
        let report = compare("command-grammars.json", &reviewed, &changed);
        assert!(!report.passes(), "{key}");
        let failed_provenance = report
            .differences
            .iter()
            .any(|entry| entry.category == Category::Provenance && entry.status == Status::Fail);
        assert!(failed_provenance);
    }
}

#[test]
fn wrong_and_missing_current_stamps_fail_even_when_files_match() {
    for candidate in [commands("wrong"), {
        let mut value = commands("hotfix");
        value["effect/sample"]["source"]
            .as_object_mut()
            .unwrap()
            .remove("build");
        value
    }] {
        let report = compare("command-grammars.json", &candidate, &candidate);
        assert!(!report.passes());
        assert!(report.full_text().contains("source"));
        assert!(
            !report
                .differences
                .iter()
                .any(|entry| entry.status == Status::Pass)
        );
    }
}

#[test]
fn malformed_command_sources_are_input_errors_on_either_side() {
    let valid = commands("hotfix");
    for source in [json!([]), json!({"build": "hotfix"}), Value::Null] {
        let mut malformed = valid.clone();
        malformed["effect/sample"]["source"] = source;
        for (reviewed, candidate) in [(&malformed, &valid), (&valid, &malformed)] {
            let report = compare("command-grammars.json", reviewed, candidate);
            assert!(!report.passes());
            assert!(report.has_input_errors());
            assert!(report.differences.iter().any(|entry| {
                entry.path == "/effect~1sample/source"
                    && entry.category == Category::Input
                    && entry.status == Status::Fail
            }));
        }
    }
    let report = compare("command-grammars.json", &valid, &commands("wrong"));
    assert!(!report.passes());
    assert!(!report.has_input_errors());
}

#[test]
fn command_facts_completeness_and_gaps_are_focused_failures() {
    let reviewed = commands("hotfix");
    for (pointer, replacement) in [
        ("/effect~1sample/value/forms/0", json!("Unresolved")),
        ("/effect~1sample/completeness", json!("Partial")),
        ("/effect~1sample/gaps", json!(["lost form"])),
    ] {
        let mut candidate = reviewed.clone();
        *candidate.pointer_mut(pointer).unwrap() = replacement;
        let report = compare("command-grammars.json", &reviewed, &candidate);
        assert!(!report.passes());
        assert_eq!(report.differences.len(), 1);
        assert!(report.differences[0].path.starts_with(pointer));
        assert_eq!(report.differences[0].category, Category::Answer);
    }
}

fn namespaces() -> Value {
    json!({"count": 2, "gaps": {"UnresolvedReader": 1}, "namespaces": [
        {"owner": "star", "defined_by": ["set_star_flag"], "read_by": []},
        {"owner": "star", "defined_by": [], "read_by": ["has_star_flag", "has_other_flag"]}
    ]})
}

#[test]
fn same_owner_namespace_rows_can_move_with_original_indices_retained() {
    let reviewed = namespaces();
    let mut candidate = reviewed.clone();
    candidate["namespaces"].as_array_mut().unwrap().reverse();
    let report = compare("dynamic-names.json", &reviewed, &candidate);
    assert!(report.passes());
    assert_eq!(report.differences.len(), 2);
    assert!(
        report
            .differences
            .iter()
            .all(|entry| entry.category == Category::Ordering)
    );
    assert_eq!(report.differences[0].path, "/namespaces/0");
    assert!(report.differences[0].note.contains("/namespaces/1"));
    assert_eq!(
        report.differences[0].reviewed,
        Some(reviewed["namespaces"][0].clone())
    );
    assert_eq!(
        report.differences[0].reviewed,
        report.differences[0].candidate
    );
}

#[test]
fn namespace_roles_nested_order_counts_gaps_and_duplicate_multiplicity_fail() {
    let reviewed = namespaces();
    let mut changes = Vec::new();
    for (pointer, value) in [
        ("/count", json!(3)),
        ("/gaps/UnresolvedReader", json!(2)),
        ("/namespaces/0/defined_by", json!(["different_flag"])),
        (
            "/namespaces/1/read_by",
            json!(["has_other_flag", "has_star_flag"]),
        ),
    ] {
        let mut candidate = reviewed.clone();
        *candidate.pointer_mut(pointer).unwrap() = value;
        changes.push(candidate);
    }
    let mut duplicate = reviewed.clone();
    duplicate["namespaces"][1] = reviewed["namespaces"][0].clone();
    changes.push(duplicate);
    let mut extra = reviewed.clone();
    extra["namespaces"]
        .as_array_mut()
        .unwrap()
        .push(reviewed["namespaces"][0].clone());
    changes.push(extra);
    for candidate in changes {
        let report = compare("dynamic-names.json", &reviewed, &candidate);
        assert!(!report.passes());
        assert!(
            report
                .differences
                .iter()
                .any(|entry| entry.status == Status::Fail && entry.category == Category::Answer)
        );
    }
}

#[test]
fn recursive_paths_distinguish_absent_and_null_and_keep_array_order() {
    let reviewed = json!({"a/b~c": {"gone": null}, "array": [1, 2]});
    let candidate = json!({"a/b~c": {"added": null}, "array": [2, 1]});
    let report = compare("references.json", &reviewed, &candidate);
    assert!(!report.passes());
    assert_eq!(
        report
            .differences
            .iter()
            .map(|entry| entry.path.as_str())
            .collect::<Vec<_>>(),
        ["/a~1b~0c/added", "/a~1b~0c/gone", "/array/0", "/array/1"]
    );
    assert_eq!(report.differences[0].reviewed, None);
    assert_eq!(report.differences[0].candidate, Some(Value::Null));
    assert_eq!(report.differences[1].reviewed, Some(Value::Null));
    assert_eq!(report.differences[1].candidate, None);
    assert!(report.full_text().contains("<absent>"));
    assert_eq!(report, compare("references.json", &reviewed, &candidate));
}

#[test]
fn byte_comparators_fail_on_layout_while_structural_comparators_ignore_it() {
    let value = namespaces();
    let compact = serde_json::to_vec(&value).unwrap();
    let pretty = serde_json::to_vec_pretty(&value).unwrap();
    let report = compare_static(
        &build("hotfix"),
        "fields-traditions.json",
        &compact,
        &pretty,
    );
    assert!(!report.passes());
    assert_eq!(report.differences[0].category, Category::Layout);
    assert_ne!(
        report.differences[0].reviewed,
        report.differences[0].candidate
    );
    assert!(report.full_text().contains("near_first_difference"));
    assert!(compare_static(&build("hotfix"), "dynamic-names.json", &compact, &pretty).passes());
}

#[test]
fn malformed_json_and_missing_shapes_are_input_errors() {
    for (file, reviewed, candidate) in [
        ("registries.json", &b"["[..], &b"[]"[..]),
        ("dynamic-names.json", &b"{}"[..], &b"{}"[..]),
        ("command-grammars.json", &b"[]"[..], &b"[]"[..]),
    ] {
        let report = compare_static(&build("hotfix"), file, reviewed, candidate);
        assert!(!report.passes());
        assert!(report.has_input_errors());
        assert!(report.full_text().contains(file));
    }
    assert!(!file_difference("missing.json", true, false, "missing candidate").passes());
}

fn duration_report(reviewed: &Value, candidate: &Value) -> Report {
    compare_durations(
        &build("hotfix"),
        "duration-m452/live.json",
        &serde_json::to_vec(reviewed).unwrap(),
        &serde_json::to_vec(candidate).unwrap(),
    )
}

#[test]
fn fresh_durations_compare_behavior_without_transferring_historical_evidence() {
    let reviewed = json!({"build": "release", "cases": {"timed_flag/days": {"count": 7, "completeness": "Complete", "gaps": []}}});
    let mut candidate = reviewed.clone();
    candidate["build"] = json!("hotfix");
    let report = duration_report(&reviewed, &candidate);
    assert!(report.passes());
    assert_eq!(report.differences[0].category, Category::Provenance);
    assert!(
        report.differences[0]
            .note
            .contains("historical live observations do not apply")
    );
    candidate["cases"]["timed_flag/days"]["count"] = json!(8);
    let report = duration_report(&reviewed, &candidate);
    assert!(!report.passes());
    assert_eq!(report.differences[1].path, "/cases/timed_flag~1days/count");
    assert_eq!(report.differences[1].reviewed, Some(json!(7)));
    assert_eq!(report.differences[1].candidate, Some(json!(8)));
}

#[test]
fn duration_stamp_and_cases_are_required() {
    for candidate in [
        json!({"build": "wrong", "cases": {}}),
        json!({"cases": {}}),
        json!({"build": "hotfix"}),
    ] {
        assert!(!duration_report(&json!({"build": "release", "cases": {}}), &candidate).passes());
    }
    assert!(
        duration_report(&json!({"build": "release"}), &json!({"build": "hotfix"}))
            .has_input_errors()
    );
}

fn tracked_on_actions() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/expected/m452/on-actions.json"
    );
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

#[test]
fn callback_answers_compare_by_name_and_gap_subject() {
    let reviewed = tracked_on_actions();
    let paths = |candidate: &Value| {
        compare("on-actions.json", &reviewed, candidate)
            .differences
            .into_iter()
            .map(|entry| (entry.category, entry.path))
            .collect::<Vec<_>>()
    };

    let mut removed_gap = reviewed.clone();
    removed_gap["gaps"].as_array_mut().unwrap().remove(1);
    assert_eq!(
        paths(&removed_gap),
        [(
            Category::Answer,
            "/gaps/UnresolvedPath/answer_item/empire_init_add_technologies".to_owned()
        )]
    );

    let mut changed_entry = reviewed.clone();
    changed_entry["names"]["on_add_to_council"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert_eq!(
        paths(&changed_entry),
        [(Category::Answer, "/names/on_add_to_council".to_owned())]
    );

    let mut reordered = reviewed.clone();
    reordered["gaps"].as_array_mut().unwrap().swap(0, 1);
    assert_eq!(paths(&reordered), [(Category::Ordering, String::new())]);
    assert!(!compare("on-actions.json", &reviewed, &reordered).passes());
}
