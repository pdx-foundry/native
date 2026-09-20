//! Live tests: each case starts the real game through the public API and checks the answers
//! and the cleanup.
//!
//! The cases are ignored by default. To run them, name the installation and pass `--ignored`:
//!
//! ```text
//! STELLARIS_PATH=/path/to/Stellaris cargo test --release --test live -- --ignored
//! STELLARIS_PATH=/path/to/Stellaris cargo test --release --test live -- --ignored missing_hook
//! ```
//!
//! A word after `--ignored` selects the cases whose name contains it. A case takes about 35
//! seconds; the full set takes about ten minutes.
//!
//! This file has its own `main` (`harness = false` in `Cargo.toml`) for two reasons. The
//! supervisor is this executable with the `--supervisor` argument, and the standard harness
//! writes to the standard output that the supervisor protocol owns. Only one Native-owned game
//! may run on a host, so the cases run one at a time.
//!
//! The fault cases use the hidden `GameOptions::fault`. A fault applies to one registry; the
//! other registry must stay complete. The tests never stop a process. They only check that every
//! game and supervisor process that a case started is gone when the case ends.
use pdx_native::internals::ObservationControl as Fault;
use pdx_native::{
    Answer, Basis, Completeness, Disposal, Error, Game, GameOptions, GameReadiness, GapKind, Native,
};
use std::{
    collections::BTreeSet,
    process::Command,
    time::{Duration, Instant},
};

const TRADITIONS: &str = "common/traditions";
const CATEGORIES: &str = "common/tradition_categories";
/// Item counts of the catalogued M45 build.
const ITEM_COUNTS: [(&str, usize); 2] = [(TRADITIONS, 234), (CATEGORIES, 33)];

type Outcome = Result<(), Box<dyn std::error::Error>>;

/// What the registry that receives a fault must give.
#[derive(Clone, Copy)]
enum Expect {
    /// An `Error::Observation`: the items could not be observed.
    NoAnswer,
    /// A `Partial` answer with an `IncompleteObservation` gap.
    PartialAnswer,
}

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    // The supervisor role. Nothing else may write to the standard output in this role.
    if arguments
        .first()
        .is_some_and(|argument| argument == "--supervisor")
    {
        if let Err(error) = pdx_native::supervisor::serve(std::io::stdin(), std::io::stdout()) {
            eprintln!("supervisor: {error}");
            std::process::exit(1);
        }
        return;
    }
    let filter = arguments.iter().find(|argument| !argument.starts_with('-'));
    let cases: Vec<_> = cases()
        .into_iter()
        .filter(|(name, _)| filter.is_none_or(|filter| name.contains(filter.as_str())))
        .collect();
    if arguments.iter().any(|argument| argument == "--list") {
        for (name, _) in &cases {
            println!("{name}: test");
        }
        return;
    }
    if !arguments.iter().any(|argument| argument == "--ignored") {
        println!(
            "{} live cases ignored: they require STELLARIS_PATH and `--ignored`",
            cases.len()
        );
        return;
    }
    let installation = std::env::var_os("STELLARIS_PATH")
        .expect("STELLARIS_PATH names the Stellaris installation");
    let native = Native::open(installation).expect("the installed build is in the catalogue");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let mut failed = Vec::new();
    println!("running {} live cases, one at a time", cases.len());
    for (name, case) in cases {
        let before = game_processes();
        if !before.is_empty() {
            // Never start a second game, and never touch a game that is not ours.
            println!("test {name} ... not run: Stellaris already runs {before:?}");
            failed.push(name);
            break;
        }
        let started = Instant::now();
        let result = runtime
            .block_on(run(&native, &case))
            .and_then(|()| processes_are_gone(&before))
            .and_then(|()| remove_work_directories());
        let seconds = started.elapsed().as_secs();
        match result {
            Ok(()) => println!("test {name} ... ok ({seconds} s)"),
            Err(error) => {
                println!("test {name} ... FAILED ({seconds} s): {error}");
                failed.push(name);
                // A later case cannot start while a process of this one remains.
                if processes_are_gone(&before).is_err() {
                    break;
                }
            }
        }
    }
    if !failed.is_empty() {
        println!("failed: {failed:?}");
        std::process::exit(1);
    }
}

enum Case {
    Normal,
    StartupTimeout,
    Cancel,
    DropWithoutClose,
    Fault {
        registry: &'static str,
        other: &'static str,
        control: Fault,
        expect: Expect,
    },
    WorkerLoss {
        registry: &'static str,
    },
}

