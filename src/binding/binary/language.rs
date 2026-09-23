//! Read the inputs of the modifier, category, scope and scope-link methods from executable text.
use std::collections::{BTreeMap, BTreeSet};

use object::{Object, ObjectSection, SectionKind};

use crate::AnalysisError;
use crate::engine::analysis::{
    decode::decode_arm64,
    discovery::Symbol,
    evaluate::{Code, ReadOnlyData},
    localization::{LocalizationFunctions, LocalizationInput},
    modifiers::{CategoryInput, ModifierInput},
    scopes::{ScopeFunctions, ScopeInput},
};

use super::super::targets::DeclarationRecipe;
use super::declarations::{Text, addresses, read_only_data, unique};

const DEFINE_MODIFIER: &str = "CPdxModifier<ModifierType, ModifierCategory, CModifier, CDefaultPdxModifierValueReader>::AddDefinition(int, ModifierType, CString const&, bool, bool, bool, int, bool, bool, ModifierCategory, bool, bool, CFixedPoint, bool)";
/// The functions that register a modifier generated from content. The first registers a name
/// that may exist already.
pub(super) const GENERATE_MODIFIER: [&str; 2] = [
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

    let generation_sites = generation_calls(&text, symbols)?.len();

    Ok(ModifierInput {
        tokens: text.token_names(symbols, strings)?,
        definition_sites,
        define,
        category_offset: recipe.modifier_category_offset,
        generation_sites,
        categories: CategoryInput {
            category_name,
            string_object_size: recipe.string_object_size,
            short_length_offset: recipe.short_string_length_offset,
            assign_literal,
            code: code(&text, &[category_name])?,
            data: read_only_data(bytes)?,
        },
    })
}

/// Every direct call to a function that registers a generated modifier, in address order.
pub(super) fn generation_calls(text: &Text, symbols: &[Symbol]) -> Result<Vec<u64>, AnalysisError> {
    let mut calls = Vec::new();
    for name in GENERATE_MODIFIER {
        calls.extend(text.direct_calls(unique(symbols, name)?));
    }
    calls.sort();
    Ok(calls)
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

pub(super) fn code(text: &Text, functions: &[u64]) -> Result<Code, AnalysisError> {
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

const SCOPE_OBJECT: &str = "CGameText::SetScopeObject(CScopeObjectReference const&)";
const LINK_FUNCTION: &str = "(void const*, CGameText&, int)";

/// Read the text object's constructor, the functions that its tables can name, the setters, and
/// the data that holds names and rows.
///
/// The functions are found by their signatures: row getters take `int&`, link functions take
/// `(void const*, CGameText&, int)`, and setters are the text object's `Set` members and every
/// other function that takes the text object. The method decides which of them a context uses.
/// A function that does not decode is left out, so a run that reaches it is unresolved.
pub(in crate::binding) fn localization(
    bytes: &[u8],
    symbols: &[Symbol],
    strings: &BTreeMap<u64, String>,
    pointers: &BTreeMap<u64, u64>,
    bound_slots: &BTreeSet<u64>,
    recipe: &DeclarationRecipe,
) -> Result<LocalizationInput, AnalysisError> {
    let text = Text::read(bytes, symbols)?;
    let scope_object = unique(symbols, SCOPE_OBJECT)?;
    let context_name = unique(
        symbols,
        "CGameText::GetStringForCurrentPointer(ECURRENT_POINTER)",
    )?;
    let constructors = addresses(symbols, "CGameText::CGameText()");
    let text_constructor = *constructors.first().ok_or(AnalysisError::InvalidRange)?;
    let setters = matching(symbols, |name| {
        let takes_text = name.contains("CGameText&") && !name.ends_with(LINK_FUNCTION);
        (name.starts_with("CGameText::Set") || takes_text) && name != SCOPE_OBJECT
    });
    let row_getters = matching(symbols, |name| {
        name.ends_with("PromotionTargets(int&)") || name.ends_with("PropertyTargets(int&)")
    });
    let link_functions = matching(symbols, |name| name.ends_with(LINK_FUNCTION));

    let run_code = decoded(
        &text,
        constructors
            .iter()
            .chain(&row_getters)
            .chain(&link_functions)
            .chain(&setters)
            .chain([&context_name]),
    );
    let scope_object_code = decoded(&text, setters.iter().chain([&scope_object]));
    let fixups = Fixups {
        pointers,
        bound_slots,
    };
    let data = loaded_data(bytes, &fixups, |segment, kind| {
        is_read_only(kind) || segment == "__DATA_CONST"
    })?;
    let rows = loaded_data(bytes, &fixups, |segment, kind| {
        is_read_only(kind) || segment == "__DATA_CONST" || segment == "__DATA"
    })?;

    Ok(LocalizationInput {
        functions: LocalizationFunctions {
            text_constructor,
            context_name,
            string_from_literal: addresses(symbols, "CString::CString(char const*)"),
            scope_object,
            setters,
            scope_object_getters: matching(symbols, |name| {
                name.starts_with("CScopeObjectReference::Get")
            }),
        },
        layout: recipe.game_text,
        code: run_code,
        scope_object_code,
        data,
        rows,
        scope_names: text.scope_names(symbols, strings),
    })
}

fn matching(symbols: &[Symbol], select: impl Fn(&str) -> bool) -> BTreeSet<u64> {
    symbols
        .iter()
        .filter(|symbol| !symbol.name.contains(".cold.") && select(&symbol.name))
        .map(|symbol| symbol.address)
        .collect()
}

/// Decode each function on its own and keep the ones that decode completely.
fn decoded<'a>(text: &Text, starts: impl IntoIterator<Item = &'a u64>) -> Code {
    let mut rows = Vec::new();
    for &start in starts {
        let Ok((address, code)) = text.function(start) else {
            continue;
        };
        let function: Result<Vec<_>, _> = code
            .chunks(4096)
            .enumerate()
            .map(|(index, chunk)| decode_arm64(chunk, address + (index * 4096) as u64))
            .collect();
        if let Ok(function) = function {
            rows.extend(function.into_iter().flatten());
        }
    }
    Code::from_rows(rows)
}

fn is_read_only(kind: SectionKind) -> bool {
    matches!(
        kind,
        SectionKind::ReadOnlyData | SectionKind::ReadOnlyString
    )
}

/// The chained-fixup facts that the loaded-data views need.
struct Fixups<'a> {
    /// Rebased pointer locations and their targets.
    pointers: &'a BTreeMap<u64, u64>,
    /// Every pointer location that the loader binds to another image.
    bound_slots: &'a BTreeSet<u64>,
}

