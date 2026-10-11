//! Collect shared weight readers from executable pointer slots, with the constructors that their
//! member readers call.
use super::{
    declarations::{Text, read_only_data},
    grammar::CHILD_FAMILIES,
    references::Image,
};
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
        data: pointer_view(image)?,
        reader_token_offset: recipe.reader_token_offset,
        value_token_offset: recipe.reader_value_token_offset,
        token_text_offset: recipe.token_text_offset,
    })
}

/// The read-only sections with the target of every rebased pointer, writable `__DATA` slots
/// included: a slot outside the constant sections reads as the target that the loader gave it.
/// [`super::language::constant_data`] leaves such a slot out, and a bound slot is unknown there.
fn pointer_view(image: &Image<'_>) -> Result<ReadOnlyData, AnalysisError> {
    Ok(read_only_data(image.bytes)?.with_words(image.pointers))
}

/// The code of the function that starts at `address`.
pub(in crate::binding) fn body<'a>(text: &Text<'a>, address: u64) -> Option<&'a [u8]> {
    text.function(address).ok().map(|(_, code)| code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::analysis_support::macho_with_fixups;

    #[test]
    fn the_weight_view_reads_a_writable_slot_that_the_constant_view_leaves_out() {
        const STRING: u64 = 0x1_0000_2008;
        // The rebased pointer in `__DATA,__data`, which targets `_helper`.
        const WRITABLE_SLOT: u64 = 0x1_0000_4000;
        const HELPER: u64 = 0x1_0000_1020;

        let bytes = macho_with_fixups(6);
        let facts = super::super::discovery::read(&bytes).unwrap();
        let image = Image {
            bytes: &bytes,
            symbols: &facts.symbols,
            strings: &facts.strings,
            pointers: &facts.pointers,
            imports: &facts.imports,
            bound_slots: &facts.bound_slots,
        };

        let weight = pointer_view(&image).unwrap();
        let constant =
            super::super::language::constant_data(&bytes, &facts.pointers, &facts.bound_slots)
                .unwrap();

        assert_eq!(weight.read(WRITABLE_SLOT, 8), Some(HELPER));
        assert_eq!(constant.read(WRITABLE_SLOT, 8), None);
        assert_eq!(weight.string(STRING), Some("entity_offset".into()));
    }
}
