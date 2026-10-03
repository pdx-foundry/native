# Engine calls, memory, and live identity

## Script checks in a paused game (M45-release)

`Game::check_script` parses trigger or effect text inside the game paused at the loaded-modifier
pause, up to validation. All addresses and layouts apply only to the exact build in
[targets](targets.md); the target recipe holds them.

**Route.** The debug console's `trigger_file` and `effect` commands parse text through
`ReadAndEvaluateTrigger` and `ReadAndExecuteEffect`. A trigger check repeats that route up to
validation: `CString(char const*)` → `CBlob::Append` → `CMemoryFile(blob, 1, 0, false)` →
`CTextLexer(CFile*, false)` → `CReader(CLexer&)`, then a `CAndTrigger` built as the console builds it
on its stack (`CTrigger()`, then the `CAndTrigger` vtable at `+0x0`, a child array at `+0x68` and
byte `+0x60` = 1), `CTrigger::Read(CReader&, EScopeType)`, and `CTriggerDatabase::PostInit` and
`PostValidate`. An effect check constructs `CEffect`, sets its top-level byte at `+0x78` to 1, calls
`Read(reader, scope)`, the effect database's `PostInit` and `PostValidate`, then the trigger
database's; the child count is at `+0x1c`. Nothing is evaluated or executed.

**Scope width.** `EScopeType` is a 64-bit bit value (planet 2, country 4, fleet 64); `GetScopeName`
must receive all 64 bits. Bits 0–41 have names, including colony at bit 40 and mission at bit 41.
Country and observer-country are different bits (2 and 19) with the same displayed name. A 32-bit
parameter silently truncated bits above 31 in an early trial.

**Capture.** `CFilterLogger::Log(int, CString const&, int, CString const&)` receives the final
message in `x4`, before ordinary-log duplicate suppression; a breakpoint at its entry captures the
offending key, identical consecutive errors and source-free errors. Assigning a unique name
through `CString::operator=(char const*)` at the bound file-name offset of the `CMemoryFile` (which
starts with an empty name) before constructing the lexer propagates to reader and deferred
validation messages; later validations keep reporting earlier names, and source-free messages stay
unjoined. Capture keeps the raw signed logger level. All file-load observation hooks are disabled
at the admitted pause, including hooks left by a failed fixture callback.

**Calls.** Each call writes its PC last, verifies every argument and control register before
resuming, and waits for a synchronous return breakpoint before restoring registers. Calls use the
paused main thread's OS-guarded stack, with an aligned stack pointer 256 bytes below the witnessed
one to keep the 128-byte red zone; a guard fault ends the session. Fresh `pc`, `sp`, `fp` and `lr`
checks are required before reporting a held pause. Engine memory for each check stays allocated
until disposal.

**Documentation dumps.** `CGameApplication::PrintScriptingDocumentation` (`0x1005ef2e0`) reads
neither its receiver nor a world; calling it at the pause writes the effects, triggers, scopes and
localizations logs. The console handler `OnExecute_PrintTriggerDocumentation` (`0x101337204`) only
builds a console result and writes nothing. `modifiers.log` is written at startup.

### Pitfalls

- **Terminate memory text with whitespace.** Without a trailing newline, `always = yes` read token
  19 (end-of-file) while keeping the text `yes`, and gave the Boolean invalid-value message; with a
  newline it read 16367 (`yes`) or 11436 (`no`), the IDs that `CBoolTrigger::Assign` compares.
  Native appends trailing whitespace to every check.
- **LLDB's `SBValue.SetData` reports success for ARM64 vector registers without writing them.**
  `SetValueFromCString` with a brace-enclosed byte list writes all 16 bytes
  (`.local/sdk-649/register_control.py` reproduces this without the game). Read vector registers
  with raw `SBData`, not `GetValue()`, and check every saved register after restoration.
- **The expression evaluator stops at the logging breakpoint** instead of continuing its callback;
  use direct calls. An early asynchronous version wrote stale call registers, and the game resumed
  ordinary startup.
- **An unguarded per-check stack overflows.** A 64 KiB scratch stack was too small for recursive
  readers within the text bound and wrote into other debugger allocations. `script_deep_nesting`
  checks 681 nested trigger blocks and 254 nested effect blocks.
- **A cached `SBThread` can report a stale frame 0** after a call although `pc`, `sp`, `fp` and `lr`
  are unchanged. Read the thread again and compare registers.
- **The ordinary log drops a message identical to the one before it**, and `CLogger::GetLogCount`
  is not an error counter. Give each probe a distinct line or text.
- **The supervisor confirms the pause every 100 ms and allows 2 s.** Run engine calls on a separate
  worker thread.
- **A check is not a file fixture.** The console route builds its top-level objects differently,
  and normal loading resolves deferred references before the command stages. Keep a paired
  file-route control with the same message identity for each rule that a check establishes. A
  file-loaded invalid object can change later validation in the same process, so the paired memory
  checks use a fresh session.
- **Never reuse a session after a failed call.** An interrupted call can leave partial allocations
  or database entries even when LLDB restores the registers.
- **Requests are isolated, not processes.** Refutations repeated in separate requests with corrected
  contrasts all survived, but that is not fresh-process isolation.

The probe scripts and results are in `.local/evidence/in-process-probe-2026-09-28/` and
`.local/sdk-649/`; the config-test sessions are in Atlas `docs/prototypes/config-test-spike/`.

## M45-old engine-call prototypes

These findings come from the SDK-439 to SDK-449 prototypes on M45-old and Windows; no Rust method
uses them. Sources are in the `sdk-testing` bundle under
`prototype/compatibility-harness/apple-silicon/native/src/{engine,prepared,locators,hooks}.rs`,
the Windows `bridge.cpp` and `pins.hpp`, and the `linear-supplement` assets.

- **ABI.** ARM64 structure returns use **x8** (set to the aligned output buffer before the branch).
  A `CString` temporary needs a 16-byte-aligned 64-byte buffer and the engine destructor; 32 bytes
  were too small. A string reader checks the sign bit of byte 23 before choosing indirect storage.
  The bridge acted only on the main thread and located the `MH_EXECUTE` image: image zero was the
  inserted library.
- **Demonstrated on M45-old:** reads of player, date, pause and AI; parsed conditions and immediate
  effects; `JumpToNextDay` and synchronous `FastForwardDays`; event option selection through
  `PostEventOptionSelection`; `CCountry::GetResource` (fixed point at scale 100000); and an
  asynchronous engine save with its completion state.
- **Posting an event choice is acceptance, not completion**; later logs, pending-event removal and
  follow-ups establish completion. Text logs suppress repeats and cannot count invocations.
- **Object identity.** Locators scan live country and planet entries and reject zero, nonunique or
  wrong-kind results; a binding keeps the acquired object instead of rerunning its locator. Full ID
  and address reuse happened for both kinds, so destructor hooks retire bindings permanently. A new
  paused planet was invisible to `every_galaxy_planet` until a day advanced; `set_owner` alone made
  an empty colony.
- **Windows** needed native RNG permission matching the console-event context, restored after
  every request; scope construction itself uses RNG.
