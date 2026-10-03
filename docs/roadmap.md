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

The config fork has 49,196 lines in 172 `.cwt` files.

| Share | Content | Route |
| --- | --- | --- |
| ~37% | Script language: 1,060 effects, 1,089 triggers, 747 modifiers, scopes, 89 links, localisation commands | Engine declarations, then argument grammars |
| ~35% | 253 type schemas under `common/` | Registry discovery, field readers, references |
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
| 4 | Shared readers | Field shapes and conditions, nested blocks, argument grammars, references and dynamic names, numerics, weights, naming rules, modifier nodes and container categories, scope context, script parameters. Each method ticket ends with one run over its full registry or command inventory. | The council agenda test, method fixture criteria and Atlas integration checks below pass; full-inventory counts and failure shapes are recorded | SDK-541 to SDK-550, SDK-607; preparation SDK-596; Atlas SDK-597, SDK-625, SDK-626 and SDK-577; entry contexts SDK-608 and SDK-677; developer tracing SDK-629; acceptance test SDK-600 |
| 5 | Registry sweep | Custom, nested and late registries; mounted files and duplicates; the method set, unchanged at a recorded commit, over all registries | Automatic rate known for all 253 types; every exception recorded | SDK-551 to SDK-553 |
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
   nested `modifier` entries. The test uses the public API and the exact supported executable.
2. **Fixture criteria:** the dated 2026-10-02 amendments on the Linear tickets govern SDK-541 to
   SDK-550. SDK-544 retains numeric storage controls. SDK-545 checks weight parsing and
   source-located diagnostics; SDK-549 checks read entry scope (`this`) with a fixture diagnostic;
   `check_script` cannot, because its caller supplies the scope.
   Runtime weights and scope availability are out of scope. SDK-542 and SDK-550 own required parser
   diagnostic hooks. SDK-547's application rule is a typed gap by design. SDK-608 and SDK-677 check
   static entry bindings against hand-read call sites and independent scope expectations.
3. **Method transfer:** each method runs unchanged over its full registry or command inventory,
   recording complete, partial and failed counts and distinct failure shapes. SDK-569 passes.
   Production registry validation follows the bound build; M45-release parity explicitly asserts
   164 registries as a regression expectation, not a production cap.
4. **Atlas integration (SDK-597, then SDK-625):** two established field branches and one
   unresolved branch survive assembly, verification, comparison and coverage; an established
   argument never credits an unresolved sibling. Claims use typed subjects. Directories with
   several CWT types are joined or recorded as counted gaps. Credited rates come from the live
   run; recordings support reproduction. SDK-597 owns the first slice (conditional branches,
   block families, shared directories, owner categories, stable gap reasons, the first live
   re-record with SDK-577). SDK-625 owns partial command grammars, the remaining claim types,
   parser fixture conclusions and a fresh M451-hotfix recording. SDK-626 measures failure shapes
   after that pin move and creates tickets only for compiler-relevant shapes with a dependent
   config claim. Other shapes receive “out of scope, vision 2026-10-02”.

**Use-time inheritance.** Tradition inheritance affects the names and icons
selected at use time, although the parser reads the fields without those tests. SDK-541 retains
the inheritance condition-to-field extraction; SDK-546 depends on it for conditional naming
templates. SDK-597 must preserve the processing stage and unresolved context. This is not a
new exclusion from SDK-600. The exact-build findings and remaining work are in
[registry fields](native/registry-fields.md#read-conditions-and-use-time-inheritance-m45-release).

## Release and later work

The first release (SDK-511) is one schema-valid snapshot for one catalogued build, checked by the
end-to-end offline test; SDK-511 owns the tradition and category composition verdict from SDK-600
and SDK-553. The update rehearsal is not a release gate, and there is no demo consumer.
Ship-size modifier templates are SDK-678. Atlas-side work after the tradition snapshot is SDK-558.
Private remote preservation of the prototype sources is SDK-486.

Tickets are in the Linear **Atlas** project, one milestone per row above; the `Repo` label
(`Native` or `Atlas`) says where the work is done. Completed tickets keep their original text;
criteria in them that name replay, retained captures or qualification records are superseded.
