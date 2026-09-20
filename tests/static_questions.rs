//! Parity of the static questions with tracked expected output for the M45 build.
//! Needs the real executable: set `STELLARIS_PATH` and run with `--ignored`. No game starts.
use pdx_native::{Basis, Completeness, Error, Field, GapKind, Native, OpenRequest};

fn native() -> Native {
    Native::open(OpenRequest {
        installation_hint: std::env::var_os("STELLARIS_PATH")
            .expect("STELLARIS_PATH names the installation or executable")
            .into(),
    })
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
    assert_eq!(answer.completeness, Completeness::Partial);
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
        assert_eq!(answer.completeness, Completeness::Partial);
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
