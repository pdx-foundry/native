use super::{TargetRecord, recipes::M451_HOTFIX};

// M451-hotfix, macOS ARM64: Cygnus v4.5.1. The executable hash is the universal image from Steam;
// the slice hash is its ARM64 slice.
pub(super) const CATALOGUE: &[TargetRecord] = &[TargetRecord {
    executable: "29fa877366040a528098da39ec7e70b7baac76782a2a6bd161616d691f86fa38",
    slice: "2aeb9e15241bb114fd9f35a2dd09b454a5df6a0b1948b229d9eb83123e665c21",
    architecture: object::Architecture::Aarch64,
    format: object::BinaryFormat::MachO,
    recipe: &M451_HOTFIX,
}];
