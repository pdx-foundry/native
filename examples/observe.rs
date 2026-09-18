//! Consumer-hosted candidate capture harness. This command launches a real game.
use pdx_native::investigation::{self, ObservationControl, ObservationRequest};
use std::{
    io,
    path::PathBuf,
    process::{Command, Stdio},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--supervisor") {
        investigation::serve(io::stdin(), io::stdout())?;
        return Ok(());
    }
    if !(3..=4).contains(&args.len()) {
        return Err("usage: observe INSTALLATION NEW_ABSOLUTE_OUTPUT FIXTURE [normal|missing-hook|late-hook|dropped-record|missing-terminal|access-failure|worker-loss|cancel|caller-loss|timeout]".into());
    }
    let scenario = args.get(3).map(String::as_str).unwrap_or("normal");
    let control = match scenario {
        "cancel" | "caller-loss" | "timeout" => ObservationControl::Normal,
        name => serde_json::from_value(serde_json::Value::String(name.into()))?,
    };
    let plan = investigation::prepare_observation_control(
        ObservationRequest {
            installation_hint: PathBuf::from(&args[0]),
            output: PathBuf::from(&args[1]),
            fixture: std::fs::read_to_string(&args[2])?,
            deadline_seconds: if scenario == "timeout" { 1 } else { 180 },
        },
        control,
    )?;
    let mut supervisor = Command::new(std::env::current_exe()?)
        .arg("--supervisor")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut job = investigation::connect(
        supervisor.stdout.take().unwrap(),
        supervisor.stdin.take().unwrap(),
        plan,
    )?;
    if let Some((attempt, game)) = job.started()? {
        eprintln!("attempt={attempt} owner={} game={game}", supervisor.id());
        if scenario == "cancel" {
            job.cancel()?;
        }
        if scenario == "caller-loss" {
            std::process::exit(0);
        }
    }
    println!("{}", serde_json::to_string_pretty(&job.finish()?)?);
    if !supervisor.wait()?.success() {
        return Err("supervisor failed".into());
    }
    Ok(())
}
