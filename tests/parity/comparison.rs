//! Comparison authority for offline candidates and static/live parity assertions.
#[path = "report.rs"]
mod report;

use pdx_native::{BuildId, Source};
use serde_json::Value;
use std::collections::BTreeSet;

/// Why an entry differs, independently of whether parity permits it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Answer,
    Provenance,
    Ordering,
    Layout,
    Files,
    Input,
}

/// The comparison rule's disposition of an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Pass,
    Fail,
}

/// One difference or applicability notice. `None` means absent, not JSON null.
#[derive(Debug, Clone, PartialEq)]
pub struct Difference {
    pub file: String,
    pub path: String,
    pub category: Category,
    pub status: Status,
    pub reviewed: Option<Value>,
    pub candidate: Option<Value>,
    pub note: String,
}

/// Differences and notices in deterministic comparison order.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Report {
    pub differences: Vec<Difference>,
}

impl Report {
    /// Whether all checked entries pass; skipped evidence is never counted as checked.
    pub fn passes(&self) -> bool {
        self.differences
            .iter()
            .all(|entry| entry.status != Status::Fail)
    }

    /// Malformed JSON or comparison shapes are input errors rather than changed answers.
    pub fn has_input_errors(&self) -> bool {
        self.differences
            .iter()
            .any(|entry| entry.category == Category::Input)
    }

    /// Append another file's report without changing its comparison outcome.
    pub fn extend(&mut self, report: Report) {
        self.differences.extend(report.differences);
    }

    fn record_failure(
        &mut self,
        file: &str,
        path: &str,
        reviewed: Option<&Value>,
        candidate: Option<&Value>,
    ) {
        let category = if path.split('/').any(|part| part == "source") {
            Category::Provenance
        } else {
            Category::Answer
        };
        self.notice(file, path, category, Status::Fail, reviewed, candidate, "");
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "Keep location, disposition and both compared values explicit."
    )]
    fn notice(
        &mut self,
        file: &str,
        path: &str,
        category: Category,
        status: Status,
        reviewed: Option<&Value>,
        candidate: Option<&Value>,
        note: &str,
    ) {
        self.differences.push(Difference {
            file: file.to_owned(),
            path: path.to_owned(),
            category,
            status,
            reviewed: reviewed.cloned(),
            candidate: candidate.cloned(),
            note: note.to_owned(),
        });
    }
}

/// Compare a static selection using its existing byte, source, or full-row rule.
pub fn compare_static(build: &BuildId, file: &str, reviewed: &[u8], candidate: &[u8]) -> Report {
    let mut report = Report::default();
    let Some((mut reviewed_value, mut candidate_value)) =
        parse_pair(&mut report, file, reviewed, candidate)
    else {
        return report;
    };

    match file {
        "command-grammars.json" => {
            validate_commands(&mut report, build, file, &reviewed_value, &candidate_value);
            // Only the build stamp may differ; other provenance remains strict.
            remove_command_builds(&mut reviewed_value);
            remove_command_builds(&mut candidate_value);
            diff_json(
                &mut report,
                file,
                "",
                Some(&reviewed_value),
                Some(&candidate_value),
            );
        }
        "dynamic-names.json" => {
            compare_namespaces(&mut report, file, &reviewed_value, &candidate_value);
            for value in [&mut reviewed_value, &mut candidate_value] {
                if let Some(object) = value.as_object_mut() {
                    object.remove("namespaces");
                }
            }
            diff_json(
                &mut report,
                file,
                "",
                Some(&reviewed_value),
                Some(&candidate_value),
            );
        }
        _ => compare_bytes(
            &mut report,
            file,
            reviewed,
            candidate,
            &reviewed_value,
            &candidate_value,
        ),
    }
    report
}

