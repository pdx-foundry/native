//! Generate static parity candidates for review. Starts no game and never accepts an answer.
//! Modes: --out NEW_DIRECTORY, --compare REVIEWED_DIR CANDIDATE_DIR --build BUILD_ID,
//! and --compare-durations REVIEWED_FILE CANDIDATE_FILE --build BUILD_ID.
//! STELLARIS_PATH names the installation for generation. Comparison is fully offline.
#[path = "../tests/parity/mod.rs"]
mod parity;

use parity::comparison::{Report, compare_durations, compare_static, file_difference};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};

fn main() -> std::process::ExitCode {
    match run() {
        Ok(code) => std::process::ExitCode::from(code),
        Err(error) => {
            eprintln!("expected: {error}");
            std::process::ExitCode::from(2)
        }
    }
}

const USAGE: &str = concat!(
    "usage: expected --out NEW_DIRECTORY (set STELLARIS_PATH)\n",
    "       expected --compare REVIEWED_DIR CANDIDATE_DIR --build BUILD_ID\n",
    "       expected --compare-durations REVIEWED_FILE CANDIDATE_FILE --build BUILD_ID",
);

#[derive(Debug, PartialEq)]
enum Command {
    Generate(PathBuf),
    Compare {
        reviewed: PathBuf,
        candidate: PathBuf,
        build: pdx_native::BuildId,
    },
    CompareDurations {
        reviewed: PathBuf,
        candidate: PathBuf,
        build: pdx_native::BuildId,
    },
}

fn parse_args(args: &[std::ffi::OsString]) -> parity::Result<Command> {
    match args {
        [flag, directory] if flag == "--out" => Ok(Command::Generate(PathBuf::from(directory))),
        [flag, reviewed, candidate, build_flag, build] if build_flag == "--build" => {
            let identity = build
                .to_str()
                .filter(|value| !value.is_empty())
                .ok_or(USAGE)?;
            let build = serde_json::from_value(serde_json::json!(identity))?;
            let reviewed = PathBuf::from(reviewed);
            let candidate = PathBuf::from(candidate);
            match flag.to_str() {
                Some("--compare") => Ok(Command::Compare {
                    reviewed,
                    candidate,
                    build,
                }),
                Some("--compare-durations") => Ok(Command::CompareDurations {
                    reviewed,
                    candidate,
                    build,
                }),
                _ => Err(USAGE.into()),
            }
        }
        _ => Err(USAGE.into()),
    }
}

fn run() -> parity::Result<u8> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let report = match parse_args(&args)? {
        Command::Generate(directory) => {
            generate(&directory)?;
            return Ok(0);
        }
        Command::Compare {
            reviewed,
            candidate,
            build,
        } => compare_tree(&build, &reviewed, &candidate)?,
        Command::CompareDurations {
            reviewed,
            candidate,
            build,
        } => compare_durations(
            &build,
            &reviewed.display().to_string(),
            &std::fs::read(&reviewed)?,
            &std::fs::read(&candidate)?,
        ),
    };
    print!("{}", report.render_and_save()?);
    Ok(comparison_exit_code(&report))
}

fn comparison_exit_code(report: &Report) -> u8 {
    if report.has_input_errors() {
        2
    } else if report.passes() {
        0
    } else {
        1
    }
}

fn tree_names(directory: &Path) -> parity::Result<BTreeSet<String>> {
    std::fs::read_dir(directory)?
        .map(|entry| {
            let entry = entry?;
            entry
                .file_name()
                .into_string()
                .map_err(|_| "parity filenames must be UTF-8".into())
        })
        .collect()
}

fn compare_tree(
    build: &pdx_native::BuildId,
    reviewed: &Path,
    candidate: &Path,
) -> parity::Result<Report> {
    let reviewed_names = tree_names(reviewed)?;
    let candidate_names = tree_names(candidate)?;
    let names: BTreeSet<_> = reviewed_names
        .iter()
        .chain(&candidate_names)
        .map(String::as_str)
        .chain(parity::FILES.iter().copied())
        .collect();
    let mut report = Report::default();
    for name in names {
        let in_reviewed = reviewed_names.contains(name);
        let in_candidate = candidate_names.contains(name);
        if !parity::FILES.contains(&name) {
            report.extend(file_difference(
                name,
                in_reviewed,
                in_candidate,
                "unexpected file outside the static parity selection",
            ));
        } else if !in_reviewed || !in_candidate {
            report.extend(file_difference(
                name,
                in_reviewed,
                in_candidate,
                "required parity file missing",
            ));
        } else {
            report.extend(compare_static(
                build,
                name,
                &std::fs::read(reviewed.join(name))?,
                &std::fs::read(candidate.join(name))?,
            ));
        }
    }
    Ok(report)
}

fn generate(directory: &Path) -> parity::Result<()> {
    let directory = prepare_output(directory)?;
    let installation = std::env::var_os("STELLARIS_PATH")
        .ok_or("set STELLARIS_PATH to the installation or executable")?;
    let native = pdx_native::Native::open(installation)?;
    for name in parity::FILES {
        let bytes = parity::candidate(&native, name)?;
        if *name == "field-storage-sdk533.json" {
            let observed = serde_json::from_slice(&bytes)?;
            let report = parity::comparison::historical_storage_report(&native.build(), &observed);
            eprint!("{}", report.render_and_save()?);
        }
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join(name))?;
        file.write_all(&bytes)?;
        eprintln!("wrote {name}");
    }
    eprintln!("Candidates only: review the diff before copying any file into tests/expected/m45.");
    Ok(())
}

