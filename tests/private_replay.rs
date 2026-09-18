use pdx_native::{
    Activation, ArtifactReference, CaptureOrigin, Completion, Disposal, Engine, Gap, ReplayRequest,
};
use std::{fs, path::PathBuf};

#[test]
#[ignore = "requires a restored private SDK-483 bundle; run tools/prepare-private-replay.py, then set PDX_NATIVE_PRIVATE_EVIDENCE"]
fn retained_accepted_attempts_reproduce_the_four_outcomes() {
    let root = std::env::var_os("PDX_NATIVE_PRIVATE_EVIDENCE")
        .map(PathBuf::from).expect("evidence-unavailable: set PDX_NATIVE_PRIVATE_EVIDENCE to the prepared private replay root; see docs/native/retrieval.md");
    let cases = [
        ("normal", Activation::Demonstrated, Completion::Complete, 5),
        (
            "missing-hook",
            Activation::NotEstablished,
            Completion::Unavailable,
            3,
        ),
        (
            "incomplete-stream",
            Activation::Demonstrated,
            Completion::Incomplete,
            4,
        ),
        (
            "worker-loss",
            Activation::NotEstablished,
            Completion::WorkerLost,
            3,
        ),
    ];
    for (case, activation, completion, count) in cases {
        let reference = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(format!("tests/fixtures/private/{case}.ref.json"));
        let descriptor: ArtifactReference =
            serde_json::from_slice(&fs::read(reference).unwrap()).unwrap();
        let result = Engine
            .replay(ReplayRequest {
                artifact_root: root.clone(),
                descriptor,
            })
            .unwrap();
        assert_eq!(result.capture_origin, CaptureOrigin::Captured);
        assert_eq!(result.activation, activation, "{case}");
        assert_eq!(result.completion, completion, "{case}");
        assert_eq!(result.disposal, Disposal::Confirmed, "{case}");
        assert_eq!(result.observations.len(), count, "{case}");
        assert_eq!(
            result
                .gaps
                .iter()
                .any(|gap| matches!(gap, Gap::OriginalProfileInputUnavailable { .. })),
            case != "worker-loss"
        );
    }
}
