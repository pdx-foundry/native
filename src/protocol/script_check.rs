//! Exact-build call data and the private request/reply files for a script check.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[cfg_attr(test, derive(Default))]
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CallBinding {
    pub address: u64,
    /// ARM64 integer-register arguments, in order. Pointer and scope arguments are 64 bits.
    pub widths: Vec<u8>,
}

#[cfg_attr(test, derive(Default))]
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ObjectWrite {
    pub offset: u64,
    pub width: usize,
    pub value: u64,
    pub relocate: bool,
}

#[cfg_attr(test, derive(Default))]
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CommandBinding {
    pub size: usize,
    pub constructor: CallBinding,
    pub read: CallBinding,
    pub writes: Vec<ObjectWrite>,
    pub children_offset: u64,
    /// Owner offset of the pointer to the top-level children, one 8-byte pointer each.
    pub children_array_offset: u64,
    pub validation: Vec<DatabaseBinding>,
}

#[cfg_attr(test, derive(Default))]
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DatabaseBinding {
    pub instance: u64,
    pub post_init: CallBinding,
    pub post_validate: CallBinding,
}

#[cfg_attr(test, derive(Default))]
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScriptCheckBinding {
    pub string_constructor: CallBinding,
    pub string_assign: CallBinding,
    pub blob_constructor: CallBinding,
    pub blob_append: CallBinding,
    pub file_constructor: CallBinding,
    pub lexer_constructor: CallBinding,
    pub reader_constructor: CallBinding,
    pub string_size: usize,
    pub blob_size: usize,
    pub file_size: usize,
    pub lexer_size: usize,
    pub reader_size: usize,
    pub file_arguments: [u64; 3],
    pub lexer_argument: u64,
    pub file_name_offset: u64,
    pub string_tag_offset: u64,
    pub logger_entry: u64,
    pub logger_text_register: String,
    pub logger_level_register: String,
    pub trigger: CommandBinding,
    pub effect: CommandBinding,
    pub scopes: BTreeMap<String, u64>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CheckRequest {
    pub attempt: String,
    pub check: u64,
    pub kind: String,
    pub scope: u64,
    pub text: String,
    /// Receivers of the commands that the text names, at most `MAX_DURATION_RECEIVERS`.
    pub durations: Vec<DurationReceiver>,
}

/// The most receivers that one check carries.
pub(crate) const MAX_DURATION_RECEIVERS: usize = 64;

/// The duration slots of one command receiver, which the worker matches by vtable.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DurationReceiver {
    /// File address of the receiver's vtable address point.
    pub vtable: u64,
    /// The groups whose slots the worker reads from a matched child.
    pub groups: Vec<DurationSlots>,
    /// False when a duration group of this receiver has no slots here, or is in a nested block.
    pub groups_complete: bool,
}

/// Where one duration group of a receiver keeps its count and shared factor.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DurationSlots {
    /// The group's unit keys, returned with each read so the answer names its group.
    pub units: Vec<String>,
    /// Owner offset and decoder of the count slot.
    pub count: super::observation::FixtureStorageBinding,
    /// Owner offset of the signed 32-bit shared factor.
    pub factor_offset: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CheckReply {
    pub attempt: String,
    pub check: u64,
    pub result: Result<CheckedScript, String>,
}

/// What the worker observed in one check.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CheckedScript {
    pub observation: crate::ScriptObservation,
    pub durations: StoredDurations,
}

/// The duration counts that the worker read from the top-level children.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoredDurations {
    /// Whether every child matched a receiver whose groups are complete, and every read succeeded.
    pub complete: bool,
    pub stored: Vec<crate::StoredDuration>,
}

impl CheckedScript {
    /// The public observation, with the stored durations as a known or partial property.
    pub(crate) fn into_observation(self) -> crate::ScriptObservation {
        let stored = self.durations.stored;
        let stored_durations = if self.durations.complete {
            crate::GrammarProperty::Known(stored)
        } else {
            crate::GrammarProperty::Partial(stored)
        };

        crate::ScriptObservation {
            stored_durations,
            ..self.observation
        }
    }
}
