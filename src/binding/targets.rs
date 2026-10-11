mod recipes;
mod records;

use super::binary::ImageIdentity;
use crate::OpenError;

pub(super) use recipes::InlineFixtureRecipe;
pub(super) use recipes::{
    AnchorSlot, BindingGroupId, DeclarationRecipe, PersistentRecipe, Recipe, StrategyId,
};

pub(super) struct TargetRecord {
    pub executable: &'static str,
    pub slice: &'static str,
    pub architecture: object::Architecture,
    pub format: object::BinaryFormat,
    pub recipe: &'static Recipe,
}

pub(super) fn lookup(image: &ImageIdentity) -> Result<&'static Recipe, OpenError> {
    lookup_in(image, records::CATALOGUE)
}

fn lookup_in(
    image: &ImageIdentity,
    records: &[TargetRecord],
) -> Result<&'static Recipe, OpenError> {
    let mut matches = records.iter().filter(|record| {
        record.executable == image.executable
            && record.slice == image.slice
            && record.architecture == image.architecture
            && record.format == image.format
    });
    let record = matches.next().ok_or(OpenError::UnknownTarget)?;
    if matches.next().is_some() {
        return Err(OpenError::Ambiguous);
    }
    Ok(record.recipe)
}

#[cfg(test)]
mod tests;

/// The catalogued recipe, for tests of an authored image.
#[cfg(test)]
pub(super) fn test_recipe() -> &'static Recipe {
    records::CATALOGUE[0].recipe
}

/// The catalogued declarations, for tests of an authored image.
#[cfg(test)]
pub(super) fn test_declarations() -> &'static DeclarationRecipe {
    test_recipe().declarations
}
