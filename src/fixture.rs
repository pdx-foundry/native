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
    /// Initial parsing of the supplied file, ending when its reader returns to `LoadFile`.
    InitialFileLoad,
    /// Initial parsing and subsequent validation, ending at the bound content-loaded point.
    InitialFileLoadAndValidation,
}

/// One field whose parser outcome is requested for a named definition.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureFieldQuestion {
    /// Content directory that owns the definition. It must equal the fixture file's directory.
    pub registry: String,
    /// Definition key as observed by the engine. Nonempty, at most 128 bytes of ASCII letters,
    /// digits, `_`, `-`, `.` or `:`.
    pub definition: String,
    /// Root field name, with the same character and length limits as `definition`.
    pub field: String,
    /// Whether to witness field-reader entries and returns independently of storage.
    #[serde(default)]
    pub parsing: bool,
    /// Whether parser diagnostics from the file-load window are requested.
    pub diagnostics: bool,
    /// Whether a runtime outcome is requested. This initial-load method reports it unavailable.
    pub runtime: bool,
}

impl FixtureFieldQuestion {
    /// Request parser storage and diagnostics for one field. Runtime is not requested.
    pub fn new(
        registry: impl Into<String>,
        definition: impl Into<String>,
        field: impl Into<String>,
    ) -> Self {
        Self {
            registry: registry.into(),
            definition: definition.into(),
            field: field.into(),
            parsing: false,
            diagnostics: true,
            runtime: false,
        }
    }

    /// Also observe parser entries and returns, including fields with no storage decoder.
    pub fn with_parsing(mut self) -> Self {
        self.parsing = true;
        self
    }

    /// Also request the runtime dimension, which is outside the initial-load method.
    pub fn with_runtime(mut self) -> Self {
        self.runtime = true;
        self
    }
}

/// Files and questions fixed before `Native::start_game` launches its process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureRequest {
    /// One UTF-8 `.txt` file under a bounded relative registry path, at most 64 KiB.
    /// Paths are relative to the game content root; values are the exact file contents.
    pub files: BTreeMap<String, String>,
    /// A nonempty set of the requested event kinds, with no duplicates.
    pub observations: Vec<FixtureObservationKind>,
    /// At most 32 field-outcome questions, sorted in ascending `Ord` order. Each
    /// `(registry, definition, field)` identity must be unique. `field_outcomes` sorts its
    /// questions.
    pub field_questions: Vec<FixtureFieldQuestion>,
    /// The engine phase in which to observe the fixture.
    pub window: FixtureWindow,
    /// Startup observation budget in seconds, 1–180. The smaller startup budget wins.
    pub deadline_seconds: u64,
}

