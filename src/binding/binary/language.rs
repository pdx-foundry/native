//! Read the inputs of the modifier, category, scope and scope-link methods from executable text.
use std::collections::BTreeMap;

use crate::AnalysisError;
use crate::engine::analysis::{
    decode::decode_arm64,
    discovery::Symbol,
    evaluate::Code,
    modifiers::ModifierInput,
    scopes::{ScopeFunctions, ScopeInput},
};

use super::super::targets::DeclarationRecipe;
use super::declarations::{Text, addresses, read_only_data, unique};

const DEFINE_MODIFIER: &str = "CPdxModifier<ModifierType, ModifierCategory, CModifier, CDefaultPdxModifierValueReader>::AddDefinition(int, ModifierType, CString const&, bool, bool, bool, int, bool, bool, ModifierCategory, bool, bool, CFixedPoint, bool)";
const GENERATE_MODIFIER: [&str; 2] = [
    "CModifier::TryAddDynamicModifier(ModifierType&, CString const&, bool, bool, bool, int, bool, bool, ModifierCategory, bool, CFixedPoint, bool)",
    "CModifier::AddDynamicModifier(CString const&, bool, bool, bool, int, bool, bool, ModifierCategory, bool, CFixedPoint, bool)",
];
const CATEGORY_NAME: &str =
    "(anonymous namespace)::GetModifierCategoryName(ModifierCategory, CString&)";
const ASSIGN_LITERAL: &str = "std::__1::basic_string<char, std::__1::char_traits<char>, CPdxCommonStringAllocator>::__assign_external(char const*, unsigned long)";

/// Read every direct modifier definition call and the category-name function.
pub(in crate::binding) fn modifiers(
    bytes: &[u8],
    symbols: &[Symbol],
    strings: &BTreeMap<u64, String>,
    recipe: &DeclarationRecipe,
) -> Result<ModifierInput, AnalysisError> {
    let define = unique(symbols, DEFINE_MODIFIER)?;
    let category_name = unique(symbols, CATEGORY_NAME)?;
    let assign_literal = addresses(symbols, ASSIGN_LITERAL);
    let text = Text::read(bytes, symbols)?;

    let mut definition_sites = Vec::new();
    for call in text.direct_calls(define) {
        definition_sites.push(straight_line_before(&text, call)?);
    }

    let mut generation_sites = 0;
    for name in GENERATE_MODIFIER {
        generation_sites += text.direct_calls(unique(symbols, name)?).len();
    }

    Ok(ModifierInput {
        tokens: text.token_names(symbols, strings)?,
        definition_sites,
        define,
        category_offset: recipe.modifier_category_offset,
        generation_sites,
        category_name,
        assign_literal,
        code: code(&text, &[category_name])?,
        data: read_only_data(bytes)?,
    })
}

/// Read the scope-name table and the link, scope and special-value functions.
pub(in crate::binding) fn scopes(
    bytes: &[u8],
    symbols: &[Symbol],
    strings: &BTreeMap<u64, String>,
    recipe: &DeclarationRecipe,
) -> Result<ScopeInput, AnalysisError> {
    let text = Text::read(bytes, symbols)?;
    let functions = ScopeFunctions {
        scope_of_token: unique(symbols, "GetScopeTypeEnumFromToken(int)")?,
        link_documentation: unique(
            symbols,
            "CEventTarget::GenerateEventTargetDocumentation(CString&)",
        )?,
        supported_scopes: unique(symbols, "CEventTarget::GetSupportedScopes() const")?,
        output_scope: unique(symbols, "CEventTarget::GetScopeType(int, char const*)")?,
        target_constructor: addresses(symbols, "CEventTarget::CEventTarget(int)"),
        target_destructor: addresses(symbols, "CEventTarget::~CEventTarget()"),
        target_documentation: unique(symbols, "CEventTarget::GetTargetDocumentation(int)")?,
        token_type_count: unique(symbols, "GetNoOfTokenTypes()")?,
        token_type: unique(symbols, "GetTokenType(int)")?,
    };
    if functions.target_constructor.is_empty() {
        return Err(AnalysisError::InvalidRange);
    }

    let special_values = unique(
        symbols,
        "CEventTarget::ParseForSpecialValues(EScopeType, CString const&)",
    )?;
    let (address, special_code) = text.function(special_values)?;
    let mut special_rows = Vec::new();
    for (index, chunk) in special_code.chunks(4096).enumerate() {
        special_rows.extend(
            decode_arm64(chunk, address + (index * 4096) as u64)
                .map_err(|_| AnalysisError::InvalidRange)?,
        );
    }

    Ok(ScopeInput {
        tokens: text.token_names(symbols, strings)?,
        scope_names: text.scope_names(symbols, strings),
        code: code(
            &text,
            &[
                functions.scope_of_token,
                functions.link_documentation,
                functions.supported_scopes,
                functions.output_scope,
            ],
        )?,
        functions,
        token_offset: recipe.event_target_token_offset,
        special_values: special_rows,
        data: read_only_data(bytes)?,
    })
}

/// The straight-line code from the previous call up to and including the call at `call`.
fn straight_line_before(
    text: &Text,
    call: u64,
) -> Result<Vec<crate::engine::analysis::decode::Instruction>, AnalysisError> {
    let window = text.window_ending_at(call, 1024)?;
    let rows =
        decode_arm64(&window.code, window.address).map_err(|_| AnalysisError::InvalidRange)?;
    let before = &rows[..rows.len() - 1];
    let start = before
        .iter()
        .rposition(|row| is_control_transfer(&row.operation))
        .map_or(0, |index| index + 1);
    Ok(rows[start..].to_vec())
}

fn code(text: &Text, functions: &[u64]) -> Result<Code, AnalysisError> {
    let ranges: Vec<_> = functions
        .iter()
        .map(|&start| text.function(start))
        .collect::<Result<_, _>>()?;
    Code::decode(&ranges).map_err(AnalysisError::Input)
}

fn is_control_transfer(operation: &str) -> bool {
    operation.starts_with("b.")
        || matches!(
            operation,
            "b" | "bl" | "blr" | "br" | "ret" | "cbz" | "cbnz" | "tbz" | "tbnz"
        )
}
