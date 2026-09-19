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
};
