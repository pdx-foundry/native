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
//! other registry must stay complete. The tests stop only their own unrelated sentinel process. They check that every
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
        let earlier_work = work_directories().expect("work directory inventory");
        let started = Instant::now();
        let isolation =
            Isolation::begin().expect("ordinary profile and unrelated process baseline");
        let outcome = runtime.block_on(run(&native, &case));
        let isolation_result = isolation.finish();
        let cleanup = processes_are_gone(&before);
        let result = outcome
            .and(isolation_result)
            .and(cleanup)
            .and_then(|()| remove_work_directories(&earlier_work));
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
    Fixture(Fault),
    FixtureSelection(pdx_native::FixtureObservationKind),
    FixtureTimeout,
    FixtureRefusal,
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
    for (name, control) in [
        ("normal", Fault::Normal),
        ("missing_hook", Fault::MissingHook),
        ("late_hook", Fault::LateHook),
        ("dropped_record", Fault::DroppedRecord),
        ("missing_terminal", Fault::MissingTerminal),
        ("worker_loss", Fault::WorkerLoss),
        ("access_failure", Fault::AccessFailure),
    ] {
        cases.push((format!("fixture_{name}"), Case::Fixture(control)));
    }
    cases.push((
        "fixture_registration_only".into(),
        Case::FixtureSelection(pdx_native::FixtureObservationKind::RegistrationEntries),
    ));
    cases.push((
        "fixture_field_reads_only".into(),
        Case::FixtureSelection(pdx_native::FixtureObservationKind::CategoryFieldReads),
    ));
    cases.push(("fixture_timeout".into(), Case::FixtureTimeout));
    cases.push(("fixture_refusal".into(), Case::FixtureRefusal));
    cases
}

