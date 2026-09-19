# Roadmap: from two registries to full config coverage

Status: proposed plan, 2026-09-19. The [Native specification](specs/native.md) and
[technical design](design/architecture.md) remain the authority for behavior and layout. The
[Atlas map](https://linear.app/unnamed-system/issue/SDK-470/specify-pdx-atlas-and-its-engine-derived-rule-database)
remains the authority for decisions and open extraction questions. This document only orders the work.

## Goal

Atlas generates the data that `cwtools-stellaris-config` maintains by hand, and publishes it as
platform-independent JSON snapshots. Native supplies every engine observation that Atlas needs.

## Starting point

Native admits one production operation: item names for `traditions` and `tradition_categories` on
one exact Mac ARM64 executable. Supervision, admission, capture, replay and the frozen Atlas caller
are verified. There is no `engine/analysis` module. All analysis methods (registry ownership, field
discovery, reader binding, references, numeric grammar, command inventories) exist only as retained
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
parser and the source-tagged ledger. Native's future declarations operation remains separate.

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

## Ordering principle

Coverage grows with each **shared method**, not with each file. The known risk is method transfer:
the frozen reference matcher failed on both unfamiliar resolver shapes, and five shared-reader
contracts block the council agenda completeness result. Each method is frozen and then tested on
held-out cases. The registry sweep measures the automatic rate; its failures return as new reader
shapes, not as handwritten answers.

## Decisions taken with this roadmap

- **The `.cwt` emitter is a test tool.** It compares an Atlas snapshot with the config fork. The
  product remains the JSON snapshot (SDK-477).
- **Windows is deferred.** Atlas output is platform-independent. Windows support returns with the
  real-game testing project. This amends the Mac/Windows promise of SDK-476 and specification
  section 7. The amendment is recorded in the specification (section 7, acceptance check 9, Out of
  Scope) and in comments on SDK-476 and SDK-485.
- **The update rehearsal stays, and runs last.** Per-build adaptation cost has never been measured.
  The rehearsal blocks no earlier milestone, and it measures most when the full method set exists,
  so it is the final milestone (rescoped SDK-485).
- **The beta installation is preserved first.** Stellaris 4.5 leaves beta in the week of 2026-09-21.
  The only accepted qualification is pinned to the exact beta executable and 68 content files. A
  verified copy of that installation must exist before Steam updates it (SDK-522), or live work
  stops until a forced requalification.

## Milestones

Linear works milestones in order, so the order below is the work order.

| # | Milestone | Work | Exit gate | Tickets |
| --- | --- | --- | --- | --- |
| 1 | Scoreboard | Preserve the beta installation. Claim ledger from the `.cwt` files with owner and documentation-provenance tags; `.cwt` comparison tool. The ledger work is done in the Atlas repository. | Every config line maps to a claim; a coverage percentage exists | SDK-522 to SDK-525 |
| 2 | Registry schema | Static analysis context; registry candidates and ownership; items for every registry; seedless field discovery; reader binding; public fixture observation; separate parse, validation and runtime outcomes; full tradition observations to frozen Atlas | Rust results equal retained results (41/41 ownership controls, 10 agenda fields); tradition rules compared with the config | SDK-527 to SDK-534 |
| 3 | Language declarations | Effects, triggers, modifiers, categories, scopes, links and localisation commands with engine description and usage text; on_actions and entry scopes; defines; generated modifier families | The five `script-docs` logs and the config name lists are replaced | SDK-535 to SDK-540 |
| 4 | Shared readers | Field shapes and conditions, nested blocks, argument grammars, references and dynamic names, numerics, weights, naming rules, modifier application, scope context, script parameters. Each method is frozen, then tested on held-out cases. | Council agenda completeness passes; held-out rate recorded per method | SDK-541 to SDK-550 |
| 5 | Registry sweep | Custom, nested and late registries; mounted files and duplicates; frozen methods over all registries | Automatic rate known for all 253 types; every exception recorded | SDK-551 to SDK-553 |
| 6 | Other formats | Transfer tests on interface, graphics, sound, map and descriptor loaders | Each family is supported or an explicit gap in the ledger | SDK-554 to SDK-556 |
| 7 | Update rehearsal | Requalify the full method set on a new build with Atlas frozen; record the effort by category | Second executable passes with no Atlas change; routine update cost known | SDK-557 |

Milestone 3 needs only the static context (SDK-527), so its first tickets are unblocked as soon as
that ticket is done. The blocking relations in Linear are the authority for what can start.

## Not yet ticketed

- Atlas-side rule work after the scoreboard: snapshot assembly per milestone, the policy overlay
  (severity, subtype naming, alias factoring) keyed to rule identities, and the authored remainder
  of the documentation. These get the `Atlas` repository label.
- Installation discovery without a location hint (specification user story 2). It does not block
  config coverage.
- Private remote evidence preservation is already tracked as SDK-486.

## Tracking

Tickets are in the Linear **Atlas** project, one milestone per row above. The `Repo` label group
(`Native` or `Atlas`) says in which repository the work of a ticket is done. Each ticket is a
vertical slice: it ends at the public interface, with replayable evidence and a qualification
record, checked through the frozen Atlas caller. Open extraction questions stay with their Atlas
map tickets; each Native ticket links to the question it implements and reports its result there.
