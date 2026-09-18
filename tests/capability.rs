#![cfg(feature = "test-support")]

use pdx_native::test_support::{SyntheticCase, engine};
use pdx_native::{
    Availability, CapabilityRequest, ContextOrigin, Qualification, UnavailableReason,
};

#[test]
fn accepted_admission_does_not_require_historical_evidence_bytes() {
    let context = engine(SyntheticCase::Accepted);
    let report = context.capability(&CapabilityRequest::default());
    assert_eq!(report.context, context.identity());
    assert_eq!(report.origin, ContextOrigin::Synthetic);
    assert_eq!(report.qualification, Qualification::Qualified);
    assert_eq!(report.availability, Availability::Available);
    assert!(report.reasons.is_empty());
    assert_eq!(report.qualification_records, ["synthetic-acceptance-v1"]);
    assert_eq!(report.evidence.len(), 1);
    assert_eq!(
        report.evidence[0].path,
        "unavailable-history/synthetic-qualification.json"
    );
}

#[test]
fn fixed_cases_report_independent_qualification_and_availability() {
    let cases = [
        (
            SyntheticCase::HelperMismatch,
            Qualification::Incomplete,
            UnavailableReason::HelperMismatch,
        ),
        (
            SyntheticCase::HelperUnavailable,
            Qualification::Incomplete,
            UnavailableReason::PrerequisiteMissing,
        ),
        (
            SyntheticCase::RecipeOnly,
            Qualification::Incomplete,
            UnavailableReason::QualificationMissing,
        ),
        (
            SyntheticCase::Withdrawn,
            Qualification::Incomplete,
            UnavailableReason::QualificationWithdrawn,
        ),
        (
            SyntheticCase::RevisionMismatch,
            Qualification::Incomplete,
            UnavailableReason::RevisionMismatch,
        ),
        (
            SyntheticCase::ContentMismatch,
            Qualification::Incomplete,
            UnavailableReason::ContentMismatch,
        ),
        (
            SyntheticCase::TargetChanged,
            Qualification::Incomplete,
            UnavailableReason::TargetChanged,
        ),
        (
            SyntheticCase::ContentChanged,
            Qualification::Incomplete,
            UnavailableReason::ContentChanged,
        ),
        (
            SyntheticCase::InputUnavailable,
            Qualification::Incomplete,
            UnavailableReason::InputUnavailable,
        ),
        (
            SyntheticCase::MissingPrerequisite,
            Qualification::Qualified,
            UnavailableReason::PrerequisiteMissing,
        ),
        (
            SyntheticCase::NarrowQualification,
            Qualification::OutsideSupport,
            UnavailableReason::OutsideBounds,
        ),
    ];
    for (case, qualification, reason) in cases {
        let report = engine(case).capability(&CapabilityRequest::default());
        assert_eq!(report.origin, ContextOrigin::Synthetic);
        assert_eq!(report.qualification, qualification, "{case:?}");
        assert_eq!(report.availability, Availability::Unavailable, "{case:?}");
        assert!(
            report.reasons.contains(&reason),
            "{case:?}: {:?}",
            report.reasons
        );
    }
}

#[test]
fn requests_must_fit_both_declared_and_accepted_bounds() {
    let context = engine(SyntheticCase::Accepted);
    for name in ["", "technology", "TRADITIONS", "tradition", "traditions "] {
        let report = context.capability(&CapabilityRequest {
            registry: name.into(),
        });
        assert_eq!(report.qualification, Qualification::OutsideSupport);
        assert_eq!(report.availability, Availability::Unavailable);
    }
    let narrow = engine(SyntheticCase::NarrowQualification);
    let outside = narrow.capability(&CapabilityRequest::default());
    assert_eq!(outside.bounds.registries.len(), 2);
    assert_eq!(
        outside.accepted_bounds[0].registries,
        ["tradition_categories"]
    );
    assert_eq!(
        narrow
            .capability(&CapabilityRequest {
                registry: "tradition_categories".into()
            })
            .availability,
        Availability::Available
    );
}

#[test]
fn real_strategy_resolution_never_becomes_available_in_a_synthetic_context() {
    let report = engine(SyntheticCase::RealStrategy).capability(&CapabilityRequest::default());
    assert_eq!(report.qualification, Qualification::Qualified);
    assert_eq!(report.availability, Availability::Unavailable);
    let expected = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        UnavailableReason::ImplementationUnavailable
    } else {
        UnavailableReason::HostUnavailable
    };
    assert_eq!(report.reasons, [expected]);
}

#[test]
fn each_registry_requires_its_own_applicable_acceptance() {
    let context = engine(SyntheticCase::SplitQualifications);
    for name in ["traditions", "tradition_categories"] {
        let report = context.capability(&CapabilityRequest {
            registry: name.into(),
        });
        assert_eq!(report.qualification, Qualification::Qualified);
        assert_eq!(report.qualification_records.len(), 1);
    }
    assert_eq!(
        context
            .capability(&CapabilityRequest {
                registry: "technology".into()
            })
            .qualification,
        Qualification::OutsideSupport
    );
}

#[test]
fn withdrawal_does_not_hide_a_separate_current_acceptance() {
    let report =
        engine(SyntheticCase::ReplacementAcceptance).capability(&CapabilityRequest::default());
    assert_eq!(report.qualification, Qualification::Qualified);
    assert_eq!(report.qualification_records, ["synthetic-replacement"]);
    assert_eq!(report.availability, Availability::Available);
}

#[test]
fn unreadable_content_is_not_a_demonstrated_mismatch() {
    let report =
        engine(SyntheticCase::ContentUnavailable).capability(&CapabilityRequest::default());
    assert_eq!(report.qualification, Qualification::Incomplete);
    assert_eq!(report.availability, Availability::Unavailable);
    assert_eq!(report.reasons, [UnavailableReason::InputUnavailable]);
    assert!(report.accepted_bounds.is_empty());
    assert!(report.qualification_records.is_empty());
}

#[test]
fn accepted_synthetic_context_never_starts_a_live_registry_query() {
    let root = tempfile::tempdir().unwrap();
    let mut native = engine(SyntheticCase::Accepted)
        .with_supervisor(
            std::process::Command::new("must-not-execute"),
            pdx_native::RegistryOptions {
                retention_directory: root.path().into(),
                deadline_seconds: None,
            },
        )
        .unwrap();
    let error = native.get_registry("traditions").unwrap_err();
    assert!(error.to_string().contains("Synthetic"));
    assert!(matches!(
        native.get_registry("technology"),
        Err(pdx_native::RegistryError::Unsupported { .. })
    ));
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn registry_options_refuse_invalid_deadlines_and_retention_locations() {
    let root = tempfile::tempdir().unwrap();
    for deadline in [0, 181] {
        assert!(
            engine(SyntheticCase::Accepted)
                .with_supervisor(
                    std::process::Command::new("must-not-execute"),
                    pdx_native::RegistryOptions {
                        retention_directory: root.path().into(),
                        deadline_seconds: Some(deadline)
                    }
                )
                .is_err()
        );
    }
    assert!(
        engine(SyntheticCase::Accepted)
            .with_supervisor(
                std::process::Command::new("must-not-execute"),
                pdx_native::RegistryOptions {
                    retention_directory: "relative".into(),
                    deadline_seconds: None
                }
            )
            .is_err()
    );
}
