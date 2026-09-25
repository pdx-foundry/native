use super::*;
use std::fs;

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn fixture_bindings_follow_reader_arguments_and_owner_symbols() {
    let native = crate::Native::open(std::env::var_os("STELLARIS_PATH").unwrap()).unwrap();
    let analysis = native.bound().analysis.as_ref().unwrap();
    let fields = analysis.fixture_string_fields("common/traditions").unwrap();
    let found: Vec<_> = fields
        .iter()
        .map(|field| (field.name.as_str(), field.token, field.storage_offset))
        .collect();
    assert_eq!(
        found,
        [
            ("custom_tooltip", 10001, 0x1c0),
            ("custom_tooltip_with_modifiers", 11046, 0x1e8),
            ("unlocks_agenda", 14639, 0x5a0),
        ]
    );
    assert_eq!(
        analysis.fixture_loader("common/traditions").unwrap(),
        Some(FixtureLoader {
            load_entry: 0x100ce381c,
            reader_entry: 0x100ce4afc,
            reader_return: 0x100ce388c,
            constructor_entry: 0x100cdc930,
            member_entry: 0x100cdcf38,
        })
    );
    assert_eq!(
        analysis.fixture_loader("common/relics").unwrap(),
        Some(FixtureLoader {
            load_entry: 0x100ae4494,
            reader_entry: 0x100ae6860,
            reader_return: 0x100ae4504,
            constructor_entry: 0x100ae26d8,
            member_entry: 0x100ae2950,
        })
    );
    let relic_fields = analysis.fixture_string_fields("common/relics").unwrap();
    assert!(
        relic_fields
            .iter()
            .any(|field| field.name == "portrait" && field.storage_offset == 728)
    );
}

use crate::engine::analysis::analysis_support as support;

fn fixture() -> (tempfile::TempDir, BoundAnalysis) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("image");
    fs::write(&path, support::macho(&support::code())).unwrap();
    let (installation, _) = Installation::open(&path).unwrap();
    let analysis = BoundAnalysis::new(None, None, installation);
    (root, analysis)
}

#[test]
fn session_admission_follows_the_registries_that_the_executable_declares() {
    use crate::binding::{Binding, ExecutionPlan, compose};
    use crate::engine::analysis::{directories::Directory, discovery::CandidateRecord};
    use crate::protocol::session::SessionRequest;

    // More registries than M45-release declares: the count is a property of the build.
    let names: Vec<String> = (0..200)
        .map(|index| format!("common/synthetic_{index}"))
        .collect();
    let (root, analysis) = fixture();
    let candidates = names
        .iter()
        .map(|name| NamedCandidate {
            record: CandidateRecord {
                database: "CSyntheticDatabase".into(),
                owner_candidate: "CSyntheticOwner".into(),
                loader: "loader".into(),
                address: "0x1000".into(),
                initial_loader: Some("0x2000".into()),
                has_named_member_reader: false,
            },
            directory: Directory::Named(name.clone()),
        })
        .collect();
    let catalog = Catalog {
        candidates,
        symbols: Vec::new(),
        strings: BTreeMap::new(),
        pointers: BTreeMap::new(),
        bound_slots: Default::default(),
    };
    assert!(analysis.catalog.set(Ok(catalog)).is_ok());

    let (installation, _) = Installation::open(&root.path().join("image")).unwrap();
    let plan = ExecutionPlan {
        binding: Binding {
            analysis: Some(std::sync::Arc::new(analysis)),
            operation: Some(compose::synthetic_variation()),
            installation,
        },
    };
    let request = SessionRequest {
        installation: root.path().into(),
        build: "synthetic".into(),
        work_directory: root.path().join("unused"),
        startup_seconds: 1,
        idle_seconds: 1,
        registries: names.clone(),
        fault: None,
        fixture: None,
        fixture_fault: None,
        loaded_modifiers: None,
        modifier_fault: None,
    };
    request.validate().unwrap();
    assert_eq!(
        plan.registry_bindings(&request.registries).unwrap().len(),
        200
    );

    let mut unknown = names;
    unknown.push("common/undeclared".into());
    assert!(plan.registry_bindings(&unknown).is_err());
}

