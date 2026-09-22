//! Parity of the static questions with tracked expected output for the M45 build.
//! Needs the real executable: set `STELLARIS_PATH` and run with `--ignored`. No game starts.
use pdx_native::{
    Basis, Completeness, Declaration, DeclarationKind, DeclaredScopes, Error, Field, GapKind,
    Native, ReaderKind,
};

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn direct_declarations_match_the_recorded_m45_boundary() {
    let native = native();
    let gap_counts: serde_json::Value = expected("declaration-gaps.json");
    for kind in [DeclarationKind::Effect, DeclarationKind::Trigger] {
        let subject = match kind {
            DeclarationKind::Effect => "effect",
            DeclarationKind::Trigger => "trigger",
            _ => unreachable!("the test covers the M45 effect and trigger kinds"),
        };
        let answer = native.declarations(kind).unwrap();
        assert_eq!(answer.source.basis, Basis::Declared);
        let names: std::collections::BTreeSet<_> =
            answer.value.iter().map(|item| item.name.clone()).collect();
        let omitted: std::collections::BTreeSet<_> = answer
            .gaps
            .iter()
            .filter(|gap| {
                gap.kind == GapKind::UnresolvedPath
                    && !names.contains(gap.subject.as_deref().unwrap_or(""))
            })
            .filter_map(|gap| gap.subject.clone())
            .collect();
        let accounted: Vec<_> = names.union(&omitted).cloned().collect();
        assert_eq!(
            omitted.into_iter().collect::<Vec<_>>(),
            gap_counts[subject]["unreadable"]
                .as_array()
                .unwrap()
                .iter()
                .map(|name| name.as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            accounted,
            expected::<Vec<String>>(&format!("declarations-{subject}.json"))
        );
        assert_eq!(
            answer
                .gaps
                .iter()
                .filter(|gap| gap.kind == GapKind::UnnamedDeclaration)
                .count(),
            gap_counts[subject]["runtime_token_sites"].as_u64().unwrap() as usize
        );
        assert_eq!(
            answer.completeness,
            if answer
                .gaps
                .iter()
                .all(|gap| gap.kind == GapKind::OutsideMethod)
            {
                Completeness::Complete
            } else {
                Completeness::Partial
            }
        );
        let global_scope_gap = answer.gaps.iter().any(|gap| {
            gap.kind == GapKind::UnreadableInput && gap.detail == "scope name table not found"
        });
        for item in &answer.value {
            assert_eq!(item.targets, DeclaredScopes::Unresolved);
            assert!(!item.description.contains("Supported Scopes:"));
            assert!(!item.usage.contains("Supported Scopes:"));
            if item.scopes == DeclaredScopes::Unresolved && !global_scope_gap {
                assert!(
                    answer
                        .gaps
                        .iter()
                        .any(|gap| gap.kind == GapKind::UnresolvedPath
                            && gap.subject.as_deref() == Some(&item.name))
                );
            }
        }
        assert!(
            answer
                .gaps
                .iter()
                .any(|gap| gap.kind == GapKind::UnresolvedPath
                    && gap.subject.is_none()
                    && gap.detail == "target declarations are not followed by this method")
        );
        let samples: Vec<Declaration> = expected(&format!("declaration-samples-{subject}.json"));
        assert_eq!(samples.len(), 10);
        for sample in samples {
            assert_eq!(
                answer.value.iter().find(|item| item.name == sample.name),
                Some(&sample)
            );
        }
    }
    let effects = native.declarations(DeclarationKind::Effect).unwrap();
    assert_eq!(
        effects
            .value
            .iter()
            .find(|item| item.name == "win")
            .unwrap()
            .scopes,
        DeclaredScopes::Listed(vec!["country".into()])
    );
    assert!(
        matches!(effects.value.iter().find(|item| item.name == "add_blocker").unwrap().scopes, DeclaredScopes::Listed(ref scopes) if scopes.contains(&"colony".into()))
    );
    let triggers = native.declarations(DeclarationKind::Trigger).unwrap();
    assert_eq!(
        triggers
            .value
            .iter()
            .find(|item| item.name == "if")
            .unwrap()
            .scopes,
        DeclaredScopes::Any
    );
}

fn native() -> Native {
    Native::open(
        std::env::var_os("STELLARIS_PATH")
            .expect("STELLARIS_PATH names the installation or executable"),
    )
    .expect("the installed build is in the target catalogue")
}

fn expected<T: serde::de::DeserializeOwned>(name: &str) -> T {
    let path = format!("{}/tests/expected/m45/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn registries_are_named_by_their_content_directory() {
    let answer = native().registries().unwrap();
    let names: Vec<_> = answer.value.iter().map(|r| r.name.clone()).collect();
    assert_eq!(names, expected::<Vec<String>>("registries.json"));
    assert_eq!(answer.completeness, Completeness::Complete);
    assert_eq!(answer.source.basis, Basis::StaticAnalysis);
    // `common/ship_categories` passes a global CString; its static initializer names it.
    assert!(names.contains(&"common/ship_categories".to_owned()));
    assert!(
        answer
            .gaps
            .iter()
            .all(|gap| gap.kind != GapKind::UnnamedRegistries)
    );
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn registry_fields_match_and_share_reader_identities_across_registries() {
    let native = native();
    let mut potential = Vec::new();
    for (registry, file) in [
        ("common/traditions", "fields-traditions.json"),
        (
            "common/tradition_categories",
            "fields-tradition_categories.json",
        ),
        ("common/council_agendas", "fields-council_agendas.json"),
    ] {
        let answer = native.registry_fields(registry).unwrap();
        assert_eq!(answer.value, expected::<Vec<Field>>(file), "{registry}");
        assert_eq!(
            answer.completeness,
            if answer
                .gaps
                .iter()
                .all(|gap| gap.kind == GapKind::OutsideMethod)
            {
                Completeness::Complete
            } else {
                Completeness::Partial
            }
        );
        for field in &answer.value {
            let expected_gap = if field.reader.id.is_none() {
                Some(GapKind::UnresolvedReader)
            } else if field.reader.kind == ReaderKind::Unknown {
                Some(GapKind::ReaderSemantics)
            } else {
                None
            };
            if let Some(kind) = expected_gap {
                assert!(
                    answer
                        .gaps
                        .iter()
                        .any(|gap| gap.kind == kind && gap.subject.as_deref() == Some(&field.name)),
                    "{registry}: {}",
                    field.name
                );
            }
        }
        potential.extend(
            answer
                .value
                .into_iter()
                .filter(|field| field.name == "potential")
                .map(|field| field.reader.id.expect("potential has one reader")),
        );
    }
    assert!(potential.len() >= 2 && potential.windows(2).all(|pair| pair[0] == pair[1]));
    assert!(matches!(
        native.registry_fields("common/no_such_registry"),
        Err(Error::UnknownRegistry { .. })
    ));
}

#[test]
#[ignore = "requires STELLARIS_PATH with the exact M45 build"]
fn recorded_answers_equal_the_real_answers_apart_from_the_basis() {
    let directory = tempfile::tempdir().unwrap();
    let real = native().record_answers_to(directory.path());
    let registries = real.registries().unwrap();
    let fields = real.registry_fields("common/traditions").unwrap();
    let effects = real.declarations(DeclarationKind::Effect).unwrap();
    let unknown = real.registry_fields("common/no_such_registry");

    let recorded = Native::from_recorded_answers(directory.path()).unwrap();
    assert_eq!(recorded.build(), real.build());
    let mut again = recorded.registries().unwrap();
    assert_eq!(again.source.basis, Basis::Recorded);
    again.source.basis = registries.source.basis;
    assert_eq!(again, registries);
    let mut again = recorded.registry_fields("common/traditions").unwrap();
    again.source.basis = fields.source.basis;
    assert_eq!(again, fields);
    let mut again = recorded.declarations(DeclarationKind::Effect).unwrap();
    again.source.basis = effects.source.basis;
    assert_eq!(again, effects);
    // Errors are recorded too.
    assert_eq!(recorded.registry_fields("common/no_such_registry"), unknown);
    assert!(matches!(
        recorded.registry_fields("common/armies"),
        Err(Error::NotRecorded { .. })
    ));
}
