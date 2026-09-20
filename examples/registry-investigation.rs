//! Maintainer-only capture of the same registry implementation used by ordinary queries.
use pdx_native::internals::legacy::{Engine, ReplayRequest};
use pdx_native::investigation as candidate;
use std::{
    io,
    path::PathBuf,
    process::{Command, Stdio},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--supervisor") {
        candidate::serve(io::stdin(), io::stdout())?;
        return Ok(());
    }
    if args.len() < 3 {
        return Err(
            "usage: registry-investigation INSTALLATION NEW_OUTPUT REGISTRY [CONTROL]".into(),
        );
    }
    let mode = args.get(3).map(String::as_str).unwrap_or("normal");
    let control = match mode {
        "normal" | "cancel" | "caller-loss" | "timeout" => candidate::ObservationControl::Normal,
        other => serde_json::from_value(serde_json::Value::String(other.into()))?,
    };
    let plan = candidate::prepare_registry(
        candidate::CandidateRequest {
            installation_hint: args[0].clone().into(),
            output: args[1].clone().into(),
            hold_ms: 1,
        },
        args[2].clone(),
        if mode == "timeout" { 1 } else { 180 },
        control,
    )?;
    let mut owner = Command::new(std::env::current_exe()?)
        .arg("--supervisor")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    let mut job = candidate::connect(
        owner.stdout.take().unwrap(),
        owner.stdin.take().unwrap(),
        plan,
    )?;
    if job.started()?.is_some() {
        if mode == "cancel" {
            job.cancel()?;
        }
        if mode == "caller-loss" {
            std::process::exit(0);
        }
    }
    let report = job.finish()?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if !owner.wait()?.success() {
        return Err("supervisor failed".into());
    }
    if let Some(descriptor) = report.replay {
        let retained = Engine.replay_registry(ReplayRequest {
            artifact_root: PathBuf::from(&args[1]).join("evidence"),
            descriptor,
        })?;
        eprintln!(
            "replay: {} entries, {:?}, {:?}",
            retained.registered_items.len(),
            retained.completion,
            retained.disposal
        );
    }
    Ok(())
}
