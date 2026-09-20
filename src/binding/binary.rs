use object::read::macho::{FatArch, MachOFatFile32, MachOFatFile64};
use object::{
    Architecture, BinaryFormat, FileKind, Object, ObjectKind, ObjectSection, SectionKind,
};
use sha2::{Digest, Sha256};

use crate::OpenError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ImageIdentity {
    pub executable: String,
    pub slice: String,
    pub architecture: Architecture,
    pub format: BinaryFormat,
}

pub(super) fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(super) fn identify(bytes: &[u8]) -> Result<ImageIdentity, OpenError> {
    let slice = selected_slice(bytes)?;
    let file = object::File::parse(slice).map_err(|_| OpenError::MalformedExecutable)?;
    if file.kind() != ObjectKind::Executable {
        return Err(OpenError::UnsupportedTarget);
    }
    if !matches!(
        (file.format(), file.architecture()),
        (BinaryFormat::MachO, Architecture::Aarch64) | (BinaryFormat::Pe, Architecture::X86_64)
    ) {
        return Err(OpenError::UnsupportedTarget);
    }
    Ok(ImageIdentity {
        executable: hash(bytes),
        slice: hash(slice),
        architecture: file.architecture(),
        format: file.format(),
    })
}

pub(super) fn selected_slice(bytes: &[u8]) -> Result<&[u8], OpenError> {
    let kind = FileKind::parse(bytes).map_err(|_| OpenError::MalformedExecutable)?;
    match kind {
        FileKind::MachOFat32 => {
            let file = MachOFatFile32::parse(bytes).map_err(|_| OpenError::MalformedExecutable)?;
            select_arm64(file.arches(), bytes)
        }
        FileKind::MachOFat64 => {
            let file = MachOFatFile64::parse(bytes).map_err(|_| OpenError::MalformedExecutable)?;
            select_arm64(file.arches(), bytes)
        }
        FileKind::MachO64 | FileKind::Pe64 => Ok(bytes),
        _ => Err(OpenError::UnsupportedTarget),
    }
}

pub(super) fn code_range(
    bytes: &[u8],
    address: u64,
    length: u64,
) -> Result<Vec<u8>, crate::AnalysisError> {
    use crate::AnalysisError;
    if length == 0 || length > 4096 || !length.is_multiple_of(4) || !address.is_multiple_of(4) {
        return Err(AnalysisError::InvalidRange);
    }
    let end = address
        .checked_add(length)
        .ok_or(AnalysisError::InvalidRange)?;
    let slice = selected_slice(bytes).map_err(|_| AnalysisError::InvalidRange)?;
    let file = object::File::parse(slice).map_err(|_| AnalysisError::InvalidRange)?;
    let mut matched = None;
    for section in file.sections() {
        let section_end = section
            .address()
            .checked_add(section.size())
            .ok_or(AnalysisError::InvalidRange)?;
        if section.kind() != SectionKind::Text || address < section.address() || end > section_end {
            continue;
        }
        if matched.is_some() {
            return Err(AnalysisError::InvalidRange);
        }
        matched = Some(
            section
                .data_range(address, length)
                .map_err(|_| AnalysisError::InvalidRange)?
                .filter(|data| data.len() as u64 == length)
                .ok_or(AnalysisError::InvalidRange)?
                .to_vec(),
        );
    }
    matched.ok_or(AnalysisError::InvalidRange)
}

fn select_arm64<'a, A: FatArch>(arches: &[A], bytes: &'a [u8]) -> Result<&'a [u8], OpenError> {
    let mut selected = None;
    for arch in arches {
        // Validate every declared range, even if it is not the selected architecture.
        let slice = arch
            .data(bytes)
            .map_err(|_| OpenError::MalformedExecutable)?;
        if arch.architecture() != Architecture::Aarch64 {
            continue;
        }
        if selected.is_some() {
            return Err(OpenError::Ambiguous);
        }
        let file = object::File::parse(slice).map_err(|_| OpenError::MalformedExecutable)?;
        if file.format() != BinaryFormat::MachO || file.architecture() != arch.architecture() {
            return Err(OpenError::MalformedExecutable);
        }
        selected = Some(slice);
    }
    selected.ok_or(OpenError::UnsupportedTarget)
}

pub(in crate::binding) mod discovery;

pub(super) mod fields;
