# PDX Native specification

Status: implementation specification, rewritten 2026-09-20 to agree with the approved
[simplification decision](../design/simplification.md). It replaces the evidence-producer
specification of 2026-09-17. The earlier text is in Git history. The delivered operations agree
with this document; the roadmap tracks the operations that are still planned.

## Problem Statement

Atlas needs to ask the Stellaris engine questions to derive scripting rules. The answers depend on
executable addresses, memory layouts, compiler patterns, launch workarounds, and platform process
control. If Atlas depends on these details, each game update becomes an Atlas port.

Atlas must ask the same questions on each supported platform and game build. It must know when an
answer is partial or unavailable. A successful launch, a discovered name, and a valid rule
conclusion are different results.

## Solution

PDX Native is a Rust library: one standard API to ask Stellaris questions, the same on each
platform and game build. Atlas is its first consumer. Native owns all platform and game-build
knowledge: build identification, static analysis of the executable, process lifetime, hooks, and
engine reads.

A caller gets an answer, a statement of how complete the answer is, and a small stamp that says
which build and method gave it. To check an answer, run the question again. Native is not an
evidence archive. Tests use small recorded answers when no game is available.

**Atlas has no platform or game-build knowledge.** It does not select adapters, decode native data,
compare version strings, or choose a platform path. Atlas decides what an answer establishes about
a rule.

## User Stories

1. As an Atlas developer, I want one engine interface, so that extraction logic works unchanged
   across supported platforms and game builds.
2. As an Atlas developer, I want Native to identify the installation from a location, so that I do
   not choose executable architectures or adapters.
3. As an Atlas developer, I want `supports(operation)` with a reason, so that missing native
   support is a visible extraction gap.
4. As an Atlas developer, I want to list engine registries and their fields without config seeds,
   so that I can find fields and families that the config does not have.
5. As an Atlas developer, I want each field's shared reader, so that one supported reader informs
   many rule properties.
6. As an Atlas developer, I want engine declarations (effects, triggers, modifiers, scopes, links,
   defines, on_actions) marked as declared, so that I can distinguish them from observed behavior.
7. As an Atlas developer, I want to supply fixture files and receive source-correlated field
   observations, so that I control the experiment without controlling hooks or launch timing.
8. As an Atlas developer, I want parser storage, validation, and runtime outcomes reported
   separately, so that engine recovery does not imply valid input.
9. As an Atlas developer, I want unknown conditions and typed gaps kept in partial answers, so that
   useful answers do not conceal what is missing.
10. As an Atlas developer, I want a source stamp on each answer, so that each claim records the
    build and method that support it.
11. As an Atlas developer, I want recorded answers for static and live questions, so that I can
    develop and test extraction without a game.
12. As an Atlas developer, I want deadlines and cancellation, so that a stuck game cannot leave
    extraction waiting.
13. As an operator, I want isolated profiles and owned processes, so that experiments do not change
    my ordinary game profile or unrelated processes.
14. As an operator, I want cleanup to survive caller and worker failure, so that a failed probe
    does not abandon its game process.
15. As a Native maintainer, I want an exact build check before each native operation, so that a
    game update cannot silently reuse stale assumptions.
16. As a Native maintainer, I want methods with no per-registry or per-command branch, run over
    every registry, so that reuse is demonstrated and not inferred from training examples.
17. As an offline tool maintainer, I want Native used only when rules are produced, so that
    ordinary builds and authoring need neither native tooling nor a game.

## Implementation Decisions

### 1. Authority and module boundaries

