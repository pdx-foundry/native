//! Bounded observations of command text in a paused game.
use crate::{
    Answer, Basis, BuildId, Completeness, DeclarationKind, Error, Gap, GapKind, ScopeId, Source,
};
use serde::{Deserialize, Serialize};

pub(crate) const MAX_TEXT_BYTES: usize = 4096;
pub(crate) const MAX_DIAGNOSTICS: usize = 32;
pub(crate) const MAX_CHECKS: u64 = 3000;
pub(crate) const CHECK_SECONDS: u64 = 5;

/// One trigger or effect snippet to read and validate without evaluation or execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptCheck {
    /// The command family to read.
    pub kind: DeclarationKind,
    /// A scope identity from `Native::scopes` for this build.
    pub scope: ScopeId,
    /// At most 4 KiB of UTF-8 script, with no NUL byte. Native adds trailing whitespace.
    pub text: String,
}

impl ScriptCheck {
    pub(crate) fn validate(&self) -> Result<(), Error> {
        if self.text.len() > MAX_TEXT_BYTES || self.text.contains('\0') {
            return Err(Error::ScriptRequest {
                reason: "script must be at most 4 KiB and contain no NUL byte".into(),
            });
        }
        Ok(())
    }

    pub(crate) fn recorded_subject(&self, previous: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hash = Sha256::new();
        hash.update(previous.as_bytes());
        hash.update(serde_json::to_vec(self).expect("script request serialization"));
        format!("{:x}", hash.finalize())
    }
}

/// The engine phase in which a diagnostic occurred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScriptStage {
    /// Constructing or reading the command tree.
    Read,
    /// Initializing or validating the command databases.
    Validation,
}

/// One occurrence of a fully formatted engine message. Repeated messages remain repeated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ScriptDiagnostic {
    /// Raw signed engine log level. All levels are retained; Native does not classify severity.
    pub level: i32,
    /// Message text, with the generated source name removed when attribution succeeds.
    pub text: String,
    /// The phase of the current check during which the message was observed.
    pub stage: ScriptStage,
    /// One-based line in the attributed snippet, when established.
    pub line: Option<u32>,
}

/// A message attributed to an earlier check in the same session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ForeignScriptDiagnostic {
    /// The earlier answer's check number.
    pub check: u64,
    /// The observed message.
    pub diagnostic: ScriptDiagnostic,
}

/// What one bounded read and validation observed. Silence never establishes acceptance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ScriptObservation {
    /// One-based identity within this session; retained commands may report again later.
    pub check: u64,
    /// Whether the top-level command reader returned.
    pub read_returned: bool,
    /// Number of top-level children built by the reader.
    pub children: u32,
    /// Messages attributed to this check's unique source.
    pub diagnostics: Vec<ScriptDiagnostic>,
    /// Messages attributed to earlier checks, kept separate from the current answer.
    pub foreign: Vec<ForeignScriptDiagnostic>,
    /// Messages whose source cannot be attributed uniquely, including source-free errors.
    pub unjoined: Vec<ScriptDiagnostic>,
    /// Whether every required capture hook was active throughout the check.
    pub hooks_active: bool,
    /// Whether the diagnostic count or message-size bound was reached.
    pub bound_reached: bool,
    /// Stored duration counts. `Known` only when every top-level child was classified by its
    /// receiver; a child with no duration group adds no entry. Old recordings read as
    /// `Unresolved`.
    #[serde(default)]
    // Out of the worker reply schema: the worker sends raw reads, not a `GrammarProperty`.
    #[schemars(skip)]
    pub stored_durations: crate::GrammarProperty<Vec<StoredDuration>>,
}

/// Stored duration count of one top-level child after reading, before validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StoredDuration {
    /// Zero-based index among the top-level children.
    pub child: u32,
    /// The unit keys of the child's static `Duration` group, which name the group.
    pub units: Vec<String>,
    /// The count slot: `Integer` for a scaled-at-read count, `ScopedNumeric` for a shared-factor
    /// operand.
    pub count: crate::FixtureValue,
    /// The shared factor slot, for a shared-factor group.
    pub factor: Option<i32>,
}