impl FixtureRequest {
    /// Request both category observation kinds through the initial category load, with a
    /// 180-second deadline. The file must be under `common/tradition_categories`.
    /// `start_game` validates the path and contents before launching anything.
    pub fn new(path: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            files: BTreeMap::from([(path.into(), text.into())]),
            observations: vec![
                FixtureObservationKind::RegistrationEntries,
                FixtureObservationKind::CategoryFieldReads,
            ],
            field_questions: Vec::new(),
            window: FixtureWindow::InitialCategoryLoad,
            deadline_seconds: crate::protocol::session::MAX_SESSION_SECONDS,
        }
    }

    /// Request field outcomes for one registry file. The questions are sorted into the
    /// canonical order that validation requires. Existing registration and category-read
    /// selections are not added; callers may add registration entries, while category field
    /// reads require tradition categories.
    pub fn field_outcomes(
        path: impl Into<String>,
        text: impl Into<String>,
        questions: impl IntoIterator<Item = FixtureFieldQuestion>,
    ) -> Self {
        let mut field_questions: Vec<_> = questions.into_iter().collect();
        field_questions.sort();
        Self {
            files: BTreeMap::from([(path.into(), text.into())]),
            observations: Vec::new(),
            field_questions,
            window: FixtureWindow::InitialFileLoad,
            deadline_seconds: crate::protocol::session::MAX_SESSION_SECONDS,
        }
    }

    /// Keep diagnostic observation open through the engine's content-loaded boundary.
    /// Requires at least one field question with diagnostics enabled. No world is loaded.
    pub fn through_validation(mut self) -> Self {
        self.window = FixtureWindow::InitialFileLoadAndValidation;
        self
    }

    pub(crate) fn validate(&self) -> Result<(), Error> {
        let reject = |reason: &str| Error::FixtureRequest {
            reason: reason.into(),
        };
        if self.files.len() != 1 {
            return Err(reject("Exactly one fixture file is supported"));
        }
        let (path, text) = self.files.first_key_value().unwrap();
        let Some((registry, filename)) = path.rsplit_once('/') else {
            return Err(reject("Fixture files must be under a registry directory"));
        };
        if registry.len() > 256 || !registry.split('/').all(is_path_component) {
            return Err(reject("Fixture registry path has an invalid component"));
        }
        let Some(stem) = filename.strip_suffix(".txt") else {
            return Err(reject("The fixture must have a .txt extension"));
        };
        if !is_path_component(stem) {
            return Err(reject(
                "The fixture filename must contain only letters, digits, underscores or hyphens",
            ));
        }
        if text.is_empty() || text.len() > 64 * 1024 || text.contains('\0') {
            return Err(reject(
                "Fixture text must be nonempty UTF-8 without NUL, at most 64 KiB",
            ));
        }
        if (self.observations.is_empty() && self.field_questions.is_empty())
            || self.observations.iter().collect::<BTreeSet<_>>().len() != self.observations.len()
        {
            return Err(reject(
                "Request at least one observation kind, without duplicates",
            ));
        }
        let identities = self
            .field_questions
            .iter()
            .map(|question| (&question.registry, &question.definition, &question.field))
            .collect::<BTreeSet<_>>();
        if self.field_questions.len() > 32
            || identities.len() != self.field_questions.len()
            || self
                .field_questions
                .windows(2)
                .any(|pair| pair[0] > pair[1])
        {
            return Err(reject(
                "Request at most 32 unique field identities in canonical order",
            ));
        }
        for question in &self.field_questions {
            let valid_name = |name: &str| {
                !name.is_empty()
                    && name.len() <= 128
                    && name.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b':')
                    })
            };
            if question.registry != registry
                || !valid_name(&question.definition)
                || !valid_name(&question.field)
            {
                return Err(reject(
                    "Field questions must name this fixture registry and bounded nonempty names",
                ));
            }
        }
        if self.field_questions.is_empty() && self.window != FixtureWindow::InitialCategoryLoad {
            return Err(reject("Read-entry observations use InitialCategoryLoad"));
        }
        if !self.field_questions.is_empty() && self.window == FixtureWindow::InitialCategoryLoad {
            return Err(reject("Field outcomes require a file-load window"));
        }
        if self.window == FixtureWindow::InitialFileLoadAndValidation
            && !self
                .field_questions
                .iter()
                .any(|question| question.diagnostics)
        {
            return Err(reject(
                "Validation observation requires a diagnostic question",
            ));
        }
        if self.window == FixtureWindow::InitialCategoryLoad
            && registry != "common/tradition_categories"
        {
            return Err(reject(
                "InitialCategoryLoad requires a common/tradition_categories fixture",
            ));
        }
        if registry != "common/tradition_categories"
            && self
                .observations
                .contains(&FixtureObservationKind::CategoryFieldReads)
        {
            return Err(reject(
                "CategoryFieldReads requires a common/tradition_categories fixture",
            ));
        }
        if !(1..=crate::protocol::session::MAX_SESSION_SECONDS).contains(&self.deadline_seconds) {
            return Err(reject("Fixture deadline must be 1 to 180 seconds"));
        }
        Ok(())
    }

    pub(crate) fn file(&self) -> &str {
        self.files.first_key_value().expect("validated fixture").0
    }

    pub(crate) fn registry(&self) -> &str {
        self.file().rsplit_once('/').expect("validated fixture").0
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
        let questions: BTreeSet<_> = self.field_questions.iter().collect();
        let question = serde_json::to_vec(&(observations, questions, self.window))
            .expect("fixture serializes");
        format!("{:x}/{:x}", files.finalize(), Sha256::digest(question))
    }
}

/// A nonempty directory or filename stem of at most 128 ASCII letters, digits, `_` or `-`.
fn is_path_component(part: &str) -> bool {
    !part.is_empty()
        && part.len() <= 128
        && part
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
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

/// One stored string read after a source occurrence returned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredStringOccurrence {
    /// One-based source line reported by the engine.
    pub line: u64,
    /// One-based occurrence of this field on this definition.
    pub occurrence: u64,
    /// Actual string in the definition after the reader returned.
    pub value: String,
}

