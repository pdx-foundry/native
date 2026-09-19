//! Production registry consumer: configure hosting once, then ask an engine question.
use pdx_native::{Engine, OpenRequest, RegistryOptions, supervisor};
use std::{io, path::PathBuf, process::Command};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--supervisor") {
        supervisor::serve(io::stdin(), io::stdout())?;
        return Ok(());
    }
    if !(3..=4).contains(&args.len()) {
        return Err("usage: live INSTALLATION EXISTING_RETENTION_DIRECTORY REGISTRY [normal|cancel|caller-loss|timeout]".into());
    }
    let mode = args.get(3).map(String::as_str).unwrap_or("normal");
    if !["normal", "cancel", "caller-loss", "timeout"].contains(&mode) {
        return Err("unknown mode".into());
    }
    let context = Engine::open(OpenRequest {
        installation_hint: PathBuf::from(&args[0]),
    })?;
    let mut host = Command::new(std::env::current_exe()?);
    host.arg("--supervisor");
    let mut native = context.with_supervisor(
        host,
        RegistryOptions {
            retention_directory: PathBuf::from(&args[1]),
            deadline_seconds: if mode == "timeout" { Some(1) } else { None },
        },
    )?;
    eprintln!("capability: {:?}", native.capability(&args[2]));
    let report = if mode == "normal" {
        native.get_registry_items(&args[2])?
    } else {
        let mut job = native.start_registry_items(&args[2])?;
        if job.started()? {
            if mode == "cancel" {
                job.cancel()?;
            }
            if mode == "caller-loss" {
                std::process::exit(0);
            }
        }
        job.finish()?
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    if let Some(replay) = report.replay {
        let retained = Engine.replay_registry(replay)?;
        eprintln!(
            "replay: {} entries, {:?}, {:?}",
            retained.registered_items.len(),
            retained.completion,
            retained.disposal
        );
    }
    Ok(())
}
