//! Recorded answers: JSON files that stand in for an installation and a game in tests.
//!
//! One file holds one `Result<Answer<T>, Error>`. `Native::record_answers_to` writes a file for
//! each question as it is answered in a real run. `Native::from_recorded_answers` reads them and
//! starts no process. A file can also be written by hand, for example for a failure case.
//!
//! Layout: `build.json`, `registries.json`, `registry_fields/<registry>.json`,
//! `modifier_families/<registry>.json`, `derived_names/<registry>.json`, where `<registry>` is
//! the content directory, such as `common/traditions`. The language
//! questions use `<question>.json`, such as `on_actions.json`, `game_rules.json`, `defines.json`,
//! `dynamic_names.json` and `modifier_nodes.json`, and
//! `declarations/<kind>.json`. Command grammar uses `command_grammar/<kind>/<name>.json`. Fixture
//! answers use `observe_fixture/<files-hash>/<request-hash>.json`; hashes are internal lookup keys,
//! not provenance. The loaded modifier inventory uses `loaded_modifiers.json`, or
//! `loaded_modifiers/<files-hash>/<request-hash>.json` for a session with a fixture.
//!
//! A subject whose every `/`-separated segment holds only `[a-z0-9_]` names its file directly, as
//! above. Any other subject (an uppercase, hyphenated, empty or `..` name, which a caller can
//! pass and a method answers with an error) is recorded as `<operation>/hex-encoded/x<hex>.json`,
//! where `<hex>` is the hex of the whole subject. No plain segment holds the `-` of `hex-encoded`,
//! and the hex is injective, so no subject leaves the root or takes another subject's file. A
//! subject over 124 bytes has an encoded name over the 255-byte file name limit, and recording
//! it fails with `Error::Recorded`. Change this when a caller must record such a subject.
//!
//! Each write goes to its own `.<process>.<count>.tmp` beside the file and is then renamed, so a
//! reader never sees a partial file, even while two queries record at once. One recorder writes to
//! a directory at a time.
use crate::answer::{Answer, Basis, BuildId, Error, Operation};
use serde::{Serialize, de::DeserializeOwned};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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

    /// Whether an answer file is present where the operation's questions read it; decoding
    /// remains the query's job. A file at the other layout is not an answer.
    pub(crate) fn contains(&self, operation: Operation) -> bool {
        let path = self.directory.join(operation.name());
        let (file, directory) = match layout(operation) {
            Layout::File => (true, false),
            Layout::Directory => (false, true),
            Layout::FileOrDirectory => (true, true),
        };
        (file && path.with_extension("json").is_file()) || (directory && contains_json(&path))
    }

    /// Preserve the original build, and mark the answer as recorded regardless of its file.
    pub(crate) fn read<T: DeserializeOwned>(
        &self,
        operation: Operation,
        subject: Option<&str>,
    ) -> Result<Answer<T>, Error> {
        let mut answer = read(&self.directory, operation, subject)?;
        if answer.source.build != self.build {
            return Err(Error::Recorded(format!(
                "{operation}: answer build differs from build.json"
            )));
        }
        answer.source.basis = Basis::Recorded;
        Ok(answer)
    }
}

/// Record the identity even for an error-only run, and refuse to mix builds in one directory.
fn record_build(root: &Path, build: &BuildId) -> Result<(), Error> {
    let path = root.join("build.json");
    let present = path
        .try_exists()
        .map_err(|error| Error::Recorded(format!("{}: {error}", path.display())))?;
    if present {
        if Answers::open(root.into())?.build != *build {
            return Err(Error::Recorded(
                "recording build differs from build.json".into(),
            ));
        }
        return Ok(());
    }

    let mut bytes = serde_json::to_vec(build).expect("build identity serializes");
    bytes.push(b'\n');
    publish(&path, &bytes)
}

/// Write `bytes` to a temporary file that no other write uses, and rename it to `path`, so that
/// an interrupted write leaves no partial file under the name that readers open.
fn publish(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    static WRITES: AtomicU64 = AtomicU64::new(0);
    let failed = |error: std::io::Error| Error::Recorded(format!("{}: {error}", path.display()));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(failed)?;
    }

    // The temporary name is short and its own, so a final name near the file name limit fits.
    let write = WRITES.fetch_add(1, Ordering::Relaxed);
    let temporary = path.with_file_name(format!(".{}.{write}.tmp", std::process::id()));
    std::fs::write(&temporary, bytes).map_err(failed)?;
    std::fs::rename(&temporary, path).map_err(failed)
}

/// The directory component of every encoded subject. Its `-` is outside the plain alphabet, so no
/// plain subject names it.
const ENCODED: &str = "hex-encoded";