#[test]
fn static_support_checks_the_pinned_executable_without_requiring_content() {
    use crate::{Native, Operation, Support};
    for mutation in ["changed", "missing"] {
        let (root, analysis) = fixture();
        let binding = crate::binding::Binding {
            installation: analysis.installation.clone(),
            analysis: Some(std::sync::Arc::new(analysis)),
            operation: None,
        };
        let native = Native::from_binding(binding);
        for operation in [Operation::Registries, Operation::RegistryFields] {
            assert_eq!(native.supports(operation), Support::Supported);
        }
        let path = root.path().join("image");
        let original = fs::read(&path).unwrap();
        if mutation == "changed" {
            fs::write(&path, "changed").unwrap();
        } else {
            fs::remove_file(&path).unwrap();
        }
        for operation in [Operation::Registries, Operation::RegistryFields] {
            assert!(matches!(
                native.supports(operation),
                Support::Unsupported(_)
            ));
        }
        fs::write(path, original).unwrap();
        for operation in [Operation::Registries, Operation::RegistryFields] {
            assert!(matches!(
                native.supports(operation),
                Support::Unsupported(_)
            ));
        }
    }
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn every_m45_named_candidate_has_one_initial_loader_entry() {
    let installation =
        std::env::var_os("STELLARIS_PATH").expect("STELLARIS_PATH names the installation");
    let binding = crate::binding::Binding::open(std::path::Path::new(&installation)).unwrap();
    let candidates = binding.analysis.as_ref().unwrap().verified().unwrap();
    assert_eq!(candidates.named_candidates().len(), 164);
    assert!(
        candidates
            .named_candidates()
            .iter()
            .all(|candidate| candidate.record.initial_loader.is_some())
    );
    let known = binding
        .registry_bindings(&binding.default_registries())
        .unwrap();
    assert_eq!(known["common/traditions"].load_entry, 0x100ce3384);
    assert_eq!(known["common/tradition_categories"].load_entry, 0x100cdac80);
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn m45_loaded_modifier_table_binds_by_symbol_with_each_generator_registry() {
    let installation =
        std::env::var_os("STELLARIS_PATH").expect("STELLARIS_PATH names the installation");
    let binding = crate::binding::Binding::open(std::path::Path::new(&installation)).unwrap();
    let index = binding.analysis.as_ref().unwrap().family_index().unwrap();
    let registries: Vec<String> = index
        .registries
        .iter()
        .filter(|(_, code)| !code.roots.is_empty())
        .map(|(name, _)| name.clone())
        .collect();
    for generator in [
        "common/buildings",
        "common/bypass",
        "common/districts",
        "common/megastructures",
        "common/situations",
        "common/zones",
    ] {
        assert!(
            registries.iter().any(|name| name == generator),
            "{generator}"
        );
    }
    let table = binding.modifier_table_binding(&registries).unwrap();
    assert_eq!(table.documentation_entry, 0x100972384);
    assert_eq!(table.definitions, 0x10329da80);
    assert_eq!(table.array_data_offset, 0x8);
    assert_eq!(table.array_count_offset, 0x14);
    assert_eq!(table.definition_stride, 0x98);
    assert_eq!(table.token_offset, 0x78);
    assert_eq!(table.mask_offset, 0x84);
    assert_eq!(table.lookup, 0x103796d70);
    assert_eq!(table.lookup_size, 0x103796d88);
    assert_eq!(table.lookup_stride, 0x28);
    assert_eq!(table.registries["common/buildings"].instance, 0x10329edc0);
    assert_eq!(table.registries["common/bypass"].key_offset, Some(0x18));
    assert_eq!(table.registries["common/zones"].key_offset, Some(0x10));
    assert!(
        binding
            .modifier_table_binding(&["common/no_such_registry".into()])
            .is_err()
    );
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn m45_registry_keys_follow_their_item_constructors() {
    let installation =
        std::env::var_os("STELLARIS_PATH").expect("STELLARIS_PATH names the installation");
    let binding = crate::binding::Binding::open(std::path::Path::new(&installation)).unwrap();
    let selected = ["common/bypass".into(), "common/traditions".into()];
    let bindings = binding.registry_bindings(&selected).unwrap();
    assert_eq!(bindings["common/bypass"].key_offset, Some(0x18));
    assert_eq!(bindings["common/traditions"].key_offset, Some(0x10));
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn m45_registry_key_storage_sweep() {
    let installation =
        std::env::var_os("STELLARIS_PATH").expect("STELLARIS_PATH names the installation");
    let binding = crate::binding::Binding::open(std::path::Path::new(&installation)).unwrap();
    let verified = binding.analysis.as_ref().unwrap().verified().unwrap();
    let names: Vec<String> = verified
        .named_candidates()
        .iter()
        .filter_map(|candidate| match &candidate.directory {
            crate::engine::analysis::directories::Directory::Named(name) => Some(name.clone()),
            _ => None,
        })
        .collect();
    let bindings = binding.registry_bindings(&names).unwrap();
    let mut established = 0;
    let mut refused = std::collections::BTreeMap::<String, usize>::new();
    for registry in bindings.values() {
        match (registry.key_offset, registry.key_unavailable.as_deref()) {
            (Some(offset), None) => {
                established += 1;
                if offset != 0x10 {
                    println!("{}: key offset {offset:#x}", registry.name);
                }
            }
            (None, Some(reason)) => *refused.entry(reason.into()).or_default() += 1,
            other => panic!("invalid key binding: {other:?}"),
        }
    }
    println!("established={established}, refused={refused:?}");
    assert_eq!(established + refused.values().sum::<usize>(), names.len());
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn repeated_public_and_binding_queries_agree() {
    use crate::Native;
    let installation =
        std::env::var_os("STELLARIS_PATH").expect("STELLARIS_PATH names the installation");
    let native = Native::open(installation).unwrap();
    let first = native.registries().unwrap();
    assert_eq!(native.registries().unwrap(), first);
    for registry in ["common/traditions", "common/tradition_categories"] {
        let fields = native.registry_fields(registry).unwrap();
        assert_eq!(native.registry_fields(registry).unwrap(), fields);
        let fixture_fields = native
            .bound()
            .analysis
            .as_ref()
            .unwrap()
            .registry_fields(registry)
            .unwrap()
            .unwrap();
        assert_eq!(fixture_fields, fields.value);
    }
    let selected = [
        "common/traditions".into(),
        "common/tradition_categories".into(),
    ];
    let first = native.bound().registry_bindings(&selected).unwrap();
    let second = native.bound().registry_bindings(&selected).unwrap();
    assert_eq!(
        serde_json::to_value(first).unwrap(),
        serde_json::to_value(second).unwrap()
    );
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn cached_static_answers_refuse_changed_or_missing_executables() {
    use crate::{Error, Native};
    let installed =
        std::env::var_os("STELLARIS_PATH").expect("STELLARIS_PATH names the installation");
    let (_, bytes) = Installation::open(std::path::Path::new(&installed)).unwrap();
    for mutation in ["changed", "missing"] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("image");
        fs::write(&path, &bytes).unwrap();
        let native = Native::open(&path).unwrap();
        assert!(!native.registries().unwrap().value.is_empty());
        assert!(matches!(
            native.registry_fields("common/no_such_registry"),
            Err(Error::UnknownRegistry { .. })
        ));
        if mutation == "changed" {
            fs::write(&path, "changed").unwrap();
        } else {
            fs::remove_file(&path).unwrap();
        }
        let error = native.registries().unwrap_err();
        match mutation {
            "changed" => assert_eq!(error, Error::BuildChanged),
            _ => assert!(matches!(error, Error::Unsupported { .. })),
        }
        for registry in ["common/traditions", "common/no_such_registry"] {
            assert!(matches!(
                native.registry_fields(registry),
                Err(Error::BuildChanged | Error::Unsupported { .. })
            ));
        }
        assert!(
            native
                .bound()
                .registry_bindings(&["common/traditions".into()])
                .is_err()
        );
        fs::write(path, &bytes).unwrap();
        assert_eq!(native.registries().unwrap_err(), error);
        assert!(
            native
                .bound()
                .registry_bindings(&["common/traditions".into()])
                .is_err()
        );
    }
}

#[test]
fn reader_uses_verified_executable_bytes_without_content_or_live_tools() {
    let (root, binding) = fixture();
    assert!(!root.path().join("common").exists());
    let image = support::macho(&support::code());
    assert_eq!(binding.executable().unwrap(), image);
    fs::create_dir(root.path().join("common")).unwrap();
    fs::write(root.path().join("common/arbitrary.txt"), "content changed").unwrap();
    assert_eq!(binding.executable().unwrap(), image);
}

#[test]
fn a_changed_or_missing_executable_permanently_invalidates_static_reads() {
    for mutation in ["changed", "missing"] {
        let (root, binding) = fixture();
        let path = root.path().join("image");
        let original = fs::read(&path).unwrap();
        if mutation == "changed" {
            fs::write(&path, "changed").unwrap();
        } else {
            fs::remove_file(&path).unwrap();
        }
        let error = binding.executable().unwrap_err();
        let expected = if mutation == "changed" {
            UnavailableReason::TargetChanged
        } else {
            UnavailableReason::InputUnavailable
        };
        assert_eq!(
            error,
            AnalysisError::Unavailable {
                reasons: vec![expected]
            }
        );
        fs::write(path, original).unwrap();
        assert_eq!(binding.executable().unwrap_err(), error);
    }
}

#[cfg(unix)]
#[test]
fn retargeted_executable_permanently_invalidates_static_reads() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let image = support::macho(&support::code());
    let original = root.path().join("original");
    let replacement = root.path().join("replacement");
    let hint = root.path().join("hint");
    fs::write(&original, &image).unwrap();
    fs::write(&replacement, &image).unwrap();
    symlink(&original, &hint).unwrap();
    let (installation, _) = Installation::open(&hint).unwrap();
    let analysis = BoundAnalysis::new(None, None, installation);
    assert_eq!(analysis.executable().unwrap(), image);
    fs::remove_file(&hint).unwrap();
    symlink(&replacement, &hint).unwrap();
    let error = analysis.executable().unwrap_err();
    assert_eq!(
        error,
        AnalysisError::Unavailable {
            reasons: vec![UnavailableReason::TargetChanged]
        }
    );
    fs::remove_file(&hint).unwrap();
    symlink(&original, &hint).unwrap();
    assert_eq!(analysis.executable().unwrap_err(), error);
}

#[test]
fn range_reader_refuses_unmapped_truncated_misaligned_and_overlapping_sections() {
    let bytes = support::macho(&support::code());
    for (address, length) in [
        (0xffc, 4),
        (0x1028, 8),
        (0x1000, 0),
        (0x1001, 4),
        (0x1000, 3),
        (u64::MAX - 3, 8),
        (0x1000, 4100),
    ] {
        assert_eq!(
            binary::code_range(&bytes, address, length),
            Err(AnalysisError::InvalidRange)
        );
    }
    let mut truncated = bytes.clone();
    truncated.pop();
    assert_eq!(
        binary::code_range(&truncated, 0x1000, 44),
        Err(AnalysisError::InvalidRange)
    );
    // A second __text section with the same mapped range must not silently choose one.
    let mut overlap = bytes.clone();
    overlap[20..24].copy_from_slice(&232u32.to_le_bytes());
    overlap[36..40].copy_from_slice(&232u32.to_le_bytes());
    overlap[64..72].copy_from_slice(&264u64.to_le_bytes());
    overlap[96..100].copy_from_slice(&2u32.to_le_bytes());
    let mut section = overlap[104..184].to_vec();
    section[48..52].copy_from_slice(&264u32.to_le_bytes());
    overlap[152..156].copy_from_slice(&264u32.to_le_bytes());
    overlap.splice(184..184, section);
    assert_eq!(
        binary::code_range(&overlap, 0x1000, 44),
        Err(AnalysisError::InvalidRange)
    );
}

#[test]
fn range_reader_selects_the_same_arm64_slice_from_a_universal_image() {
    let arm = support::macho(&support::code());
    let mut intel = arm.clone();
    intel[4..8].copy_from_slice(&0x01000007u32.to_le_bytes());
    let offset = 48u32;
    let mut fat: Vec<u8> = [
        0xcafebabeu32,
        2,
        0x01000007,
        0,
        offset,
        intel.len() as u32,
        0,
        0x0100000c,
        0,
        offset + intel.len() as u32,
        arm.len() as u32,
        0,
    ]
    .into_iter()
    .flat_map(u32::to_be_bytes)
    .collect();
    fat.extend(intel);
    fat.extend(arm);
    assert_eq!(
        binary::code_range(&fat, 0x1000, 44).unwrap(),
        support::code()
    );
    fat[8..12].copy_from_slice(&0x0100000cu32.to_be_bytes());
    assert!(binary::code_range(&fat, 0x1000, 44).is_err());
}
