//! Minimal Atlas-style caller; accepts only a storage root and a retained descriptor reference.
use pdx_native::internals::legacy::{ArtifactReference, Engine, ReplayRequest};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let artifact_root = args
        .next()
        .ok_or("usage: replay <artifact-root> <reference.json>")?;
    let reference_path = args.next().ok_or("missing descriptor reference")?;
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    let descriptor: ArtifactReference = serde_json::from_slice(&std::fs::read(reference_path)?)?;
    let result = Engine.replay(ReplayRequest {
        artifact_root: artifact_root.into(),
        descriptor,
    })?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
