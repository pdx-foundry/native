#![cfg(feature = "maintainer-tools")]

use pdx_native::investigation::{self, CandidateRequest};

#[test]
fn invalid_candidate_bounds_fail_before_resource_allocation() {
    let root = tempfile::tempdir().unwrap();
    for hold_ms in [0, 30_001] {
        let output = root.path().join(format!("invalid-{hold_ms}"));
        let request = CandidateRequest {
            installation_hint: root.path().join("not-an-installation"),
            output: output.clone(),
            hold_ms,
        };
        let error = investigation::prepare(request).err().unwrap();
        assert!(error.to_string().contains("hold_ms"));
        assert!(!output.exists());
    }
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
#[test]
fn unsupported_hosts_refuse_before_touching_the_installation_or_output() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("must-not-exist");
    let request = CandidateRequest {
        installation_hint: root.path().join("not-an-installation"),
        output: output.clone(),
        hold_ms: 250,
    };
    let error = investigation::prepare(request).err().unwrap();
    assert!(error.to_string().contains("HostUnavailable"));
    assert!(!output.exists());
}

#[test]
fn observation_request_rejects_unretained_fixture_and_unbounded_deadline() {
    use investigation::ObservationRequest;
    let root = tempfile::tempdir().unwrap();
    for (fixture, deadline) in [
        ("different fixture", 180),
        (include_str!("fixtures/candidate/category.txt"), 0),
        (include_str!("fixtures/candidate/category.txt"), 181),
    ] {
        let output = root.path().join("must-not-exist");
        let error = investigation::prepare_observation(ObservationRequest {
            installation_hint: root.path().join("not-an-installation"),
            output: output.clone(),
            fixture: fixture.into(),
            deadline_seconds: deadline,
        })
        .err()
        .unwrap();
        assert!(error.to_string().contains("retained category fixture"));
        assert!(!output.exists());
    }
}
