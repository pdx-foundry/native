//! Analyze a discovered candidate ordinal and retain the inputs for independent replay.
use pdx_native::{Native, OpenRequest};
use std::{fs, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 3 {
        return Err(
            "usage: analyze-subject <executable> <candidate-ordinal> <new-output-directory>".into(),
        );
    }
    let ordinal: usize = args[1].to_str().ok_or("invalid ordinal")?.parse()?;
    let native = Native::open(OpenRequest {
        installation_hint: args[0].clone().into(),
    })?;
    let analysis = native.analysis()?;
    let discovery = analysis.discover_registries()?;
    let candidate = discovery
        .candidates
        .get(ordinal)
        .ok_or("candidate ordinal outside this discovery result")?;
    let result = analysis.analyze_subject(&discovery, &candidate.subject)?;
    let output = PathBuf::from(&args[2]);
    fs::create_dir(&output)?;
    fs::create_dir(output.join("registry-fields"))?;
    fs::write(
        output.join(&result.descriptor.input.path),
        result.input_bytes(),
    )?;
    let descriptor = serde_json::to_vec_pretty(&result.descriptor)?;
    fs::write(output.join("descriptor.json"), &descriptor)?;
    use sha2::{Digest, Sha256};
    let reference = pdx_native::ArtifactReference {
        path: "descriptor.json".into(),
        bytes: descriptor.len() as u64,
        sha256: format!("{:x}", Sha256::digest(&descriptor)),
    };
    fs::write(
        output.join("descriptor.ref.json"),
        serde_json::to_vec_pretty(&reference)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
