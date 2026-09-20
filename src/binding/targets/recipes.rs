#[derive(Debug, Clone, Copy)]
pub(in crate::binding) enum BindingGroupId {
    Registration,
    #[cfg(test)]
    SyntheticRegistration,
    CategoryReader,
    Registries,
}

#[derive(Debug, Clone, Copy)]
pub(in crate::binding) enum MethodId {
    TraditionRegistryKeys,
}

#[derive(Debug, Clone, Copy)]
pub(in crate::binding) enum StrategyId {
    MacSuspendedChildLoaderEntry,
}

pub(in crate::binding) struct Recipe {
    pub revision: &'static str,
    pub groups: &'static [BindingGroupId],
    pub method: MethodId,
    pub strategy: StrategyId,
    pub content: &'static str,
    pub discovery: &'static DiscoveryRecipe,
}

pub(super) const M45_EARLY_READS: Recipe = Recipe {
    revision: "m45-tradition-registries/candidate-v1",
    groups: &[
        BindingGroupId::Registration,
        BindingGroupId::CategoryReader,
        BindingGroupId::Registries,
    ],
    method: MethodId::TraditionRegistryKeys,
    strategy: StrategyId::MacSuspendedChildLoaderEntry,
    content: include_str!("m45-observation-content.json"),
    discovery: &M45_DISCOVERY,
};

/// SDK-489 literal scheduling initialization, ending before scheduling begins.
pub(in crate::binding) struct DiscoveryRecipe {
    pub revision: &'static str,
    pub start: u64,
    pub end: u64,
    pub offset: u64,
    pub stride: u64,
    pub count: usize,
}
const M45_DISCOVERY: DiscoveryRecipe = DiscoveryRecipe {
    revision: "m45-registry-discovery/v1",
    start: 0x1005ea4c8,
    end: 0x1005ed980,
    offset: 96,
    stride: 48,
    count: 198,
};
