//! Minimal production consumer. Native owns all game-specific launch and observation choices.
use pdx_native::{
    Availability, CapabilityRequest, CaptureOptions, Engine, ObservationRequest, OpenRequest,
    supervisor,
};
use std::{
    io,
    path::PathBuf,
    process::{Command, Stdio},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--supervisor") {
        supervisor::serve(io::stdin(), io::stdout())?;
        return Ok(());
    }
    if !(3..=4).contains(&args.len()) {
        return Err("usage: live INSTALLATION NEW_ABSOLUTE_OUTPUT FIXTURE [normal|cancel|caller-loss|timeout]".into());
    }
    let mode = args.get(3).map(String::as_str).unwrap_or("normal");
    if !["normal", "cancel", "caller-loss", "timeout"].contains(&mode) {
        return Err("unknown consumer mode".into());
    }
    let context = Engine::open(OpenRequest {
        installation_hint: PathBuf::from(&args[0]),
    })?;
    let capability = context.capability(&CapabilityRequest::default());
    if capability.availability != Availability::Available {
        return Err(format!("Observation unavailable: {:?}", capability.reasons).into());
    }
    let plan = context.prepare_observation(
        ObservationRequest {
            fixture: std::fs::read_to_string(&args[2])?,
            deadline_seconds: if mode == "timeout" { 1 } else { 180 },
        },
        CaptureOptions {
            output: PathBuf::from(&args[1]),
        },
    )?;
    let mut owner = Command::new(std::env::current_exe()?)
        .arg("--supervisor")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut job = supervisor::connect(
        owner.stdout.take().unwrap(),
        owner.stdin.take().unwrap(),
        plan,
    )?;
    if job.started()? {
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
    if let Some(replay) = report.replay {
        let retained = Engine.replay(replay)?;
        eprintln!(
            "replay: {:?}, {:?}, {:?}",
            retained.activation, retained.completion, retained.disposal
        );
    }
    Ok(())
}
