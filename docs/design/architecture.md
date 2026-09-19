# Native technical design: knowledge ownership and target composition

Status: implementation design supporting the [Native specification](../specs/native.md). The
[bounded replay foundation](replay.md) is implemented through the public interface and an isolated
evidence package. [Capability admission](admission.md) now implements exact-target composition and
qualification reporting. The public registry-query interface and maintainer capture methods share consumer-hosted supervision. Production admission is limited to the exact accepted registry qualification.
The full layout below describes what to build; it does not qualify another target.

## Design position

Native has one public engine interface and one place that assembles the implementation for an exact target. Internally, it separates operating-system process services, machine mechanisms, engine bindings, observation methods, and qualification. Established libraries supply executable parsing, disassembly, and serialization; Native owns the game-specific interpretation and qualification of their output.

Use composition to share established knowledge across targets. A new game build should normally add an exact-target description and qualification evidence, with binding or method changes only where the executable requires them. It should not require copying the preceding adapter or adding version tests to operation handlers.

Two principles govern the design:

- **Hunt & Thomas, DRY:** each piece of knowledge has one authority. Generated manifests, cached bindings, wire messages, and historical evidence may repeat its representation, but their source and validity must be explicit. Similar-looking code need not be combined when it represents different knowledge.
- **Meyer, Single Choice:** the module that knows a set of alternatives selects the behavior. Downstream modules receive the selected behavior or a refined value; they do not repeat the classification. This means one owner per distinction, not one enormous switch that knows every distinction in the project.

The public interface remains the primary test seam, as agreed for the specification. Internal composition makes that interface maintainable; it does not require Atlas to understand the composition.

## Proposed project layout

Keep the live runtime in one Cargo package as a library. Consumers supply supervisor executables; maintainer Cargo examples demonstrate integration. Add one evidence-only package for the concrete dependency constraint that replay must not depend on any live-launch code. This is not the former runtime-package split for binary entry points. Other knowledge ownership remains in Rust modules; further splits require an actual build, loading, dependency, or distribution constraint.

```text
Cargo.toml                         pdx-native package and workspace membership
src/
  lib.rs                           explicit exports of the supported interface
  supervisor.rs                    public consumer-hosted supervisor entry point
  registry.rs                      configured registry client, jobs, and results
  api/                             Engine, Job, semantic request/result types
  protocol/                        private owner/worker messages and handshake
  session/                         context binding, operation admission, job coordination
  binding.rs                       narrow bound interfaces; private composition subtree
  binding/
    compose.rs                     sole target implementation assembly point
    targets.rs                     catalogue lookup; no concrete host imports
    targets/
      records/                     exact-target data and recipe references
      recipes/                     host-neutral identifiers for required implementations
    groups/                        typed engine function/global/layout definitions
    platform.rs                    compile-time host selection; live strategy resolution
    platform/
      macos/                       macOS ownership/access and concrete live strategies
      windows/                     Windows ownership/access and concrete live strategies
    binary.rs                      thin object-crate integration and identity capture
    machine.rs                     decoder/call-mechanism resolution
    machine/
      arm64/                       qualified ARM64 normalization and call glue
      x86_64/                      qualified x64 normalization and call glue
  engine/
    analysis/                      bounded reader/owner/reference methods
    operations/                    normalized engine-level operations
  execution/
    supervisor.rs                  independent process/resource ownership
    instances.rs                   host-wide live-job exclusion and durable reservations
    worker.rs                      worker coordination; selected LLDB subprocess
  qualification/
    admission.rs                   matching tracked qualification authority to inputs
    records/                       accepted qualification and withdrawal records
  capture.rs                       live artifact writing using shared evidence types
  investigation/                   maintainer-only candidate plans and reports
  test_support.rs                  test-support feature; fixed synthetic engine scenarios
crates/
  native-evidence/                  no dependency on pdx-native or live execution
    Cargo.toml                     pdx-native-evidence package
    src/
      lib.rs                       recorded-data interface only
      records.rs                   shared evidence/observation types and format identity
      stream.rs                    pure sequence, ordering and terminal integrity rules
      replay.rs                    retained-data derivations; no live callbacks
      registry.rs                  registry snapshot derivation and opaque provenance
      store.rs                     artifact reads, hashes, restore and locator mapping
tests/
  contract.rs                      public request/result behavior
  replay.rs                        retained-evidence behavior through that interface
  live.rs                          explicitly configured live qualification
docs/
  specs/native.md                  product behavior and acceptance obligations
  design/architecture.md          this implementation design and ownership rules
  native/                         bounded historical findings and retrieval instructions
tools/                            maintainer evidence/qualification commands
.local/evidence/                   ignored raw captures and restored working material
```

The main library contains the shared live implementation. Consumers such as Atlas supply an
executable command with a dedicated role that calls `supervisor::serve`. Native starts and reaps that
consumer-owned executable for each query. Native also owns the protocol, reservation, target checks,
resource lifetime, and disposal. The consumer owns application scheduling and presentation. The
[registry consumer](live-observations.md) configures hosting once and asks `get_registry_items(name)`;
maintainer examples retain explicit pipe-based integration for investigation. No Native runtime
executable is distributed.

