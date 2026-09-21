//! Observe a category fixture prepared before launch.
//!
//! usage: observe-fixture <installation-or-recordings> <category.txt>
//! `RECORDED=1` reads recorded answers; `RECORD_ANSWERS_TO` saves live answers.
use pdx_native::{FixtureRequest, GameOptions, Native};
use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--supervisor") {
        pdx_native::supervisor::serve(std::io::stdin(), std::io::stdout())?;
        return Ok(());
    }
    let [installation, file] = args.as_slice() else {
        return Err("usage: observe-fixture <installation-or-recordings> <category.txt>".into());
    };
    let fixture = FixtureRequest::new(
        "common/tradition_categories/atlas.txt",
        std::fs::read_to_string(file)?,
    );
    let native = if std::env::var_os("RECORDED").is_some() {
        Native::from_recorded_answers(installation)?
    } else {
        Native::open(installation)?
    };
    let native = match std::env::var_os("RECORD_ANSWERS_TO") {
        Some(directory) => native.record_answers_to(directory),
        None => native,
    };
    let mut supervisor = Command::new(std::env::current_exe()?);
    supervisor.arg("--supervisor");
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let mut game = native
                .start_game(GameOptions::new(supervisor).fixture(fixture))
                .await?;
            let answer = game.observe_fixture().await;
            let disposal = game.close().await?;
            eprintln!("disposal: {disposal:?}");
            println!("{}", serde_json::to_string_pretty(&answer?)?);
            Ok(())
        })
}
