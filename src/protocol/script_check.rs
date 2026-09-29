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
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CheckReply {
    pub attempt: String,
    pub check: u64,
    pub result: Result<crate::ScriptObservation, String>,
}
