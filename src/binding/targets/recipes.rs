//! Recipes: which binding groups, live strategy and static layout make up one build's
//! operations. A recipe is host-neutral data.
#[derive(Debug, Clone, Copy)]
pub(in crate::binding) enum BindingGroupId {
    M45TraditionRegistries,
}

#[derive(Debug, Clone, Copy)]
pub(in crate::binding) enum StrategyId {
    MacSuspendedChildLoaderEntry,
}

pub(in crate::binding) struct Recipe {
    pub groups: &'static [BindingGroupId],
    pub strategy: StrategyId,
    pub discovery: &'static DiscoveryRecipe,
}

pub(super) const M45_OBSERVE: Recipe = Recipe {
    groups: &[BindingGroupId::M45TraditionRegistries],
    strategy: StrategyId::MacSuspendedChildLoaderEntry,
    discovery: &M45_DISCOVERY,
};

/// The literal initialization of the startup scheduling table (SDK-489). It ends before
/// scheduling begins.
pub(in crate::binding) struct DiscoveryRecipe {
    pub start: u64,
    pub end: u64,
    pub offset: u64,
    pub stride: u64,
    pub count: usize,
}
const M45_DISCOVERY: DiscoveryRecipe = DiscoveryRecipe {
    start: 0x1005ea4c8,
    end: 0x1005ed980,
    offset: 96,
    stride: 48,
    count: 198,
};
