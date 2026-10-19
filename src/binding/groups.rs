//! Typed engine bindings: the addresses and layouts that one binding group declares.
/// Fresh fixture pins for the exact 4.5.2 ARM64 slice. They are the SDK-483/517 read-entry and
/// source joins, found on the M45-observe beta slice and ported to each exact build by symbol
/// name. The retained implementation is in Git at dd33300; these bindings authorize observation
/// only.
pub(super) fn m452_fixture() -> crate::protocol::observation::FixtureBinding {
    crate::protocol::observation::FixtureBinding {
        reader_lexer_offset: 0x30,
        lexer_file_offset: 8,
        file_name_offset: 0x20,
        string_tag_offset: M45_TEMPLATE_LAYOUT.string_tag_offset,
        file_line_offset: 8,
        malformed_entry: 0x1025b5500,
        unexpected_entry: 0x1025b5288,
        validation: Some(crate::protocol::observation::FixtureValidationBinding {
            log_entry: 0x10250b6f4,
            log_text_register: "x4".into(),
            unformatted_log_entry: 0x10250b7c8,
            stream_log_entry: 0x102126434,
            stream_log_text_register: "x1".into(),
            sourced_log_entry: 0x101d242b0,
            sourced_log_text_register: "x20".into(),
            sourced_log_owner_register: "x19".into(),
            sourced_log_source_offset: 0x28,
            sourced_log_generated_frame_offset: 0x50,
        }),
    }
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
    pub(super) const fn string_tag_offset(self) -> u64 {
        self.string_tag_offset
    }

    /// Where a template database keeps its items and its directory text.
    pub(super) fn items(self) -> DatabaseItems {
        DatabaseItems {
            directory_offset: Some(self.directory_offset),
            data_offset: self.data_offset,
            count_offset: self.count_offset,
        }
    }

    /// Where a template database holds its items, for static methods that run its code.
    pub(super) fn database(self) -> crate::engine::analysis::families::DatabaseLayout {
        crate::engine::analysis::families::DatabaseLayout {
            items_offset: self.data_offset,
            count_offset: self.count_offset,
        }
    }
}

pub(super) const M45_TEMPLATE_LAYOUT: RegistryLayout = RegistryLayout {
    directory_offset: 0x10,
    data_offset: 0x48,
    count_offset: 0x54,
    pointer_size: 8,
    string_tag_offset: 23,
};

/// Where a database keeps its items, and its directory text when it holds one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DatabaseItems {
    pub directory_offset: Option<u64>,
    pub data_offset: u64,
    pub count_offset: u64,
}

/// Where a database that its own loader builds keeps its items, by the database's class.
pub(super) struct LoaderDatabase {
    pub class: &'static str,
    pub items: DatabaseItems,
}

// Exact M452 disassembly: `CStrategicResourceDatabase::CreateInstance` builds the database in
// place (no constructor), with the item array's data at +0x8 and its count at +0x14, which
// `Init` and `GetResource` use. It holds no directory text.
pub(super) const M452_LOADER_DATABASES: &[LoaderDatabase] = &[LoaderDatabase {
    class: "CStrategicResourceDatabase",
    items: DatabaseItems {
        directory_offset: None,
        data_offset: 0x8,
        count_offset: 0x14,
    },
}];

/// Why the loader of one database builds no class with a member reader, as read on the exact
/// build. A database with no stated reason gets the general one.
#[derive(Debug)]
pub(super) struct LoaderReason {
    pub database: &'static str,
    pub reason: &'static str,
}

// Exact M452 disassembly; `docs/native/registry-fields.md`, "Root-key owners", gives each loader's
// functions and what would answer its registry.
pub(super) const M452_LOADER_REASONS: &[LoaderReason] = &[
    LoaderReason {
        database: "CColorsDatabase",
        reason: "the loader reads fixed root keys of unnamed colours into its database and builds \
                 no item",
    },
    LoaderReason {
        database: "CWeaponTagDatabase",
        reason: "the loader registers each token of its files as a tag and builds no item",
    },
    LoaderReason {
        database: "CShipBehaviorDatabase",
        reason: "the loader reads each item in its own statement loop, not through a member reader",
    },
    LoaderReason {
        database: "CRandomNameDatabase",
        reason: "the loader reads each fixed root key's list of names into the database and \
                 builds no item",
    },
    LoaderReason {
        database: "CGameRules",
        reason: "the loader reads each fixed root key, a game rule, as a trigger block into the \
                 database and builds no item",
    },
    LoaderReason {
        database: "CWorldGfxDatabase",
        reason: "the loader reads the database itself, whose member reader builds and reads each \
                 item",
    },
];

/// The engine locations that `modifier_table_binding` joins with the layouts.
pub(super) struct ModifierTableSymbols {
    pub documentation_entry: u64,
    pub definitions: u64,
    /// Each registry's database instance global, where that database keeps its items, and its
    /// item key offset, by content directory.
    pub registries: std::collections::BTreeMap<String, (u64, DatabaseItems, Result<u64, String>)>,
}

pub(super) fn modifier_table_binding(
    table: crate::engine::analysis::modifier_table::Layout,
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
            .map(|(directory, (instance, items, key_offset))| {
                (
                    directory,
                    crate::protocol::observation::ModifierRegistryBinding {
                        instance,
                        directory_offset: items.directory_offset,
                        data_offset: items.data_offset,
                        count_offset: items.count_offset,
                        pointer_size: registry.pointer_size,
                        key_storage: key_storage(key_offset),
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
    crate::protocol::observation::RegistryBinding {
        name: directory.into(),
        directory: directory.into(),
        load_entry,
        directory_offset: layout.directory_offset,
        data_offset: layout.data_offset,
        count_offset: layout.count_offset,
        key_storage: key_storage(key_offset),
        pointer_size: layout.pointer_size,
        string_tag_offset: layout.string_tag_offset,
    }
}

fn key_storage(key_offset: Result<u64, String>) -> crate::protocol::observation::KeyStorage {
    use crate::protocol::observation::KeyStorage;

    match key_offset {
        Ok(offset) => KeyStorage::Offset(offset),
        Err(reason) => KeyStorage::Unavailable(reason),
    }
}
