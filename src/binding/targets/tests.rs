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
fn a_recipe_cannot_supply_its_own_acceptance() {
    let (inputs, _) =
        crate::binding::compose::compose(&identity(), Ok(Default::default())).unwrap();
    let report = crate::qualification::evaluate(
        &inputs,
        &crate::qualification::Authority {
            accepted: vec![],
            withdrawn: vec![],
        },
        &crate::CapabilityRequest::default(),
        crate::ContextOrigin::Installation,
        None,
    );
    assert_eq!(report.qualification, crate::Qualification::Incomplete);
    assert_eq!(report.availability, crate::Availability::Unavailable);
    assert!(
        report
            .reasons
            .contains(&crate::UnavailableReason::QualificationMissing)
    );
}
