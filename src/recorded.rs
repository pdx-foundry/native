//! Recorded answers: JSON files that stand in for an installation and a game in tests.
//!
//! One file holds one `Result<Answer<T>, Error>`. `Native::record_answers_to` writes a file for
//! each question as it is answered in a real run. `Native::from_recorded_answers` reads them and
//! starts no process. A file can also be written by hand, for example for a failure case.
//!
//! Layout: `registries.json`, `registry_fields/<registry>.json`, `registry_items/<registry>.json`,
//! where `<registry>` is the content directory, such as `common/traditions`.
use crate::answer::{Answer, Basis, Error};
use serde::{Serialize, de::DeserializeOwned};
use std::path::{Path, PathBuf};

/// Location of one question's file. A subject that could leave the directory is refused.
fn path(root: &Path, question: &str, subject: Option<&str>) -> Result<PathBuf, Error> {
    let mut path = root.join(question);
    if let Some(subject) = subject {
        let plain = |segment: &str| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        };
        let subject = subject.trim_end_matches('/');
        if !subject.split('/').all(plain) {
            return Err(Error::NotRecorded {
                question: format!("{question}/{subject}"),
            });
        }
        path.push(subject);
    }
    path.set_extension("json");
    Ok(path)
}

/// Read one recorded answer. Its basis is always `Recorded`, whatever the file says.
pub(crate) fn read<T: DeserializeOwned>(
    root: &Path,
    question: &str,
    subject: Option<&str>,
) -> Result<Answer<T>, Error> {
    let path = path(root, question, subject)?;
    let named = || match subject {
        Some(subject) => format!("{question}/{subject}"),
        None => question.to_owned(),
    };
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::NotRecorded { question: named() });
        }
        Err(error) => return Err(Error::Recorded(format!("{}: {error}", named()))),
    };
    let recorded: Result<Answer<T>, Error> = serde_json::from_slice(&bytes)
        .map_err(|error| Error::Recorded(format!("{}: {error}", named())))?;
    let mut answer = recorded?;
    answer.source.basis = Basis::Recorded;
    Ok(answer)
}

/// Write one answer or error. A write failure is returned; it never changes the answer itself.
pub(crate) fn write<T: Serialize>(
    root: &Path,
    question: &str,
    subject: Option<&str>,
    answer: &Result<Answer<T>, Error>,
) -> Result<(), Error> {
    let path = path(root, question, subject)?;
    let failed = |error: std::io::Error| Error::Recorded(format!("{}: {error}", path.display()));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(failed)?;
    }
    let mut bytes = serde_json::to_vec_pretty(answer).expect("answers serialize");
    bytes.push(b'\n');
    std::fs::write(&path, bytes).map_err(failed)
}