impl ScriptObservation {
    pub(crate) fn answer(self, build: BuildId) -> Answer<Self> {
        let mut gaps = Vec::new();
        for (missing, detail) in [
            (!self.read_returned, "The command reader did not return."),
            (
                !self.hooks_active,
                "A required diagnostic hook was missing, late, or unreadable.",
            ),
            (self.bound_reached, "A diagnostic bound was reached."),
            (
                !self.unjoined.is_empty(),
                "Some messages could not be attributed to one check.",
            ),
            (
                self.diagnostics
                    .iter()
                    .any(|message| message.line.is_none()),
                "An attributed message has no established source line.",
            ),
            (
                !matches!(self.stored_durations, crate::GrammarProperty::Known(_)),
                "Some top-level children could not be classified for duration storage.",
            ),
        ] {
            if missing {
                gaps.push(Gap {
                    kind: GapKind::IncompleteObservation,
                    subject: None,
                    detail: detail.into(),
                });
            }
        }
        Answer {
            completeness: Completeness::from_gaps(&gaps),
            value: self,
            gaps,
            source: Source::new(build, "check-script/v2", Basis::LiveObservation),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation() -> ScriptObservation {
        ScriptObservation {
            check: 1,
            read_returned: true,
            children: 1,
            diagnostics: Vec::new(),
            foreign: Vec::new(),
            unjoined: Vec::new(),
            hooks_active: true,
            bound_reached: false,
            stored_durations: crate::GrammarProperty::Known(Vec::new()),
        }
    }

    #[test]
    fn capture_gaps_never_become_complete_answers() {
        let diagnostic = ScriptDiagnostic {
            level: 1,
            text: "error".into(),
            stage: ScriptStage::Validation,
            line: None,
        };
        let mut cases = vec![observation(); 7];
        cases[0].read_returned = false;
        cases[1].hooks_active = false;
        cases[2].bound_reached = true;
        cases[3].unjoined.push(diagnostic.clone());
        cases[4].diagnostics.push(diagnostic.clone());
        cases[5].stored_durations = crate::GrammarProperty::Partial(Vec::new());
        cases[6].stored_durations = crate::GrammarProperty::Unresolved;
        for case in cases {
            let answer = case.answer(BuildId("test".into()));
            assert_eq!(answer.completeness, Completeness::Partial);
            assert!(!answer.gaps.is_empty());
        }
        assert_eq!(
            observation().answer(BuildId("test".into())).completeness,
            Completeness::Complete
        );
    }

    #[test]
    fn old_recordings_read_stored_durations_as_unresolved() {
        let mut recorded = serde_json::to_value(observation()).unwrap();
        recorded.as_object_mut().unwrap().remove("stored_durations");

        let old: ScriptObservation = serde_json::from_value(recorded).unwrap();

        assert_eq!(old.stored_durations, crate::GrammarProperty::Unresolved);
        assert_eq!(
            old.answer(BuildId("test".into())).completeness,
            Completeness::Partial
        );
    }

    #[test]
    fn only_complete_worker_reads_are_known_stored_durations() {
        use crate::protocol::script_check::{CheckedScript, StoredDurations};

        let stored = vec![StoredDuration {
            child: 0,
            units: vec!["days".into(), "months".into(), "years".into()],
            count: crate::FixtureValue::Integer(60),
            factor: None,
        }];

        for (complete, expected) in [
            (true, crate::GrammarProperty::Known(stored.clone())),
            (false, crate::GrammarProperty::Partial(stored.clone())),
        ] {
            let checked = CheckedScript {
                observation: observation(),
                durations: StoredDurations {
                    complete,
                    stored: stored.clone(),
                },
            };

            assert_eq!(checked.into_observation().stored_durations, expected);
        }
    }

    #[test]
    fn foreign_missing_lines_do_not_degrade_current_capture() {
        let mut current = observation();
        current.check = 2;
        current.foreign.push(ForeignScriptDiagnostic {
            check: 1,
            diagnostic: ScriptDiagnostic {
                level: 1,
                text: "earlier error".into(),
                stage: ScriptStage::Validation,
                line: None,
            },
        });
        assert_eq!(
            current.answer(BuildId("test".into())).completeness,
            Completeness::Complete
        );
    }

    #[test]
    fn text_bound_counts_bytes_and_rejects_nul() {
        let mut check = ScriptCheck {
            kind: DeclarationKind::Trigger,
            scope: ScopeId("scope".into()),
            text: "é".repeat(MAX_TEXT_BYTES / 2),
        };
        assert!(check.validate().is_ok());
        check.text.push('a');
        assert!(check.validate().is_err());
        check.text = "always = yes\0".into();
        assert!(check.validate().is_err());
    }

    #[test]
    fn recorded_subject_follows_the_check_sequence() {
        let check = ScriptCheck {
            kind: DeclarationKind::Trigger,
            scope: ScopeId("scope".into()),
            text: "always = yes".into(),
        };
        let first = check.recorded_subject("");
        assert_ne!(first, check.recorded_subject(&first));
        assert_ne!(
            check.recorded_subject("bad"),
            check.recorded_subject("good")
        );
        assert_eq!(first, check.recorded_subject(""));
    }
}
