use std::collections::{BTreeMap, BTreeSet};

use object::{Object, ObjectSection, SectionKind};

use crate::engine::analysis::{
    declarations::{DeclarationInput, Function, ScopeSlots},
    decode::decode_arm64,
    discovery::Symbol,
    fields::literal_token_names,
};
use crate::{AnalysisError, DeclarationKind};

use super::super::targets::DeclarationRecipe;

/// Read every direct registration call in executable text, including calls in compiler outlined
/// regions that have no reliable symbol boundary.
pub(in crate::binding) fn read(
    bytes: &[u8],
    symbols: &[Symbol],
    strings: &BTreeMap<u64, String>,
    pointers: &BTreeMap<u64, u64>,
    kind: DeclarationKind,
    recipe: &DeclarationRecipe,
) -> Result<DeclarationInput, AnalysisError> {
    let registrar_name = match kind {
        DeclarationKind::Effect => "CEffectDatabase::RegisterEffectEntry(int, CEffectEntryBase*)",
        DeclarationKind::Trigger => {
            "CTriggerDatabase::RegisterTriggerEntry(int, CTriggerEntryBase*)"
        }
    };
    let register_entry = unique(symbols, registrar_name)?;
    let operator_new: BTreeSet<_> = symbols
        .iter()
        .filter(|symbol| symbol.name == "operator new(unsigned long)")
        .map(|symbol| symbol.address)
        .collect();
    if operator_new.is_empty() {
        return Err(AnalysisError::InvalidRange);
    }
    let token_start = unique(symbols, "GetTokenArray()")?;
    let slice = super::selected_slice(bytes).map_err(|_| AnalysisError::InvalidRange)?;
    let file = object::File::parse(slice).map_err(|_| AnalysisError::InvalidRange)?;
    let mut text = None;
    for section in file
        .sections()
        .filter(|section| section.kind() == SectionKind::Text)
    {
        if text
            .replace((
                section.address(),
                section.data().map_err(|_| AnalysisError::InvalidRange)?,
            ))
            .is_some()
        {
            return Err(AnalysisError::InvalidRange);
        }
    }
    let (text_address, code) = text.ok_or(AnalysisError::InvalidRange)?;
    let text_end = text_address
        .checked_add(code.len() as u64)
        .ok_or(AnalysisError::InvalidRange)?;
    let function_starts: BTreeSet<_> = symbols
        .iter()
        .map(|symbol| symbol.address)
        .filter(|address| *address >= text_address && *address < text_end)
        .collect();
    let function_end = function_starts
        .range((token_start + 1)..)
        .next()
        .copied()
        .ok_or(AnalysisError::InvalidRange)?;
    let token_code = section_bytes(code, text_address, token_start, function_end - token_start)?;
    let tokens = literal_token_names(token_code, token_start, symbols, strings)
        .map_err(AnalysisError::Input)?;
    let scope_names = scope_names(code, text_address, &function_starts, symbols, strings);

    let mut registrars = Vec::new();
    for (index, bytes) in code.as_chunks::<4>().0.iter().enumerate() {
        let at = text_address + (index * 4) as u64;
        let word = u32::from_le_bytes(*bytes);
        if branch_target(word, at) != Some(register_entry) {
            continue;
        }
        let start = at.saturating_sub(1024).max(text_address);
        let length = at + 4 - start;
        registrars.push(Function {
            address: start,
            code: section_bytes(code, text_address, start, length)?.to_vec(),
        });
    }
    if registrars.is_empty() {
        return Err(AnalysisError::InvalidRange);
    }
    let mut functions = BTreeMap::new();
    for &start in &function_starts {
        let end = function_starts
            .range((start + 1)..)
            .next()
            .copied()
            .unwrap_or(text_end);
        let length = (end - start).min(4096);
        if length == 0 || !length.is_multiple_of(4) {
            continue;
        }
        functions.insert(
            start,
            Function {
                address: start,
                code: section_bytes(code, text_address, start, length)?.to_vec(),
            },
        );
    }
    let scope_slot = match kind {
        DeclarationKind::Effect => recipe.effect_scope_slot,
        DeclarationKind::Trigger => recipe.trigger_scope_slot,
    };
    Ok(DeclarationInput {
        kind,
        tokens,
        registrars,
        register_entry: BTreeSet::from([register_entry]),
        operator_new,
        functions,
        pointers: pointers.clone(),
        strings: strings.clone(),
        slots: ScopeSlots {
            create: recipe.create_slot,
            supported_scopes: scope_slot,
        },
        scope_names,
        symbols_by_address: symbols
            .iter()
            .map(|symbol| (symbol.address, symbol.name.clone()))
            .collect(),
    })
}

