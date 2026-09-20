//! Static discovery without a game launch. The output directory retains private replay artifacts.
use pdx_native::{CapabilityRequest, Native, OpenRequest};
use std::{fs, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: discover-registries <executable> <new-output-directory>".into());
    }
    let native = Native::open(OpenRequest {
        installation_hint: args[0].clone().into(),
    })?;
    let capability = native.capability(&CapabilityRequest::RegistryDiscovery);
    eprintln!("{}", serde_json::to_string_pretty(&capability)?);
    let result = native.analysis()?.discover_registries()?;
    let output = PathBuf::from(&args[1]);
    fs::create_dir(&output)?;
    let input = output.join(&result.descriptor.input.path);
    fs::create_dir_all(input.parent().ok_or("missing input parent")?)?;
    fs::write(input, result.input_bytes())?;
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
