# Decision: return Native to a simple engine API

Status: approved by Jackson, 2026-09-19; implemented 2026-09-20. This document amends the
[specification](../specs/native.md) and the [technical design](architecture.md), which now agree
with it.

Naming rule: public names prefer clarity to brevity. A method name says its subject
(`registry_fields`, not `fields`).

## Purpose of Native

Native is a standard API to ask Stellaris questions, the same on each platform and game build.
It is not an evidence archive. A caller gets an answer, a statement of how complete the answer is,
and a small stamp that says which build and method gave it. To check an answer, run the question
again. Tests use small recorded answers when no game is available.

## What stays

- **Exact build identification.** `open` hashes the executable and refuses an unknown build. There
  is no nearest-version fallback.
- **Target composition** (`binding`): target records, recipes, platform and machine leaves. A new
  build adds a record, not a copy of an adapter.
- **Independent process ownership.** The consumer-hosted supervisor owns the game and cleans up
  when the caller or the worker fails. The ordinary game profile is never touched.
- **Honest partial answers.** Complete, partial with typed gaps, or an error. An empty answer is
  never used for "could not look".
- **No native details in the public types.** No addresses, tokens, symbols or instructions.

## What goes

| Removed | Replaced by |
| --- | --- |
| Replay as a feature; the `native-evidence` package; `Engine::replay*`; descriptors and artifact hashes on results | Run the question again. Analysis methods move to `engine/analysis`. |
| Bundled qualification records, admission, promotion, withdrawal | A build is supported when it is in the catalogue. Its tests prove it. |
| `production`, `maintainer-tools`, `test-support` features; the `investigation` module; `build.rs` guards | One build. Experiments are ordinary examples or tests. |
| `capture.rs`, retention directories, startup and final snapshots | A temporary work directory, deleted on close. Kept only on failure, for debugging. |
| `CapabilityRequest`, `CapabilityBounds`, `CapabilityReport` | `native.supports(operation)` |
| Multi-gigabyte captures as test input | Small tracked test inputs and recorded answers (see Tests) |
| `tools/check-*.py` (11 files), qualification pages in `docs/native/` | Cargo tests. Knowledge pages stay (see Documents). |

## Public API

```rust
// Installation. No process is started.
let native = Native::open("/path/to/Stellaris")?;      // Result<Native, OpenError>
native.build();                                         // opaque exact-build identity
native.supports(Operation::RegistryFields);             // Supported | Unsupported(reason)

// Implemented static questions.
native.registries()?;                                   // Answer<Vec<Registry>>
native.registry_fields("common/traditions")?;           // Answer<Vec<Field>>
native.declarations(DeclarationKind::Effect)?;          // Answer<Vec<Declaration>>
native.modifiers()?;  native.modifier_categories()?;    // SDK-536
native.modifier_families("common/bypass")?;             // SDK-540
native.scopes()?;     native.scope_links()?;            // SDK-536
native.localization_declarations()?;                    // SDK-537
native.on_actions()?; native.game_rules()?;             // SDK-538
native.defines()?;                                      // SDK-539

// Implemented live questions. The consumer owns when a game runs; Native owns how.
let mut game = native.start_game(GameOptions::new(supervisor_command)).await?;
game.registry_items("common/traditions").await?;        // Answer<Vec<String>>
game.close().await?;                                    // Disposal: Confirmed | Unconfirmed

// SDK-564: pause after all content loads and read the loaded modifier table.
let mut game = native.start_game(GameOptions::new(supervisor_command).loaded_modifiers()).await?;
game.loaded_modifiers().await?;                         // Answer<LoadedModifiers>

// Tests without a game.
let native = Native::from_recorded_answers("/path/to/recorded-answers")?;

```

**SDK-532:** fixture observation is now implemented. Prepare `GameOptions::fixture(request)`
before `start_game`, then call `game.observe_fixture().await?`. Its `Answer<FixtureObservation>`
contains separate registration entries and field reads from that fixed session.

One result type:

```rust
pub struct Answer<T> {
    pub value: T,
    pub completeness: Completeness,   // Complete | Partial
    pub gaps: Vec<Gap>,               // typed; Complete may retain limits outside the method
    pub source: Source,               // build id, Native version, method name, basis
}
pub enum Basis { Declared, StaticAnalysis, LiveObservation, Recorded }
```

Question, session and recorded-answer failures use `Error`; installation opening uses the
separate `OpenError`. `Answer<T>` and `Error` implement `Serialize` and `Deserialize`; recorded
answers are a directory of these, and successful recorded answers carry `Basis::Recorded`.

