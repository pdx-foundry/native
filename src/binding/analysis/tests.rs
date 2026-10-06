use super::*;
use std::fs;

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn fixture_bindings_follow_reader_arguments_and_owner_symbols() {
    let native = crate::Native::open(std::env::var_os("STELLARIS_PATH").unwrap()).unwrap();
    let analysis = native.bound().analysis.as_ref().unwrap();
    let fields = analysis.fixture_fields("common/traditions").unwrap();
    let found: Vec<_> = fields
        .iter()
        .filter_map(|field| Some((field.name.as_str(), field.token, field.storage?.offset)))
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
            load_entry: 0x100ce48ac,
            reader_entry: 0x100ce5b8c,
            reader_return: 0x100ce491c,
            constructor_entry: 0x100cdd9c0,
            member_entry: 0x100cddfc8,
        })
    );
    assert_eq!(
        analysis.fixture_loader("common/relics").unwrap(),
        Some(FixtureLoader {
            load_entry: 0x100ae39c8,
            reader_entry: 0x100ae5d94,
            reader_return: 0x100ae3a38,
            constructor_entry: 0x100ae1c0c,
            member_entry: 0x100ae1e84,
        })
    );
    let relic_fields = analysis.fixture_fields("common/relics").unwrap();
    assert!(relic_fields.iter().any(|field| field.name == "portrait"
        && field.storage.map(|storage| storage.offset) == Some(728)));
}

use crate::engine::analysis::analysis_support as support;

fn fixture() -> (tempfile::TempDir, BoundAnalysis) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("image");
    fs::write(
        &path,
        support::macho_with_text(&support::sample_arm64_code()),
    )
    .unwrap();
    let (installation, _) = Installation::open(&path).unwrap();
    let analysis = BoundAnalysis::new(None, None, installation, Default::default());
    (root, analysis)
}

