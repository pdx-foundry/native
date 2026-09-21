//! Consumer-authored fixture inputs and normalized read-entry observations.
use crate::Error;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// The engine events requested from a fixture session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum FixtureObservationKind {
    /// The first three calls to the initial effect-registration entry point.
    RegistrationEntries,
    /// Category reads of `tree_template` and `traditions`, before storage or validation.
    CategoryFieldReads,
}

/// A bounded part of engine initialization. No world is loaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FixtureWindow {
    /// Initial registration through the supplied category file's loader return.
    /// At most three registration entries and two category field reads are observed.
    InitialCategoryLoad,
}

/// Files and questions fixed before `Native::start_game` launches its process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureRequest {
    /// One UTF-8 `.txt` file under `common/tradition_categories`, at most 64 KiB.
    /// Paths are relative to the game content root; values are the exact file contents.
    pub files: BTreeMap<String, String>,
    /// A nonempty set of the requested event kinds, with no duplicates.
    pub observations: Vec<FixtureObservationKind>,
    /// The engine phase in which to observe the fixture.
    pub window: FixtureWindow,
    /// Startup observation budget in seconds, 1–180. The smaller startup budget wins.
    pub deadline_seconds: u64,
}

impl FixtureRequest {
    /// Request both observation kinds through the initial category load, with a 180-second
    /// deadline. `start_game` validates the path and contents before launching anything.
    pub fn new(path: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            files: BTreeMap::from([(path.into(), text.into())]),
            observations: vec![
                FixtureObservationKind::RegistrationEntries,
                FixtureObservationKind::CategoryFieldReads,
            ],
            window: FixtureWindow::InitialCategoryLoad,
            deadline_seconds: 180,
        }
    }

    pub(crate) fn validate(&self) -> Result<(), Error> {
        let reject = |reason: &str| Error::FixtureRequest {
            reason: reason.into(),
        };
        if self.files.len() != 1 {
            return Err(reject("Exactly one category fixture file is supported"));
        }
        let (path, text) = self.files.first_key_value().unwrap();
        let Some(filename) = path.strip_prefix("common/tradition_categories/") else {
            return Err(reject(
                "Fixture files must be in common/tradition_categories",
            ));
        };
        let Some(stem) = filename.strip_suffix(".txt") else {
            return Err(reject("The category fixture must have a .txt extension"));
        };
        if stem.is_empty()
            || stem.len() > 128
            || !stem
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return Err(reject(
                "The fixture filename must contain only letters, digits, underscores or hyphens",
            ));
        }
        if text.is_empty() || text.len() > 64 * 1024 || text.contains('\0') {
            return Err(reject(
                "Fixture text must be nonempty UTF-8 without NUL, at most 64 KiB",
            ));
        }
        if self.observations.is_empty()
            || self.observations.iter().collect::<BTreeSet<_>>().len() != self.observations.len()
        {
            return Err(reject(
                "Request at least one observation kind, without duplicates",
            ));
        }
        if !(1..=180).contains(&self.deadline_seconds) {
            return Err(reject("Fixture deadline must be 1 to 180 seconds"));
        }
        Ok(())
    }

    pub(crate) fn file(&self) -> &str {
        self.files.first_key_value().expect("validated fixture").0
    }

    pub(crate) fn requests(&self, kind: FixtureObservationKind) -> bool {
        self.observations.contains(&kind)
    }

    /// Files select the recording directory. The question selects a file within it; deadlines
    /// do not change the question, and recorded errors remain errors regardless of the budget.
    pub(crate) fn recorded_subject(&self) -> String {
        let mut files = Sha256::new();
        for (path, contents) in &self.files {
            for part in [path.as_bytes(), contents.as_bytes()] {
                files.update((part.len() as u64).to_le_bytes());
                files.update(part);
            }
        }
        let observations: BTreeSet<_> = self.observations.iter().collect();
        let question =
            serde_json::to_vec(&(observations, self.window)).expect("fixture serializes");
        format!("{:x}/{:x}", files.finalize(), Sha256::digest(question))
    }
}

/// What happened at an engine entry point. Neither stage establishes successful storage,
/// validation, or gameplay behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessingStage {
    /// Entry to an initial effect-registration call, before it returns.
    RegistrationEntry,
    /// Entry to a category field reader, before it stores or validates a value.
    FieldReadEntry,
}

/// Opaque owner identity within one fixture observation. Equal identities mean the same owner;
/// identities from different sessions are not comparable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixtureOwnerId(pub(crate) u64);

/// One initial registration entry. The entry does not establish the registered command's name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistrationEntry {
    /// One-based position within the initial three-entry window.
    pub ordinal: u64,
    /// The observed processing stage.
    pub stage: ProcessingStage,
}

/// One source-correlated entry to the category field reader.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldRead {
    /// Fixture-relative content path.
    pub file: String,
    /// One-based source line reported by the engine.
    pub line: u64,
    /// Field key as the engine spells it.
    pub field: String,
    /// Session-local identity of the engine object receiving the read.
    pub owner: FixtureOwnerId,
    /// The observed processing stage.
    pub stage: ProcessingStage,
}

