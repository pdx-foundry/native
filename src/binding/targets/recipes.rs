//! Recipes: which binding groups, live strategy and static layout make up one build's
//! operations. A recipe is host-neutral data.
use crate::binding::groups::M45_TEMPLATE_LAYOUT;
use crate::engine::analysis::callbacks::{CallbackLayout, RuleArray};
use crate::engine::analysis::declarations::ParserSlots;
use crate::engine::analysis::dynamic_names::CommandSlots;
use crate::engine::analysis::localization::TextLayout;

// Each group is one build's bindings, so its name starts with the build.
#[derive(Debug, Clone, Copy)]
pub(in crate::binding) enum BindingGroupId {
    M45TemplateRegistryLayout,
    M451CategoryFixture,
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
    pub script_checks: Option<fn() -> crate::protocol::script_check::ScriptCheckBinding>,
}

/// Virtual reader slots and shared family methods on the selected build.
pub(in crate::binding) struct PersistentRecipe {
    pub string_array: [i64; 3],
    pub compound_sizes: [i64; 3],
    pub value_token: u64,
    pub token_text: u64,
    pub read_slot: u64,
    pub member_slot: u64,
    /// Offset of a modifier container's 32-bit category mask from the container's start.
    pub container_mask_offset: u64,
    pub families: &'static [FamilyAnchor],
}

/// A vtable slot whose target names the block family of every address point that holds it.
pub(in crate::binding) struct FamilyAnchor {
    pub symbol: &'static str,
    pub slot: AnchorSlot,
    pub family: crate::BlockFamily,
    /// A further virtual slot whose target tells apart readers that share read and member.
    pub delegate_slot: Option<u64>,
}

/// The slot whose target a family anchor names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::binding) enum AnchorSlot {
    Read,
    /// For readers whose read slot holds the generic `CPersistent::Read`.
    Member,
}

impl PersistentRecipe {
    /// Offset of an anchor's slot from its vtable address point.
    pub fn anchor_offset(&self, slot: AnchorSlot) -> u64 {
        match slot {
            AnchorSlot::Read => self.read_slot,
            AnchorSlot::Member => self.member_slot,
        }
    }
}

/// Layout facts that the declaration methods need on this exact build.
pub(in crate::binding) struct DeclarationRecipe {
    /// Concrete modifier parser and numeric-entry insertion functions.
    pub numeric_modifier_member: &'static str,
    pub modifier_serializer_constructor: &'static str,
    pub modifier_reference_database: &'static str,
    pub modifier_reference_member: &'static str,
    pub numeric_modifier_insert: &'static str,
    /// Numeric shared-reader signatures to bind; conversions are proved from their code.
    pub numeric_types: &'static [&'static str],
    pub inline_fixtures: &'static [InlineFixtureRecipe],
    /// Virtual slots used by the two command families.
    pub create_slot: u64,
    /// Local object pointer of a scope reference.
    pub scope_object_offset: u64,
    pub trigger_scope_slot: u64,
    pub effect_scope_slot: u64,
    pub trigger_parser: ParserSlots,
    pub effect_parser: ParserSlots,
    /// The assign-reader slot, and the evaluate or execute slot, that the dynamic-name method
    /// reads.
    pub trigger_names: CommandSlots,
    pub effect_names: CommandSlots,
    pub persistent: PersistentRecipe,
    pub command_children: crate::engine::analysis::grammar::ChildLayout,
    pub numeric_key_reader: &'static str,
    pub reader_token_offset: u64,
    /// The assigned value token within a reader.
    pub reader_value_token_offset: u64,
    /// Text object within an assigned token.
    pub token_text_offset: u64,
    /// Full event-target object, including its token and link fields.
    pub event_target_size: u64,
    /// Post-validation and developer-only target getter slots, from the address point.
    pub effect_validation_slot: u64,
    pub trigger_validation_slot: u64,
    pub effect_target_getter_slot: u64,
    pub trigger_target_getter_slot: u64,
    /// Token names used for the Boolean assignment probes, true then false.
    pub boolean_tokens: [&'static str; 2],
    pub child_families: &'static [(&'static str, crate::BlockFamily)],
    /// Stack offset of the category argument of the modifier definition call.
    pub modifier_category_offset: u64,
    /// Stack offset of the category argument of the call that registers a generated modifier.
    pub dynamic_modifier_category_offset: u64,
    /// Where a modifier node keeps its category mask and how its constructor receives it.
    pub modifier_nodes: crate::engine::analysis::modifier_nodes::ModifierNodeLayout,
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