Normalized value types (a sketch; each ticket fixes its own):

```rust
pub struct Registry { pub name: String }
pub struct Field    { pub name: String, pub reader: Reader, pub conditional: bool }
pub struct Reader   { pub id: Option<ReaderId>, pub kind: ReaderKind }
pub enum ReaderKind { Unknown /* extended as reader support lands */ }
pub struct Declaration { pub name: String, pub description: String, pub usage: String,
                         pub scopes: DeclaredScopes } // SDK-568: no targets; see discovery.md
pub enum DeclaredScopes { Any, Listed(Vec<ScopeReference>), Unresolved }
pub struct ScopeReference { pub id: ScopeId, pub name: String } // SDK-536: join by id, not name
```

### Recorded answers cover static and live questions

The word *fixture* means only the script files that a consumer prepares with
`GameOptions::fixture` before launch and reads through `observe_fixture`.

`Native::from_recorded_answers(dir)` selects a recorded back end once. Static questions read recorded
answers. `start_game` returns a `Game` that reads recorded answers and starts no process; its
launch options are ignored, the prepared fixture selects its recording, and `close` returns
`Disposal::NotApplicable`. Consumer code is the same for a
real game and recorded answers.

- A recorded file holds a `Result<Answer<T>, Error>` as JSON, so `Error` is serializable. Failure
  cases (partial answer, worker loss, timeout) can be written by hand.
- `registry_items(name)` reads `registry_items/<name>.json`. `observe_fixture` finds its answer by
  a hash of the fixture's files.
- A question with no recorded answer returns `Error::NotRecorded`, never an empty answer.
- Every recorded answer carries `Basis::Recorded`.
- `native.record_answers_to(dir)` writes each answer as it is returned during a real run. This
  stores normalized answers (kilobytes), not captures, and nothing is verified or derived again.

Recorded answers test the consumer's logic. They do not test Native's live path; the live tests and the
fake-worker supervisor tests do that (see Tests).

### Registry names

A registry is named by a string in every operation, static or live. A discovered candidate with
no established name is a `Gap`, not a `Registry`. The name is the full content directory
(`common/traditions`, `map/galaxy`), because seven template registries load from outside `common/`.

**Resolved, 2026-09-20:** the name is available statically. Every template database constructor
passes its content directory, as a `CString`, to one shared base constructor. The method
(`engine/analysis/directories.rs`) finds that call and establishes its argument. It names all 164
template registries on M45 in about two seconds. All 162 directories that the retained live run
observed for template registries are in the static result, with no conflict.

Two compiled shapes exist. 163 constructors build a temporary `CString` from a literal.
`common/ship_categories` passes a global `CString` that the static initializer of its source file
builds, because that file uses the path twice and declares it as a named constant. A first
version of the method looked for any directory-shaped literal in the constructor. It missed the
global shape, and it had no tie to the meaning of the literal; the call anchor replaced it.

Seven candidates load from outside `common/` (`map/galaxy`, `sound/advisor_voice_types`,
`gfx/portraits/sprite_configurations`, `interface/resource_groups`, and others). 59 more
`common/` literals belong to loaders outside the template method (`common/component_templates`,
`common/agendas`, `common/static_modifiers`); they are inputs for SDK-551.

`registries()` and `registry_fields(name)` are static questions.

**SDK-529:** `GameOptions::registries` selects the content directories observed in a live
session. `registry_items(name)` accepts any directory returned by `registries()` when selected,
and gives a precise unsupported or partial result when its initial loader cannot be witnessed.
The default session retains the two tradition registries. Session content consistency is checked
in memory; this adds no qualification record or persisted content pin.

## Tests

- **Static methods:** small authored inputs test the method logic. Parity tests read the real
  executable (ignored by default; `STELLARIS_PATH`) and compare with a small tracked expected
  output. Real method inputs are 47 to 56 MB and are not tracked.
- **Live operations:** ignored by default; run with `STELLARIS_PATH` set. They cover the normal
  case and the failure controls (missing hook, worker loss, timeout, cancel).
- **Supervisor without a game:** unit tests cover reservations, worker-process cleanup, pause
  witnesses and caller cancellation. A fake-worker session test
  (`fake_worker_session_reports_a_paused_answer_and_reaps_both_processes` in
  `src/execution/supervisor.rs`) runs a full session to a paused answer and checks that both
  processes are reaped. The live tests cover the end-to-end worker-loss, timeout, cancel and
  cleanup cases.
