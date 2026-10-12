# Roadmap: from two registries to full config coverage

This page orders the work. The [specification](specs/native.md) and
[technical design](design/architecture.md) own behavior and layout; Linear owns ticket status and
blocking order; the [Atlas map](https://linear.app/unnamed-system/issue/SDK-470/specify-pdx-atlas-and-its-engine-derived-rule-database)
owns Atlas decisions and open extraction questions.

## Goal

Atlas generates the data that `cwtools-stellaris-config` maintains by hand, and publishes it as
platform-independent JSON snapshots, at parity with the config. Native supplies every engine
observation that Atlas needs. Additional compiler facts beyond parity must pass the compiler-need
test and are scrutinized for overbuilding ([vision](design/simplification.md#vision)).
The [specification's operation table](specs/native.md#2-public-api) lists what Native answers now.

## The target, measured

At revision `8574760` the config fork has 50,195 lines in 175 `.cwt` files.

| Share | Content | Route |
| --- | --- | --- |
| ~37% | Script language: 1,060 effects, 1,089 triggers, 747 modifiers, scopes, 89 links, localisation commands | Engine declarations, then argument grammars |
| ~35% | 216 of the 257 type schemas: those under `common/` and `events/` | Registry discovery, field readers, references |
| ~13% | 2,042 defines, 385 on_actions, game rules | Inventories |
| ~8% | Interface, graphics, sound, map, descriptors | Separate loaders, not yet investigated |

Of the config's documentation entries, 4,014 of 5,784 are exact copies of base-game or
`script-docs` text; Atlas owns that [measurement](https://github.com/pdx-foundry/atlas/blob/67003ade425a3a071cc90db11c352206e49854d6/docs/coverage/documentation.md).
Severity, soft cardinality, subtypes and alias factoring are cwtools modelling decisions, not
engine knowledge; severity belongs to the compiler, and subtype names and alias factoring to the
`.cwt` emitter tests. The `.cwt` emitter is a test tool that compares a snapshot with the config;
the product is the JSON snapshot.

Coverage started at 0 of 56,551 Atlas-owned claims (item names answer no rule question). At the
Milestone 3 gate, a live snapshot on config revision `8574760` covered 15,278 of 58,032 claims
(26.33%); see Atlas's [language snapshot measurement](https://github.com/pdx-foundry/atlas/blob/9c39c807ed062f65c893789edb16677b7ea843a1/docs/coverage/language-snapshot.md).
At the Milestone 4 gate, SDK-625's live snapshot on M452 with Native `573e35f` covered 22,668 of
58,032 claims (39.06%) on the same config revision; Atlas's [Milestone 4 capture](https://github.com/pdx-foundry/atlas/blob/baf003a8982db386f53b47b5d87b7ce9efe7d8d5/docs/coverage/milestone-4.md)
gives the per-area figures.

## Ordering principle

Coverage grows with each **shared method**, not with each file. The known risk is method transfer:
the reference matcher failed on both unfamiliar resolver shapes, and five shared-reader contracts
block the council agenda completeness result. The
[development policy](development-policy.md#keep-engine-knowledge-in-its-home) states the method
rule and how transfer is measured.

Windows is deferred: Atlas output is platform-independent, and Windows returns with the real-game
testing project. The update rehearsal (SDK-557) runs last, when the full method set exists.

## Milestones

Linear works milestones in order. Milestones 1 to 3.5 are complete.

| # | Milestone | Work | Exit gate | Tickets |
| --- | --- | --- | --- | --- |
| 1 | Scoreboard | Preserve the beta installation. Claim ledger from the `.cwt` files with owner and documentation-provenance tags. The ledger work is done in the Atlas repository. | Every config line maps to a claim; a coverage percentage exists | SDK-522, SDK-523, SDK-525 |
| 2 | Registry schema | Static analysis context; registry candidates and ownership; items for every registry; seedless field discovery; reader binding; public fixture observation; separate parse, validation and runtime outcomes; full tradition observations to frozen Atlas. In Atlas: the first rule snapshot, then the `.cwt` comparison tool that reads it. | Rust results equal retained results (164 named template registries that agree with every live-observed directory; 10 agenda fields); the tradition snapshot raises coverage above the baseline and is compared with the config | SDK-527 to SDK-534, SDK-558, SDK-524 |
| 3 | Language declarations | Effects, triggers, modifiers, categories, scopes, links and localisation commands with engine description and usage text; on_actions and entry scopes; defines; generated modifier families | Each of the five `script-docs` logs and each config name list has an engine-derived answer in the snapshot, with its gaps in the ledger; the per-area coverage figures are recorded | SDK-535 to SDK-540, SDK-562, SDK-564 to SDK-568; Atlas: SDK-570 |
| 3.5 | Foundations | Shortcut guard; jump-table repair; stop diagnostics and the sweep report; test assembler helper; method-authoring guide; one pause owner in the worker; the review's refactors and cleanup | The shortcut guard passes; the 32 megastructure jump-table fields are found; a failed path is located from one inspector run; the sweep report groups failures by stop diagnostic; every refactor keeps parity output byte-identical | SDK-563, SDK-569, SDK-571, SDK-574, SDK-581, SDK-589, SDK-593, SDK-601 to SDK-606 |
| 4 | Shared readers | Field shapes and conditions, nested blocks, argument grammars, references and dynamic names, numerics, weights, naming rules, modifier nodes and container categories, scope context, script parameters. Each method ticket ends with one run over its full registry or command inventory. | The council agenda test, method fixture criteria and Atlas integration checks below pass; full-inventory counts and failure shapes are recorded | SDK-541 to SDK-550, SDK-607, SDK-705; preparation SDK-596; Atlas SDK-597, SDK-625, SDK-626 and SDK-577; entry contexts SDK-608 and SDK-677; developer tracing SDK-629; acceptance test SDK-600 |
| 5 | Registry sweep | Custom, nested and late registries; `events/`, with its event kinds and the scope that each kind sets; pre_trigger key sets; file selection, duplicate and definition-name rules; the method set, unchanged at a recorded commit, over all registries | Every config type under `common/` or `events/` has a Native registry status in the sweep report or an Atlas missing-join reason; rates per registry and per `(type, file)`; every exception recorded (see below) | SDK-551: SDK-778, SDK-779, SDK-783, SDK-784, SDK-786, SDK-787, SDK-789, SDK-794, SDK-802; SDK-552: SDK-788, SDK-790 to SDK-792, SDK-795, SDK-803; SDK-553: SDK-780; events SDK-702, SDK-801; pre_triggers SDK-703; Atlas SDK-781, SDK-785, SDK-793, SDK-796 to SDK-800; cost SDK-591; roadmap SDK-782 |
| 6 | Other formats | Transfer tests on interface, graphics, sound, map and descriptor loaders | Each family is supported or an explicit gap in the ledger | SDK-554 to SDK-556 |
| 7 | Update rehearsal | Support the full method set on a new build with Atlas frozen; record the effort by category | Second executable passes with no Atlas change; routine update cost known | SDK-557 |

### Milestone 4 acceptance

SDK-608 blocks SDK-677, which blocks SDK-600. SDK-625 delivers the Atlas integration and the final
live coverage run; SDK-626 triages the unowned gaps.

Milestone 4 completes only when all of these hold:

1. **Council agenda parity (SDK-600):** `registry_fields("common/council_agendas")` is `Complete`
   with established reader kinds for all ten fields. Without typed gaps, establish `agenda_cost`
   storage kind, scale and script-value acceptance; the read conditions of `agenda_cooldown` and
   `agenda_finish_modifier_duration`; trigger family and entry scopes for `potential` and `allow`;
   effect family and entry scopes for `effect` and `init_effect`; the content-directory target of
   `finish_modifier`; the member family of `modifier`; and `ai_weight` keys, reader kinds and
   nested `modifier` entries. The test uses the public API and the exact supported executable:
   `council_agenda_fields_are_complete_with_every_milestone_4_fact` in `tests/static_questions.rs`
   (`cargo parity council_agenda`). It checks each fact on the answer's value and requires
   `Complete`: a typed gap fails it, and neither a missing observation nor an `OutsideMethod`
   exclusion can satisfy it. It reports every missing fact in one run. Two typed gaps are accepted,
   by the rule that the gate is for Atlas's sake and should not go red for something Atlas does
   not need (Jackson, 2026-10-08). The `UnresolvedPath` entry gap of `ai_weight`: a template
   helper evaluates it on an array element, and no Atlas `replace_scopes` claim needs those
   contexts. The `NumericConversion` range gap of `ai_weight` `days`, `months`, `years` and
   `factor` (SDK-739): their storage must be known, but no live fixture reaches a weight key, and
   the config gives these keys no range.
2. **Fixture criteria:** the dated 2026-10-02 amendments on the Linear tickets govern SDK-541 to
   SDK-550. SDK-544 retains numeric storage controls. SDK-545 checks weight parsing and
   source-located diagnostics; SDK-549 checks read entry scope (`this`) with a fixture diagnostic;
   `check_script` cannot, because its caller supplies the scope.
   Runtime weights and scope availability are out of scope. SDK-542 and SDK-550 own required parser
   diagnostic hooks. SDK-547's application rule is a typed gap by design. SDK-608 and SDK-677 check
   static entry bindings against hand-read call sites and independent scope expectations.
3. **Method transfer:** each method runs unchanged over its full registry or command inventory,
   recording complete, partial and failed counts and distinct failure shapes. SDK-569 passes.
   Production registry validation follows the bound build; parity explicitly asserts
   164 registries as a regression expectation, not a production cap.
4. **Atlas integration (SDK-597, then SDK-625):** two established field branches and one
   unresolved branch survive assembly, verification, comparison and coverage; an established
   argument never credits an unresolved sibling. Claims use typed subjects. Directories with
   several CWT types are joined or recorded as counted gaps. Credited rates come from the live
   run; recordings support reproduction. SDK-597 owns the first slice (conditional branches,
   block families, shared directories, owner categories, stable gap reasons, the first live
   re-record with SDK-577). SDK-625 owns partial command grammars, the remaining claim types,
   parser fixture conclusions and a fresh recording, made on M452 after SDK-714. SDK-626 measures
   failure shapes after that pin move and creates tickets only for compiler-relevant shapes with
   a dependent config claim. Other shapes receive “out of scope, vision 2026-10-02”. SDK-625's
   final run credits each established command key only for its own claims, carries block,
   scope, value-form, reference and naming answers, and measures the SDK-626 table again; weight
   claims are credited at field level. Atlas publishes the conclusions of its own fixture games
   apart from static answers; the method tickets' parser checks are Native tests and earn no
   Atlas credit.

**Use-time inheritance.** Tradition inheritance affects the names and icons
selected at use time, although the parser reads the fields without those tests. SDK-541 retains
the inheritance condition-to-field extraction. `Native::derived_names` (SDK-546) uses it: the
swap's name carries `inherit_name` as a field condition, and the swap tooltips carry
`inherit_effects`. The tradition icon (`GFX_` and the key) is composed by interface code, outside
that method. SDK-597 must preserve the processing stage and unresolved context. This is not a new
exclusion from SDK-600. The exact-build findings and remaining work are in
[registry fields](native/registry-fields.md#read-conditions-and-use-time-inheritance)
and [derived names](native/derived-names.md).

### Milestone 5 acceptance

The "5 · Registry sweep" milestone description in the [Linear Atlas project](https://linear.app/unnamed-system/project/atlas-65cdd1747212)
holds the decisions of 2026-10-12 and the ticket blockers; its "Second review"
section supersedes the first plan where they differ.

**Gate population (D1).** The population is every config type whose `path` is under `common/` or
`events/` at config revision `8574760`: 216 of the 257 `type[…]` definitions, joined by
`(type, file)` pairs to about 180 directories. Each type has either a Native registry with a
per-relationship status in the sweep report (SDK-780, over the SDK-798 recording) or an Atlas
missing-join reason in the registry comparison (SDK-781). Atlas owns the count. Native reports
rates per registry; Atlas reports them per `(type, file)` (SDK-781) and per claim. The `gfx`,
`interface`, `sound`, `music`, `flags`, `fonts`, `map/setup_scenarios` and `prescripted_countries`
families are Milestone 6.

**Lanes.** Three lanes run side by side:

1. **Registries (SDK-551).** SDK-779 reads the scheduler table as the candidate inventory; SDK-783
   names the registries whose own loader enumerates a directory and carries the listed-name
   contract: a listed registry gets an answer or `Unsupported`, never `UnknownRegistry`. SDK-786
   joins a directory that a caller enumerates; SDK-787 answers `registry_fields` through the
   owner, and SDK-802 answers `derived_names` and `modifier_families`, for non-template
   registries. SDK-789 and SDK-794 join the strategic-resource and planet-class generation sites
   and name two-registry modifier templates. SDK-778 investigates the events route and nested
   identity; SDK-784 settles nested definitions, or closes with no session if SDK-778 recommends
   an amendment.
2. **Events and pre_triggers.** SDK-702 establishes event kinds and the scope each kind sets,
   starting with one vertical case; SDK-703 extracts the pre_trigger key sets of the two registry
   fields; SDK-801 extracts those of event kinds.
3. **Loader rules (SDK-552).** SDK-788 adds the loader-rules operation and file selection
   (`path_file` included); SDK-790 states the duplicate, file-order and mod-over-vanilla rules;
   SDK-791 states definition-name rules; SDK-792 observes a repeated key in one fixture file and
   SDK-803 several files in one session; SDK-795 checks each rule shape with a fixture.

On the Atlas side, SDK-799 checks the entry-scope answers again against `callbacks/v4` before any
recording refresh; SDK-800 decides whether a `type_key_filter` subtype can earn coverage; SDK-785
joins a directory with several config types, keeping the type selectors; SDK-793 and SDK-797
carry events, pre_triggers and loader rules into snapshots and coverage.

**Pin moves.** Atlas moves its Native pin twice. Pin A (SDK-796) follows the registry, events and
pre_trigger work: SDK-799, SDK-781, SDK-783, SDK-787, SDK-802 and SDK-793. Pin C (SDK-798) is
the sweep's recorded commit, after every other ticket: Atlas removes its local path override,
pins, makes a live run with recording, takes coverage from the live run, and reports the rates and
the SDK-591 cost measurement. SDK-553 then records the commit, the method set, the report and one follow-up
ticket per new failure shape.

**Order.** Tickets in one wave can run side by side. Live tickets launch the game, and only one
game runs at a time.

| Wave | Ticket | Repo | Waits for |
| --- | --- | --- | --- |
| 1 | SDK-782 Restate the exit gate in the roadmap | Native | — |
| 1 | SDK-800 Decide whether a `type_key_filter` subtype can earn coverage | Atlas | — |
| 1 | SDK-778 Investigate the events loader and nested-definition identity | Native | — |
| 1 | SDK-779 Read the scheduler table | Native | — |
| 1 | SDK-799 Check the entry scopes again against `callbacks/v4` | Atlas | — |
| 1 | SDK-780 Sweep report over a recorded-answers directory | Native | — |
| 1 | SDK-781 Compare registries with config types | Atlas | — |
| 1 | SDK-703 Pre_trigger key sets of the two registry fields | Native | — |
| 2 | SDK-783 Name custom-loader registries, with the listed-name contract | Native | SDK-779 |
| 2 | SDK-785 Join directories that hold several config types | Atlas | SDK-778, SDK-800 |
| 2 | SDK-784 Nested definitions (or close it) | Native | SDK-778 |
| 3 | SDK-786 Join a directory that a caller enumerates | Native | SDK-783 |
| 3 | SDK-787 Owner join and `registry_fields` | Native | SDK-783 |
| 3 | SDK-788 Loader-rules operation and file selection | Native | SDK-783 |
| 4 | SDK-802 `derived_names` and `modifier_families` for new registries | Native | SDK-787 |
| 4 | SDK-702 Event kinds and their scopes | Native | SDK-778, SDK-783, SDK-787 |
| 4 | SDK-790 Duplicate, file-order and mod-over-vanilla rules | Native | SDK-788 |
| 4 | SDK-791 Definition-name rules | Native | SDK-788 |
| 4 | SDK-792 Repeated key in one fixture file (live) | Native | SDK-788 |
| 5 | SDK-789 Strategic-resource and planet-class family joins (live) | Native | SDK-786, SDK-802 |
| 5 | SDK-801 Event pre_trigger key sets | Native | SDK-702, SDK-703 |
| 5 | SDK-803 Several files in one fixture session (live) | Native | SDK-792 |
| 6 | SDK-794 Two-registry modifier templates (live) | Native | SDK-789 |
| 6 | SDK-795 One fixture per loader rule shape (live) | Native | SDK-790, SDK-791, SDK-803 |
| 6 | SDK-793 Event kinds and pre_triggers into Atlas | Atlas | SDK-785, SDK-702, SDK-703, SDK-801 |
| 7 | SDK-796 Pin A | Atlas | SDK-799, SDK-781, SDK-783, SDK-787, SDK-802, SDK-793 |
| 7 | SDK-797 Loader rules into Atlas | Atlas | SDK-785, SDK-788, SDK-790, SDK-791 |
| 8 | SDK-798 Pin C: the sweep run | Atlas | All of the above |
| 8 | SDK-553 Record the method set and the follow-up tickets | Native | SDK-798 |

**Baseline (D2).** The sweep is a baseline. Repairs for known failure shapes may land before pin C,
and the run reflects them; none is mandatory. The rest are labelled deficits in the report.

**Decisions of 2026-10-12.**

- D3: fixture observation support (SDK-792, SDK-803) is split from the rule controls (SDK-795).
- D4: event kinds as root fields of `registry_fields("events")` is a hypothesis until SDK-778 and
  one vertical case in SDK-702 and SDK-793 prove it.
- D5: `Registry` keeps only `name`; the loader rules are one separate string-keyed operation
  (SDK-788).
- D6: SDK-640 and SDK-651 close through the superseding SDK-770, SDK-771 and SDK-774 decisions,
  with the whole-inventory recording time that SDK-780 records.
- D7: SDK-778 is the one investigation, and it is narrow; SDK-779 and SDK-783 start directly.

## Release and later work

The first release (SDK-511) is one schema-valid snapshot for one catalogued build, checked by the
end-to-end offline test; SDK-511 owns the tradition and category composition verdict from SDK-600
and SDK-553. The update rehearsal is not a release gate, and there is no demo consumer.
Ship-size modifier templates are SDK-678. Atlas-side work after the tradition snapshot is SDK-558.
Private remote preservation of the prototype sources is SDK-486.

Tickets are in the Linear **Atlas** project, one milestone per row above; the `Repo` label
(`Native` or `Atlas`) says where the work is done. Completed tickets keep their original text;
criteria in them that name replay, retained captures or qualification records are superseded.
