//! Replay root-field discovery without an installation, debugger, or game process.
use pdx_native::{Engine, ReplayRequest};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: replay-fields <artifact-root> <descriptor-reference-json>".into());
    }
    let descriptor = serde_json::from_slice(&std::fs::read(&args[1])?)?;
    let result = Engine.replay_registry_fields(ReplayRequest {
        artifact_root: args[0].clone().into(),
        descriptor,
    })?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
