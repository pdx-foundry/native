use super::StrategyResolution;
use crate::binding::targets::StrategyId;

pub(super) fn resolve(strategy: StrategyId) -> StrategyResolution {
    match strategy {
        StrategyId::MacSuspendedChildLoaderEntry => StrategyResolution {
            unavailable: None,
            package: observation::package(),
            probe: observation::probe_observer,
            prepare: observation::Observer::prepare,
        },
    }
}

pub(in crate::binding) mod lifecycle;
pub(in crate::binding) mod observation;
