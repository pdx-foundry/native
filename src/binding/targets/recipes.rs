#[derive(Debug, Clone, Copy)]
pub(in crate::binding) enum BindingGroupId {
    Registration,
    #[cfg(test)]
    SyntheticRegistration,
    CategoryReader,
}

#[derive(Debug, Clone, Copy)]
pub(in crate::binding) enum MethodId {
    BoundedRegistrationCategoryReads,
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
    revision: "m45-early-reads/candidate-v1",
    groups: &[BindingGroupId::Registration, BindingGroupId::CategoryReader],
    method: MethodId::BoundedRegistrationCategoryReads,
    strategy: StrategyId::MacSuspendedChildLoaderEntry,
    content: include_str!("m45-observation-content.json"),
};
