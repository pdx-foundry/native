pub(in crate::binding) mod recipes;
mod records;

use super::binary::ImageIdentity;
use crate::OpenError;

pub(super) use recipes::M45_DEFAULT_REGISTRIES;
pub(super) use recipes::{BindingGroupId, DeclarationRecipe, Recipe, StrategyId};

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

#[cfg(test)]
pub(super) fn test_identity() -> ImageIdentity {
    let record = &records::CATALOGUE[0];
    ImageIdentity {
        executable: record.executable.into(),
        slice: record.slice.into(),
        architecture: record.architecture,
        format: record.format,
    }
}
