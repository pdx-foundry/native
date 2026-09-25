# Milestone 3 review

## Executive summary

**Milestone 3 met its exit gate. Proceed into Milestone 4 after a short preparation pass.**
The review found a sound base: Native supplies engine-derived answers, Atlas keeps unresolved
claims visible, and shared methods are replacing case-specific work. The recorded live coverage
is 26.33% (15,278 of 58,032 claims). That result belongs to the measured revision; it is not a
fresh measurement of the current code.

The [start checklist](#91-before-shared-reader-implementation) was completed in
[SDK-596](https://linear.app/unnamed-system/issue/SDK-596), merged in
[PR #44](https://github.com/pdx-foundry/native/pull/44). It covered:

1. Define exactly what Milestone 4 must prove, and assign its acceptance test.
2. Give the Atlas integration work an owner and ticket. Native answers must reach Atlas's
   snapshot and coverage calculation without losing conditions or crediting unresolved claims.
3. Assign the missing fixture-observation work and record its blocking dependencies.
4. Fix duplicate modifier registrations, which can currently hide uncertainty.
5. Record the current field and reader population on M45-release **before changing field discovery**.
6. Put the shortcut guard and jump-table repair ahead of the first shared-reader methods.

Every remaining action has a Linear ticket or an explicit retained/conditional disposition in
[section 8](#8-linear). Start with SDK-569 and the SDK-579 → SDK-563 dependency chain, then
the first shared-reader methods with SDK-597 alongside. Preparation completion does not mean
that those implementation tickets are complete.

The main delivery risk is that static answers could look complete while required fixture checks
remain impossible. Numeric storage, runtime weight evaluation and scope availability need
specific observation work. Those checks remain required; a static result cannot replace them.

**Milestone 4 can start before all repairs and cuts are finished.** Atlas integration runs
alongside the first methods. Other repairs attach to the changes they serve; optional cleanup
does not delay the milestone. *Amended 2026-09-24:* a Milestone 3.5 now comes first and holds the
method tooling and the cleanup ([section 8.1](#81-milestone-35-amendment-2026-09-24)).
Milestone 4 finishes only when the council agenda test, method fixture criteria, full-inventory
measurements and Atlas integration checks pass.

## Review basis

Status: agreed recommendation, revision 7, 2026-09-24. Accepted by Jackson: "Review is accepted." Revision 5 added the summary and expanded the action plan. Revision 6 records explicit agreement on G1 and G2, including the distinction between production validation and build-specific regression tests. Revision 7 records completed preparation and reconciles all remaining actions with Linear; it does not repeat the source review. Revision 8 (2026-09-24) records Jackson's Milestone 3.5 amendment in [section 8.1](#81-milestone-35-amendment-2026-09-24). Revision 9 (2026-09-25) returns SDK-577 to Milestone 4 in the same section. Revision 10 (2026-09-25) adds the churn and files-per-operation measurements from SDK-603 and records N12 and N13 as done. Native reviewed at `866e2ea` (main), Atlas at its working tree (pin `866e2ea`, pdxscript pin `7cf9f15`). Every lead finding has a `path:line` that I read myself; items marked "agent-reported" were not re-read. Not run during the original review: `cargo fmt`, `clippy`, `cargo test`, `git log --stat` in either repository (no shell). CI at `866e2ea` runs fmt, clippy, test, doc and the Python codec tests on macOS and Linux (`.github/workflows/ci.yml:15-22`); its status was not checked. Findings below describe those reviewed revisions; section 8 records their current disposition.

The findings of `docs/design/native-dx.md` are already ticketed (SDK-579 to SDK-595) and are not repeated; they appear only as dependencies.

## 1. State of the authorities

- **Milestone 3 exit gate met** (roadmap:132-138; Atlas `docs/coverage/language-snapshot.md:131-137`). All M3 tickets Done. Coverage 15,278 / 58,032 (26.33%) live, measured at Native `2166dbf`, not re-measured at the current pin (agent-reported).
- **Milestone 4 exit gate** (roadmap:127): "Council agenda completeness passes; full-registry counts and failure shapes recorded per method." Open: SDK-541 to SDK-550, SDK-569, DX tickets 579, 581, 588, 589, 593, 594, 595.
- **Linear milestone 4 text is stale**: "frozen method ... held-out cases ... held-out success rate" contradicts the development policy amendment of 2026-09-23 (`docs/development-policy.md:45-52`).
- **Milestone 2 order of work, still open:** N5 (`CategoryFieldReads`, `InitialCategoryLoad`: `src/fixture.rs:205-217`; recorded manual exception `docs/native/early-observations.md:128-138`); N6 partial (`src/lib.rs:51`; SDK-579 will widen `internals`, acceptable because the boundary test rejects it); R4 partial (`recorded().is_some()` at `src/session/questions.rs:65`; agent-reported `:37,51`); R6 partial (crack 7); N9 leftover (`.gitattributes:1,5`); Atlas A2 half done, A5 reversed, A6 partial. Done and verified: R1, R2, R3, R5, R7, N1, N2, N3, N4, N7 as decided (`milestone-2-repair-notes.md:15-22`), N8, A1, A3, A4.

## 2. Finding

**Milestone 3 is on track, and Milestone 4 should proceed.** The gate is met with a reproducible measurement. The consumer boundary holds. Typed gap subjects landed. Static completeness is derived. The string-reader fixture method transferred to two unfamiliar registries with no field constants and recorded one honest failure (`early-observations.md:90-126`). One evaluator serves six methods. pdxscript-rs is game-agnostic and two doc-only commits behind its default branch.

**What Milestone 4 needs before its gate can be judged:** a semantic, test-decidable definition of "council agenda completeness" (section 3), and an owned Atlas composition slice, because the gate is measured in Atlas's ledger and roadmap:145 leaves Milestone 4 assembly unticketed. Alongside, a small set of repairs keeps the method path honest. None is a prerequisite programme; each is scoped to the ticket it serves.

One qualification to the Milestone 3 wording: roadmap:134-136 says every loaded modifier is "in" the snapshot; the snapshot holds the loaded table's counts, and the 45,578 names stay in the recorded answer (`language-snapshot.md:81-83`). The measurement states this correctly; the roadmap sentence is too broad. Fix the sentence; do not add unconditional rules for vanilla names to satisfy it.

### Cracks and defects, verified

1. **A per-registry branch in an analysis method, producing data nothing reads.** `src/engine/analysis/fields/inventory.rs:74-93`: `if owner == "CCouncilAgenda"` returns five `ReaderContractGap`s; `fields.rs:118-119` stores them with `complete_registry: false` (constant; `records.rs:141-142`). Nothing under `src/session` reads either; the `reader-contract` gap is dropped at `questions.rs:417`. High confidence.
2. **SDK-569 as specified would not catch the existing case-specific shortcuts.** It scans `src/engine/analysis` for content-directory literals and operation code for build-version tests. Crack 1 is an owner-class comparison. `src/engine/operations/fixture.rs:128` (`registry() != "common/tradition_categories"`) and `:463` (`matches!(field, "tree_template" | "traditions")`) are registry and field comparisons in operation code. `src/protocol/session.rs:93,103,112` fix `164`, M45's registry count, as a wire limit (`src/game.rs:76` repeats it). High confidence.
3. **Answer-integrity defect: duplicate modifier registrations discard uncertainty.** `src/session/language.rs:149-151` skips a repeated name before inspecting its tags. A known registration followed by an unresolved one (`Tags::Unresolved`, `:154`) loses the gap; conflicting known tags are discarded. Completeness is derived from the surviving gaps (`:125`), so this can yield an unjustified `Complete`. The M45 impact is not verified. `src/session/defines.rs:48-56` already handles conflicting value types conservatively and is the pattern to reuse. High confidence on the source.
4. **Fixture observation supports only string storage; five M4 tickets need other observations.** `src/binding.rs:411` marks every non-`String` reader "has no storage decoder"; `src/binding/analysis.rs:408` binds outcomes only for `CReader::Read(CString&, bool)`. Diagnostics are carried separately from storage (`binding.rs:418-428`; hooks `fixture:malformed`, `fixture:unexpected` at `operations/fixture.rs:130-131`), and a runtime request is an explicit unavailable outcome (`early-observations.md:88`). The missing capability differs per ticket (repair R3). High confidence.
5. **The current field and reader population has no registry-wide result on the catalogued build.** The only `registry_fields` sweep is method v2 on M45-observe (`milestone-2-registry-sweep.md:3-9`; 231 of 873 fields unknown kind); reader kinds were measured on 3 registries (`reader-kinds.md:21-25`). Other methods do have release-build registry-wide results (key storage 156 of 164, `modifier-families.md:88`; `modifier_families` over its registry set). The missing measurement is narrow: `registry-fields/v3` and reader kinds on M45-release, which is exactly the population each M4 ticket selects with "every field bound to a X reader". High confidence.
6. **One layout fact has two production homes.** The CString flag byte is `string_tag_offset: 23` in `src/binding/groups.rs:79` and `short_string_length_offset: 0x17` in `src/binding/targets/recipes.rs:70`; both feed `StringLayout.flag_byte` (`binary/modifier_table.rs:40`, `binary/families.rs:143,886`, `binary/declarations.rs:180`). The fixture binding already reads the group value (`groups.rs:20`). The literal at `platform/macos/observation.rs:466` is inside `#[cfg(test)] test_observer` (`:445`) and does not count. The `& 128` rule appears five times in `worker.py` (agent-reported). High confidence on the two Rust sites.
7. **Strings and `Debug` output cross layers as identity.** Native `binding.rs:407` sends `format!("{:?}", field.reader.kind)` to the worker; `questions.rs:408-421` re-classifies analysis gaps from string codes (`FieldGap.kind: String`, `records.rs:116`). Atlas `snapshot/registry.rs:129` writes `{error:?}` into published gap text while the snapshot name is a content digest (agent-reported siblings). Milestone 2 cut A5 was reversed: `atlas/src/snapshot/language.rs:50-54` hard-codes five ticket numbers as gap owners; the v2 schema requires `^SDK-[0-9]+$` (agent-reported). High confidence on the lines read.
8. **Recorded answers and credit.** `atlas/src/coverage/projection.rs:92` credits only non-`Recorded` bases; recorded coverage is 0 / 58,032 (`language-snapshot.md:29`). This is correct policy: `src/recorded.rs:5` permits handwritten files, and `:44-49` only checks build identity, so a matching build cannot establish a real run. Atlas already assembles directly from Native while recording (`atlas/src/main.rs:170,188`), so SDK-548 and SDK-553 can keep their recordings for reproduction and take credited numbers from the live run. No guarantee change; a wording clarification on both tickets (section 8).
9. **Atlas composition costs and risks for Milestone 4.** Each new Native question needs about seven hand edits, including a match arm per config file, path and property (`atlas/src/coverage/language_subject.rs:92-178`). Two concrete integration risks: (a) rule identity is `subject#property` and must be unique (`snapshot.rs:456-457`), conditional rules use the same identity (`snapshot.rs:380-385`), and gaps carry no conditions (`snapshot.rs:216-231`), so two established branches plus one unresolved branch of one field cannot be represented today; (b) command descendant claims map to one aggregate `arguments` record (`language_subject.rs:209`) that `projection.rs:69-103` distributes to every joined question, so a partial grammar could credit an unresolved sibling argument. `projection.rs:166-168` also drops a registry whose directory has several CWT types (count not established). Medium-high confidence.
10. **Leftovers.** `src/engine/analysis.rs:11` puts `#[allow(dead_code)]` on `discovery`; `discover()` and `scheduler()` are called only from `tests_discovery.rs`, while `candidates` and the record types in the same module are live (`fields.rs:77`, `binding/analysis.rs:515,523`). `binding/analysis.rs:522` still reads the scheduler window on every catalogue build, and SDK-579's written design keeps that window as the code window of `discovery::read` (`native-dx.md:53`), so the split alone does not remove the dependency. Atlas `src/provenance/{comments,engine,model}.rs` are not declared in `lib.rs:4-8` and nothing references them; `rule-snapshot-v1.schema.json` and the offline-contract example JSON are unreferenced (agent-reported). pdxscript `VERIFICATION.md:14` at the pin still holds the absolute local path; upstream fixed it.

### Focus (Native line counts, agent-reported, approximate)

`src/` 37,355 lines (974 Python, about 12,000 in-source tests): `engine/analysis` 17,263 (evaluate 3,006; families 3,505; callbacks 3,469), `binding` 8,310, `engine/operations` 3,557, `session` 3,035. `tests/` 4,367 Rust plus 6,194 JSON; `docs/` 3,898 Markdown plus 15,866 JSON. Functions over 100 lines: `evaluate.rs:791` `step` about 592 (verified); agent-reported: `registry_items.rs:65` 226, `callbacks.rs:254` 193, `dispatch.rs:235` 150, and about ten more between 104 and 157. The evaluator and family modules serve current operations and carry substantial inline tests; no new unreachable feature or supported-path duplication justifies a broad cut. Fixture observation remains the largest feature with the narrowest use (impression). Churn and files per operation were not obtained in the original review; SDK-603 measured them after acceptance ([below](#churn-and-files-per-operation)).

### Churn and files per operation

Measured on 2026-09-25 for SDK-603. This is historical review evidence, not a gate.

**Ranges.** Native `89820bd..866e2ea`, from the Milestone 2 review revision to this review's
revision (25 commits). Atlas `01902e4..29546ff`, from the last Atlas commit before `89820bd` to
the first Atlas commit that pins Native `866e2ea` (10 commits). The data comes from
`git log --numstat --no-renames --format=@%h <range>`; binary files count zero lines. Areas:
`src/`, with in-source tests (`tests.rs`, `tests_*.rs` or a `tests/` directory) counted apart;
`tests/`; docs (`docs/` and other Markdown); other.

**Repository churn.** Distinct files changed, then lines added and removed:

| Area | Native files | Native lines | Atlas files | Atlas lines |
| --- | ---: | ---: | ---: | ---: |
| `src/` | 73 | +22,064 −3,648 | 19 | +6,319 −2,124 |
| In-source tests | 8 | +2,712 −36 | 1 | +253 −0 |
| `tests/` | 24 | +8,226 −1,394 | 359 | +96,406 −8,597 |
| Docs | 29 | +15,825 −267 | 13 | +1,384 −459 |
| Other | 12 | +500 −695 | 29 | +792 −4,407 |
| Total | 146 | +49,327 −6,040 | 421 | +105,154 −15,587 |

JSON is 20,748 of Native's 55,367 changed lines. The Milestone 2 repair commit `1da4abf` adds
15,905 lines, 13,628 of them the old sweep JSON; without it, Native has 24 commits, 115 files
and +33,422 −2,344. In Atlas, JSON is 104,953 of 120,741 changed lines, and 102,167 of those
are under `tests/`, mostly recorded Native answers in `tests/fixtures/native`. Line churn
therefore measures recorded data more than operation cost.

**Files per operation.** An operation's introducing commit is the first Native commit whose
`src/` diff adds `pub fn <name>` or `pub async fn <name>` (`git log --reverse -G`). Pull
requests are squash-merged, so one commit is one pull request. That commit also carries shared
refactors and docs, so its count is an upper bound on the operation's cost; later repairs to
the operation are not attributed. Shared files are the 11 `src/` files that at least four of
the seven commits touched.

| Operations | Commit | `src/` files (shared) | `tests/` | Docs | Other | Total |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `declarations` | `a2c2ada` | 15 (7) | 8 | 7 | 1 | 31 |
| `modifiers`, `modifier_categories`, `scopes`, `scope_links` | `ff37ef9` | 15 (10) | 9 | 6 | 1 | 31 |
| `localization_declarations` | `24f24ba` | 16 (9) | 4 | 7 | 1 | 28 |
| `on_actions`, `game_rules` | `20425ea` | 19 (11) | 5 | 7 | 1 | 32 |
| `modifier_families` | `aa73780` | 20 (11) | 5 | 8 | 1 | 34 |
| `defines` | `c7edcf2` | 11 (8) | 4 | 4 | 0 | 19 |
| `Game::loaded_modifiers` | `72e3de8` | 30 (7) | 2 | 7 | 2 | 41 |

`src/` counts include in-source tests. All seven commits touched `answer.rs`, `lib.rs`,
`binding/analysis.rs` and `session/questions.rs`. The live operation touched the most files
because it crosses the protocol, worker and supervisor. The other four operations came before
the range: `registries`, `registry_fields` and `registry_items` in `2331278` (the
simplification rewrite, which cannot be attributed per operation) and `observe_fixture` in
`5c98745`.

In Atlas, one commit, `2721a2f`, first called all 11 operations above (11 `src/` files, 183
`tests/` files, 202 files in total), so its cost cannot be divided per operation. Atlas first
called `registries`, `registry_fields` and `observe_fixture` in `1d2ed48` (40 files). Atlas at
`29546ff` does not call `registry_items`.

## 3. Acceptance target

The Milestone 4 gate has two parts. Both must hold; the first alone establishes nothing about ticket completion.

**Static sub-gate (one ignored parity test through the public API).**

> For `common/council_agendas`, `registry_fields` returns `Complete` with an established reader kind for all ten fields, and the Milestone 4 methods establish, without a typed gap: for `agenda_cost`, the numeric storage kind and scale and whether a script value is accepted (SDK-544); for `agenda_cooldown` and `agenda_finish_modifier_duration`, the normalized condition under which each is read (SDK-541); for `potential` and `allow`, that the block accepts triggers, and its entry scope types (SDK-542, SDK-549); for `effect` and `init_effect`, that the block accepts effects, and its entry scope types; for `finish_modifier`, the target registry named by content directory (SDK-543); for `modifier`, the member family the block accepts (SDK-542); for `ai_weight`, the accepted keys, the reader kind of each, and the nesting of `modifier` entries (SDK-545). The same methods run unchanged over every discovered registry with counts recorded, and the locality gate passes.

**Fixture part.** Every fixture criterion written in SDK-541 to SDK-550 stays as written. A criterion whose observation capability Native lacks is unmet; it is not excluded, not converted to a gap, and not satisfied by the static sub-gate. Milestone 4 completes when the static sub-gate passes and each method ticket's criteria, including its fixture criteria, are met or Jackson has amended the ticket.

Accepted exclusions, which the tickets themselves already place outside Milestone 4: runtime evaluation and modifier application contexts beyond SDK-547's first bounded form, and script expansion beyond SDK-550's stated scope. No other exclusion.

## 4. Rule for the cuts

> Keep code that serves current behavior, verifies a current guarantee, or preserves knowledge that is not yet transferred. Remove duplicate implementations, obsolete paths, and speculative machinery with no current responsibility.

To preserve knowledge means small representative inputs, expected outcomes, failed cases and a short note. Then the implementation is deleted; Git history keeps it.

## 5. Repairs

**Measurement first.** M1. Run `registry-fields/v3` over all 164 registries on M45-release with the SDK-588 report shape before any M4 method or SDK-563 changes a field list. Record in `milestone-4-field-baseline.md`: complete, partial, failed; fields per reader kind; the distinct callee signatures behind `Unknown`. Limit: one run, no fix inside it.

- **R1. Duplicate modifier registrations keep their uncertainty** (`language.rs:149`). Combine registrations by name; an unresolved or conflicting tag set on any registration of the name yields a gap and `DeclaredTags::Unresolved`, as `defines.rs:48-56` does for value types. Tests: equal tags, conflicting tags, known-then-unresolved and unresolved-then-known. Check parity output; record whether M45 changes. Limit: a bounded correctness fix, no new framework.
- **R2. Widen SDK-569 to case-specific shortcuts.** Scan `src/engine/analysis`, `src/engine/operations`, `src/session` and `src/protocol` for a comparison or match that selects behavior or output for one engine class, registry, field or registry count (`owner == "C..."`, `registry != "common/..."`, `matches!(field, "...")`, a literal `164`), plus build-version tests. Uniform tables keyed on the executable's own overload or symbol shapes (`readers.rs:35-60`, `binary/defines.rs:61-98`, `binary/callbacks.rs:82-118`) are not shortcuts and are not flagged. The exception list holds one entry, the category read-entry exception (`early-observations.md:128`), with its removal route. Negative controls: crack 1, `operations/fixture.rs:128,463` and `protocol/session.rs:93` go red before they are cut, moved to the binding, or listed.
- **R3. Assign the observation needed by each fixture criterion.** Before shared-reader implementation, update the five tickets using the table below: name the Native capability, its implementation owner, and the Atlas fixture check. Create blocking relations where a separate ticket owns the capability. This preparation is complete when the work is assigned, not when every observer is built. A method ticket stays open until its fixture criterion passes.
- **R4. One home for the CString layout.** `recipes.rs:70` reads `M45_TEMPLATE_LAYOUT.string_tag_offset` (`groups.rs:79`) as the fixture binding already does (`groups.rs:20`); the `& 128` rule in `worker.py` becomes one helper. Check: byte-identical parity output. SDK-580 is the larger anchor move; this is the one-fact fix.
- **R5. Make the gate decidable.** Add the section 3 static sub-gate to `tests/static_questions.rs`, expected to fail until the M4 readers land. Record section 3 in roadmap:127 and fix the Linear milestone 4 description.
- **R6. Own the Atlas Milestone 4 integration.** Create an Atlas-labelled counterpart to SDK-570, assign it before shared-reader implementation, and schedule it alongside SDK-541 and SDK-542. Its result is a snapshot and coverage calculation that preserve the meaning of Native's new answers. The four acceptance cases below define completion. SDK-577 belongs alongside this slice; do not turn all Atlas cleanup into a prerequisite.
- **R7. Identity hygiene.** Serialize `ReaderKind` on the worker wire with serde (`binding.rs:407`); type `FieldGap.kind` as an enum so `questions.rs:408-421` matches variants. Atlas gap text uses `Display` or a fixed reason.

### R3: fixture capabilities and dependencies

Native owns the observation mechanism. Atlas owns the script fixtures, rule conclusions and
coverage. Each ticket must name who implements and checks each side.

| Method ticket | Required observation | Work and completion condition |
| --- | --- | --- |
| SDK-542: nested blocks | Parser acceptance and rejection of nesting | At ticket start, confirm that a rejected nesting produces a source-located diagnostic within a verified loader and owner boundary. If it does not, include the missing diagnostic hook in SDK-542. Close only after accepted and rejected fixtures pass. |
| SDK-544: numeric readers | Stored values for each required reader kind | Add numeric storage decoding to SDK-544. Start with the bounded experiment below, then complete the remaining required reader kinds. A static storage description alone does not satisfy the fixture criterion. |
| SDK-545: weights | Runtime results for additive and multiplicative cases | Create a blocking runtime-observation ticket, using SDK-547's first bounded form if it supplies this result, otherwise a new ticket. `FixtureRuntime` currently reports unavailable. SDK-545 cannot complete until the observer exists and both fixture cases pass. |
| SDK-549: scope context | Scope availability observed during parsing | Create a blocking observation ticket. Start from the SDK-488 prototype's 14 live parser-scope checks (`engine-commands.md:14`) and verify what transfers to the supported build. Close SDK-549 only after its required fixture observations pass. |
| SDK-550: script parameters | A call with parameters and an inline script, including parser diagnostics | Keep the SDK-542 dependency. Confirm the diagnostic capability at ticket start; add hook work to SDK-550 if needed. Both fixture forms must meet the existing ticket criteria. |

### R6: Atlas acceptance cases

These are checks of the full path from Native answers to Atlas coverage, not just successful
serialization of new fields:

1. **Conditional fields:** one field has two established branches and one unresolved branch.
   Assembly, verification, comparison and coverage retain all three. The unresolved branch
   stays a gap, and verification still rejects duplicate identities. Structured alternatives
   inside one answer are acceptable; this finding does not require a schema redesign.
2. **Partial command grammars:** one argument is established and its sibling is unresolved.
   Coverage credits only claims for the established argument. Replace the aggregate join that
   currently distributes an `arguments` record to every descendant claim
   (`language_subject.rs:209`, `projection.rs:69-103`).
3. **New claim types:** field shape, condition, block family, numeric, weight, naming and
   scope-context answers reach the snapshot and comparison through typed subjects.
4. **Shared content directories:** when one directory maps to several CWT types, join each
   applicable type or retain an explicit gap. Record how many such mappings remain unresolved;
   do not silently omit the directory.

### Bounded experiment: numeric fixture decoder (first attempt inside SDK-544)

- Limit: one working day, implementation and verification together.
- Method: derive the decoder from the joined callee and owner-relative destination of `CReader::Read(int&)`, `CFixedPoint` and the fixed-point template, with no field or registry constant; develop on one registry, then run on a numeric field in a registry not used for development (the development policy's rule; there is no freeze commit).
- Success: through `Game::observe_fixture`, the stored value for one boundary, one fractional and one malformed input, with unsupported fields reported as unavailable. SDK-544's fixture criterion is then met for that reader kind.
- Failure outcome: the string-only decoder stays, the failed case and trace are recorded in `early-observations.md`, and SDK-544's fixture criterion is unmet. SDK-544 does not close on the static result. The next attempt is scoped from the recorded obstacle; only Jackson can amend the ticket.

## 6. Cuts

### Native

| # | Cut | Lines | Reason and conditions |
| --- | --- | ---: | --- |
| N10 | `blocking_readers`, `ReaderContractGap`, `complete_registry` (`fields/inventory.rs:74-93`, `fields/records.rs:122-129,141-144`, `fields.rs:118-119`, `tests_fields.rs:87,166`, `questions.rs:417`) | ~40 | Per-registry branch with no reader. Preserve first: the five contract labels are already in `registry-fields.md:142-146`; add a line that they left the code. After R2's negative control has gone red. |
| N11 | Scheduler-window removal, as its own conditional change: (a) replace the code window that `binary::discovery::read` takes from `SchedulerLayout` (`binding/analysis.rs:522`, `recipes.rs:105-111`) with the inventory's code range, keeping the vtable witnesses and every supported operation byte-identical in parity; (b) then remove `discover()` (`discovery.rs:25-`), `scheduler()` and `SchedulerLayout` in `discovery/scheduler.rs`, their `tests_discovery.rs` cases, and the `#[allow(dead_code)]` at `analysis.rs:11`. | part of ~480 (agent-reported) | Not a consequence of SDK-579, whose design keeps the layout's code window (`native-dx.md:53`); it can be done after SDK-579 or independently. Condition: (a) passes parity before (b) starts. Keep `candidates`, `CandidateRecord`, `Symbol`, `records.rs` and their tests: they are live (`fields.rs:77`, `binding/analysis.rs:515,523`); move `candidates` out of `scheduler.rs` first. Preserve the scheduler layout (198 rows, offset 96, stride 48) and its recovered/gap cases as a note in `registry-fields.md:159-191`. |
| N12 | `.gitattributes:1,5` replay wording and the missing `tests/fixtures/**` rule | small | N9 leftover. |
| N13 | `docs/native/milestone-2-registry-sweep.json` | 13,628 (agent-reported) | After M1 records the v3 baseline; keep the Markdown summary. |

Keep: the category read-entry exception as the one recorded manual exception; `readers.rs` and `binary/defines.rs` tables (uniform, symbol-keyed; each method keeps its own admission rules; share type parsing only where a demonstrated common shape appears, under SDK-594); `binary/callbacks.rs` `FORWARDERS` (binding authority; SDK-580); `internals` (SDK-579); `tools/profiling`.

### Atlas

| # | Cut | Lines | Reason and conditions |
| --- | --- | ---: | --- |
| A7 | `src/provenance/*.rs` orphan files | 459 (agent-reported) | Not compiled, not referenced. Fix `docs/coverage/documentation.md:123` (agent-reported reference to a removed test). |
| A8 | `rule-snapshot-v1.schema.json`, offline-contract example JSON | ~486 (agent-reported) | Unreferenced. Check no test loads them first. |
| A9 | Ticket-number owners in gap output (`snapshot/language.rs:50-54`, schema pattern, `tests/language.rs`) | small | Policy decision G1. |
| A10 | `{:?}` in gap reasons (`snapshot/registry.rs:129` and agent-reported siblings) | small | Snapshot digest changes on a Native rename. |

### pdxscript-rs

No cuts. Optional: bump the pin to `ccb681a` (doc-only; removes the local path).

## 7. Agreed decisions

- **G1. Gap owners — agreed, 2026-09-24.** A published gap carries a reason and an owner category; the ticket mapping lives in docs. Remove the ticket-number owners reintroduced by SDK-570 through A9. This loses a direct ticket reference in the snapshot, but keeps changes to the work plan from changing the published engine knowledge.
- **G2. Registry count — agreed, 2026-09-24.** Production and tests have different responsibilities. Production must adapt to supported builds; tests must detect unexpected changes to discovery on a known build.

  **Production:** replace the fixed `164` selection limit in `protocol/session.rs:93-112` with validation against the bound build's discovered registries. The supervisor derives or verifies this information from the installation it opens; a caller-supplied count is not the authority. Keep duplicate and unknown-name checks and the separate transport message-size limit. The removed guarantee is a constant registry-count limit across builds, not bounded message transport.

  **Tests:** the parity test for the exact supported M45-release build must explicitly assert **164 registries**, alongside the existing expected registry identities. This count is a regression expectation, not a production limit. An unexpected count must fail; do not automatically replace the expected count with the latest discovery result. A synthetic target with more than 164 valid registries must also pass session admission, proving that production no longer imposes M45's count on other builds. Keep rejection tests for duplicates and unknown names.

  Jackson's clarification: "Production shouldn’t have a fixed limit because we need to adapt to supported builds. But we should also assert in tests that the number currently is 164. That way we can detect regressions in registry discovery."

  **Effect on R2:** build-specific counts and names in test expectations are valid. The shortcut guard must reject case-specific production behavior without banning the constants that make regression tests useful.

- Withdrawn in revision 3: crediting recorded answers. The coverage policy stays as it is (crack 8).

## 8. Linear

**Reconciled, 2026-09-24.** Linear holds delivery status, ownership, acceptance criteria and
dependencies. This report retains the evidence, accepted decisions and finding-to-ticket index.
The preparation pass is Done; the remaining actions below are tracked, not implemented.
New tickets SDK-601 to SDK-605 and expanded delivery tickets are assigned to Jackson, with
repository labels. Optional and conditional work is scheduled alongside M4, without becoming
a new start or exit gate.

| Finding or action | Linear home and disposition |
| --- | --- |
| M1, R1; gate wording, roadmap/specification corrections; preparation checklist | [SDK-596](https://linear.app/unnamed-system/issue/SDK-596), Done in PR #44. Includes the explicit M45-release 164-registry assertion. |
| R2, N10, G2 production validation and synthetic >164 test | [SDK-569](https://linear.app/unnamed-system/issue/SDK-569), including the failing shortcut controls before removal. |
| R3 parser diagnostics and expansion observations | [SDK-542](https://linear.app/unnamed-system/issue/SDK-542) and [SDK-550](https://linear.app/unnamed-system/issue/SDK-550); the latter remains blocked by the former. |
| R3 bounded numeric storage experiment and remaining required fixture cases | [SDK-544](https://linear.app/unnamed-system/issue/SDK-544). |
| R3 runtime weight and scope-availability observations | [SDK-598](https://linear.app/unnamed-system/issue/SDK-598) blocks SDK-545; [SDK-599](https://linear.app/unnamed-system/issue/SDK-599) blocks SDK-549. |
| R4 CString layout and worker flag helper | [SDK-601](https://linear.app/unnamed-system/issue/SDK-601), alongside SDK-579; distinct from SDK-580's larger anchor move. |
| R5 council agenda public-API test and M4 acceptance checks | [SDK-600](https://linear.app/unnamed-system/issue/SDK-600), with SDK-541 to SDK-550 retaining their own fixture criteria. |
| R6, A9/G1, Atlas R7/A10; carried M2 A5 | [SDK-597](https://linear.app/unnamed-system/issue/SDK-597): four integration cases, category owners and stable published gap reasons. |
| The 755 gaps without owners | SDK-597 explicitly requires a follow-up per failure shape or an accepted-gap note, with ticket mappings in docs. This triage is still open; solving every gap is not an integration prerequisite. |
| Native R7 and carried M2 R6 identity cleanup | [SDK-574](https://linear.app/unnamed-system/issue/SDK-574) owns serde ReaderKind on the worker wire; [SDK-581](https://linear.app/unnamed-system/issue/SDK-581) owns the FieldGap enum and normalization. |
| Jump-table repair | [SDK-563](https://linear.app/unnamed-system/issue/SDK-563), moved to M4; blocked by SDK-579 and completed preparation, and blocking SDK-541. |
| Atlas display-name identity repair | [SDK-577](https://linear.app/unnamed-system/issue/SDK-577), M4, in the same change as SDK-597's first live re-record ([section 8.1](#81-milestone-35-amendment-2026-09-24)). |
| Live hang investigation and pause coordination | [SDK-571](https://linear.app/unnamed-system/issue/SDK-571) and SDK-574 now have a person and M4 assignment. Reproduce the hang first; block only affected live checks. |
| N11 conditional scheduler removal | [SDK-602](https://linear.app/unnamed-system/issue/SDK-602): replace the live code window and pass parity before deleting; preserve candidates and recovered/failed cases. |
| N12/N13 and carried M2 N9; missing churn/files-per-operation measurements | [SDK-603](https://linear.app/unnamed-system/issue/SDK-603): preserve distinct old findings before removing duplicate artifacts; retain the historical measurement scope. |
| A7/A8 and carried M2 A2/A6; optional pdxscript pin bump | [SDK-604](https://linear.app/unnamed-system/issue/SDK-604): check references before removal; record a disposition for the optional doc-only bump. |
| Carried M2 R4 backend dispatch | [SDK-605](https://linear.app/unnamed-system/issue/SDK-605): finish dispatch at the existing Live/Recorded boundary; no new backend framework. |
| Developer inspection, diagnostics, reporting and authoring guidance | [SDK-579](https://linear.app/unnamed-system/issue/SDK-579), [SDK-581](https://linear.app/unnamed-system/issue/SDK-581) and [SDK-589](https://linear.app/unnamed-system/issue/SDK-589). SDK-588 was merged into SDK-581 on 2026-09-24; the stop-diagnostic grouping and cross-run diffs that remained after the baseline live there. |
| Long evaluator step function | [SDK-593](https://linear.app/unnamed-system/issue/SDK-593) now includes the bounded instruction-family split with unchanged behavior. |
| Conditional shared type parser | [SDK-594](https://linear.app/unnamed-system/issue/SDK-594) requires checking for a demonstrated common shape; retaining separate parsers is a valid documented outcome. |
| Shared Rust/Python hook names | SDK-574 includes generation from the protocol authority with the next live change. |
| Prepared-operation design and modifier contexts | [SDK-547](https://linear.app/unnamed-system/issue/SDK-547) begins with its own bounded design. SDK-598 separately owns weight observations. |
| Live measurement credit; full inventories and failure shapes | [SDK-548](https://linear.app/unnamed-system/issue/SDK-548), [SDK-553](https://linear.app/unnamed-system/issue/SDK-553), SDK-597 and SDK-600 retain live-only credit and method acceptance requirements. Recordings remain reproduction inputs. |

Retained decisions are constraints, not missing implementation tickets: the category read-entry
exception (M2 N5) stays under SDK-569's documented removal route; hidden `internals` (M2 N6)
and its consumer boundary stay under SDK-579. Keep the live candidate code, symbol-keyed reader
tables, callback forwarders, evaluator/family methods and profiling tools. [SDK-580](https://linear.app/unnamed-system/issue/SDK-580)
owns the later build-anchor move. No broad rewrite or deletion is implied by the line counts.

### 8.1 Milestone 3.5 amendment, 2026-09-24

After reconciliation, Milestone 4 held 32 tickets: its ten method tickets, four gate tickets and
about twenty repair, tooling and cleanup tickets from this review and from
[native-dx.md](native-dx.md). Jackson chose a separate milestone between 3 and 4. This changes
the rule above that repairs do not gate Milestone 4. The contents of the new milestone are
limited to work that makes each method cheaper or safer to write, plus independent cleanup.

| Change | Tickets |
| --- | --- |
| Moved to Milestone 3.5 (Foundations) | SDK-563 and SDK-569 (both block SDK-541); SDK-581 (DX 1, 2; SDK-588 merged into it on 2026-09-24); SDK-589 (DX 8); SDK-574 (now blocks SDK-598 and SDK-599); SDK-571; SDK-593 (DX 7, step 1); SDK-601 (R4); SDK-602 (N11); SDK-603 (N12, N13); SDK-604 (A7, A8); SDK-605 |
| New in Milestone 3.5 | SDK-606, the test assembler helper, split from SDK-594 (DX 7) |
| Out of Milestones 3.5 and 4 | SDK-594 (typed operands) and SDK-595. Neither blocks a method ticket |
| Stay in Milestone 4 | SDK-541 to SDK-550, SDK-597, SDK-598, SDK-599, SDK-600; SDK-577 (returned 2026-09-25, see below) |

SDK-577 was first moved to Milestone 3.5 with the cleanup. On 2026-09-25 it returned to
Milestone 4. It adds `display_name` rules, so the snapshot changes, and the full-config gate
must be re-pinned from a live run. SDK-597 needs a live run for credited rates, so SDK-577 lands
in the same change as SDK-597's first live re-record, and before scope-context answers become
claims. This also replaces the earlier plan in the ticket to wait until Milestone 7.

The Milestone 4 exit gate in section 3 and the checks in section 9.5 do not change. SDK-569
now passes in Milestone 3.5 and must still pass when Milestone 4 closes.

The open prototype children of SDK-470 were closed on the same day. The inspector, stop
diagnostics and sweep report let one task explore and deliver a method, so a separate prototype
is not needed. Each prototype's unique cases were copied into its production ticket under
"Carried from SDK-NNN". Work that no open ticket owned became SDK-607 (shared modifier-block
grammar, from SDK-498), SDK-608 (event and pre_trigger contexts, from SDK-496), SDK-609 (live
localisation checks, from SDK-500) and SDK-610 (define defaults and bounds, from SDK-504). SDK-480,
SDK-486, SDK-506 and SDK-511 stay open. SDK-486 now also owns the archive of `.local/sdk-498/`.

## 9. Order of work

This is the recommended sequence for carrying out the review. **Starting Milestone 4 does not
mean finishing every repair below.** Preparation establishes the target, ownership and baseline;
the first implementation work removes the known obstacles to shared-reader methods. Checkboxes
remain open until the stated result exists.

**Preparation complete, 2026-09-24 ([SDK-596](https://linear.app/unnamed-system/issue/SDK-596), PR #44 merged as `61d299e`).**
The following delivery tickets are assigned to Jackson in Milestone 4:

| Responsibility | Ticket |
| --- | --- |
| Atlas composition and fixture conclusions (R6, G1) | [SDK-597](https://linear.app/unnamed-system/issue/SDK-597) |
| Runtime weight observations blocking SDK-545 | [SDK-598](https://linear.app/unnamed-system/issue/SDK-598) |
| Scope availability observations blocking SDK-549 | [SDK-599](https://linear.app/unnamed-system/issue/SDK-599) |
| Public-API council agenda parity gate (R5) | [SDK-600](https://linear.app/unnamed-system/issue/SDK-600) |

SDK-542, SDK-544 and SDK-550 now name their observation work and owner. SDK-563 and SDK-577
were placed in Milestone 4; on 2026-09-24 both moved to Milestone 3.5 with SDK-569, and on
2026-09-25 SDK-577 returned to Milestone 4 ([section 8.1](#81-milestone-35-amendment-2026-09-24)). SDK-569 and SDK-563 are blocked by
preparation; SDK-541 is blocked by both, and SDK-542 by preparation and SDK-569. Existing dependencies remain. SDK-548 and SDK-553 state
that credited rates come from the live run, while recordings support reproduction.

M1 used the sweep's completeness report shape with current public diagnostics. The internal
stop diagnostics, the grouping by them and the normalized cross-run diffs are in SDK-581, which
absorbed SDK-588 on 2026-09-24. SDK-598 is a separate
weight-observation ticket: SDK-547's naval-capacity state reads are not assumed to evaluate weights.

**Preparation verification, 2026-09-24:** R1's regression cases and existing M45 modifier parity
pass without expected-output changes. M45-release registry parity explicitly asserts 164. M1 is
recorded in [the baseline](../native/milestone-4-field-baseline.md): 28 complete,
136 partial, no failed queries; 878 fields, of which 232 have unknown kinds. The 11 unknown-kind
fields with an identity use four signatures; the other 221 have no single established identity.
No field discovery or binding code changed. Formatting, clippy, the default Rust suite, doc checks
and all seven Python codec tests passed. Full live-game and unrelated ignored parity suites were
not run; preparation changes no live operation.

### 9.1 Before shared-reader implementation

- [x] **Record the acceptance target (R5).** Put section 3 in the roadmap, specification and
  Linear milestone description. Assign the public-API parity test in `tests/static_questions.rs`.
  State that it will initially fail and must pass before M4 closes. Remove the stale freeze and
  held-out policy wording, and correct the loaded-modifier sentence. Done when the documents and
  tracker describe the same static and fixture requirements.
- [x] **Record the two decisions (G1, G2).** Agreed in section 7: category owners with ticket
  mappings in docs; production validation against the bound build's registries, with an explicit
  164-registry assertion in M45-release tests. G1 controls A9; G2 controls R2's protocol work.
  The decisions are recorded; implementation and updates to the specification and tickets remain open.
- [x] **Assign Atlas integration and fixture dependencies (R3, R6).** Create the R6 ticket with
  an Atlas label and owner. Update all five fixture tickets and assign the two missing
  observation dependencies. Link SDK-577 to the relevant Atlas work. Done when every required
  result has a delivery ticket, an owner and a completion check; the implementations can follow
  during M4.
- [x] **Fix the existing modifier-answer defect (R1, Native).** Combine duplicate registrations
  conservatively. Equal tags remain established; conflicting or unresolved tags retain a gap
  in either input order. Pass the four regression cases in section 5 and check parity. Record
  whether the supported M45 result changes. This repairs an existing answer before new results
  build on it.
- [x] **Save the baseline before field discovery changes (M1, Native).** Use SDK-588's report
  shape to run the current `registry-fields/v3` and reader-kind analysis on M45-release across
  all 164 discovered registries. Record the build and Native revision, complete/partial/failed
  totals, fields per reader kind, and distinct callee signatures behind `Unknown` in
  `docs/native/milestone-4-field-baseline.md`. Do not fix failures inside the measurement. Done
  when the report can distinguish later discovery improvements from the starting population.
- [x] **Set the first implementation dependencies.** Widen SDK-569's written scope to R2 and
  schedule SDK-563 in M4 ahead of SDK-541. Preserve this order: **M1 baseline → shortcut guard
  and field-discovery repair → shared-reader methods**, with R6 alongside the first methods.

### 9.2 First implementation work

Since 2026-09-24, rows 1 to 3 are Milestone 3.5 work and row 4 starts Milestone 4
([section 8.1](#81-milestone-35-amendment-2026-09-24)). The order does not change.

| Order | Responsible area and action | Result needed before moving on |
| --- | --- | --- |
| 1 | Native: implement R2 / SDK-569 and the agreed G2. Demonstrate the existing shortcuts as failing controls before removing or relocating them. | The guard catches production shortcuts and accepts uniform symbol-keyed methods and build-specific test expectations. N10 remains a known failure until the next step. M45-release parity explicitly asserts 164 registries; a synthetic build with more than 164 valid registries passes session admission. Duplicate and unknown selections are rejected. |
| 2 | Native: remove N10 after its negative control has failed. Preserve the contract labels and their removal note. Complete the other shortcut repairs or relocations identified by R2. | The council-agenda-only branch and its unused result fields are gone. The guard now passes with only the documented category exception. |
| 3 | Native: implement SDK-563 after M1. | Jump-table controls and parity pass; the missed megastructure fields are found, and changes to the registry population are recorded. SDK-541 can use the repaired field discovery. |
| 4 | Native and Atlas: start SDK-541 and SDK-542 with R6. Add R5's public-API council agenda test. | Each delivered answer reaches Atlas with its conditions and gaps intact. The full council agenda test remains an explicit unfinished check until the later methods land. |

### 9.3 Work that follows its dependent ticket

| Work | Schedule and completion check |
| --- | --- |
| R4: CString layout (SDK-601, Milestone 3.5) | With SDK-579, use one production layout fact and one worker helper; parity stays byte-identical. |
| R7 and A10: typed identities and stable gap text | The Native parts (SDK-574, SDK-581) are Milestone 3.5 work; A10 stays with SDK-597 in Milestone 4. With the next live change, check that the Rust/Python wire agrees on serialized reader kinds, analysis gaps use enum variants, and Atlas publishes stable reasons rather than `Debug` output. |
| SDK-544 numeric observation | Run the bounded decoder experiment inside the ticket. Preserve failures and continue from the obstacle; do not close the ticket while required fixture cases remain unmet. |
| SDK-545 and SDK-549 observations | Start their assigned dependencies when these tickets approach implementation. Keep each ticket blocked until its required observation works. SDK-547's first bounded form still needs its own design. |
| SDK-548 and SDK-550 | Start after SDK-542. SDK-548 uses the live run for credited rates; SDK-550 retains its fixture checks. |
| Developer tooling | SDK-579 is done. SDK-581, SDK-589, SDK-593 and SDK-606 are Milestone 3.5 work. SDK-593 is the natural point to split the evaluator's long `step` function. |

### 9.4 Cleanup

This cleanup first did not hold up Milestone 4. Since 2026-09-24, N11 (SDK-602), N12 and N13
(SDK-603), A7, A8 and the pdxscript-rs pin (SDK-604) are Milestone 3.5 work, so they now come
before Milestone 4 ([section 8.1](#81-milestone-35-amendment-2026-09-24)). A9 stays with SDK-597 in
Milestone 4.

- **N11:** done in SDK-602. No live method read the scheduler code window, so the discovery input
  no longer takes one; every static answer on M45-release stayed byte-identical. The scheduler
  method is removed, the candidate logic stays, and the table facts and cases are in
  [registry fields](../native/registry-fields.md#scheduler-table-on-m45-release).
- **N12:** done in SDK-603. `.gitattributes` no longer names replay artifacts or the missing
  `tests/fixtures/**` path.
- **A7 and A8:** remove stale wording and unused files after checking references and tests.
- **N13:** done in SDK-603. The old sweep differs from the M1 baseline only in five megastructure
  fields, already in [registry fields](../native/registry-fields.md#compiler-jump-tables), and in
  completeness labels. The [Markdown summary](../native/milestone-2-registry-sweep.md) records
  the comparison and how to retrieve the removed JSON from git.
- **A9:** G1 is agreed. Remove ticket-number owners and update schema and tests together.
- **pdxscript-rs:** the documentation-only pin bump is optional.

### 9.5 Before Milestone 4 is declared complete

- [ ] Section 3's public-API council agenda test passes, including all ten fields and their
  required semantics; the shortcut guard also passes.
- [ ] Every SDK-541 to SDK-550 acceptance criterion passes, including fixtures, or Jackson has
  explicitly amended the affected ticket. Missing runtime or scope observations are not passes.
- [ ] Each method has run over its full applicable inventory, unchanged during that run, with
  complete, partial and failed counts and distinct failure shapes recorded. Commands use the
  whole command inventory, as required by the development policy.
- [ ] R6's four Atlas cases pass through assembly, verification, comparison and coverage.
  Unresolved branches and sibling arguments receive no unearned coverage credit.
- [ ] The Atlas result and coverage measurement identify the build and source revisions.
  Recorded answers remain reproduction inputs; credited rates come from the live run.

## 10. Deferred

- SDK-547 waits for its own design; SDK-598 separately supplies the runtime weight observations blocking SDK-545.
- SDK-550 waits for SDK-542.
- Sharing type parsing between `readers.rs` and `binary/defines.rs`: only when SDK-594 shows a common shape; not a prerequisite.
- Generating hook names from the protocol for Rust and Python (agent-reported duplication): with the next live operation.

## What is solid

Milestone 3 gate with pinned identities and a reproducible command; typed gap subjects; derived static completeness; one evaluator behind six methods; the string-reader transfer with a recorded failed case; the retired reference method with preserved cases; target composition, the OS lock and the fake-worker supervisor test; a coverage policy that credits only live, qualified evidence; CI on macOS and Linux; a game-agnostic pdxscript with CI green at the pin.
