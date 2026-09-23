# Milestone 2 review: what to cut and what to repair

Status: agreed recommendation, 2026-09-21. It covers Native, Atlas and `pdxscript-rs` at the end
of milestone 2 of the [roadmap](../roadmap.md). Native was reviewed at `89820bd`. Two independent
reviews were combined; the factual claims below were checked in the source. Line counts are
approximate. The live measurements were not run again.

## Finding

Milestone 2 meets its exit gate, and the public design of Native holds. The milestone shows a
useful end-to-end path. It does not yet show that a new registry mainly reuses existing methods.
That is the problem to solve.

- The tradition snapshot has 37 rules and 111 gaps. The ledger credits 20 claims, mostly registry
  existence, loader paths and field existence. Zero differences against the config tells little
  while most questions have no answer.
- Two cracks block the step from one registry to many:
  1. **Fixture observation holds handwritten field knowledge.** `src/binding/groups.rs` holds
     tokens and storage offsets for named fields. `src/fixture.rs` restricts the public request to
     two registries. Engine layouts and function bindings belong in an exact-build binding;
     answers for named fields do not (specification section 5).
  2. **Atlas selects its questions by hand.** `src/extraction.rs` in Atlas lists the registries
     and the field names. Snapshot assembly has matching special cases.
- Three signals together justify action now, with no further audit: the largest feature has the
  narrowest use; about 2,100 lines have no supported caller; registry-specific knowledge is in
  both repositories.

The useful question for each decision below: **what work gives the next ten rules from
unfamiliar registries?**

## Acceptance target

> An unfamiliar registry enters Atlas's static snapshot with no new field list, and a supported
> fixture method transfers with no new field-specific native constants.

A smaller line count is a welcome result, not the target.

## Rule for the cuts

> Keep code that serves current behavior, verifies a current guarantee, or preserves knowledge
> that is not yet transferred. Remove duplicate implementations, obsolete paths, and speculative
> machinery with no current responsibility.

To preserve knowledge does not mean to keep an implementation in the build. Preserve small
representative inputs, expected outcomes, failed cases, and a short note on what they establish.
Then delete the implementation. Git history keeps the old code; preserve useful untracked
material separately.

**No new entries in the fixture table** until the R1 experiment has a result.

## Repairs

### R3. Sweep `registry_fields` over all 164 registries (first)

Run the current method frozen. Record the result before any fix: a case that informs a repair
becomes a regression case and is no longer a held-out test. One unknown-reader rate can mislead,
so record:

| Measure | What it answers |
| --- | --- |
| Successful, failed and unresolved registry queries | Can the method reach the registry? |
| Fields found and unresolved paths | How much of the dispatch was understood? |
| Known and unknown reader identities | Was routing established? |
| Known and unknown reader kinds | Can Atlas interpret the reader? |
| Distinct reader identities and their field counts | Which repair helps the most fields? |
| Time and resource limits reached | Is the sweep practical? |

### R2. Atlas derives its static questions

Atlas loops over `registries()` and asks about the fields that `registry_fields` returns. Remove
the field lists, the `starts_with("tradition")` selection and the `swapped_tradition` special
case from the static path. This does not wait for R1.

Automatic fixture generation is a separate problem. A field name and a reader kind do not give a
valid enclosing definition, companion fields, a valid reference target, or the engine phase.
Atlas generates an experiment only where it has an established recipe for the reader or
structure. A missing prerequisite gives a clear gap. Authored fixtures stay as controls and as
documented exceptions.

### R1. Fixture observation: one bounded experiment

First, state the exact limits in the specification table. "Implemented for two registries" is
too broad: category read-entry observation and tradition storage observation have different
coverage.

Use the R3 result to select the reader shape. Then:

