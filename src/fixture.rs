//! Consumer-authored fixture inputs and parser outcomes.
use crate::Error;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// A bounded part of engine initialization. No world is loaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FixtureWindow {
    /// Initial parsing of the supplied file, ending at its verified file-load boundary.
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
    /// Field name, with the same character and length limits as `definition`.
    pub field: String,
    /// Optional enclosing field for one level of embedded-owner storage observation.
    /// Uses the same name limits as `definition`. Unsupported owner joins are unavailable.
    /// See [`Self::with_parent_field`] for supported setup.
    #[serde(default)]
    pub parent_field: Option<String>,
    /// Whether to witness field-reader entries and returns independently of storage.
    #[serde(default)]
    pub parsing: bool,
    /// Whether parser diagnostics from the file-load window are requested.
    pub diagnostics: bool,
}

impl FixtureFieldQuestion {
    /// Request parser storage and diagnostics for one field.
    pub fn new(
        registry: impl Into<String>,
        definition: impl Into<String>,
        field: impl Into<String>,
    ) -> Self {
        Self {
            registry: registry.into(),
            definition: definition.into(),
            field: field.into(),
            parent_field: None,
            parsing: false,
            diagnostics: true,
        }
    }

    /// Select a field inside an embedded block of the named definition.
    ///
    /// The M451-hotfix binding supports `common/special_projects` in an initial file-load
    /// field-outcome request. Validation observations are not supported
    /// for this loader. Other parent or leaf shapes report unavailable when not proven.
    ///
    /// ```
    /// use pdx_native::{FixtureFieldQuestion, FixtureRequest};
    /// let request = FixtureRequest::field_outcomes(
    ///     "common/special_projects/sample.txt",
    ///     "special_project = { key = sample requirements = { fleet_power = 1.25 } }",
    ///     [FixtureFieldQuestion::new("common/special_projects", "sample", "fleet_power")
    ///         .with_parent_field("requirements")
    ///         .with_parsing()],
    /// );
    /// ```
    pub fn with_parent_field(mut self, parent: impl Into<String>) -> Self {
        self.parent_field = Some(parent.into());
        self
    }

