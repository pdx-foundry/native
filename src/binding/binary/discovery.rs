use crate::AnalysisError;
use crate::engine::analysis::discovery::StaticInput;

use super::{fixups, inventory};

/// The image inventory and its required fixups.
pub(in crate::binding) fn read(bytes: &[u8]) -> Result<StaticInput, AnalysisError> {
    let inventory = inventory::read(bytes).map_err(|_| AnalysisError::InvalidRange)?;
    let fixups = fixups::read(&inventory).map_err(|_| AnalysisError::InvalidRange)?;

    Ok(StaticInput {
        symbols: inventory.symbols,
        pointers: fixups.pointers,
        bound_slots: fixups.bound,
        strings: inventory.strings,
    })
}