- **Consumers (Atlas):** `Native::from_recorded_answers`, for static and live questions. This replaces
  replay and the synthetic test-support engine.
- **Held-out method tests** (roadmap ordering principle) stay. They need the executable, not
  captures.

## Work order

1. **Done, 2026-09-20.** Amend the specification and design. Update the roadmap, README notice,
   `AGENTS.md` and development policy. This work order has no Linear tickets; this document
   tracks it. The open Native tickets (SDK-522, SDK-529, SDK-531 to SDK-557) and the open Atlas
   map tickets (SDK-470, SDK-480, SDK-485, SDK-486, SDK-490 to SDK-511, SDK-524, SDK-558) were
   updated on 2026-09-20, after the merge, to agree with this decision. Completed tickets keep
   their original text.
2. **Done, 2026-09-20.** The decode, discovery and fields methods moved from
   `crates/native-evidence` to `src/engine/analysis`. The source-hash pin and the static
   qualification records are removed: a static method is available when the build is in the
   catalogue and the executable is unchanged. The module comments hold the method descriptions
   from the deleted design pages. The private parity tests pass on the installed M45 executable
   (41/41 ownership controls; council agenda, traditions and tradition-category fields). The moved
   code keeps its old result types and replay functions; step 3 replaces them.

   Two findings change the plan:
   - **Real method inputs are 47 to 56 MB**, because each holds the full symbol and string
     inventory. They cannot be small tracked files. Parity tests read the executable itself
     (ignored by default; `STELLARIS_PATH`) and compare with small tracked expected output. The
     method logic keeps its small authored inputs.
   - **The present discovery method gives no registry name.** A short experiment showed that the
     name is available statically from each database constructor. See "Registry names".
3. **Static part done, 2026-09-20.** `Answer<T>`, `Error`, the normalized value types,
   `Native::build`, `Native::registries` and `Native::registry_fields` exist. The directory method
   (`engine/analysis/directories.rs`) names all 164 template registries on M45; every
   live-observed directory is in the result, with no conflict. Parity tests compare with 8 KB of
   tracked expected output (`tests/expected/m45`). Two fields in different registries already
   report one reader identity. The earlier static API is removed: `Native::analysis`,
   `AnalysisContext`, the raw result exports, the three static replay methods, the static
   capability variants, the decode control, and six examples. The historical ownership replay
   (`discovery/ownership.rs`, 386 lines) is removed with them: it reduced traces in the format of
   a Python prototype, and static analysis now gives the owner class and the directory of each
   template registry. Its last run passed 41/41 controls; the code is in Git at `3eef4f5`. The
   method internals still carry replay descriptors; step 4 removes those.

   **Live side, in progress.** Live admission needs no acceptance record, content match, or
   exact toolchain match (decided 2026-09-20): a live operation is available when the build
   composed and the present inputs and tools can be read. The synthetic test engine and the
   `test-support` feature are removed. `Game::registry_items("common/traditions")` returns
   `Answer<Vec<String>>`; on M45 it gives 234 traditions and 33 categories, complete, with the
   game reaped, in about 34 seconds (`examples/registry-items.rs`, no Cargo feature). A registry
   outside the build's live recipe gives `Unsupported` with the covered names, not
   `UnknownRegistry`.

   **Step 3 done, 2026-09-20.** The documented public root contains `Native`, `Game`,
   `GameOptions`, `Answer` and their operation, value, gap, source and error types, plus
   `supervisor::serve` and its error type.
   `Native::supports(operation)` replaces the capability report. `Native::get_registry`,
   `RegistryDescription`, `EngineContext` and `Engine::open` are removed. The earlier capability,
   replay and result types are in a hidden `internals::legacy` module, which only Native's live
   harness and replay tests use; step 4 deletes it with the evidence package.

   Recorded answers exist (`src/recorded.rs`): `Native::from_recorded_answers(dir)` and
   `Native::record_answers_to(dir)`. Layout: `registries.json`,
   `registry_fields/<registry>.json`, `registry_items/<registry>.json`. Verified on M45: a
   recorded live run (20 KB) reads back the same 234 and 33 items and the same error, with
   `Basis::Recorded`, no supervisor and no process. `tests/recorded_answers.rs` uses
   hand-written files, including a failure case, and needs no game.

   Left for step 4 because they depend on `capture.rs` and retention: `start_game` and `close`
   still return `GameError` and `GameReport`, and `GameOptions` still takes a work directory.
   The target is `close() -> Disposal` and one `Error` type.
   Original text of this step: add `Answer`, `Error`, the `Native` and `Game` methods, `from_recorded_answers` and `record_answers_to`. Remove `Engine`,
   the replay methods and the capability types. Update the Atlas caller; it is not frozen again
   until this is done.
