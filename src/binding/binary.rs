use object::read::macho::{FatArch, MachOFatFile32, MachOFatFile64};
use object::{Architecture, BinaryFormat, FileKind, Object, ObjectKind};
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
    let kind = FileKind::parse(bytes).map_err(|_| OpenError::MalformedExecutable)?;
    let slice = match kind {
        FileKind::MachOFat32 => {
            let file = MachOFatFile32::parse(bytes).map_err(|_| OpenError::MalformedExecutable)?;
            select_arm64(file.arches(), bytes)?
        }
        FileKind::MachOFat64 => {
            let file = MachOFatFile64::parse(bytes).map_err(|_| OpenError::MalformedExecutable)?;
            select_arm64(file.arches(), bytes)?
        }
        FileKind::MachO64 | FileKind::Pe64 => bytes,
        _ => return Err(OpenError::UnsupportedTarget),
    };
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
