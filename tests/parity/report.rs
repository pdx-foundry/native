//! Bounded terminal rendering and preservation of complete difference reports.
use super::{Report, Status};
use serde_json::Value;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};

const LINE_LIMIT: usize = 80;
const WIDTH_LIMIT: usize = 240;

impl Report {
    /// Render every entry and both full values, without terminal limits.
    pub fn full_text(&self) -> String {
        let failures = self
            .differences
            .iter()
            .filter(|entry| entry.status == Status::Fail)
            .count();
        let skips = self
            .differences
            .iter()
            .filter(|entry| entry.status == Status::Skip)
            .count();
        let permitted = self.differences.len() - failures - skips;
        let mut text = format!(
            "Parity: {failures} failing differences, {permitted} permitted differences, {skips} skips\n"
        );
        for entry in &self.differences {
            let status = match entry.status {
                Status::Pass => "PASS",
                Status::Fail => "FAIL",
                Status::Skip => "SKIP",
            };
            let path = if entry.path.is_empty() {
                "(root)"
            } else {
                &entry.path
            };
            writeln!(text, "{status} {:?} {} {path}", entry.category, entry.file).unwrap();
            writeln!(
                text,
                "  reviewed: {}",
                display_value(entry.reviewed.as_ref())
            )
            .unwrap();
            writeln!(
                text,
                "  candidate: {}",
                display_value(entry.candidate.as_ref())
            )
            .unwrap();
            if !entry.note.is_empty() {
                writeln!(text, "  {}", entry.note).unwrap();
            }
        }
        text
    }

    /// Render bounded terminal output and save the complete report if any output is truncated.
    pub fn render_and_save(&self) -> Result<String, Box<dyn std::error::Error>> {
        self.render_and_save_in(&Path::new(env!("CARGO_MANIFEST_DIR")).join(".local/parity"))
    }

    fn render_and_save_in(&self, directory: &Path) -> Result<String, Box<dyn std::error::Error>> {
        let full = self.full_text();
        let lines: Vec<_> = full.lines().collect();
        let truncated =
            lines.len() > LINE_LIMIT || lines.iter().any(|line| line.chars().count() > WIDTH_LIMIT);
        if !truncated {
            return Ok(full);
        }
        let path = save_full_report(directory, &full)?;
        clip_terminal_report(&full, &path)
    }
}

fn save_full_report(directory: &Path, full: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let expected = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/expected")
        .canonicalize()?;
    let mut existing = directory;
    while !existing.try_exists()? {
        existing = existing
            .parent()
            .ok_or("report directory needs an existing ancestor")?;
    }
    if existing.canonicalize()?.starts_with(&expected) {
        return Err("difference reports must be outside tests/expected".into());
    }
    std::fs::create_dir_all(directory)?;
    // Resolve again before writing: a concurrent replacement must not redirect into expectations.
    let directory = directory.canonicalize()?;
    if directory.starts_with(expected) {
        return Err("difference reports must be outside tests/expected".into());
    }
    let mut file = tempfile::Builder::new()
        .prefix("differences-")
        .suffix(".txt")
        .tempfile_in(directory)?;
    file.write_all(full.as_bytes())?;
    file.flush()?;
    let (_, path) = file.keep()?;
    Ok(path)
}

fn clip_terminal_report(full: &str, path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let footer = format!("Truncated; full report: {}", path.display());
    if footer.chars().count() > WIDTH_LIMIT {
        return Err(format!(
            "full report path exceeds terminal width: {}",
            path.display()
        )
        .into());
    }
    let mut terminal = String::new();
    for line in full.lines().take(LINE_LIMIT - 1) {
        if line.chars().count() > WIDTH_LIMIT {
            let clipped: String = line.chars().take(WIDTH_LIMIT - 1).collect();
            writeln!(terminal, "{clipped}…").unwrap();
        } else {
            writeln!(terminal, "{line}").unwrap();
        }
    }
    writeln!(terminal, "{footer}").unwrap();
    Ok(terminal)
}

fn display_value(value: Option<&Value>) -> String {
    value.map_or_else(|| "<absent>".into(), Value::to_string)
}

#[cfg(test)]
mod tests {
    // The custom live harness omits test functions, so their imports must stay inside each test.
    #[test]
    fn truncation_preserves_complete_values_in_fresh_reports() {
        use super::super::{Category, Difference};
        use super::*;
        let directory = tempfile::tempdir().unwrap();
        let report = Report {
            differences: (0..40)
                .map(|index| Difference {
                    file: "sample.json".into(),
                    path: format!("/{index}"),
                    category: Category::Answer,
                    status: Status::Fail,
                    reviewed: Some(Value::Null),
                    candidate: Some(Value::String("é".repeat(400))),
                    note: String::new(),
                })
                .collect(),
        };
        let first = report.render_and_save_in(directory.path()).unwrap();
        let second = report.render_and_save_in(directory.path()).unwrap();
        assert!(first.lines().count() <= LINE_LIMIT);
        assert!(
            first
                .lines()
                .all(|line| line.chars().count() <= WIDTH_LIMIT)
        );
        assert!(first.contains('…'));
        assert_ne!(first, second);
        for entry in std::fs::read_dir(directory.path()).unwrap() {
            assert_eq!(
                std::fs::read_to_string(entry.unwrap().path()).unwrap(),
                report.full_text()
            );
        }
    }

    #[test]
    fn short_reports_do_not_create_files_and_write_failures_are_visible() {
        use super::super::{Category, Difference};
        use super::*;
        let directory = tempfile::tempdir().unwrap();
        let absent = directory.path().join("absent");
        assert_eq!(
            Report::default().render_and_save_in(&absent).unwrap(),
            Report::default().full_text()
        );
        assert!(!absent.exists());
        let report = Report {
            differences: vec![Difference {
                file: "sample.json".into(),
                path: "/value".into(),
                category: Category::Answer,
                status: Status::Fail,
                reviewed: None,
                candidate: Some(Value::String("x".repeat(400))),
                note: String::new(),
            }],
        };
        let blocked = directory.path().join("file");
        std::fs::write(&blocked, "keep").unwrap();
        assert!(report.render_and_save_in(&blocked).is_err());
        assert_eq!(std::fs::read_to_string(blocked).unwrap(), "keep");
    }

    #[cfg(unix)]
    #[test]
    fn reports_reject_symlink_routes_into_expected_before_creating_directories() {
        use super::super::{Category, Difference};
        use super::*;

        let temporary = tempfile::tempdir().unwrap();
        let alias = temporary.path().join("expected-alias");
        let expected = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/expected");
        std::os::unix::fs::symlink(&expected, &alias).unwrap();
        let new_directory = alias.join(temporary.path().file_name().unwrap());
        let report = Report {
            differences: vec![Difference {
                file: "sample.json".into(),
                path: "/value".into(),
                category: Category::Answer,
                status: Status::Fail,
                reviewed: None,
                candidate: Some(Value::String("x".repeat(400))),
                note: String::new(),
            }],
        };
        assert!(report.render_and_save_in(&new_directory).is_err());
        assert!(!new_directory.exists());
    }
}
