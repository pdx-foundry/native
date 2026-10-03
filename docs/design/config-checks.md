# Checking the config against the engine (Atlas proposals)

Status: Atlas-owned rules and a proposal, kept in Native until Atlas holds them. They are not
accepted Native designs. They came from the 2026-09-28 generated-fixture and config-test spikes
and the script-check proposal. Native implemented the observation route as `Game::check_script`;
see the [specification](../specs/native.md). The spike results are in Atlas
`docs/prototypes/generated-fixture-spike/REPORT.md` and
`docs/prototypes/config-test-spike/REPORT.md`. That directory is local and ignored; Native keeps
checked copies under `.local/evidence/` (see [preservation](../native/preservation.md)). The
engine findings are in [engine calls](../native/engine-calls.md) and the
[diagnostic survey](../native/diagnostic-survey.md).

## Every claim needs an oracle

A config claim is a test only when something from the engine can answer it. There are three kinds
of answer, and they answer different claims:

| Oracle | What it answers | Example result |
| --- | --- | --- |
| **Inventory** (Native static answers, engine dumps, loaded item names) | Existence and completeness: is something missing, or no longer present | "No effect `new_effect`", "scope `pop` no longer exists" |
| **Probe** (text given to the engine, then its diagnostics) | One stated property of one subject | "`set_deposit` is not valid in `country`" |
| **Static property** (Native reader kind, ordering, conditions) | Properties that the engine does not report | "`cost` stores an integer", "a repeated `exceptions` overwrites" |

A probe can check a claim that the config makes. A probe cannot find a subject that the config
omits. Completeness always comes from an inventory.

Each tested claim gets exactly one result:

1. **Confirmed.** The oracle agrees, with a working control on the same route.
2. **Refuted.** The oracle disagrees, and the refutation survived the checks below.
3. **Partial.** Some of the claim is confirmed, for example examples of an open range.
4. **No comment.** The route is not observed or the engine is silent. Silence alone is never
   confirmation.
5. **Untestable.** No oracle can answer this kind of claim.

## Rules

1. **Control every probe route.** A quiet probe is a confirmation only if a known-bad probe in the
   same context and on the same route produces a diagnostic. The trigger `Assign` path that returns
   false without a message, the template fixed-point reader that stores zero silently, and the
   target check that skips unknown types are known silent routes.
2. **Test set claims in both directions.** "Accepts only these scopes" needs each listed member
   accepted and each other member rejected. For finite sets (scopes, enums, loaded items), probe
   the complement. For open types (integer ranges), confirm examples and report the claim as
   partial.
3. **Probe outside the guess.** Give every value claim a fixed set of wrong-kind inputs: `yes`,
   an integer, a fraction, text, an empty block and a key from another registry. A form that the
   engine accepts and the config omits is a finding.
4. **Check refutations.** The config is about 98–99% correct by the maintainer's estimate, so a
   small false-refutation rate hides the real errors. Rerun each refutation alone with a corrected
   contrast. Compare it with Native's static answer where one exists. Count the refutations that
   do not survive. An isolated rerun checks repeatability, not correctness.
5. **Freeze selections first.** Choose the claims, their types, their oracles and the probe inputs
   before reading any result. Do not change the extraction rule and rescore the same claims. Known
   answers validate the channel; they are not discoveries.
6. **Keep the config out of the answer.** A claim is credited only by an engine observation or a
   Native answer. Config text never fills a silence.
7. **A stated rule can be a subset.** A message that lists scopes or types may list one permitted
   alternative, not the whole set. Compare a stated set with the resolved declarations of the same
   build; a proper subset is partial.

## Proposed vanilla corpus check

Atlas runs the planned checker over vanilla content with the generated config, for a supported
slice (for example, trigger blocks in one registry), and compares it with the engine. It does not
build another validator. It needs a generated config and a checker, so it starts with the first
config slice. Atlas, or the compiler project, owns it.

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
supported route (a Native question for the ordinary load errors with their sources) is
additional API work; build it only if the corpus needs it.

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
