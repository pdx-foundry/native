use super::{TargetRecord, recipes::M45_RELEASE};

// M45-release, macOS ARM64: Cygnus v4.5.0 (8697), the full 4.5 release. The executable hash is
// the universal image from Steam; the slice hash is its ARM64 slice.
pub(super) const CATALOGUE: &[TargetRecord] = &[TargetRecord {
    executable: "07988b4f1b865623becd7a61af1cae92e111be6515d341754af70f02107822cd",
    slice: "a4cb49ad17a84ef6bf438019a50d3a66362c80731f8359888ddbce47c0d0aab9",
    architecture: object::Architecture::Aarch64,
    format: object::BinaryFormat::MachO,
    recipe: &M45_RELEASE,
}];