4. **Done, 2026-09-20.**
   - **Done, 2026-09-20: the public signatures.** `Native::open(path)`,
     `GameOptions::new(supervisor_command)`, `native.start_game(options) -> Result<Game, Error>`
     and `game.close() -> Result<Disposal, Error>`. Native makes a temporary work directory and
     removes it after a confirmed disposal; after any other result it stays for inspection.
     `Error::Startup` carries the disposal of a failed start. Verified on M45. The earlier
     `with_supervisor`, `start_game_with_report`, `close_with_report` and `RetentionOptions` are
     hidden and serve only the live harness. `OpenError` stays a separate type for `open`.
   - **Finding: part of the evidence package is the live reducer.** The supervisor writes the
     worker's event stream to files in the work directory, and the caller gets its items by
     running `evidence::registry::replay` on those files (`game.rs`, `replay_results`). So
     `registry.rs` (event stream to items, with the loader, owner, thread and terminal joins),
     `stream.rs`, `recorded.rs` and `store.rs` are production code. They move into
     `src/engine/operations` and lose the word "replay". What is deleted: `replay.rs` and the
     SDK-483 early-observation format with `tests/replay.rs` and `tests/fixtures`, the
     descriptor, artifact-hash and provenance types, `Engine`, `ReplayRequest`, and
     `internals::legacy`.
   - **The work, in order:**
     1. **Done, 2026-09-20.** `tests/live.rs` replaces the `maintainer-tools` harness. Its cases
        are ignored by default; run them with `STELLARIS_PATH` set and `--ignored`. The test
        executable is also its own supervisor (`--supervisor`), so the target has
        `harness = false` and runs the cases one at a time. It uses only the public API and one
        hidden entry, `GameOptions::fault(registry, control)`. Cases: normal, startup timeout,
        cancel, drop without close, and the six faults (missing hook, late hook, dropped record,
        missing terminal, access failure, worker loss) on each of the two registries. After each
        case it checks that no game or supervisor process of that case remains.

        There are no Cargo features. `Authorization` is removed: every request is a game
        session, and the supervisor accepts a fault only together with the registry that
        receives it. Removed: the `investigation` module, the five maintainer examples, the
        lifecycle-only and single-capture client functions, `tests/investigation.rs`, and six
        check tools (`check-game-sessions`, `check-live-observations`,
        `check-candidate-lifecycle`, `check-candidate-observations`,
        `check-registry-observations`, `check-admission-boundary`). The supervisor still holds
        the code paths for a request with no session; no request can reach them, and item 2
        removes them with the capture code.
     2. **Done, 2026-09-20.** The live reducer is `src/engine/operations`: `event_stream.rs`
        (the worker and owner record types, and the rules for reading the worker's stream file)
        and `registry_items.rs` (stream to items, and the readiness of the pause). The
        supervisor reduces the stream once, when the game is paused, and sends the items of each
        registry in its `Paused` reply. The caller reads no file from the work directory. For
        that reason the descriptors, artifact hashes, startup and final snapshot copies,
        provenance fields and references between the two sides are all gone, with no simpler
        check in their place: the only file transport left is from the worker to the supervisor.

        Kept, because they decide whether an answer is complete: the activation witness,
        sequence continuity, the loader, owner and thread joins, slot order, terminal totals, and
        worker loss. Kept, because they stop a half-written or foreign file: a stream record must
        be one whole line with this session's attempt identity (a damaged stream loses every
        terminal, so it cannot give a complete answer); files are created once and never
        replaced; control messages appear under their final name only when complete; reads
        accept only a regular file of bounded size; the worker checks the hash of each file of
        its package and of the executable.

        Removed: `GameReport`, `GameError`, `RetentionOptions`, the hidden earlier methods, the
        capability types, `src/qualification` (admission is `Binding::blocking_reasons`, a list),
        `internals::legacy`, `capture.rs`, and the supervisor paths for a request with no
        session. Live admission now probes the debugger once for a session, not once for each
        registry. `BuildId` is the SHA-256 of the executable.

        Removed with the SDK-483 early-observation format: its reducer, tests and fixtures, the
        two replay examples, the worker's early-observation and single-registry modes, and the
        tools that served replay or qualification. Its engine bindings (registration entry,
        category reader) are in Git at `dd33300`, `src/binding/groups.rs`. `tools/knowledge_bundles.py`
        stays: it still works, and `docs/native/retrieval.md` uses it to verify and restore the
        knowledge bundles. `tools/observation/test_protocol.py` stays for the worker codec.

        Not done on 2026-09-20: the fake-worker supervisor test that "Tests" names did not
        exist. It was added on 2026-09-21 (commit `1da4abf`). The unit test of the removed hold
        loop went with that loop. The live tests cover worker loss,
        timeout, cancel, drop and cleanup; without a game, unit tests cover the reducer, the
        reservation journal, the worker's process cleanup and the pause witness.
     3. **Done, 2026-09-20.** `discovery::discover(input)` and `fields::analyze(input)` are
        plain functions from the method input to the method result. All method logic and its
        tests stay. `crates/native-evidence` and the workspace are removed; Native is one
        package. `pdx_native::internals` (hidden) holds only what Native's own integration tests
        use: the three static method modules and the live fault control.
     4. **Done, 2026-09-20.** `build.rs` no longer hashes the source. It compiles the presentation guard and writes a build stamp: the time at which
        Cargo ran the script, which Cargo does again when `src`, the manifest or the script
        changes. The caller and the supervisor compare the package version plus this stamp in
        their handshake, so two different states of the source do not talk to each other. The
        README has the new API and the live test command. The qualification records in
        `docs/native` were removed in commit `955987c`. The remaining tools are bundle retrieval
        and worker-codec tests. Historical trial instructions now point to their preserved source;
        current roadmap and test descriptions match the implementation.