/// Compare fresh duration cases with retained behavior, checking the fresh run's build.
pub fn compare_durations(build: &BuildId, file: &str, reviewed: &[u8], candidate: &[u8]) -> Report {
    let mut report = Report::default();
    let Some((reviewed, candidate)) = parse_pair(&mut report, file, reviewed, candidate) else {
        return report;
    };
    let current = serde_json::json!(build);
    let candidate_is_current = candidate.get("build") == Some(&current);
    if !candidate_is_current {
        report.notice(
            file,
            "/build",
            Category::Provenance,
            Status::Fail,
            reviewed.get("build"),
            candidate.get("build"),
            &format!("candidate build must match the supplied current build {current}"),
        );
    }
    if reviewed.get("build").and_then(Value::as_str).is_none() {
        report.notice(
            file,
            "/build",
            Category::Input,
            Status::Fail,
            reviewed.get("build"),
            candidate.get("build"),
            "reviewed duration build must be a string",
        );
    } else if candidate_is_current && reviewed.get("build") != candidate.get("build") {
        let note = "fresh behavior compared with retained cases; \
            historical live observations do not apply to the current exact build";
        report.notice(
            file,
            "/build",
            Category::Provenance,
            Status::Pass,
            reviewed.get("build"),
            candidate.get("build"),
            note,
        );
    }
    for (side, value) in [("reviewed", &reviewed), ("candidate", &candidate)] {
        if value.get("cases").and_then(Value::as_object).is_none() {
            report.notice(
                file,
                "/cases",
                Category::Input,
                Status::Fail,
                reviewed.get("cases"),
                candidate.get("cases"),
                &format!("{side} duration cases must be an object"),
            );
        }
    }
    diff_json(
        &mut report,
        file,
        "/cases",
        reviewed.get("cases"),
        candidate.get("cases"),
    );
    report
}

/// A missing or unexpected member of the static candidate tree fails parity.
pub fn file_difference(file: &str, reviewed: bool, candidate: bool, note: &str) -> Report {
    let mut report = Report::default();
    report.notice(
        file,
        "",
        Category::Files,
        Status::Fail,
        reviewed.then(|| Value::String("present".into())).as_ref(),
        candidate.then(|| Value::String("present".into())).as_ref(),
        note,
    );
    report
}

fn parse_pair(
    report: &mut Report,
    file: &str,
    reviewed: &[u8],
    candidate: &[u8],
) -> Option<(Value, Value)> {
    let reviewed = parse_json(report, file, "reviewed", reviewed);
    let candidate = parse_json(report, file, "candidate", candidate);
    Some((reviewed?, candidate?))
}

fn parse_json(report: &mut Report, file: &str, side: &str, bytes: &[u8]) -> Option<Value> {
    match serde_json::from_slice(bytes) {
        Ok(value) => Some(value),
        Err(error) => {
            report.notice(
                file,
                "",
                Category::Input,
                Status::Fail,
                None,
                None,
                &format!("{side} JSON: {error}"),
            );
            None
        }
    }
}

fn validate_source(
    report: &mut Report,
    file: &str,
    path: &str,
    side: &str,
    value: Option<&Value>,
    reviewed: Option<&Value>,
    candidate: Option<&Value>,
) -> Option<Source> {
    match serde_json::from_value(value.cloned().unwrap_or(Value::Null)) {
        Ok(source) => Some(source),
        Err(error) => {
            report.notice(
                file,
                path,
                source_error_category(&error),
                Status::Fail,
                reviewed,
                candidate,
                &format!("invalid {side} source: {error}"),
            );
            None
        }
    }
}

