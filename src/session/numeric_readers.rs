//! Cached numeric conversion facts and stops for population reports. Not a consumer API.
use super::Native;
pub use crate::engine::analysis::numeric::{ModifierNumericEntry, NumericFacts, NumericReader};
use crate::{Error, Operation};

/// Analyze all bound numeric shared readers on an opened installation. Recorded answers have
/// no executable method result and return `Error::Unsupported`.
pub fn run(native: &Native) -> Result<NumericFacts, Error> {
    native.method_result(Operation::RegistryFields, || {
        native.numeric_facts(Operation::RegistryFields).cloned()
    })
}
