//! Typed engine bindings: the addresses and layouts that one binding group declares.
use super::targets::BindingGroupId;

/// SDK-483/517 read-entry and source joins, found on the M45-observe beta slice and moved to the
/// exact M45-release ARM64 slice by symbol name. The retained implementation is in Git at
/// dd33300; these bindings authorize observation only.
pub(super) fn fixture(
    groups: &[BindingGroupId],
) -> Option<crate::protocol::observation::FixtureBinding> {
    groups
        .iter()
        .any(|group| matches!(group, BindingGroupId::M45CategoryFixture))
        .then(|| crate::protocol::observation::FixtureBinding {
            registration_entry: 0x100456d24,
            load_entry: 0x100cdb168,
            field_entry: 0x100cd8e3c,
            reader_lexer_offset: 0x30,
            lexer_file_offset: 8,
            file_name_offset: 0x20,
            string_tag_offset: M45_TEMPLATE_LAYOUT.string_tag_offset,
            file_line_offset: 8,
            fields: [(16793, "tree_template"), (14263, "traditions")]
                .into_iter()
                .map(
                    |(token, name)| crate::protocol::observation::FixtureFieldBinding {
                        token,
                        name: name.into(),
                    },
                )
                .collect(),
            outcome_registries: vec![
                crate::protocol::observation::FixtureOutcomeRegistryBinding {
                    registry: "common/traditions".into(),
                    load_entry: 0x100ce381c,
                    reader_entry: 0x100ce4afc,
                    reader_return: 0x100ce388c,
                    constructor_entry: 0x100cdc930,
                    member_entry: 0x100cdcf38,
                    malformed_entry: 0x1025b274c,
                    unexpected_entry: 0x1025b24d4,
                    fields: Vec::new(),
                },
            ],
        })
}

// Exact M45 disassembly: each PostReadInit traverses +0x48 pointers / +0x54 count.
// Tradition tab completion reads each object's CString at +0x10. Each item class establishes
// its own key storage through constructor analysis. A CString holds short text in place; bit 7
// of the byte at +23 says that it holds a pointer to the text.
#[derive(Clone, Copy)]
pub(super) struct RegistryLayout {
    directory_offset: u64,
    data_offset: u64,
    count_offset: u64,
    pointer_size: u64,
    string_tag_offset: u64,
}

impl RegistryLayout {
    pub(super) fn string_tag_offset(self) -> u64 {
        self.string_tag_offset
    }

    /// Where a template database holds its items, for static methods that run its code.
    pub(super) fn database(self) -> crate::engine::analysis::families::DatabaseLayout {
        crate::engine::analysis::families::DatabaseLayout {
            items_offset: self.data_offset,
            count_offset: self.count_offset,
        }
    }
}

const M45_TEMPLATE_LAYOUT: RegistryLayout = RegistryLayout {
    directory_offset: 0x10,
    data_offset: 0x48,
    count_offset: 0x54,
    pointer_size: 8,
    string_tag_offset: 23,
};

pub(super) fn registry_layout(groups: &[BindingGroupId]) -> Option<RegistryLayout> {
    groups
        .iter()
        .any(|group| matches!(group, BindingGroupId::M45TemplateRegistryLayout))
        .then_some(M45_TEMPLATE_LAYOUT)
}

// Exact M45-release disassembly. `CModifier::LogDefinitions()` walks
// `CPdxModifier<…>::_Definitions`, a `CPdxArray` (data +0x8, count +0x14) of 0x98-byte
// definitions. It names each one with `CStaticLexer::GetString(token at +0x78)` and tags it with
// the mask at +0x84. `GetString(i)` returns element `i` of the lexer's lookup, a
// `CPdxArray<CString>` of 0x28-byte elements in unnamed globals at 0x103796d70. It first rebuilds
// the lookup when the lookup's count differs from the size at 0x103796d88.
#[derive(Clone, Copy)]
pub(super) struct ModifierTableLayout {
    array_data_offset: u64,
    array_count_offset: u64,
    definition_stride: u64,
    token_offset: u64,
    mask_offset: u64,
    lookup: u64,
    lookup_size: u64,
    lookup_stride: u64,
}

const M45_MODIFIER_TABLE: ModifierTableLayout = ModifierTableLayout {
    array_data_offset: 0x8,
    array_count_offset: 0x14,
    definition_stride: 0x98,
    token_offset: 0x78,
    mask_offset: 0x84,
    lookup: 0x103796d70,
    lookup_size: 0x103796d88,
    lookup_stride: 0x28,
};

pub(super) fn modifier_table(groups: &[BindingGroupId]) -> Option<ModifierTableLayout> {
    groups
        .iter()
        .any(|group| matches!(group, BindingGroupId::M45ModifierTable))
        .then_some(M45_MODIFIER_TABLE)
}

/// The engine locations that `modifier_table_binding` joins with the layouts.
pub(super) struct ModifierTableSymbols {
    pub documentation_entry: u64,
    pub definitions: u64,
    /// Each registry's database instance global and its item key offset, by content directory.
    pub registries: std::collections::BTreeMap<String, (u64, Result<u64, String>)>,
}

pub(super) fn modifier_table_binding(
    table: ModifierTableLayout,
    registry: RegistryLayout,
    symbols: ModifierTableSymbols,
) -> crate::protocol::observation::ModifierTableBinding {
    crate::protocol::observation::ModifierTableBinding {
        documentation_entry: symbols.documentation_entry,
        definitions: symbols.definitions,
        array_data_offset: table.array_data_offset,
        array_count_offset: table.array_count_offset,
        definition_stride: table.definition_stride,
        token_offset: table.token_offset,
        mask_offset: table.mask_offset,
        lookup: table.lookup,
        lookup_size: table.lookup_size,
        lookup_stride: table.lookup_stride,
        string_tag_offset: registry.string_tag_offset,
        registries: symbols
            .registries
            .into_iter()
            .map(|(directory, (instance, key_offset))| {
                let (key_offset, key_unavailable) = match key_offset {
                    Ok(offset) => (Some(offset), None),
                    Err(reason) => (None, Some(reason)),
                };
                (
                    directory,
                    crate::protocol::observation::ModifierRegistryBinding {
                        instance,
                        directory_offset: registry.directory_offset,
                        data_offset: registry.data_offset,
                        count_offset: registry.count_offset,
                        pointer_size: registry.pointer_size,
                        key_offset,
                        key_unavailable,
                    },
                )
            })
            .collect(),
    }
}

pub(super) fn registry_binding(
    layout: RegistryLayout,
    directory: &str,
    load_entry: u64,
    key_offset: Result<u64, String>,
) -> crate::protocol::observation::RegistryBinding {
    let (key_offset, key_unavailable) = match key_offset {
        Ok(offset) => (Some(offset), None),
        Err(reason) => (None, Some(reason)),
    };
    crate::protocol::observation::RegistryBinding {
        name: directory.into(),
        directory: directory.into(),
        load_entry,
        directory_offset: layout.directory_offset,
        data_offset: layout.data_offset,
        count_offset: layout.count_offset,
        key_offset,
        key_unavailable,
        pointer_size: layout.pointer_size,
        string_tag_offset: layout.string_tag_offset,
    }
}
