//! Prepared world observations and their normalized results.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A fixed observation prepared before launch. Native copies the save into a private profile,
/// executes the effect in the named local human country, and reads flags and variables after
/// each game day.
/// The save must be compatible with the opened build and installed content.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorldRequest {
    /// Absolute path to a compatible game-produced save, at most 16 MiB. The source is never changed.
    #[schemars(with = "String")]
    pub save: PathBuf,
    /// Exact displayed name of the local human country, 1–256 UTF-8 bytes with no NUL.
    /// A different country ends startup.
    pub country: String,
    /// Prepared country effect, at most 4 KiB of UTF-8 with no NUL.
    /// Empty text only observes and advances time.
    pub effect: String,
    /// Days to advance through the engine, from 0 to 120. Each day has a separate sample.
    pub days: u32,
    /// At most 32 unique flag names, each 1–128 ASCII letters, digits or underscores.
    /// Absence is separate from a remaining count.
    pub flags: Vec<String>,
    /// At most 32 unique variable names, each 1–128 ASCII letters, digits or underscores.
    /// A name is read as the engine reads a variable operand in the country scope: a `local_`
    /// name belongs to the prepared scope, any other name to the country. An unset variable is
    /// separate from a zero value.
    #[serde(default)]
    pub variables: Vec<String>,
}

/// Flag and variable names share one bound and one character set.
fn bounded_unique_names(names: &[String]) -> bool {
    let unique: std::collections::BTreeSet<_> = names.iter().collect();
    names.len() <= 32
        && unique.len() == names.len()
        && names.iter().all(|name| {
            !name.is_empty()
                && name.len() <= 128
                && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
}

impl WorldRequest {
    pub(crate) fn recorded_subject(&self, save: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        let input = serde_json::to_vec(&(
            &self.country,
            &self.effect,
            self.days,
            &self.flags,
            &self.variables,
        ))
        .expect("world input serializes");
        format!("{:x}/{:x}", Sha256::digest(save), Sha256::digest(input))
    }

    pub(crate) fn validate(&self) -> Result<(), crate::Error> {
        let valid = self.save.is_absolute()
            && !self.country.is_empty()
            && self.country.len() <= 256
            && !self.country.contains('\0')
            && self.effect.len() <= crate::script::MAX_TEXT_BYTES
            && !self.effect.contains('\0')
            && self.days <= 120
            && bounded_unique_names(&self.flags)
            && bounded_unique_names(&self.variables);
        if !valid {
            return Err(crate::Error::Observation { operation: crate::Operation::ObserveWorld,
                reason: "world request needs an absolute save, a country name, at most 4 KiB of effect text, 0 to 120 days, at most 32 unique flag names and at most 32 unique variable names".into() });
        }
        Ok(())
    }
}

/// A ready world observation. Effect execution and time advancement are separate outcomes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorldObservation {
    /// Displayed country name, established from the actual local human's country.
    pub country: String,
    /// Engine date before the prepared effect, in year.month.day form.
    pub initial_date: String,
    /// Whether the prepared effect returned. Empty text counts as no execution.
    pub executed: bool,
    /// Engine diagnostics during reading, validation and execution. Any message prevents
    /// execution after validation; execution messages make the observation partial.
    pub diagnostics: Vec<String>,
    /// Day zero is after the effect; subsequent samples follow each engine day.
    /// A rejected effect leaves only day zero and advances no time.
    pub samples: Vec<WorldSample>,
}

/// The date and the selected flags and variables after one engine day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorldSample {
    /// Days advanced since the prepared effect, starting at zero.
    pub day: u32,
    /// Date read from the engine, in year.month.day form.
    pub date: String,
    /// Selected flags in request order.
    pub flags: Vec<WorldFlag>,
    /// Selected variables in request order.
    #[serde(default)]
    pub variables: Vec<WorldVariable>,
}

/// One named flag in the country's store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorldFlag {
    /// Requested flag name.
    pub name: String,
    /// Stored signed count, or `None` when the flag is absent. Zero and negative counts
    /// are preserved without inferring an expiry date.
    pub remaining: Option<i32>,
}

/// One named variable, read as the engine reads a variable operand in the country scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorldVariable {
    /// Requested variable name.
    pub name: String,
    /// Stored value, or `None` when the variable is not set.
    pub value: Option<WorldFixedPoint>,
}

/// An engine fixed-point number. The represented value is `raw / scale`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorldFixedPoint {
    /// Stored signed integer.
    pub raw: i64,
    /// Number of raw units in one whole unit on the observed build.
    pub scale: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> WorldRequest {
        WorldRequest {
            save: PathBuf::from("/private/fixture.sav"),
            country: "Earth".into(),
            effect: String::new(),
            days: 120,
            flags: vec!["native_flag".into()],
            variables: vec!["native_variable".into()],
        }
    }

    #[test]
    fn requests_refuse_ambiguous_flags_and_unbounded_world_updates() {
        assert!(request().validate().is_ok());
        let mut duplicate = request();
        duplicate.flags.push("native_flag".into());
        assert!(duplicate.validate().is_err());
        let mut too_long = request();
        too_long.days = 121;
        assert!(too_long.validate().is_err());
        let mut relative = request();
        relative.save = PathBuf::from("fixture.sav");
        assert!(relative.validate().is_err());
    }

    #[test]
    fn requests_refuse_ambiguous_malformed_and_unbounded_variable_names() {
        let mut duplicate = request();
        duplicate.variables.push("native_variable".into());
        assert!(duplicate.validate().is_err());
        let mut qualified = request();
        qualified.variables = vec!["root.native_variable".into()];
        assert!(qualified.validate().is_err());
        let mut bounded = request();
        bounded.variables = (0..32).map(|index| format!("native_{index}")).collect();
        assert!(bounded.validate().is_ok());
        bounded.variables.push("native_32".into());
        assert!(bounded.validate().is_err());
    }

    #[test]
    fn requests_refuse_embedded_nulls_and_oversized_effects() {
        let mut null = request();
        null.effect = "set_country_flag = native_flag\0".into();
        assert!(null.validate().is_err());
        let mut oversized = request();
        oversized.effect = " ".repeat(crate::script::MAX_TEXT_BYTES + 1);
        assert!(oversized.validate().is_err());
    }

    #[test]
    fn recordings_identify_save_contents_and_every_observation_input() {
        let original = request();
        let subject = original.recorded_subject(b"original save");
        assert_ne!(subject, original.recorded_subject(b"changed save"));
        let mut relocated = original.clone();
        relocated.save = "/another/fixture.sav".into();
        assert_eq!(subject, relocated.recorded_subject(b"original save"));
        let mut changes = vec![original.clone(); 5];
        changes[0].country = "Another country".into();
        changes[1].effect = "set_country_flag = other".into();
        changes[2].days = 1;
        changes[3].flags.push("another_flag".into());
        changes[4].variables.push("another_variable".into());
        for changed in changes {
            assert_ne!(subject, changed.recorded_subject(b"original save"));
        }
    }
}