5. **Done, 2026-09-20.** Verified every prototype bundle and its external copy, then removed
   duplicated restores, staging, verification trees and obsolete run outputs. `.local` fell from
   7.2 GB to about 1.5 GB. Kept the bundles, a separate exact M45 ARM64 executable, and 1,780
   additional source, note and small observation files that were not in the bundle manifests.
   Those files also have a verified archive outside `.local`; see [preservation](../native/preservation.md).

**Atlas caller migrated, 2026-09-20.** The local Atlas prototype uses the public answer API and
recorded answers, with four game-free tests. The [migration guide](atlas-caller-migration.md)
records the replacements and remaining fixture limits. The SDK-473 source-stamp amendment is
recorded in the Atlas map (SDK-470).

**Review of the four final implementation commits.** `e1f0e12`, `6a3d8dc`, `66fc181`
and `c679617` were checked against this work order. The moved live reducer keeps its activation,
sequence, loader/owner/thread, slot and terminal checks; static method logic and its bounds remain.
The caller reads normalized answers from the supervisor and no longer opens capture files. The
review follow-up repairs stale document/tool references and keeps failed live-case work directories
when a later case passes. At that review, the full fake-worker supervisor test was the known test
gap above; it was added on 2026-09-21.

This paragraph records the original decision. The milestone 2 review later replaced the durable
reservation journal with the OS lock and process inventory; see
[the repair notes](../native/milestone-2-repair-notes.md).

## `.local` (7.2 GB)

| Content | Size | Action |
| --- | ---: | --- |
| `evidence/restored`, `evidence/staging`, `evidence/verification` | 3.8 GB | Delete. They are copies of `bundles`. |
| `evidence/sdk-5xx-*`, `.local/sdk-5xx-*` run outputs | ~1.9 GB | Delete after step 2 makes its test inputs. |
| `evidence/bundles` (prototype sources and findings) | 1.4 GB | Keep until the Python prototypes are ported (milestones 2 to 4). A second copy is in `~/Documents/PDX/evidence/native-2026-09-18/`. Then keep only the prototype sources. |
| M45-observe ARM64 executable | 85 MB | Keep. Static methods and held-out tests need this exact file. SDK-522 still applies. |

## Documents

Keep the pages that hold engine knowledge: `engine-calls.md`, `discovery.md`, `targets.md`,
`early-observations.md`, `loader-entry-worker.md`, `lifecycle.md`. Remove the qualification,
production and verification records. Rewrite the specification to the purpose above; most of
sections 3 and 8 and half of the acceptance checks go.

## Effect on Atlas

Atlas claims keep provenance through `Source` (build, Native version, method, basis). They lose
hash references to retained captures. Jackson accepted this amendment to the SDK-473 evidence
decision; record it in the Atlas map.