| Module | Owns | Provides to its caller |
| --- | --- | --- |
| Native public API | `Native`, `Game`, `Answer<T>`, `Error`, normalized value types | Engine questions and answers |
| Native target composition | Build identification; target records and recipes; platform and machine leaves | A fixed internal binding for one exact build |
| Native analysis methods | Decoding, value provenance, bounded control flow, owner joins, reader patterns | Normalized static answers |
| Native game supervision | Private profiles; process ownership; worker transport; deadlines; cancellation; disposal | Bounded live answers and a disposal result |
| Atlas extraction | Rule questions; fixtures; interpretation of answers; coverage and gaps | Supported claims and unresolved properties |
| Atlas rule assembly | Rule identities; conditional rules; documentation; snapshots | Deterministic offline snapshots |
| Offline consumers | Snapshot pinning; rule application; authoring advice | Author-facing tools |

Dependencies run from Atlas extraction into Native's public API. Native does not import Atlas's
rule model. Atlas does not import adapters, analysis helpers, or process control.
The frozen Atlas caller is checked by `tests/consumer_boundary.rs` against this public boundary.

Native establishes what was read or observed. Atlas decides what that establishes about a rule.

### 2. Public API

The [decision document](../design/simplification.md) holds the API sketch. Availability below is
the state after the simplification effort:

| Operation | Availability | Atlas supplies | Native returns |
| --- | --- | --- | --- |
| `Native::open` | Implemented | Installation location | A pinned installation, or a precise `OpenError` |
| `supports` | Implemented | An `Operation` | `Support::Supported`, or `Support::Unsupported` with a reason |
| `registries`, `registry_fields` | Implemented | A registry name for fields | Registries; fields with reader identity and kind, or an unknown reader |
| `start_game` | Implemented | Supervisor command and deadlines | A `Game` paused at a stated readiness boundary |
| `Game::registry_items` | Implemented | A registry name | Item names from the engine collection |
| `Game::loaded_modifiers` | Implemented for M45-release | `GameOptions::loaded_modifiers` before launch | The modifier table after all content loads, read where the engine documents its modifiers: each name with its loaded category tags, whether the executable declares it, and each `modifier_families` family and loaded item that gives it; the loaded keys of each family registry; the loaded content. Unexplained names and unjoined generation sites are gaps. No config or log file is read. |
| `Game::close`, `Game::cancel` | Implemented | — | A disposal result from `close`; `cancel` requests shutdown |
| `from_recorded_answers`, `record_answers_to` | Implemented | A directory | Recorded answers in place of a game; a record of real questions |
| `declarations` | Implemented for effects and triggers | A declaration kind | Engine name, description, usage, and declared scopes from every registration call and tail call in executable text, including registry helper constructors and names composed at run time through up to two callers; each chain of callers is one declaration. Registrations that cannot be followed, unreadable documentation, and scope getters that cannot be followed make the answer partial. Target arguments are outside it (SDK-548). |
| `modifiers`, `modifier_categories` | Implemented | — | Built-in modifiers with their declared category tags, from every direct definition call; category names from the engine's category switch. Generated modifier families are gaps. Tags are intended-use tags, not application contexts. |
| `modifier_families` | Implemented for database generators, post-read code and shared helpers | A registry name | Name templates that the registry's code registers for each item, with the item-key position, category tags, whether every item generates the family, and a name-length limit. Code that generates modifiers and is not joined to a registry is a gap, with its reason. |
| `scopes`, `scope_links` | Implemented | — | Scope types with the keywords that the engine maps to each, and keywords that match several types (`carrier`); documented links and the link prefixes that take data, each with declared input and output scopes |
| `localization_declarations` | Implemented | — | Localization contexts from the engine's text tables, each context's commands and links, each link's output context, and the scope types that select each context; a missing join keeps its commands; links that the method cannot follow are gaps |
| `on_actions` | Implemented | — | On_action names that engine call sites fire, from every direct call to the firing functions, the deferred command and the checked forwarders, with the cached pulse lists; for each name, each distinct context of `this`, `root` and the `from` chain that a followed call site supplies. Names that script content fires, names built at run time and call sites that the method cannot follow are gaps. |
| `game_rules` | Implemented | — | Game rules from the engine's rule declarations, scripted and weighted, with each distinct context that the rule set's call sites supply. A declared rule with no followed call site is a gap. |
| `defines` | Implemented for M45-release | — | Define namespace, name and engine read type from compiled read helpers; unresolved helpers are gaps. No shipped define or config file is read. |
| `Game::observe_fixture`: registration entries | M45-release only; first three initial effect-registration calls | One file under `common/tradition_categories`, selected before launch | Entry ordinal and stage during the initial category-load window |
| `Game::observe_fixture`: category reads | M45-release only; `tree_template` and `traditions` in `common/tradition_categories` | One category file and `InitialCategoryLoad` | At most two read-entry events before storage or validation; no parser outcome claim |
| `Game::observe_fixture`: field outcomes | M45-release only; initial file load for a registry with a verified loader and owner boundary | At most 32 named definition and field questions in one bounded relative text file | Source-correlated diagnostics and string storage where the exact-build binding supports the field; other dimensions report unavailable |

