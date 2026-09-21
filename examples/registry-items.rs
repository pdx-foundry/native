//! List the item names of registries from a supervised game. The game starts, pauses after its
//! registries load, and is closed at the end.
//!
//! usage: registry-items <installation> [registry ...]
//!
//! Set `RECORD_ANSWERS_TO` to a directory to write each answer there. Give that directory as
//! `<installation>` with `RECORDED=1` to read the answers back with no game.
use pdx_native::{GameOptions, Native};
use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // The consumer supplies the supervisor process: this executable, in a dedicated role.
    if args.first().is_some_and(|arg| arg == "--supervisor") {
        pdx_native::supervisor::serve(std::io::stdin(), std::io::stdout())?;
        return Ok(());
    }
    let [installation, registries @ ..] = args.as_slice() else {
        return Err("usage: registry-items <installation> [registry ...]".into());
    };
    let mut supervisor = Command::new(std::env::current_exe()?);
    supervisor.arg("--supervisor");
    let native = if std::env::var_os("RECORDED").is_some() {
        Native::from_recorded_answers(installation)?
    } else {
        Native::open(installation)?
    };
    let native = match std::env::var_os("RECORD_ANSWERS_TO") {
        Some(directory) => native.record_answers_to(directory),
        None => native,
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let default = ["common/traditions".to_owned()];
        let registries = if registries.is_empty() {
            &default[..]
        } else {
            registries
        };
        let mut game = native
            .start_game(GameOptions::new(supervisor).registries(registries.to_vec()))
            .await?;
        for registry in registries {
            match game.registry_items(registry).await {
                Ok(answer) => println!("{}", serde_json::to_string_pretty(&answer)?),
                Err(error) => eprintln!("{registry}: {error}"),
            }
        }
        // Always close, including when a question had no answer.
        let disposal = game.close().await?;
        eprintln!("disposal: {disposal:?}");
        Ok(())
    })
}
