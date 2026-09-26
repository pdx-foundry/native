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
let grammar = native.command_grammar(DeclarationKind::Effect, "random_list")?;
let modifiers = native.modifiers()?;                        // Answer<Vec<ModifierDeclaration>>
let categories = native.modifier_categories()?;             // Answer<Vec<ModifierCategory>>
let families = native.modifier_families("common/bypass")?;  // Answer<Vec<ModifierFamily>>
let scopes = native.scopes()?;                              // Answer<ScopeInventory>
let links = native.scope_links()?;                          // Answer<Vec<ScopeLink>>
let localization = native.localization_declarations()?;     // Answer<LocalizationDeclarations>
let on_actions = native.on_actions()?;                      // Answer<Vec<OnAction>>
let game_rules = native.game_rules()?;                      // Answer<Vec<GameRule>>
let defines = native.defines()?;                            // Answer<Vec<Define>>
```

`declarations` covers every call and tail call to the effect or trigger registration function or
to a registry helper constructor in executable text. It follows names composed at run time, such as
the script-list commands, through their callers. Each declaration has the scopes that its command
declares; which scope types a target argument accepts is not part of it. A registration that it
cannot follow, and unreadable documentation, are gaps. `modifiers` covers direct
modifier definitions; modifier families that content generates are gaps. `modifier_families`
gives the name templates that one registry's code registers for each item, such as
`{key}_ship_windup_mult`: its database generator, its post-read code and the shared helpers that
they call. A family that only the item's post-read code registers is `Always` only when Native
establishes that the engine runs that code for every item it loads. Apply
`ModifierFamily::name_for` to item keys. Generating code that is not joined to a
registry is counted as a gap, with the reason. Category tags are intended-use tags, not where a
modifier takes effect. `scopes` groups keywords only by the engine's
keyword-to-scope map; a keyword that matches several types, such as `carrier`, is a group.
`scope_links` gives declared input and output scopes for each link, including the links that take
data, such as `event_target:`, and marks those links. A scope
reference carries an opaque `ScopeId`; join it to `scopes()` by that identity, because two scope
types can share a display name. `localization_declarations` gives the localization contexts (such
as `Country` or `Dead Fleet`), the commands and links that each context declares, each link's
output context, and the scope types that select each context; a context that no scope type selects
is `Missing`, and its commands stay in the answer. Join context references by their
`LocalizationContextId`. `on_actions` and `game_rules` give the callbacks that the engine calls by
name, each with the scopes that its call sites supply for `this`, `root` and the `from` chain. A
name that different call sites fire with different scopes keeps each `EntryContext`; a link that
points back to its own scope, the engine's default, is `SelfLink`; a name whose call sites could
not be followed has no entries and a gap. See `examples/declarations.rs`.
`defines` reports the namespace, name and value type of each resolved executable read helper.
Custom table searches that cannot be followed are named gaps. It does not read define files or
return their example values, documentation or defaults.

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

Always call `close`, also after a question fails. After a failed question, `close` keeps the
session's work directory for inspection; `game.work_directory()` gives its path.
`native.supports(operation)` checks the build and host method without starting a game;
`start_game` checks the selected content.
`GameOptions::registries` selects the directories to observe before launch. Without it, the
M45 session observes traditions and tradition categories. A listed but unselected registry
returns `Unsupported`; a selected loader that does not run before the pause gives a precise
`Unsupported` reason. When using a fixture, include its registry in the selection. See
`examples/registry-items-report.rs` for a report over all discovered
registries.

## The loaded modifier inventory

`GameOptions::loaded_modifiers` runs the game on until all content has loaded and pauses it where
the engine documents its modifiers (`GameReadiness::PausedAfterContentLoad`). See
`examples/loaded-modifiers.rs`.

```rust
let mut game = native.start_game(GameOptions::new(supervisor).loaded_modifiers()).await?;
let loaded = game.loaded_modifiers().await; // Answer<LoadedModifiers>, or an error
let disposal = game.close().await?;
```

Each `LoadedModifier` has its loaded category tags, by the rule of `Native::modifiers`, whether
the executable declares its name, and each `modifier_families` family and loaded item whose
generated name it is. A name that is neither declared nor generated is unexplained; a gap counts
those names. `registry_items` holds the loaded keys that the families were applied to, and
`content` states what the game loaded. The selected registries are still observed. On M45-release
the table has 45,578 entries: 571 declared, 987 generated and 44,020 unexplained.

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

- `FixtureParsing` contains source-located entry/return occurrences when `.with_parsing()` is
  requested. Block parsing needs no storage decoder; a return does not establish runtime success.
- `FixtureStorage` contains actual String storage after each joined occurrence and an optional
  file-terminal value, or a typed unavailable reason. Its own completeness keeps witnessed values
  when a later record or terminal is missing. A complete zero-occurrence result requires a
  witnessed definition constructor and completed file-load window.
- `diagnostics` preserves messages captured at the engine reader-report stage, with a source join
  or a missing-join reason. `DiagnosticCoverage::Complete` names its bounded window.
  `.through_validation()` extends field outcomes through the bound post-read validation point,
  including source-correlated engine-log errors. `NotRequested` remains distinct from unsupported
  or incomplete collection. Neither window claims runtime validation.
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
- `gaps`: what is missing, each with a reason (`GapKind`) and, when known, a typed
  `GapSubject`. A subject names a registry, field, answer item, localization link, context,
  scope type, or fixture file. Context and scope subjects include stable IDs as well as names.
- `source`: the build, the Native version, the method, and the basis (such as `StaticAnalysis`).

A question that could not be answered returns `Error`, never an empty answer. To check an answer,
ask the question again.

## Recorded answers for tests

`native.record_answers_to(dir)` writes each answer as JSON during a real run.
`Native::from_recorded_answers(dir)?` reads those files and starts no process; consumer code stays
the same. Each file holds one `Result<Answer<T>, Error>`, so you can write a failure case by hand.
A recorded answer always has `Basis::Recorded`. A question with no file gives `Error::NotRecorded`.
Named gap subjects use an object such as `{"kind":"registry","name":"common/traditions"}`;
context and scope subjects also have an `id`. Older recordings with a string subject must be
updated to this format.
`build.json` holds the original serialized `BuildId`, including for error-only recordings.
`native.build()` returns that identity; answers from another build return `Error::Recorded`.

```text
build.json
registries.json
registry_fields/common/traditions.json
declarations/effect.json
declarations/trigger.json
command_grammar/effect/random_list.json
modifiers.json
modifier_categories.json
modifier_families/common/bypass.json
scopes.json
scope_links.json
localization_declarations.json
on_actions.json
game_rules.json
defines.json
registry_items/common/traditions.json
observe_fixture/<files-hash>/<request-hash>.json
loaded_modifiers.json
```

## Supported build

The target catalogue has one build: the exact M45-release ARM64 executable (Stellaris
Cygnus v4.5.0 (8697), the full 4.5 release, Apple Silicon). An unknown build is refused; it never inherits the recipe of a different build.

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
STELLARIS_PATH=/path/to/Stellaris cargo parity
STELLARIS_PATH=/path/to/Stellaris cargo live
cargo run --release --example registry-items-report -- /path/to/Stellaris
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
```

`rust-toolchain.toml` pins the Rust toolchain. Local builds and CI use the same version.

The default tests need no game. The static parity tests read the executable of the exact supported
build and start no game. The live test command runs registry, fixture and loaded-modifier controls one case after
the other, and takes several minutes; a word after `cargo live` selects cases by name.
`cargo parity` and `cargo live` are aliases in `.cargo/config.toml` for
`cargo test --release --test static_questions -- --ignored` and
`cargo test --release --test live -- --ignored`.
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
