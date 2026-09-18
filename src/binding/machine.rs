mod arm64;

pub(super) fn resolve(
    architecture: object::Architecture,
) -> Result<&'static str, crate::OpenError> {
    match architecture {
        object::Architecture::Aarch64 => Ok(arm64::READ_ENTRY_REVISION),
        _ => Err(crate::OpenError::UnsupportedTarget),
    }
}
