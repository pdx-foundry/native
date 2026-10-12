//! Resolve the named readers and decode their complete bodies for layout analysis.
use super::{
    declarations::{Text, read_only_data, unique},
    families::{DEFINITIONS, string_functions},
};
use crate::AnalysisError;
use crate::binding::MODIFIER_LOG_DEFINITIONS;
use crate::engine::analysis::{
    decode::decode_arm64, discovery::Symbol, modifier_table::Input, strings::StringLayout,
};
use std::collections::BTreeMap;

/// The table method's input, and the entry of `LogDefinitions`, which returns once content
/// validation has completed the loaded table.
pub(in crate::binding) fn read(
    bytes: &[u8],
    symbols: &[Symbol],
    pointers: &BTreeMap<u64, u64>,
    string_tag_offset: u64,
) -> Result<(Input, u64), AnalysisError> {
    let text = Text::read(bytes, symbols)?;
    let rows = |entry| {
        let (address, bytes) = text.function(entry)?;
        decode_arm64(bytes, address).map_err(|_| AnalysisError::InvalidRange)
    };
    let documentation_entry = unique(symbols, MODIFIER_LOG_DEFINITIONS)?;
    let input = Input {
        documentation: rows(documentation_entry)?,
        get_string: rows(unique(symbols, "CStaticLexer::GetString(int)")?)?,
        definitions: unique(symbols, DEFINITIONS)?,
        category_name: unique(
            symbols,
            "(anonymous namespace)::GetModifierCategoryName(ModifierCategory, CString&)",
        )?,
        rebuild_lookup: unique(symbols, "CStaticLexer::RebuildLookup()")?,
        logger: unique(symbols, "CLogger::AccessInstance()")?,
        pointers: pointers.clone(),
        data: read_only_data(bytes)?,
        strings: string_functions(symbols),
        string_layout: StringLayout {
            flag_byte: string_tag_offset,
        },
    };

    Ok((input, documentation_entry))
}
