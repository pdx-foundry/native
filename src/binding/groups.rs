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
            string_tag_offset: 23,
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
        })
}

// Exact M45 disassembly: each PostReadInit traverses +0x48 pointers / +0x54 count.
// Tradition tab completion reads each object's CString at +0x10. The category constructor
// establishes the same key storage. A CString holds short text in place; bit 7 of the byte at
// +23 says that it holds a pointer to the text.
const M45_TRADITION_REGISTRIES: &[(&str, u64)] = &[
    ("traditions-load-entry", 0x100ce0474),
    ("tradition_categories-load-entry", 0x100cd7d70),
    ("registry-directory-offset", 0x10),
    ("registry-data-offset", 0x48),
    ("registry-count-offset", 0x54),
    ("registry-key-offset", 0x10),
    ("registry-pointer-size", 8),
    ("string-storage-tag-offset", 23),
];

fn declaration(group: BindingGroupId) -> &'static [(&'static str, u64)] {
    match group {
        BindingGroupId::M45TraditionRegistries => M45_TRADITION_REGISTRIES,
        BindingGroupId::M45CategoryFixture => &[],
    }
}

/// The registries that these groups bind, by internal name.
pub(super) fn registries(
    groups: &[BindingGroupId],
) -> std::collections::BTreeMap<String, crate::protocol::observation::RegistryBinding> {
    let bindings: std::collections::BTreeMap<&str, u64> = groups
        .iter()
        .flat_map(|group| declaration(*group))
        .copied()
        .collect();
    ["traditions", "tradition_categories"]
        .into_iter()
        .filter_map(|name| {
            let address = bindings.get(format!("{name}-load-entry").as_str())?;
            Some((
                name.into(),
                crate::protocol::observation::RegistryBinding {
                    name: name.into(),
                    directory: format!("common/{name}"),
                    load_entry: *address,
                    directory_offset: bindings["registry-directory-offset"],
                    data_offset: bindings["registry-data-offset"],
                    count_offset: bindings["registry-count-offset"],
                    key_offset: bindings["registry-key-offset"],
                    pointer_size: bindings["registry-pointer-size"],
                    string_tag_offset: bindings["string-storage-tag-offset"],
                },
            ))
        })
        .collect()
}