/// Location of one operation's file. Any subject has one, and it lies inside `root`.
fn path(root: &Path, operation: Operation, subject: Option<&str>) -> PathBuf {
    let mut path = root.join(operation.name());
    match subject {
        None => {}
        Some(subject) if is_plain(subject) => path.push(subject),
        Some(subject) => {
            let hex: String = subject.bytes().map(|byte| format!("{byte:02x}")).collect();
            path.push(ENCODED);
            path.push(format!("x{hex}"));
        }
    }
    path.set_extension("json");
    path
}

/// Whether every segment of `subject` is a non-empty run of `[a-z0-9_]`.
fn is_plain(subject: &str) -> bool {
    subject.split('/').all(|segment| {
        !segment.is_empty()
            && segment
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    })
}

fn read<T: DeserializeOwned>(
    root: &Path,
    operation: Operation,
    subject: Option<&str>,
) -> Result<Answer<T>, Error> {
    let path = path(root, operation, subject);
    let named = || match subject {
        Some(subject) => format!("{operation}/{subject}"),
        None => operation.to_string(),
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
    operation: Operation,
    subject: Option<&str>,
    answer: &Result<Answer<T>, Error>,
) -> Result<(), Error> {
    let path = path(root, operation, subject);
    record_build(root, build)?;
    let mut bytes = serde_json::to_vec_pretty(answer).expect("answers serialize");
    bytes.push(b'\n');
    publish(&path, &bytes)
}

/// Where an operation's questions read their files, as the module layout states.
enum Layout {
    /// One `<operation>.json`.
    File,
    /// `<operation>/<subject>.json`.
    Directory,
    /// `loaded_modifiers.json`, or a subject directory for a session with a fixture.
    FileOrDirectory,
}

fn layout(operation: Operation) -> Layout {
    use Operation::*;
    match operation {
        RegistryFields | ModifierFamilies | DerivedNames | Declarations | CommandGrammar
        | ObserveFixture | CheckScript => Layout::Directory,
        LoadedModifiers => Layout::FileOrDirectory,
        Registries
        | Modifiers
        | ModifierCategories
        | Scopes
        | ScopeLinks
        | LocalizationDeclarations
        | OnActions
        | GameRules
        | Defines
        | DynamicNames
        | ModifierNodes
        | ModifierCategoryKeys
        | ScriptExpansions => Layout::File,
    }
}

fn contains_json(directory: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let Ok(kind) = entry.file_type() else {
            return false;
        };
        (kind.is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "json"))
            || (kind.is_dir() && contains_json(&entry.path()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_only_recordings_keep_the_build_and_refuse_a_different_recorder() {
        let root = tempfile::tempdir().unwrap();
        let build = BuildId("original".into());
        let answer: Result<Answer<Vec<String>>, Error> = Err(Error::BuildChanged);
        write(root.path(), &build, Operation::Registries, None, &answer).unwrap();
        let recorded = Answers::open(root.path().into()).unwrap();
        assert_eq!(recorded.build, build);
        assert_eq!(
            recorded.read::<Vec<String>>(Operation::Registries, None),
            answer
        );
        let different = BuildId("different".into());
        assert!(matches!(
            write(
                root.path(),
                &different,
                Operation::Registries,
                None,
                &answer
            ),
            Err(Error::Recorded(_))
        ));
        assert_eq!(Answers::open(root.path().into()).unwrap().build, build);
    }

    #[test]
    fn an_interrupted_write_leaves_no_file_under_its_final_name() {
        let root = tempfile::tempdir().unwrap();
        let build = BuildId("original".into());
        std::fs::write(root.path().join("build.json.1.0.tmp"), "\"orig").unwrap();
        std::fs::write(root.path().join("registries.json.1.1.tmp"), "{\"Ok\":").unwrap();
        assert!(matches!(
            Answers::open(root.path().into()),
            Err(Error::Recorded(_))
        ));

        let answer: Result<Answer<Vec<String>>, Error> = Err(Error::BuildChanged);
        write(root.path(), &build, Operation::Scopes, None, &answer).unwrap();
        let recorded = Answers::open(root.path().into()).unwrap();
        assert_eq!(recorded.build, build);
        assert!(!recorded.contains(Operation::Registries));
        assert!(matches!(
            recorded.read::<Vec<String>>(Operation::Registries, None),
            Err(Error::NotRecorded { .. })
        ));
        let left_over: Vec<_> = std::fs::read_dir(root.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .filter(|name| name.to_string_lossy().ends_with(".tmp"))
            .collect();
        assert_eq!(left_over.len(), 2, "{left_over:?}");
    }

    #[test]
    fn two_threads_record_into_one_directory_at_once() {
        let root = tempfile::tempdir().unwrap();
        let build = BuildId("original".into());
        let answer: Result<Answer<Vec<String>>, Error> = Err(Error::BuildChanged);
        std::thread::scope(|scope| {
            for _ in 0..2 {
                scope.spawn(|| {
                    for _ in 0..50 {
                        for operation in [Operation::Registries, Operation::Scopes] {
                            write(root.path(), &build, operation, None, &answer).unwrap();
                        }
                    }
                });
            }
        });

        let recorded = Answers::open(root.path().into()).unwrap();
        assert_eq!(recorded.build, build);
        for operation in [Operation::Registries, Operation::Scopes] {
            assert_eq!(recorded.read::<Vec<String>>(operation, None), answer);
        }
    }

    #[test]
    fn plain_subjects_keep_their_file_names() {
        let root = Path::new("recording");
        assert_eq!(
            path(root, Operation::RegistryFields, Some("common/traditions")),
            root.join("registry_fields/common/traditions.json")
        );
        assert_eq!(
            path(root, Operation::CommandGrammar, Some("effect/add_district")),
            root.join("command_grammar/effect/add_district.json")
        );
        assert_eq!(
            path(root, Operation::Registries, None),
            root.join("registries.json")
        );
    }

    #[test]
    fn a_subject_at_the_encoded_length_limit_is_recorded() {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("recording");
        let build = BuildId("original".into());
        let subject = "A".repeat(124);
        let error: Result<Answer<Vec<String>>, Error> = Err(Error::UnknownRegistry {
            name: subject.clone(),
        });

        write(
            &root,
            &build,
            Operation::RegistryFields,
            Some(&subject),
            &error,
        )
        .unwrap();
        let recorded = Answers::open(root).unwrap();
        assert_eq!(
            recorded.read::<Vec<String>>(Operation::RegistryFields, Some(&subject)),
            error
        );
    }

    #[test]
    fn every_subject_gets_its_own_file_inside_the_root() {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("recording");
        let build = BuildId("original".into());
        // The encoded name of "A" is `x41`; plain subjects that look like it are separate files.
        let subjects = [
            "common/traditions",
            "Common/Traditions",
            "common/some-thing",
            "",
            "/",
            "common//traditions",
            "common/traditions/",
            "../escape",
            "common/../../escape",
            "A",
            "x41",
            "encoded/x41",
            "hex-encoded/x41",
            "hex-encoded",
        ];
        for subject in subjects {
            let error: Result<Answer<Vec<String>>, Error> = Err(Error::UnknownRegistry {
                name: subject.into(),
            });
            let operation = Operation::RegistryFields;

            write(&root, &build, operation, Some(subject), &error).unwrap();
        }

        let recorded = Answers::open(root.clone()).unwrap();
        for subject in subjects {
            assert_eq!(
                recorded.read::<Vec<String>>(Operation::RegistryFields, Some(subject)),
                Err(Error::UnknownRegistry {
                    name: subject.into()
                }),
                "{subject:?}"
            );
        }
        let outside: Vec<_> = std::fs::read_dir(base.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(outside, ["recording"]);
        let mut files = Vec::new();
        let mut directories = vec![root];
        while let Some(directory) = directories.pop() {
            for entry in std::fs::read_dir(directory).unwrap() {
                let entry = entry.unwrap().path();
                if entry.is_dir() {
                    directories.push(entry);
                } else {
                    files.push(entry);
                }
            }
        }
        assert_eq!(files.len(), subjects.len() + 1, "{files:?}");
    }

    #[test]
    fn support_follows_the_layout_that_each_operation_reads() {
        let root = tempfile::tempdir().unwrap();
        let build = BuildId("original".into());
        let answer: Result<Answer<Vec<String>>, Error> = Ok(Answer {
            value: Vec::new(),
            completeness: crate::Completeness::Complete,
            gaps: Vec::new(),
            source: crate::Source {
                build: build.clone(),
                native_version: "test".into(),
                method: "test".into(),
                basis: Basis::StaticAnalysis,
            },
        });
        // A subject file under a singleton question is not where `registries()` reads.
        write(
            root.path(),
            &build,
            Operation::Registries,
            Some("archive"),
            &answer,
        )
        .unwrap();
        // A singleton file for a subject question is not where `observe_fixture` reads.
        write(
            root.path(),
            &build,
            Operation::ObserveFixture,
            None,
            &answer,
        )
        .unwrap();
        write(
            root.path(),
            &build,
            Operation::RegistryFields,
            Some("common/traditions"),
            &answer,
        )
        .unwrap();
        write(
            root.path(),
            &build,
            Operation::LoadedModifiers,
            None,
            &answer,
        )
        .unwrap();
        let recorded = Answers::open(root.path().into()).unwrap();
        assert!(!recorded.contains(Operation::Registries));
        assert!(!recorded.contains(Operation::ObserveFixture));
        assert!(recorded.contains(Operation::RegistryFields));
        assert!(recorded.contains(Operation::LoadedModifiers));
        assert!(!recorded.contains(Operation::Scopes));
    }
}