fn scope_names(
    code: &[u8],
    base: u64,
    starts: &BTreeSet<u64>,
    symbols: &[Symbol],
    strings: &BTreeMap<u64, String>,
) -> Option<Vec<String>> {
    let start = unique(symbols, "NEventScope::GetScopeName(EScopeType, bool)").ok()?;
    let end = starts.range((start + 1)..).next().copied()?;
    let code = section_bytes(code, base, start, end - start).ok()?;
    let mut rows = Vec::new();
    for (index, chunk) in code.chunks(4096).enumerate() {
        rows.extend(decode_arm64(chunk, start + (index * 4096) as u64).ok()?);
    }
    let mut names = BTreeMap::new();
    for window in rows.windows(3) {
        let [test, page, offset] = window else {
            continue;
        };
        if test.operation != "tbz"
            || !(test.operands.starts_with("w21,#") || test.operands.starts_with("x21,#"))
            || page.operation != "adrp"
            || offset.operation != "add"
            || !page.operands.starts_with("x1,")
            || !offset.operands.starts_with("x1,x1,")
        {
            continue;
        }
        let bit = test.operands.split(',').nth(1).and_then(parse_number)? as usize;
        let page = page.operands.split(',').nth(1).and_then(parse_number)?;
        let offset = offset.operands.split(',').nth(2).and_then(parse_number)?;
        if let Some(name) = strings.get(&(page + offset)) {
            names.insert(bit, name.clone());
        }
    }
    if names.get(&2).map(String::as_str) != Some("country")
        || names.get(&40).map(String::as_str) != Some("colony")
    {
        return None;
    }
    let last = *names.keys().max()?;
    Some(
        (0..=last)
            .map(|bit| names.get(&bit).cloned().unwrap_or_default())
            .collect(),
    )
}

fn parse_number(text: &str) -> Option<u64> {
    let text = text.strip_prefix('#').unwrap_or(text);
    if let Some(hex) = text.strip_prefix("0x") {
        u64::from_str_radix(hex, 16).ok()
    } else {
        text.parse().ok()
    }
}

fn unique(symbols: &[Symbol], name: &str) -> Result<u64, AnalysisError> {
    let addresses: BTreeSet<_> = symbols
        .iter()
        .filter(|symbol| symbol.name == name)
        .map(|symbol| symbol.address)
        .collect();
    (addresses.len() == 1)
        .then(|| *addresses.first().unwrap())
        .ok_or(AnalysisError::InvalidRange)
}

fn section_bytes(code: &[u8], base: u64, start: u64, length: u64) -> Result<&[u8], AnalysisError> {
    let offset = usize::try_from(start.checked_sub(base).ok_or(AnalysisError::InvalidRange)?)
        .map_err(|_| AnalysisError::InvalidRange)?;
    let length = usize::try_from(length).map_err(|_| AnalysisError::InvalidRange)?;
    code.get(
        offset
            ..offset
                .checked_add(length)
                .ok_or(AnalysisError::InvalidRange)?,
    )
    .ok_or(AnalysisError::InvalidRange)
}

fn branch_target(word: u32, address: u64) -> Option<u64> {
    if word >> 26 != 0b100101 {
        return None;
    }
    let signed = ((word << 6) as i32 >> 6) as i64;
    address.checked_add_signed(signed * 4)
}
