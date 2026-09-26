use super::super::targets::DeclarationRecipe;
use crate::AnalysisError;
use crate::engine::analysis::{
    declarations::{DeclarationInput, DeclarationResult},
    decode::decode_arm64,
    discovery::Symbol,
    fields,
    grammar::GrammarInput,
};
use std::collections::{BTreeMap, BTreeSet};

pub(in crate::binding) fn read(
    bytes: &[u8],
    symbols: &[Symbol],
    strings: &BTreeMap<u64, String>,
    bound_slots: &BTreeSet<u64>,
    mut declarations: DeclarationInput,
    inventory: &DeclarationResult,
    recipe: &DeclarationRecipe,
) -> Result<GrammarInput, AnalysisError> {
    let text = super::declarations::Text::read(bytes, symbols)?;
    let mut roots: Vec<_> = inventory
        .sites
        .iter()
        .filter_map(|(_, site)| {
            let crate::engine::analysis::declarations::Site::Declared { factory, .. } = site else {
                return None;
            };
            let entry = declarations
                .pointers
                .get(&(factory + declarations.slots.create))?;
            declarations.functions.get(entry)
        })
        .collect();
    roots.extend(
        symbols
            .iter()
            .filter(|symbol| {
                symbol
                    .name
                    .ends_with("::ReadMember(CReader&, int, EScopeType)")
            })
            .filter_map(|symbol| declarations.functions.get(&symbol.address)),
    );
    declarations.constructors = super::receivers::constructors(
        bytes,
        symbols,
        &declarations.pointers,
        bound_slots,
        &roots,
    )?;

    let token_start = super::declarations::unique(symbols, "GetTokenArray()")?;
    let (_, token_code) = text.function(token_start)?;
    let rows = decode_arm64(token_code, token_start).map_err(|_| AnalysisError::InvalidRange)?;
    let (tokens, _) = fields::recover_decoded(&rows, symbols, strings);
    let families = recipe
        .child_families
        .iter()
        .map(|&(name, family)| (name.into(), family))
        .collect();
    let numeric_decoder = super::declarations::unique(symbols, recipe.numeric_key_reader)?;
    Ok(GrammarInput {
        child_layout: recipe.command_children,
        numeric_decoder,
        reader_token_offset: recipe.reader_token_offset,
        declarations,
        symbols: symbols.to_vec(),
        data: super::fields::read_only_data(bytes)?,
        tokens,
        families,
    })
}