/// The bytes of the selected sections as the loader leaves them: each rebased pointer holds its
/// target. A pointer slot that the loader binds to another image, whose value is not known here,
/// is left out, so it reads as unknown.
fn loaded_data(
    bytes: &[u8],
    fixups: &Fixups,
    select: impl Fn(&str, SectionKind) -> bool,
) -> Result<ReadOnlyData, AnalysisError> {
    let slice = super::selected_slice(bytes).map_err(|_| AnalysisError::InvalidRange)?;
    let file = object::File::parse(slice).map_err(|_| AnalysisError::InvalidRange)?;
    let mut sections = Vec::new();

    for section in file.sections() {
        let segment = section
            .segment_name()
            .map_err(|_| AnalysisError::InvalidRange)?
            .unwrap_or_default();
        if !select(segment, section.kind()) {
            continue;
        }

        let Ok(data) = section.data() else {
            continue;
        };
        sections.extend(resolved_runs(section.address(), data, fixups));
    }

    Ok(ReadOnlyData::new(sections))
}

/// Split `data` into runs of known bytes, with rebased pointers resolved and bound slots removed.
fn resolved_runs(start: u64, data: &[u8], fixups: &Fixups) -> Vec<(u64, Vec<u8>)> {
    let mut bytes = data.to_vec();
    let end = start + data.len() as u64;

    for (&address, &target) in fixups.pointers.range(start..end) {
        if address + 8 > end {
            continue;
        }

        let offset = (address - start) as usize;
        bytes[offset..offset + 8].copy_from_slice(&target.to_le_bytes());
    }

    let mut runs = Vec::new();
    let mut run_start = 0;
    for &address in fixups.bound_slots.range(start..end) {
        if fixups.pointers.contains_key(&address) {
            continue;
        }

        let offset = (address - start) as usize;
        if offset > run_start {
            runs.push((start + run_start as u64, bytes[run_start..offset].to_vec()));
        }
        run_start = run_start.max(offset + 8);
    }
    if run_start < bytes.len() {
        runs.push((start + run_start as u64, bytes[run_start..].to_vec()));
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loaded_data_resolves_rebases_and_drops_only_bound_slots() {
        let mut data = Vec::new();
        data.extend(0x8000_0000_0000_0001u64.to_le_bytes()); // rebase, encoded
        data.extend(0x8000_0000_0000_0002u64.to_le_bytes()); // bind, encoded
        data.extend(u64::MAX.to_le_bytes()); // ordinary data with its top bit set
        let pointers = BTreeMap::from([(0x1000, 0x1_0000_4000)]);
        let bound_slots = BTreeSet::from([0x1008]);
        let fixups = Fixups {
            pointers: &pointers,
            bound_slots: &bound_slots,
        };

        let view = ReadOnlyData::new(resolved_runs(0x1000, &data, &fixups));

        assert_eq!(view.read(0x1000, 8), Some(0x1_0000_4000));
        assert_eq!(view.read(0x1008, 8), None);
        assert_eq!(view.read(0x1010, 8), Some(u64::MAX));
    }
}
