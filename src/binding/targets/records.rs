use super::{
    TargetRecord,
    recipes::{M45_RELEASE, M451_HOTFIX},
};

// M45-release, macOS ARM64: Cygnus v4.5.0 (8697), the full 4.5 release. The executable hash is
// the universal image from Steam; the slice hash is its ARM64 slice.
pub(super) const CATALOGUE: &[TargetRecord] = &[
    TargetRecord {
        executable: "07988b4f1b865623becd7a61af1cae92e111be6515d341754af70f02107822cd",
        slice: "a4cb49ad17a84ef6bf438019a50d3a66362c80731f8359888ddbce47c0d0aab9",
        architecture: object::Architecture::Aarch64,
        format: object::BinaryFormat::MachO,
        recipe: &M45_RELEASE,
    },
    TargetRecord {
        executable: "29fa877366040a528098da39ec7e70b7baac76782a2a6bd161616d691f86fa38",
        slice: "2aeb9e15241bb114fd9f35a2dd09b454a5df6a0b1948b229d9eb83123e665c21",
        architecture: object::Architecture::Aarch64,
        format: object::BinaryFormat::MachO,
        recipe: &M451_HOTFIX,
    },
];
