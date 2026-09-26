# Native technical design: knowledge ownership and target composition

Status: implementation design supporting the [Native specification](../specs/native.md), rewritten
2026-09-20 to agree with the [simplification decision](simplification.md). The layout below is the
present source. The earlier text is in Git history.

## Design position

Native has one public API and one place that assembles the implementation for an exact build.
Internally it separates operating-system process services, machine mechanisms, engine bindings,
and analysis methods. Established libraries supply executable parsing, disassembly, and
serialization; Native owns the game-specific interpretation.

Use composition to share knowledge across builds. A new build normally adds an exact-target record
and tests, with binding or method changes only where the executable requires them. It does not
copy the preceding adapter or add version tests to operation handlers.

Two principles govern the design:

- **Hunt & Thomas, DRY:** each piece of knowledge has one authority. Similar-looking code need not
  be combined when it represents different knowledge.
- **Meyer, Single Choice:** the module that knows a set of alternatives selects the behavior.
  Downstream modules receive the selected behavior; they do not repeat the classification.

The public API is the primary test seam.

## Project layout

One Cargo package, one library, no optional features. Consumers supply the supervisor executable.

```text
Cargo.toml                         pdx-native package
build.rs                           compiles the presentation guard; writes the build stamp
src/
  lib.rs                           explicit exports of the public API
  answer.rs                        Answer, Source, Gap, Error, Disposal, normalized value types
  api.rs                           OpenError; the private reasons that block an answer
  session.rs                       Native: a pinned installation, or recorded answers
  session/                         Native's questions, one file for each group
    questions.rs                   build, support, declarations, registries, registry fields
    language.rs                    modifiers, modifier categories, scopes, scope links
    localization.rs                localization contexts, commands and links
    callbacks.rs                   on_actions and game rules
    defines.rs                     the define inventory
    families.rs                    the modifier families that a registry generates
    loaded_modifiers.rs            the live loaded modifier table, joined with static answers
  game.rs                          Game: live session or recorded back end
  game/driver.rs                   the thread that talks to the supervisor process
  fixture.rs                       consumer fixture request and normalized observation types
  grammar.rs                       partial child grammar and conditional routing types
  recorded.rs                      recorded answers: read, write, NotRecorded
  supervisor.rs                    public consumer-hosted supervisor entry point
  work_directory.rs                file rules of a session's work directory
  protocol.rs                      caller/supervisor handshake, replies and framing
  protocol/
    session.rs                     session request, controls, final report, test faults
    observation.rs                 supervisor/worker wire; generates the worker's Python schemas
  binding.rs                       narrow bound interfaces; private composition subtree
  binding/
    compose.rs                     sole target implementation assembly point
    analysis.rs                    static methods bound to one verified executable
    installation.rs                installation location, pinned content, integrity
    targets.rs                     catalogue lookup; no concrete host imports
    targets/
      records.rs                   exact-target data and recipe references
      recipes.rs                   host-neutral identifiers for required implementations
    groups.rs                      build-specific live addresses and layouts
    platform.rs                    compile-time host selection; live strategy resolution
    platform/
      macos/                       macOS ownership/access, the LLDB strategy and its worker
      unavailable/                 every other host: live operations are unsupported
    binary.rs                      thin object-crate integration and identity capture
    binary/                        executable readers for the static methods, including declarations.rs and language.rs
      inventory.rs                 symbols and strings of any supported image; no target record
      fixups.rs                    chained fixups, or a diagnostic that names the unread form
      discovery.rs                 registry discovery input: inventory and required fixups
    inspect.rs                     developer inspector, re-exported as the hidden internals::inspect
    machine.rs                     decoder/call-mechanism resolution
    machine/
      arm64.rs                     ARM64 registers and spawn preference
  engine/
    analysis/                      callbacks, decode, declarations, defines, directories, discovery, evaluate, families, fields, localization, modifier_table, modifiers, readers, scopes: bounded static methods
    operations/
      event_stream.rs              worker and owner records; rules for reading the worker's stream
      fixture.rs                   fixture observation reducer
      loaded_modifiers.rs          stream and table file to the loaded modifier table
      registry_items.rs            stream to registry items; readiness of the pause
  execution/
    supervisor.rs                  independent process/resource ownership; reduces at the pause
    owner_events.rs                the supervisor's record of what it did
    instances.rs                   host lock and process-inventory admission
tests/
  (static method unit tests live beside engine/analysis source)
  static_questions.rs              parity with tests/expected; ignored; needs STELLARIS_PATH
  recorded_answers.rs              recorded answers through the public API
  installation.rs                  installation identification errors
  live.rs                          the real game; ignored; needs STELLARIS_PATH
  consumer_boundary.rs             Atlas caller's source uses only the public API; ignored; needs ATLAS_CALLER_PATH
  expected/                        small tracked expected output of the parity tests
  support/                         synthetic Mach-O, fat and PE files for the installation tests
tools/
  knowledge_bundles.py             verify and restore the private knowledge bundles
  observation/test_protocol.py     the worker's generated codec
  profiling/                       SDK-559 timing runner and the script that instruments a source copy
docs/
  specs/native.md                  product behavior
  design/                          this design and the simplification decision
  native/                          engine knowledge pages
  native/performance/              measurement records of SDK-559 to SDK-561
```

