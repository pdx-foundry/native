use crate::UnavailableReason;
use crate::binding::targets::StrategyId;

pub(super) fn resolve(strategy: StrategyId) -> UnavailableReason {
    match strategy {
        StrategyId::MacSuspendedChildLoaderEntry => UnavailableReason::HostUnavailable,
    }
}

#[cfg(feature = "maintainer-tools")]
pub(in crate::binding) mod lifecycle;
