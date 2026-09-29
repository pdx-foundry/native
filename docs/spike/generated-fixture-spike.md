# Spike: learn Stellaris rules from generated fixtures

Status: completed on 2026-09-28 after user approval and an authorized extension. The reviewed
proposal below is retained. The [Native diagnostic findings](../native/diagnostic-survey.md) and
Atlas report at `docs/prototypes/generated-fixture-spike/REPORT.md` record 24 launches, the full
question/message matrix, transfer scores and limits. The user allowed promising work beyond the
initial limits; the final run stayed within the original overall launch and live-time ceilings.

The user's subsequent clarification sets the first priority: determine what the engine tells us
in its error messages. The numeric experiment below is a possible follow-up, not the starting
assumption about which questions fixtures can answer.

## Question

Can automatically chosen fixtures recover useful modding rules with less work than deriving each
rule from engine code?

First ask: **when deliberately given the wrong type, scope, key, or reference, does Stellaris
report the expected rule?** A message that names allowed scopes or an expected type may answer
the question directly. A generic error may still distinguish candidates across controlled probes.
We must measure this before deciding that types or scopes need executable analysis.

The idea is “fast-check for Stellaris”: generate controlled scripts, observe the engine, and reduce
surprising cases to small examples. Property testing normally checks a supplied rule. This spike
also needs an inference step: keep several possible rules, choose inputs on which they disagree,
and remove explanations contradicted by observations.

The hypothesis is that one reliable observation method can support many inexpensive experiments.
The spike must measure the cost of getting reliable observations as well as the cost of generating
inputs. It does not assume that fixtures can replace all executable analysis.

## Existing foundation

Native already mounts consumer-authored fixtures and reports separate parsing, storage, and
diagnostic observations. See [the fixture types](../../src/fixture.rs),
[the live fixture tests](../../tests/live.rs), and
[the numeric conversion matrix](../../tests/live/numeric.rs).

The numeric matrix already tries signs, fractions, malformed text, overflow, and repeated writes
on integer and fixed-point readers. It checks finite observations against recorded expectations
and static representation facts. The [numeric conversion notes](../native/numeric-conversion.md)
also record a later change to `build_time`: nonpositive stored values become one after the member
returns. The same reader on `war_exhaustion` retains negative values. Those are existing findings,
not discoveries this spike can claim. The proposed addition is a diagnostic survey, followed by
direct rule extraction or controlled rejection probes where the survey supports them. Searching
for later storage changes is a separate optional experiment.

Current constraints matter:

- A request supplies one text file, at most 64 KiB, and at most 32 field questions. Inputs are fixed
  before launch; another adaptive round needs another session.
- Parsing, diagnostics, and typed storage have separate availability and completeness. String,
  integer, and fixed-point storage can be observed only where the engine bindings support them.
- Deferred validation can be requested where supported. An initial file-load result says nothing
  about checks after that window.
- The current fixture method loads no world. A requested runtime result is unavailable.
- Findings belong to the exact executable identified by Native. They do not automatically transfer
  to another build.