    /// Also observe parser entries and returns, including fields with no storage decoder.
    pub fn with_parsing(mut self) -> Self {
        self.parsing = true;
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
    /// One to 32 field-outcome questions, sorted in ascending `Ord` order. Each
    /// `(registry, definition, parent_field, field)` identity must be unique. `field_outcomes` sorts its
    /// questions.
    pub field_questions: Vec<FixtureFieldQuestion>,
    /// The engine phase in which to observe the fixture.
    pub window: FixtureWindow,
}

impl FixtureRequest {
    /// Request field outcomes for one registry file, sorting questions into canonical order.
    pub fn field_outcomes(
        path: impl Into<String>,
        text: impl Into<String>,
        questions: impl IntoIterator<Item = FixtureFieldQuestion>,
    ) -> Self {
        let mut field_questions: Vec<_> = questions.into_iter().collect();
        field_questions.sort();
        Self {
            files: BTreeMap::from([(path.into(), text.into())]),
            field_questions,
            window: FixtureWindow::InitialFileLoad,
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
        if self.field_questions.is_empty() {
            return Err(reject("Request at least one field outcome"));
        }
        let identities = self
            .field_questions
            .iter()
            .map(|question| {
                (
                    &question.registry,
                    &question.definition,
                    &question.parent_field,
                    &question.field,
                )
            })
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
                || question
                    .parent_field
                    .as_ref()
                    .is_some_and(|name| !valid_name(name))
            {
                return Err(reject(
                    "Field questions must name this fixture registry and bounded nonempty names",
                ));
            }
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
        Ok(())
    }

    pub(crate) fn file(&self) -> &str {
        self.files.first_key_value().expect("validated fixture").0
    }

    pub(crate) fn registry(&self) -> &str {
        self.file().rsplit_once('/').expect("validated fixture").0
    }

    /// Files select the recording directory. The question selects a file within it.
    pub(crate) fn recorded_subject(&self) -> String {
        let mut files = Sha256::new();
        for (path, contents) in &self.files {
            for part in [path.as_bytes(), contents.as_bytes()] {
                files.update((part.len() as u64).to_le_bytes());
                files.update(part);
            }
        }
        let questions: BTreeSet<_> = self.field_questions.iter().collect();
        let question = serde_json::to_vec(&(questions, self.window)).expect("fixture serializes");
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

/// Opaque owner identity within one fixture observation. Equal identities mean the same owner;
/// identities from different sessions are not comparable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixtureOwnerId(pub(crate) u64);

/// An exact value read from the definition's storage, independently of parser diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub enum FixtureValue {
    /// Parsed scoped operand state, independently of evaluation.
    ScopedNumeric(ScopedNumericStorage),
    /// Text stored by the string reader.
    String(String),
    /// A signed 32-bit integer stored by the direct integer reader.
    Integer(i32),
    /// The IEEE binary32 pattern stored by the float reader; `f32::from_bits` gives its value.
    Float {
        /// Exact stored bits, including negative zero and nonfinite patterns.
        bits: u32,
    },
    /// The 16 bits stored by the short reader, without a signed interpretation.
    Integer16 {
        /// Exact stored bits; static signedness is independent of this observation.
        bits: u16,
    },
    /// A signed fixed-point value. Divide `raw` by `scale` to interpret it exactly.
    FixedPoint {
        /// The signed integer held in storage, without conversion or rounding.
        raw: i64,
        /// Positive number of stored units per whole unit.
        scale: u64,
    },
}

/// Literal slot of a scoped numeric operand, even when a reference is selected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub enum ScopedNumericLiteral {
    /// Signed whole-number storage.
    Integer(i32),
    /// Exact signed storage with positive units per whole value.
    FixedPoint {
        /// Stored numerator.
        raw: i64,
        /// Units per whole value.
        scale: u64,
    },
}

/// Parser storage of a scoped operand; reference presence does not prove lookup success.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScopedNumericStorage {
    /// Numeric slot, which can coexist with older references.
    pub literal: ScopedNumericLiteral,
    /// Whether the source-location string is nonempty. This is a selection condition, not a value.
    pub has_source_location: bool,
    /// Whether a trigger object is stored.
    pub has_trigger: bool,
    /// Whether a script-value lookup object is stored.
    pub has_script_value: bool,
    /// Whether the modifier slot differs from its unset marker.
    pub has_modifier: bool,
    /// Stored variable text, including an empty string when no name was stored.
    pub variable: String,
}

/// One stored value read after a source occurrence returned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredFieldOccurrence {
    /// One-based source line reported by the engine.
    pub line: u64,
    /// One-based occurrence of this field on this definition.
    pub occurrence: u64,
    /// Actual value in the definition after the reader returned.
    pub value: FixtureValue,
}

/// Independently observed parser storage for one requested field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FixtureStorage {
    /// No storage observation was established; the reason states what was unavailable.
    Unavailable(String),
    /// Values after each occurrence and the value when the file load completed.
    Observed {
        /// Source-ordered values after each joined reader return.
        occurrences: Vec<StoredFieldOccurrence>,
        /// Value at the file-load terminal, including constructor initialization when observed.
        final_value: Option<FixtureValue>,
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

/// Parser outcomes for one requested definition field.
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
    pub parsing: FixtureParsing,
    /// Independently observed parser storage.
    pub storage: FixtureStorage,
    /// Indices into `FixtureObservation::diagnostics`.
    pub diagnostics: Vec<usize>,
}

/// Outcomes and diagnostics from the requested fixture window.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixtureObservation {
    /// Outcomes for each requested field, in canonical question order.
    pub field_outcomes: Vec<FixtureFieldOutcome>,
    /// Engine diagnostics from the fixture file load, including diagnostics without a field join.
    pub diagnostics: Vec<FixtureDiagnostic>,
    /// Explicit coverage of the parser-diagnostic window.
    pub diagnostic_coverage: DiagnosticCoverage,
}