async fn run(native: &Native, case: &Case) -> Outcome {
    match *case {
        Case::Normal => normal(native).await,
        Case::Fixture(control) => fixture_case(control, None).await,
        Case::FixtureSelection(kind) => fixture_case(Fault::Normal, Some(kind)).await,
        Case::FixtureTimeout => fixture_timeout(native).await,
        Case::FixtureRefusal => fixture_refusal(native).await,
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

fn fixture_request() -> pdx_native::FixtureRequest {
    pdx_native::FixtureRequest::new(
        "common/tradition_categories/atlas.txt",
        "atlas_early_category = {\n tree_template = \"atlas_early_template\"\n traditions = { }\n}\n",
    )
}

async fn fixture_case(
    control: Fault,
    selection: Option<pdx_native::FixtureObservationKind>,
) -> Outcome {
    use pdx_native::{Operation, ProcessingStage, Support};
    let recorded = tempfile::tempdir()?;
    let native = Native::open(std::env::var_os("STELLARIS_PATH").unwrap())?
        .record_answers_to(recorded.path());
    if native.supports(Operation::ObserveFixture) != Support::Supported {
        return Err("fixture operation is not supported".into());
    }
    let mut request = fixture_request();
    if let Some(kind) = selection {
        request.observations = vec![kind];
    }
    let mut prepared = options().fixture(request.clone());
    if control != Fault::Normal {
        prepared = prepared.fixture_fault(control);
    }
    let started = native.start_game(prepared).await;
    if control == Fault::WorkerLoss {
        return match started {
            Err(Error::Startup {
                disposal: Disposal::Confirmed,
                reason,
            }) if reason.contains("WorkerLost") => Ok(()),
            Ok(mut game) => {
                let _ = game.close().await;
                Err("fixture worker loss unexpectedly started a session".into())
            }
            Err(error) => Err(format!("fixture worker loss: {error:?}").into()),
        };
    }
    let mut game = started?;
    let mut result = async {
        let first = game.observe_fixture().await;
        match (&control, &first) {
            (
                Fault::MissingHook | Fault::LateHook,
                Err(Error::Observation {
                    operation: Operation::ObserveFixture,
                    ..
                }),
            ) => {}
            (Fault::DroppedRecord | Fault::MissingTerminal | Fault::AccessFailure, Ok(answer)) => {
                if answer.completeness != Completeness::Partial || answer.gaps.is_empty() {
                    return Err(format!(
                        "{control:?}: expected partial fixture answer: {answer:?}"
                    )
                    .into());
                }
                let expected_reads = if control == Fault::DroppedRecord {
                    1
                } else {
                    2
                };
                if answer.value.registration_entries.len() != 3
                    || answer.value.field_reads.len() != expected_reads
                {
                    return Err(format!("{control:?}: established entries lost: {answer:?}").into());
                }
            }
            (Fault::Normal, Ok(answer)) => {
                let reads = &answer.value.field_reads;
                let expected_registrations: &[u64] = if request
                    .observations
                    .contains(&pdx_native::FixtureObservationKind::RegistrationEntries)
                {
                    &[1, 2, 3]
                } else {
                    &[]
                };
                let expected_reads: &[(&str, u64, &str)] = if request
                    .observations
                    .contains(&pdx_native::FixtureObservationKind::CategoryFieldReads)
                {
                    &[
                        ("common/tradition_categories/atlas.txt", 2, "tree_template"),
                        ("common/tradition_categories/atlas.txt", 3, "traditions"),
                    ]
                } else {
                    &[]
                };
                if answer.completeness != Completeness::Complete
                    || !answer.gaps.is_empty()
                    || answer.source.basis != Basis::LiveObservation
                    || answer
                        .value
                        .registration_entries
                        .iter()
                        .map(|entry| entry.ordinal)
                        .collect::<Vec<_>>()
                        != expected_registrations
                    || reads
                        .iter()
                        .map(|read| (read.file.as_str(), read.line, read.field.as_str()))
                        .collect::<Vec<_>>()
                        != expected_reads
                    || (reads.len() == 2 && reads[0].owner != reads[1].owner)
                    || reads
                        .iter()
                        .any(|read| read.stage != ProcessingStage::FieldReadEntry)
                {
                    return Err(format!("normal fixture: {answer:?}").into());
                }
            }
            (_, answer) => {
                return Err(format!("{control:?}: unexpected fixture result: {answer:?}").into());
            }
        }
        for _ in 0..2 {
            let categories = game.registry_items(CATEGORIES).await?;
            if complete(&categories, CATEGORIES)? != 1
                || categories.value != ["atlas_early_category"]
            {
                return Err(format!("mounted categories: {categories:?}").into());
            }
            if complete(&game.registry_items(TRADITIONS).await?, TRADITIONS)? != 234 {
                return Err("fixture altered the pinned traditions".into());
            }
            if game.observe_fixture().await != first {
                return Err("fixture read changed after registry query".into());
            }
        }
        let recorded_native = Native::from_recorded_answers(recorded.path())?;
        let mut recorded_game = recorded_native
            .start_game(GameOptions::new(Command::new("must-not-start")).fixture(request.clone()))
            .await?;
        let expected = first.map(|mut answer| {
            answer.source.basis = Basis::Recorded;
            answer
        });
        if recorded_game.observe_fixture().await != expected {
            return Err("recorded fixture differs from live answer".into());
        }
        if recorded_game.close().await? != Disposal::NotApplicable {
            return Err("recorded fixture started a game".into());
        }
        let mut changed = request.clone();
        changed.files.values_mut().next().unwrap().push('\n');
        let mut absent = recorded_native
            .start_game(GameOptions::new(Command::new("must-not-start")).fixture(changed))
            .await?;
        if !matches!(
            absent.observe_fixture().await,
            Err(Error::NotRecorded { .. })
        ) {
            return Err("different fixture used another file's answer".into());
        }
        absent.close().await?;
        Ok(())
    }
    .await;
    and_close(&mut result, &mut game).await;
    if result.is_ok() && !matches!(game.observe_fixture().await, Err(Error::Closed)) {
        return Err("closed fixture session still answered".into());
    }
    result
}

async fn fixture_timeout(native: &Native) -> Outcome {
    let mut request = fixture_request();
    request.deadline_seconds = 1;
    match native.start_game(options().fixture(request)).await {
        Err(Error::Startup {
            disposal: Disposal::Confirmed,
            reason,
        }) if reason.contains("TimedOut") => Ok(()),
        Ok(mut game) => {
            let _ = game.close().await;
            Err("fixture deadline was not enforced".into())
        }
        Err(error) => Err(format!("fixture timeout: {error:?}").into()),
    }
}

async fn fixture_refusal(native: &Native) -> Outcome {
    let before = work_directories()?;
    let request = pdx_native::FixtureRequest::new("common/unsupported/fixture.txt", "x = {}");
    match native.start_game(options().fixture(request)).await {
        Err(Error::FixtureRequest { reason }) if reason.contains("common/tradition_categories") => {
        }
        Ok(mut game) => {
            let _ = game.close().await;
            return Err("unsupported fixture launched".into());
        }
        Err(error) => return Err(format!("fixture refusal: {error:?}").into()),
    }
    if work_directories()? != before {
        return Err("refused fixture created session state".into());
    }
    Ok(())
}

/// An unrelated owned child must survive Native's cleanup, and the ordinary game profile must
/// remain byte-for-byte unchanged. The sentinel is reaped before the supervisor-leak check.
struct Isolation {
    sentinel: std::process::Child,
    profile: std::collections::BTreeMap<std::path::PathBuf, String>,
}
impl Isolation {
    fn begin() -> Result<Self, Box<dyn std::error::Error>> {
        let profile = profile_snapshot()?;
        let sentinel = Command::new("/bin/sleep").arg("3600").spawn()?;
        Ok(Self { sentinel, profile })
    }
    fn finish(mut self) -> Outcome {
        if self.sentinel.try_wait()?.is_some() {
            return Err("Native stopped an unrelated process".into());
        }
        if profile_snapshot()? != self.profile {
            return Err("Native changed the ordinary profile".into());
        }
        Ok(())
    }
}
impl Drop for Isolation {
    fn drop(&mut self) {
        let _ = self.sentinel.kill();
        let _ = self.sentinel.wait();
    }
}

fn profile_snapshot()
-> Result<std::collections::BTreeMap<std::path::PathBuf, String>, Box<dyn std::error::Error>> {
    use sha2::{Digest, Sha256};
    use std::{
        collections::BTreeMap,
        fs,
        io::Read,
        path::{Path, PathBuf},
    };
    fn visit(path: &Path, snapshot: &mut BTreeMap<PathBuf, String>) -> std::io::Result<()> {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        let value = if metadata.is_symlink() {
            format!("link:{:?}", fs::read_link(path)?)
        } else if metadata.is_dir() {
            for child in fs::read_dir(path)? {
                visit(&child?.path(), snapshot)?;
            }
            "directory".into()
        } else {
            let mut digest = Sha256::new();
            let mut file = fs::File::open(path)?;
            let mut buffer = [0; 65536];
            loop {
                let read = file.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                digest.update(&buffer[..read]);
            }
            format!("{:x}", digest.finalize())
        };
        snapshot.insert(
            path.into(),
            format!(
                "{value}:{:?}:{:?}",
                metadata.permissions(),
                metadata.modified()?
            ),
        );
        Ok(())
    }
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME is missing")?);
    let mut snapshot = BTreeMap::new();
    for relative in [
        "Documents/Paradox Interactive/Stellaris",
        "Library/Application Support/Paradox Interactive/Stellaris",
    ] {
        visit(&home.join(relative), &mut snapshot)?;
    }
    Ok(snapshot)
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
fn remove_work_directories(earlier: &BTreeSet<std::path::PathBuf>) -> Outcome {
    for path in work_directories()?.difference(earlier) {
        std::fs::remove_dir_all(path)?;
    }
    Ok(())
}

/// Keep a failed case's diagnostics when a later case succeeds.
fn work_directories() -> std::io::Result<BTreeSet<std::path::PathBuf>> {
    let prefix = format!("pdx-native-{}-", std::process::id());
    std::fs::read_dir(std::env::temp_dir())?
        .filter_map(|entry| match entry {
            Ok(entry) if entry.file_name().to_string_lossy().starts_with(&prefix) => {
                Some(Ok(entry.path()))
            }
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .collect()
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
