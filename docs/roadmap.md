# Roadmap: from two registries to full config coverage

Status: proposed plan, 2026-09-19; amended 2026-09-20 for the
[simplification decision](design/simplification.md). The [Native specification](specs/native.md) and
[technical design](design/architecture.md) remain the authority for behavior and layout. The
[Atlas map](https://linear.app/unnamed-system/issue/SDK-470/specify-pdx-atlas-and-its-engine-derived-rule-database)
remains the authority for decisions and open extraction questions. This document only orders the work.

## Goal

Atlas generates the data that `cwtools-stellaris-config` maintains by hand, and publishes it as
platform-independent JSON snapshots. Native supplies every engine observation that Atlas needs.

## Starting point

Native exposes static `registries()` and `registry_fields(name)`, plus live item names for
selected discovered registries on one exact Mac ARM64 executable. The default live session
observes `common/traditions` and `common/tradition_categories`.
Static analysis lives in `engine/analysis`; the live stream reducer lives in `engine/operations`.
The API returns normalized answers with typed gaps and source stamps. The Atlas prototype caller
uses the same questions for live and recorded answers; see the [migration](design/atlas-caller-migration.md).
The remaining reader, reference, numeric-grammar and command-inventory methods exist as retained
Python prototypes.

## The target, measured

The config fork has 49,196 lines in 172 `.cwt` files.

| Share | Content | Route |
| --- | --- | --- |
| ~37% | Script language: 1,060 effects, 1,089 triggers, 747 modifiers, scopes, 89 links, localisation commands | Engine declarations, then argument grammars |
| ~35% | 253 type schemas under `common/` | Registry discovery, field readers, references |
| ~13% | 2,042 defines, 385 on_actions, game rules | Inventories |
| ~8% | Interface, graphics, sound, map, descriptors | Separate loaders, not yet investigated |

### Documentation provenance measured in full

SDK-525 measured every documentation claim against the installed base-game `common/**/*.txt`
files and the config fork's `script-docs/v4.4.1` effects/triggers logs. Atlas owns the comment-to-key
parser and the source-tagged ledger. Native's `declarations` operation (SDK-535) is separate: it
reads the effect and trigger documentation strings from the executable.

| Config area | Doc entries | Exact copies | Rewritten candidates | Authored remainder |
| --- | ---: | ---: | ---: | ---: |
| Effects | 1,520 | 967 | 79 | 474 |
| Triggers | 1,152 | 939 | 90 | 123 |
| Defines | 1,329 | 1,286 | 7 | 36 |
| On_actions | 342 | 269 | 38 | 35 |
| Game rules | 195 | 152 | 13 | 30 |
| Type schemas | 1,054 | 401 | 68 | 585 |
| Other | 192 | 0 | 0 | 192 |
| **Total** | **5,784** | **4,014** | **295** | **1,475** |

The type-schema row measures **all 1,703 documentation lines**, replacing the three-sample check.
The full ledger contains 7,282 documentation lines. Entry counts include nested prose and repeated
declarations, so they are not directly comparable with the earlier documented-command counts.
Exact means equal after removing comment markers and folding whitespace. Rewritten candidates use
an explicit similarity threshold and remain provisional.

The 1,475 entries with no match are listed as authored and are not evidence. This is a bounded
source search, not proof of historical authorship: version drift, source-parser limits, and content
outside the examined corpus remain possible. The report retains 36 source parse diagnostics and
the eight existing config diagnostics. Matching text grants no verified rule coverage.

See Atlas's [complete measurement and reproduction instructions](https://github.com/pdx-foundry/atlas/blob/67003ade425a3a071cc90db11c352206e49854d6/docs/coverage/documentation.md)
and [pinned acceptance fixture](https://github.com/pdx-foundry/atlas/blob/67003ade425a3a071cc90db11c352206e49854d6/tests/fixtures/documentation-baseline.json).
The source manifest covers 2,062 files, with SHA-256
`3d744becf7976e9ddd52e57f2d92cbaaf0b56f60b57af61852e45052519d3007`.

Policy is not engine knowledge: severity (86), soft cardinality, subtypes (164) and alias factoring
are cwtools modelling decisions. Subtypes have an engine-true counterpart in conditional field
constraints (SDK-490).

### Baseline

The ledger exists (SDK-523, Atlas commit `be9cb3b`). With the retained registry capture as input,
coverage is **0 of 56,551** Atlas-owned claims (53,465 engine-fact, 3,086 content-derived). The
capture holds item names only, and item names answer no rule question. A further 2,637 claims are
consumer policy and 192 are authored text; these are outside the headline figure. Atlas scores
coverage by evidence, not by agreement with the config.

## Ordering principle

Coverage grows with each **shared method**, not with each file. The known risk is method transfer:
the reference matcher failed on both unfamiliar resolver shapes, and five shared-reader contracts
block the council agenda completeness result. A method has no branch on a registry, a command or a
build; an unfamiliar shape is a typed gap, and a repair lands in the shared module. Each method
ticket ends with one run over every discovered registry and records its counts and failure shapes.
The registry sweep runs the method set unchanged at a recorded commit and measures the automatic
rate; its failures return as new reader shapes, not as handwritten answers. The
[development policy](development-policy.md#keep-engine-knowledge-in-its-home) states the rule
(amended 2026-09-23; it replaces the per-ticket freeze and held-out tests).

## Decisions taken with this roadmap

- **The `.cwt` emitter is a test tool.** It compares an Atlas snapshot with the config fork. The
  product remains the JSON snapshot (SDK-477).
- **Windows is deferred.** Atlas output is platform-independent. Windows support returns with the
  real-game testing project. This amends the Mac/Windows promise of SDK-476 and specification
  section 7. The amendment is recorded in the specification (section 7, acceptance check 9, Out of
  Scope) and in comments on SDK-476 and SDK-485.
- **The update rehearsal stays, and runs last.** Per-build adaptation cost has never been measured.
  The rehearsal blocks no earlier milestone, and it measures most when the full method set exists,
  so it is the final milestone (SDK-557; the rescoped SDK-485 closed on 2026-09-24).
- **The beta installation is preserved first.** Stellaris 4.5 leaves beta in the week of 2026-09-21.
  The only supported target record is the exact beta executable. A verified copy of that
  installation must exist before Steam updates it (SDK-522), or live work stops until a new
  target record exists and its tests pass.
  **Superseded, 2026-09-22:** Steam updated the installation to the full release, Cygnus v4.5.0
  (8697). Its target record (M45-release) replaces the beta record. Steam offers old full releases
  for download but not old open betas, so Native keeps full-release targets only. The beta ARM64
  executable stays in `.local/executables`.
- **Foundations come before the shared readers (2026-09-24).** After the Milestone 3 review and
  the [DX proposal](design/native-dx.md), Milestone 4 held its ten method tickets and about twenty
  repair and tooling tickets. Milestone 3.5 now holds the work that makes each method cheaper or
  safer to write, and the independent cleanup. Refactors that block no method (SDK-594, SDK-595)
  are in neither milestone. This amends the review's rule that repairs do not gate Milestone 4
  ([review, section 8.1](design/milestone-3-review.md#81-milestone-35-amendment-2026-09-24)).
- **A method is written in one task (2026-09-24).** The inspector (SDK-579), stop diagnostics
  and the sweep report (both SDK-581, which absorbed SDK-588 on 2026-09-24) replace the
  throwaway prototype. One task explores with the inspector, records findings and failed shapes
  on the method's page in `docs/native/` (indexed by `discovery.md`), and delivers the method with
  its authored tests. The open prototype children of SDK-470 are closed. Their unique
  cases moved into the production tickets; four remainders became SDK-607 to SDK-610.

## Milestones

Linear works milestones in order, so the order below is the work order.

**Simplification completed, 2026-09-20.** The [work order](design/simplification.md#work-order)
records the API migration, analysis and live reducer moves, removal of replay and qualification,
and private-directory cleanup. This work has no Linear tickets.

| # | Milestone | Work | Exit gate | Tickets |
| --- | --- | --- | --- | --- |
| 1 | Scoreboard | Preserve the beta installation. Claim ledger from the `.cwt` files with owner and documentation-provenance tags. The ledger work is done in the Atlas repository. | Every config line maps to a claim; a coverage percentage exists | SDK-522, SDK-523, SDK-525 |
| 2 | Registry schema | Static analysis context; registry candidates and ownership; items for every registry; seedless field discovery; reader binding; public fixture observation; separate parse, validation and runtime outcomes; full tradition observations to frozen Atlas. In Atlas: the first rule snapshot, then the `.cwt` comparison tool that reads it. | Rust results equal retained results (164 named template registries that agree with every live-observed directory; 10 agenda fields); the tradition snapshot raises coverage above the baseline and is compared with the config | SDK-527 to SDK-534, SDK-558, SDK-524 |
| 3 | Language declarations | Effects, triggers, modifiers, categories, scopes, links and localisation commands with engine description and usage text; on_actions and entry scopes; defines; generated modifier families | Each of the five `script-docs` logs and each config name list has an engine-derived answer in the snapshot, with its gaps in the ledger; the per-area coverage figures are recorded | SDK-535 to SDK-540, SDK-562, SDK-564 to SDK-568; Atlas: SDK-570 |
| 3.5 | Foundations | Shortcut guard; jump-table repair; stop diagnostics and the sweep report; test assembler helper; method-authoring guide; one pause owner in the worker; the review's refactors and cleanup | The shortcut guard passes; the 32 megastructure jump-table fields are found; a failed path is located from one inspector run; the sweep report groups failures by stop diagnostic; every refactor keeps parity output byte-identical | SDK-563, SDK-569, SDK-571, SDK-574, SDK-581, SDK-589, SDK-593, SDK-601 to SDK-606 |
| 4 | Shared readers | Field shapes and conditions, nested blocks, argument grammars, references and dynamic names, numerics, weights, naming rules, modifier application, scope context, script parameters. Each method ticket ends with one run over its full registry or command inventory. | The council agenda test, method fixture criteria and Atlas integration checks below pass; full-inventory counts and failure shapes are recorded | SDK-541 to SDK-550; preparation SDK-596; Atlas SDK-597 and SDK-577; observations SDK-598 and SDK-599; acceptance test SDK-600 |
| 5 | Registry sweep | Custom, nested and late registries; mounted files and duplicates; the method set, unchanged at a recorded commit, over all registries | Automatic rate known for all 253 types; every exception recorded | SDK-551 to SDK-553 |
| 6 | Other formats | Transfer tests on interface, graphics, sound, map and descriptor loaders | Each family is supported or an explicit gap in the ledger | SDK-554 to SDK-556 |
| 7 | Update rehearsal | Support the full method set on a new build with Atlas frozen; record the effort by category | Second executable passes with no Atlas change; routine update cost known | SDK-557 |

**Milestone 3 exit gate met, 2026-09-24 (SDK-570).** The Atlas language snapshot answers each
of the five `script-docs` logs and each config name list: effect, trigger, log scope-link and
localization entries are in it, and the ledger keeps its gaps. The snapshot holds the loaded
modifier table's counts; the 45,578 loaded names remain in the recorded answer. With a live snapshot
on config revision `8574760`, coverage is 15,278 of 58,032 Atlas-owned claims (26.33%); the
language areas give 14,137 of 31,751 (44.52%), from effects 25.97% to scopes and links 91.15%. No
on_action or game-rule entry scope is established: each followed call site supplies a self link or
an unresolved scope. See Atlas's [language snapshot measurement](https://github.com/pdx-foundry/atlas/blob/9c39c807ed062f65c893789edb16677b7ea843a1/docs/coverage/language-snapshot.md).

### Milestone 4 acceptance and start order

The accepted [Milestone 3 review](design/milestone-3-review.md) defines the work order.
SDK-596 owns preparation: align the contracts and tickets, repair duplicate modifier uncertainty,
and measure the current M45-release field population before changing discovery. Milestone 3.5
comes next: SDK-569's shortcut guard and SDK-563's jump-table repair block SDK-541, and SDK-574's
single pause owner blocks the new observers of SDK-598 and SDK-599. SDK-597 delivers Atlas
integration alongside SDK-541 and SDK-542.

Milestone 4 completes only when all of these hold:

1. **Council agenda parity (SDK-600):** `registry_fields("common/council_agendas")` is `Complete`
   with established reader kinds for all ten fields. Without typed gaps, establish `agenda_cost`
   storage kind, scale and script-value acceptance; the read conditions of `agenda_cooldown` and
   `agenda_finish_modifier_duration`; trigger family and entry scopes for `potential` and `allow`;
   effect family and entry scopes for `effect` and `init_effect`; the content-directory target of
   `finish_modifier`; the member family of `modifier`; and `ai_weight` keys, reader kinds and
   nested `modifier` entries. The test uses the public API and the exact supported executable.
2. **Fixture criteria:** all SDK-541 to SDK-550 acceptance criteria remain required. Numeric
   storage decoding belongs to SDK-544; SDK-598 blocks SDK-545's runtime weight checks; SDK-599
   blocks SDK-549's scope-availability check. SDK-542 and SDK-550 own any missing parser diagnostic
   hooks. Missing observations are unmet criteria, not static passes. Only Jackson may amend them.
   Existing SDK-547 runtime/application bounds and SDK-550 expansion bounds remain the exclusions.
3. **Method transfer:** each method runs unchanged over its full registry or command inventory,
   recording complete, partial and failed counts and distinct failure shapes. SDK-569 passes.
   Production registry validation follows the bound build; M45-release parity explicitly asserts
   164 registries as a regression expectation, not a production cap.
4. **Atlas integration (SDK-597):** two established field branches and one unresolved branch
   survive assembly, verification, comparison and coverage; an established argument never credits
   an unresolved sibling. Claims use typed subjects. Directories with several CWT types are joined
   or recorded as counted gaps. Credited rates come from the live run; recordings support reproduction.

The [specification](specs/native.md#milestone-4-shared-reader-acceptance) records the same contract.

Milestone 3 needs only the static context (SDK-527), so its first tickets are unblocked as soon as
that ticket is done. The blocking relations in Linear are the authority for what can start.

## Not yet ticketed

- Atlas-side rule work after the tradition snapshot (SDK-558): snapshot assembly for milestones
  5 and 6 (milestone 3 is SDK-570; milestone 4 is SDK-597), the run driver and rule composition of
  the registry sweep (SDK-553), the policy overlay
  (severity, subtype naming, alias factoring) keyed to rule identities, and the authored remainder
  of the documentation. These get the `Atlas` repository label.
- Installation discovery without a location hint (specification user story 2). It does not block
  config coverage.
- Private remote preservation of the prototype sources is tracked as SDK-486. Retained captures
  are no longer preserved.

## Tracking

Tickets are in the Linear **Atlas** project, one milestone per row above. The `Repo` label group
(`Native` or `Atlas`) says in which repository the work of a ticket is done. Each ticket is a
vertical slice: it ends at the public API, with tests on small tracked inputs and recorded
answers, checked through the Atlas caller. The open tickets agree with the simplification
decision (rewritten 2026-09-20). Completed tickets keep their original text; criteria in them that
name replay, retained captures or qualification records are superseded. Open extraction questions
stay with their Atlas map tickets; each Native ticket links to the question it implements and
reports its result there.