#[cfg(test)]
mod tests {
    #[test]
    fn nested_selectors_validate_and_have_distinct_recording_keys() {
        use super::*;
        let root = FixtureFieldQuestion::new("common/example", "sample", "number");
        let nested = root.clone().with_parent_field("requirements");
        let request = |questions| {
            FixtureRequest::field_outcomes("common/example/sample.txt", "sample = {}", questions)
        };
        assert!(
            request(vec![root.clone(), nested.clone()])
                .validate()
                .is_ok()
        );
        assert_ne!(
            request(vec![root]).recorded_subject(),
            request(vec![nested.clone()]).recorded_subject()
        );
        assert!(request(vec![nested.clone(), nested]).validate().is_err());
        for parent in ["", "one/two", "one.two.three "] {
            assert!(
                request(vec![
                    FixtureFieldQuestion::new("common/example", "sample", "number")
                        .with_parent_field(parent)
                ])
                .validate()
                .is_err()
            );
        }
    }

    use super::*;

    fn request_for(path: impl Into<String>, text: impl Into<String>) -> FixtureRequest {
        FixtureRequest::field_outcomes(
            path,
            text,
            [
                FixtureFieldQuestion::new(
                    "common/tradition_categories",
                    "category",
                    "tree_template",
                )
                .with_parsing(),
                FixtureFieldQuestion::new("common/tradition_categories", "category", "traditions")
                    .with_parsing(),
            ],
        )
    }

    const FILE: &str = "common/tradition_categories/example.txt";

    #[test]
    fn only_bounded_relative_fixture_files_and_unique_questions_are_accepted() {
        let request = request_for(FILE, "category = {}\n");
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
                    request_for(path, "x").validate(),
                    Err(Error::FixtureRequest { .. })
                ),
                "{path:?}"
            );
        }
        for text in [String::new(), "\0".into(), "x".repeat(65537)] {
            assert!(request_for(FILE, text).validate().is_err());
        }
        assert!(request_for(FILE, "x".repeat(65536)).validate().is_ok());
        let mut invalid = request.clone();
        invalid.field_questions.clear();
        assert!(invalid.validate().is_err());
        invalid = request.clone();
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
            request_for("common/traditions/x.txt", "sample = {}\n")
                .validate()
                .is_err()
        );
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
    fn recording_keys_distinguish_files_and_questions_but_not_order() {
        let request = request_for(FILE, "a");
        let key = request.recorded_subject();
        let mut same = request.clone();
        same.field_questions.reverse();
        assert_eq!(same.recorded_subject(), key);
        let mut different = request.clone();
        different.field_questions[0].parsing = false;
        assert_ne!(different.recorded_subject(), key);
        assert_eq!(
            different.recorded_subject().split('/').next(),
            key.split('/').next()
        );
        assert_ne!(request_for(FILE, "b").recorded_subject(), key);
        assert_ne!(
            request_for("common/tradition_categories/renamed.txt", "a").recorded_subject(),
            key
        );
    }

    #[tokio::test]
    async fn recorded_fixture_results_round_trip_without_a_supervisor() {
        use crate::{
            Answer, Basis, BuildId, Completeness, Disposal, GameOptions, Gap, GapKind, GapSubject,
            Native, Operation, Source, Support,
        };
        let request = request_for(FILE, "category = {}\n");
        let build = BuildId("authored-build".into());
        let complete = Answer {
            value: FixtureObservation::default(),
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
            for changed in [request_for(FILE, "different"), {
                let mut changed = request.clone();
                changed.field_questions[0].parsing = false;
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
                    .start_game(options().fixture(request_for("../escape", "x")))
                    .await,
                Err(Error::FixtureRequest { .. })
            ));
        }
    }
}
