//! Collect shared weight readers from executable pointer slots, with the constructors that their
//! member readers call.
use super::{declarations::Text, references::Image};
use crate::binding::targets::DeclarationRecipe;
use crate::engine::analysis::{
    declarations::Function, decode::decode_arm64, evaluate::ReadOnlyData, fields,
    weight_blocks::WeightBlockInput,
};
use crate::{AnalysisError, BlockFamily};

pub(in crate::binding) fn read(
    image: &Image<'_>,
    bound_slots: &std::collections::BTreeSet<u64>,
    recipe: &DeclarationRecipe,
) -> Result<WeightBlockInput, AnalysisError> {
    let text = Text::read(image.bytes, image.symbols)?;
    let names =
        super::references::names(image.symbols, image.pointers, image.imports, image.strings);
    let points = super::modifier_blocks::family_points(image, &names, recipe, BlockFamily::Weight);
    let mut roots = Vec::new();
    for reader in points.values() {
        for symbol in image
            .symbols
            .iter()
            .filter(|symbol| symbol.name == reader.member)
        {
            let (address, code) = text.function(symbol.address)?;
            roots.push(Function {
                address,
                code: code.to_vec(),
            });
        }
    }
    let constructors = super::receivers::constructors(
        image.bytes,
        image.symbols,
        image.pointers,
        bound_slots,
        &roots.iter().collect::<Vec<_>>(),
    )?;
    let (address, code) = text.function(super::declarations::unique(
        image.symbols,
        "GetTokenArray()",
    )?)?;
    let rows = decode_arm64(code, address).map_err(|_| AnalysisError::InvalidRange)?;
    let (tokens, _) = fields::recover_decoded(&rows, image.symbols, image.strings);
    let sections = super::fields::read_only_data(image.bytes)?
        .into_iter()
        .map(|section| (section.address, section.bytes))
        .collect();
    Ok(WeightBlockInput {
        points,
        constructors,
        operator_new: super::declarations::addresses(image.symbols, "operator new(unsigned long)"),
        symbols: image.symbols.to_vec(),
        tokens,
        families: recipe
            .child_families
            .iter()
            .map(|&(name, family)| (name.into(), family))
            .collect(),
        pointers: image.pointers.clone(),
        data: ReadOnlyData::new(sections).with_words(image.pointers),
        reader_token_offset: recipe.reader_token_offset,
        value_token_offset: recipe.reader_value_token_offset,
        token_text_offset: recipe.token_text_offset,
    })
}

/// The code of the function that starts at `address`.
pub(in crate::binding) fn body<'a>(text: &Text<'a>, address: u64) -> Option<&'a [u8]> {
    text.starts
        .contains(&address)
        .then(|| text.function(address).ok().map(|(_, code)| code))
        .flatten()
}