#[test]
fn session_admission_follows_the_registries_that_the_executable_declares() {
    use crate::binding::{Binding, ExecutionPlan, compose};
    use crate::engine::analysis::{directories::Directory, discovery::CandidateRecord};
    use crate::protocol::session::SessionRequest;

    // More registries than the catalogued build declares: the count is a property of the build.
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
        imports: BTreeMap::new(),
    };
    assert!(analysis.catalog.set(Ok(catalog)).is_ok());

    let (installation, _) = Installation::open(&root.path().join("image")).unwrap();
    let plan = ExecutionPlan {
        binding: Binding {
            invalidated: analysis.invalidated.clone(),
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
        loaded_modifiers: None,
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
            invalidated: analysis.invalidated.clone(),
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
        assert!(native.bound().target_integrity().is_some());
    }
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn every_named_candidate_has_one_initial_loader_entry() {
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
    assert_eq!(known["common/traditions"].load_entry, 0x100ce4414);
    assert_eq!(known["common/tradition_categories"].load_entry, 0x100cdbd10);
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn m452_loaded_modifier_table_binds_by_symbol_with_each_generator_registry() {
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
    assert_eq!(table.documentation_entry, 0x10097181c);
    assert_eq!(table.definitions, 0x1032a1a80);
    assert_eq!(table.array_data_offset, 0x8);
    assert_eq!(table.array_count_offset, 0x14);
    assert_eq!(table.definition_stride, 0x98);
    assert_eq!(table.token_offset, 0x78);
    assert_eq!(table.mask_offset, 0x84);
    assert_eq!(table.lookup, 0x10379b8f0);
    assert_eq!(table.lookup_size, 0x10379b908);
    assert_eq!(table.lookup_stride, 0x28);
    assert_eq!(table.registries["common/buildings"].instance, 0x1032a2dc0);
    assert_eq!(table.registries["common/bypass"].key_offset, Some(0x18));
    assert_eq!(table.registries["common/zones"].key_offset, Some(0x10));
    assert!(
        binding
            .modifier_table_binding(&["common/no_such_registry".into()])
            .is_err()
    );
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn m452_registry_keys_follow_their_item_constructors() {
    let installation =
        std::env::var_os("STELLARIS_PATH").expect("STELLARIS_PATH names the installation");
    let binding = crate::binding::Binding::open(std::path::Path::new(&installation)).unwrap();
    let selected = ["common/bypass".into(), "common/traditions".into()];
    let bindings = binding.registry_bindings(&selected).unwrap();
    assert_eq!(bindings["common/bypass"].key_offset, Some(0x18));
    assert_eq!(bindings["common/traditions"].key_offset, Some(0x10));
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn m452_registry_key_storage_sweep() {
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
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
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
        // Fixture setup reads only these reader facts; the public answer adds numeric and scoped
        // facts on top.
        let readers = |fields: &[crate::Field]| -> Vec<_> {
            fields
                .iter()
                .map(|field| {
                    (
                        field.name.clone(),
                        field.reader.id.clone(),
                        field.reader.kind,
                        field.reader.family,
                    )
                })
                .collect()
        };
        let fixture_fields = native
            .bound()
            .analysis
            .as_ref()
            .unwrap()
            .registry_fields(registry)
            .unwrap()
            .unwrap();
        assert_eq!(readers(&fixture_fields), readers(&fields.value));
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
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
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
    let image = support::macho_with_text(&support::sample_arm64_code());
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
    let image = support::macho_with_text(&support::sample_arm64_code());
    let original = root.path().join("original");
    let replacement = root.path().join("replacement");
    let hint = root.path().join("hint");
    fs::write(&original, &image).unwrap();
    fs::write(&replacement, &image).unwrap();
    symlink(&original, &hint).unwrap();
    let (installation, _) = Installation::open(&hint).unwrap();
    let analysis = BoundAnalysis::new(None, None, installation, Default::default());
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
    let bytes = support::macho_with_text(&support::sample_arm64_code());
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
    let arm = support::macho_with_text(&support::sample_arm64_code());
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
        support::sample_arm64_code()
    );
    fat[8..12].copy_from_slice(&0x0100000cu32.to_be_bytes());
    assert!(binary::code_range(&fat, 0x1000, 44).is_err());
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn m452_persistent_field_families() {
    let native = crate::Native::open(std::env::var_os("STELLARIS_PATH").unwrap()).unwrap();
    let verified = native
        .bound()
        .analysis
        .as_ref()
        .unwrap()
        .verified()
        .unwrap();
    for registry in ["common/traditions", "common/council_agendas"] {
        let candidate = unique_named_candidate(verified.named_candidates(), registry).unwrap();
        let input = verified.field_input(candidate.record.clone()).unwrap();
        let result = crate::engine::analysis::fields::analyze(&input).unwrap();
        eprintln!(
            "destinations={:?}; gaps={:?}",
            result.persistent, result.gaps
        );
        let fields = crate::session::questions::normalized_fields(&result, &Default::default());
        assert_eq!(
            fields
                .iter()
                .find(|field| field.name == "modifier")
                .unwrap()
                .reader
                .family,
            crate::BlockFamily::Modifier,
            "{registry}"
        );
    }
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn m452_command_grammar_foundations_resolve_and_reuse_inputs() {
    use crate::DeclarationKind::{Effect, Trigger};

    let native = crate::Native::open(std::env::var_os("STELLARIS_PATH").unwrap()).unwrap();
    let analysis = native.bound().analysis.as_ref().unwrap();
    let verified = analysis.verified().unwrap();
    let name_at = |address| {
        verified
            .catalog
            .symbols
            .iter()
            .find(|symbol| symbol.address == address)
            .map(|symbol| symbol.name.as_str())
            .expect("binding has a symbol")
    };
    for (kind, assign, validation, target_getter) in
        [(Effect, 0x20, 0x98, 0x88), (Trigger, 0x28, 0x68, 0x80)]
    {
        let cached = analysis.grammar_input(kind).unwrap();
        assert!(std::ptr::eq(cached, analysis.grammar_input(kind).unwrap()));
        assert_eq!(cached.0.forms.token_text_offset, 0x10);
        let keys = &cached.0.key_readers;
        assert_eq!(
            [keys.array_data, keys.array_count, keys.string_stride],
            [8, 0x14, 0x28]
        );
        assert_eq!(keys.compound_sizes, [0x190, 0x30, 0x18]);
        assert_eq!(keys.value_token, 0x278);
        assert_eq!(keys.token_text, 0x10);
        assert_eq!(
            name_at(keys.string_emplace.unwrap()),
            "void CPdxArray<CString, int>::SetSizeAndEmplace<>(int, const&)"
        );
        assert_eq!(
            name_at(keys.optional_string.unwrap()),
            "void CPdxOptional<CString>::SetEmplace<char const*>(char const*&&)"
        );
        assert_eq!(
            name_at(keys.string_read.unwrap()),
            "CReader::Read(CString&, bool)"
        );
        assert!(!keys.token_copy.is_empty());
        assert!(!keys.target_construct.is_empty());
        assert_eq!(cached.0.forms.target_size, 0x190);
        assert!(!cached.0.forms.strings_from_text.is_empty());
        assert!(!cached.0.forms.string_copies.is_empty());
        assert!(!cached.0.forms.qualified_references.is_empty());
        let bindings = &cached.0.command_bindings;
        assert_eq!(bindings.reader_value_token_offset, 0x278);
        assert_eq!(bindings.assign_slot, assign);
        assert_eq!(bindings.validation_slot, validation);
        assert_eq!(bindings.target_getter_slot, target_getter);
        assert_eq!(bindings.boolean_tokens, [0x3ff8, 0x2cac]);
        assert_eq!(cached.0.tokens[&bindings.boolean_tokens[0]].name, "yes");
        assert_eq!(cached.0.tokens[&bindings.boolean_tokens[1]].name, "no");
        for (addresses, name, count) in [
            (&bindings.token_copy, "CToken::CToken(CToken const&)", 2),
            (
                &bindings.target_from_token,
                "CEventTarget::CEventTarget(CToken, EScopeType, CString const&)",
                2,
            ),
            (
                &bindings.target_from_id,
                "CEventTarget::CEventTarget(int)",
                2,
            ),
        ] {
            assert_eq!(addresses.len(), count, "{name}");
            for &address in addresses {
                assert_eq!(name_at(address), name);
            }
        }
        for (address, name) in [
            (
                bindings.target_create_from_token,
                "CEventTarget::CreateFromToken(int)",
            ),
            (
                bindings.target_move,
                "CEventTarget::operator=(CEventTarget&&)",
            ),
            (
                bindings.target_resolver,
                "CEventTarget::GetScope(CEventScope&, char const*) const",
            ),
            (
                bindings.target_scope_type,
                "CEventTarget::GetScopeType() const",
            ),
            (
                bindings.operator_readers[0],
                "CAssignOperator::Read(CReader&)",
            ),
            (
                bindings.operator_readers[1],
                "CCompareOperator::Read(CReader&)",
            ),
            (
                bindings.variable_assign,
                "CVariableValue::Assign(CToken const&, EScopeType, CString const&)",
            ),
        ] {
            assert_eq!(name_at(address), name);
        }
        assert_eq!(bindings.target_getters.len(), 27);
        assert_eq!(bindings.scope_accessors.len(), 41);
        assert_eq!(bindings.error_logs.len(), 5);
        use crate::engine::analysis::grammar::AccessorNullObject;
        let expected_accessors = [
            (
                "CGalacticCommunity const* CScopeObjectReference::GetObject<CGalacticCommunity>() const",
                None,
            ),
            (
                "CPopGroup const* CScopeObjectReference::GetObject<CPopGroup>() const",
                Some("TPdxNullObject<CPopGroup>::_pInstance"),
            ),
            (
                "CPopJob const* CScopeObjectReference::GetObject<CPopJob>() const",
                Some("TPdxNullObject<CPopJob>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetAgreement() const",
                Some("TPdxNullObject<CAgreement>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetAmbientObject() const",
                Some("TPdxNullObject<CAmbientObject>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetArchaeologicalSite() const",
                Some("TPdxNullObject<CArchaeologicalSite>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetArmy() const",
                Some("TPdxNullObject<CArmy>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetAstralRift() const",
                Some("TPdxNullObject<CAstralRift>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetBypass() const",
                Some("TPdxNullObject<CBypass>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetColony() const",
                Some("TPdxNullObject<CColony>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetCosmicStorm() const",
                Some("TPdxNullObject<CCosmicStorm>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetCosmicStormInfluenceField() const",
                Some("TPdxNullObject<CCosmicStormInfluenceField>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetCountry() const",
                Some("TPdxNullObject<CCountry>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetDebris() const",
                Some("TPdxNullObject<CDebris>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetDeposit() const",
                Some("TPdxNullObject<CDeposit>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetDesign() const",
                Some("TPdxNullObject<CShipDesign>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetDlcRecommendation() const",
                Some("TPdxNullObject<SDlcRecommendationScriptData>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetEspionageAsset() const",
                Some("TPdxNullObject<CEspionageAsset>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetEspionageOperation() const",
                Some("TPdxNullObject<CEspionageOperation>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetExhibit() const",
                Some("TPdxNullObject<CExhibit>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetFederation() const",
                Some("TPdxNullObject<CFederation>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetFirstContact() const",
                Some("TPdxNullObject<CFirstContact>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetFleet() const",
                Some("TPdxNullObject<CFleet>::_pInstance"),
            ),
            ("CScopeObjectReference::GetGalacticCommunity() const", None),
            (
                "CScopeObjectReference::GetGalacticObject() const",
                Some("TPdxNullObject<CGalacticObject>::_pInstance"),
            ),
            ("CScopeObjectReference::GetGrowthStage() const", None),
            (
                "CScopeObjectReference::GetLeader() const",
                Some("TPdxNullObject<CLeader>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetMegaStructure() const",
                Some("TPdxNullObject<CMegaStructure>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetMission() const",
                Some("TPdxNullObject<CMission>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetPlanet() const",
                Some("TPdxNullObject<CPlanet>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetPopFaction() const",
                Some("TPdxNullObject<CPopFaction>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetPopGroup() const",
                Some("TPdxNullObject<CPopGroup>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetPopJob() const",
                Some("TPdxNullObject<CPopJob>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetSector() const",
                Some("TPdxNullObject<CSector>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetShip() const",
                Some("TPdxNullObject<CShip>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetSituation() const",
                Some("TPdxNullObject<CSituation>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetSpecies() const",
                Some("TPdxNullObject<CSpecies>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetSpyNetwork() const",
                Some("TPdxNullObject<CSpyNetwork>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetStarbase() const",
                Some("TPdxNullObject<CStarbase>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetTrait() const",
                Some("TPdxNullObject<CTrait>::_pInstance"),
            ),
            (
                "CScopeObjectReference::GetWar() const",
                Some("TPdxNullObject<CWar>::_pInstance"),
            ),
        ];
        let mut actual_accessors: Vec<_> = bindings
            .scope_accessors
            .iter()
            .map(|(&address, null)| {
                let null_name = match null {
                    AccessorNullObject::Global(slot) => Some(name_at(*slot)),
                    AccessorNullObject::NoNullObject => None,
                };
                (name_at(address), null_name)
            })
            .collect();
        actual_accessors.sort();
        assert_eq!(actual_accessors, expected_accessors);
        let without_null_object: Vec<_> = bindings
            .scope_accessors
            .iter()
            .filter(|(_, null)| matches!(null, AccessorNullObject::NoNullObject))
            .map(|(&address, _)| name_at(address))
            .collect();
        assert_eq!(
            without_null_object,
            [
                "CScopeObjectReference::GetGalacticCommunity() const",
                "CGalacticCommunity const* CScopeObjectReference::GetObject<CGalacticCommunity>() const",
                "CScopeObjectReference::GetGrowthStage() const",
            ]
        );
        for (&address, null) in &bindings.scope_accessors {
            match null {
                AccessorNullObject::Global(slot) => {
                    assert!(name_at(*slot).starts_with("TPdxNullObject<"));
                    eprintln!("{} -> {}", name_at(address), name_at(*slot));
                }
                AccessorNullObject::NoNullObject => {
                    eprintln!("{} -> no null object", name_at(address))
                }
            }
        }
        for (label, addresses) in [
            ("typed target getters", &bindings.target_getters),
            ("error logs", &bindings.error_logs),
        ] {
            assert!(!addresses.is_empty(), "{label}");
            eprintln!("{kind:?} {label}: {} bindings", addresses.len());
            for &address in addresses {
                eprintln!("  {address:#x} {}", name_at(address));
            }
        }
    }
    let dynamic = analysis.dynamic_name_input().unwrap();
    for family in &dynamic.families {
        let cached = analysis.grammar_input(family.kind).unwrap();
        assert!(std::ptr::eq(family.declarations, &cached.0.declarations));
        assert!(std::ptr::eq(family.inventory, &cached.1));
    }
}

#[test]
fn cached_grammar_still_checks_executable_integrity() {
    let (root, analysis) = fixture();
    for cache in &analysis.grammar {
        assert!(cache.set(Err(AnalysisError::InvalidRange)).is_ok());
    }
    let path = root.path().join("image");
    let original = fs::read(&path).unwrap();
    fs::write(&path, "changed").unwrap();
    for kind in [
        crate::DeclarationKind::Effect,
        crate::DeclarationKind::Trigger,
    ] {
        assert!(matches!(
            analysis.grammar_input(kind),
            Err(AnalysisError::Unavailable { .. })
        ));
    }
    fs::write(path, original).unwrap();
    assert!(matches!(
        analysis.grammar_input(crate::DeclarationKind::Effect),
        Err(AnalysisError::Unavailable { .. })
    ));
}

#[test]
fn fixture_storage_requires_one_unconditional_owner_destination() {
    use crate::engine::analysis::fields::{
        Condition, PathOutcome, ReaderJoin, RootField, TokenPath, Value,
    };
    use crate::protocol::observation::FixtureStorageDecoder;
    let mut field = RootField {
        name: "synthetic".into(),
        token: 7,
        constructor: 0x1000,
        paths: vec![0],
        readers: vec![ReaderJoin::Joined {
            callee: "CReader::Read(int&)".into(),
            tail: true,
            arguments: BTreeMap::from([
                ("x0".into(), Value::Reader(0)),
                ("x1".into(), Value::Owner(48)),
                ("x8".into(), Value::Constant(7)),
            ]),
        }],
    };
    let mut paths = vec![TokenPath {
        domain: [7, 7],
        conditions: vec![],
        instructions: vec![0x1000],
        terminal: 0x1000,
        outcome: PathOutcome::Reader(field.readers[0].clone()),
    }];
    for (callee, decoder) in [
        ("CReader::Read(int&)", FixtureStorageDecoder::Integer),
        ("CReader::Read(float&)", FixtureStorageDecoder::Float),
        ("CReader::Read(short&)", FixtureStorageDecoder::Integer16),
        (
            "CReader::Read(CFixedPoint&)",
            FixtureStorageDecoder::FixedPoint { scale: 100_000 },
        ),
        (
            "CReader::Read(fpml::fixed_point<long long, (unsigned char)48, (unsigned char)15>&)",
            FixtureStorageDecoder::FixedPoint { scale: 32_768 },
        ),
        (
            "CReader::Read(CString&, bool)",
            FixtureStorageDecoder::String,
        ),
    ] {
        let ReaderJoin::Joined { callee: target, .. } = &mut field.readers[0] else {
            unreachable!()
        };
        *target = callee.into();
        let bound = fixture_storage_binding(&field, &paths).unwrap();
        assert_eq!(bound.offset, 48);
        assert_eq!(bound.decoder, decoder);
        let mut scratch_changed = field.clone();
        let ReaderJoin::Joined { arguments, .. } = &mut scratch_changed.readers[0] else {
            unreachable!()
        };
        arguments.remove("x8");
        assert_eq!(
            fixture_storage_binding(&scratch_changed, &paths)
                .unwrap()
                .decoder,
            decoder
        );
        let mut ambiguous = paths.clone();
        ambiguous[0].domain = [7, 8];
        assert!(fixture_storage_binding(&field, &ambiguous).is_none());
        ambiguous[0].domain = [8, 8];
        assert!(fixture_storage_binding(&field, &ambiguous).is_none());
        for (register, value) in [("x0", Value::Owner(0)), ("x1", Value::Constant(48))] {
            let mut missing = field.clone();
            let ReaderJoin::Joined { arguments, .. } = &mut missing.readers[0] else {
                unreachable!()
            };
            arguments.insert(register.into(), value);
            assert!(fixture_storage_binding(&missing, &paths).is_none());
        }
    }
    let original = field.clone();
    for (register, value) in [
        ("x0", Value::Owner(0)),
        ("x1", Value::Constant(48)),
        ("x1", Value::Owner(-1)),
    ] {
        field = original.clone();
        let ReaderJoin::Joined { arguments, .. } = &mut field.readers[0] else {
            unreachable!()
        };
        arguments.insert(register.into(), value);
        assert!(fixture_storage_binding(&field, &paths).is_none());
    }
    field = original.clone();
    let ReaderJoin::Joined { callee, .. } = &mut field.readers[0] else {
        unreachable!()
    };
    *callee = "CReader::Read(unsigned int&)".into();
    assert!(fixture_storage_binding(&field, &paths).is_none());
    field = original.clone();
    field.readers.push(field.readers[0].clone());
    assert!(fixture_storage_binding(&field, &paths).is_none());
    paths[0].conditions.push(Condition {
        at: 0x1000,
        value: None,
        zero: true,
    });
    assert!(fixture_storage_binding(&original, &paths).is_none());
}

/// Report storage coverage independently of whether the registry has a live loader boundary.
#[test]
#[ignore = "requires STELLARIS_PATH; reports every numeric registry field"]
fn numeric_fixture_storage_population() {
    let native = crate::Native::open(std::env::var_os("STELLARIS_PATH").unwrap()).unwrap();
    let analysis = native.bound().analysis.as_ref().unwrap();
    for registry in native.registries().unwrap().value {
        let fields = analysis
            .registry_fields(&registry.name)
            .unwrap()
            .unwrap_or_default();
        let bindings = analysis.fixture_fields(&registry.name).unwrap();
        let numeric: Vec<_> = fields.iter().filter(|field|
            matches!(field.reader.kind, crate::ReaderKind::Integer | crate::ReaderKind::FixedPoint | crate::ReaderKind::Float))
            .map(|field| serde_json::json!({"field": field.name, "reader": field.reader,
                "storage": bindings.iter().find(|bound| bound.name == field.name).and_then(|bound| bound.storage)}))
            .collect();
        println!(
            "{}",
            serde_json::json!({"registry": registry.name, "numeric": numeric,
            "loader": analysis.fixture_loader(&registry.name).unwrap().is_some()})
        );
    }
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn nested_fixture_bindings_derive_owner_key_and_numeric_storage() {
    let native = crate::Native::open(std::env::var_os("STELLARIS_PATH").unwrap()).unwrap();
    let analysis = native.bound().analysis.as_ref().unwrap();
    let request = crate::FixtureRequest::field_outcomes(
        "common/special_projects/native.txt",
        "special_project = { key = test }",
        vec![
            crate::FixtureFieldQuestion::new("common/special_projects", "test", "fleet_power")
                .with_parent_field("requirements"),
        ],
    );
    let base = super::super::groups::fixture(&[
        super::super::targets::BindingGroupId::M452CategoryFixture,
    ])
    .unwrap()
    .outcome_registries
    .remove(0);
    let (binding, questions) = analysis.inline_fixture(&request, &base).unwrap().unwrap();
    assert_eq!(binding.inline.unwrap().key_storage.offset, 8);
    assert_eq!(binding.load_entry, 0x100b9df4c);
    assert_eq!(binding.reader_entry, 0x100b9dfbc);
    assert_eq!(binding.reader_return, 0x100b9de94);
    let question = &questions[0];
    assert_eq!(question.storage_unavailable, None);
    assert_eq!(question.token, Some(18433));
    let nested = question.nested.as_ref().unwrap();
    assert_eq!(nested.owner_offset, 0x5a0);
    assert_eq!(nested.member_entry, 0x100b965a8);
    let storage = question.storage.unwrap();
    assert_eq!(storage.offset, 0x5d8);
    assert_eq!(
        storage.decoder,
        crate::protocol::observation::FixtureStorageDecoder::FixedPoint { scale: 32768 }
    );
}

#[test]
#[ignore = "requires exact M452 through STELLARIS_PATH"]
fn scoped_fixture_destinations() {
    let native = crate::Native::open(std::env::var_os("STELLARIS_PATH").unwrap()).unwrap();
    let analysis = native.bound().analysis.as_ref().unwrap();
    let verified = analysis.verified().unwrap();
    for registry in ["common/council_agendas", "common/megastructures"] {
        let candidate = unique_named_candidate(verified.named_candidates(), registry).unwrap();
        let input = verified.field_input(candidate.record.clone()).unwrap();
        let result = crate::engine::analysis::fields::analyze(&input).unwrap();
        let bindings = analysis.fixture_fields(registry).unwrap();
        for field in &result.fields {
            if matches!(
                field.name.as_str(),
                "agenda_cost" | "cycle_length_in_days" | "overclock_cooldown"
            ) {
                assert!(
                    bindings
                        .iter()
                        .find(|bound| bound.name == field.name)
                        .unwrap()
                        .storage
                        .is_some(),
                    "{} has no scoped decoder",
                    field.name
                );
            }
        }
    }
}

#[test]
#[ignore = "requires exact M452 through STELLARIS_PATH"]
fn m452_float_and_short_fixture_storage_bindings() {
    use crate::protocol::observation::FixtureStorageDecoder;
    let native = crate::Native::open(std::env::var_os("STELLARIS_PATH").unwrap()).unwrap();
    assert_eq!(
        native.build().0,
        "c621723d9c8e0c1cd153319208d30a9dfbb9e63675be86f9d0ae7debeaa7fe1b"
    );
    let analysis = native.bound().analysis.as_ref().unwrap();
    for (registry, fields, decoder) in [
        (
            "common/star_classes",
            &["icon_scale"][..],
            FixtureStorageDecoder::Float,
        ),
        (
            "common/storm_types",
            &[
                "cosmic_storm_galaxy_lightning_time",
                "cosmic_storm_galaxy_max_opacity",
            ][..],
            FixtureStorageDecoder::Float,
        ),
        (
            "common/astral_actions",
            &["unlock_threshold", "usages"][..],
            FixtureStorageDecoder::Integer16,
        ),
        (
            "common/sector_types",
            &[
                "max_systems",
                "min_systems",
                "min_colonies",
                "max_colonies",
                "max_jumps",
            ][..],
            FixtureStorageDecoder::Integer16,
        ),
    ] {
        let bindings = analysis.fixture_fields(registry).unwrap();
        assert!(
            analysis.fixture_loader(registry).unwrap().is_some(),
            "{registry}"
        );
        for name in fields {
            let field = bindings.iter().find(|field| field.name == *name).unwrap();
            assert_eq!(
                field
                    .storage
                    .unwrap_or_else(|| panic!("{registry}/{name}"))
                    .decoder,
                decoder,
                "{registry}/{name}"
            );
        }
    }
}

#[test]
fn fixture_loader_boundary_preserves_template_specialization_and_requires_cleanup() {
    use crate::engine::analysis::assembler::arm64;
    for specialization in ["true", "false"] {
        let database = format!("TSingleObjectGameDatabase<Database, Owner, {specialization}>");
        let loader = format!("{database}::LoadFile(char const*, bool)");
        let symbols = vec![
            Symbol {
                address: 0x2000,
                name: format!("{database}::LoadFromReader(CReader&, bool)"),
            },
            Symbol {
                address: 0x3000,
                name: "CReader::~CReader()".into(),
            },
        ];
        let code = arm64!(at 0x1000; bl extern 0x2000; mov x0, sp; bl extern 0x3000; ret);
        assert_eq!(
            fixture_reader_boundary(&code, 0x1000, &loader, &symbols).unwrap(),
            Some((0x2000, 0x1004))
        );
        assert_eq!(
            fixture_reader_boundary(&code, 0x1000, &loader, &symbols[..1]).unwrap(),
            None
        );
        for code in [
            arm64!(at 0x1000; bl extern 0x4000; mov x0, sp; bl extern 0x3000; ret),
            arm64!(at 0x1000; bl extern 0x2000; mov x0, x19; bl extern 0x3000; ret),
            arm64!(at 0x1000; bl extern 0x2000; mov x0, sp; bl extern 0x4000; ret),
            arm64!(at 0x1000; bl extern 0x2000; mov x0, sp; bl extern 0x3000; bl extern 0x2000; mov x0, sp; bl extern 0x3000; ret),
        ] {
            assert_eq!(
                fixture_reader_boundary(&code, 0x1000, &loader, &symbols).unwrap(),
                None
            );
        }
        let opposite = if specialization == "true" {
            "false"
        } else {
            "true"
        };
        let wrong_loader = loader.replace(specialization, opposite);
        assert_eq!(
            fixture_reader_boundary(&code, 0x1000, &wrong_loader, &symbols).unwrap(),
            None
        );
    }
}

#[test]
fn fixture_constructor_joins_only_one_direct_or_matching_new_entry_route() {
    use crate::engine::analysis::assembler::arm64;
    let database = "TSingleObjectGameDatabase<Database, Owner, true>";
    let symbols = vec![
        Symbol {
            address: 0x1000,
            name: format!("{database}::LoadFromReader(CReader&, bool)"),
        },
        Symbol {
            address: 0x2000,
            name: format!("{database}::ReadNewEntry(CReader&, CString const&)"),
        },
        Symbol {
            address: 0x3000,
            name: "Owner::Owner(int, CString const&)".into(),
        },
        Symbol {
            address: 0x4000,
            name: "Owner::Owner(int, CString const&)".into(),
        },
    ];
    let helper = decode_arm64(&arm64!(at 0x1000; bl extern 0x2000; ret), 0x1000).unwrap();
    assert_eq!(
        fixture_constructor_route(&helper, 0x1000, "Owner", &symbols),
        Some(FixtureConstructorRoute::NewEntry(0x2000))
    );
    assert_eq!(
        fixture_constructor_route(&helper, 0x2000, "Owner", &symbols),
        None
    );
    assert_eq!(
        fixture_constructor_route(&helper, 0x1000, "Owner", &symbols[1..]),
        None
    );
    let direct = decode_arm64(&arm64!(at 0x2000; bl extern 0x3000; ret), 0x2000).unwrap();
    assert_eq!(
        fixture_constructor_route(&direct, 0x2000, "Owner", &symbols),
        Some(FixtureConstructorRoute::Constructor(0x3000))
    );
    assert_eq!(
        fixture_constructor_route(&direct, 0x2000, "Other", &symbols),
        None
    );
    for code in [
        arm64!(at 0x1000; bl extern 0x3000; bl extern 0x4000; ret),
        arm64!(at 0x1000; bl extern 0x2000; bl extern 0x3000; ret),
        arm64!(at 0x1000; blr x8; ret),
    ] {
        let rows = decode_arm64(&code, 0x1000).unwrap();
        assert_eq!(
            fixture_constructor_route(&rows, 0x1000, "Owner", &symbols),
            None
        );
    }
}
