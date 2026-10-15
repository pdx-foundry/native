use super::*;
use crate::{GrammarProperty, protocol::observation::KeyStorage};
use std::fs;

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn fixture_bindings_follow_reader_arguments_and_owner_symbols() {
    let native = crate::session::installed_native();
    let analysis = &native.bound().analysis;
    let fields = analysis
        .fixture_registry_fields("common/traditions")
        .unwrap()
        .bindings;
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
    let relic_fields = analysis
        .fixture_registry_fields("common/relics")
        .unwrap()
        .bindings;
    assert!(relic_fields.iter().any(|field| field.name == "portrait"
        && field.storage.map(|storage| storage.offset) == Some(728)));
    assert_eq!(
        analysis
            .verified()
            .unwrap()
            .symbol(super::super::MODIFIER_LOG_DEFINITIONS),
        Ok(0x10097181c)
    );
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
    let analysis = BoundAnalysis::new(
        crate::binding::targets::test_declarations(),
        &[],
        None,
        installation,
        Default::default(),
    );
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
                loader_class: "loader".into(),
                address: 0x1000,
                initial_loader: Some(0x2000),
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
        symbol_index: OnceLock::new(),
        field_input: OnceLock::new(),
        schedule: OnceLock::new(),
    };
    assert!(analysis.catalog.set(Ok(catalog)).is_ok());

    let (installation, _) = Installation::open(&root.path().join("image")).unwrap();
    let plan = ExecutionPlan {
        binding: Binding {
            invalidated: analysis.invalidated.clone(),
            analysis: std::sync::Arc::new(analysis),
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
fn every_fixture_registry_refuses_without_a_derived_loader() {
    use crate::binding::{Binding, ExecutionPlan, compose};
    use crate::engine::analysis::{directories::Directory, discovery::CandidateRecord};

    // A loader body with no call to its LoadFromReader.
    let loader = std::iter::repeat_n(0xd503201fu32, 64)
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("image");
    fs::write(&path, support::macho_with_text(&loader)).unwrap();
    let (installation, _) = Installation::open(&path).unwrap();
    let analysis = BoundAnalysis::new(
        crate::binding::targets::test_declarations(),
        &[],
        None,
        installation.clone(),
        Default::default(),
    );
    let catalog = Catalog {
        candidates: vec![NamedCandidate {
            record: CandidateRecord {
                database: "CTraditionDatabase".into(),
                owner_candidate: "CTraditionType".into(),
                loader_class: "loader".into(),
                address: 0x1000,
                initial_loader: Some(0x2000),
            },
            directory: Directory::Named("common/traditions".into()),
        }],
        symbols: Vec::new(),
        strings: BTreeMap::new(),
        pointers: BTreeMap::new(),
        bound_slots: Default::default(),
        imports: BTreeMap::new(),
        symbol_index: OnceLock::new(),
        field_input: OnceLock::new(),
        schedule: OnceLock::new(),
    };
    assert!(analysis.catalog.set(Ok(catalog)).is_ok());

    let plan = ExecutionPlan {
        binding: Binding {
            invalidated: analysis.invalidated.clone(),
            analysis: std::sync::Arc::new(analysis),
            operation: Some(compose::synthetic_variation()),
            installation,
        },
    };
    let request = crate::FixtureRequest::field_outcomes(
        "common/traditions/native.txt",
        "tr_native = { }",
        vec![crate::FixtureFieldQuestion::new(
            "common/traditions",
            "tr_native",
            "custom_tooltip",
        )],
    );
    let error = plan.fixture_setup(&request).unwrap_err();
    assert_eq!(
        error.0,
        "Fixture registry has no verified loader and owner boundary"
    );
}

#[test]
fn field_input_refuses_an_owner_that_no_registry_has() {
    use crate::engine::analysis::{
        directories::Directory,
        discovery::{CandidateRecord, loader_owner::LoaderOwner},
    };

    // The symbols that the key readers need, so that the held record reads.
    let symbols = [
        "CToken::CToken(CToken const&)",
        "CEventTarget::CEventTarget(CToken, EScopeType, CString const&)",
        "CEventTarget::operator=(CEventTarget&&)",
        "void CPdxArray<CString, int>::SetSizeAndEmplace<>(int, const&)",
        "void CPdxOptional<CString>::SetEmplace<char const*>(char const*&&)",
        "CReader::Read(CString&, bool)",
        "CReader::Read(CPersistent&)",
    ]
    .into_iter()
    .zip(0x9000..)
    .map(|(name, address)| Symbol {
        address,
        name: name.into(),
    })
    .collect();
    let held = CandidateRecord {
        database: "CTraditionDatabase".into(),
        owner_candidate: "CTraditionType".into(),
        loader_class: "loader".into(),
        address: 0x1000,
        initial_loader: Some(0x2000),
    };
    let (_image, analysis) = fixture();
    let catalog = Catalog {
        candidates: vec![NamedCandidate {
            record: held.clone(),
            directory: Directory::Named("common/traditions".into()),
        }],
        symbols,
        strings: BTreeMap::new(),
        pointers: BTreeMap::new(),
        bound_slots: Default::default(),
        imports: BTreeMap::new(),
        symbol_index: OnceLock::new(),
        field_input: OnceLock::new(),
        schedule: OnceLock::from(Ok(ScheduledDatabases {
            inventory: ScheduleInventory {
                rows: 2,
                unjoined: vec!["CLoaderDatabase".into()],
                unresolved: 0,
            },
            loader_directories: BTreeMap::from([(
                "CLoaderDatabase".into(),
                Directory::Named("common/loader".into()),
            )]),
            loader_owners: Ok(BTreeMap::from([(
                "CLoaderDatabase".into(),
                LoaderOwner::Owner("CLoaderItem".into()),
            )])),
            loader_reasons: &[],
        })),
    };
    assert!(analysis.catalog.set(Ok(catalog)).is_ok());
    let verified = analysis.verified().unwrap();

    assert!(verified.field_input("CTraditionType").is_ok());
    assert!(verified.field_input("CLoaderItem").is_ok());
    assert!(matches!(
        verified.field_input("COther"),
        Err(AnalysisError::InvalidRange)
    ));
}

#[test]
fn a_root_key_owner_answers_for_the_other_class_and_names_the_keyed_class() {
    use crate::engine::analysis::{directories::Directory, discovery::loader_owner::LoaderOwner};
    use crate::{Answer, Basis, BuildId, Completeness, GapKind, Native, Source};

    let (_image, analysis) = fixture();
    let schedule = ScheduledDatabases {
        inventory: ScheduleInventory {
            rows: 1,
            unjoined: vec!["CPlanetDatabase".into()],
            unresolved: 0,
        },
        loader_directories: BTreeMap::from([(
            "CPlanetDatabase".into(),
            Directory::Named("common/planets".into()),
        )]),
        loader_owners: Ok(BTreeMap::from([(
            "CPlanetDatabase".into(),
            LoaderOwner::ByRootKey {
                key: "random_list".into(),
                keyed: "CRandomList".into(),
                other: "CPlanet".into(),
            },
        )])),
        loader_reasons: &[],
    };
    assert_eq!(
        schedule
            .loader_registry("common/planets")
            .unwrap()
            .unwrap()
            .owner,
        "CPlanet"
    );
    assert_eq!(
        schedule.keyed_class("common/planets"),
        Some(("random_list", "CRandomList"))
    );

    let catalog = Catalog {
        candidates: Vec::new(),
        symbols: Vec::new(),
        strings: BTreeMap::new(),
        pointers: BTreeMap::new(),
        bound_slots: Default::default(),
        imports: BTreeMap::new(),
        symbol_index: OnceLock::new(),
        field_input: OnceLock::new(),
        schedule: OnceLock::from(Ok(schedule)),
    };
    assert!(analysis.catalog.set(Ok(catalog)).is_ok());
    let native = Native::from_binding(crate::binding::Binding {
        installation: analysis.installation.clone(),
        invalidated: analysis.invalidated.clone(),
        analysis: std::sync::Arc::new(analysis),
        operation: None,
    });
    let mut answer = Answer {
        value: (),
        completeness: Completeness::Complete,
        gaps: Vec::new(),
        source: Source::new(BuildId("test".into()), "test", Basis::StaticAnalysis),
    };

    native
        .add_keyed_class_gap(
            crate::Operation::DerivedNames,
            "common/planets",
            &mut answer,
        )
        .unwrap();

    let [gap] = &answer.gaps[..] else {
        panic!("{:?}", answer.gaps);
    };
    assert_eq!(gap.kind, GapKind::OutsideMethod);
    assert!(
        gap.detail
            .contains("`random_list` are items of another class")
    );
}

#[test]
fn a_custom_loader_registry_without_one_owner_is_unsupported_with_the_reason() {
    use crate::engine::analysis::{
        directories::Directory,
        discovery::{CandidateRecord, loader_owner::LoaderOwner},
    };
    use crate::{Error, GapKind, Native, Operation};

    let (_image, analysis) = fixture();
    let loader_directories = BTreeMap::from([
        ("COneDatabase".into(), Directory::Named("common/one".into())),
        (
            "CColorsDatabase".into(),
            Directory::Named("common/colors".into()),
        ),
        (
            "CLeftDatabase".into(),
            Directory::Named("common/pair".into()),
        ),
        (
            "CRightDatabase".into(),
            Directory::Named("common/pair".into()),
        ),
        (
            "CTwoDatabase".into(),
            Directory::Ambiguous(vec!["common/first".into(), "common/second".into()]),
        ),
        ("CNoneDatabase".into(), Directory::Missing),
    ]);
    let schedule = ScheduledDatabases {
        inventory: ScheduleInventory {
            rows: 7,
            unjoined: loader_directories.keys().cloned().collect(),
            unresolved: 0,
        },
        loader_directories,
        loader_owners: Ok(BTreeMap::from([
            (
                "COneDatabase".into(),
                LoaderOwner::Several(vec!["CFirst".into(), "CSecond".into()]),
            ),
            ("CLeftDatabase".into(), LoaderOwner::Owner("CLeft".into())),
            ("CRightDatabase".into(), LoaderOwner::Owner("CRight".into())),
            ("CColorsDatabase".into(), LoaderOwner::NoOwner),
        ])),
        loader_reasons: &[crate::binding::groups::LoaderReason {
            database: "CColorsDatabase",
            reason: "the stated reason",
        }],
    };
    let catalog = Catalog {
        candidates: vec![NamedCandidate {
            record: CandidateRecord {
                database: "CTraditionDatabase".into(),
                owner_candidate: "CTraditionType".into(),
                loader_class: "loader".into(),
                address: 0x1000,
                initial_loader: Some(0x2000),
            },
            directory: Directory::Named("common/traditions".into()),
        }],
        symbols: Vec::new(),
        strings: BTreeMap::new(),
        pointers: BTreeMap::new(),
        bound_slots: Default::default(),
        imports: BTreeMap::new(),
        symbol_index: OnceLock::new(),
        field_input: OnceLock::new(),
        schedule: OnceLock::from(Ok(schedule)),
    };
    assert!(analysis.catalog.set(Ok(catalog)).is_ok());
    let native = Native::from_binding(crate::binding::Binding {
        installation: analysis.installation.clone(),
        invalidated: analysis.invalidated.clone(),
        analysis: std::sync::Arc::new(analysis),
        operation: None,
    });

    let answer = native.registries().unwrap();
    let names: Vec<_> = answer.value.iter().map(|registry| &registry.name).collect();
    assert_eq!(
        names,
        [
            "common/colors",
            "common/one",
            "common/pair",
            "common/traditions"
        ]
    );
    let unnamed = answer
        .gaps
        .iter()
        .find(|gap| gap.kind == GapKind::UnnamedRegistries)
        .unwrap();
    assert!(unnamed.detail.starts_with("2 of 7 scheduled databases"));

    let unsupported = |registry: &str| match native.registry_fields(registry) {
        Err(Error::Unsupported {
            operation: Operation::RegistryFields,
            reason,
        }) => reason,
        other => panic!("{registry}: {other:?}"),
    };
    assert_eq!(
        unsupported("common/one"),
        "the loader builds several classes with a member reader: CFirst, CSecond"
    );
    // A loader that the build's recipe states a reason for gives that reason.
    assert_eq!(unsupported("common/colors"), "the stated reason");
    // Two databases load one directory; neither owner is selected.
    assert_eq!(
        unsupported("common/pair"),
        "2 databases load this directory"
    );
    for operation in [Operation::DerivedNames, Operation::ModifierFamilies] {
        assert_eq!(
            native.unanswered_registry(operation, "common/one"),
            Error::Unsupported {
                operation,
                reason: "the loader builds several classes with a member reader: CFirst, CSecond"
                    .into(),
            }
        );
    }
    for unlisted in ["common/first", "common/second"] {
        assert_eq!(
            native.registry_fields(unlisted),
            Err(Error::UnknownRegistry {
                name: unlisted.into()
            })
        );
    }
}

#[test]
fn an_unknown_registry_name_is_recorded_as_its_error_inside_the_recording_root() {
    use crate::engine::analysis::{directories::Directory, discovery::CandidateRecord};
    use crate::{Error, Native};

    let (_image, analysis) = fixture();
    let catalog = Catalog {
        candidates: vec![NamedCandidate {
            record: CandidateRecord {
                database: "CTraditionDatabase".into(),
                owner_candidate: "CTraditionType".into(),
                loader_class: "loader".into(),
                address: 0x1000,
                initial_loader: Some(0x2000),
            },
            directory: Directory::Named("common/traditions".into()),
        }],
        symbols: Vec::new(),
        strings: BTreeMap::new(),
        pointers: BTreeMap::new(),
        bound_slots: Default::default(),
        imports: BTreeMap::new(),
        symbol_index: OnceLock::new(),
        field_input: OnceLock::new(),
        schedule: OnceLock::new(),
    };
    assert!(analysis.catalog.set(Ok(catalog)).is_ok());
    let binding = crate::binding::Binding {
        installation: analysis.installation.clone(),
        invalidated: analysis.invalidated.clone(),
        analysis: std::sync::Arc::new(analysis),
        operation: None,
    };
    let base = tempfile::tempdir().unwrap();
    let recording = base.path().join("recording");
    let native = Native::from_binding(binding).record_answers_to(&recording);
    let names = [
        "common/Traditions",
        "common/some-thing",
        "",
        "../escape",
        "common/../../escape",
    ];

    for name in names {
        assert_eq!(
            native.registry_fields(name),
            Err(Error::UnknownRegistry { name: name.into() }),
            "{name:?}"
        );
    }

    // A trailing slash names the same registry, so "/" is the empty name live and recorded.
    assert_eq!(
        native.registry_fields("/"),
        Err(Error::UnknownRegistry { name: "".into() })
    );

    let replay = Native::from_recorded_answers(&recording).unwrap();
    for name in names {
        assert_eq!(
            replay.registry_fields(name),
            Err(Error::UnknownRegistry { name: name.into() }),
            "{name:?}"
        );
    }
    assert_eq!(
        replay.registry_fields("/"),
        Err(Error::UnknownRegistry { name: "".into() })
    );
    let beside: Vec<_> = fs::read_dir(base.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(beside, ["recording"]);
}

#[test]
fn static_support_checks_the_pinned_executable_without_requiring_content() {
    use crate::{Native, Operation, Support};
    for mutation in ["changed", "missing"] {
        let (root, analysis) = fixture();
        let binding = crate::binding::Binding {
            installation: analysis.installation.clone(),
            invalidated: analysis.invalidated.clone(),
            analysis: std::sync::Arc::new(analysis),
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
    let binding = crate::session::installed_native().bound();
    let candidates = binding.analysis.verified().unwrap();
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
    let binding = crate::session::installed_native().bound();
    let index = binding.analysis.family_index().unwrap();
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
    assert_eq!(
        table.registries["common/bypass"].key_storage,
        KeyStorage::Offset(0x18)
    );
    assert_eq!(
        table.registries["common/zones"].key_storage,
        KeyStorage::Offset(0x10)
    );
    // A database that its own loader builds: its own instance and item layout, no directory.
    let strategic = &table.registries["common/strategic_resources"];
    assert_eq!(strategic.instance, 0x1032ed728);
    assert_eq!(strategic.directory_offset, None);
    assert_eq!((strategic.data_offset, strategic.count_offset), (0x8, 0x14));
    assert_eq!(strategic.key_storage, KeyStorage::Offset(0x20));
    assert!(
        binding
            .modifier_table_binding(&["common/no_such_registry".into()])
            .is_err()
    );
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn m452_registry_keys_follow_their_item_constructors() {
    let binding = crate::session::installed_native().bound();
    let selected = ["common/bypass".into(), "common/traditions".into()];
    let bindings = binding.registry_bindings(&selected).unwrap();
    assert_eq!(
        bindings["common/bypass"].key_storage,
        KeyStorage::Offset(0x18)
    );
    assert_eq!(
        bindings["common/traditions"].key_storage,
        KeyStorage::Offset(0x10)
    );
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn m452_registry_key_storage_sweep() {
    let binding = crate::session::installed_native().bound();
    let verified = binding.analysis.verified().unwrap();
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
        match &registry.key_storage {
            KeyStorage::Offset(offset) => {
                established += 1;
                if *offset != 0x10 {
                    println!("{}: key offset {offset:#x}", registry.name);
                }
            }
            KeyStorage::Unavailable(reason) => {
                *refused.entry(reason.clone()).or_default() += 1;
            }
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
    // Not the shared `installed_native`: the first queries must compute what the repeats read
    // from the caches.
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
            .fixture_registry_fields(registry)
            .unwrap()
            .fields;
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
    assert_eq!(*binding.executable().unwrap(), *image);
    fs::create_dir(root.path().join("common")).unwrap();
    fs::write(root.path().join("common/arbitrary.txt"), "content changed").unwrap();
    assert_eq!(*binding.executable().unwrap(), *image);
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

#[test]
fn an_unchanged_executable_is_hashed_once_and_a_metadata_change_is_checked_again() {
    let (root, binding) = fixture();
    let path = root.path().join("image");
    let first = binding.executable().unwrap();
    assert!(Arc::ptr_eq(&first, &binding.executable().unwrap()));
    let opened = fs::metadata(&path).unwrap().modified().unwrap();
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(opened + std::time::Duration::from_secs(60))
        .unwrap();
    assert_eq!(*binding.executable().unwrap(), *first);

    // The length and modification time at open, on the same inode: only the status-change time
    // differs.
    #[cfg(unix)]
    {
        let mut bytes = fs::read(&path).unwrap();
        *bytes.last_mut().unwrap() ^= 1;
        let file = fs::File::options().write(true).open(&path).unwrap();
        std::os::unix::fs::FileExt::write_all_at(&file, &bytes, 0).unwrap();
        file.set_modified(opened).unwrap();
        let error = binding.executable().unwrap_err();
        assert_eq!(
            error,
            AnalysisError::Unavailable {
                reasons: vec![UnavailableReason::TargetChanged]
            }
        );
        *bytes.last_mut().unwrap() ^= 1;
        fs::write(&path, bytes).unwrap();
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
    let analysis = BoundAnalysis::new(
        crate::binding::targets::test_declarations(),
        &[],
        None,
        installation,
        Default::default(),
    );
    assert_eq!(*analysis.executable().unwrap(), *image);
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
    let native = crate::session::installed_native();
    let verified = native.bound().analysis.verified().unwrap();
    for registry in ["common/traditions", "common/council_agendas"] {
        let candidate = unique_named_candidate(verified.named_candidates(), registry).unwrap();
        let input = verified
            .field_input(&candidate.record.owner_candidate)
            .unwrap();
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

    let native = crate::session::installed_native();
    let analysis = &native.bound().analysis;
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
        assert_eq!(
            cached.0.tokens[&bindings.boolean_tokens[0]].name.as_deref(),
            Some("yes")
        );
        assert_eq!(
            cached.0.tokens[&bindings.boolean_tokens[1]].name.as_deref(),
            Some("no")
        );
        for (addresses, name, count) in [
            (&bindings.token_copy, "CToken::CToken(CToken const&)", 2),
            (
                &bindings.target_from_token,
                "CEventTarget::CEventTarget(CToken, EScopeType, CString const&)",
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
fn a_kept_registry_field_result_still_checks_executable_integrity() {
    use crate::engine::analysis::{directories::Directory, discovery::CandidateRecord};

    let (root, analysis) = fixture();
    let catalog = Catalog {
        candidates: vec![NamedCandidate {
            record: CandidateRecord {
                database: "CTraditionDatabase".into(),
                owner_candidate: "CTraditionType".into(),
                loader_class: "loader".into(),
                address: 0x1000,
                initial_loader: Some(0x2000),
            },
            directory: Directory::Named("common/traditions".into()),
        }],
        symbols: Vec::new(),
        strings: BTreeMap::new(),
        pointers: BTreeMap::new(),
        bound_slots: Default::default(),
        imports: BTreeMap::new(),
        symbol_index: OnceLock::new(),
        field_input: OnceLock::new(),
        schedule: OnceLock::new(),
    };
    assert!(analysis.catalog.set(Ok(catalog)).is_ok());
    let kept = Arc::new(RegistryFieldResult::default());
    analysis.kept_registry_fields().insert(
        ("common/traditions".into(), "CTraditionType".into()),
        Arc::clone(&kept),
    );

    let (owner, result) = analysis
        .registry_field_result("common/traditions")
        .unwrap()
        .unwrap();
    assert_eq!(owner, "CTraditionType");
    assert!(Arc::ptr_eq(&result, &kept));

    let path = root.path().join("image");
    let original = fs::read(&path).unwrap();
    fs::write(&path, "changed").unwrap();
    assert!(matches!(
        analysis.registry_field_result("common/traditions"),
        Err(AnalysisError::Unavailable { .. })
    ));
    fs::write(path, original).unwrap();
    assert!(matches!(
        analysis.registry_field_result("common/traditions"),
        Err(AnalysisError::Unavailable { .. })
    ));
}

const CFIXED_POINT_READER: &str = "CReader::Read(CFixedPoint&)";
const FPML_READER: &str =
    "CReader::Read(fpml::fixed_point<long long, (unsigned char)48, (unsigned char)15>&)";
/// Scales that no engine reader has, so that a decoder can only have taken them from the facts.
const CFIXED_POINT_SCALE: u64 = 1_000;
pub(super) const FPML_SCALE: u64 = 64;

/// Numeric facts whose two fixed-point readers have these scales.
pub(super) fn fixed_point_numeric(
    cfixed_point: GrammarProperty<Option<u64>>,
    fpml: GrammarProperty<Option<u64>>,
) -> crate::engine::analysis::numeric::NumericFacts {
    use crate::engine::analysis::numeric::NumericReader;
    let reader = |scale| NumericReader {
        conversion: GrammarProperty::Known(Some(crate::NumericConversion {
            scale,
            ..Default::default()
        })),
        gaps: Vec::new(),
        boundary: Vec::new(),
    };

    crate::engine::analysis::numeric::NumericFacts {
        readers: [
            (CFIXED_POINT_READER.into(), reader(cfixed_point)),
            (FPML_READER.into(), reader(fpml)),
        ]
        .into(),
        ..Default::default()
    }
}

#[test]
fn a_fixed_point_reader_has_a_storage_decoder_only_with_a_known_scale() {
    use crate::engine::analysis::numeric::NumericFacts;
    use crate::protocol::observation::FixtureStorageDecoder;
    let both = [CFIXED_POINT_READER, FPML_READER];

    let numeric = fixed_point_numeric(
        GrammarProperty::Known(Some(250)),
        GrammarProperty::Unresolved,
    );
    assert_eq!(
        fixture_decoder(CFIXED_POINT_READER, &numeric),
        Some(FixtureStorageDecoder::FixedPoint { scale: 250 })
    );
    assert_eq!(fixture_decoder(FPML_READER, &numeric), None);

    for scale in [
        GrammarProperty::Partial(Some(250)),
        GrammarProperty::Known(None),
        GrammarProperty::Unresolved,
    ] {
        let numeric = fixed_point_numeric(scale.clone(), scale);
        for callee in both {
            assert_eq!(fixture_decoder(callee, &numeric), None, "{callee}");
        }
    }
    for callee in both {
        assert_eq!(fixture_decoder(callee, &NumericFacts::default()), None);
    }
    assert_eq!(
        fixture_decoder("CReader::Read(int&)", &NumericFacts::default()),
        Some(FixtureStorageDecoder::Integer)
    );
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
    let numeric = fixed_point_numeric(
        GrammarProperty::Known(Some(CFIXED_POINT_SCALE)),
        GrammarProperty::Known(Some(FPML_SCALE)),
    );
    for (callee, decoder) in [
        ("CReader::Read(int&)", FixtureStorageDecoder::Integer),
        ("CReader::Read(float&)", FixtureStorageDecoder::Float),
        ("CReader::Read(short&)", FixtureStorageDecoder::Integer16),
        (
            CFIXED_POINT_READER,
            FixtureStorageDecoder::FixedPoint {
                scale: CFIXED_POINT_SCALE,
            },
        ),
        (
            FPML_READER,
            FixtureStorageDecoder::FixedPoint { scale: FPML_SCALE },
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
        let bound = fixture_storage_binding(&field, &paths, &numeric).unwrap();
        assert_eq!(bound.offset, 48);
        assert_eq!(bound.decoder, decoder);
        let mut scratch_changed = field.clone();
        let ReaderJoin::Joined { arguments, .. } = &mut scratch_changed.readers[0] else {
            unreachable!()
        };
        arguments.remove("x8");
        assert_eq!(
            fixture_storage_binding(&scratch_changed, &paths, &numeric)
                .unwrap()
                .decoder,
            decoder
        );
        let mut ambiguous = paths.clone();
        ambiguous[0].domain = [7, 8];
        assert!(fixture_storage_binding(&field, &ambiguous, &numeric).is_none());
        ambiguous[0].domain = [8, 8];
        assert!(fixture_storage_binding(&field, &ambiguous, &numeric).is_none());
        for (register, value) in [("x0", Value::Owner(0)), ("x1", Value::Constant(48))] {
            let mut missing = field.clone();
            let ReaderJoin::Joined { arguments, .. } = &mut missing.readers[0] else {
                unreachable!()
            };
            arguments.insert(register.into(), value);
            assert!(fixture_storage_binding(&missing, &paths, &numeric).is_none());
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
        assert!(fixture_storage_binding(&field, &paths, &numeric).is_none());
    }
    field = original.clone();
    let ReaderJoin::Joined { callee, .. } = &mut field.readers[0] else {
        unreachable!()
    };
    *callee = "CReader::Read(unsigned int&)".into();
    assert!(fixture_storage_binding(&field, &paths, &numeric).is_none());
    field = original.clone();
    field.readers.push(field.readers[0].clone());
    assert!(fixture_storage_binding(&field, &paths, &numeric).is_none());
    paths[0].conditions.push(Condition {
        at: 0x1000,
        value: None,
        zero: true,
    });
    assert!(fixture_storage_binding(&original, &paths, &numeric).is_none());
}

/// Report storage coverage independently of whether the registry has a live loader boundary.
#[test]
#[ignore = "requires STELLARIS_PATH; reports every numeric registry field"]
fn numeric_fixture_storage_population() {
    let native = crate::session::installed_native();
    let analysis = &native.bound().analysis;
    for registry in native.registries().unwrap().value {
        let FixtureRegistryFields { fields, bindings } =
            analysis.fixture_registry_fields(&registry.name).unwrap();
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
    let native = crate::session::installed_native();
    let analysis = &native.bound().analysis;
    let request = crate::FixtureRequest::field_outcomes(
        "common/special_projects/native.txt",
        "special_project = { key = test }",
        vec![
            crate::FixtureFieldQuestion::new("common/special_projects", "test", "fleet_power")
                .with_parent_field("requirements"),
        ],
    );
    let two_files = request
        .clone()
        .with_file("common/special_projects/second.txt", "special_project = {}");
    assert!(analysis.inline_fixture(&two_files).is_err());
    let (binding, questions) = analysis.inline_fixture(&request).unwrap().unwrap();
    assert_eq!(binding.inline.unwrap().key_storage.offset, 8);
    assert_eq!(binding.load_entry, 0x100b9df4c);
    assert_eq!(binding.reader_entry, 0x100b9dfbc);
    assert_eq!(binding.reader_return, 0x100b9de94);
    let question = &questions[0];
    assert_eq!(question.token, Some(18433));
    let nested = question.nested.as_ref().unwrap();
    assert_eq!(nested.owner_offset, 0x5a0);
    assert_eq!(nested.member_entry, 0x100b965a8);
    let crate::protocol::observation::FixtureQuestionStorage::Bound(storage) = &question.storage
    else {
        panic!("{:?}", question.storage)
    };
    assert_eq!(storage.offset, 0x5d8);
    assert_eq!(
        storage.decoder,
        crate::protocol::observation::FixtureStorageDecoder::FixedPoint { scale: 32768 }
    );
}

#[test]
#[ignore = "requires exact M452 through STELLARIS_PATH"]
fn scoped_fixture_destinations() {
    let native = crate::session::installed_native();
    let analysis = &native.bound().analysis;
    let verified = analysis.verified().unwrap();
    for registry in ["common/council_agendas", "common/megastructures"] {
        let candidate = unique_named_candidate(verified.named_candidates(), registry).unwrap();
        let input = verified
            .field_input(&candidate.record.owner_candidate)
            .unwrap();
        let result = crate::engine::analysis::fields::analyze(&input).unwrap();
        let bindings = analysis.fixture_registry_fields(registry).unwrap().bindings;
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
    let native = crate::session::installed_native();
    assert_eq!(
        native.build().0,
        "c621723d9c8e0c1cd153319208d30a9dfbb9e63675be86f9d0ae7debeaa7fe1b"
    );
    let analysis = &native.bound().analysis;
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
        let bindings = analysis.fixture_registry_fields(registry).unwrap().bindings;
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
        let loader_class = format!("TSingleObjectGameDatabase<Database, Owner, {specialization}>");
        let symbols = vec![
            Symbol {
                address: 0x2000,
                name: format!("{loader_class}::LoadFromReader(CReader&, bool)"),
            },
            Symbol {
                address: 0x3000,
                name: "CReader::~CReader()".into(),
            },
        ];
        let code = arm64!(at 0x1000; bl extern 0x2000; mov x0, sp; bl extern 0x3000; ret);
        assert_eq!(
            fixture_reader_boundary(&code, 0x1000, &loader_class, &symbols).unwrap(),
            Some((0x2000, 0x1004))
        );
        assert_eq!(
            fixture_reader_boundary(&code, 0x1000, &loader_class, &symbols[..1]).unwrap(),
            None
        );
        for code in [
            arm64!(at 0x1000; bl extern 0x4000; mov x0, sp; bl extern 0x3000; ret),
            arm64!(at 0x1000; bl extern 0x2000; mov x0, x19; bl extern 0x3000; ret),
            arm64!(at 0x1000; bl extern 0x2000; mov x0, sp; bl extern 0x4000; ret),
            arm64!(at 0x1000; bl extern 0x2000; mov x0, sp; bl extern 0x3000; bl extern 0x2000; mov x0, sp; bl extern 0x3000; ret),
        ] {
            assert_eq!(
                fixture_reader_boundary(&code, 0x1000, &loader_class, &symbols).unwrap(),
                None
            );
        }
        let opposite = if specialization == "true" {
            "false"
        } else {
            "true"
        };
        let wrong_class = loader_class.replace(specialization, opposite);
        assert_eq!(
            fixture_reader_boundary(&code, 0x1000, &wrong_class, &symbols).unwrap(),
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

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn m452_block_evaluations_derive_their_slots_and_type_pointers() {
    use crate::BlockFamily::{Effect, Trigger, Weight};
    use crate::engine::analysis::callbacks::blocks::TypePointers;
    use crate::engine::analysis::callbacks::{Buffer, OffsetGetter};

    let binding = crate::session::installed_native().bound();
    let analysis = &binding.analysis;
    let recipe = analysis.declarations;
    let input = analysis.verified().unwrap().block_input(recipe).unwrap();
    assert_eq!(
        input.evaluation_slots,
        std::collections::BTreeMap::from([
            (0x10, Trigger),
            (0x18, Trigger),
            (0x20, Trigger),
            (0x48, Effect)
        ])
    );
    assert_eq!(
        input.tooltip_slots,
        std::collections::BTreeMap::from([(0x58, Trigger)])
    );
    // CSpecialistSubjectType::GetOnProgressCompleteEffect() const: add x0, x0, #0xa0; ret.
    let getter = |offset: i64, owner: &str| OffsetGetter {
        offset,
        owner: Some(owner.into()),
    };
    assert_eq!(
        input.offset_getters.get(&0x100ba5d44),
        Some(&getter(0xa0, "CSpecialistSubjectType"))
    );
    // CAgreementTermValue::GetActivateEffect() const, which CAgreementManager::CreateAgreement
    // calls on each element of a term array that no type pointer leads to.
    assert_eq!(
        input.offset_getters.get(&0x10004ec04),
        Some(&getter(0x978, "CAgreementTermValue"))
    );
    assert!(input.type_pointers.contains_key(&0x10004335c));
    // CScriptableValue::GetValue and GetValueNoScopeCopy evaluate a weight block.
    assert_eq!(input.evaluators.get(&0x10092f03c), Some(&Weight));
    assert_eq!(input.evaluators.get(&0x10092f0a8), Some(&Weight));

    let pointers = |function: u64| input.type_pointers.get(&function).cloned();
    let member = |register: usize, offset: i64, owner: &str| TypePointers {
        method_of: None,
        registers: Default::default(),
        members: [((register, offset), owner.into())].into(),
        elements: Default::default(),
        scope_parameters: Default::default(),
    };
    // CMission::Start: its mission type is the word at this + 0x18.
    assert_eq!(pointers(0x1009531d4), Some(member(0, 0x18, "CMissionType")));
    // CCountry::AddEdict(CEdict const*).
    assert_eq!(
        pointers(0x10028d49c).map(|pointers| pointers.registers),
        Some([(1, "CEdict".into())].into())
    );
    // CGalacticCommunity::PassResolution(CResolution&, bool, bool): the resolution's type is the
    // word at x1 + 0x18. Members of CGalacticCommunity store the null objects of CGalacticFocus
    // at this + 0x2c0 and of COwnershipLimits at this + 0x348.
    assert_eq!(
        pointers(0x10055ce08).map(|pointers| pointers.members),
        Some(
            [
                ((0, 0x2c0), "CGalacticFocus".into()),
                ((0, 0x348), "COwnershipLimits".into()),
                ((1, 0x18), "CResolutionType".into())
            ]
            .into()
        )
    );
    // CSubjectSpecialization::FinishConversion(CSpecialistSubjectType const&, CAgreement const&):
    // the class has no vtable, type info, constructor or destructor, but a `const` member.
    assert_eq!(
        pointers(0x100c6f8a0).map(|pointers| pointers.registers),
        Some([(1, "CSpecialistSubjectType".into())].into())
    );
    // (anonymous namespace)::TradeActions(CPdxArray<CTradableAction const*, int> const&, …): the
    // buffer at x0 + 8 holds tradable actions.
    assert_eq!(
        pointers(0x100cd2790)
            .map(|pointers| pointers.elements.get(&Buffer::Argument(0, 8)).cloned()),
        Some(Some("CTradableAction".into()))
    );
    // CTradeDealSide::CalcValueFor: CTradeDealSide() stores the vtable of a
    // CPdxArray<CTradableAction const*, int> at this + 0xa0, so its buffer is at this + 0xa8.
    assert_eq!(
        pointers(0x100ccbfd4)
            .map(|pointers| pointers.elements.get(&Buffer::Argument(0, 0xa8)).cloned()),
        Some(Some("CTradableAction".into()))
    );
    // CTechnologyStatus::GetTechWeight(CTechnology const*): CTechnology::ReadMember stores
    // TPdxNullObject<CTechnologyTier>::_pInstance at + 0x600.
    assert_eq!(
        pointers(0x100ca3290).map(|pointers| pointers.members.get(&(1, 0x600)).cloned()),
        Some(Some("CTechnologyTier".into()))
    );
    // CArchaeologicalSiteManager::CreateRandomSite loads the database instance through the slot
    // 0x102ffba68; CArchaeologicalSiteTypeDatabase() stores the vtable of a
    // CPdxArray<CArchaeologicalSiteType const*, int> at + 0x40.
    assert_eq!(
        pointers(0x100088528).map(|pointers| pointers
            .elements
            .get(&Buffer::Instance(0x102ffba68, 0x48))
            .cloned()),
        Some(Some("CArchaeologicalSiteType".into()))
    );
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn m452_planet_class_lookups_name_the_directory_that_the_loader_reads() {
    use crate::engine::analysis::references::initialization::Initialization;

    let binding = crate::session::installed_native().bound();
    let facts = binding.analysis.reference_facts().unwrap();
    let planet_classes = Some("common/planet_classes".to_owned());
    let readers: Vec<_> = facts
        .readers
        .values()
        .filter(|reader| reader.database == "CPlanetClassDatabase")
        .map(|reader| &reader.directory)
        .collect();
    assert_eq!(readers, [&planet_classes, &planet_classes]);

    let initializers: Vec<_> = facts
        .initializers
        .values()
        .filter_map(|initialization| match initialization {
            Initialization::Lookups(lookups) => Some(lookups),
            _ => None,
        })
        .flatten()
        .filter(|lookup| lookup.database == "CPlanetClassDatabase")
        .map(|lookup| &lookup.directory)
        .collect();
    assert_eq!(initializers, [&planet_classes, &planet_classes]);
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn m452_schedule_rows_outside_the_template_method_are_custom_loader_candidates() {
    use crate::engine::analysis::{directories::Directory, discovery::loader_owner::LoaderOwner};

    let binding = crate::session::installed_native().bound();
    let verified = binding.analysis.verified().unwrap();
    let schedule = verified.database_schedule().unwrap();
    let inventory = &schedule.inventory;
    assert_eq!(inventory.rows, 198);
    assert_eq!(inventory.unjoined.len(), 35);
    assert_eq!(inventory.unresolved, 0);

    let directory = |database: &str| schedule.loader_directories[database].clone();
    let named = |name: &str| Directory::Named(name.into());
    assert_eq!(
        directory("CStaticModifierDatabase"),
        named("common/static_modifiers")
    );
    assert_eq!(directory("CEventManager"), named("events"));
    // The directory is a global `char const*` that the loader reads from its pointer slot.
    assert_eq!(
        directory("CPlanetClassDatabase"),
        named("common/planet_classes")
    );
    assert_eq!(directory("CEmpireFlagDatabase"), named("flags"));
    // Two databases enumerate one directory.
    assert_eq!(
        directory("CEmpireDesignDatabase"),
        directory("CPrescriptedSpeciesDatabase")
    );

    let owners = schedule.loader_owners.as_ref().unwrap();
    let owner = |database: &str| match &owners[database] {
        LoaderOwner::Owner(owner) => owner.as_str(),
        other => panic!("{database}: {other:?}"),
    };
    for (database, expected) in [
        ("CAmbientObjectDatabase", "CAmbientObjectData"),
        ("CDiploPhraseDatabase", "CDiploPhrase"),
        ("CFallenEmpiresDatabase", "CFallenEmpireType"),
        ("CDesignerDatabase", "CShipDesign"),
        ("COnActionDatabase", "COnActionList"),
        ("COpinionModifierDatabase", "COpinionModifier"),
        ("CSpecialProjectDatabase", "CSpecialProjectType"),
        ("CSpeciesNamesDatabase", "CSpeciesNames"),
        ("CStartScreenMessageDatabase", "CStartScreenMessage"),
        // An inlined constructor: the loader forms the vtable address point.
        ("CStaticModifierDatabase", "CStaticModifier"),
        ("CStrategicResourceDatabase", "CStrategicResource"),
        ("CTerraformDatabase", "CTerraformLink"),
        ("CTraitDatabase", "CTrait"),
        ("CEventManager", "CEvent"),
        ("CEmpireDesignDatabase", "CEmpireDesign"),
        ("CPrescriptedSpeciesDatabase", "CPrescriptedSpecies"),
        ("CEmpireFlagDatabase", "SFlagColor"),
    ] {
        assert_eq!(owner(database), expected);
    }
    for database in [
        "CColorsDatabase",
        "CWeaponTagDatabase",
        "CShipBehaviorDatabase",
        "CWorldGfxDatabase",
    ] {
        assert_eq!(owners[database], LoaderOwner::NoOwner, "{database}");
    }
    // `random_list` builds the random list, inlined; every other key builds a planet class.
    assert_eq!(
        owners["CPlanetClassDatabase"],
        LoaderOwner::ByRootKey {
            key: "random_list".into(),
            keyed: "CPlanetClassRandomList".into(),
            other: "CPlanetClass".into(),
        }
    );
    assert_eq!(owners.len(), 22);
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M452 build"]
fn m452_list_readers_and_getter_readers_establish_their_lookups() {
    use crate::engine::analysis::references::{Missing, ReaderForm, reader};

    let binding = crate::session::installed_native().bound();
    let facts = binding.analysis.reference_facts().unwrap();
    let missing = |database: &str, form: ReaderForm| -> Vec<Result<Option<Missing>, &str>> {
        facts
            .readers
            .iter()
            .filter(|(callee, _)| {
                reader(callee).is_some_and(|reader| {
                    reader.form == form && (database.is_empty() || reader.database == database)
                })
            })
            .map(|(_, fact)| match &fact.lookup {
                Ok(lookup) => Ok(lookup.on_missing),
                Err(stop) => Err(stop.reason),
            })
            .collect()
    };

    assert_eq!(
        missing("", ReaderForm::ImmediateList),
        [Ok(Some(Missing::Omitted)); 13]
    );
    assert_eq!(
        missing("CScriptedActionDatabase", ReaderForm::DeferredList),
        [Ok(Some(Missing::NullObject))]
    );
    for database in ["CBypassTypeDatabase", "CPlanetClassDatabase"] {
        assert_eq!(
            missing(database, ReaderForm::Immediate),
            [Ok(Some(Missing::NullObject))],
            "{database}"
        );
    }
    // Resolver lambdas that call a getter and return a validity check other than slot 0x98.
    for database in [
        "CPlanetClassDatabase",
        "CSpecialProjectDatabase",
        "CStrategicResourceDatabase",
        "CTraitDatabase",
    ] {
        assert_eq!(
            missing(database, ReaderForm::Deferred),
            [Ok(Some(Missing::NullObject))],
            "{database}"
        );
    }
}