const M45_EVENT_TARGET_SIZE: u64 = 0x190;
pub(in crate::binding) const M45_DEFAULT_REGISTRIES: &[&str] =
    &["common/traditions", "common/tradition_categories"];

pub(super) const M451_HOTFIX: Recipe = Recipe {
    groups: &[
        BindingGroupId::M45TemplateRegistryLayout,
        BindingGroupId::M451CategoryFixture,
    ],
    default_registries: M45_DEFAULT_REGISTRIES,
    strategy: StrategyId::MacSuspendedChildLoaderEntry,
    declarations: Some(&M45_DECLARATIONS),
    script_checks: Some(m451_script_checks),
};

/// Console readers and logger verified on the exact 4.5.1 ARM64 slice.
fn m451_script_checks() -> crate::protocol::script_check::ScriptCheckBinding {
    use crate::protocol::script_check::{
        CallBinding, CommandBinding, DatabaseBinding, ObjectWrite, ScriptCheckBinding,
    };
    let call = |address, widths: &[u8]| CallBinding {
        address,
        widths: widths.to_vec(),
    };
    let trigger_database = DatabaseBinding {
        instance: 0x1032e9758,
        post_init: call(0x100d052d0, &[64]),
        post_validate: call(0x100d05268, &[64]),
    };
    ScriptCheckBinding {
        string_constructor: call(0x102521fec, &[64, 64]),
        string_assign: call(0x100229a00, &[64, 64]),
        blob_constructor: call(0x1024f26d8, &[64]),
        blob_append: call(0x1024f2b74, &[64, 64]),
        file_constructor: call(0x10250b5b4, &[64, 64, 32, 32, 8]),
        lexer_constructor: call(0x1025adfe8, &[64, 64, 8]),
        reader_constructor: call(0x1025b1e8c, &[64, 64]),
        string_size: 0x40,
        blob_size: 0x80,
        file_size: 0x400,
        lexer_size: 0x400,
        reader_size: 0x800,
        file_arguments: [1, 0, 0],
        lexer_argument: 0,
        file_name_offset: 0x20,
        string_tag_offset: 23,
        logger_entry: 0x102504718,
        logger_text_register: "x4".into(),
        logger_level_register: "w1".into(),
        trigger: CommandBinding {
            size: 0x200,
            constructor: call(0x100d0613c, &[64]),
            read: call(0x100d066d0, &[64, 64, 64]),
            writes: vec![
                ObjectWrite {
                    offset: 0,
                    width: 8,
                    value: 0x103095ee8,
                    relocate: true,
                },
                ObjectWrite {
                    offset: 0x68,
                    width: 8,
                    value: 0x103000458,
                    relocate: true,
                },
                ObjectWrite {
                    offset: 0x60,
                    width: 1,
                    value: 1,
                    relocate: false,
                },
            ],
            children_offset: 0x7c,
            children_array_offset: 0x70,
            validation: vec![trigger_database.clone()],
        },
        effect: CommandBinding {
            size: 0x200,
            constructor: call(0x10045680c, &[64]),
            read: call(0x100456d30, &[64, 64, 64]),
            writes: vec![ObjectWrite {
                offset: 0x78,
                width: 1,
                value: 1,
                relocate: false,
            }],
            children_offset: 0x1c,
            children_array_offset: 0x10,
            validation: vec![
                DatabaseBinding {
                    instance: 0x1032e8370,
                    post_init: call(0x100455f10, &[64]),
                    post_validate: call(0x1004560f0, &[64]),
                },
                trigger_database,
            ],
        },
        scopes: Default::default(),
    }
}

