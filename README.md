# PDX Native

A standard Rust API to ask Stellaris questions, the same on each platform and game build.
Native identifies the exact build, answers from the executable or from a supervised game, and
says how complete each answer is. Atlas is its first consumer.

Status: the [simplification work order](docs/design/simplification.md#work-order) is not complete.
The API below is final; some earlier internals and Cargo features are still in the source.

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
`Native::from_recorded_answers(dir)` reads those files and starts no process; consumer code stays
the same. Each file holds one `Result<Answer<T>, Error>`, so you can write a failure case by hand.
A recorded answer always has `Basis::Recorded`. A question with no file gives `Error::NotRecorded`.

```text
registries.json
registry_fields/common/traditions.json
registry_items/common/traditions.json
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
cargo test --workspace
STELLARIS_PATH=/path/to/Stellaris cargo test --test static_questions -- --ignored
```

The first command needs no game. The ignored tests need the exact supported build.

## Documents

- [Specification](docs/specs/native.md): what Native does.
- [Technical design](docs/design/architecture.md): project layout and target composition.
- [Roadmap](docs/roadmap.md): order of work.
- [Engine knowledge index](docs/native-evidence.md): findings from prototypes and probes.
