mod arm64;

pub(super) fn resolve(
    architecture: object::Architecture,
) -> Result<super::Machine, crate::OpenError> {
    match architecture {
        object::Architecture::Aarch64 => Ok(arm64::read_entries()),
        _ => Err(crate::OpenError::UnsupportedTarget),
    }
}
