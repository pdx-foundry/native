mod arm64;

pub(super) fn resolve(
    architecture: object::Architecture,
) -> Result<super::Machine, crate::OpenError> {
    match architecture {
        object::Architecture::Aarch64 => Ok(arm64::loader_entry()),
        _ => Err(crate::OpenError::UnsupportedTarget),
    }
}

/// Whether the static methods can decode this architecture.
pub(super) fn static_methods(architecture: object::Architecture) -> Result<(), crate::OpenError> {
    match architecture {
        object::Architecture::Aarch64 => Ok(()),
        _ => Err(crate::OpenError::UnsupportedTarget),
    }
}
