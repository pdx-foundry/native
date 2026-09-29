# Spike: test the config against the engine

Status: proposed on 2026-09-28. Not reviewed. No game launch was part of writing it.

This follows the [generated-fixture spike](generated-fixture-spike.md). That spike found that
wrong-scope and Boolean messages state their rule exactly, and that key, numeric and reference
messages reject a candidate but do not state the rule. A later one-session trial showed that the
engine can parse and validate trigger text inside the paused game, without a new launch per probe.
See [In-process results](#in-process-results) for the facts this spike builds on.

## Question

Can the engine act as an automatic test suite for a hand-written config? The config supplies each
claim. The engine, or Native's static answer, confirms or refutes it. The goal is a report such as:

- The megastructures registry is missing the field `new_field`.
- The config has no registry `new_feature`.
- The effect `set_deposit` is not valid in the `country` scope.
- The config has no effect `new_effect`.
- The field `cost` stores an integer, not a fixed-point number.
- The config lists the scope `pop`, which no longer exists.

The config is the first guess, not the source of truth. By the maintainer's experience with the
pdx-ts-sdk overlay, `cwtools-stellaris-config` is about 98–99% correct. That is an estimate, not
a measurement. The value of the method is in finding the remaining errors without false alarms.

## Principle: every claim needs an oracle

A claim is a test only when something from the engine can answer it. There are three kinds of
answer, and they answer different claims:

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
5. **Untestable.** No oracle in this spike can answer this kind of claim.

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
4. **Check refutations.** With a 1–2% error rate, a small false-refutation rate hides the real
   errors. Rerun each refutation alone with a corrected contrast. Compare it with Native's static
   answer where one exists. Count the refutations that do not survive.
5. **Freeze selections first.** Choose the claims, their types, their oracles and the probe inputs
   before reading any result. Do not change the extraction rule and rescore the same claims.
6. **Keep the config out of the answer.** A claim is credited only by an engine observation or a
   Native answer. Config text never fills a silence.

## In-process results

These facts come from one bounded trial on M45-release (five launches, all with confirmed
disposal). The worker patch, the knowledge note and the scripts are retained:

- Branch `spike/in-process-probe`, commit `7b4c8ad`: the worker patch and the section
  "In-process parse probes" in `docs/native/engine-calls.md`.
- `.local/evidence/in-process-probe-2026-09-28/`: `scripts/helpers.py`, the probe scripts,
  the Rust runner, `worker.diff` and all five sessions' results and error logs.

Facts to reuse:

- The console's `trigger_file` route (`ReadAndEvaluateTrigger`) works at the loaded-modifier
  pause without a world, up to validation: memory text → `CReader` → `CTrigger::Read(reader,
  scope)` → `CTriggerDatabase::PostInit` → `PostValidate`. `EScopeType` is a bit value (planet 2,
  country 4, fleet 64).
- One trigger probe takes about 0.19 s. A 32-scope matrix for one trigger took 9.9 s.
- Diagnostics appear in the ordinary `error.log` at once, with the line inside the snippet. A
  destructive key error does not affect the next probe.
- Pitfalls: the ordinary log drops a message identical to the one before it, so give each probe a
  distinct line; the supervisor needs a pause confirmation every 2 s, so run probes on a separate
  worker thread; after an expression, compare registers, not a cached frame 0.

## Experiments

Run them in this order. Each has its own stop condition. A failed experiment ends only its branch.

### E0. Port the probe harness

Rebase `spike/in-process-probe` on current `main`. Replace the frame-0 check with a register
check (`pc`, `sp`, `fp`, `lr`). Keep the harness out of Native's public API and keep it on the
spike branch; see [Ownership](#ownership-and-deliverables).

Pass: the level-1 trigger controls from the trial repeat in one session, and the session ends
with confirmed disposal after at least five minutes of idle refreshes.

### E1. Collect the engine dumps

`modifiers.log` is written at every launch, at `CModifier::LogDefinitions`. The other four logs
(`effects`, `triggers`, `scopes`, `localizations`) come from the console handler
`OnExecute_PrintTriggerDocumentation`. At the pause, call that handler in-process with an empty
`CPdxArray<CString>` argument. It is not known whether the handler needs a game state; read its
body with `examples/inspect` first.

Compare each dump with Native's static answer: `declarations` (effects, triggers), `scopes` and
`scope_links`, `modifiers` and `Game::loaded_modifiers`, and `localization_declarations`. Report
agreements and differences in both directions.

Pass: all five logs are written in one session. Stop: the handler needs a world; then record that
and use Native's static answers alone.

### E2. Inventory differences

With no launch beyond E1, compare the config with each inventory:

| Inventory | Native or dump source | Config source |
| --- | --- | --- |
| Registries and their directories | `registries` | Ledger `loader_path` claims |
| Fields per registry | `registry_fields` | Ledger field-presence claims |
| Effects, triggers | `declarations`, E1 dumps | `effects.cwt`, `triggers.cwt` |
| Scopes, scope links | `scopes`, `scope_links`, `scopes.log` | `scopes.cwt`, `links.cwt` |
| Modifiers | `Game::loaded_modifiers`, `modifiers.log` | `modifiers.cwt` |
| Localization commands | `localization_declarations`, `localizations.log` | `localisation.cwt` |
| On_actions, game rules, defines | `on_actions`, `game_rules`, `defines` | `on_actions.cwt`, `game_rules.cwt`, defines claims |

Classify each difference as: missing in the config, stale in the config, a Native gap (a partial
answer that cannot establish absence), or a naming difference (alias or keyword). A partial Native
inventory can report "missing in the config" but never "stale in the config".

Then check a frozen random sample of 20 differences per inventory by hand, with the inspector, or
with a probe. Report how many are real.

### E3. Effect route without execution

Repeat the trigger route for effects, stopping before execution. Read `ReadAndExecuteEffect`
with the inspector: it uses `CEffectDatabase::PostInit` and `PostValidate`, as the trigger route
does. Controls:

| Probe | Scope | Expected |
| --- | --- | --- |
| `change_pc = pc_barren` | planet / country | quiet / wrong-scope message |
| `stop_crisis_sound = yes` / `= native_not_boolean` | country | quiet / named Boolean message |
| `every_owned_fleet = { auto_follow_fleet = { target = this } }` | country | quiet |
| same with `target = owner` | country | `has a target of not fleet type!` |

Pass: all four pairs behave as expected, and a good probe after each bad probe stays quiet. This
opens the load-time target checks (about 18 commands have a target message at `PostValidate`).
Execution-stage checks stay outside this spike.

### E4. Claim tests: calibration

This is the main experiment. It measures how many known errors the probes catch and how many false
alarms they raise. Limit it to claim types that the probes can answer:

- supported scopes of an effect or trigger
- Boolean value domains
- existence of a fixed key in a command block
- the target registry of a reference argument
- enum values drawn from loaded items

Build three frozen sets before any probe runs:

1. **Known errors.** The pdx-ts-sdk overlay rows that correct a game fact for triggers or effects
   (`packages/codegen-cwt/src/overlay/script.ts`: `EFFECT_FIELD_ADDITIONS`,
   `EFFECT_FIELD_CARDINALITY_OVERRIDES`, `EFFECT_VALUE_TYPE_OVERRIDES`). Label each one with its
   claim type. Cardinality rows are expected to be "no comment"; keep them in the denominator.
2. **Seeded errors.** 30 believed-right claims, each changed in one way: an extra or missing
   scope, a key renamed, a reference pointed at another registry, an enum value removed. This gives
   a controlled recall measurement, because there are few real known errors of these types.
3. **Believed right.** 50 other ledger claims of the same types, with no overlay row. These
   measure false refutations.

Mix the sets and run them without the set labels. For each claim, generate its probes with
rules 1–3. Score each claim with the five results, then remove the labels.

### E5. Scope canaries (optional)

A wrong-scope message states `Current Scope: X`. Put a deliberate wrong-scope trigger inside a
block, and the message reports the scope that the block supplies.

- In-process: `owner = { <canary> }` in several input scopes. This checks link output scopes
  against `scope_links` and `scopes.log`.
- One fixture: a canary inside a tradition's `potential` and `on_enabled`. This checks a registry
  block's entry scope, which Native does not yet establish for most fields.

Pass: every canary reports the expected scope for its controls, and a block whose context is
dynamic reports something that is recorded, not guessed.

## Measures

| Measure | Purpose |
| --- | --- |
| Dump contents compared with Native's static answers, both directions | Decide whether dumps are free cross-checks |
| Inventory differences by class, and the hand-checked sample rate | Decide whether "missing" and "stale" reports can be trusted |
| Known-error recall per claim type | Which real config errors probes catch |
| Seeded-error recall per mutation type | Controlled recall |
| False refutations on believed-right claims, before and after rule 4 | Precision at a 1–2% base rate |
| Share of "no comment" and "untestable" per claim type | Where static analysis must answer instead |
| Probes per claim, seconds per claim, launches | Cost |
| Refutations that did not survive isolation | Interference between probes in one session |

## Decision

Proceed to a full-ledger run for a claim type only if:

- rule 4 leaves zero false refutations among its believed-right claims, and every surviving
  refutation is explained;
- it catches at least 80% of its seeded errors; and
- its "no comment" share is stated.

Report the other claim types with their obstacle. A claim type that is mostly "no comment" goes to
static analysis (required fields, repeats, conditional reading), not to more probes. Inventory
checks proceed on their own if the E2 sample shows that the reported differences are real.

## Budget

At most 10 game launches and 60 minutes of live time, and two engineering days. One session can
hold probes for up to 25 minutes, so most experiments need one launch. Failed launches count.
Stop scheduling when the rest of the budget cannot cover another session.

## Limits

- Everything applies to M45-release only. Record the config fork revision that the ledger uses.
- No world is loaded. Execution-stage checks, trigger evaluation and modifier application stay
  out of scope.
- Type schemas get inventory differences (E2) and, optionally, one canary fixture (E5). In-process
  probes of registry fields need each registry's own member reader; that is a later experiment.
- The ordinary-log capture is enough for a spike. A production route would hook
  `CPdxLogFileAndLine` instead of reading the file.
- The new config format is out of scope. The spike records one JSON row per ledger claim
  (question ID, claim type, oracle, probes, result). That row structure is input for the format
  design.

## Ownership and deliverables

Atlas owns the claims, the selections, the probe runner, the results and the rule conclusions.
Native owns the in-process observation route and its build-specific addresses. Read the
[simplification decision](../design/simplification.md) before any of this becomes a Native operation: it
must be a normal answer with gaps, with no evidence descriptors, replay paths or artifact hashes.
The worker patch stays on the spike branch until then.

Deliver:

- the E1 dumps and the E2 difference lists, with the hand-checked sample;
- the frozen E4 sets, the per-claim result rows and the scores;
- a short report with the measures above and a proceed or stop decision for each claim type;
- engine findings on the Native knowledge pages (`engine-calls.md`, `diagnostic-survey.md`), and
  rule conclusions with Atlas.

Preserve the scripts, results and logs under the
[preservation policy](../development-policy.md#preserve-acquired-knowledge).