The connection checks the linked Native build identity, not a Native helper executable path.
The evidence package remains a pinned workspace dependency and must also be published/versioned
before any registry release of Native.

Use `feature.rs` as each module's entry file and `feature/` for its internal modules. Apply this convention throughout; the tree omits some entry files for brevity.

Folders describe responsibilities, not a requirement to create empty files now. Add operation families, architecture implementations, and helper libraries when an implemented operation needs them. If an injected library is later required, give it a separate build target because its loading requirements differ; reuse shared declarations or generate its bindings rather than duplicating them.

## Dependency rules

The direction of ordinary work is:

```text
Atlas → public Native interface → session coordination
                                  ├─ target composition → selected implementations
                                  ├─ qualification admission → qualified operation
                                  └─ operation execution → normalized evidence/results

Native → evidence package → retained records, hashes, pure derivations
         (no reverse dependency and no live execution dependencies)

Independent supervisor → owned game + worker + resource journal
```

The private `binding` subtree contains composition, target records, platform leaves, and machine leaves. Its root exposes only the bound interfaces and construction operations needed by session coordination, supervision, and engine operations. Shared operation logic receives bound interfaces and cannot name the concrete descendants. Platform services do not import game-build records. Binary and machine code does not import Atlas concepts or game-rule conclusions.

