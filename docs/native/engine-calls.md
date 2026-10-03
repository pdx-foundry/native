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

### SDK-649 capture and source controls

The 2026-09-28 implementation controls used the exact M45-release identities below.
`CFilterLogger::Log(int, CString const&, int, CString const&)` receives the final message
in `x4`, before ordinary-log duplicate suppression. A breakpoint at its entry captured
the offending unknown key, two identical consecutive Boolean errors, and both occurrences
of the source-free missing-technology error. The callback uses the current read or validation
window. Disabling the hook made capture incomplete. These are finite capture controls, not
proof that silent validation paths accept input.

`CMemoryFile` starts with an empty `CFile` name. Assigning the name through
`CString::operator=(char const*)` at the bound file-name offset before constructing the lexer
propagated a unique name to both reader and deferred validation messages. Later validations
reported the earlier names unchanged. Source-free messages stay unjoined. No empty-line
padding is needed. The addresses, signatures and construction layout belong to the target
recipe; source attribution belongs to the worker.

The expression evaluator stopped at the logging breakpoint instead of continuing its callback.
The direct-call control worked with synchronous continuation and a return breakpoint. An early
asynchronous version sometimes wrote stale call registers: a recorded pre-call PC still named
the original pause, and the game resumed ordinary startup. Therefore each call writes its PC
last, verifies every argument and control register before resuming, and waits for synchronous
return before restoring registers. Vector registers require raw `SBData`, not `GetValue()`.
Fresh `pc`, `sp`, `fp`, and `lr` checks remain required before reporting a held pause.

The retained controls and results are in `.local/sdk-649/`. The ten-check capture/source run
`capture-1790649059928203000` took 2.46 seconds and confirmed disposal. Earlier failed runs
also confirmed disposal. The production request path and sustained-session acceptance are
separate checks and are not established by these measurements.

A spike on 2026-09-28 parsed trigger text inside the paused game, with no new launch per probe.
It applies only to the M45-release executable in [targets](targets.md). The worker patch, probe
scripts and results are in `.local/evidence/in-process-probe-2026-09-28/`. The spike branch
`spike/in-process-probe` is merged into `main` (`1a564ce`), and `Game::check_script` is the
supported route that grew from it; see the SDK-649 sections below.

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
message. The original `is_planet_class` matrix accepted planet, ship and dlc_recommendation and
rejected the other 29 tested bits. It did not test the whole scope universe: the helper used a
32-bit parameter. The later config-test spike corrected this mistake; colony is bit 40.

**Pitfalls.**

- The ordinary log drops a message identical to the one before it. Give each probe a distinct
  line (leading newlines) or text. `CLogger::GetLogCount` stayed zero and is not an error counter.
- The supervisor confirms the pause every 100 ms and allows 2 s. Run probes on a separate worker
  thread, or the session ends.
- After an expression, a cached `SBThread` can report a stale frame 0. The registers did not
  change (`pc`, `sp`, `fp` and `lr` were equal before and after), but the worker's paused-frame
  check compared frame 0 and ended the session. Read the thread again, and compare registers.

### Config-test spike: pause and token controls

The E0 repeat used the same M45-release build and two launches, both with confirmed disposal.
A strict check of freshly read `pc`, `sp`, `fp`, and `lr` replaced the spike's frame-PC reset.
Nine scope, key, Boolean and reference controls passed, followed by 305 seconds of idle time
and 31 successful loaded-modifier refreshes. The first launch failed its Boolean positive
control and remains in the experiment's cost and failure counts.

**Terminate memory text with whitespace.** In the first run, `always = yes` without a trailing
newline produced the Boolean invalid-value message. A controlled token read returned token 19
(end-of-file) while retaining the text `yes`; `no` behaved the same. Adding a newline returned
16367 (`yes`) and 11436 (`no`), the IDs compared by `CBoolTrigger::Assign`, and removed the error.
The in-process reader helper now appends a newline to every snippet. Without this boundary,
quiet string-based readers and failing token-based readers cannot establish value domains.

Evidence: Atlas `docs/prototypes/config-test-spike/sessions/e0-{1,2}/`, including probe inputs,
results, refresh loop, ordinary logs and disposal summaries. The worker patch was merged with
`spike/in-process-probe` (`1a564ce`); the strict register check is the worker's
`pause_registers`.

### Config-test spike: documentation, effects and full scope width

All following addresses and layouts apply only to M45-release, executable SHA-256
`07988b4f1b865623becd7a61af1cae92e111be6515d341754af70f02107822cd`, ARM64 slice
`a4cb49ad17a84ef6bf438019a50d3a66362c80731f8359888ddbce47c0d0aab9`.

**Documentation call.** `OnExecute_PrintTriggerDocumentation` at `0x101337204` builds a console
result and reads the argument count; it does not call a documentation writer. The E1 control
called it with an empty `CPdxArray<CString>` and the inspected x8 structure-return convention.
Four log files remained empty. The inspected `CGameApplication::PrintScriptingDocumentation`
at `0x1005ef2e0` does not read its receiver or a world. Calling it at the loaded-modifier pause
fills effects, triggers, scopes and localizations logs. Modifiers were already written at startup.
The disassembly and before/after sizes are retained in Atlas `static/` and `sessions/e1-3/`.