/// Independently observed parser storage for one requested field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FixtureStorage {
    /// No String storage observation was established; the reason states what was unavailable.
    Unavailable(String),
    /// Values after each occurrence and the value when the file load completed.
    String {
        /// Source-ordered values after each joined reader return.
        occurrences: Vec<StoredStringOccurrence>,
        /// Value at the file-load terminal, including constructor initialization when observed.
        final_value: Option<String>,
        /// Whether all storage records and terminals completed intact.
        completeness: crate::Completeness,
    },
}

/// One witnessed invocation of a field's parser. A return does not establish validity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedFieldOccurrence {
    /// One-based source line at entry to the field reader.
    pub line: u64,
    /// One-based occurrence of this field on the requested definition.
    pub occurrence: u64,
    /// Source line when the same invocation returned, or `None` if its return was lost.
    pub return_line: Option<u64>,
}

/// Parser invocations observed independently of storage decoding and diagnostics.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FixtureParsing {
    /// The caller did not request parser entries and returns.
    #[default]
    NotRequested,
    /// The field's parser could not be observed within a verified owner boundary.
    Unavailable(String),
    /// Entries and returns within the fixture file load. Empty means omission only when complete.
    Observed {
        /// Source-ordered reader invocations, including invocations without a return.
        occurrences: Vec<ParsedFieldOccurrence>,
        /// Whether the entries, returns, owner and terminal counts all joined.
        completeness: crate::Completeness,
    },
}

/// Runtime state for a requested field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FixtureRuntime {
    /// The caller did not request this dimension.
    NotRequested,
    /// Runtime is outside this initial-file-load method.
    Unavailable(String),
}

/// Source correlation for one engine diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticJoin {
    /// The diagnostic was joined to a fixture source location.
    Source {
        /// Fixture-relative file.
        file: String,
        /// One-based engine source line.
        line: u64,
        /// Definition key when established.
        definition: Option<String>,
        /// Field when established.
        field: Option<String>,
        /// Field occurrence when established.
        occurrence: Option<u64>,
    },
    /// The engine diagnostic was preserved, but its fixture source join was not established.
    Unavailable(String),
}

/// One diagnostic captured at the engine's reader-report stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixtureDiagnostic {
    /// Exact diagnostic text supplied to the engine report routine.
    pub text: String,
    /// Actual engine stage that was intercepted.
    pub stage: String,
    /// Independent source correlation.
    pub join: DiagnosticJoin,
}

/// Coverage of parser diagnostics during the fixture file load.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticCoverage {
    /// No field question requested parser diagnostics.
    #[default]
    NotRequested,
    /// Hooks were active and collection completed at the file-load return.
    Complete {
        /// Exact bounded diagnostic window that completed.
        window: DiagnosticWindow,
    },
    /// Diagnostic collection is unsupported or did not complete.
    Unavailable(String),
}

/// Bounded engine window covered by diagnostic collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticWindow {
    /// Parser diagnostics emitted while the selected fixture file loaded.
    FixtureFileLoad,
    /// Reader reports and source-located engine logs through the content-loaded boundary.
    FixtureFileLoadAndValidation,
}

/// Parser and runtime outcomes for one requested definition field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixtureFieldOutcome {
    /// The original bounded question.
    pub question: FixtureFieldQuestion,
    /// Fixture-relative file.
    pub file: String,
    /// Session-local owner identity, when the definition constructor was witnessed.
    pub owner: Option<FixtureOwnerId>,
    /// One-based definition source line, when established by the engine reader.
    pub definition_line: Option<u64>,
    /// Shared reader established by Native's static method.
    pub reader: crate::Reader,
    /// Parser entries and returns; independent of stored values and diagnostic coverage.
    #[serde(default)]
    pub parsing: FixtureParsing,
    /// Independently observed parser storage.
    pub storage: FixtureStorage,
    /// Indices into `FixtureObservation::diagnostics`.
    pub diagnostics: Vec<usize>,
    /// Requested runtime dimension.
    pub runtime: FixtureRuntime,
}