> Allow one working day of implementation and verification effort. Select one reader shape,
> derive its field binding from existing analysis, and verify the required ownership and
> observation boundary. Freeze the method before applying it to unfamiliar fields in another
> registry. No field-specific constants may be added to make those cases pass.
>
> Success means the unfamiliar cases produce correct observations through the public API, with
> unsupported cases reported honestly.
>
> If the experiment fails or reaches the limit, remove the handwritten storage-read path and its
> dependent claims. Preserve its findings as small cases and a note. Keep independently supported
> observations. Further generalization returns as shared-reader work, with a new proposal based
> on the identified obstacle.

The limit includes verification. A method that works only on the development case has not
passed. A destination passed to a reader is not, alone, proof that a later read is safe: object
identity, representation, lifetime and the observation boundary still count. Report which
relationships were derived and which stay manual. A manual exception that stays records its
claim, conditions, obstacle and removal route (specification section 5).

**The fallback can reduce the supported claims in the snapshot.** That is accepted.

### Smaller repairs

| # | Repair | Note |
| --- | --- | --- |
| R4 | One selected back end. Replace the two `Option` fields in `Native` with a private enum; do the same in `Game`. Remove the repeated `if recorded` checks. | The architecture document forbids the pattern by name. No framework is necessary. |
| R5 | Before the next live operation, find where pull requests #15 and #18 wrote the same decision in more than one layer, and give each decision one owner. | A live operation correctly crosses layers. The fault is repeated knowledge, not the file count. |
| R6 | Stable identities. Stop using `Debug` output as snapshot keys in Atlas. Give fault names and the 180-second limit one authority for Rust and Python. | A rename in Native changes snapshot hashes today. |
| R7 | Static `Completeness`. Both static answers always return `Partial`. First define the search boundary; separate omissions inside it from questions outside it; then derive completeness. Keep unresolved paths and unknown readers as gaps where they prevent the promised answer. | A contract decision comes before the code change. Do not change the flag alone. |
| R8 | Update the Atlas pin of Native (4 commits behind). Update the layout in `architecture.md`: it omits `fixture.rs`, `engine/operations/fixture.rs`, `readers.rs` and `references.rs`, and lists a `Declaration` type that does not exist. | Stale. |

## Cuts in Native

| # | Cut | Lines | Reason and conditions |
| --- | --- | ---: | --- |
| N1 | The `references` method: `engine/analysis/references.rs`, `binding/binary/references.rs`, their tests | ~2,100 | No supported operation uses it. First preserve the useful cases, including the two failed resolver shapes, as small data and a note. The failure does not prove that each part needs a rewrite; the note says what worked. |
| N2 | `scripts/watch-codex-reviews.*` | ~590 | Not product code. Low value; it must not delay R3. |
| N3 | The Windows CI job only | small | Windows is deferred. Atlas CI runs on Linux, so the Linux job and `platform/unavailable` stay. The `cfg_attr(..., allow(dead_code))` attributes also apply on Linux, so this cut does not remove them. |
| N5 | Registry-specific public names: `CategoryFieldReads`, `InitialCategoryLoad` | small | Rename when the behavior is general, or remove with the R1 fallback. |
| N6 | The `pdx_native::internals` module, as far as possible | — | Move implementation tests inside the crate, so that their needs do not define public exports. |
| N9 | Words from the cut design: `qualification_controls`, `artifacts`; the names `tools/evidence.py` and `docs/native-evidence.md` | small | The names mislead. Modest priority. |

### Changes to guarantees (decide each one separately)

- **N4. Reservation journal.** Nothing retires disposed entries, each launch reads all of them,
  and one damaged entry blocks all later launches. Replace it with the OS lock plus the process
  inventory. This is a lifecycle-policy change, not a cleanup: an unresolved earlier attempt no
  longer blocks a launch after the process checks pass. Supervisor-loss recovery is already out
  of scope, so the change is acceptable. It amends specification section 4. Verify conflict
  refusal and cleanup reporting. Keep "the game is gone" and "bookkeeping failed" as separate
  results.
