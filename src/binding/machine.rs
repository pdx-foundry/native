mod arm64;

pub(super) fn resolve(
    architecture: object::Architecture,
) -> Result<super::Machine, crate::OpenError> {
    match architecture {
        object::Architecture::Aarch64 => Ok(arm64::read_entries()),
        _ => Err(crate::OpenError::UnsupportedTarget),
    }
}

pub(super) fn decoder(
    architecture: object::Architecture,
) -> Result<super::Decoder, crate::OpenError> {
    match architecture {
        object::Architecture::Aarch64 => Ok(crate::engine::analysis::decode::decode_arm64),
        _ => Err(crate::OpenError::UnsupportedTarget),
    }
}
