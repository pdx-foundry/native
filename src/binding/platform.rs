use super::targets::StrategyId;
use crate::UnavailableReason;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod macos;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use macos as host;

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
mod unavailable;
#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
use unavailable as host;

pub(super) struct StrategyResolution {
    pub revision: &'static str,
    pub unavailable: UnavailableReason,
}

pub(super) fn resolve(strategy: StrategyId) -> StrategyResolution {
    let revision = match strategy {
        StrategyId::MacSuspendedChildLoaderEntry => {
            "mac-suspended-child-loader-entry/unimplemented-v1"
        }
    };
    StrategyResolution {
        revision,
        unavailable: host::resolve(strategy),
    }
}
