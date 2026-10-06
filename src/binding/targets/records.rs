use super::{TargetRecord, recipes::M452};

// M452, macOS ARM64: Cygnus v4.5.2 (9776). The executable hash is the universal image from Steam;
// the slice hash is its ARM64 slice.
pub(super) const CATALOGUE: &[TargetRecord] = &[TargetRecord {
    executable: "c621723d9c8e0c1cd153319208d30a9dfbb9e63675be86f9d0ae7debeaa7fe1b",
    slice: "573e0f2511317e83f47a4d11f9c48f31361fe1cbb68900aae99ca945b2e47507",
    architecture: object::Architecture::Aarch64,
    format: object::BinaryFormat::MachO,
    recipe: &M452,
}];
