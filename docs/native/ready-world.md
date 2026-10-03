# Ready-world observations (retired route)

The world API and the M451-hotfix world recipe were retired on 2026-10-02. Keep this page complete
enough to restore the world pause if the entry-context self-link assumption fails (SDK-677).

## Restore

`d8f9d8a` is the last complete implementation: world recipe, worker, protocol, controls
(`tests/live/world.rs`, `tests/live/world_numeric.rs`) and expected data. `2d930e4` holds
`tests/fixtures/world-m451/fixture.sav` (first added in `9ad2938`) and
`tests/expected/world-numeric-m451/cases.json` with the evaluated operand results. Use
`git show <commit>:<path>` or a separate checkout, and match the exact M451-hotfix identity in
[targets](targets.md) first; the 4.5.0 target never had a world recipe. Private findings are in
`.local/sdk-650/` and `.local/sdk-647/`. The non-world operand controls stay in
`tests/live/script_numeric.rs` (`cargo live script_numeric`).

## How the route worked

- **Request.** `GameOptions::world(WorldRequest)` named the displayed local human country, one
  prepared effect, at most 120 engine days, 32 flag names and 32 variable names; effect text at
  most 4 KiB, the save at most 16 MiB. Native copied the save (made on 4.5.1 without mods) into an
  isolated profile and loaded installed content with no mods. The startup deadline covered
  loading and the whole observation.
- **Readiness gate.** The game-state readiness byte, the paused idler, the main-thread receiver and
  the normal `UpdateInternal` / `Idle` / `UpdateOneFrame` stack; a startup or loading stack does not
  pass. The observer resolved the local human and country, checked the full country ID and the
  displayed name, and checked identity and readiness again after each day.
- **Result.** The worker returned a completed result before reporting `PausedInWorld`; repeated
  reads did not execute again. An invalid effect gave a partial answer with the initial sample
  and no time advance. Recorded answers were keyed by save contents, country, effect, day count,
  and ordered flag and variable names (stamp `observe-world/v2`).
- **Calls.** The register-call method on the OS-guarded main-thread stack, with other threads
  running because daily updates wait on engine jobs. A return breakpoint checked the main thread
  and the call's stack pointer, so a nested update cannot end the call. Every saved scalar and
  vector register was checked after restoration; a fault ended the session.
- **Effect.** The memory parser and validation route, then `CEffect::Execute` in a constructed
  country scope. The diagnostic window covered reading, validation and execution, with the logger
  hook restricted to the executing main thread; any message there before execution prevented the
  call. The window closed before daily simulation.
- **Days and flags.** `FastForward(1, false)` advances one engine day per call; the raw date must
  advance by exactly 24. Flag IDs and signed counts came from the country store; absence is `None`,
  and zero and negative counts stay distinct.
- **Variables.** `GetVariablePointer(CEventScope const&, CString const&)` (`0x100d0d704`) selects
  the store: a `local_` name uses the scope-local store (`CEventScope::GetVariables`), any other
  name the country's own store (`CEventScope::GetSavedVariables`). `CVariables::VariableIsSet`
  (`0x100d1e7b8`) and `CVariables::GetVariable` (`0x100d1e784`) read the map; `GetVariable` returns
  the raw `CFixedPoint` in `x0` at scale 100000. A null store means "not set": the scope-local
  store does not exist until an effect creates it. Each name costs two or three engine calls per
  sample. To read another engine number, export it to a country variable
  (`export_resource_stockpile_to_variable`, `export_modifier_to_variable`,
  `export_trigger_value_to_variable`) or copy it with a qualified operand.

## World pins on M451-hotfix

- `CInGameIdler::UpdateInternal(bool)` is `0x10086de60`. The pause is at `+4`, after its first
  `sub sp, sp, #0xe0`; the expected caller chain is `CGameIdler::Idle(bool)`, then
  `CApplication::UpdateOneFrame(bool)`.
- `GetGameDateIfPossible()` is `0x100707da4` and reads `g_CurrentGameState` at `0x1032e9450`,
  readiness at `+0x98`, and date at `+0xb8`. The in-game idler global is `0x1032e9438`;
  `JumpToNextDay` reads its pause byte at `+0x584`.
- `CHuman::AccessSelectedCountry` reads the selected country ID at `+0x54` and checks country IDs
  at `+0x20`. `SetCountry` writes scope type 4 at `+8`, ID at `+0x10`, and clears the cached
  object at `+0x1c`. The scope constructor fits the allocated `0x180` bytes.
- Flag lookup uses 16-bit IDs at array `+0x10`, count `+0x1c`; signed counts use array `+0x40`
  and count `+0x4c`.
- `GetVariable` looks up the map at `+8` and loads the 64-bit value at entry `+0x30`, or the
  engine's zero constant for a missing entry.

## Observed country-flag countdown

The five SDK-646 durations agreed with the [flag store countdown](durations.md#flag-store-countdown-m45-release)
over 90 days: one decrement per engine day; `days = 0` becomes -1 on day 1 and stays; negative
counts stay; `years = 5965233` wraps to -2147483416 and stays. This is country-flag behavior on
this build and fixture, not the update frequency of every flag owner.

## Pitfalls

- **Pause one instruction after entry.** At `UpdateInternal` entry the processor status carried a
  transient branch-type bit that LLDB reported restoring but discarded. Pausing past the first
  ordinary instruction keeps exact register checks.
- **Executable allocations can fail silently.** A scratch allocation returned an invalid address
  with a success status; use no executable allocation and reject invalid addresses.
- **Profile setup.** Both save directories must exist, and `continue_game.json` needs all three
  string fields (`title`, `desc`, `date`); omitting `date` crashed the engine. A frequent
  startup-input hook is unsuitable for the world gate. The old beta fixture did not establish a
  world on 4.5.0.
- **One rejected statement stops the whole prepared effect.** Read every statement with
  `check_script` first.
- **A quiet read is not a valid reference.** An unknown prefix, an unknown modifier and an absent
  saved target all read quietly and fail only when the effect executes.
- **Equality at zero proves nothing about a modifier:** an absent modifier also gives zero. Use a
  nonzero value that a content definition gives.
- **A modifier added by `add_modifier` is not visible to `modifier:` in the next statement.** It
  became visible after a later `random_country` statement; do not read a rule from this.
- **Trigger names change between builds.** `num_pops` is not a trigger on M451-hotfix.
- **A plain country flag has count -1**; a timed flag whose operand evaluates to zero has count 0
  on day zero.
