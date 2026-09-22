# PDX Native

A standard Rust API to ask Stellaris questions, the same on each platform and game build.
Native identifies the exact build, answers from the executable or from a supervised game, and
says how complete each answer is. Atlas is its first consumer.

Native is one Cargo package with no optional features. The
[simplification decision](docs/design/simplification.md) records how it got there.

## Static questions

No game starts. A registry is named by its content directory. See `examples/registries.rs`.

```rust
use pdx_native::{DeclarationKind, Native};

let native = Native::open("/path/to/Stellaris")?;
let registries = native.registries()?;                      // Answer<Vec<Registry>>
let fields = native.registry_fields("common/traditions")?;  // Answer<Vec<Field>>
let effects = native.declarations(DeclarationKind::Effect)?; // Answer<Vec<Declaration>>
```

`declarations` covers direct calls to the effect or trigger registration function in executable
text. Runtime-composed names and unreadable documentation are gaps. See
`examples/declarations.rs`.

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
let mut game = native.start_game(
    GameOptions::new(supervisor).registries(["common/traditions", "common/ascension_perks"])
).await?;
let items = game.registry_items("common/traditions").await?;  // Answer<Vec<String>>
let disposal = game.close().await?;                           // Disposal::Confirmed
```

Always call `close`, also after a question fails. `native.supports(operation)` checks the
build and host method without starting a game; `start_game` checks the selected content.
`GameOptions::registries` selects the directories to observe before launch. Without it, the
M45 session observes traditions and tradition categories. A listed but unselected registry
returns `Unsupported`; a selected loader that does not run before the pause gives a precise
`Unsupported` reason. When using a fixture, include its registry in the selection. See
`examples/registry-items-report.rs` for a report over all discovered
registries.

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

Supply one `.txt` file in a supported registry, at most 64 KiB. Its filename uses letters, digits,
underscores or hyphens. The fixture replaces that private registry directory; registry queries
describe this mounted content. Other observed registry content remains pinned. Unsupported
paths, observation selections and budgets fail before game launch. Script parsing remains the
engine's responsibility; an unobserved file or a read outside the bounded window cannot give a
complete answer.

For parser outcomes, use `FixtureRequest::field_outcomes` with one or more
`FixtureFieldQuestion` values. The file may be in `common/traditions` or
`common/tradition_categories`. Each outcome keeps these dimensions separate:

- `FixtureStorage` contains actual String storage after each joined occurrence and an optional
  file-terminal value, or a typed unavailable reason. Its own completeness keeps witnessed values
  when a later record or terminal is missing. A complete zero-occurrence result requires a
  witnessed definition constructor and completed file-load window.
- `diagnostics` preserves messages captured at the engine reader-report stage, with a source join
  or a missing-join reason. `DiagnosticCoverage::Complete` covers only parser diagnostics during
  this file load. `NotRequested` is distinct from unsupported or incomplete collection. It does
  not claim later validation.
- `FixtureRuntime` distinguishes `NotRequested` from `Unavailable`. Runtime is outside this
  initial-load method, so a runtime request makes the answer partial with an `OutsideMethod` gap.

Unknown fields and readers without a bound storage decoder return explicit unavailable storage.
Native does not infer validity or runtime success from storage, a diagnostic list, or a loader
return.

Choose either or both `FixtureObservationKind` values for the category entry question. It uses
`FixtureWindow::InitialCategoryLoad`; field outcomes use `FixtureWindow::InitialFileLoad`.
`InitialCategoryLoad` and `CategoryFieldReads` require `common/tradition_categories`. Registration
entries may accompany that category window, or accompany an `InitialFileLoad` field-outcome request
in either supported registry. Tradition field outcomes can capture malformed and unexpected-field
parser diagnostics. Category field outcomes report parser diagnostics and storage unavailable
because this build has no outcome binding for that registry.
`deadline_seconds` defaults to 180 and must be 1–180; the
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
declarations/effect.json
declarations/trigger.json
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
commands to take over a directory that another account owns; inspect its ownership first. Native
never creates, moves, or clears this directory itself.

## Checks

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
ATLAS_CALLER_PATH=/path/to/pdx-atlas cargo test --test consumer_boundary -- --ignored
STELLARIS_PATH=/path/to/Stellaris cargo test --release --test static_questions -- --ignored
STELLARIS_PATH=/path/to/Stellaris cargo test --release --test live -- --ignored
cargo run --release --example registry-items-report -- /path/to/Stellaris
cargo doc --no-deps
```

The default tests need no game. The static parity tests read the executable of the exact supported
build and start no game. The live test command runs registry and fixture controls one case after
the other, and takes several minutes; a word after `--ignored` selects cases by name.
The Atlas boundary check scans the frozen caller's Rust source for unsupported Native imports,
hidden hooks, platform or build branches, and native constants.
Run the live suite separately from the default tests: lifecycle unit tests briefly create a
harmless process named `stellaris` to check that Native refuses an ordinary game.

## Documents

- [Specification](docs/specs/native.md): what Native does.
- [Technical design](docs/design/architecture.md): project layout and target composition.
- [Roadmap](docs/roadmap.md): order of work.
- [Atlas caller migration](docs/design/atlas-caller-migration.md): API replacements and recorded tests.
- [Engine knowledge index](docs/engine-knowledge.md): findings from prototypes and probes.
