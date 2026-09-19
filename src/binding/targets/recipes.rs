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
    pub analysis: &'static DecodeRecipe,
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
    analysis: &M45_DECODE,
};

/// Exact function control from the verified SDK-482 planet-getter disassembly.
pub(in crate::binding) struct DecodeRecipe {
    pub revision: &'static str,
    pub address: u64,
    pub length: u64,
    pub code_sha256: &'static str,
}

pub(super) const M45_DECODE: DecodeRecipe = DecodeRecipe {
    revision: "m45-planet-getter-decode/v1",
    address: 0x101156518,
    length: 44,
    code_sha256: "eadf7f0bacadae414f7b05e9a426ad586c370273c7f3b0b56cd69b1dbcc2b97f",
};
