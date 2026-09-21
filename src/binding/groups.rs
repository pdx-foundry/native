//! Typed engine bindings: the addresses and layouts that one binding group declares.
use super::targets::BindingGroupId;

/// SDK-483/517 read-entry and source joins on the exact M45-observe ARM64 slice. The
/// retained implementation is in Git at dd33300; these bindings authorize observation only.
pub(super) fn fixture(
    groups: &[BindingGroupId],
) -> Option<crate::protocol::observation::FixtureBinding> {
    groups
        .iter()
        .any(|group| matches!(group, BindingGroupId::M45CategoryFixture))
        .then(|| crate::protocol::observation::FixtureBinding {
            registration_entry: 0x1004559bc,
            load_entry: 0x100cd8258,
            field_entry: 0x100cd5f2c,
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
                    load_entry: 0x100ce090c,
                    reader_entry: 0x100ce1bec,
                    reader_return: 0x100ce097c,
                    constructor_entry: 0x100cd9a20,
                    member_entry: 0x100cda028,
                    malformed_entry: 0x1025ae998,
                    unexpected_entry: 0x1025ae720,
                    fields: Vec::new(),
                },
            ],
        })
}

// Exact M45 disassembly: each PostReadInit traverses +0x48 pointers / +0x54 count.
// Tradition tab completion reads each object's CString at +0x10. The category constructor
// establishes the same key storage. A CString holds short text in place; bit 7 of the byte at
// +23 says that it holds a pointer to the text.
#[derive(Clone, Copy)]
pub(super) struct RegistryLayout {
    directory_offset: u64,
    data_offset: u64,
    count_offset: u64,
    key_offset: u64,
    pointer_size: u64,
    string_tag_offset: u64,
}

const M45_TEMPLATE_LAYOUT: RegistryLayout = RegistryLayout {
    directory_offset: 0x10,
    data_offset: 0x48,
    count_offset: 0x54,
    key_offset: 0x10,
    pointer_size: 8,
    string_tag_offset: 23,
};

pub(super) fn registry_layout(groups: &[BindingGroupId]) -> Option<RegistryLayout> {
    groups
        .iter()
        .any(|group| matches!(group, BindingGroupId::M45TemplateRegistryLayout))
        .then_some(M45_TEMPLATE_LAYOUT)
}

pub(super) fn registry_binding(
    layout: RegistryLayout,
    directory: &str,
    load_entry: u64,
) -> crate::protocol::observation::RegistryBinding {
    crate::protocol::observation::RegistryBinding {
        name: directory.into(),
        directory: directory.into(),
        load_entry,
        directory_offset: layout.directory_offset,
        data_offset: layout.data_offset,
        count_offset: layout.count_offset,
        key_offset: layout.key_offset,
        pointer_size: layout.pointer_size,
        string_tag_offset: layout.string_tag_offset,
    }
}
