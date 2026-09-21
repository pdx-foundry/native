# PDX Native

A standard Rust API to ask Stellaris questions, the same on each platform and game build.
Native identifies the exact build, answers from the executable or from a supervised game, and
says how complete each answer is. Atlas is its first consumer.

Native is one Cargo package with no optional features. The
[simplification decision](docs/design/simplification.md) records how it got there.

## Static questions

No game starts. A registry is named by its content directory. See `examples/registries.rs`.

```rust
use pdx_native::Native;

let native = Native::open("/path/to/Stellaris")?;
let registries = native.registries()?;                      // Answer<Vec<Registry>>
let fields = native.registry_fields("common/traditions")?;  // Answer<Vec<Field>>
```

## Live questions

The consumer decides when a game runs. Native starts it, pauses it after its registries load,
and removes it. The consumer supplies the supervisor process: its own executable, started in a
dedicated role that calls `supervisor::serve`. See `examples/registry-items.rs`.

```rust
use pdx_native::{GameOptions, Native};
use std::process::Command;

// In `main`, before anything else: the supervisor role.
if std::env::args().nth(1).as_deref() == Some("--supervisor") {
    pdx_native::supervisor::serve(std::io::stdin(), std::io::stdout())?;
    return Ok(());
}

let mut supervisor = Command::new(std::env::current_exe()?);
supervisor.arg("--supervisor");

let native = Native::open("/path/to/Stellaris")?;
let mut game = native.start_game(GameOptions::new(supervisor)).await?;
let items = game.registry_items("common/traditions").await?;  // Answer<Vec<String>>
let disposal = game.close().await?;                           // Disposal::Confirmed
```

Always call `close`, also after a question fails. `native.supports(operation)` says if a
question can run here, and starts nothing.

## Prepared fixtures

Prepare the files and observation request before launching. See `examples/observe-fixture.rs`.

```rust
use pdx_native::{FixtureRequest, GameOptions};

let fixture = FixtureRequest::new(
    "common/tradition_categories/atlas.txt",
    "atlas = {\n tree_template = \"template\"\n traditions = {}\n}\n",
);
let mut game = native.start_game(GameOptions::new(supervisor).fixture(fixture)).await?;
let observed = game.observe_fixture().await; // Answer<FixtureObservation>, or an error
let disposal = game.close().await?;          // Also close after an observation error.
let observed = observed?;
```

The initial window covers three registration entries and up to two field-reader entries for
`tree_template` and `traditions`. `FixtureObservation` has separate `registration_entries` and
`field_reads` lists. Each field read names its file, source line, opaque owner and processing
stage. These are entry observations; they establish no stored value, validation or gameplay rule.

Supply one category `.txt` file, at most 64 KiB. Its filename uses letters, digits, underscores or
hyphens. The fixture replaces the private category directory; registry queries describe this
mounted content. Other observed registry content remains pinned to the installation. Unsupported
paths, observation selections and budgets fail before game launch. Script parsing remains the
engine's responsibility; an unobserved file or a read outside the bounded window cannot give a
complete answer.

Choose either or both `FixtureObservationKind` values. The only window is
`FixtureWindow::InitialCategoryLoad`. `deadline_seconds` defaults to 180 and must be 1–180; the
smaller of it and `GameOptions::startup_seconds` bounds startup observation. Repeated questions
read the same startup results and refresh the idle timeout. A different fixture needs a new session.

Recorded sessions also take the prepared request. File hashes include relative paths and exact
contents; the request hash distinguishes observation selections and windows, but not deadlines.
`Basis::Recorded` is the only change to a saved successful answer.

## Answers

Each question returns `Answer<T>`:

- `value`: what was established. A partial answer keeps each established part.
- `completeness`: `Complete` or `Partial`. `Complete` with an empty value means that nothing was found.
- `gaps`: what is missing, each with a typed `GapKind`. Empty when the answer is complete.
- `source`: the build, the Native version, the method, and the basis (such as `StaticAnalysis`).

A question that could not be answered returns `Error`, never an empty answer. To check an answer,
ask the question again.

## Recorded answers for tests

`native.record_answers_to(dir)` writes each answer as JSON during a real run.
`Native::from_recorded_answers(dir)?` reads those files and starts no process; consumer code stays
the same. Each file holds one `Result<Answer<T>, Error>`, so you can write a failure case by hand.
A recorded answer always has `Basis::Recorded`. A question with no file gives `Error::NotRecorded`.
`build.json` holds the original serialized `BuildId`, including for error-only recordings.
`native.build()` returns that identity; answers from another build return `Error::Recorded`.

```text
build.json
registries.json
registry_fields/common/traditions.json
registry_items/common/traditions.json
observe_fixture/<files-hash>/<request-hash>.json
```

## Supported build

The target catalogue has one build: the exact M45-observe ARM64 executable (Stellaris 4.5 beta,
Apple Silicon). An unknown build is refused; it never inherits the recipe of a different build.

## One-time setup for live games (Apple Silicon macOS)

Native permits one Native-owned game per host. Its reservation directory must exist before the
first live game. Run these commands from the account that runs Native:

```sh
sudo install -d -o root -g wheel -m 755 "/Library/Application Support/PDX Native"
sudo install -d -o "$(id -un)" -g "$(id -gn)" -m 700 "/Library/Application Support/PDX Native/instances"
```

The parent directory must be owned by root and not writable by group or others. The `instances`
directory must belong to the running account with no group or other access. Do not run these
commands to take over a directory that another account owns; inspect its ownership and its
unresolved reservations first. Native never creates, moves, or clears this directory itself.

## Checks

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
STELLARIS_PATH=/path/to/Stellaris cargo test --release --test static_questions -- --ignored
STELLARIS_PATH=/path/to/Stellaris cargo test --release --test live -- --ignored
cargo doc --no-deps
```

The default tests need no game. The static parity tests read the executable of the exact supported
build and start no game. The live test command runs registry and fixture controls one case after
the other, and takes several minutes; a word after `--ignored` selects cases by name.
Run the live suite separately from the default tests: lifecycle unit tests briefly create a
harmless process named `stellaris` to check that Native refuses an ordinary game.

## Documents

- [Specification](docs/specs/native.md): what Native does.
- [Technical design](docs/design/architecture.md): project layout and target composition.
- [Roadmap](docs/roadmap.md): order of work.
- [Atlas caller migration](docs/design/atlas-caller-migration.md): API replacements and recorded tests.
- [Engine knowledge index](docs/native-evidence.md): findings from prototypes and probes.