fn source_error_category(error: &(dyn std::error::Error + 'static)) -> Category {
    if error.is::<serde_json::Error>() {
        Category::Input
    } else {
        Category::Provenance
    }
}

fn validate_commands(
    report: &mut Report,
    build: &BuildId,
    file: &str,
    reviewed: &Value,
    candidate: &Value,
) {
    for (side, value) in [("reviewed", reviewed), ("candidate", candidate)] {
        if !value.is_object() {
            report.notice(
                file,
                "",
                Category::Input,
                Status::Fail,
                Some(reviewed),
                Some(candidate),
                &format!("{side} command selection must be an object"),
            );
        }
    }
    if let Some(answers) = reviewed.as_object() {
        for (subject, answer) in answers {
            let previous_source = answer.get("source");
            let current_source = candidate
                .get(subject)
                .and_then(|answer| answer.get("source"));
            let path = format!("{}/source", child_path("", subject));
            validate_source(
                report,
                file,
                &path,
                "reviewed",
                previous_source,
                previous_source,
                current_source,
            );
        }
    }
    if let Some(answers) = candidate.as_object() {
        for (subject, answer) in answers {
            let previous_source = reviewed
                .get(subject)
                .and_then(|answer| answer.get("source"));
            let current_source = answer.get("source");
            let path = format!("{}/source", child_path("", subject));
            let Some(source) = validate_source(
                report,
                file,
                &path,
                "candidate",
                current_source,
                previous_source,
                current_source,
            ) else {
                continue;
            };
            let previous_build = previous_source.and_then(|source| source.get("build"));
            let current_build = current_source.and_then(|source| source.get("build"));
            if &source.build != build {
                let note = format!(
                    "candidate source does not match the supplied current build {}",
                    serde_json::json!(build)
                );
                report.notice(
                    file,
                    &format!("{path}/build"),
                    Category::Provenance,
                    Status::Fail,
                    previous_build,
                    current_build,
                    &note,
                );
            } else if previous_build.is_some() && previous_build != current_build {
                report.notice(
                    file,
                    &format!("{path}/build"),
                    Category::Provenance,
                    Status::Pass,
                    previous_build,
                    current_build,
                    "reviewed build differs; candidate stamp is checked against the current build",
                );
            }
        }
    }
}

fn remove_command_builds(value: &mut Value) {
    if let Some(answers) = value.as_object_mut() {
        for answer in answers.values_mut() {
            if let Some(source) = answer.get_mut("source").and_then(Value::as_object_mut) {
                source.remove("build");
            }
        }
    }
}

fn compare_namespaces(report: &mut Report, file: &str, reviewed: &Value, candidate: &Value) {
    let (Some(reviewed_rows), Some(candidate_rows)) = (
        reviewed.get("namespaces").and_then(Value::as_array),
        candidate.get("namespaces").and_then(Value::as_array),
    ) else {
        report.notice(
            file,
            "/namespaces",
            Category::Input,
            Status::Fail,
            reviewed.get("namespaces"),
            candidate.get("namespaces"),
            "dynamic namespaces must be arrays",
        );
        return;
    };
    // Internal store IDs depend on relocated addresses. Owners alone do not identify namespace rows.
    let mut used = vec![false; candidate_rows.len()];
    for (index, row) in reviewed_rows.iter().enumerate() {
        let matched = candidate_rows
            .iter()
            .enumerate()
            .position(|(other, value)| !used[other] && value == row);
        if let Some(other) = matched {
            used[other] = true;
            if index != other {
                report.notice(
                    file,
                    &format!("/namespaces/{index}"),
                    Category::Ordering,
                    Status::Pass,
                    Some(row),
                    Some(&candidate_rows[other]),
                    &format!("complete row moved to candidate /namespaces/{other}"),
                );
            }
        } else {
            report.record_failure(file, &format!("/namespaces/{index}"), Some(row), None);
        }
    }
    for (index, row) in candidate_rows.iter().enumerate() {
        if !used[index] {
            report.record_failure(file, &format!("/namespaces/{index}"), None, Some(row));
        }
    }
}

fn compare_bytes(
    report: &mut Report,
    file: &str,
    reviewed: &[u8],
    candidate: &[u8],
    reviewed_value: &Value,
    candidate_value: &Value,
) {
    if reviewed == candidate {
        return;
    }
    if reviewed_value == candidate_value {
        let offset = reviewed
            .iter()
            .zip(candidate)
            .position(|(left, right)| left != right)
            .unwrap_or(reviewed.len().min(candidate.len()));
        let note = format!(
            "JSON values match, but byte equality is required; first difference at byte {offset}"
        );
        report.notice(
            file,
            "",
            Category::Layout,
            Status::Fail,
            Some(&byte_window(reviewed, offset)),
            Some(&byte_window(candidate, offset)),
            &note,
        );
        return;
    }
    diff_json(
        report,
        file,
        "",
        Some(reviewed_value),
        Some(candidate_value),
    );
}

fn byte_window(bytes: &[u8], offset: usize) -> Value {
    let start = offset.saturating_sub(24);
    let end = (offset + 24).min(bytes.len());
    serde_json::json!({"byte_length": bytes.len(), "start_byte": start,
        "near_first_difference": String::from_utf8_lossy(&bytes[start..end])})
}

fn child_path(path: &str, key: &str) -> String {
    format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"))
}

fn diff_json(
    report: &mut Report,
    file: &str,
    path: &str,
    reviewed: Option<&Value>,
    candidate: Option<&Value>,
) {
    if reviewed == candidate {
        return;
    }
    match (reviewed, candidate) {
        (Some(Value::Object(left)), Some(Value::Object(right))) => {
            let keys: BTreeSet<_> = left.keys().chain(right.keys()).collect();
            for key in keys {
                diff_json(
                    report,
                    file,
                    &child_path(path, key),
                    left.get(key),
                    right.get(key),
                );
            }
        }
        (Some(Value::Array(left)), Some(Value::Array(right))) => {
            for index in 0..left.len().max(right.len()) {
                diff_json(
                    report,
                    file,
                    &format!("{path}/{index}"),
                    left.get(index),
                    right.get(index),
                );
            }
        }
        _ => report.record_failure(file, path, reviewed, candidate),
    }
}