**Effect validation.** Repeat `ReadAndExecuteEffect` only through validation: construct `CEffect`,
set its top-level byte at `+0x78` to 1, and call `Read(reader, scope)`, then the effect database's
`PostInit` and `PostValidate`, followed by the trigger database equivalents. The helper records
the child count at `+0x1c`. E3 passed 12 scope, Boolean, target-type and key controls with clean
corrections. It never calls effect execution. Exact call signatures/addresses are in the retained
helper and `static/read-effect.txt`; they are experimental build facts, not a new public ABI.

**Scope width.** `GetScopeName` must receive the 64-bit `EScopeType`. Passing `int` truncated bits
above 31 in the earlier trial. E3 queried all 64 individual bits with `unsigned long`: bits 0–41
have names, including colony at bit 40 and mission at bit 41. Country and observer-country are
different bits (2 and 19) with the same displayed name. The final matrices preserve that distinction.
The raw `scope_bits` response and Atlas `scope-universe.json` retain the exact mapping.

**Sustained use and limits.** E4 completed 3,409 probes in one session and 1,771 more in a fresh
session. Each of the 31 refuted claims then occupied its own request; 528 added positive contrasts
were quiet with parsed children. No refutation disappeared and no register/refresh fault occurred.
This shows bounded request isolation, not fresh-process isolation for every claim. Engine memory
allocated for readers and deferred references is retained until process disposal. Six launches,
including the initial failed Boolean control, used 1,717.542 seconds; all confirmed disposal.

The findings and rule limits are separate: Native records the route here, while Atlas owns
`docs/prototypes/config-test-spike/REPORT.md`, its calibration and config conclusions. A checked
copy is under Native `.local/evidence/config-test-spike-2026-09-28/`; see [preservation](preservation.md).

**SDK-649 register restoration control.** The installed LLDB reports success from
`SBValue.SetData` for ARM64 vector registers without changing their bytes. A small native
control in `.local/sdk-649/register_control.py` reproduces this independently of the game.
`SetValueFromCString` with a brace-enclosed byte list writes all 16 bytes. The direct-call
method uses that form for vectors and checks every saved register after restoration.


**SDK-649 sustained public API control.** On 2026-09-28, 3,000 sequential trigger checks
used 4,096-byte multiline snippets. Mean wall time was 0.347280 seconds; maximum was 0.475936
seconds. Check 3,001 was rejected before a call, and disposal was confirmed. Resident memory
was 2,712,880 KiB after check 1 and 968,048 KiB after check 3,000; paging makes the endpoints
unsuitable as an allocation-growth estimate. The final eleven checks grew by about 96 KiB per
check. Allocations stay bounded by the session count and are released with the process.
Per-check measurements and full default-suite results are retained in
`.local/sdk-649/sustained-3000-before-stack-fix.log`. That run used the original per-check
64 KiB scratch stack; it predates the guarded thread-stack repair. The sustained runner is a
retained one-off experiment in `.local/sdk-649/runner`, not part of the live suite.


**SDK-649 parity and attribution.** The live suite passed the full command-argument matrix
with a corrected contrast after every rejection. The retained file controls cover `potential`,
`on_enabled`, one unknown key and missing/installed `has_technology` references. File and memory
routes matched message identities after source/line normalization. A file-loaded invalid councilor
object can change later validation in the same process; the paired regression therefore uses a
fresh session for its memory checks. Four isolated controls and bad/good, duplicate, unknown-key
and reference orderings preserved current diagnostics and completeness; prior-source messages
remained separate. Colony's bit-40 scope also passed. The seven redundant unknown-key file launches
were removed only after the matrix and paired controls passed.

**SDK-649 guarded stack repair.** The initial direct-call method allocated an unguarded
64 KiB stack per check. Recursive readers can exceed that size well within the text bound and
write into other debugger allocations. Calls now use the paused main thread's OS-guarded stack,
with an aligned stack pointer 256 bytes below the witnessed pointer to preserve the ARM64
128-byte red zone. There is no per-check stack allocation. Full register restoration remains
mandatory. A guard fault ends the session rather than allowing a return to held.

The committed `script_deep_nesting` live case passed 681 nested trigger blocks in 4,096 bytes
and 254 nested effect blocks in 4,085 bytes. Each was followed by a clean check and a repeat of
an earlier invalid Boolean check; current diagnostics and completeness stayed unchanged, and
disposal was confirmed. The command matrix passed again. Full output is preserved at
`.local/sdk-649/guarded-stack-deep-nesting.log`. These controls verify deep inputs and reuse;
the earlier 3,000-check measurements have not been rerun with the repaired stack.

Capture also retains the raw signed logger level. Source lines missing only from foreign
messages do not reduce current completeness. All file-load observation hooks are disabled at
the admitted pause, including hooks left active by a failed fixture callback, so they cannot
intercept later command checks.
