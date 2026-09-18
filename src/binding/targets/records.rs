use super::{TargetRecord, recipes::M45_EARLY_READS};

// Verified typed-extraction bundle 70fce0ce8dae5dbb…; reference-observation-prototype/
// evidence/initial-pin.json and early-observation-prototype/runs/20260917-002215-none-d8a910.
// This is a candidate recipe, not an accepted production qualification.
pub(super) const CATALOGUE: &[TargetRecord] = &[TargetRecord {
    executable: "3d4c8a7046d87175ce7e3b513b1a2ce589050d654d332744518a49d13ac82216",
    slice: "1e0c9aec45650272fcaecba2eb47f8dce8f17bc08ef2b992be18c99ae098c623",
    architecture: object::Architecture::Aarch64,
    format: object::BinaryFormat::MachO,
    recipe: &M45_EARLY_READS,
}];
