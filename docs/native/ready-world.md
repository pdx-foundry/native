# Ready-world observations (SDK-650, SDK-647)

## Verified route

The route is supported on the exact M451-hotfix ARM64 executable in [targets](targets.md).
The 4.5.0 target keeps its startup operations; it has no world recipe. Addresses and layouts
are held only in the target recipe. The tracked save is
[world-m451/fixture.sav](../../tests/fixtures/world-m451/README.md), created on 4.5.1 without mods.
Native copies it into an isolated profile and loads installed content with no mods enabled.
The original save is never written.

`GameOptions::world(WorldRequest)` selects this pause instead of a registry, fixture or modifier
pause. The request names the displayed local human country, one prepared effect, at most 120
engine days, at most 32 flag names and at most 32 variable names. Effect text is limited to
4 KiB; the save to 16 MiB. Startup's configured deadline covers loading and the whole prepared
observation. There is no second deadline or open-ended execution loop. The numeric operand
results that this route gives are in [scoped numeric](scoped-numeric.md#world-evaluation-on-m451-hotfix-sdk-647).

The readiness gate checks the actual game-state readiness byte, paused idler, main-thread
receiver and the normal `UpdateInternal` / `Idle` / `UpdateOneFrame` stack. A startup or loading
stack does not pass. The observer resolves the actual local human and country, checks the full
country ID against the human reference, and compares its displayed name with the request.
Country identity and readiness are checked again after each day.

The worker returns a completed result before reporting `PausedInWorld`. The supervisor joins
that result to the session, owned game, activated hook and main thread. `Game::observe_world`
reads this fixed result and refreshes the idle timeout; repeated reads do not execute again.
The result distinguishes effect execution, diagnostics and the date/flags after each day.
An invalid effect gives a partial answer with the initial sample and advances no time.
Recorded answers select the world by save contents, country, effect, day count and ordered flag
names. A live recording uses the loaded private save, even if the caller replaces the source.
Reading a different request or save requires its own recorded answer.

## Calls, dates and flags

The worker uses the existing ARM64 register-call method on the OS-guarded main-thread stack.
Other threads can run because daily updates may wait on engine jobs. A return breakpoint checks
both the main thread and the call's stack pointer: a nested normal update cannot end the call.
Every saved scalar and vector register is checked after restoration. Calls share the startup
deadline; a fault ends the session and enters owned-process cleanup.

The pause is one instruction after `UpdateInternal` begins, after its stack adjustment. At entry,
the processor status carried a transient branch-type bit. LLDB reported successful restoration
but discarded that bit. A retained small local program confirmed that write behavior. Advancing
the pause past the first ordinary instruction preserves exact register checking. An earlier
executable scratch allocation also returned an invalid address despite a success status; no
executable allocation is used now, and all allocations reject invalid addresses.

The effect uses the established memory parser and validation route, then `CEffect::Execute` in a
constructed country scope. The finite diagnostic window covers reading, validation and execution.
Its logger hook is restricted to the main thread that executes the prepared calls, so concurrent
engine-job messages cannot reject an effect or consume its capture bound. Any message on that
thread before execution prevents the call, including engine errors that omit a source name.
The window closes before daily simulation;
ordinary queued-command messages are outside that prepared effect's diagnostics.

`FastForward(1, false)` advances one engine day per call. Each sample reads the engine date and
requires its raw value to advance by exactly 24. Flag IDs and signed counts come from the actual
country store; matching array lengths and bounded names are required. Interned names are cached
within the observation. Absence is `None`; zero and negative stored counts remain distinct.

## Variables

Each sample also gives the requested variables, in request order. A set variable gives its raw
signed 64-bit value and the scale 100000; an unset one gives `None`. The source stamp is
`observe-world/v2`, and the recording key includes the variable names.

The worker reads a name as the engine reads a variable operand:

- `GetVariablePointer(CEventScope const&, CString const&)` (`0x100d0d704`) selects the store. A
  name that starts with `local_` uses the scope-local store of the prepared scope
  (`CEventScope::GetVariables`). Any other name uses the country's own store
  (`CEventScope::GetSavedVariables`).
- `CVariables::VariableIsSet` (`0x100d1e7b8`) and `CVariables::GetVariable` (`0x100d1e784`) read
  the map. `GetVariable` returns the raw `CFixedPoint` in `x0`.
- The scale is a recipe value. The supervisor refuses a result that carries another scale.

A null store means "not set", not a failure: the scope-local store does not exist until an effect
creates it. `world_ready` asks for one unset name of each kind and gets `None` for both. A failed
call or memory read still ends the session. A literal case (`set_variable` with `2.75`, raw
275000) shows that the read and the scale are correct before any reference case depends on them.

Each name costs two or three engine calls in every sample. The numeric cases use `days = 0`;
a request with many names and many days uses more of the startup deadline.

A variable is the way to read other engine numbers. An effect such as
`export_resource_stockpile_to_variable`, `export_modifier_to_variable` or
`export_trigger_value_to_variable` writes the number to a country variable, and the request names
that variable. A value of another scope is copied through a qualified operand, such as
`set_variable = { which = V value = capital_scope.planet_variable }`.

## Live result, 2026-09-29

The empty baseline passed in 32 seconds: United Nations of Earth, paused at 2200.01.01, no effect
execution, one sample and a complete answer. A second run executed the five SDK-646 effects and
passed every sample from day 0 to day 90 in 43 seconds. Day 90 was 2200.04.01.

| Prepared duration | Day 0 count | Removal or retained behavior |
| --- | ---: | --- |
| `months = 2 days = 3` | 90 | Decrements once per engine day; absent on day 90 |
| `days = 1` | 1 | Absent on day 1 |
| `days = 0` | 0 | Becomes -1 on day 1; present through day 90 |
| `days = -1` | -1 | Present with -1 through day 90 |
| `years = 5965233` | -2147483416 | The signed 32-bit product wraps; unchanged and present through day 90 |

These observations agree with `FlagCountdown`. They establish country-flag behavior on this build
and fixture, not the update frequency of every flag owner. The source save was unchanged.
Missing-hook, worker-loss, access-failure and cancellation controls all passed with confirmed
disposal. The rejected-effect control also preserved day zero and left the flag absent;
the wrong-country control refused startup with confirmed disposal. Production tests are
`cargo live world_ready`, `cargo live world_expiry`,
`cargo live world_missing_hook`, `cargo live world_worker_loss`, `cargo live world_access_failure`,
`cargo live world_cancel`, `cargo live world_rejected` and `cargo live world_wrong_country`,
with `STELLARIS_PATH` set.

The old beta fixture did not establish a world on 4.5.0. Failed profile experiments found that
both save directories must exist and `continue_game.json` must contain all three string fields
(`title`, `desc`, `date`); omitting `date` crashed the engine. A frequent startup-input hook was
also unsuitable for the world gate. Those probes, the status-register control, raw disassembly,
full successful observations and disposal summaries are retained under `.local/sdk-650/`.
