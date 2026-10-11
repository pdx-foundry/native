//! Collect shared weight readers from executable pointer slots, with the constructors that their
//! member readers call.
use super::{declarations::Text, grammar::CHILD_FAMILIES, references::Image};
use crate::binding::targets::DeclarationRecipe;
use crate::engine::analysis::{
    declarations::Function, evaluate::ReadOnlyData, weight_blocks::WeightBlockInput,
};
use crate::{AnalysisError, BlockFamily};

pub(in crate::binding) fn read(
    image: &Image<'_>,
    text: &Text<'_>,
    recipe: &DeclarationRecipe,
) -> Result<WeightBlockInput, AnalysisError> {
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
    let constants = super::language::constant_data(image.bytes, image.pointers, image.bound_slots)?;
    let constructors = super::receivers::constructors(
        text,
        image.symbols,
        &constants,
        &roots.iter().collect::<Vec<_>>(),
    )?;
    let tokens = text.token_table(image.symbols, image.strings)?;
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
        families: CHILD_FAMILIES
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
    text.function(address).ok().map(|(_, code)| code)
}