Use `feature.rs` as each module's entry file and `feature/` for its internal modules. Folders
describe responsibilities; add them when an implemented operation needs them. If an injected
library is later required, give it a separate build target because its loading requirements differ.

## Dependency rules

```text
Atlas → public API ─┬─ target composition → selected implementations
                    ├─ engine::analysis   → normalized static answers
                    ├─ game supervision   → normalized live answers
                    └─ recorded answers   (no process, no installation)

Independent supervisor → host lock + owned game + worker + session report
```

The private `binding` subtree contains composition, target records, platform leaves, and machine
leaves. Its root exposes only the bound interfaces that the API, supervision, and engine operations
need. Shared operation logic receives bound interfaces and cannot name the concrete descendants.
Platform services do not import game-build records. Binary and machine code does not import Atlas
concepts.

Enforce this with module privacy. `binding::targets` is private to `binding`; lookup functions use
`pub(super)` or `pub(in crate::binding)`. Platform leaves are private children of
`binding::platform`. Do not widen descriptors to `pub(crate)` for convenience.

**Public types hold no native details.** Addresses, tokens, symbols, and instruction paths stay
inside `engine::analysis` and `binding`. A method maps its internal result to the normalized value
type (`Registry`, `Field`, `Reader`) before it returns.

Keep internal interfaces narrow. An operation that needs a bound reader and an observation channel
receives those, not a universal object that exposes platform, version, and every engine service.

## Where knowledge lives

| Knowledge or decision | Single authority |
| --- | --- |
| Public request and answer meaning | `answer`, `api`, `session`, and `game` definitions and their documented invariants |
| Owner/worker message shape | `protocol` definitions; generated bindings for another language |
| Recorded-answer file shape | `recorded` |
| Installation locations and host prerequisites | Compile-time-selected `binding::platform` implementation |
| Exact executable/slice identity and its recipe | Target record under `binding/targets/records` |
| Executable-format parsing | `object` library, integrated by `binding::binary` |
| Instruction decoding and call lowering | Decoder plus selected `binding::machine` mechanisms |
| Engine ABI, layouts, function/global bindings | Definitions under `binding/groups` |
| Which implementations make one build's operation | Recipe under `binding/targets/recipes`, resolved by `compose` |
| What a reusable analysis method establishes | Method implementation in `engine/analysis` |
| Whether a build is supported | Presence in the target catalogue; tests prove it |
| Whether a live answer is complete | The validated worker event stream, reduced once by the supervisor with `engine::operations` |
| Which live resources belong to a job and whether they were disposed | The independent supervisor |
| Whether any Native owner may launch a game on this host | `execution::instances` reservation |
| What answers establish about a game rule | Atlas extraction |

Store an engine offset or function declaration once in its binding definition. An observer,
analyzer, and runtime caller refer to that definition. A target record selects the binding
definition; it does not copy its fields.

Reuse requires evidence that knowledge is the same. Two equal offsets on unrelated builds may
remain separate bindings, because equal numbers do not establish shared meaning.

## Single Choice at each decision point

| Distinction | Decided by | Passed onward | Not passed as a selector |
| --- | --- | --- | --- |
| Host operating system | Compile-time selection at `binding::platform` entry | Host services and strategy resolver | Runtime OS switch, `is_windows` |
| Executable format | `object` library when opening the bytes | Parsed image through `binding::binary` | Native's own parallel parser hierarchy |
| Process architecture | Machine factory using the selected slice | Decoder/call mechanisms | Architecture strings in operation handlers |
| Exact game build | Target catalogue after identity capture | Selected target record and recipe | Semantic-version comparisons |
| Native variation for an operation | Target composition | Bound operation implementation | `legacy`, `new_layout`, or build booleans |
| Real game versus recorded answers | `Native::open` or `Native::from_recorded_answers` | The selected back end inside `Native` and `Game` | Repeated `if recorded` inside methods |
| Answer completeness | Event and ownership reducers | `Completeness` and typed gaps | Success inferred separately by several callers |

These choices happen once per relevant lifetime. A build is bound per `Native`. Checking that the
executable is unchanged before an operation is an integrity check, not a second selection.

Use compile-time platform selection at the platform module entry and for unavoidable foreign
declarations. Do not scatter `cfg` sections through the API, game, and analysis logic.

