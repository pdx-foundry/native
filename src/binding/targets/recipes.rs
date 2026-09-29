//! Recipes: which binding groups, live strategy and static layout make up one build's
//! operations. A recipe is host-neutral data.
use crate::binding::groups::M45_TEMPLATE_LAYOUT;
use crate::engine::analysis::callbacks::{CallbackLayout, RuleArray};
use crate::engine::analysis::declarations::ParserSlots;
use crate::engine::analysis::dynamic_names::CommandSlots;
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
    pub families: &'static [(&'static str, crate::BlockFamily)],
}

/// Layout facts that the declaration methods need on this exact build.
pub(in crate::binding) struct DeclarationRecipe {
    /// Concrete modifier parser and numeric-entry insertion functions.
    pub numeric_modifier_member: &'static str,
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

pub(super) const M45_RELEASE: Recipe = Recipe {
    groups: &[
        BindingGroupId::M45TemplateRegistryLayout,
        BindingGroupId::M45CategoryFixture,
    ],
    default_registries: M45_DEFAULT_REGISTRIES,
    strategy: StrategyId::MacSuspendedChildLoaderEntry,
    declarations: Some(&M45_DECLARATIONS),
    script_checks: Some(m45_script_checks),
};

/// Console memory readers and pre-filter logger on the exact M45-release ARM64 slice.
/// Verified by SDK-649 controls; the call argument widths follow the compiled signatures.
fn m45_script_checks() -> crate::protocol::script_check::ScriptCheckBinding {
    use crate::protocol::script_check::{
        CallBinding, CommandBinding, DatabaseBinding, ObjectWrite, ScriptCheckBinding,
    };
    let call = |address, widths: &[u8]| CallBinding {
        address,
        widths: widths.to_vec(),
    };
    let trigger_database = DatabaseBinding {
        instance: 0x1032e9758,
        post_init: call(0x100d05b08, &[64]),
        post_validate: call(0x100d05aa0, &[64]),
    };
    ScriptCheckBinding {
        string_constructor: call(0x102522174, &[64, 64]),
        string_assign: call(0x10022a680, &[64, 64]),
        blob_constructor: call(0x1024f2860, &[64]),
        blob_append: call(0x1024f2cfc, &[64, 64]),
        file_constructor: call(0x10250b73c, &[64, 64, 32, 32, 8]),
        lexer_constructor: call(0x1025ae170, &[64, 64, 8]),
        reader_constructor: call(0x1025b2014, &[64, 64]),
        string_size: 0x40,
        blob_size: 0x80,
        file_size: 0x400,
        lexer_size: 0x400,
        reader_size: 0x800,
        file_arguments: [1, 0, 0],
        lexer_argument: 0,
        file_name_offset: 0x20,
        string_tag_offset: 23,
        logger_entry: 0x1025048a0,
        logger_text_register: "x4".into(),
        logger_level_register: "w1".into(),
        trigger: CommandBinding {
            size: 0x200,
            constructor: call(0x100d06974, &[64]),
            read: call(0x100d06f08, &[64, 64, 64]),
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
            validation: vec![trigger_database.clone()],
        },
        effect: CommandBinding {
            size: 0x200,
            constructor: call(0x1004571b0, &[64]),
            read: call(0x1004576d4, &[64, 64, 64]),
            writes: vec![ObjectWrite {
                offset: 0x78,
                width: 1,
                value: 1,
                relocate: false,
            }],
            children_offset: 0x1c,
            validation: vec![
                DatabaseBinding {
                    instance: 0x1032e8370,
                    post_init: call(0x1004568b4, &[64]),
                    post_validate: call(0x100456a94, &[64]),
                },
                trigger_database,
            ],
        },
        scopes: Default::default(),
    }
}

const M45_DECLARATIONS: DeclarationRecipe = DeclarationRecipe {
    numeric_modifier_member: "CPdxModifier<ModifierType, ModifierCategory, CModifier, CDefaultPdxModifierValueReader>::TryReadMember(CReader&, int)",
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
