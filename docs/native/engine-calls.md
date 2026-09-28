# Engine calls, memory, and live identity

## Calling boundary and addresses

On M45-old, the injected ARM64 bridge polls a serial file mailbox from the main dispatch queue. It acts only on the main thread, with run/attempt identity, recursion protection, and the qualified normal `UpdateInput` → `UpdateOneFrame` stack return addresses. It locates the `MH_EXECUTE` image and adds its dyld slide to unslid native addresses. An early assumption that image zero was the game failed because it was the inserted library.

Use the exact function declarations in `sdk-testing/sdk-testing/prototype/compatibility-harness/apple-silicon/native/src/{engine,prepared,locators,hooks}.rs` and `platform.rs`. These `extern "C"` declarations and retained assembly are the experimental calling convention source, not a new offset table. The game's ARM64 structure return uses **x8**: the trampoline sets x8 to the aligned output buffer before branching. CString temporary results use a 16-byte-aligned 64-byte buffer and the engine destructor; an initial 32-byte assumption was insufficient. Heap-backed and inline strings were both exercised. The retained string reader checks the sign bit of byte 23 before choosing indirect storage. This is an exact-build layout, not a public C++ ABI guarantee.

The x64 Windows source uses the Windows calling convention with explicit receiver arguments, exact-build RVA plus image base, engine-owned string/scope lifetimes and prologue-pinned hooks. See `windows/native/bridge.cpp`, `windows-446/native/bridge.cpp` and their `pins.hpp`. A signature/layout inferred from Mac naming is a candidate until fresh Windows calls and independent witnesses qualify it. NativeScope and NativeString source define storage/alignment; keep their constructor/destructor pairs intact.

## Demonstrated operations

| Operation | Evidence and boundary |
| --- | --- |
| Actual player/date/pause/AI | Ready-world native reads plus fixture markers and game saves; player is read from session, not hardcoded to country zero |
| Parsed conditions and immediate effects | Engine definitions and scope kind checked; native predicate result and invocation-correlated effect markers, with independent script/save witnesses |
| Explicit time | `JumpToNextDay` and synchronous `FastForwardDays`; exact requested date changes and pause preserved in recorded scenarios |
| Event option selection | Actual pending event identity, named key, engine visibility/validity/exclusive predicates, then normal `PostEventOptionSelection` command path |
| Resource reads | Local human energy/minerals via `CCountry::GetResource`, signed 64-bit CFixedPoint at scale 100000; later expanded stockpile evidence is retained separately |
| Save witness | Actual asynchronous engine save request, completion/pending state plus ZIP structure and independent parser checks |

Posting an event choice is **acceptance**, not completion: successful immediate replies still showed it pending. Later engine logs, pending-event removal, selected effects, `after` and dated follow-ups establish completion. Text logs suppress repeated messages and cannot count repeated invocations. The common scenario uses native invocation markers correlated with world, phase, invocation and script digest; omitted/stale markers prevent completion even if native execution returns.

SDK-441 strengthens recursive fire identity on M45-old: entry/return hooks on trigger, pending insertion, immediate and after paths maintain a native parent/child call tree. The exact pending insertion is attached to the outer invocation frame and verified against the live instance; neither list order, before/after set difference nor counter-plus-one supplies identity. Three-level same-definition recursion, repeated firing, older matches and independent FROM/option/after witnesses pass. Hidden completed handles require balanced native return/after evidence with no outer insertion. A visible option-free event and the tested direct `auto_select` boundary remain pending. Uncertain post-mutation identity/completion returns no success handle or retry. Two complete fresh-process runs and 651 original artifact checks are retained in `linear-supplement`, document `native-recursive-event-identity-and-player-country-coverage-a90f1f25ef05` and asset `824630f8-c877-4ce1-8b3f-a113bd298ae5`.

SDK-439's separate shared-suite experiment integrates these native mechanisms with fixed prepared scripts and invocation-scoped log-effect hooks. It demonstrates repeated condition/effect/wait identities, historical captured observations and independent cleanup under intentional failures; it remains a temporary testing consumer. Native retains the hook/context/protocol evidence, while test API and result policy remain with the paused testing project. The source archive `linear-supplement/assets/b1a98c0f-224d-4aef-b681-8394911392e9` preserves its source and raw matrices. Its document and SDK-439 acceptance comments are available offline beside it.

Windows required native RNG permission matching the console-event context, restored after every request. Omitting it caused forbidden-RNG diagnostics in early successful-behavior runs. Scope construction itself uses RNG; boolean queries are not promised to preserve RNG state. Main-thread calls alone do not establish every engine context is valid.

The resource probe retains raw fixed-point integers and scale as decimal strings and compares with integer arithmetic. A 0.00001 mineral increment survives; 0.000009 energy input truncated at parser precision. That is input quantization, not read rounding. Ordered resource groups explicitly do not promise an atomic snapshot. Paused samples with equal date do not prove cross-subsystem atomicity. Broader ranges, negative stockpiles and arbitrary native queries remain unqualified.

## Live object and registry memory