These are existing capabilities, not results of this spike. The
[consumer contract](../design/atlas-caller-migration.md#block-and-command-grammar-migration-sdk-542--sdk-597--sdk-625)
and [specification](../specs/native.md) govern how observations may support conclusions.

## First experiment: what do engine errors reveal?

Start with a diagnostic survey, using existing working fixtures and known rules as controls.
Rediscovering a known rule is useful here: the question is whether the diagnostic channel exposes
that rule, not whether the rule itself is new. Do not require a storage decoder for a question
answered by a source-correlated diagnostic.

Before launching, inspect known diagnostic format strings and their argument construction on the
exact build. Start with retained inspections, then use the existing inspector where necessary;
do not turn this into a new whole-engine analysis method. For example, the retained SDK-548
`CEffect::ReadMember` inspection contains `Wrong scope for effect '%s' at %s\nCurrent Scope: %s\n%s`.
What fills the final placeholder is a useful first question. Record this as static evidence,
separately from a rendered live message. Missing keywords in string searches cannot prove a rule
is absent: messages can be assembled dynamically or emitted through another route.

For each selected sample, identify the expected reporting route and whether the current Native
bindings observe it in the requested window. Record an unresolved route explicitly. Compare with
the isolated game's ordinary error log when available; a log-only diagnostic identifies a capture
gap. Complete collection from bound hooks does not establish that every possible engine error
route was covered. A positive control must exercise the route relevant to the tested mistake.

Existing evidence provides a partial answer:

- [Numeric recordings](../../tests/expected/numeric-m45/live.json) contain `Malformed token` for
  `not_a_number` after `7` on the direct integer and fixed-point samples. This message alone does
  not distinguish those types. The template fixed-point sample instead stores zero without a
  diagnostic in that observed window.
- [Live scope controls](../../tests/live.rs) use `is_planet_class = pc_barren` and
  `set_planet_class = pc_barren` in country contexts. The
  [parser observation notes](../native/command-grammar.md#parser-observation) record source-located
  wrong-scope diagnostics. These tests establish rejection, not that the message enumerates all
  permitted scopes. Read or capture the full message to answer that question.
- [Command argument fixtures](../native/command-grammar.md#live-fixtures-of-complete-grammars)
  already reject non-Boolean values and unknown block keys. Their summary records the diagnostic
  route; it does not establish how much type or grammar information the rendered text contains.

The examples `set_deposit = d_energy_5` in country scope and `cost = "ten"` are proposed questions,
not verified engine messages or type claims. Resolve a valid command/reference and an exact owning
registry/field before probing them, so an unrelated error cannot hide the intended failure.

| Deliberate mistake | What to look for in the full message | What it could answer |
| --- | --- | --- |
| Wrong current scope, with otherwise valid arguments | Actual scope; required scope; complete allowed set or only one example | Command input scopes; possibly a block's supplied scope |
| Wrong target argument scope | Argument name; expected and actual target types | Target scope constraints, separately from the current scope |
| Text supplied to a numeric or Boolean field | Expected type, accepted spellings, offending token | Broad type or a finite domain |
| Scalar supplied to a block, or block to a scalar | Expected form; child family; named reader context | Value/block alternatives and block family |
| Wrong-family child or unknown block key | Trigger/effect/modifier context; expected keys or alternatives | Child grammar and dispatch family |
| Missing key or unknown enum/reference value | Required key; allowed values; target database | Required fields, enum domain, reference target |

For each selected row, start with a passing control and change one factor. Prioritize current
scope, then target scope, then numeric/Boolean type errors. Select target arguments with an
established parsing or validation check stage; execution-only or unresolved stages do not qualify
for this first survey. Reserve additional commands and contexts for the follow-up when they do not
fit the survey budget. For generic type errors, compare several input kinds on the same field.
Preserve the complete multiline message,
input, source location, observation stage/window, exact build, and relevant coverage. Distinguish
ordinary game-log text from text available only through Native's intercepted diagnostic routes.
Record whether the current Native observation truncates or omits any useful detail.

Classify every result as one of:

1. **States a rule:** names an expected type, allowed scope set, required key, or lookup target.
2. **Rejects this candidate:** identifies a mismatch without revealing the full rule.
3. **Generic failure:** an error occurred, but its text cannot identify the tested property.
4. **Silent on covered routes:** the relevant controls and collection completed, but no matching
   diagnostic was observed. This is not a claim of silence through every engine route.
5. **Observation unavailable:** an unobserved or unresolved route, unsupported window, missing
   source join, or incomplete run prevents the intended check.

Check stated rules against known controls before using the text as an automatic answer. A listed
scope or type may be one permitted alternative rather than an exhaustive set. Compare stated scope
sets with resolved declarations on the same build; a proper subset is partial, not a correct
complete answer. Correct the input
according to the message and rerun to confirm that the intended diagnostic disappears. Silence
alone still does not prove acceptance or runtime success.

Run this survey through parsing and deferred validation where existing support permits; record
which messages require each phase. Reuse a known valid wrapper and all required arguments.
An unknown-key error can disrupt later parsing, so isolate destructive parser cases rather than
assuming every invalid case can share a batch. No running-world harness is required for this survey;
mark errors that can only occur at execution as outside its present reach.

The output is a question-to-diagnostic matrix with verbatim examples, message richness, coverage,
and a proposed method for each question: direct error extraction, a sequence of rejection probes,
or engine analysis/additional observation. Include all tested cases in the denominator. Measuring
this channel is a successful first-stage result even if it yields no new engine rule.

## Choose the next method for each question

| Survey result | Follow-up | Stop or limit |
| --- | --- | --- |
| States a rule and agrees with controls | Extract rules for three additional subjects selected before reading their messages; compare against independently established answers | Stop promotion for that message family at the first false rule or subset reported as complete. Keep partial results partial. |
| Reliably rejects the tested candidate | Run a finite matrix of contexts or input forms on three additional subjects, with working controls and one-factor changes | Stop that question on an ambiguous cause, unstable result, missing relevant coverage, or exhausted budget. Untested candidates stay unknown. |
| Generic failure with no reliable connection to the tested property | Use targeted engine analysis or identify an additional observation needed | Do not infer a type or scope from this message. If controls later isolate its cause, reclassify it as candidate rejection. |
| Silent on covered routes or observation unavailable | Record the precise observation limit and use engine analysis where useful | No acceptance inference and no automatic expansion into a runtime harness or new hooks. |

For a scope rejection matrix, keep command arguments valid and vary only the known input context.
For a type matrix, keep the field and wrapper fixed and vary Boolean, integer, fractional, text,
and block inputs. Classify each tested cell as rejected, accepted within the verified parser or
validation contract, or unresolved. Absence of an error alone cannot fill an accepted cell; require
witnessed parsing, controls, and relevant diagnostic coverage. Do not turn a finite set of accepted
examples into an exhaustive grammar. Only report a complete scope set if the candidate scope
inventory and every relevant cell are established; otherwise return a partial set.

A branch earns a wider trial if its three additional subjects agree with the independent controls,
it makes no false definitive claim, and it produces a useful direct rule or reproducible distinction
within budget. If fewer than three subjects can be evaluated, report a promising but incomplete
trial. This evaluates the alternative method; new engine knowledge is a separate result. Isolate
and rerun messages or patterns before using them as rules, rather than isolating every survey sample.

## Possible follow-up: numeric field behavior

This is a separate optional question about numeric storage, not the fallback for failed type or
scope extraction. Prioritize the diagnostic branches above. Its steps and success criteria apply
only if it is selected within the remaining budget.

The discovery question is: which observable numeric fields change their last stored value before
file load ends, and can a small number of additional probes explain those changes?

First inventory candidate fields using existing static answers, fixture bindings, and recorded
live evidence. The [numeric population example](../../examples/numeric-population.rs) identifies
numeric readers; its counts do not establish live fixture coverage. Report separately fields with
compatible decoders and owner/load bindings, fields with prior complete live observations, and
fields whose coverage the spike has newly demonstrated. A static inventory cannot prove the last
category. Do not add decoders or owner joins in this experiment.

Use `common/megastructures/sensor_range` and `build_time` only to calibrate the runner against known
answers. Then select up to eight other candidate fields, preferring the same registry to reduce
launches, across the supported integer and fixed-point readers. Record what is already known about
each selected field before running it. If no additional fields have a plausible observation path,
stop with a measured reach limitation. Small reach can justify a narrow result, not a broad claim.

Give each field a fixed initial probe set: `-1`, `0`, `0.25`, `0.75`, `1`, `1000000`, omission,
and `7` followed by `not_a_number`. These are input experiments, not claimed valid values.
Keep unrelated fields at their working control values. For each non-omitted case, compare the
**last occurrence value** with `final_value`; earlier occurrences naturally differ after another
assignment. A difference flags a later change for investigation, not automatically a clamp.
An equal value means only that these probes found no change in this observation window.

For flagged fields, choose additional values that distinguish identity, lower or upper clamps,
nonpositive-value substitution, and other explanations. Keep “none of these explanations” explicit.
Freeze these generic candidate families and the reserved-input selection rule before seeing new
field results. Do not customize them to match each field's answers. Exercise values on both sides
of a proposed boundary; do not assume monotonic behavior merely to enable binary search. Keep
numeric comparisons exact in stored units.

If the broader search yields no flag beyond the known controls, the `build_time` response to
representable values strictly between zero and one is a fallback bounded question: those cases
are absent from the existing numeric matrix. Check other existing evidence before calling this a
gap. Resolving it alone would show narrow usefulness, not transfer across fields.

Use `InitialFileLoad`, as the existing numeric matrix does. This experiment makes no deferred
validation or runtime claim and does not depend on `through_validation()` support for these fields.

## Numeric experiment loop

1. Run a known working control and a known diagnostic-producing control. Confirm that the requested
   parser and diagnostic observations actually cover the intended file and phase. Establish storage
   coverage separately for questions that use it.
2. Begin with candidate explanations and choose a small input set that separates their predicted
   outcomes. Use deterministic choices first; seeded random variation can search around remaining
   boundaries and combinations.
3. Generate uniquely named definitions in the valid wrapper. Change one factor at a time. Batch
   independent definitions within the existing request limits, and include controls in each batch.
4. Run through Native and retain the observed dimensions separately. Update candidate explanations
   only from complete, relevant observations. Missing hooks, incomplete parsing, unjoined errors,
   timeouts, and worker loss leave the affected question unresolved.
5. Spend reduction runs only on multi-assignment cases. Keep the wrapper, relevant prior
   assignments, and observation coverage intact. The reduced case must reproduce the same
   contradiction, not merely a parse error or crash. Scalar boundary search is the useful operation
   for the numeric trial; general script shrinking can wait for a nested-grammar experiment.
6. Re-run every proposed finding alone in a fresh session and with a nearby contrasting input.
   If a batched case differs when isolated, record the interference and withhold that conclusion.

No diagnostic is not, by itself, acceptance. A parser return does not establish validity. A stored
value establishes a storage observation, not successful runtime use. Report bounded statements such
as “these decimal inputs stored these integer values during file load.” Keep the proposed general
rule separate from its observed examples and unresolved alternatives.

Occurrence storage and file-terminal storage establish behavior at different points. Do not turn
a field-specific change between those points into a rule for every field sharing its reader.

## Independent evaluation

For the diagnostic branches, use resolved scope sets and the Boolean/key grammars exercised by
`fixture_argument` as controls. Select the three follow-up subjects and candidate input/context
matrix before reading their messages. Check extraction and rejection conclusions against those
answers, counting partial and unavailable results separately. Known answers validate the channel;
they are not new discoveries. Do not revise the extraction rule and rescore the same subjects as
independent validation.

For the optional numeric branch, use the existing numeric matrix as a calibration baseline.
Give the inference procedure field locations, valid
wrappers, input families, and live observations. Do not give it the existing expected outcomes or
the static conversion conclusions being evaluated. Native may still use its normal static machinery
to locate and decode observations; this is not an independent test of those decoding internals.

For that numeric branch, select three known behaviors from the matrix to check the runner. Recovery proves
calibration only: the experiment author has already seen these answers. Use the frozen candidate
families unchanged for discovery and reserve additional inputs by the predeclared selection rule.
Report incorrect and unresolved predictions as well as correct ones; do not change the candidates
after seeing reserved outcomes and then rescore those same inputs as independent checks.

An isolated rerun checks repeatability, not independent correctness. Corroborate a proposed new rule
with targeted inspection of the relevant engine path, or a separately justified observation method
that can distinguish that rule from its alternatives. A second field using the same decoder does
not independently verify that decoder. Without corroboration, retain a repeatable candidate and its
bounded observations; do not promote it to a confirmed general rule.

For one question in the selected follow-up branch, measure a time-boxed attempt to derive the same behavior from
engine code before revealing the generated answer. Record any prior familiarity with that path.
Compare analyst time, setup time, and live execution separately, including corroboration work.
Cap this baseline attempt at two hours within the overall budget. An unfinished attempt gives a
lower bound on cost, not an invented completion estimate. Historical SDK-643/644 effort may provide
context if recorded, but is not a substitute for a comparable measurement. If no comparison is
feasible, leave the claim that generation is cheaper unresolved.

## Budget and decision

Time-box the implementation to two engineering days, with at most 40 game launches and two hours
of total live execution, whichever limit is reached first. These are proposed experiment limits,
not estimates of current performance. Controls, isolation runs, reduction, and failed launches all
count. Stop scheduling runs when the remaining budget cannot cover another configured session.
Reserve at most ten launches for the diagnostic survey. Use the remaining thirty only for the
follow-up selected from its results, including calibration, adaptive probes, isolated repetitions,
and corroboration that needs live execution.

Plan the survey sessions before launch, using these allocations:

| Priority | Samples | Maximum launches |
| --- | --- | ---: |
| 1 | Current-scope errors and valid controls; corrected rerun | 2 |
| 2 | Parsing/validation target-scope errors and valid controls; corrected rerun | 2 |
| 3 | Numeric/Boolean type errors and controls; corrected rerun | 2 |
| 4 | One isolated scalar/block or child-key/family rejection, then its corrected control | 2 |
| 5 | One isolated missing-key or enum/reference rejection, then its corrected control | 2 |

Combine only known non-disruptive log-route cases within the same registry file. Each isolated
parser rejection gets its own session; put its passing control before it or in the corrected run.
Write the exact cases and question count for each planned file before execution. If the packing
does not fit or a retry is needed, drop priority 5 first, then 4; do not spend beyond ten launches
to fill the table. Uncovered mistake families remain untested. Unused allocations can fund a
higher-priority route. Historical validation runs took roughly 83–91 seconds each; measure fresh
timings rather than treating that as a guarantee.

Batch only within one
registry file; the 32-question limit counts every field on every generated definition, including
controls. Eight fields with eight sequences need at least 64 questions before controls. Record the
actual packing and session count; do not assume thirty fields fit in one launch.

Run long experiments through the `run-and-queue` skill's bundled wrapper.

Report:

| Measure | Purpose |
| --- | --- |
| Diagnostic richness by question, with exact messages and required observation stage | Decide which questions error messages can answer |
| Correct, incorrect, and unresolved predictions on reserved inputs | Detect plausible but wrong rules |
| Known behaviors recovered and independently confirmed new findings | Separate calibration from discovery |
| Unique cases, launches, total elapsed time, and reduction runs | Measure actual experiment cost |
| Setup effort and any missing Native observation capability | Expose the cost hidden by generation |
| Candidate, previously observed, and newly observed field counts | Measure usable reach without equating static and live coverage |
| Analyst time for the same engine-code question, with prior knowledge noted | Test the claim of reduced reverse-engineering effort |
| Batched versus isolated disagreements and repeatability | Detect contamination or unstable observations |
| Additional knowledge beyond the existing matrix | Establish whether automation adds value |

For a diagnostic follow-up, apply the branch criteria above and report
correct, incorrect, partial, and unavailable results and the cost per useful answer. Do not make
numeric storage success a prerequisite for exploring informative scope or type messages.

For the numeric follow-up, proceed to a wider discovery trial only if the runner recovers all three calibration behaviors
without false definitive conclusions, preserves unknowns for incomplete observations, and produces
at least one corroborated finding on a field outside the calibration pair within budget. Report
the cost comparison separately: a discovery success does not by itself prove the method is cheaper.
Prediction scores must include all reserved cases, including unresolved ones.

If the numeric calibration succeeds but no new finding is confirmed, retain that result as a possible
regression testing tool; numeric discovery remains unproven. This does not invalidate a successful
diagnostic survey or extraction/rejection trial. If observation support or launch costs consume the budget,
report the specific obstacle and the measured cost before proposing more infrastructure. A negative
result is a valid outcome of the spike.

## Limits on broader type and scope conclusions

Broader type inference would combine accepted syntax, stored representation, validation, and runtime
meaning. These may differ. Numeric success alone cannot establish enum values, reference targets,
conditional required fields, or arbitrary nested grammar.

The diagnostic survey and any later scope experiment should keep three questions separate:

- Which input scope permits a command?
- Which scope types may its target argument reference?
- Is a suitable target object available when the command executes?

Native already answers much of the first two through declarations and command grammar, including
target check stages. The [scope notes](../native/engine-commands.md#supported-scopes) retain
unresolved cases; they do not establish every command's scope. The
[command grammar notes](../native/command-grammar.md) also describe existing `cargo live fixture_argument`
checks. Known answers are useful controls for measuring diagnostic information. A later discovery
claim must concern a previously unresolved scope set, check stage, or context; recovering known
answers measures the alternative method's accuracy and cost.

Require a known wrong-scope control that produces a diagnostic in the selected observation window.
Use valid command arguments and otherwise equivalent contexts, so an unrelated error cannot
masquerade as a scope restriction. Runtime-only checks and actual target availability need a
controlled world and a witnessed execution path. A general runtime harness is outside this spike;
learning scope restrictions from existing parser/validation diagnostics is explicitly in scope.

## Ownership and deliverables

Atlas owns fixture generation, experiment selection, inferred rules, and coverage. Native owns
mounting, process lifetime, and build-specific observation methods. This proposal lives here because
it evaluates Native's observation boundary; a runner should live with Atlas's extraction work.
Do not add a second process supervisor or a new Native evidence/replay API.

Use the existing consumer route first. TypeScript and fast-check are optional implementation tools;
a language bridge must not consume the spike. The useful fast-check concepts are composable
[generators and shrinkers](https://fast-check.dev/docs/core-blocks/arbitraries/). An inference layer
and an engine observation contract are still required.

Deliver the diagnostic matrix first. For the selected follow-up, deliver a small runner,
reproducible input seeds or selection history, minimal fixtures supporting
findings, and a short result report containing the measurements and a proceed/stop recommendation.
Keep enough source and observations to reproduce new knowledge under the
[preservation policy](../development-policy.md#preserve-acquired-knowledge). Record engine findings
on the appropriate Native knowledge page and rule conclusions with Atlas. No production integration,
roadmap commitment, or game launch is part of writing and reviewing this proposal.

## Review disposition

Claude reviewed the proposal twice through LARP session `7b7f1b79-875d-468e-8545-c16008e307ad`.
The re-review found the diagnostic survey concrete enough to run, but requested explicit follow-up
branches, route coverage distinctions, and session budgeting. Those repairs are incorporated here;
this last edit has not had another external review. No live experiments ran during these reviews.