Rules for the API:

- **Names prefer clarity to brevity.** A method name says its subject (`registry_fields`).
- **A registry is named by a string** in every operation: the full content directory
  (`common/traditions`, `map/galaxy`). A discovered candidate with no established name is a
  gap, not a registry.
- **No native details in public types.** No addresses, offsets, tokens, symbols, instructions,
  debugger commands, launch switches, or timing workarounds appear in a request or a result.
  Missing behavior needs a Native extension or an explicit gap. There is no raw-native escape hatch.
- **The build is fixed at `open`.** Native checks that the executable is unchanged before each
  operation. A caller cannot force an adapter or ask for the nearest supported version.
- **Later operations** (prepared scripts, state reads, save loading) are added as methods on
  `Game` with their own readiness boundary. They are not promised now.

### 3. Answers

```rust
pub struct Answer<T> {
    pub value: T,
    pub completeness: Completeness,   // Complete | Partial
    pub gaps: Vec<Gap>,               // typed; OutsideMethod can accompany Complete
    pub source: Source,               // build id, Native version, method name, basis
}
pub enum Basis { Declared, StaticAnalysis, LiveObservation, Recorded }
```

`Gap.subject` is `Option<GapSubject>`, not a free-text name. The subject kind identifies a
registry, field, named answer item, localization context, localization link, scope type, or
fixture file. Context and scope subjects include `LocalizationContextId` or `ScopeId` and a
human-readable name; names alone do not identify them. A gap without an identifiable subject
has `None`. Recorded JSON writes named subjects as objects with `kind` and `name`, plus `id`
for contexts and scope types.

- `Complete` means the stated search or window completed. It is not complete game knowledge.
  `OutsideMethod` describes an explicit boundary; other gaps make the bounded answer partial.
- An empty `Complete` answer needs a completed search that found nothing. An access failure, a
  missing hook, or a lost record gives `Partial` with a gap, or an `Error`.
- An unsupported operation does not mean that the game forbids a construct.
- `Basis` keeps a declaration, a traced static relationship, and an observed behavior distinct.
- Parser storage, engine validation, and runtime outcome are separate values. A scope pointer does
  not establish scope availability. A candidate reference class does not establish lookup semantics.
- Two fields that use one shared reader report the same reader identity.
- The build id in `Source` is opaque to Atlas. Atlas may keep it and compare it for equality.

Question, session, and recorded-answer failures use one `Error` type. Opening an installation uses
`OpenError`, because no `Native` exists yet. `Answer<T>` and `Error` are serializable.

### 4. Game lifecycle and isolation

Native is a library. The consumer supplies a dedicated supervisor executable that calls
`supervisor::serve`. The supervisor owns the game independently of the caller and of the
observation worker.

- Native prepares an isolated profile. It never changes the ordinary profile and never kills a
  process that it does not own.
- Hooks must be active before the engine phase that they observe, with an ordering witness. A
  fixed delay is not a witness. If activation is not established, the answer is an error or
  partial; it is never complete.
