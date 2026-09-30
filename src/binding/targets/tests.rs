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

#[test]
fn release_and_hotfix_select_their_own_recipes() {
    for record in records::CATALOGUE {
        let image = ImageIdentity {
            executable: record.executable.into(),
            slice: record.slice.into(),
            architecture: record.architecture,
            format: record.format,
        };
        let selected = lookup(&image).unwrap();
        let expected_pause = record.recipe.world.map(|world| world().pause_entry);
        let expected_script = record.recipe.script_checks.unwrap()();
        assert_eq!(
            selected.world.map(|world| world().pause_entry),
            expected_pause
        );
        assert_eq!(
            selected.script_checks.unwrap()().logger_entry,
            expected_script.logger_entry
        );
        for other in records::CATALOGUE {
            if other.executable == record.executable {
                continue;
            }
            let mixed = ImageIdentity {
                slice: other.slice.into(),
                ..image.clone()
            };
            assert!(matches!(lookup(&mixed), Err(OpenError::UnknownTarget)));
        }
    }
}
