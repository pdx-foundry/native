use super::StrategyResolution;
use crate::{UnavailableReason, binding::targets::StrategyId};

pub(super) fn resolve(strategy: StrategyId) -> StrategyResolution {
    match strategy {
        StrategyId::MacSuspendedChildLoaderEntry => StrategyResolution {
            unavailable: Some(UnavailableReason::HostUnavailable),
            package: Default::default(),
            probe: observation::probe_observer,
            prepare: observation::Observer::prepare,
        },
    }
}

pub(in crate::binding) mod lifecycle;
pub(in crate::binding) mod observation;