- `close` returns disposal as confirmed, unconfirmed, or not applicable. Worker failure, caller
  loss, timeout, cancellation, and partial launch all end with a bounded cleanup attempt.
  Failed final cleanup returns `Error::Cleanup` with the witnessed disposal and retains the work
  directory. A report-write failure is separate from whether the game was reaped.
- One Native-owned game runs on a host at a time. A conflicting instance is refused with a reason.
  An OS lock excludes concurrent Native sessions; a process inventory checks for other Stellaris
  instances. Old session files do not block a new launch after these checks pass.
- A `Game` uses a temporary work directory. `close` deletes it only after a clean, confirmed
  disposal with no read error; otherwise it is kept, and the error names its path.
- No blind retry of an operation whose completion is uncertain.

### 5. Shared native methods

Decoding, value provenance, bounded control-flow analysis, owner joins, and reader patterns stay
inside Native. Do not build a handwritten answer table for each command or field.

An unfamiliar instruction shape, unresolved callee, clobbered value, or unproved owner narrows or
stops an answer and becomes a gap. Engine-only discovery runs without config or a field list. A
method has no branch on a registry, a command or a build: a fact the executable states is derived,
a per-build fact lives in the binding authority, and a fact no method reaches is a manual exception.
A manual exception records its claim, conditions, obstacle, and removal route; it is never
presented as automatic extraction. Transfer is measured by running a method over every registry,
not by freezing it; the [development policy](../development-policy.md#keep-engine-knowledge-in-its-home)
states the rule and its measurement (amended 2026-09-23).

### 6. Atlas's first consumer path

| Atlas question | Native responsibility | Atlas responsibility |
| --- | --- | --- |
| Which definitions and fields exist? | Registries, fields, readers, with gaps | Coverage obligations and rule subjects |
| What happens when fields are omitted, repeated, malformed, or conditional? | Observe stages, storage, diagnostics, conditions | Structural and conditional claims |
| Which definitions do references select? | Lookup and owner relationships, with conditional outcomes | Reference categories |
| Which shared numeric, command, modifier, or weight reader is used? | Reader behavior and unresolved paths | Reusable rule definitions |
| Which script contexts are available? | Actual scope type and availability | Scope constraints |
| Which files and duplicate definitions were used? | Mounted selection, loader phases, duplicates | Naming and loading relationships |

Atlas owns fixture meaning. Native owns mounting, isolation, and execution. Native does not ship
game catalogues or config-derived fallback answers.

### 7. Supported builds and maintenance

A build is supported when its exact executable identity is in the target catalogue. Its tests
prove the support. There are no separate qualification records. A new patch does not inherit
support; it needs a target record and passing tests.

Begin with the pinned Apple Silicon Stellaris 4.5 beta. Keep one copy of that executable
(SDK-522); static methods and their parity tests need the exact file. Maintain one supported stable
release at a time.

**Amendment, 2026-09-22 (Jackson):** the 4.5 full release, Cygnus v4.5.0 (8697), replaces the
beta in the catalogue. Steam does not offer old open betas for download, but it does offer old
full releases, so a full release is the only target worth keeping. The beta ARM64 executable
stays in `.local/executables` as a knowledge source.

**Amendment, 2026-09-19 (Jackson):** Windows x64 is deferred. Atlas publishes platform-independent
snapshots, so one platform is sufficient for rule coverage. Windows returns with the separate
real-game testing framework. Target composition keeps platform knowledge in its own leaves, so
the later Windows work adds a leaf, not a redesign.

Freeze Atlas extraction logic for a second distinct Apple Silicon executable (the update
rehearsal). Routine port work stays in Native. Measure tooling, routine updates, and exceptional
repairs separately. No numeric maintenance guarantee is accepted.

### 8. Recorded answers and provenance

Recorded answers are JSON files of `Result<Answer<T>, Error>`. `record_answers_to` writes them
during a real run. `from_recorded_answers` serves them for static and live questions and starts no
process. A question with no recorded answer returns `Error::NotRecorded`. Each recorded answer
carries `Basis::Recorded`. Failure cases can be written by hand.

Each directory has a `build.json` containing the original serialized `BuildId` (a JSON string).
Opening a recorded directory returns `Result<Native, Error>` and requires valid build metadata.
`Native::build()` and all successful answers use that original identity. Reads and recording
into an existing directory reject a different build. Errors can be recorded without a successful
answer and still keep the build identity.

Atlas claims keep provenance through `Source`. They do not reference retained captures. This
amends the SDK-473 evidence decision (accepted by Jackson, 2026-09-19).

Atlas pins an exact Native source release. Prototype sources that hold unported knowledge are kept
until their methods exist in Rust; see the [development policy](../development-policy.md).

## Testing Decisions

Test behavior through the public API. Internal tests are justified for unsafe decoding and for
supervision failures that the public API cannot cause safely.

1. **Boundary:** the Atlas caller has no platform or build branches, native constants, or adapter
   imports.
2. **Build check:** a changed executable, an unknown build, and an unsupported operation each give
   the declared error and execute nothing.
3. **Answer integrity:** wrong owners, clobbered values, unresolved calls, and missing joins give
   typed gaps. No partial answer becomes complete.
4. **Static methods:** small authored inputs test method logic. Ignored parity tests read the
   exact executable and compare with small tracked expected answers (164 template registries and
   ten agenda fields, plus traditions and categories). Large real method inputs are not tracked.
5. **Live operations:** ignored by default; run with `STELLARIS_PATH`. Normal, missing-hook,
   dropped-record, worker-loss, timeout, and cancel cases. The ordinary profile and unrelated
   processes stay unchanged.
6. **Supervisor without a game:** unit tests cover reservation ownership, worker-process cleanup,
   pause witnesses and caller cancellation. A fake-worker session test sends a complete paused
   answer through the supervisor observation and cleanup path. It checks the final report,
   worker reap, game disposal and reservation release. The real debugger attachment checks
   require the live game.
7. **Recorded answers:** a recorded run gives the same answers as the real run apart from `Basis`;
   a missing record gives `NotRecorded`; no process starts.
8. **Shared-method transfer:** a method ticket ends with one run over every discovered registry,
   recording complete, partial and failed counts and each failure shape. The registry sweep runs
   the method set unchanged at a recorded commit; its failures become follow-up tickets, not fixes
   inside the sweep. A locality gate (SDK-569) checks that method, session and operation code
   has no registry, command or build branch (amended 2026-09-23).
9. **Atlas integration:** carry a tradition field through Native answers, Atlas claims, a
   snapshot, and an offline consumer. Include invalid input and an absent answer.
10. **Update portability:** run the frozen Atlas flow on a second Apple Silicon executable.

## Out of Scope

- Atlas rule extraction, snapshot assembly, or consumer diagnostic policy inside Native.
- Replay of retained captures, evidence archives, and qualification records.
- Game or mod catalogues, and config-derived fallback answers.
- A general decompiler, an arbitrary native-code API, or a promise that every property can be
  extracted automatically.
- The author-facing real-game testing framework.
- Other games, Windows, Linux, and Intel Mac in the first release.
- Concurrent games, supervisor-loss recovery, and guaranteed invisible launch.
- Installation discovery without a location. It does not block config coverage.

## Governing records

- [Simplification decision](../design/simplification.md): purpose, API, removals, work order.
- [Technical design](../design/architecture.md): project layout and target composition.
- [Roadmap](../roadmap.md): order of work.
- [Atlas map, SDK-470](https://linear.app/unnamed-system/issue/SDK-470/specify-pdx-atlas-and-its-engine-derived-rule-database):
  Atlas decisions and open extraction questions. SDK-475 (module boundary), SDK-476 (supported
  builds) and SDK-472 (first coverage) still apply. SDK-473 is amended as stated in section 8.
- [Engine knowledge index](../engine-knowledge.md): where prototype knowledge is kept.