## Established primitives before new abstractions

| Concern | Choice | Native still owns |
| --- | --- | --- |
| Executable files | [`object`](https://docs.rs/object/latest/object/) | Exact identity, slice selection, game-specific symbol and fixup interpretation |
| Disassembly | The present ARM64 decoder; [`capstone`](https://docs.rs/capstone/latest/capstone/) if method controls need it | Instruction normalization, value tracking, bounded pattern semantics |
| Serialization | [Serde](https://serde.rs/) with JSON | Answer shape and completeness rules |

There is no separate pluggable `image` axis. Library support for a file format or instruction does
not make a Native operation supported.

## Composing the version × platform cases

A displayed game version is a label, not the composition key. The lookup key identifies the actual
executable and the selected architecture slice.

Represent support as a **sparse set of exact-target compositions**. There is no default Cartesian
product of versions, platforms, methods, and operations.

Each target record refers to an explicit recipe. Both are host-neutral Rust data: architecture,
format, binding-group references, a live strategy identifier, and the layout facts of the static
declaration methods, with no references to concrete platform types or function pointers. Records
do not use `cfg`.

`binding::compose` looks up the record and recipe, resolves its binding groups and machine methods,
and asks the compiled host's strategy resolver to turn a strategy identifier into an
implementation. A strategy that the host cannot run returns unsupported; it never selects a
fallback. The composer rejects no match, multiple matches, missing parts, and incompatible parts.
Catalogue ordering never breaks a tie.

Share the largest proven unit of knowledge:

- Process creation and disposal are normally shared by host platform.
- Instruction handling is shared by architecture where its method limits permit.
- Calling conventions can depend on platform and architecture together.
- Engine layout and call bindings are shared by an established compatibility family, or stay
  exact-target bindings.
- Analysis methods are shared by demonstrated compiler and reader behavior, not by version ranges.
- Launch and hook strategies can depend on platform, build, and phase.

Avoid adapter inheritance ("4.5 extends 4.4.6 except for these fields"). Prefer immutable binding
groups referenced by a complete recipe. A recipe may reuse an unchanged group and replace a
changed group.

### M45-release data sketch

Types and lookup boilerplate are abbreviated. The live layout lives in `binding/groups`;
static discovery finds the initial loader symbol for each selected registry.

```rust
const M45_RELEASE: TargetRecord = TargetRecord {
    executable: "07988b4f1b865623becd7a61af1cae92e111be6515d341754af70f02107822cd",
    slice: "a4cb49ad17a84ef6bf438019a50d3a66362c80731f8359888ddbce47c0d0aab9",
    architecture: object::Architecture::Aarch64,
    format: object::BinaryFormat::MachO,
    recipe: &M45_RELEASE_RECIPE,
};

const M45_RELEASE_RECIPE: Recipe = Recipe {
    groups: &[BindingGroupId::M45TemplateRegistryLayout],
    default_registries: &["common/traditions", "common/tradition_categories"],
    strategy: StrategyId::MacSuspendedChildLoaderEntry,
    declarations: Some(&M45_DECLARATIONS),
};
```

The registry group holds the shared M45 collection layout. Static discovery supplies each
selected registry's initial loader entry. The content directory is its identity on the caller,
supervisor and worker sides. The supervisor derives these bindings again from the executable;
it does not trust addresses from the caller.

## Binding once, executing without target tests

`Native::open` does four steps:

1. Accept an installation location. Read the executable identity and select the slice. Atlas
   supplies no build or architecture selector.
2. Look up the exact target record. An unknown build is an error.
3. Resolve the recipe's machine support, live strategy and binding groups, and prepare the static
   analysis binding.
4. Return `Native`. Target records, recipes and native bindings stay private. Input integrity and
   host prerequisites are checked again when a question or game needs them.

Later operations dispatch through the bound operation set. They do not take a build enum or
inspect a version string. `supports(operation)` reads the same set.

Prefer concrete types or a small private trait where behavior varies. Do not build a generic
plugin system or a string-keyed service locator. A new operation is an explicit public method; a
new build does not modify shared operation dispatch.

Facts that can change are checked at their proper lifetime: executable integrity before an
operation, object liveness before access, observation ordering during a session.

## Process and state ownership

The caller holds `Native` and `Game`. The independent supervisor process holds the game ownership
resources, the cleanup budget, and the cancellation state. The observation worker holds transient
debugger state. Worker failure must not remove the supervisor's disposal capability.

The supervisor opens the installation itself and refuses a game build other than the one that the
caller opened. The caller and the supervisor also compare their Native build in the handshake. The
worker checks the hash of the executable and of its own package; it does not search for a
different compatible build.

`execution::instances` owns the host-wide live-instance namespace: one Native-owned Stellaris game
per host. The supervisor takes an OS-backed exclusive lock before it checks for conflicting game
processes, and holds it through disposal. Old session reports do not block a later launch after
the lock and process inventory checks pass. The work directory records process identities,
disposal and report-write failures for the caller.

Ordinary game launchers do not honor the lock. Check for a conflicting instance before launch and
during the job. Never kill an unowned game.

| Lifetime | Owned state |
| --- | --- |
| Native release | API definitions, catalogue entries, recipes, methods, bindings |
| Host-wide namespace | Exclusive launch lock; held by the supervisor |
| `Native` | Exact target binding, bound operation set, selected back end |
| `Game` | Deadlines, temporary work directory, normalized paused answers |
| Supervisor | Owned process handles, private profile, disposal result |
| Worker | Current hooks, debugger state, transient observations |

Completeness and disposal each have one producer. The event reducer derives completeness from the
declared witnesses; the supervisor supplies disposal. The public answer combines them without
recomputing them from exit codes or empty logs.

`close` requests bounded cleanup from the supervisor. A Rust destructor gives best-effort
signaling only. A broken control channel gives an unconfirmed disposal; it does not imply that the
game exited.

## Debugger-worker integration decision

SDK-515 selects an LLDB subprocess with embedded Python callbacks for the
`MacSuspendedChildLoaderEntry` strategy. The supervisor stays the game's direct parent and spawns
LLDB directly. Target recipes and Atlas requests have no debugger selector. Private `protocol` is
the authority for wire meaning; do not duplicate Python breakpoint semantics in Rust.

| Alternative | Why it was not selected |
| --- | --- |
| External Python that imports LLDB | Needs a matched interpreter, framework loader paths and module paths. The host's ordinary Python could not import `lldb`; Xcode's LLDB loads its own Python. A second interpreter setup adds no capability. |
| Rust LLDB binding | Needs a compatible binding and framework to be selected, distributed and linked, and the callbacks to be translated. No required benefit. Not built. |
| Custom debugger or in-process observer | Reopens ordering, calling conventions, access and failure handling. The earlier dylib attempt did not establish parsing. |

The worker and the supervisor exchange a handshake (protocol revision, attempt identity, process
identities, executable identity, LLDB and Python versions) before the game resumes. The trace is
an append-only sequence with a terminal record; a missing sequence number or terminal prevents a
complete answer. Worker output streams are diagnostics and are never parsed as observations.

## Change examples and locality checks

| Change | Expected edits | Edits that indicate knowledge has spread |
| --- | --- | --- |
| New exact build using established bindings/methods | New target record; existing recipe where valid; tests | Version checks in the API or operation handlers |
| One field offset changes | New or revised binding group; affected recipe reference | The offset repeated in observer, analyzer, and worker |
| Calling convention differs | Selected call-mechanism implementation plus typed bindings | Platform checks around each engine call |
| New reader or compiler pattern | Bounded shared method and controls | Command-specific answers in Atlas or a universal fallback decoder |
| One build needs a different early-hook strategy | Concrete strategy; one recipe change | `if build == ...` in every observation operation |
| A rule conclusion changes | Atlas interpretation; Native only if its answer was wrong | Native assembling a corrective rule table |

The useful maintenance question is: **which authoritative decision changed?**

## Verification

1. **Catalogue consistency:** exact-target keys are unique; recipe references resolve; each
   operation has one implementation per composition; incompatible parts fail assembly.
2. **Change locality:** a synthetic target that reuses existing methods needs no change to shared
   operation source.
3. **Dependency checks:** shared engine operations cannot import `binding::targets::records` or
   platform and machine leaves. Module privacy enforces this.
4. **Public types:** no address, token, symbol, or instruction appears in a public type.
5. **Authority under failure:** dropped records cannot give a complete answer; worker death cannot
   erase resource ownership; a missing recorded answer cannot give an empty answer.
6. **Host-wide exclusion:** two supervisors contend for one lock; an ordinary game instance is
   refused and never terminated. A finished prior session does not block the next launch.

Do not assert private call sequences in consumer tests. Assert answers, gaps, side effects, and
errors.

## Alternatives not selected

- **One complete adapter per version/platform pair:** duplicates lifecycle, analysis, and binding
  knowledge.
- **A universal adapter with flags and version checks:** distributes the same decisions.
- **A hierarchy of version adapters with fallback overrides:** hides the effective behavior.
- **A declarative language for arbitrary native behavior:** moves branching into a second language.
- **Runtime plugin discovery and nearest-version fallback:** incompatible with exact-build support.
- **Evidence archive, replay, and qualification records:** removed by the
  [simplification decision](simplification.md). An answer is checked by running the question again.
- **A distributed Native supervisor executable:** consumers supply the process and call the
  library entry point.
