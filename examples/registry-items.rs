//! List the item names of registries from a supervised game. The game starts, pauses after its
//! registries load, and is closed at the end.
//!
//! usage: registry-items <installation> <existing-work-directory> [registry ...]
//!
//! Set `RECORD_ANSWERS_TO` to a directory to write each answer there. Give that directory as
//! `<installation>` with `RECORDED=1` to read the answers back with no game.
use pdx_native::{GameOptions, Native, OpenRequest};
use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // The consumer supplies the supervisor process: this executable, in a dedicated role.
    if args.first().is_some_and(|arg| arg == "--supervisor") {
        pdx_native::supervisor::serve(std::io::stdin(), std::io::stdout())?;
        return Ok(());
    }
    let [installation, work, registries @ ..] = args.as_slice() else {
        return Err("usage: registry-items <installation> <work-directory> [registry ...]".into());
    };
    let mut supervisor = Command::new(std::env::current_exe()?);
    supervisor.arg("--supervisor");
    let native = if std::env::var_os("RECORDED").is_some() {
        Native::from_recorded_answers(installation)
    } else {
        Native::open(OpenRequest {
            installation_hint: installation.into(),
        })?
    };
    let native = match std::env::var_os("RECORD_ANSWERS_TO") {
        Some(directory) => native.record_answers_to(directory),
        None => native,
    };
    let native = native.with_supervisor(supervisor, GameOptions::new(work.into()))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let mut game = native.start_game().await?;
        let default = ["common/traditions".to_owned()];
        let registries = if registries.is_empty() {
            &default[..]
        } else {
            registries
        };
        for registry in registries {
            match game.registry_items(registry).await {
                Ok(answer) => println!("{}", serde_json::to_string_pretty(&answer)?),
                Err(error) => eprintln!("{registry}: {error}"),
            }
        }
        // Always close, including when a question had no answer.
        let report = game.close().await?;
        eprintln!("disposal: {:?}", report.disposal);
        Ok(())
    })
}
