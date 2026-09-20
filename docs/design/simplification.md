# Decision: return Native to a simple engine API

Status: approved by Jackson, 2026-09-19. This document amends the [specification](../specs/native.md) and the
[technical design](architecture.md); those documents are to be rewritten to agree with it.

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
let native = Native::open("/path/to/Stellaris")?;      // Err: Missing, UnknownBuild, Ambiguous...
native.build();                                         // label + opaque id, for stamps only
native.supports(Operation::Fields);                     // Supported | Unsupported(reason)

// Static questions.
native.registries()?;                                   // Answer<Vec<Registry>>
native.registry_fields("common/traditions")?;           // Answer<Vec<Field>>
native.declarations(DeclarationKind::Effect)?;          // Answer<Vec<Declaration>>   (milestone 3)
native.defines()?;  native.on_actions()?;               //                            (milestone 3)

// Live questions. The consumer owns when a game runs; Native owns how.
let mut game = native.start_game(GameOptions::new(supervisor_command)).await?;
game.registry_items("common/traditions").await?;        // Answer<Vec<String>>
game.observe_fixture(fixture).await?;                   // Answer<Vec<FieldRead>>     (SDK-532)
game.close().await?;                                    // Disposal: Confirmed | Unconfirmed

// Tests without a game.
let native = Native::from_recorded_answers("tests/recorded/m45")?;
```

One result type:

```rust
pub struct Answer<T> {
    pub value: T,
    pub completeness: Completeness,   // Complete | Partial
    pub gaps: Vec<Gap>,               // typed; empty when Complete
    pub source: Source,               // build id, Native version, method name, basis
}
pub enum Basis { Declared, StaticAnalysis, LiveObservation, Recorded }
```

One error type, `Error`, with `Unsupported { operation, reason }`, `BuildChanged`, `Game(...)`,
and `Io`. `Answer<T>` implements `Serialize` and `Deserialize`; recorded answers are a
directory of these, and they always carry `Basis::Recorded`.

Normalized value types (a sketch; each ticket fixes its own):

```rust
pub struct Registry { pub name: String, pub directory: Option<String> }
pub struct Field    { pub name: String, pub reader: Reader, pub conditional: bool }
pub enum   Reader   { Known { id: ReaderId, kind: ReaderKind }, Unknown }
pub struct Declaration { pub name: String, pub description: String, pub usage: String,
                         pub scopes: Vec<String>, pub targets: Vec<String> }
```

### Recorded answers cover static and live questions

The word *fixture* means only the script files that a consumer gives to `observe_fixture`.

`Native::from_recorded_answers(dir)` selects a recorded back end once. Static questions read recorded
answers. `start_game` returns a `Game` that reads recorded answers and starts no process; its
options are ignored and `close` returns `Disposal::NotApplicable`. Consumer code is the same for a
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

**Resolved by experiment, 2026-09-20:** the name is available statically. The constructor of each
database class loads its content directory literal (an `adrp`/`add` pair). A scan of the M45
executable names 163 of the 164 template candidates with exactly one directory each, in about one
second. `CShipCategoryDatabase` has no literal in its constructor and stays a gap. The 163
directories observed live in the retained run contain no directory that conflicts; the
per-candidate comparison is a parity test for step 3.

Seven candidates load from outside `common/` (`map/galaxy`, `sound/advisor_voice_types`,
`gfx/portraits/sprite_configurations`, `interface/resource_groups`, and others). 59 more
`common/` literals belong to loaders outside the template method (`common/component_templates`,
`common/agendas`, `common/static_modifiers`); they are inputs for SDK-551.

Step 3 adds this as a static method in `engine/analysis`: constructor symbol, literal reference,
directory. `registries()` and `registry_fields(name)` stay static questions.

## Tests

- **Static methods:** small authored inputs test the method logic. Parity tests read the real
  executable (ignored by default; `STELLARIS_PATH`) and compare with a small tracked expected
  output. Real method inputs are 47 to 56 MB and are not tracked.
- **Live operations:** ignored by default; run with `STELLARIS_PATH` set. They cover the normal
  case and the failure controls (missing hook, worker loss, timeout, cancel).
- **Supervisor without a game:** a fake worker drives the supervisor through worker loss, timeout,
  cancel and cleanup. The present synthetic scenarios are kept for this purpose only.
- **Consumers (Atlas):** `Native::from_recorded_answers`, for static and live questions. This replaces
  replay and the synthetic test-support engine.
- **Held-out method tests** (roadmap ordering principle) stay. They need the executable, not
  captures.

## Work order

1. **Done, 2026-09-20.** Amend the specification and design. Update the roadmap, README notice,
   `AGENTS.md` and development policy. The Linear tickets are not edited: the roadmap "Tracking"
   paragraph states that criteria which name replay, retained captures or qualification records
   are superseded. This work order has no Linear tickets; this document tracks it.
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
   (`engine/analysis/directories.rs`) names 163 of 164 template registries on M45; every name
   agrees with the live-observed directory, with no conflict. Parity tests compare with 8 KB of
   tracked expected output (`tests/expected/m45`). Two fields in different registries already
   report one reader identity. Still to do in this step: remove the earlier static API
   (`Native::analysis`, the raw result exports, the static replay methods and examples), then the
   live side below.
   Add `Answer`, `Error`, the `Native` and `Game` methods, `from_recorded_answers` and `record_answers_to`. Remove `Engine`,
   the replay methods and the capability types. Update the Atlas caller; it is not frozen again
   until this is done.
4. Remove admission, the three features, `investigation`, `capture.rs`, the evidence package,
   and the check tools. Simplify `operation.rs` and the supervisor protocol to match.
5. Clean `.local` (below).

The durable reservation journal is unchanged. Whether an operating-system lock alone is
sufficient is a separate, later decision.

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
