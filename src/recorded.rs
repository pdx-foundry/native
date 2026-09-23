//! Recorded answers: JSON files that stand in for an installation and a game in tests.
//!
//! One file holds one `Result<Answer<T>, Error>`. `Native::record_answers_to` writes a file for
//! each question as it is answered in a real run. `Native::from_recorded_answers` reads them and
//! starts no process. A file can also be written by hand, for example for a failure case.
//!
//! Layout: `build.json`, `registries.json`, `registry_fields/<registry>.json`,
//! `registry_items/<registry>.json` and `modifier_families/<registry>.json`, where `<registry>` is
//! the content directory, such as `common/traditions`. The language
//! questions use `<question>.json`, such as `on_actions.json` and `game_rules.json`, and
//! `declarations/<kind>.json`. Fixture answers use
//! `observe_fixture/<files-hash>/<request-hash>.json`; hashes are internal lookup keys, not provenance.
use crate::answer::{Answer, Basis, BuildId, Error};
use serde::{Serialize, de::DeserializeOwned};
use std::io::Write;
use std::path::{Path, PathBuf};

/// One recorded build. Every successful answer must have the same original identity.
#[derive(Debug)]
pub(crate) struct Answers {
    directory: PathBuf,
    pub build: BuildId,
}

impl Answers {
    pub(crate) fn open(directory: PathBuf) -> Result<Self, Error> {
        let path = directory.join("build.json");
        let bytes = std::fs::read(&path)
            .map_err(|error| Error::Recorded(format!("{}: {error}", path.display())))?;
        let build = serde_json::from_slice(&bytes)
            .map_err(|error| Error::Recorded(format!("{}: {error}", path.display())))?;
        Ok(Self { directory, build })
    }

    /// Preserve the original build, and mark the answer as recorded regardless of its file.
    pub(crate) fn read<T: DeserializeOwned>(
        &self,
        question: &str,
        subject: Option<&str>,
    ) -> Result<Answer<T>, Error> {
        let mut answer = read(&self.directory, question, subject)?;
        if answer.source.build != self.build {
            return Err(Error::Recorded(format!(
                "{question}: answer build differs from build.json"
            )));
        }
        answer.source.basis = Basis::Recorded;
        Ok(answer)
    }
}

/// Record the identity even for an error-only run, and refuse to mix builds in one directory.
fn record_build(root: &Path, build: &BuildId) -> Result<(), Error> {
    let path = root.join("build.json");
    let failed = |error: std::io::Error| Error::Recorded(format!("{}: {error}", path.display()));
    std::fs::create_dir_all(root).map_err(failed)?;
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(mut file) => {
            let mut bytes = serde_json::to_vec(build).expect("build identity serializes");
            bytes.push(b'\n');
            file.write_all(&bytes).map_err(failed)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if Answers::open(root.into())?.build != *build {
                return Err(Error::Recorded(
                    "recording build differs from build.json".into(),
                ));
            }
        }
        Err(error) => return Err(failed(error)),
    }
    Ok(())
}

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

fn read<T: DeserializeOwned>(
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
    recorded
}

/// Write one answer or error. A write failure is returned; it never changes the answer itself.
pub(crate) fn write<T: Serialize>(
    root: &Path,
    build: &BuildId,
    question: &str,
    subject: Option<&str>,
    answer: &Result<Answer<T>, Error>,
) -> Result<(), Error> {
    let path = path(root, question, subject)?;
    record_build(root, build)?;
    let failed = |error: std::io::Error| Error::Recorded(format!("{}: {error}", path.display()));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(failed)?;
    }
    let mut bytes = serde_json::to_vec_pretty(answer).expect("answers serialize");
    bytes.push(b'\n');
    std::fs::write(&path, bytes).map_err(failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_only_recordings_keep_the_build_and_refuse_a_different_recorder() {
        let root = tempfile::tempdir().unwrap();
        let build = BuildId("original".into());
        let answer: Result<Answer<Vec<String>>, Error> = Err(Error::BuildChanged);
        write(root.path(), &build, "registries", None, &answer).unwrap();
        let recorded = Answers::open(root.path().into()).unwrap();
        assert_eq!(recorded.build, build);
        assert_eq!(recorded.read::<Vec<String>>("registries", None), answer);
        let different = BuildId("different".into());
        assert!(matches!(
            write(root.path(), &different, "registries", None, &answer),
            Err(Error::Recorded(_))
        ));
        assert_eq!(Answers::open(root.path().into()).unwrap().build, build);
    }
}
