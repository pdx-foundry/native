mod arm64;

use crate::protocol::observation::Machine;

pub(super) fn resolve(architecture: object::Architecture) -> Result<Machine, crate::OpenError> {
    match architecture {
        object::Architecture::Aarch64 => Ok(arm64::loader_entry()),
        _ => Err(crate::OpenError::UnsupportedTarget),
    }
}
