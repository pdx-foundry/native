//! Prepared world observations and their normalized results.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A fixed observation prepared before launch. Native copies the save into a private profile,
/// executes the effect in the named local human country, and reads flags after each game day.
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
}

impl WorldRequest {
    pub(crate) fn validate(&self) -> Result<(), crate::Error> {
        let valid = self.save.is_absolute()
            && !self.country.is_empty()
            && self.country.len() <= 256
            && !self.country.contains('\0')
            && self.effect.len() <= crate::script::MAX_TEXT_BYTES
            && !self.effect.contains('\0')
            && self.days <= 120
            && self.flags.len() <= 32
            && self.flags.iter().all(|name| {
                !name.is_empty()
                    && name.len() <= 128
                    && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            })
            && self
                .flags
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                == self.flags.len();
        if !valid {
            return Err(crate::Error::Observation { operation: crate::Operation::ObserveWorld,
                reason: "world request needs an absolute save, a country name, at most 4 KiB of effect text, 0 to 120 days and at most 32 unique flag names".into() });
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

/// The date and selected flags after one engine day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorldSample {
    /// Days advanced since the prepared effect, starting at zero.
    pub day: u32,
    /// Date read from the engine, in year.month.day form.
    pub date: String,
    /// Selected flags in request order.
    pub flags: Vec<WorldFlag>,
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
    fn requests_refuse_embedded_nulls_and_oversized_effects() {
        let mut null = request();
        null.effect = "set_country_flag = native_flag\0".into();
        assert!(null.validate().is_err());
        let mut oversized = request();
        oversized.effect = " ".repeat(crate::script::MAX_TEXT_BYTES + 1);
        assert!(oversized.validate().is_err());
    }
}