fn cases() -> Vec<(String, Case)> {
    let mut cases = vec![
        ("normal".to_owned(), Case::Normal),
        ("startup_timeout".to_owned(), Case::StartupTimeout),
        ("cancel".to_owned(), Case::Cancel),
        ("drop_without_close".to_owned(), Case::DropWithoutClose),
    ];
    let faults = [
        ("missing_hook", Fault::MissingHook, Expect::NoAnswer),
        ("late_hook", Fault::LateHook, Expect::NoAnswer),
        ("access_failure", Fault::AccessFailure, Expect::NoAnswer),
        (
            "dropped_record",
            Fault::DroppedRecord,
            Expect::PartialAnswer,
        ),
        (
            "missing_terminal",
            Fault::MissingTerminal,
            Expect::PartialAnswer,
        ),
    ];
    for (registry, other) in [(TRADITIONS, CATEGORIES), (CATEGORIES, TRADITIONS)] {
        let short = registry.trim_start_matches("common/");
        for (name, control, expect) in faults {
            let case = Case::Fault {
                registry,
                other,
                control,
                expect,
            };
            cases.push((format!("{name}_in_{short}"), case));
        }
        cases.push((
            format!("worker_loss_in_{short}"),
            Case::WorkerLoss { registry },
        ));
    }
    cases
}

async fn run(native: &Native, case: &Case) -> Outcome {
    match *case {
        Case::Normal => normal(native).await,
        Case::StartupTimeout => startup_timeout(native).await,
        Case::Cancel => cancel(native).await,
        Case::DropWithoutClose => drop_without_close(native).await,
        Case::Fault {
            registry,
            other,
            control,
            expect,
        } => fault(native, registry, other, control, expect).await,
        Case::WorkerLoss { registry } => worker_loss(native, registry).await,
    }
}

fn options() -> GameOptions {
    let mut supervisor = Command::new(std::env::current_exe().expect("test executable path"));
    supervisor.arg("--supervisor");
    GameOptions::new(supervisor)
}

/// The item count of a complete live answer.
fn complete(answer: &Answer<Vec<String>>, registry: &str) -> Result<usize, String> {
    if answer.completeness != Completeness::Complete || !answer.gaps.is_empty() {
        return Err(format!(
            "{registry}: expected a complete answer: {:?}",
            answer.gaps
        ));
    }
    if answer.source.basis != Basis::LiveObservation {
        return Err(format!("{registry}: basis {:?}", answer.source.basis));
    }
    Ok(answer.value.len())
}

async fn close_confirmed(game: &mut Game) -> Outcome {
    let disposal = game.close().await?;
    if disposal != Disposal::Confirmed {
        return Err(format!("disposal: {disposal:?}").into());
    }
    // A repeated close gives the same result, and a closed game answers nothing.
    if game.close().await? != Disposal::Confirmed {
        return Err("the second close gave another disposal".into());
    }
    match game.registry_items(TRADITIONS).await {
        Err(Error::Closed) => Ok(()),
        other => Err(format!("after close: {other:?}").into()),
    }
}

async fn normal(native: &Native) -> Outcome {
    let mut game = native.start_game(options()).await?;
    let readiness = game.readiness();
    let mut result = async {
        if readiness != GameReadiness::PausedAfterRegistryInitialization {
            return Err(format!("readiness: {readiness:?}").into());
        }
        // Both orders, so that each registry is read after the other.
        for (registry, count) in ITEM_COUNTS.into_iter().chain(ITEM_COUNTS.into_iter().rev()) {
            let first = game.registry_items(registry).await?;
            if complete(&first, registry)? != count {
                return Err(format!("{registry}: {} items", first.value.len()).into());
            }
            if game.registry_items(registry).await? != first {
                return Err(format!("{registry}: a repeated read gave another answer").into());
            }
        }
        match game.registry_items("common/agendas").await {
            Err(Error::Unsupported { .. }) => Ok(()),
            other => Err(format!("a registry outside the live recipe: {other:?}").into()),
        }
    }
    .await;
    and_close(&mut result, &mut game).await;
    result
}

/// Always close, so that a failed check leaves no game. The first failure is the one reported.
async fn and_close(result: &mut Outcome, game: &mut Game) {
    let closed = close_confirmed(game).await;
    if result.is_ok() {
        *result = closed;
    }
}

