//! Collect triggered modifier clause readers from executable pointer slots, with their member and
//! delegate bodies and the constructors of each reader's class.
use super::{declarations::Text, references::Image};
use crate::binding::targets::DeclarationRecipe;
use crate::engine::analysis::{
    declarations::Function, decode::decode_arm64, fields,
    modifier_blocks::triggered::TriggeredInput, stop::Unresolved,
};
use crate::{AnalysisError, BlockFamily};
use std::collections::{BTreeMap, BTreeSet};

pub(in crate::binding) fn read(
    image: &Image<'_>,
    bound_slots: &BTreeSet<u64>,
    recipe: &DeclarationRecipe,
) -> Result<TriggeredInput, AnalysisError> {
    let text = Text::read(image.bytes, image.symbols)?;
    let names =
        super::references::names(image.symbols, image.pointers, image.imports, image.strings);
    let points = super::modifier_blocks::family_points(
        image,
        &names,
        recipe,
        BlockFamily::TriggeredModifier,
    );
    let mut functions = BTreeMap::new();

    for name in points
        .values()
        .flat_map(|reader| std::iter::once(&reader.member).chain(&reader.delegate))
    {
        let Ok(address) = super::declarations::unique(image.symbols, name) else {
            continue;
        };
        let (address, code) = text.function(address)?;
        functions.insert(
            address,
            Function {
                address,
                code: code.to_vec(),
            },
        );
    }

    let data = super::language::constant_data(image.bytes, image.pointers, bound_slots)?;
    let classes = points
        .keys()
        .map(|&point| {
            let class = super::families::point_class(image.symbols, &data, point)
                .ok_or_else(|| Unresolved::new("triggered-class"))
                .and_then(|class| {
                    super::receivers::persistent(
                        image.bytes,
                        image.symbols,
                        image.pointers,
                        bound_slots,
                        &class,
                        &recipe.persistent,
                    )
                    .map_err(|_| Unresolved::new("triggered-class-constructors"))
                });

            (point, class)
        })
        .collect();
    let (address, code) = text.function(super::declarations::unique(
        image.symbols,
        "GetTokenArray()",
    )?)?;
    let rows = decode_arm64(code, address).map_err(|_| AnalysisError::InvalidRange)?;
    let (tokens, _) = fields::recover_decoded(&rows, image.symbols, image.strings);

    Ok(TriggeredInput {
        points,
        classes,
        functions,
        symbols: image.symbols.to_vec(),
        pointers: image.pointers.clone(),
        data: super::fields::read_only_data(image.bytes)?,
        tokens,
        key_readers: super::fields::key_readers(image.symbols, &recipe.persistent)?,
        reader_token_offset: recipe.reader_token_offset,
    })
}
