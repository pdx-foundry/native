//! Bind numeric reader functions and external helper identities on the supported recipe.
use super::{declarations::Text, references::Image};
use crate::AnalysisError;
use crate::binding::targets::DeclarationRecipe;
use crate::engine::analysis::{
    decode::{Instruction, decode_arm64},
    discovery::Symbol,
    numeric::{ModifierInput, NumericInput, ReaderInput, TokenInput},
    references::shapes::canonical,
};
use std::collections::{BTreeMap, BTreeSet};

pub(in crate::binding) fn read(
    image: &Image<'_>,
    recipe: &DeclarationRecipe,
) -> Result<NumericInput, AnalysisError> {
    let text = Text::read(image.bytes, image.symbols)?;
    let names =
        super::references::names(image.symbols, image.pointers, image.imports, image.strings);
    let readers: BTreeMap<String, ReaderInput> = recipe
        .numeric_types
        .iter()
        .map(|value_type| reader_input(&text, image.symbols, &names, value_type, recipe))
        .collect::<Result<_, _>>()?;
    let modifier = modifier_input(&text, image.symbols, &names, recipe)?;
    let token_readers = recipe
        .numeric_types
        .iter()
        .filter_map(|value_type| {
            let wrapper = format!("CReader::Read({value_type}&)");
            let input = readers.get(&wrapper)?;
            let name = format!("CToken::ReadValue({value_type}&) const");
            Some((
                name,
                TokenInput {
                    body: input.token.clone(),
                    names: input.names.clone(),
                    token_text_offset: input.token_text_offset,
                },
            ))
        })
        .collect();
    Ok(NumericInput {
        readers,
        modifier,
        token_readers,
    })
}

fn reader_input(
    text: &Text<'_>,
    symbols: &[Symbol],
    names: &BTreeMap<u64, String>,
    value_type: &str,
    recipe: &DeclarationRecipe,
) -> Result<(String, ReaderInput), AnalysisError> {
    let wrapper_name = format!("CReader::Read({value_type}&)");
    let token_name = format!("CToken::ReadValue({value_type}&) const");
    let raw_name = "CToken::ReadValue(long long&) const";
    let wrapper = decode_unique_symbol_or_empty(text, symbols, &wrapper_name)?;
    let token = decode_unique_symbol_or_empty(text, symbols, &token_name)?;
    let raw_token = decode_unique_symbol_or_empty(text, symbols, raw_name)?;
    let cold_one = format!("{wrapper_name} [clone .cold.1]");
    let cold_two = format!("{wrapper_name} [clone .cold.2]");
    // The scanner contract supplies C-format storage types and representable conversion
    // forms, not overflow, locale, trailing-text or lexer acceptance rules.
    let aliases = [
        (token_name.as_str(), "token_conversion"),
        (raw_name, "raw_conversion"),
        (cold_one.as_str(), "cold_one"),
        (cold_two.as_str(), "cold_two"),
        ("_sscanf", "scan"),
        ("_strchr", "find_character"),
        ("_atoll", "decimal_integer"),
        ("CString::CString(char const*)", "diagnostic_string"),
        (
            "CReader::ReportMalformed(CString const&)",
            "report_malformed",
        ),
        (
            "CPdxCommonStringAllocator::deallocate(char*, unsigned long)",
            "deallocate",
        ),
        ("__Unwind_Resume", "unwind"),
    ];
    let names = referenced_names(&[&wrapper, &token, &raw_token], names, &aliases);
    Ok((
        wrapper_name,
        ReaderInput {
            wrapper,
            token,
            raw_token,
            names,
            reader_token_offset: recipe.reader_value_token_offset,
            token_text_offset: recipe.token_text_offset,
        },
    ))
}

pub(super) fn modifier_input(
    text: &Text<'_>,
    symbols: &[Symbol],
    names: &BTreeMap<u64, String>,
    recipe: &DeclarationRecipe,
) -> Result<ModifierInput, AnalysisError> {
    let member = decode_unique_symbol_or_empty(text, symbols, recipe.numeric_modifier_member)?;
    let insert = decode_unique_symbol_or_empty(text, symbols, recipe.numeric_modifier_insert)?;
    let shared_callee = "CReader::Read(CFixedPoint&)";
    let modifier_class = recipe
        .numeric_modifier_member
        .strip_suffix("::TryReadMember(CReader&, int)")
        .ok_or(AnalysisError::InvalidRange)?;
    let definitions = format!("{modifier_class}::_Definitions");
    let cold = format!("{} [clone .cold.1]", recipe.numeric_modifier_member);
    let aliases = [
        (definitions.as_str(), "definitions"),
        (cold.as_str(), "cold_cleanup"),
        (recipe.numeric_modifier_insert, "insert_entry"),
        (shared_callee, "numeric_reader"),
        ("CReader::Read(int&)", "other_integer_reader"),
        ("operator new[](unsigned long)", "allocate_array"),
        ("_memcpy", "copy_bytes"),
        (
            "CPdxLogFileAndLine::CPdxLogFileAndLine(char const*, unsigned int, unsigned int)",
            "diagnostic_location",
        ),
        (
            "CReader::GetFileLocationDescription() const",
            "reader_location",
        ),
        (
            "CPdxLogFileAndLine::operator()(char const*, ...)",
            "report_category",
        ),
        (
            "CPdxCommonStringAllocator::deallocate(char*, unsigned long)",
            "deallocate",
        ),
        ("__Unwind_Resume", "unwind"),
    ];
    let names = referenced_names(&[&member, &insert], names, &aliases);
    Ok(ModifierInput {
        member,
        insert,
        names,
        shared_callee: shared_callee.into(),
    })
}

/// Retain only addresses used by these bodies, with their bound semantic helper identities.
fn referenced_names(
    functions: &[&[Instruction]],
    names: &BTreeMap<u64, String>,
    aliases: &[(&str, &str)],
) -> BTreeMap<u64, String> {
    let referenced: BTreeSet<_> = functions
        .iter()
        .flat_map(|rows| canonical(rows, names))
        .filter_map(|line| line.value)
        .collect();
    names
        .iter()
        .filter(|(_, name)| referenced.contains(*name))
        .map(|(address, name)| {
            let alias = aliases
                .iter()
                .find(|(original, _)| original == name)
                .map(|(_, alias)| *alias)
                .unwrap_or(name);
            (*address, alias.to_owned())
        })
        .collect()
}

fn decode_unique_symbol_or_empty(
    text: &Text<'_>,
    symbols: &[Symbol],
    name: &str,
) -> Result<Vec<Instruction>, AnalysisError> {
    let addresses: BTreeSet<_> = symbols
        .iter()
        .filter(|symbol| symbol.name == name)
        .map(|symbol| symbol.address)
        .collect();
    if addresses.len() != 1 {
        return Ok(Vec::new());
    }
    let (address, bytes) = text.function(*addresses.first().unwrap())?;
    decode_arm64(bytes, address).map_err(|_| AnalysisError::InvalidRange)
}
