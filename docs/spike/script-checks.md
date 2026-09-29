# Proposal: script checks in a paused game, and a vanilla corpus check

Status: proposed on 2026-09-28, revision 4 after three plan reviews; the last found no P1. It
adds two things to the current plan and removes nothing from it. The [roadmap](../roadmap.md),
the [specification](../specs/native.md) and the [simplification decision](../design/simplification.md) stay
the authority until this is accepted.

## Summary

1. **`Game::check_script`**, a new live Native question. It gives trigger or effect text to the
   paused game, in a stated scope, and returns the diagnostics that the engine reports while it
   reads and validates that text. Its first purpose is narrow: make Native's live command controls
   cheaper and let a caller choose each observation after reading the last one.
2. **A vanilla corpus check** in Atlas. It runs the planned checker over vanilla content and
   compares its errors with the engine's source-joined load errors. Each difference is a candidate
   to investigate, not a verdict.

Native stays the source of every inventory and of every property that the engine does not report.
A script check is an observation, like a file fixture. It never changes a static answer and never
establishes acceptance by silence.

## Background

Two spikes on 2026-09-28 measured the route. The first ran probes in the paused game; the second
([config-test spike](config-test-spike.md)) used it for 5,180 probes in two sessions:

- The console's `trigger_file` and `effect` routes read text from memory with the same command
  readers as file loading. Repeated up to validation, and without evaluation or execution, they
  work at the loaded-modifier pause, with no world.
- In the sampled controls, wrong scope, Boolean, key, reference and target-type messages match the
  file-fixture messages.
- The dumps written by `CGameApplication::PrintScriptingDocumentation` agree with Native's static
  declarations on all 1,074 effect and 1,096 trigger names, and on the declared scopes of the
  1,047 effects and 1,082 triggers that have a scope answer.
- In the calibration sample, no controlled check contradicted an independent answer. That sample
  is small, and three scope flags (the engine accepts the old `pop` bit) stay unresolved.

The route, its pitfalls and all addresses are in `docs/native/engine-calls.md` and
`docs/native/diagnostic-survey.md` on branch `spike/in-process-probe` (`db7d936`). Merge those
knowledge notes into `main` first, whatever happens to this proposal.

## Problems

### 1. Native's live command controls are slow

A file fixture needs its own launch. Validation lasts until all content loads, about 74 seconds,
whatever the fixture holds. One fixture holds at most 32 questions in one file, with inputs fixed
before launch. A rejected key upsets the definitions after it, so each one needs its own session.
`cargo live fixture_argument` uses 10 sessions for 20 complete grammars: two accepted-sample
sessions and eight unknown-key sessions.

This limits how often Native's static command answers are checked against the engine. The review
of SDK-548 found several paths that gave `Complete` without proof, and the old `set_planet_class`
wrong-scope control passed a live test for an unrelated reason. More, cheaper controls catch these
earlier.

### 2. Nothing checks the finished config end to end

Each method is tested alone. No test compares the assembled config and checker with the engine on
real script. That comparison is what the compiler needs.

## Addition 1: `Game::check_script`

### Contract

The caller starts a game with `GameOptions::loaded_modifiers` and waits for the pause, as today.
It then sends checks, one check per request. A caller that needs many checks sends many
requests; each answer can guide the next. Each check states:

- the declaration kind: trigger or effect;
- the input scope type, with its identity from `Native::scopes`;
- the text.

Each check result states:

- whether the read returned, and the number of top-level children that the engine built;
- each diagnostic joined to this check, with its text and its stage (read or validation), and its
  line inside the text when the message carries one;
- each diagnostic observed during the check that cannot be joined to it, kept separate;
- completeness: whether every required hook was active for the whole check, and whether any bound
  was reached.

