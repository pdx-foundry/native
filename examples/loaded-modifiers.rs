//! Report the loaded modifier inventory of a supervised game: how many loaded modifiers the
//! executable declares, how many a family explains, and how many stay unexplained, with each
//! family's match rate over the registry's loaded items. The game starts, pauses after all
//! content loads, and is closed at the end.
//!
//! usage: loaded-modifiers <installation>
//!
//! Set `RECORD_ANSWERS_TO` to a directory to write each answer there. Give that directory as
//! `<installation>` with `RECORDED=1` to read the answers back with no game.
use pdx_native::{GameOptions, NamePart, Native};
use std::collections::BTreeSet;
use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // The consumer supplies the supervisor process: this executable, in a dedicated role.
    if args.first().is_some_and(|arg| arg == "--supervisor") {
        pdx_native::supervisor::serve(std::io::stdin(), std::io::stdout())?;
        return Ok(());
    }
    let [installation] = args.as_slice() else {
        return Err("usage: loaded-modifiers <installation>".into());
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
        let mut game = native
            .start_game(GameOptions::new(supervisor).loaded_modifiers())
            .await?;
        let answer = game.loaded_modifiers().await;
        // Always close, including when the question had no answer.
        let disposal = game.close().await?;
        eprintln!("disposal: {disposal:?}");
        let answer = answer?;

        let loaded = &answer.value.modifiers;
        let names: BTreeSet<&str> = loaded
            .iter()
            .map(|modifier| modifier.name.as_str())
            .collect();
        let declared = loaded.iter().filter(|modifier| modifier.declared).count();
        let generated = loaded
            .iter()
            .filter(|modifier| !modifier.generated_by.is_empty())
            .count();
        let unexplained = loaded
            .iter()
            .filter(|modifier| !modifier.declared && modifier.generated_by.is_empty())
            .count();
        println!("content: {:?}", answer.value.content);
        println!(
            "loaded {}: declared {declared}, generated {generated}, unexplained {unexplained}",
            loaded.len()
        );
        println!("registry\ttemplate\titems\tnames\tloaded");
        for (registry, items) in &answer.value.registry_items {
            for family in native.modifier_families(registry)?.value {
                let template: String = family
                    .name
                    .iter()
                    .map(|part| match part {
                        NamePart::Literal(text) => text.as_str(),
                        _ => "{key}",
                    })
                    .collect();
                let applied: Vec<String> = items
                    .iter()
                    .filter_map(|item| family.name_for(item))
                    .collect();
                let present = applied
                    .iter()
                    .filter(|name| names.contains(name.as_str()))
                    .count();
                println!(
                    "{registry}\t{template}\t{}\t{}\t{present}",
                    items.len(),
                    applied.len()
                );
            }
        }
        for gap in &answer.gaps {
            println!("gap: {:?} {:?}: {}", gap.kind, gap.subject, gap.detail);
        }
        Ok(())
    })
}
