//! Development harness, not a distributed Native executable.
use pdx_native::investigation::{self, CandidateRequest};
use std::{
    io,
    path::PathBuf,
    process::{Command, Stdio},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--supervisor") {
        investigation::serve(io::stdin(), io::stdout())?;
        return Ok(());
    }
    if args.len() < 2 || args.len() > 3 {
        return Err("usage: investigate INSTALLATION NEW_ABSOLUTE_OUTPUT [normal|long-hold|cancel|caller-loss|worker-loss|worker-loss-before-launch|timeout]".into());
    }
    let scenario = args.get(2).and_then(|v| v.to_str()).unwrap_or("normal");
    if ![
        "normal",
        "long-hold",
        "cancel",
        "caller-loss",
        "worker-loss",
        "worker-loss-before-launch",
        "timeout",
    ]
    .contains(&scenario)
    {
        return Err("unknown scenario".into());
    }
    let request = CandidateRequest {
        installation_hint: PathBuf::from(&args[0]),
        output: PathBuf::from(&args[1]),
        hold_ms: match scenario {
            "normal" => 250,
            "long-hold" => 29_999,
            _ => 30_000,
        },
    };
    let plan = investigation::prepare(request)?;
    let mut child = Command::new(std::env::current_exe()?)
        .arg("--supervisor")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut job = investigation::connect(
        child.stdout.take().unwrap(),
        child.stdin.take().unwrap(),
        plan,
    )?;
    if scenario == "worker-loss-before-launch" {
        job.worker_lost()?;
    }
    let Some((attempt, game)) = job.started()? else {
        println!("{}", serde_json::to_string_pretty(&job.finish()?)?);
        child.wait()?;
        return Ok(());
    };
    eprintln!("attempt={attempt} owner={} game={game}", child.id());
    match scenario {
        "cancel" => job.cancel()?,
        "worker-loss" => job.worker_lost()?,
        "caller-loss" => std::process::exit(0),
        _ => {}
    }
    println!("{}", serde_json::to_string_pretty(&job.finish()?)?);
    if !child.wait()?.success() {
        return Err("supervisor failed".into());
    }
    Ok(())
}