/// Established entries from the requested fixture window, in each kind's observation order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixtureObservation {
    /// Initial registration entries; not a complete command registry.
    pub registration_entries: Vec<RegistrationEntry>,
    /// Field-reader entries; not stored values or validation results.
    pub field_reads: Vec<FieldRead>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "common/tradition_categories/example.txt";

    #[test]
    fn only_bounded_relative_category_files_and_unique_questions_are_accepted() {
        let request = FixtureRequest::new(FILE, "category = {}\n");
        assert!(request.validate().is_ok());
        for path in [
            "/tmp/x.txt",
            "../x.txt",
            "common/traditions/x.txt",
            "common/tradition_categories/../x.txt",
            "common/tradition_categories/a/b.txt",
            "common/tradition_categories/a\\b.txt",
            "common/tradition_categories/x.mod",
            "common/tradition_categories/.txt",
            "common/tradition_categories/a\n.txt",
        ] {
            assert!(
                matches!(
                    FixtureRequest::new(path, "x").validate(),
                    Err(Error::FixtureRequest { .. })
                ),
                "{path:?}"
            );
        }
        for text in [String::new(), "\0".into(), "x".repeat(65537)] {
            assert!(FixtureRequest::new(FILE, text).validate().is_err());
        }
        assert!(
            FixtureRequest::new(FILE, "x".repeat(65536))
                .validate()
                .is_ok()
        );
        for seconds in [0, 181, u64::MAX] {
            let mut invalid = request.clone();
            invalid.deadline_seconds = seconds;
            assert!(invalid.validate().is_err());
        }
        for observations in [vec![], vec![FixtureObservationKind::RegistrationEntries; 2]] {
            let mut invalid = request.clone();
            invalid.observations = observations;
            assert!(invalid.validate().is_err());
        }
        let mut invalid = request.clone();
        invalid.files.clear();
        assert!(invalid.validate().is_err());
        invalid = request.clone();
        invalid
            .files
            .insert("common/tradition_categories/other.txt".into(), "x".into());
        assert!(invalid.validate().is_err());
        for property in ["window", "observations"] {
            let mut serialized = serde_json::to_value(&request).unwrap();
            serialized[property] = if property == "window" {
                serde_json::json!("Runtime")
            } else {
                serde_json::json!(["RuntimeValues"])
            };
            assert!(serde_json::from_value::<FixtureRequest>(serialized).is_err());
        }
    }

    #[test]
    fn recording_keys_distinguish_files_and_questions_but_not_order_or_deadline() {
        let request = FixtureRequest::new(FILE, "a");
        let key = request.recorded_subject();
        let mut same = request.clone();
        same.observations.reverse();
        same.deadline_seconds = 1;
        assert_eq!(same.recorded_subject(), key);
        let mut different = request.clone();
        different.observations.pop();
        assert_ne!(different.recorded_subject(), key);
        assert_eq!(
            different.recorded_subject().split('/').next(),
            key.split('/').next()
        );
        assert_ne!(FixtureRequest::new(FILE, "b").recorded_subject(), key);
        assert_ne!(
            FixtureRequest::new("common/tradition_categories/renamed.txt", "a").recorded_subject(),
            key
        );
    }

    #[tokio::test]
    async fn recorded_fixture_results_round_trip_without_a_supervisor() {
        use crate::{
            Answer, Basis, BuildId, Completeness, Disposal, GameOptions, Gap, GapKind, Native,
            Operation, Source, Support,
        };
        let request = FixtureRequest::new(FILE, "category = {}\n");
        let build = BuildId("authored-build".into());
        let complete = Answer {
            value: FixtureObservation {
                registration_entries: vec![RegistrationEntry {
                    ordinal: 1,
                    stage: ProcessingStage::RegistrationEntry,
                }],
                field_reads: vec![],
            },
            completeness: Completeness::Complete,
            gaps: vec![],
            source: Source::new(build.clone(), "authored/v1", Basis::LiveObservation),
        };
        let mut partial = complete.clone();
        partial.completeness = Completeness::Partial;
        partial.gaps.push(Gap {
            kind: GapKind::IncompleteObservation,
            subject: Some(FILE.into()),
            detail: "Terminal missing".into(),
        });
        let error = Error::Observation {
            operation: Operation::ObserveFixture,
            reason: "Hook missing".into(),
        };
        for result in [Ok(complete), Ok(partial), Err(error)] {
            let root = tempfile::tempdir().unwrap();
            crate::recorded::write(
                root.path(),
                &build,
                "observe_fixture",
                Some(&request.recorded_subject()),
                &result,
            )
            .unwrap();
            let native = Native::from_recorded_answers(root.path()).unwrap();
            assert_eq!(
                native.supports(Operation::ObserveFixture),
                Support::Supported
            );
            let options = || GameOptions::new(std::process::Command::new("must-not-start"));
            let mut game = native
                .start_game(options().fixture(request.clone()))
                .await
                .unwrap();
            let expected = result.map(|mut answer| {
                answer.source.basis = Basis::Recorded;
                answer
            });
            assert_eq!(game.observe_fixture().await, expected);
            assert_eq!(game.observe_fixture().await, expected);
            assert_eq!(game.close().await.unwrap(), Disposal::NotApplicable);
            assert_eq!(game.observe_fixture().await, Err(Error::Closed));
            let mut absent = native.start_game(options()).await.unwrap();
            assert!(matches!(
                absent.observe_fixture().await,
                Err(Error::FixtureRequest { .. })
            ));
            for changed in [FixtureRequest::new(FILE, "different"), {
                let mut changed = request.clone();
                changed.observations.pop();
                changed
            }] {
                let mut game = native.start_game(options().fixture(changed)).await.unwrap();
                assert!(matches!(
                    game.observe_fixture().await,
                    Err(Error::NotRecorded { .. })
                ));
            }
            assert!(matches!(
                native
                    .start_game(options().fixture(FixtureRequest::new("../escape", "x")))
                    .await,
                Err(Error::FixtureRequest { .. })
            ));
        }
    }
}