Enforce this with module privacy, not package checks. `binding::targets` is private to the `binding` subtree; its `records` child is private, and lookup functions use `pub(super)` or `pub(in crate::binding)` as needed. Platform leaves remain private children of `binding::platform`; only that module's resolver can name them. Shared operations live outside `binding`, so even `pub` items inside an inaccessible leaf cannot be imported through its path. Do not re-export raw descriptors or widen them to `pub(crate)` for convenience. Rust's restricted visibility must name an ancestor, so the module nesting is part of the enforcement design. [Rust visibility reference](https://doc.rust-lang.org/reference/visibility-and-privacy.html).

Only the composition/admission path combines target identity, recipe identity, operation requirements, and qualification. It constructs the private values that allow execution. Atlas cannot construct those values. Internal tools for investigating unqualified targets use a separate maintainer entry point and produce candidate evidence; ordinary extraction has no `allow_unqualified` option.

**Replay cannot depend on live execution.** The evidence package owns `evidence::replay` (Native's dependency alias), stream validation, and the recorded-data types shared with capture. It has no dependency on Native, `execution`, `binding`, platform leaves, debugger libraries, or the owner/worker protocol. Its interface accepts retained bytes/records or its concrete read-only artifact store, never an `Engine`, executable plan, or caller-supplied code callback. Native's replay entry delegates to this recorded-data interface; it must not open a live context first. Retained-data derivations used by both paths live in the evidence package rather than calling back into the live engine implementation.

Private modules alone cannot enforce that rule inside the main crate: replay could still call the crate's public `Engine` interface or supervisor entry points. The separate dependency graph makes imports of Native execution code from replay fail compilation. This is an architecture constraint, not an OS sandbox: forbid process/debugger launch code and unreviewed live-capable dependencies in the evidence package as well. Public-interface isolation tests still check that Native's replay adapter does not introduce launch behavior outside that package.

Keep internal interfaces narrow and specific. An operation that needs a bound reader and an observation channel should receive those, not a universal object exposing platform, version, addresses, and every engine service. The latter would make repeated selection convenient again.

## Where knowledge lives

| Knowledge or decision | Single authority | Legitimate derived representations |
| --- | --- | --- |
| Public request/result meaning | `api` definitions and their documented invariants | Public exports, reference documentation, contract fixtures |
| Owner/worker message shape | `protocol` definitions | Encoded messages; generated foreign-language bindings when needed |
| Recorded observation and evidence shape | Evidence package's `records` definitions | Native's explicit re-exports, serialized captures, and replay inputs |
| Installation locations and host prerequisites | Compile-time-selected `binding::platform` implementation | Installation candidates and diagnostic records |
| Exact executable/slice identity and candidate recipe | Target record under `binding/targets/records` | Catalogue index and target labels in run manifests |
| Executable-format parsing | Object library, integrated by `binding::binary` | Parsed image and symbols; Native separately qualifies game-specific fixups/joins |
| Instruction decoding and call lowering | Decoder library plus selected `binding::machine` mechanisms | Normalized instructions and prepared call machinery |
| Engine ABI, layouts, function/global bindings | Definitions under `binding/groups` | Bound addresses, typed accessors, generated hook declarations |
| Which implementations make one target's operation | Recipe under `binding/targets/recipes`, resolved by `compose` | Immutable bound operation set |
| What a reusable analysis method establishes | Method implementation and revision in `engine/analysis` | Normalized observations and method provenance |
| What qualification has been accepted or withdrawn | Qualification records and admission rules | Capability report and generated supported-target matrix |
| Whether this attempt activated and completed observations | Validated event stream in `evidence/stream` | Completion report, summaries, replay result |
| Which live resources belong to a job and whether they were disposed | Independent supervisor's resource journal | Disposal report and retained cleanup evidence |
| Reservation journal format and compatibility | `execution::instances` versioned parser/writer | Versioned journal entries; unknown/unreadable entries block launch |
| Whether any Native owner may launch a game on this host | `execution::instances` reservation, held by the independent supervisor | Busy/conflict results and durable reservation journal |
| Evidence identity and artifact content | Sealed capture manifest and hashed artifacts | Storage locators, caches, compact summaries |
| What observations establish about a game rule | Atlas extraction | Atlas claims, rule documentation, and snapshots |

Store an engine offset or function declaration once in its binding definition. An observer, analyzer, and runtime caller refer to that definition rather than each defining a constant. A target record selects the binding definition; it does not copy all its fields. A capture embeds the resolved values and identities as historical evidence, not as another table to edit during the next port.

Reuse requires evidence that knowledge is the same. Two targets can point to the same string-layout implementation after qualification. Two equal offsets on unrelated targets may remain separate bindings because equality of numbers does not establish shared meaning. A common type shape is not sufficient reason to merge two semantic observations.

Qualification records and target recipes answer different questions: **what could run** and **what is qualified to run**. Do not put a second `supported: true` flag in the recipe. The supported-target report is generated by joining registered recipes with accepted, applicable qualification records.

## Single Choice at each decision point

| Distinction | Decided by | Passed onward | Not passed as a selector |
| --- | --- | --- | --- |
| Host operating system | Compile-time selection at `binding::platform` entry | Host services and its strategy resolver | Runtime OS switch, `is_windows`, `is_macos` |
| Executable format | Object library when opening executable bytes | Parsed image through thin `binding::binary` integration | Native's own parallel Mach-O/PE parser hierarchy |
| Process architecture | Machine factory using the selected executable slice/process | Decoder/call mechanisms | Architecture strings in operation handlers |
| Exact game build | Target catalogue after identity capture | Selected target record and recipe | Semantic-version comparisons |
| Native variation for an operation | Target composition | Bound operation implementation and prerequisites | `legacy`, `new_layout`, or build booleans |
| Qualification for a request's bounds | Admission using accepted evidence and current inputs | Private admitted operation, or a precise unavailable result | A capability flag that callers reinterpret |
| Live capture versus replay | Session creation | Execution or replay implementation with explicit origin | Repeated `if replay` inside native methods |
| Failure/record completeness | Event and ownership reducers | Structured result dimensions | Success inferred separately by several callers |

These choices happen once per relevant lifetime. Each process constructs the host services compiled into its binary; it does not choose an operating system at runtime. A target is bound per engine context, and request-specific bounds are admitted per operation. Checking that a bound executable is unchanged before launch is an integrity check, not a second adapter selection. Parsing received data again in another process is also a required trust-boundary check; it uses the same rules rather than inventing another selector.

Use compile-time platform selection at the platform module entry and for unavoidable foreign-function declarations. Keep platform-specific dependencies and entry points in those leaves. Do not scatter `cfg` sections through session, evidence, and engine-operation logic.

Use exhaustive enums for genuinely closed data, such as observed stages or terminal event kinds. Matching a decoded instruction in the architecture decoder is its job. Repeated conditions are a problem when several modules independently classify the same target to choose behavior, not merely because the source contains several `match` expressions.

## Established primitives before new abstractions

Use established libraries behind narrow integration functions. These are dependency candidates to pin and verify against retained cases when implementation begins, not dependencies added by this document:

| Concern | Starting choice | Native still owns |
| --- | --- | --- |
| Executable files | [`object`](https://docs.rs/object/latest/object/), which provides common and lower-level Mach-O/PE readers | Exact identity, slice selection, and qualification of game-specific symbol/fixup interpretations |
| Disassembly | [`capstone`](https://docs.rs/capstone/latest/capstone/), with ARM64/x86 support evaluated against method controls | Instruction normalization, value tracking, bounded pattern semantics, and qualification; account for its native C dependency |
| Serialization | [Serde](https://serde.rs/) and an appropriate format implementation, initially JSON for inspectable records | Contract shape/versioning, completeness rules, bounds, and evidence identity |

There is no separate pluggable `image` axis now. In the initial supported scope format and platform are paired, and the library already provides the useful abstraction. `binding::binary` adds only the identity and reader operations actually required. Promote a separate interface only if a concrete operation needs independent substitution that the library does not cover. Library support for a file format or instruction does not qualify a Native operation.

## Composing the version × platform cases

A displayed game version is a label, not the composition key. The lookup key must identify the actual executable and selected architecture slice. Relevant content and runtime prerequisites further limit operation qualification. Keep these distinct instead of treating `4.5 + macOS` as sufficient identity.

Represent support as a **sparse set of exact-target compositions**. There is no default Cartesian product of versions, platforms, methods, and operations. Registering a shared method does not qualify it on every target that could call it.

Each target record refers to an explicit recipe. Both are ordinary host-neutral Rust data: architecture, format, binding-group references, and typed strategy/method identifiers, with no references to concrete platform types or function pointers. The full catalogue, including Windows records, compiles on a Mac. Records do not use `cfg`.

`binding::compose` performs the missing resolution step: look up the record and recipe, resolve its binding groups and machine methods, and ask the compiled host's strategy resolver to turn a strategy identifier into an executable implementation. `binding::platform` selects one host implementation at compile time. That implementation owns the exhaustive mapping from known live-strategy identifiers to available implementations or an explicit host/implementation-unavailable result. The common composer does not import platform leaves or repeat that mapping. New target records can reuse existing identifiers without changing either resolver.

Catalogue lookup, static inspection, and live execution are distinct. A Mac can identify a Windows target and inspect its metadata without loading Windows code. Resolving a Windows live strategy on Mac returns host-unavailable; it does not select a fallback. Cross-host static analysis is available only where the required decoder/method and its qualification exist. The composition module rejects no match, multiple matches, missing prerequisites, or incompatible parts; catalogue ordering never breaks a tie.

Share the largest proven unit of knowledge:

- Process creation and disposal mechanisms are normally shared by host platform.
- Instruction handling is shared by architecture where its method limits permit.
- Calling conventions can depend on both platform and architecture. Select their qualified implementation explicitly; an instruction set alone does not define the engine ABI.
- Engine layout/call bindings are shared by an explicitly established compatibility family, or remain exact-target bindings when equivalence is unproved.
- Analysis methods are shared by demonstrated compiler/reader behavior, not by version-number ranges.
- Launch/hook strategies can depend on platform, target, and requested phase. For example, ready-world access and before-parsing access are separate capabilities even on one executable.

The target recipe is the one place where these dependencies are composed. An irreducible version/platform interaction belongs in a concrete strategy selected there. It does not belong in several shared operations as the same compound condition.

Avoid adapter inheritance such as “4.5 extends 4.4.6 except for these fields.” Prefer immutable binding groups referenced explicitly by a complete recipe. A recipe may reuse an unchanged group and replace a changed group, but there is no chain of fallback overrides whose effective behavior requires reading many versions. Derive a flattened composition report for review.

The retained Mac and Windows experiments justify separating process, machine, binding, and observation concerns. They do not qualify the proposed combinations. In particular, historical Windows ready-world access supplies no before-parsing guarantee, and older Mac ready-world bindings cannot be substituted into the later observation target. Use the [target records](../native/targets.md) and their verified bundles when implementing a composition.

### M45-observe data sketch

This illustrates the concrete record → recipe → binding groups → strategy references. Types and lookup boilerplate are abbreviated. The two executable hashes are from the retained target record; the symbolic group/method identifiers below are proposed names, not existing bindings. Exact native declarations must be ported once from verified evidence into `binding/groups`, not reconstructed from this example. This candidate does not include an accepted qualification record.

```rust
const M45_OBSERVE: TargetRecord = TargetRecord {
    label: "M45-observe",
    executable_sha256: "3d4c8a7046d87175ce7e3b513b1a2ce589050d654d332744518a49d13ac82216",
    slice_sha256: "1e0c9aec45650272fcaecba2eb47f8dce8f17bc08ef2b992be18c99ae098c623",
    architecture: Architecture::Arm64,
    format: BinaryFormat::MachO,
    recipe: &M45_EARLY_OBSERVATIONS,
};

const M45_EARLY_OBSERVATIONS: Recipe = Recipe {
    bindings: &[
        BindingGroupId::M45ObserveRegistrationEntries,
        BindingGroupId::M45ObserveCategoryReader,
    ],
    operations: &[OperationRecipe {
        operation: OperationId::EarlyReadEntries,
        method: MethodId::BoundedRegistrationAndCategoryReads,
        strategy: LiveStrategyId::MacSuspendedChildLoaderEntry,
    }],
};

const M45_CATEGORY_READER: BindingGroupDescriptor = BindingGroupDescriptor {
    id: BindingGroupId::M45ObserveCategoryReader,
    members: &[
        BindingKey::CategoryLoadEntry,
        BindingKey::CategoryMemberReadEntry,
        BindingKey::ReaderSourceLocation,
        BindingKey::CategoryOwnerArgument,
    ],
};

const M45_REGISTRATION_ENTRIES: BindingGroupDescriptor = BindingGroupDescriptor {
    id: BindingGroupId::M45ObserveRegistrationEntries,
    members: &[BindingKey::EffectRegistrationEntry],
};

const MAC_LOADER_ENTRY: StrategyRequirements = StrategyRequirements {
    id: LiveStrategyId::MacSuspendedChildLoaderEntry,
    ownership: OwnershipRequirement::IndependentDirectParent,
    activation: ActivationRequirement::VerifiedLoaderEntryBeforeResume,
    completion: CompletionRequirement::ContinuousSequenceAndTerminalTotals,
    disposal: DisposalRequirement::ExitAndReaping,
};
```

Each binding group owns its corresponding concrete declarations and revision identity. `BindingKey` names a role within that group; it is not a public request to call any address. Method qualification supplies the permitted observation window and fixture/content bounds, rather than making the historical three-entry/two-field experiment a universal operation guarantee. Capture records the selected revisions automatically.

On Mac, the compiled resolver recognizes `MacSuspendedChildLoaderEntry`. Its concrete strategy implementation owns the debugger backend, fixed for that strategy revision at release time. Recipes contain no second debugger selector. Until that implementation exists, the resolver returns implementation-unavailable. On Windows the same recipe is readable, but its live strategy is host-unavailable. A Windows recipe can instead name its own qualified strategy without a Windows type appearing in the catalogue. Availability and accepted qualification are both required before constructing an executable operation. Replacing a strategy's debugger backend changes its implementation revision and requires affected qualification to be re-established; it does not require editing every recipe that uses it.

## Binding once, executing without target tests

An engine context is created in five steps:

1. Discover or accept an installation location. Native reads the executable identity and selects the actual slice; Atlas supplies no build or architecture selector.
2. Look up an exact target record. Build an immutable candidate composition from its explicit recipe. Target discovery and recipe availability do not grant execution permission.
3. Resolve and verify required symbols, patterns, globals, and layouts through their owning modules. A pattern match remains a candidate until its required qualification and validation conditions hold.
4. Admit implemented operations against applicable qualification records, method/binding revisions, content dependencies, and request bounds. Construct operation-specific bound values with private constructors. Missing qualifications remain explicit capability gaps.
5. Return the public engine context with opaque identity and normalized capability results. Keep target descriptors and native bindings private.

Later operations dispatch through the bound operation set. They do not take a build enum, inspect a version string, or ask a layout object which version it represents. A bound field observer already owns the appropriate field accessor, hook plan, source join, and method revision. A bound reference analyzer already owns the qualified reader and call summaries it needs.

An operation registry may use typed slots for the currently implemented operations. Prefer concrete types or a small private trait where behavior demonstrably varies. Do not build a generic plugin system, self-registering global factories, or a string-to-`Any` service locator. Adding a new operation may require an explicit registry entry and a public contract extension; adding another target should not require modifying shared operation dispatch.

Stable choices are bound once. Facts that can change are checked at the proper lifetime: installation/content integrity before execution, object liveness before access, and observation ordering during capture. Never cache an access check or liveness result as if target qualification made it permanent.

## Process and state ownership

The caller holds an engine context and job handles. The independent owner process holds the actual game ownership resources, cleanup budget, cancellation state, and resource journal. The observation worker holds transient debugger/analysis state. Worker failure must not remove the owner's disposal capability.

The owner constructs the host services compiled into its executable and validates the pinned composition identity passed by the controller. The worker must accept that same composition identity or fail; it must not search for a different compatible target after starting. Sharing a source definition across processes does not mean trusting arbitrary serialized pointers or capability assertions. Resolve process-local addresses from the pinned definitions in the process where they are meaningful.

`execution::instances` owns the host-wide live-instance namespace. Initially permit one Native-owned Stellaris game per host, conservatively covering all installations. Every supervisor uses the same protected namespace, independent of workspace, engine context, caller PID, and Native release. Platform code supplies an OS-backed exclusive lock; the supervisor acquires it before checking for conflicting game processes or allocating game resources and holds it through confirmed disposal. The worker never inherits the lock handle. A competing owner returns busy rather than relying on a per-context semaphore or racing a process-list check.

Pair the lock with a durable reservation journal containing attempt identity, owner/game process identities with incarnation information, resource locations, and reserved/disposed state. Write the reservation before spawning the game. Caller loss leaves the supervisor, lock, and journal intact. If the supervisor dies, the OS may release the lock, but its unresolved journal still blocks another launch. Never clear that reservation from PID absence, lock availability, or elapsed time alone. Automatic orphan recovery remains unqualified; unresolved ownership requires explicit operator-assisted verification and clearance. Mark disposal durably before releasing the reservation.

Every journal entry carries an explicit format version in its envelope. While holding the namespace lock, each supervisor must parse and understand all relevant entries before admitting a launch. Unknown versions, unreadable/truncated records, unknown states, and permission failures block launch, including when an older release encounters a newer record. Never treat an unrecognized entry as absent or disposed, and never overwrite it to make progress. Use an atomic, durable write protocol; leftover incomplete records remain blocking evidence. A format migration must preserve unresolved reservations under the same lock. Until such a migration is implemented and verified, require a supervisor that understands the existing format for inspection and clearance. Changing the format must not create a new namespace that bypasses reservations from older releases.

The namespace must cover all supported Native launchers on the machine, not be hidden under a user's checkout. The platform implementation must establish the required sharing/access permissions or return an unavailable isolation prerequisite. This does not claim cross-user execution support. Ordinary game launchers do not honor Native's lock: perform a conflicting-instance check under the reservation before launch and observe conflicts during the job. Inability to inspect relevant processes prevents a claim of established isolation. An external launch detected during a job invalidates the affected isolation evidence; never kill that unowned game. The lock serializes Native owners, not arbitrary external software.

| Lifetime | Owned state |
| --- | --- |
| Native release | Contract definitions, catalogue entries, recipes, method/binding revisions |
| Host-wide live-instance namespace | Exclusive launch reservation and durable unresolved-ownership journal; held by supervisor, survives caller/context loss |
| Engine context | Exact target binding, selected operation set, evidence context, applicability data |
| Job/attempt | Fixture/content snapshot, deadline, unique identity, admitted plan, event sequence |
| Supervisor | Owned process handles, private profile/resource journal, disposal evidence |
| Worker | Current hooks, decoding/debugger state, transient observations |
| Retained capture | Immutable executed-input manifest, raw records, terminal and disposal witnesses |

Activation, observation completion, and disposal each have one producer of truth. A stream validator derives activation/completion from the declared witnesses; the supervisor supplies disposal facts. The public result combines them without recomputing them from exit codes or empty logs. Replay uses the same validation rules against retained records, with its replay origin explicit.

Closing a job requests bounded cleanup from the supervisor. A Rust destructor may provide best-effort signaling, but it cannot stand in for awaited, confirmed disposal. A broken control channel produces an explicit uncertain result; it does not imply the game exited.

## Debugger-worker integration decision

SDK-515 selects an LLDB subprocess with embedded Python callbacks for the
`MacSuspendedChildLoaderEntry` strategy. The independent owner remains the game's direct
parent. The backend is fixed by the strategy implementation revision at release time;
target recipes and Atlas requests gain no debugger selector.

The [integration decision](debugger-worker.md) records the alternatives, process tree,
file-journal transport, handshake, tool discovery, packaging, generated Python protocol
binding requirement, cancellation limits and fresh four-control trial. Private `protocol`
remains the authority for production wire meaning. The Rust owner can spawn LLDB directly;
it must not duplicate Python breakpoint semantics in Rust.

The trial reuses the retained Python parent to prove the external worker boundary. It does
not qualify a Rust parent or production live adapter. Production supervision, generated
bindings, packaging and qualification remain implementation work; the initial catalogue
still admits no live operation.

## Evidence and generated knowledge

Keep four identities separate: a method/binding revision, a target composition, an accepted qualification record, and an individual capture. A new capture does not silently change a qualification record. A method edit invalidates affected qualification unless its applicability is explicitly re-established.

Qualification records reference the exact target, recipe/binding/method identities, operation bounds, relevant content dependencies, controls, and retained evidence. Admission computes the applicable result from those references and any withdrawal. Capability reports and documentation are generated views of this relation. Maintainers accept qualification through one recorded promotion path; an experimental capture cannot enable ordinary execution by editing an unrelated flag.

**Tracked, maintainer-accepted qualification records in the pinned Native release are the authority for admission. Raw evidence is their provenance, not a required runtime input.** Admission reads the bundled records and withdrawals, verifies their applicability to actual inputs and the implemented method, and checks present runtime prerequisites. It does not open or fetch the historical private bundle. A hash reference identifies the evidence reviewed at promotion; the hash alone is not a new proof of qualification. Untrusted capture files cannot add admission records.

Evidence bytes are required to establish/review a qualification, audit it, or replay the retained derivation. Missing bytes return evidence-unavailable for those operations and block new promotion that depends on them. They do not silently erase previously accepted records from a pinned release. If an audit invalidates an acceptance, publish a withdrawal/correction through the same authority; older offline pins cannot know a withdrawal they have not received. Runtime executables, fixtures, scripts, and other current inputs remain required regardless of historical evidence availability.

A clean checkout can run contract, resolver, and admission-policy tests using small tracked synthetic or redistributable fixtures. Synthetic qualification fixtures stay test-only and cannot enter the production catalogue. Private replay tests explicitly require restored bundles and report that prerequisite when absent; live tests require a real qualified installation. There are no accepted production records merely because the historical prototype succeeded, so the initial scaffold admits no live operations until the implementation is qualified. This is distinct from requiring every user to download private evidence before any admitted operation can run.

### Test-only admission construction

Use a non-default Cargo feature named `test-support`. Only when that feature is compiled does the library export the doc-hidden factory `test_support::engine(SyntheticCase)`. Integration tests select a named, tracked scenario through that factory, then exercise the returned engine through the ordinary public interface. The factory supplies fixed synthetic qualification records to the same admission-policy evaluator used by production, paired with an in-memory execution implementation. It accepts neither arbitrary qualification files nor a real installation path.

The synthetic context has an unforgeable internal origin tag and cannot be rebound to a live target. Its admitted plans are accepted only by the simulator; live composition rejects them. Results retain synthetic origin and cannot serve as native qualification evidence. Synthetic scenarios and the factory are absent when `test-support` is disabled; production admission continues to obtain records only from its bundled authority.

Official release builds enable a `production` feature. A compile-time guard rejects `production` combined with `test-support` or `maintainer-tools`; release packaging also verifies the resolved feature set and helper identities. The build script additionally rejects `test-support` for release-profile builds, so `cargo build --release --features test-support` cannot produce a test-enabled release artifact. Contract tests run in the test profile with `test-support`, and default-build checks prove that the factory cannot be imported without it. Explicitly enabling the feature creates a test build, not an access-control boundary against someone compiling modified source.

### Maintainer investigation entry point

Expose candidate investigation through a `maintainer-tools`-gated library module. A Cargo example demonstrates the consumer-supplied controller and supervisor roles. Ordinary supervisor entry points retain admission requirements and gain no bypass switch. Default and production artifacts omit candidate investigation; production refuses its feature.

The tool lives in `investigation` and requests an `InvestigationPlan` from the shared composer. This path resolves candidate bindings and strategies without treating them as qualified. It can perform bounded native experiments through the same independent supervisor, host reservation, identity checks, and disposal machinery. Qualification exemption does not exempt process ownership or integrity checks.

The tool emits only `CandidateCapture` artifacts and an `InvestigationReport`, with explicit unqualified origin and limitations. It emits recorded observation artifacts for public offline replay, while preserving their unqualified origin.
It cannot construct `AdmittedOperation`, `Engine`, or supported public live operation results from a candidate plan: their constructors remain private to the admission/session path, and there is no conversion from investigation types. Candidate requests use a distinct gated protocol mode; ordinary helpers reject that mode, and all participating processes must link the same Native build identity. Shared supervision does not merge the two result authorities.

Promotion is a separate reviewed change to tracked qualification records after verifying the evidence bytes. The investigation API cannot update the bundled authority, mint an acceptance record, or return a supported capability merely because an experiment succeeded. This explicitly places the unqualified investigation path without opening it to Atlas or weakening normal admission.

Generate or serialize secondary representations from their authority:

- Wire definitions originate in shared protocol types. If a debugger worker requires another language, generate and check its message bindings; do not maintain parallel schemas manually.
- Runtime hook code, static analysis, and argument decoding refer to the same typed binding declaration. Language-specific calling glue can differ while the declaration and its provenance remain shared.
- Capture manifests record the actual resolved composition, never a second hand-maintained approximation of it.
- A supported-target table is generated from admitted qualification records. Historical capability pages remain experiment records and link to current implementation evidence.
- Caches key on every input the cached conclusion depends on, including executable/slice, method/binding revision, and relevant content. A cache miss does not invoke a weaker fallback. Fresh session admission and live-state checks still apply.

Keep historical evidence immutable. Repeated target identities and resolved values in old captures are intentional records of what ran. They must not be rewritten when authoritative implementation definitions change. Derived output states its source identities and can be rebuilt; relocatable storage mappings can change without changing evidence identity.

## Change examples and locality checks

| Change | Expected edits | Edits that indicate knowledge has spread |
| --- | --- | --- |
| New exact build using established bindings/methods | New target record; reference existing recipe where valid; qualification records and evidence | Version checks in session or operation handlers |
| One field offset changes | New/revised binding group; affected recipe reference; affected qualification | Repeating the offset in observer, analyzer, and worker |
| Calling convention differs | Selected call-mechanism implementation plus typed bindings and qualification | Platform checks around each engine call |
| Windows ownership defect | Windows ownership implementation and lifecycle controls; requalify affected compositions | Separate fixes in each game-version adapter |
| New reader/compiler pattern | Bounded shared method and controls; explicit use and qualification on targets | Command-specific answers in Atlas or a universal fallback decoder |
| One target needs a different early-hook strategy | Concrete execution strategy; one recipe change and early-phase qualification | `if build == ...` in every observation operation |
| Runtime access loses a prerequisite | Admission/operation result with precise reason | A different adapter selected silently during the job |
| A rule conclusion changes | Atlas evidence interpretation and claims; Native only if its observation was wrong | Native assembling a corrective rule table |

The useful maintenance question is: **which authoritative decision changed?** Editing several generated artifacts or adding required qualification evidence is not a DRY failure. Editing several independent implementations of the same decision is.

## Verification and enforcement

Continue to test the consumer-visible behavior through the public Native interface. Use qualified live adapters for live guarantees and verified replay for retained derivations. Internal tests supplement that seam for selection and unsafe primitives where precise failure injection is needed.

Add these architecture checks as implementation arrives:

1. **Catalogue consistency:** exact-target keys are unique; recipe references resolve; each operation has one implementation per composition; incompatible format/machine/call bindings fail assembly. Compile the complete data catalogue on both hosts; resolving the opposite host's live strategy must return unavailable. Deliberately introduce duplicate and missing entries to prove the checks fail.
2. **Qualification derivation:** qualification outside the exact method/target/content bounds produces no admitted operation. An unqualified operation stays unavailable even when a candidate recipe exists. With test-only accepted records, removing private evidence leaves admission-policy tests unchanged but causes retained replay to report evidence-unavailable. Withdrawal removes affected admission while preserving historical captures.
3. **Caller independence:** the same Atlas request flow exercises two real target compositions when available. A replay or synthetic provider establishes dispatch behavior only, not native portability.
4. **Change locality:** add a synthetic target that reuses existing methods. Shared operation source must remain unchanged. Test a binding variation and demonstrate that all users obtain it from the one binding authority.
5. **Dependency checks:** shared engine operations cannot import `binding::targets::records` or concrete platform/machine leaves. Private modules and `pub(super)`/`pub(in crate::binding)` enforce this within the main crate. Compile-fail controls attempt those imports from `engine::operations`; normal use of bound interfaces must compile. In the evidence package, compile-fail controls attempt imports of Native's `Engine`, `execution`, supervisor entry points, and platform leaves from replay. Enforce a dependency allowlist with no path back to live code, and reject process/debugger launching code there. The public replay adapter also retains a no-launch behavioral check. Package checks enforce the separate evidence dependency graph, not module visibility inside Native. Separately check that Atlas uses only supported exports, excluding maintainer/test entry points. A text search alone cannot prove knowledge ownership.
6. **Protocol consistency:** shared types serialize and round-trip with contract identity intact. Reject a mismatched helper artifact or protocol revision. If foreign bindings exist, verify generation leaves no diff.
7. **Authority under failure:** dropped records cannot produce completion; worker death cannot erase resource ownership; missing evidence cannot produce a supported empty result. Retain the existing activation/completion/disposal controls.
8. **Host-wide exclusion:** two owner processes launched from different contexts/checkouts contend for the same reservation. Caller death does not release it; supervisor death leaves an unresolved journal that blocks new launch even when the OS lock becomes available. Older readers reject newer journal versions, unknown states, corrupt/truncated records, and permission failures without overwriting them or launching. Conflicting ordinary game instances are rejected without being terminated. Include ambiguous process identity as an unavailable-isolation case.
9. **Build-mode separation:** integration tests reach synthetic admission only through the gated factory. Default builds cannot import it; production/test-support and production/maintainer-tools combinations fail to compile; release-profile test-support builds fail. Synthetic engines cannot reach live strategies. The maintainer library API produces only candidate artifacts, cannot convert a candidate into an admitted operation/public live result, and ordinary helpers reject its protocol mode.

Do not assert private call sequences in consumer tests. Assert observations, limits, side effects, admitted/unavailable behavior, and retained evidence. A refactor that preserves those results should not require rewriting Atlas tests.

## Implementation order

1. Add the library and consumer-hosted supervisor entry points to the existing package, with the small public interface required by the first consumer path. Establish shared recorded-data types and pure replay in the isolated evidence package. Add the test-support factory and release feature guards; replace the scaffold entry point without creating a general plugin framework.
2. Implement exact-target identification, one explicit composition, qualification admission, and unavailable outcomes. Place bounded unqualified investigation in the separate gated library module and candidate-type path. Keep catalogue registration and candidate capture distinct from support.
3. Resolve the debugger-worker integration decision. Implement the independent supervisor, host-wide reservation, and bounded early-observation path for the first qualified target. Bind platform, machine, engine definitions, and the chosen debugger strategy through composition; qualify the resulting implementation before promotion.
4. Add verified capture/replay and Atlas's bounded tradition request flow through the same public interface. Preserve partial results and refusal of unsupported completeness claims.
5. Introduce the second concrete target/strategy from the portability work. Share only proven common knowledge; use that implementation to assess whether private interfaces hide the actual differences.
6. Add further operations by following existing evidence and Atlas obligations. Extend an owning module or the composition recipe instead of adding selectors to unrelated modules.

Implement only the first required variants initially. The retained experiments establish that variation exists, but do not justify prebuilding every folder or claiming a reusable interface has already passed portability qualification.

## Alternatives not selected

- **One complete adapter per version/platform pair:** simple initially, but duplicates lifecycle, analysis, and binding knowledge. Use exact-target recipes that compose shared implementations instead.
- **A universal adapter with flags and version checks:** reduces file count while distributing the same decisions. Bind behavior at composition and admission instead.
- **A hierarchy of version adapters with fallback overrides:** hides the effective behavior and makes qualification dependencies hard to review. Use explicit immutable binding groups and complete compositions.
- **A declarative language for arbitrary native behavior:** moves branching into a second programming language. Keep selection data declarative and nontrivial behavior in typed Rust implementations.
- **Runtime plugin discovery and automatic nearest-version fallback:** unnecessary for the initial source-built product and incompatible with exact-target qualification. Use explicit registration and fail closed on unknown combinations.
- **One crate per platform, method, or operation:** adds package coordination without necessarily adding information hiding. Start with modules; split a crate when a build, loading, dependency, or distribution constraint requires it.
- **A distributed Native supervisor executable:** consumers supply the process and call the library entry point. The evidence package is a different, concrete dependency constraint: replay cannot import the public or private live runtime. Its packaging cost is explicit.

The specification remains authoritative for product scope, supported-target policy, Atlas ownership, and release gates. This design owns the proposed source layout and placement of implementation decisions. Concrete method and qualification records will own executable support; neither document is an alternate offset table or support registry.
