use crate::AnalysisError;
use crate::engine::analysis::{InputError, discovery::StaticInput};

use super::{fixups, inventory};

/// The image inventory and its required fixups.
pub(in crate::binding) fn read(bytes: &[u8]) -> Result<StaticInput, AnalysisError> {
    let inventory = inventory::read(bytes).map_err(|_| AnalysisError::InvalidRange)?;
    let fixups = fixups::read(&inventory)
        .map_err(|diagnostic| AnalysisError::Input(InputError(diagnostic.to_string())))?;

    Ok(StaticInput {
        symbols: inventory.symbols,
        pointers: fixups.pointers,
        bound_slots: fixups.bound,
        imports: fixups.bindings,
        strings: inventory.strings,
    })
}