SDK-442's final Mac locator experiment joins world identity, full database ID, native address and permanently observed destruction. Unique locators scan actual live country/planet entries, construct an engine scope, call the parsed native condition, and reject zero/nonunique/wrong-kind results. Global targets are canonicalized through actual planet accessors: a colony scope is not assumed to be its planet ID. Exact saved-ID acquisition requires the declared fixture hash, kind, initial phase and engine validation witness.

A binding retains the acquired object rather than rerunning its locator. Capital changes, reassigned targets, colonization and decolonization do not silently retarget it. Full ID and pointer equality alone cannot prevent complete reuse. Qualified country/planet destructor hooks retire prior bindings permanently before the original destructor executes.

SDK-442 separately demonstrated **complete ID and address reuse** for both object kinds on M45-old. Later SDK-447/Windows common scenarios demonstrate replacement with changed full IDs; their short runs do not independently repeat complete wraparound. Do not substitute their weaker coverage for SDK-442. Destruction requests leave objects present while paused; after an explicit day, native destruction and game-written save absence agree.

Failed approaches remain useful: `every_galaxy_planet` did not see a new live paused planet until a day advanced; direct live-table/native-condition acquisition saw it immediately. `set_owner` alone created an empty colony and failed the colony predicate; adding actual population established the final fixture. A pending byte mislabeled “observer” was replaced by the real `IsObserverEvent()` function.

Evidence: `sdk-testing/sdk-testing/scratch/{native-bridge-probe,rust-bridge-probe,resource-read-probe,time-control-probe}/REPORT.md`; `linear-records/linear/doc-country-and-planet-locator-identity-native-evidence-753293d05d90.json` and asset `83ad732d-c67f-4dd0-aeae-a6ed3c416856`; script/stockpile assets `9023851d-51f5-4803-aa1d-51008f79490e` and `8f8f3d75-c08a-4eab-b122-1ce0e8c794bb`. Relevant native source snapshots and binaries are private, retained with original manifests and licenses.

Prerequisites for fresh runs are the exact historical executable, compatible save/content, host tools and engine context. No M45-observe ready-world ABI qualification follows from M45-old addresses. Multiplayer, arbitrary scopes, concurrent callbacks, universal UI operations and production lifetime guarantees remain outside these bounded experiments.

## In-process parse probes (M45-release)

A spike on 2026-09-28 parsed trigger text inside the paused game, with no new launch per probe.
It applies only to the M45-release executable in [targets](targets.md). The worker patch, probe
scripts and results are in `.local/evidence/in-process-probe-2026-09-28/`; the patch is on branch
`spike/in-process-probe`, not in `main`.

**Route.** The debug console's `trigger_file` and `effect` commands parse text through
`ReadAndEvaluateTrigger` and `ReadAndExecuteEffect`. The probe repeats the trigger route up to
validation: `CString(char const*)` → `CBlob::Append` → `CMemoryFile(blob, 1, 0, false)` →
`CTextLexer(CFile*, false)` → `CReader(CLexer&)`, then a `CAndTrigger` built as that function
builds it on its stack (`CTrigger()`, then the `CAndTrigger` vtable at `+0x0`, a child array at
`+0x68` and byte `+0x60` = 1), `CTrigger::Read(CReader&, EScopeType)`, and
`CTriggerDatabase::PostInit` and `PostValidate`. Evaluation needs a game state; it was not run.
`EScopeType` is a bit value: planet 2, country 4, fleet 64. `NEventScope::GetScopeName` names
each bit.

**Calls.** LLDB `EvaluateExpression` on the paused main thread, with breakpoints ignored and
other threads held, calls each function through a cast of its slid address. Memory comes from
`SBProcess::AllocateMemory` and is never freed, because deferred references can keep pointers
to it. One call takes about 9 ms. One trigger probe takes 0.19 s; `PostInit` and `PostValidate`
take most of it. 32 scope types for one trigger took 9.9 s.

**Results.** Diagnostics appear in the ordinary `error.log` at once, with an empty file name and
the line number inside the snippet. A wrong scope gives the same text as a file fixture. An
unknown key in `get_councilor_level` gives `Unexpected token: <key>`, and the next probe is clean:
each snippet has its own reader, so a destructive parser error does not reach the next probe.
A missing `has_technology` key gives both `Invalid technology being referenced` and the deferred
read failure, joined by the probe itself. A text value for `always` gives the Boolean trigger
message. The `is_planet_class` scope matrix accepted planet, ship and dlc_recommendation and
rejected the other 29 named bits; its message also lists colony, which is not a separate bit.

**Pitfalls.**

- The ordinary log drops a message identical to the one before it. Give each probe a distinct
  line (leading newlines) or text. `CLogger::GetLogCount` stayed zero and is not an error counter.
- The supervisor confirms the pause every 100 ms and allows 2 s. Run probes on a separate worker
  thread, or the session ends.
- After an expression, a cached `SBThread` can report a stale frame 0. The registers did not
  change (`pc`, `sp`, `fp` and `lr` were equal before and after), but the worker's paused-frame
  check compared frame 0 and ended the session. Read the thread again, and compare registers.
