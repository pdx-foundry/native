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
    let invalid = [
        CapabilityRequest {
            registration_entries: 0,
            ..Default::default()
        },
        CapabilityRequest {
            registration_entries: 4,
            ..Default::default()
        },
        CapabilityRequest {
            category_fields: vec![],
            ..Default::default()
        },
        CapabilityRequest {
            category_fields: vec!["unknown".into()],
            ..Default::default()
        },
        CapabilityRequest {
            category_fields: vec!["traditions".into(), "traditions".into()],
            ..Default::default()
        },
    ];
    for request in invalid {
        let report = context.capability(&request);
        assert_eq!(report.qualification, Qualification::OutsideSupport);
        assert_eq!(report.availability, Availability::Unavailable);
    }
    let narrow = engine(SyntheticCase::NarrowQualification);
    let outside = narrow.capability(&CapabilityRequest::default());
    assert_eq!(outside.bounds.registration_entries, 3);
    assert_eq!(outside.accepted_bounds[0].registration_entries, 1);
    let request = CapabilityRequest {
        registration_entries: 1,
        ..Default::default()
    };
    assert_eq!(
        narrow.capability(&request).availability,
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
fn partial_acceptances_do_not_qualify_a_combined_window() {
    let context = engine(SyntheticCase::SplitQualifications);
    let report = context.capability(&CapabilityRequest::default());
    assert_eq!(report.qualification, Qualification::OutsideSupport);
    assert_eq!(report.accepted_bounds.len(), 2);
    for field in ["tree_template", "traditions"] {
        let request = CapabilityRequest {
            category_fields: vec![field.into()],
            ..Default::default()
        };
        assert_eq!(
            context.capability(&request).qualification,
            Qualification::Qualified
        );
    }
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
