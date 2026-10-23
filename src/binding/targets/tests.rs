use super::*;

fn identity() -> ImageIdentity {
    let record = &records::CATALOGUE[0];
    ImageIdentity {
        executable: record.executable.into(),
        slice: record.slice.into(),
        architecture: record.architecture,
        format: record.format,
    }
}

#[test]
fn exact_catalogue_match_requires_both_hashes_and_machine_identity() {
    assert!(lookup(&identity()).is_ok());
    for field in ["executable", "slice", "architecture", "format"] {
        let mut changed = identity();
        match field {
            "executable" => changed.executable = "changed".into(),
            "slice" => changed.slice = "changed".into(),
            "architecture" => changed.architecture = object::Architecture::X86_64,
            _ => changed.format = object::BinaryFormat::Pe,
        }
        assert!(matches!(lookup(&changed), Err(OpenError::UnknownTarget)));
    }
}

#[test]
fn catalogue_order_cannot_break_a_duplicate_match() {
    let record = || TargetRecord {
        executable: records::CATALOGUE[0].executable,
        slice: records::CATALOGUE[0].slice,
        architecture: records::CATALOGUE[0].architecture,
        format: records::CATALOGUE[0].format,
        recipe: records::CATALOGUE[0].recipe,
    };
    assert!(matches!(
        lookup_in(&identity(), &[record(), record()]),
        Err(OpenError::Ambiguous)
    ));
}

/// Each catalogued build has its build directory, `builds/<slice prefix>/`: its provenance names
/// the build, its files carry the schema revisions that this release reads, and every reader that
/// a registry roots at is in `readers.json`.
#[test]
fn each_catalogued_build_has_a_matching_build_directory() {
    for record in records::CATALOGUE {
        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("builds")
            .join(&record.slice[..8]);
        let read = |name: &str| -> serde_json::Value {
            let path = directory.join(name);
            let text =
                std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path:?}: {error}"));
            serde_json::from_str(&text).unwrap_or_else(|error| panic!("{path:?}: {error}"))
        };
        let provenance = read("provenance.json");
        let tables = read("tables.json");
        let readers = read("readers.json");

        let build = &provenance["build"];
        assert_eq!(build["executable"], record.executable);
        assert_eq!(build["slice"], record.slice);
        assert_eq!(build["architecture"], "aarch64");
        assert_eq!(record.architecture, object::Architecture::Aarch64);
        assert_eq!(tables["build"]["executable"], record.executable);
        assert_eq!(readers["build"], record.executable);
        for (file, schema) in [("tables.json", &tables), ("readers.json", &readers)] {
            assert_eq!(provenance["schemas"][file], 1, "{file}");
            assert_eq!(schema["schema"], 1, "{file}");
        }

        let walked = readers["readers"].as_object().unwrap();
        for reader in walked.values() {
            let failed = reader["incomplete_because"].as_array().unwrap();
            assert_eq!(reader["complete"], failed.is_empty(), "{reader}");
        }
        for (name, registry) in readers["registries"].as_object().unwrap() {
            for root in registry["roots"].as_array().unwrap() {
                if let Some(reader) = root["reader"].as_str() {
                    assert!(walked.contains_key(reader), "{name}: {reader}");
                }
            }
        }
    }
}
