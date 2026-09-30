//! The exact-build world recipe and the worker's prepared observation.
use super::script_check::CallBinding;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorldBinding {
    pub pause_entry: u64,
    pub normal_stack: Vec<String>,
    pub game_state: u64,
    pub idler: u64,
    pub ready_offset: u64,
    pub paused_offset: u64,
    pub date_offset: u64,
    pub local_human: CallBinding,
    pub human_country: CallBinding,
    pub country_id_offset: u64,
    pub human_country_offset: u64,
    pub country_name: CallBinding,
    pub scope_constructor: CallBinding,
    pub scope_country: CallBinding,
    pub scope_size: usize,
    pub scope_type: u64,
    pub scope_type_offset: u64,
    pub scope_id_offset: u64,
    pub scope_flags: CallBinding,
    pub effect_execute: CallBinding,
    pub fast_forward: CallBinding,
    pub date_string: CallBinding,
    pub flags_data_offset: u64,
    pub flags_count_offset: u64,
    pub counts_data_offset: u64,
    pub counts_count_offset: u64,
    pub flag_width: usize,
    pub flag_name: CallBinding,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorldSetup {
    pub binding: WorldBinding,
    pub input: crate::WorldRequest,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorldResult {
    pub attempt: String,
    pub game: u32,
    pub thread: u64,
    pub observation: crate::WorldObservation,
}
