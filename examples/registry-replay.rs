//! Replay a registry snapshot without a game installation.
use pdx_native::{Engine, ReplayRequest};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: registry-replay ARTIFACT_ROOT DESCRIPTOR_REFERENCE".into());
    }
    let result = Engine.replay_registry(ReplayRequest {
        artifact_root: args[0].clone().into(),
        descriptor: serde_json::from_slice(&std::fs::read(&args[1])?)?,
    })?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