- **N7. Worker transport.** Candidates to cut: the hash of each worker file, if package identity
  is already established; the fsync calls, because session transport is temporary and not
  recovered. Keep atomic publication, size limits, session identity and stream continuity: they
  protect answer integrity, and a supervisor that starts its own worker still races with it.
  `codec.py` and the generated `protocol.py` are one source, not two checkers; no cut there.
- **N8. Live suite (52 cases, about 30 minutes).** Move pure reducer and transport cases into
  fast tests. Add the small fake-worker session test (Testing Decision 6) when supervision
  changes. Keep representative live wiring checks. One case for each broad failure label is too
  weak: worker loss before activation and after partial observation test different guarantees.
  Remove a fault branch from `worker.py` only when no live case needs it.

Do not cut: target composition, exact-build identification, supervisor ownership and disposal,
the `registries` and `registry_items` methods.

## Cuts in Atlas

| # | Cut | Lines | Reason and conditions |
| --- | --- | ---: | --- |
| A1 | `prototypes/native-registry` | ~2,100 | Historical. First check that the production tests keep its useful acceptance cases. |
| A2 | The provenance module and its tests, out of the product crate | ~1,300 | Settled: the measurement is finished, and milestone 3 replaces the logs that it measured. Keep the report, the input identities and the reproduction instructions. If a runnable copy is useful, make it a separate research tool outside the product build and the normal tests. |
| A3 | The older coverage snapshot model (`coverage::Snapshot`, `coverage/rules.rs`, `registry_observations`) | ~500 | Two models exist, and this one reads the replay-era JSON. The ledger reads the real snapshot. |
| A4 | Linear ticket routing in `ledger/classify.rs` | part of 467 | Keep claim classification and ownership classes. Remove only the ticket identities. |
| A5 | Linear ticket numbers and the 16 fixed "outside this extraction" gaps in the published snapshot | small | A gap needs a reason, not a ticket. |
| A6 | Stale documents: `docs/planning/offline-snapshot-contract.md`, `repository-packaging-release-ownership.md`, the evidence terms in `CONTEXT.md` and `AGENTS.md` | ~600 | Keep decisions that still apply. Modest priority. |

Keep `extraction.rs`, `assemble`, `verify`, `compare`, the recorded-answer tests and the
three-command CLI. Do not grow the snapshot schema until a consumer reads it.

`pdxscript-rs` needs no cuts. Remove the absolute local path from its `VERIFICATION.md`.

## Linear

SDK-470 has 22 backlog "Validate …" child tickets, and most have a twin in milestones 3 to 6 (for
example SDK-490 and SDK-541). Recommended: one ticket for each topic, with the open question as a
paragraph in the milestone ticket. This inventory was not checked by the second review.

## Order of work

1. Record the exact limits of fixture observation in the specification table (R1, first part).
2. R3, with the current method frozen.
3. R2: Atlas static collection follows discovered registries and fields.
4. The R1 experiment, selected from the R3 result, with its binding outcome.
5. A3, R4 and R6: one snapshot path, one back end, stable identities.
6. Confirmed obsolete code, with knowledge preserved first: N1, A1, A2, then N2, N3, N5, N6,
   N9, A4, A5, A6, R8.
7. N4, N7 and N8, each as a separate change with checks tied to its guarantee. R7 after its
   contract decision. R5 at the start of the next live operation.

Milestone 3 declaration work can proceed in parallel. It depends only on the static context.

## Deferred

Held-out transfer tests (Testing Decision 8) as a general framework wait for the first
milestone 4 method. R1 and R3 carry the small transfer checks that are necessary now. The update
rehearsal (Testing Decision 10) stays last, as the roadmap says.

**Superseded, 2026-09-23:** the held-out framework is not built. The
[development policy](../development-policy.md#keep-engine-knowledge-in-its-home) replaces the
per-ticket freeze with a locality rule on method code and one run over every registry at the end
of each method ticket. The R1 and R3 results stay as findings.
