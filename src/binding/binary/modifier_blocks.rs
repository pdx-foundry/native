//! Collect shared modifier readers from executable pointer slots and bind the reference database.
use super::{declarations::Text, references::Image};
use crate::binding::{analysis::NamedCandidate, targets::DeclarationRecipe};
use crate::engine::analysis::{
    declarations::Function,
    decode::decode_arm64,
    directories::Directory,
    fields::{self, ConcreteReader},
    modifier_blocks::ModifierBlockInput,
    modifier_blocks::reference,
};
use crate::{AnalysisError, BlockFamily};
use std::collections::{BTreeMap, BTreeSet};

/// The vtable address points whose read slot holds a `family` anchor of the recipe, with the read
/// and member readers of each.
pub(super) fn family_points(
    image: &Image<'_>,
    names: &BTreeMap<u64, String>,
    recipe: &DeclarationRecipe,
    family: BlockFamily,
) -> BTreeMap<u64, ConcreteReader> {
    let mut points = BTreeMap::new();
    for (&slot, &target) in image.pointers {
        let Some(read) = names.get(&target).filter(|name| {
            recipe
                .persistent
                .families
                .iter()
                .any(|(anchor, anchored)| *anchor == name.as_str() && *anchored == family)
        }) else {
            continue;
        };
        let Some(point) = slot.checked_sub(recipe.persistent.read_slot) else {
            continue;
        };
        let Some(member) = image
            .pointers
            .get(&(point + recipe.persistent.member_slot))
            .and_then(|address| names.get(address))
            .filter(|name| name.ends_with("::ReadMember(CReader&, int)"))
        else {
            continue;
        };
        points.insert(
            point,
            ConcreteReader {
                read: read.clone(),
                member: member.clone(),
                family,
            },
        );
    }
    points
}

pub(in crate::binding) fn read(
    image: &Image<'_>,
    candidates: &[NamedCandidate],
    recipe: &DeclarationRecipe,
) -> Result<ModifierBlockInput, AnalysisError> {
    let text = Text::read(image.bytes, image.symbols)?;
    let names =
        super::references::names(image.symbols, image.pointers, image.imports, image.strings);
    let points = family_points(image, &names, recipe, BlockFamily::Modifier);
    let mut functions = BTreeMap::new();
    let mut pending: BTreeSet<_> = points
        .values()
        .map(|reader| reader.member.clone())
        .collect();
    pending.insert(recipe.modifier_reference_member.into());
    pending.insert(recipe.numeric_modifier_member.into());
    while let Some(name) = pending.pop_first() {
        let addresses: BTreeSet<_> = image
            .symbols
            .iter()
            .filter(|symbol| symbol.name == name)
            .map(|symbol| symbol.address)
            .collect();
        if addresses.len() != 1 {
            continue;
        }
        let address = *addresses.first().unwrap();
        if functions.contains_key(&address) {
            continue;
        }
        let Ok((address, code)) = text.function(address) else {
            continue;
        };
        let rows = decode_arm64(code, address).map_err(|_| AnalysisError::InvalidRange)?;
        for row in &rows {
            if !matches!(row.operation.as_str(), "bl" | "b") {
                continue;
            }
            let Some(callee) = crate::engine::analysis::declarations::number(&row.operands)
                .and_then(|target| names.get(&target))
            else {
                continue;
            };
            if callee.ends_with("::ReadMember(CReader&, int)")
                || callee.ends_with("::TryReadMember(CReader&, int)")
            {
                pending.insert(callee.clone());
            }
        }
        functions.insert(
            address,
            Function {
                address,
                code: code.to_vec(),
            },
        );
    }
    let token_address = image
        .symbols
        .iter()
        .find(|symbol| symbol.name == "GetTokenArray()")
        .ok_or(AnalysisError::InvalidRange)?
        .address;
    let (address, code) = text.function(token_address)?;
    let rows = decode_arm64(code, address).map_err(|_| AnalysisError::InvalidRange)?;
    let (tokens, _) = fields::recover_decoded(&rows, image.symbols, image.strings);
    let base = super::numeric::modifier_input(&text, image.symbols, &names, recipe)?;
    let directories = super::references::database_directories(
        &text,
        image,
        candidates,
        BTreeSet::from([recipe.modifier_reference_database.to_owned()]),
    );
    let target = match directories.get(recipe.modifier_reference_database) {
        Some(Directory::Named(name)) => crate::ReferenceTarget::Registry { name: name.clone() },
        _ => crate::ReferenceTarget::Unresolved,
    };
    let database = format!("{}::_pInstance", recipe.modifier_reference_database);
    let null = "TPdxNullObject<CStaticModifier>::_pInstance";
    let aliases = [
        (database.as_str(), "database"),
        (null, "null_item"),
        (recipe.numeric_modifier_member, "base_member"),
        ("CString::CString(char const*)", "key_string"),
        ("CToken::GetFloat() const", "fixed_value"),
        ("StringToFixedPoint(char const*)", "fixed_conversion"),
        ("_memcmp", "compare_bytes"),
    ];
    let semantic_names = names
        .iter()
        .map(|(&address, name)| {
            (
                address,
                aliases
                    .iter()
                    .find_map(|(symbol, alias)| (*symbol == name).then_some(*alias))
                    .unwrap_or(name)
                    .to_owned(),
            )
        })
        .collect();
    let body = |name: &str| -> Result<Vec<_>, AnalysisError> {
        let starts: BTreeSet<_> = image
            .symbols
            .iter()
            .filter(|symbol| symbol.name == name)
            .map(|symbol| symbol.address)
            .collect();
        if starts.len() != 1 {
            return Ok(vec![]);
        }
        let (address, bytes) = text.function(*starts.first().unwrap())?;
        decode_arm64(bytes, address).map_err(|_| AnalysisError::InvalidRange)
    };
    let reference_input = reference::Input {
        member: body(recipe.modifier_reference_member)?,
        conversion: body("CToken::GetFloat() const")?,
        names: semantic_names,
        key_text_offset: recipe.reader_token_offset + recipe.token_text_offset,
        value_token_offset: recipe.reader_value_token_offset,
        token_text_offset: recipe.token_text_offset,
    };
    let reference = reference::analyze(&reference_input).map(|()| target);
    let serializer_constructors = image
        .symbols
        .iter()
        .filter(|symbol| symbol.name == recipe.modifier_serializer_constructor)
        .filter_map(|symbol| {
            let (address, code) = text.function(symbol.address).ok()?;
            let rows = decode_arm64(code, address).ok()?;
            Some((address, fields::serializer_owner_slot(&rows)?))
        })
        .collect();
    Ok(ModifierBlockInput {
        points,
        functions,
        symbols: image.symbols.to_vec(),
        pointers: image.pointers.clone(),
        data: super::fields::read_only_data(image.bytes)?,
        tokens,
        serializer_constructors,
        key_readers: super::fields::key_readers(image.symbols, &recipe.persistent)?,
        reader_token_offset: recipe.reader_token_offset,
        base_member: recipe.numeric_modifier_member.into(),
        base,
        references: BTreeMap::from([(recipe.modifier_reference_member.into(), reference)]),
    })
}