The answer reports observations only. A quiet check is not acceptance: the engine has silent
routes, such as `false without diagnostic` results that the validation drivers ignore. A missing
line, a message that cannot be joined, or a missing hook gives a partial result, never a clean
one. This keeps the [consumer boundary](../native/command-grammar.md#consumer-boundary) that file
fixtures use.

### Bounds

| Bound | Initial value | Result when reached |
| --- | --- | --- |
| Text of one check | 4 KiB | The request is refused |
| Diagnostics per check | 32 | The check is partial |
| Time for one check | 5 s | The session is disposed; see failure behavior |
| Checks in one session | 3,000 (provisional) | Further requests are refused |

The largest measured session ran 3,409 probes on the spike's sampled inputs without a fault; a
second, fresh session ran 1,771. Those counts do not add up to one session. The provisional limit
is below the single-session measurement, and a sustained test at the limit, with memory and time
per check recorded, is an acceptance criterion. Engine memory for each check stays allocated until
disposal, because the engine databases keep the constructed commands.

One check per request keeps the timing simple. The caller's idle bound can be as short as one
second, so a check must not run inside it. When the supervisor admits a check, it replaces the idle
deadline with the absolute per-check deadline. The worker's pause confirmations do not extend that
deadline. When the answer completes, the configured idle deadline starts again. Test a one-second
idle setting and a check admitted just before the idle deadline.

### Engine route

For a trigger: text → `CString` → `CBlob` → `CMemoryFile` → `CTextLexer` → `CReader`, a
`CAndTrigger` built as the console builds it, `CTrigger::Read(reader, scope)`, then
`CTriggerDatabase::PostInit` and `PostValidate`. For an effect, the same with `CEffect` and the
effect database, then the trigger database. Nothing is evaluated or executed. The text always ends
with a newline; without it, the last token reads as end-of-file.

Function addresses and signatures, the 64-bit scope-type width, construction bytes and object
layouts are per-build facts. They go into the target record's recipe, in the binding authority.
The spike's helper, which holds them as constants, does not become the shared method.

### Prerequisite: diagnostic capture

Capture is the first work item, and it gates the rest. The spike read the new part of the
ordinary `error.log`. That is not enough: the log drops a message identical to the one before it,
some messages carry no line, and it has no stage. The existing fixture hooks cannot be reused as
they are. They record only during a file-loading window, they filter by the fixture's file name,
and their callback returns the session's pause decision, which is already "stop" at the pause.

Prove one route that meets these requirements on the known controls:

1. It captures the final message text, including the offending token (`Unexpected token: <key>`,
   `Malformed token: <text>`), not only the early reader report.
2. It records each message once per occurrence, including two identical consecutive failures.
3. It records a message with no source location, such as `Invalid technology being referenced`,
   and reports it as unjoined unless the check's own text identifies it.
4. It gives each message the stage window of the current check: read, or validation.
5. A missing or late hook gives a partial result for that check.

Two candidate routes: let the engine's log functions stop at a breakpoint whose callback records
the message and continues during the call; or make each call without LLDB's expression evaluator,
so the normal breakpoint path runs. Choose by measurement. Do not commit the public API or change
the live tests before one route passes all five requirements.

### Prerequisite: attribution across checks

`CTriggerDatabase::PostValidate` and `CEffectDatabase::PostValidate` visit every command in their
database, including commands built by earlier checks. A later check's validation can therefore
produce a message that belongs to an earlier check. A fresh reader separates reading, not
validation.

A diagnostic is joined to a check only when its source identifies that check uniquely in the
whole session, because the databases keep every earlier check. Use one of these, chosen by
measurement:

- a unique source name for each check's memory file, if the file object can carry one; or
- non-overlapping engine line ranges, only if a verified way exists to set the reader's first line
  number without allocating a prefix of empty lines.

Do not pad text with empty lines to separate checks. The prefix grows with every retained check:
3,000 checks near the 4 KiB limit would need gigabytes of padding in engine memory. If no unique
source identity is found, validation messages stay unjoined and the check is partial.

Text or token matching can join a message only when that text is unique among all retained checks.
A message joined to an earlier check is reported as foreign, with that check's identity. A message
that cannot be joined uniquely stays in the unjoined list and makes the current check partial.

Test these orders in one session: bad then good; good then bad; bad, good, then the same bad again;
two identical bad checks; and the same sequences reversed. For each check, the joined diagnostics
and the completeness must equal those of the same check run alone in a fresh session. Foreign
diagnostics may differ from the fresh run, and they are listed separately.

### Pause state and failure behavior

The spike kept confirming the old pause while a probe ran, although it had not checked the stopped
state or the registers during that time. The supervisor treats that confirmation as proof that the
same stopped frame is held. The production route needs an explicit state:

- **Checking.** The worker reports "checking" instead of "held" to the supervisor's pause check.
  The supervisor accepts that state for at most the per-check time bound. Engine calls are serial:
  one check at a time, one call at a time.
- **Return to held.** After each check, the worker reads the stopped state and the `pc`, `sp`,
  `fp` and `lr` registers again. It reports "held" only when all of them equal the witnessed pause.
- **Failure.** A call that exceeds its time bound, a register difference, an LLDB error inside a
  call, caller cancellation during a check, or worker loss ends the session. The worker stops the
  game and the supervisor confirms disposal, as for other faults. An interrupted engine call can
  leave partial allocations or database entries even when LLDB restores the registers, so the
  session is never reused after a failure. The check result is unavailable, with the reason.

Tests: a call that does not return (a deliberate stuck function in a fake worker), a register
difference, cancellation during a check, worker loss during a check, and a normal return to held
after many checks.

### What it does not do in this version

- It does not resolve static candidates. `command_grammar` publishes only accepted alternatives;
  unresolved ones become gap text, and Atlas still reports command arguments as one unresolved
  gap (SDK-625 is not done). Using checks to settle named candidates would need a typed output of
  those candidates and an Atlas composition test that never credits an unresolved sibling. That
  is a separate proposal, with its own measurements.
- It does not replace walker work. Many unresolved chains end in `false without diagnostic`,
  where no check can observe an answer, or in a target check that skips unknown types.
- It does not batch validation or check registry fields. Both wait until the single-check contract
  is established and measured.
- It does not run on Windows, which the roadmap defers.

## Addition 2: vanilla corpus check

Atlas runs the planned checker over vanilla content with the generated config, for a supported
slice (for example, trigger blocks in one registry), and compares it with the engine. It does not
build another validator.

### Engine side

One normal launch on the same build loads all vanilla content with the same DLC set. No registry
is isolated, so no background errors come from isolation. No Native operation returns ordinary
load errors today: `observe_fixture` filters its diagnostics to the fixture's file, and
`check_script` observes only its own checks.

The smallest route is an Atlas corpus harness. It starts a session with
`GameOptions::loaded_modifiers`, waits for the pause, and reads the ordinary `error.log` from the
session's work directory before cleanup. First confirm with the inspector that the loaded-modifier
pause follows the trigger and effect validation stages in `CGameApplication::InitGame`; otherwise
choose a later boundary. The work directory is kept today only through a hidden test option. A
supported route (a Native question for the ordinary load errors) is additional API work, counted
in Cost.

Only messages with a vanilla file name and line inside the slice are compared. Messages with no
source, or with a source outside the slice, are listed as unresolved. The ordinary log drops a
message identical to the one before it, so a missing repeat is not evidence. A launch that does not
reach the boundary, or a log that cannot be read, makes the whole corpus result unavailable. It
never becomes "neither reports an error".

### Comparison

| Result | Meaning | Next step |
| --- | --- | --- |
| Both report an error at the same place, for the same subject and the same property | Agreement | None |
| Both report an error at the same place, but for a different subject or property | Two candidates: a missing rule and a false error | Investigate both, as in the two rows below |
| Only the engine reports an error | A missing rule, a checker defect, or a different context | Reduce it to a small text, then settle it with `Game::check_script` or a file fixture in the same context |
| Only the checker reports an error | A false error, or an error that the engine accepts silently | Look for a static answer or a storage observation that supports the checker. A typed checker may correctly reject what the engine tolerates. |
| Neither reports an error | Agreement on this observation | None. This is not proof of acceptance. |

"Property" is a small fixed set of error classes: wrong scope, wrong value kind, unknown key,
missing reference, wrong target type, and "other". The checker states its class. An engine message
gets a class only from a fixed message pattern checked on known controls, and it must name the
same subject (command or key) as the checker's error. An engine message with no class, or with
"other", is a candidate even when it shares its place with a checker error. A difference becomes a
defect only after the same property is established in the same context on both sides. Until then
it is a candidate.

### Corpus acceptance

1. The slice and its vanilla file list are fixed before the run.
2. A deliberate config mutation (for example, one scope removed from one trigger that vanilla
   uses in that scope) makes the check report checker-only candidates at the expected vanilla
   lines.
3. A deliberate text mutation of one vanilla definition in a fixture (for example, a wrong scope)
   gives an engine error and a checker error at the same place, with the same subject and class.
4. A negative control puts two different errors at one place (for example, a missing reference
   where the checker is made to report a wrong scope). The comparison must report two candidates,
   not agreement.
5. Unjoined engine messages are counted and listed, never dropped.
6. A launch stopped before the boundary gives an unavailable corpus result.

This check needs a generated config and a checker, so it starts with the first config slice.
Atlas, or the compiler project, owns it.

## What it replaces

| Today | With this proposal |
| --- | --- |
| `fixture_argument`: 10 sessions (two accepted-sample sessions and eight unknown-key sessions) | One check session for the whole command matrix, each rejection with a corrected contrast, plus a paired file-route regression of three sessions (below) |
| The spike's dropped-script worker patch | A protocol request with bounds, a checking state, tests and binding entries |

The paired file-route regression stays, because the console route builds its top-level objects
differently from file loading, and normal loading resolves deferred references before the command
stages. Keep `fixture_argument_potential`, `fixture_argument_on_enabled` and one unknown-key
session, and add one deferred-reference pair (`has_technology` with a missing and an installed
key). Each file-route case must give the same message identity as its check, not only the same
stage and line. `fixture_control` stays unchanged: its limit, order and weight cases depend on the
tradition field context.

It does not replace static inventories, static analysis of silent properties, walker work or any
static answer. It adds nothing to the public types except the request, its answer and a support
value. There are no evidence descriptors, replay paths or artifact hashes.

## Cost

- Worker: the capture route, the checking state, serial calls and bounds. The spike's patch covers
  only the calls themselves.
- Supervisor and protocol: the checking state in the pause confirmation, the request and answer,
  and failure handling.
- Binding: one set of recipe entries for each supported build. On Mac, symbols locate them.
  Milestone 7 (the update rehearsal) should measure this cost with the rest of the method set.
- API, recorded answers and tests: about the size of `observe_fixture`'s request path.
- Atlas: the corpus harness, the message classes and their patterns. It depends on the config
  format and checker, which are planned work.
- Native, only if the corpus needs a supported route: a question that returns the ordinary load
  errors with their sources. Until then the harness uses the kept work directory.

## Acceptance

1. The knowledge notes from `spike/in-process-probe` are in `main`.
2. One capture route passes the five capture requirements on the known controls.
3. The attribution orders give the same joined diagnostics and completeness as isolated checks in
   fresh sessions, with foreign and unjoined messages listed separately.
4. The failure tests pass: stuck call, register difference, cancellation, worker loss, and return
   to held after many checks. Each failure ends with confirmed disposal.
5. `Game::check_script` runs the full `fixture_argument` command matrix in one session with the
   same verdicts as the file route, each rejection with a corrected contrast.
6. The paired file-route regression passes with matching message identities. The seven other
   unknown-key sessions are then removed from the live suite.
7. A sustained session runs checks up to the session limit, including checks near the 4 KiB text
   limit with many lines, and records memory and time per check; no fault, and no growth that makes
   a check exceed its time bound.
8. The specification lists the operation, its bounds, its limits (commands only, no evaluation or
   execution, M45-release) and its consumer boundary.
9. The corpus check meets its own acceptance criteria when the first config slice exists.
