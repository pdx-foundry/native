//! Recipes: which binding groups, live strategy and static layout make up one build's
//! operations. A recipe is host-neutral data.
use crate::engine::analysis::localization::TextLayout;

#[derive(Debug, Clone, Copy)]
pub(in crate::binding) enum BindingGroupId {
    M45TemplateRegistryLayout,
    M45CategoryFixture,
}

#[derive(Debug, Clone, Copy)]
pub(in crate::binding) enum StrategyId {
    MacSuspendedChildLoaderEntry,
}

pub(in crate::binding) struct Recipe {
    pub groups: &'static [BindingGroupId],
    pub default_registries: &'static [&'static str],
    pub strategy: StrategyId,
    pub discovery: &'static DiscoveryRecipe,
    pub declarations: Option<&'static DeclarationRecipe>,
}

/// Layout facts that the declaration methods need on this exact build.
pub(in crate::binding) struct DeclarationRecipe {
    /// Virtual slots used by the two command families.
    pub create_slot: u64,
    pub trigger_scope_slot: u64,
    pub effect_scope_slot: u64,
    /// Stack offset of the category argument of the modifier definition call.
    pub modifier_category_offset: u64,
    /// Offset of the token in an event target object.
    pub event_target_token_offset: u64,
    /// The engine's string object: its size, and the offset of a short string's length byte.
    pub string_object_size: u64,
    pub short_string_length_offset: u64,
    /// The localization text object and scope-object reference.
    pub game_text: TextLayout,
}

pub(in crate::binding) const M45_DEFAULT_REGISTRIES: &[&str] =
    &["common/traditions", "common/tradition_categories"];

pub(super) const M45_RELEASE: Recipe = Recipe {
    groups: &[
        BindingGroupId::M45TemplateRegistryLayout,
        BindingGroupId::M45CategoryFixture,
    ],
    default_registries: M45_DEFAULT_REGISTRIES,
    strategy: StrategyId::MacSuspendedChildLoaderEntry,
    discovery: &M45_DISCOVERY,
    declarations: Some(&M45_DECLARATIONS),
};

const M45_DECLARATIONS: DeclarationRecipe = DeclarationRecipe {
    create_slot: 0x10,
    trigger_scope_slot: 0x78,
    effect_scope_slot: 0x80,
    modifier_category_offset: 0x4,
    event_target_token_offset: 0x58,
    string_object_size: 0x18,
    short_string_length_offset: 0x17,
    game_text: TextLayout {
        context_offset: 0x8,
        promotion_targets: 0x318,
        promote: 0x498,
        property_targets: 0x618,
        context_count: 0x30,
        scope_reference_type_offset: 0x8,
    },
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
    start: 0x1005eb938,
    end: 0x1005eedf0,
    offset: 96,
    stride: 48,
    count: 198,
};
