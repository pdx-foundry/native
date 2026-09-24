//! Resolve the named readers and decode their complete bodies for layout analysis.
use super::{
    declarations::{Text, read_only_data, unique},
    families::string_functions,
};
use crate::AnalysisError;
use crate::engine::analysis::{
    decode::decode_arm64, discovery::Symbol, families::StringLayout, modifier_table::Input,
};
use std::collections::BTreeMap;

pub(in crate::binding) fn read(
    bytes: &[u8],
    symbols: &[Symbol],
    pointers: &BTreeMap<u64, u64>,
    string_tag_offset: u64,
) -> Result<Input, AnalysisError> {
    let text = Text::read(bytes, symbols)?;
    let rows = |name| {
        let (address, bytes) = text.function(unique(symbols, name)?)?;
        decode_arm64(bytes, address).map_err(|_| AnalysisError::InvalidRange)
    };
    Ok(Input {
        documentation: rows("CModifier::LogDefinitions()")?,
        get_string: rows("CStaticLexer::GetString(int)")?,
        definitions: unique(
            symbols,
            "CPdxModifier<ModifierType, ModifierCategory, CModifier, CDefaultPdxModifierValueReader>::_Definitions",
        )?,
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
    })
}