const M45_DECLARATIONS: DeclarationRecipe = DeclarationRecipe {
    numeric_modifier_member: "CPdxModifier<ModifierType, ModifierCategory, CModifier, CDefaultPdxModifierValueReader>::TryReadMember(CReader&, int)",
    modifier_serializer_constructor: "CCustomDescription::CSerializer::CSerializer(CCustomDescription&)",
    modifier_reference_database: "CStaticModifierDatabase",
    modifier_reference_member: "CModifier::TryReadMember(CReader&, int)",
    numeric_modifier_insert: "CPdxModifierEntry<ModifierType>& CPdxArray<CPdxModifierEntry<ModifierType>, int>::InsertAtEmplace<ModifierType, CFixedPoint&>(int, ModifierType, CFixedPoint&)",
    numeric_types: &[
        "signed char",
        "unsigned char",
        "short",
        "unsigned short",
        "int",
        "unsigned int",
        "long long",
        "unsigned long long",
        "CFixedPoint",
        "fpml::fixed_point<long long, (unsigned char)48, (unsigned char)15>",
        "float",
    ],
    inline_fixtures: M45_INLINE_FIXTURES,
    scope_object_offset: 0x1c,
    create_slot: 0x10,
    trigger_scope_slot: 0x78,
    effect_scope_slot: 0x80,
    trigger_parser: ParserSlots {
        read: 0x30,
        member: 0x38,
        initializer: 0x70,
    },
    effect_parser: ParserSlots {
        read: 0x10,
        member: 0x18,
        initializer: 0x90,
    },
    trigger_names: CommandSlots {
        assign: 0x28,
        role: 0x20,
    },
    effect_names: CommandSlots {
        assign: 0x20,
        role: 0x50,
    },
    persistent: PersistentRecipe {
        string_array: [8, 0x14, 0x28],
        compound_sizes: [M45_EVENT_TARGET_SIZE as i64, 0x30, 0x18],
        value_token: M45_VALUE_TOKEN,
        token_text: M45_TOKEN_TEXT,
        read_slot: 0x20,
        member_slot: 0x28,
        container_mask_offset: 0xac,
        families: &[
            FamilyAnchor {
                symbol: "CPdxModifier<ModifierType, ModifierCategory, CModifier, CDefaultPdxModifierValueReader>::Read(CReader&)",
                slot: AnchorSlot::Read,
                family: crate::BlockFamily::Modifier,
                delegate_slot: None,
            },
            FamilyAnchor {
                symbol: "CMeanTimeToHappen::Read(CReader&)",
                slot: AnchorSlot::Read,
                family: crate::BlockFamily::Weight,
                delegate_slot: None,
            },
            FamilyAnchor {
                symbol: "CTriggeredModifierBase<CStaticModifier>::ReadMember(CReader&, int)",
                slot: AnchorSlot::Member,
                family: crate::BlockFamily::TriggeredModifier,
                delegate_slot: Some(0x40),
            },
            FamilyAnchor {
                symbol: "CTriggeredModifierBase<CCustomDescriptionModifier>::ReadMember(CReader&, int)",
                slot: AnchorSlot::Member,
                family: crate::BlockFamily::TriggeredModifier,
                delegate_slot: Some(0x40),
            },
        ],
    },
    command_children: crate::engine::analysis::grammar::ChildLayout {
        data: 0x10,
        count: 0x1c,
        token: 0x20,
    },
    numeric_key_reader: "CToken::ReadValue(int&) const",
    reader_token_offset: 0x38,
    reader_value_token_offset: M45_VALUE_TOKEN,
    token_text_offset: M45_TOKEN_TEXT,
    event_target_size: M45_EVENT_TARGET_SIZE,
    effect_validation_slot: 0x98,
    trigger_validation_slot: 0x68,
    effect_target_getter_slot: 0x88,
    trigger_target_getter_slot: 0x80,
    boolean_tokens: ["yes", "no"],
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
    modifier_nodes: crate::engine::analysis::modifier_nodes::ModifierNodeLayout {
        category_offset: 0xdc,
        category_argument: 5,
        calculation_argument: 3,
        calculation_function_offset: 8,
    },
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

const M45_VALUE_TOKEN: u64 = 0x278;
const M45_TOKEN_TEXT: u64 = 0x10;

/// An initial loader with inline file and definition reads, verified on this exact build.
pub(in crate::binding) struct InlineFixtureRecipe {
    pub directory: &'static str,
    pub owner: &'static str,
    pub key_field: &'static str,
    pub loader: &'static str,
    pub reader_call: u64,
    pub root_call: u64,
    pub file_end: u64,
}

pub(in crate::binding) const M45_INLINE_FIXTURES: &[InlineFixtureRecipe] = &[InlineFixtureRecipe {
    directory: "common/special_projects",
    owner: "CSpecialProjectType",
    key_field: "key",
    loader: "CSpecialProjectDatabase::Init()",
    reader_call: 0x160,
    root_call: 0x1d0,
    file_end: 0xa8,
}];
