//! Recipes: which binding groups, live strategy and static layout make up one build's
//! operations. A recipe is host-neutral data.
use crate::binding::groups::M45_TEMPLATE_LAYOUT;
use crate::engine::analysis::callbacks::{CallbackLayout, RuleArray};
use crate::engine::analysis::declarations::ParserSlots;
use crate::engine::analysis::localization::TextLayout;

// Each group is one build's bindings, so its name starts with the build.
#[allow(clippy::enum_variant_names)]
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
    pub declarations: Option<&'static DeclarationRecipe>,
}

/// Virtual reader slots and shared family methods on the selected build.
pub(in crate::binding) struct PersistentRecipe {
    pub read_slot: u64,
    pub member_slot: u64,
    pub families: &'static [(&'static str, crate::BlockFamily)],
}

/// Layout facts that the declaration methods need on this exact build.
pub(in crate::binding) struct DeclarationRecipe {
    /// Virtual slots used by the two command families.
    pub create_slot: u64,
    pub trigger_scope_slot: u64,
    pub effect_scope_slot: u64,
    pub trigger_parser: ParserSlots,
    pub effect_parser: ParserSlots,
    pub persistent: PersistentRecipe,
    pub command_children: crate::engine::analysis::grammar::ChildLayout,
    pub numeric_key_reader: &'static str,
    pub reader_token_offset: u64,
    pub child_families: &'static [(&'static str, crate::BlockFamily)],
    /// Stack offset of the category argument of the modifier definition call.
    pub modifier_category_offset: u64,
    /// Stack offset of the category argument of the call that registers a generated modifier.
    pub dynamic_modifier_category_offset: u64,
    /// Offset of the token in an event target object.
    pub event_target_token_offset: u64,
    /// The engine's string object: its size, and the offset of a short string's length byte.
    pub string_object_size: u64,
    pub short_string_length_offset: u64,
    /// The localization text object and scope-object reference.
    pub game_text: TextLayout,
    /// The scope object and the rule set that the callback method reads.
    pub callbacks: CallbackLayout,
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
    declarations: Some(&M45_DECLARATIONS),
};

const M45_DECLARATIONS: DeclarationRecipe = DeclarationRecipe {
    create_slot: 0x10,
    trigger_scope_slot: 0x78,
    effect_scope_slot: 0x80,
    trigger_parser: ParserSlots {
        read: 0x30,
        member: 0x38,
    },
    effect_parser: ParserSlots {
        read: 0x10,
        member: 0x18,
    },
    persistent: PersistentRecipe {
        read_slot: 0x20,
        member_slot: 0x28,
        families: &[(
            "CPdxModifier<ModifierType, ModifierCategory, CModifier, CDefaultPdxModifierValueReader>::Read(CReader&)",
            crate::BlockFamily::Modifier,
        )],
    },
    command_children: crate::engine::analysis::grammar::ChildLayout {
        data: 0x10,
        count: 0x1c,
        token: 0x20,
    },
    numeric_key_reader: "CToken::ReadValue(int&) const",
    reader_token_offset: 0x38,
    child_families: &[
        (
            "CTriggerCollectionBase::ReadMember(CReader&, int, EScopeType)",
            crate::BlockFamily::Trigger,
        ),
        (
            "CEffect::ReadMember(CReader&, int, EScopeType)",
            crate::BlockFamily::Effect,
        ),
    ],
    modifier_category_offset: 0x4,
    dynamic_modifier_category_offset: 0x0,
    event_target_token_offset: 0x58,
    string_object_size: 0x18,
    short_string_length_offset: M45_TEMPLATE_LAYOUT.string_tag_offset(),
    game_text: TextLayout {
        context_offset: 0x8,
        promotion_targets: 0x318,
        promote: 0x498,
        property_targets: 0x618,
        context_count: 0x30,
        scope_reference_type_offset: 0x8,
    },
    callbacks: CallbackLayout {
        scope_type_offset: 0x8,
        scope_root_offset: 0x30,
        scope_from_offset: 0x38,
        scope_prev_offset: 0x40,
        scripted_rules: RuleArray {
            base: 0,
            stride: 0xc0,
        },
        weighted_rules: RuleArray {
            base: 0x9cc0,
            stride: 0x40,
        },
        declaration_token_offset: 0,
    },
};
