//! Report the initial-loader item result for every statically discovered registry.
//! usage: registry-items-report <installation> [--batch N] [--limit M] [registry ...]
use pdx_native::{Completeness, Disposal, Error, GameOptions, Native};
use std::{collections::BTreeMap, process::Command, time::Instant};

struct Options {
    installation: String,
    batch_size: usize,
    limit: usize,
    requested: Vec<String>,
}

fn parse_options(args: &[String]) -> Result<Options, Box<dyn std::error::Error>> {
    let installation = args.first().ok_or(
        "usage: registry-items-report <installation> [--batch N] [--limit M] [registry ...]",
    )?;
    let mut batch_size = 16usize;
    let mut limit = usize::MAX;
    let mut requested = Vec::new();
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--batch" | "--limit" => {
                let value: usize = args.get(index + 1).ok_or("missing option value")?.parse()?;
                if value == 0 || value > 164 {
                    return Err("batch and limit must be between 1 and 164".into());
                }
                if args[index] == "--batch" {
                    batch_size = value;
                } else {
                    limit = value;
                }
                index += 2;
            }
            name if name.starts_with('-') => return Err(format!("unknown option: {name}").into()),
            name => {
                requested.push(name.to_owned());
                index += 1;
            }
        }
    }
    Ok(Options {
        installation: installation.clone(),
        batch_size,
        limit,
        requested,
    })
}

fn selected_names(
    names: &[String],
    options: &Options,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    for name in &options.requested {
        if !names.contains(name) {
            return Err(format!("unknown registry: {name}").into());
        }
    }
    Ok(names
        .iter()
        .filter(|name| options.requested.is_empty() || options.requested.contains(name))
        .take(options.limit)
        .cloned()
        .collect())
}

fn observe_batches(
    native: &Native,
    selected: &[String],
    batch_size: usize,
) -> Result<BTreeMap<String, String>, Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let mut results = BTreeMap::new();
    for batch in selected.chunks(batch_size) {
        let mut supervisor = Command::new(std::env::current_exe()?);
        supervisor.arg("--supervisor");
        let mut options = GameOptions::new(supervisor).registries(batch.to_vec());
        options.startup_seconds = 120;
        match runtime.block_on(native.start_game(options)) {
            Ok(mut game) => {
                for name in batch {
                    let result = runtime.block_on(game.registry_items(name));
                    let status = match result {
                        Ok(answer) if answer.completeness == Completeness::Complete => {
                            format!("complete {}", answer.value.len())
                        }
                        Ok(answer) => format!("partial {} {:?}", answer.value.len(), answer.gaps),
                        Err(Error::Unsupported { reason, .. }) => format!("unsupported {reason}"),
                        Err(error) => format!("error {error}"),
                    };
                    results.insert(name.clone(), status);
                }
                let disposal = runtime.block_on(game.close())?;
                if disposal != Disposal::Confirmed {
                    return Err(format!("batch close was not confirmed: {disposal:?}").into());
                }
            }
            Err(error) => {
                eprintln!("batch starting with {} did not start: {error}", batch[0]);
                break;
            }
        }
    }
    Ok(results)
}

fn print_report(names: &[String], results: &BTreeMap<String, String>, elapsed: u64) {
    let mut totals = BTreeMap::<&str, usize>::new();
    for name in names {
        let status = results
            .get(name)
            .map(String::as_str)
            .unwrap_or("not attempted");
        println!("{name}\t{status}");
        let category = if status == "not attempted" {
            "not attempted"
        } else {
            status.split_whitespace().next().unwrap_or("error")
        };
        *totals.entry(category).or_default() += 1;
    }
    println!("totals: {totals:?}; elapsed: {elapsed} s");
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--supervisor") {
        pdx_native::supervisor::serve(std::io::stdin(), std::io::stdout())?;
        return Ok(());
    }
    let options = parse_options(&args)?;
    let native = Native::open(&options.installation)?;
    let names: Vec<_> = native
        .registries()?
        .value
        .into_iter()
        .map(|registry| registry.name)
        .collect();
    let selected = selected_names(&names, &options)?;
    let native = match std::env::var_os("RECORD_ANSWERS_TO") {
        Some(directory) => native.record_answers_to(directory),
        None => native,
    };
    let started = Instant::now();
    let results = observe_batches(&native, &selected, options.batch_size)?;
    print_report(&names, &results, started.elapsed().as_secs());
    Ok(())
}