/// Require a fresh directory and resolve its existing parent before any writes.
/// This rejects symlink aliases and prevents truncating an existing file or hard link.
fn prepare_output(path: &Path) -> parity::Result<PathBuf> {
    let name = path.file_name().ok_or("output must name a new directory")?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = parent.canonicalize()?;
    let expected = parity::expected_directory()
        .parent()
        .unwrap()
        .canonicalize()?;
    if parent.starts_with(&expected) {
        return Err("candidate output must be outside tests/expected".into());
    }
    let output = parent.join(name);
    std::fs::create_dir(&output)?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparison_arguments_need_an_explicit_build_and_no_installation() {
        let args = |values: &[&str]| {
            values
                .iter()
                .map(std::ffi::OsString::from)
                .collect::<Vec<_>>()
        };
        for flag in ["--compare", "--compare-durations"] {
            assert!(
                parse_args(&args(&[flag, "reviewed", "candidate", "--build", "hotfix"])).is_ok()
            );
            assert!(parse_args(&args(&[flag, "reviewed", "candidate"])).is_err());
            assert!(parse_args(&args(&[flag, "reviewed", "candidate", "--build", ""])).is_err());
        }
        assert!(
            parse_args(&args(&[
                "--unknown",
                "reviewed",
                "candidate",
                "--build",
                "hotfix"
            ]))
            .is_err()
        );
    }

    #[test]
    fn tree_comparison_uses_file_rules_and_reports_missing_and_extra_files() {
        let temporary = tempfile::tempdir().unwrap();
        let candidate = temporary.path();
        let reviewed = parity::expected_directory();
        for name in parity::FILES {
            std::fs::copy(reviewed.join(name), candidate.join(name)).unwrap();
        }
        let commands: serde_json::Value =
            serde_json::from_slice(&std::fs::read(reviewed.join("command-grammars.json")).unwrap())
                .unwrap();
        let build = serde_json::from_value(
            commands.as_object().unwrap().values().next().unwrap()["source"]["build"].clone(),
        )
        .unwrap();
        let report = compare_tree(&build, &reviewed, candidate).unwrap();
        assert!(report.passes(), "{}", report.full_text());
        assert_eq!(comparison_exit_code(&report), 0);

        std::fs::write(candidate.join("registries.json"), "[\"changed\"]").unwrap();
        let report = compare_tree(&build, &reviewed, candidate).unwrap();
        assert!(!report.passes());
        assert_eq!(comparison_exit_code(&report), 1);
        assert_eq!(
            report
                .differences
                .iter()
                .filter(|entry| entry.status == parity::comparison::Status::Fail)
                .map(|entry| entry.file.as_str())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["registries.json"])
        );

        std::fs::remove_file(candidate.join("registries.json")).unwrap();
        std::fs::write(candidate.join("extra.json"), "null").unwrap();
        let report = compare_tree(&build, &reviewed, candidate).unwrap();
        assert!(
            report
                .differences
                .iter()
                .any(|entry| entry.file == "registries.json"
                    && entry.category == parity::comparison::Category::Files)
        );
        assert!(
            report
                .differences
                .iter()
                .any(|entry| entry.file == "extra.json"
                    && entry.category == parity::comparison::Category::Files)
        );

        std::fs::write(candidate.join("registries.json"), "malformed").unwrap();
        assert_eq!(
            comparison_exit_code(&compare_tree(&build, &reviewed, candidate).unwrap()),
            2
        );
        assert!(compare_tree(&build, &reviewed, &candidate.join("missing")).is_err());

        std::fs::copy(
            reviewed.join("registries.json"),
            candidate.join("registries.json"),
        )
        .unwrap();
        std::fs::remove_file(candidate.join("extra.json")).unwrap();
        for source in [serde_json::json!([]), serde_json::json!({"build": build})] {
            let mut malformed = commands.clone();
            malformed
                .as_object_mut()
                .unwrap()
                .values_mut()
                .next()
                .unwrap()["source"] = source;
            std::fs::write(
                candidate.join("command-grammars.json"),
                serde_json::to_vec(&malformed).unwrap(),
            )
            .unwrap();
            for (reviewed, candidate) in [
                (reviewed.as_path(), candidate),
                (candidate, reviewed.as_path()),
            ] {
                let report = compare_tree(&build, reviewed, candidate).unwrap();
                assert!(!report.passes());
                assert_eq!(comparison_exit_code(&report), 2);
            }
        }
        let mut wrong_build = commands;
        wrong_build
            .as_object_mut()
            .unwrap()
            .values_mut()
            .next()
            .unwrap()["source"]["build"] = serde_json::json!("wrong");
        std::fs::write(
            candidate.join("command-grammars.json"),
            serde_json::to_vec(&wrong_build).unwrap(),
        )
        .unwrap();
        assert_eq!(
            comparison_exit_code(&compare_tree(&build, &reviewed, candidate).unwrap()),
            1
        );
    }

    #[test]
    fn refuses_tracked_output_and_existing_directories() {
        assert!(prepare_output(&parity::expected_directory()).is_err());
        assert!(prepare_output(&parity::expected_directory().join("new-candidate")).is_err());
        let temporary = tempfile::tempdir().unwrap();
        assert!(prepare_output(temporary.path()).is_err());
        let candidate = temporary.path().join("candidate");
        assert_eq!(
            prepare_output(&candidate).unwrap(),
            candidate.canonicalize().unwrap()
        );
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlink_routes_into_expected() {
        let temporary = tempfile::tempdir().unwrap();
        let link = temporary.path().join("alias");
        std::os::unix::fs::symlink(parity::expected_directory(), &link).unwrap();
        assert!(prepare_output(&link.join("new-candidate")).is_err());
        assert!(prepare_output(&link).is_err());
    }
}