/// Established entries from the requested fixture window, in each kind's observation order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixtureObservation {
    /// Initial registration entries; not a complete command registry.
    pub registration_entries: Vec<RegistrationEntry>,
    /// Field-reader entries; not stored values or validation results.
    pub field_reads: Vec<FieldRead>,
    /// Outcomes for each requested field, in canonical question order.
    pub field_outcomes: Vec<FixtureFieldOutcome>,
    /// Engine diagnostics from the fixture file load, including diagnostics without a field join.
    pub diagnostics: Vec<FixtureDiagnostic>,
    /// Explicit coverage of the parser-diagnostic window.
    pub diagnostic_coverage: DiagnosticCoverage,
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "common/tradition_categories/example.txt";

    #[test]
    fn only_bounded_relative_fixture_files_and_unique_questions_are_accepted() {
        let request = FixtureRequest::new(FILE, "category = {}\n");
        assert!(request.validate().is_ok());
        for path in [
            "/tmp/x.txt",
            "../x.txt",
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
        let question = FixtureFieldQuestion::new("common/traditions", "sample", "unlocks_agenda");
        let outcome = FixtureRequest::field_outcomes(
            "common/traditions/x.txt",
            "sample = {}\n",
            [question.clone()],
        );
        assert!(outcome.validate().is_ok());
        let different_owner = FixtureRequest::field_outcomes(
            "common/federation_perks/x.txt",
            "sample = { icon = \"test\" }\n",
            [FixtureFieldQuestion::new(
                "common/federation_perks",
                "sample",
                "icon",
            )],
        );
        assert!(different_owner.validate().is_ok());
        assert!(
            FixtureRequest::field_outcomes(
                "map/galaxy/x.txt",
                "sample = {}\n",
                [FixtureFieldQuestion::new(
                    "map/galaxy",
                    "sample",
                    "preview_icon"
                )],
            )
            .validate()
            .is_ok()
        );
        for path in [
            "common//x.txt",
            "common/../x.txt",
            "common/federation_perks//x.txt",
            "common/federation.perks/x.txt",
        ] {
            let mut invalid = different_owner.clone();
            invalid.files = BTreeMap::from([(path.into(), "sample = {}\n".into())]);
            assert!(invalid.validate().is_err(), "{path:?}");
        }
        let mut tradition_outcome_with_registration = outcome.clone();
        tradition_outcome_with_registration.observations =
            vec![FixtureObservationKind::RegistrationEntries];
        assert!(tradition_outcome_with_registration.validate().is_ok());
        let mut wrong_registry = outcome.clone();
        wrong_registry.field_questions[0].registry = "common/tradition_categories".into();
        assert!(wrong_registry.validate().is_err());
        let mut too_many = outcome.clone();
        too_many.field_questions = (0..33)
            .map(|index| {
                FixtureFieldQuestion::new(
                    "common/traditions",
                    format!("sample_{index}"),
                    "unlocks_agenda",
                )
            })
            .collect();
        assert!(too_many.validate().is_err());
        let sorted = FixtureRequest::field_outcomes(
            "common/traditions/x.txt",
            "sample = {}\n",
            [
                FixtureFieldQuestion::new("common/traditions", "z", "unlocks_agenda"),
                FixtureFieldQuestion::new("common/traditions", "a", "unlocks_agenda"),
            ],
        );
        assert_eq!(sorted.field_questions[0].definition, "a");
        let mut reordered = sorted.clone();
        reordered.field_questions.reverse();
        assert!(reordered.validate().is_err());
        assert_eq!(sorted.recorded_subject(), reordered.recorded_subject());
        let mut duplicate = question.clone();
        duplicate.diagnostics = false;
        duplicate.runtime = true;
        assert!(
            FixtureRequest::field_outcomes(
                "common/traditions/x.txt",
                "sample = {}\n",
                [question, duplicate],
            )
            .validate()
            .is_err()
        );
        assert!(
            FixtureRequest::new("common/traditions/x.txt", "sample = {}\n")
                .validate()
                .is_err()
        );
        let mut tradition_registration =
            FixtureRequest::new("common/traditions/x.txt", "sample = {}\n");
        tradition_registration.observations = vec![FixtureObservationKind::RegistrationEntries];
        assert!(tradition_registration.validate().is_err());
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
            Answer, Basis, BuildId, Completeness, Disposal, GameOptions, Gap, GapKind, GapSubject,
            Native, Operation, Source, Support,
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
                ..FixtureObservation::default()
            },
            completeness: Completeness::Complete,
            gaps: vec![],
            source: Source::new(build.clone(), "authored/v1", Basis::LiveObservation),
        };
        let mut partial = complete.clone();
        partial.completeness = Completeness::Partial;
        partial.gaps.push(Gap {
            kind: GapKind::IncompleteObservation,
            subject: Some(GapSubject::fixture_file(FILE)),
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
