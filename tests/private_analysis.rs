use pdx_native::{
    AnalysisError, AnalysisOrigin, Availability, CapabilityRequest, Engine, Native, OpenError,
    OpenRequest, Qualification, ReplayRequest, UnavailableReason,
};
use std::{fs, path::PathBuf};

#[path = "analysis_support/mod.rs"]
mod support;

#[test]
#[ignore = "requires PDX_NATIVE_ANALYSIS_EXECUTABLE and PDX_NATIVE_ANALYSIS_OUTPUT with verified control bytes"]
fn qualified_public_decode_and_replay_need_no_content_tools_or_process() {
    let executable = PathBuf::from(
        std::env::var_os("PDX_NATIVE_ANALYSIS_EXECUTABLE").expect("exact M45 executable required"),
    );
    let output = PathBuf::from(
        std::env::var_os("PDX_NATIVE_ANALYSIS_OUTPUT")
            .expect("prepared private evidence directory required"),
    );
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("isolated-executable");
    fs::copy(executable, &path).unwrap();
    let native = Native::open(OpenRequest {
        installation_hint: path.clone(),
    })
    .unwrap();
    let report = native.capability(&CapabilityRequest::StaticDecode);
    assert_eq!(report.qualification, Qualification::Qualified, "{report:?}");
    assert_eq!(report.availability, Availability::Available);
    let context = native.analysis().unwrap();
    let result = context.decode_control().unwrap();
    assert_eq!(result.origin, AnalysisOrigin::Executable);
    assert_eq!(result.instructions.len(), 11);
    let expected: Vec<[String; 2]> = serde_json::from_str(include_str!(
        "fixtures/analysis/planet-getter.expected.json"
    ))
    .unwrap();
    for (index, (instruction, [operation, operands])) in
        result.instructions.iter().zip(expected).enumerate()
    {
        assert_eq!(instruction.address, 0x101156518 + index as u64 * 4);
        assert_eq!(instruction.operation, operation);
        assert_eq!(instruction.operands, operands);
    }
    let descriptor = serde_json::to_vec_pretty(&result.descriptor).unwrap();
    fs::write(output.join("public-descriptor.json"), &descriptor).unwrap();
    let replay = Engine
        .replay_analysis(ReplayRequest {
            artifact_root: output.clone(),
            descriptor: support::reference("public-descriptor.json", &descriptor),
        })
        .unwrap();
    assert_eq!(replay.origin, AnalysisOrigin::Replay);
    assert_eq!(result.instructions, replay.instructions);
    assert_eq!(result.descriptor, replay.descriptor);
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    // The live admission failure must not poison the independently admitted static operation.
    let live = native.capability(&CapabilityRequest::default());
    assert_eq!(live.availability, Availability::Unavailable);
    assert!(live.reasons.contains(&UnavailableReason::InputUnavailable));
    assert_eq!(context.decode_control().unwrap(), result);
    let original = fs::read(&path).unwrap();
    let mut changed = original.clone();
    let last = changed.len() - 1;
    changed[last] ^= 1;
    fs::write(&path, &changed).unwrap();
    assert!(
        matches!(context.decode_control(), Err(AnalysisError::Unavailable { reasons }) if reasons == [UnavailableReason::TargetChanged])
    );
    assert!(matches!(
        Native::open(OpenRequest {
            installation_hint: path.clone()
        }),
        Err(OpenError::UnknownTarget)
    ));
    fs::write(&path, original).unwrap();
    assert!(context.decode_control().is_err());
    assert_eq!(
        native
            .capability(&CapabilityRequest::StaticDecode)
            .availability,
        Availability::Unavailable
    );
    // Rebinding is required after replacement, even when the original bytes have returned.
    let rebound = Native::open(OpenRequest {
        installation_hint: path,
    })
    .unwrap();
    assert_eq!(
        rebound.analysis().unwrap().decode_control().unwrap(),
        result
    );
    fs::write(
        output.join("public-result.json"),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
}