async fn fault(
    native: &Native,
    registry: &'static str,
    other: &'static str,
    control: Fault,
    expect: Expect,
) -> Outcome {
    let mut game = native
        .start_game(options().fault(registry, control))
        .await?;
    let readiness = game.readiness();
    let mut result = async {
        // A hook fault stops the game before the faulted registry returns from its load.
        let expected_readiness = match control {
            Fault::MissingHook | Fault::LateHook => {
                GameReadiness::PausedDuringRegistryInitialization
            }
            _ => GameReadiness::PausedAfterRegistryInitialization,
        };
        if readiness != expected_readiness {
            return Err(format!("readiness: {readiness:?}").into());
        }
        // Both orders: a failed read must not change the other registry's answer.
        for name in [registry, other, other, registry] {
            let answer = game.registry_items(name).await;
            if name == other {
                let answer = answer?;
                if complete(&answer, name)? == 0 {
                    return Err(format!("{name}: no items").into());
                }
                continue;
            }
            match (expect, answer) {
                (Expect::NoAnswer, Err(Error::Observation { .. })) => {}
                (Expect::PartialAnswer, Ok(answer))
                    if answer.completeness == Completeness::Partial
                        && answer
                            .gaps
                            .iter()
                            .any(|gap| gap.kind == GapKind::IncompleteObservation) => {}
                (_, answer) => return Err(format!("{name} with {control:?}: {answer:?}").into()),
            }
        }
        Ok(())
    }
    .await;
    and_close(&mut result, &mut game).await;
    result
}

/// The supervisor stops the debugger worker while the faulted registry loads. The game never
/// reaches its pause, so there is no `Game`; the start fails and the game is still reaped.
async fn worker_loss(native: &Native, registry: &'static str) -> Outcome {
    match native
        .start_game(options().fault(registry, Fault::WorkerLoss))
        .await
    {
        Err(Error::Startup {
            disposal: Disposal::Confirmed,
            reason,
        }) if reason.contains("WorkerLost") => Ok(()),
        Err(error) => Err(format!("expected a worker-loss startup error: {error:?}").into()),
        Ok(mut game) => {
            let _ = game.close().await;
            Err("the game started although its worker was lost".into())
        }
    }
}

async fn startup_timeout(native: &Native) -> Outcome {
    let mut options = options();
    options.startup_seconds = 1;
    match native.start_game(options).await {
        Err(Error::Startup {
            disposal: Disposal::Confirmed,
            reason,
        }) if reason.contains("TimedOut") => Ok(()),
        Err(error) => Err(format!("expected a startup timeout: {error:?}").into()),
        Ok(mut game) => {
            let _ = game.close().await;
            Err("the game started within one second".into())
        }
    }
}

async fn cancel(native: &Native) -> Outcome {
    let mut game = native.start_game(options()).await?;
    game.cancel();
    let mut result = match game.registry_items(TRADITIONS).await {
        Err(Error::Closed) => Ok(()),
        other => Err(format!("after cancel: {other:?}").into()),
    };
    and_close(&mut result, &mut game).await;
    result
}

/// The caller forgets to close. The supervisor sees its control input end and reaps the game.
async fn drop_without_close(native: &Native) -> Outcome {
    let game = native.start_game(options()).await?;
    if game_processes().is_empty() {
        return Err("no game process after start".into());
    }
    drop(game);
    Ok(())
}

/// Wait until every game process that started after `before`, and every child of this process,
/// is gone. This only looks; it never signals a process.
fn processes_are_gone(before: &BTreeSet<u32>) -> Outcome {
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let games: Vec<_> = game_processes().difference(before).copied().collect();
        let children = child_processes();
        if games.is_empty() && children.is_empty() {
            break;
        }
        if Instant::now() >= deadline {
            return Err(format!("still running: games {games:?}, children {children:?}").into());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Ok(())
}

/// Native removes its work directory after a confirmed `close`, and keeps it after a failed
/// start or a drop. A case that passed needs no inspection, so remove what it left.
fn remove_work_directories() -> Outcome {
    let prefix = format!("pdx-native-{}-", std::process::id());
    for entry in std::fs::read_dir(std::env::temp_dir())?.flatten() {
        if entry.file_name().to_string_lossy().starts_with(&prefix) {
            std::fs::remove_dir_all(entry.path())?;
        }
    }
    Ok(())
}

/// Every process on the host with `(pid, parent pid, executable path)`.
fn processes() -> Vec<(u32, u32, String)> {
    let output = Command::new("ps")
        .args(["-axo", "pid=,ppid=,comm="])
        .output()
        .expect("ps");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let (pid, rest) = line.trim_start().split_once(char::is_whitespace)?;
            let (parent, command) = rest.trim_start().split_once(char::is_whitespace)?;
            Some((
                pid.parse().ok()?,
                parent.parse().ok()?,
                command.trim().to_owned(),
            ))
        })
        .collect()
}

/// Processes whose executable is the game.
fn game_processes() -> BTreeSet<u32> {
    processes()
        .into_iter()
        .filter(|(_, _, command)| {
            std::path::Path::new(command)
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case("stellaris"))
        })
        .map(|(pid, _, _)| pid)
        .collect()
}

/// Children of this process: the supervisors. `ps` itself has exited when its output is read.
fn child_processes() -> Vec<u32> {
    let this = std::process::id();
    processes()
        .into_iter()
        .filter(|(_, parent, command)| *parent == this && !command.ends_with("ps"))
        .map(|(pid, _, _)| pid)
        .collect()
}
