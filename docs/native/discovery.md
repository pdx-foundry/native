# Discovery methods

Each method records its engine facts, current result, gaps and pitfalls on the page for
its subject. Add a page when a method starts a new subject. The code and its module comments
describe the methods; these pages hold what the code cannot.

Use the [method-authoring guide](method-authoring.md) to explore, implement and check a method in
one task. Each row below is one method source stamp; the two callback operations share a method.
Module paths are relative to `src/engine/analysis/` unless a full `src/` path is shown. The table
names the method owners, not every shared decoder or evaluator they use. It covers the static
methods, their live loaded-modifier join and the world observation; other live observations are
in [early observations](early-observations.md).

The SDK-542 extraction, parser checks, population counts and consumer contract are in
[nested command grammar](command-grammar.md).

| Operation | Source stamp | Modules | Knowledge section |
| --- | --- | --- | --- |
| `Native::registries` | `registry-directories/v3` | `discovery.rs`, `directories.rs` | [Registry candidates and owner joins](registry-fields.md#registry-scheduling-and-owner-joins) |
| `Native::registry_fields` | `registry-fields/v10` | `fields.rs`, `fields/control_flow.rs`, `fields/dispatch.rs`, `fields/inventory.rs`, `fields/nested.rs`, `fields/persistent.rs`, `fields/uses.rs`, `fields/records.rs`, `fields/tokens.rs`, `readers.rs` | [Field sweep and stops](registry-fields.md#current-m45-sweep) |
| `Field.reference` in `registry_fields` and `command_grammar` | `registry-fields/v10`, `command-grammar/v12` | `references.rs`, `references/initialization.rs`, `references/shapes.rs`, `src/binding/binary/references.rs` | [References and dynamic names](references.md) |
| `Reader.numeric` in fields and command grammar | `registry-fields/v10`, `command-grammar/v12` | `numeric.rs`, `numeric/modifier.rs`, `src/binding/binary/numeric.rs` | [Numeric conversion](numeric-conversion.md) |
| `Reader.scoped_operand` in fields and command grammar | `registry-fields/v10`, `command-grammar/v12` | `scoped_numeric.rs`, `src/binding/binary/scoped_numeric.rs`, `src/session/scoped_numeric.rs` | [Scoped numeric](scoped-numeric.md) |
| `Game::observe_world` | `observe-world/v2` | `src/engine/operations/world.rs`, `src/world.rs`, the world recipe in `src/binding/targets/recipes.rs` | [Ready-world observations](ready-world.md) |
| `CommandGrammar.durations` | `command-grammar/v12` | `durations.rs`, `src/binding/binary/durations.rs`, `src/session/durations.rs` | [Duration keys](durations.md) |
| `Native::dynamic_names` | `dynamic-names/v2` | `dynamic_names.rs`, `dynamic_names/routes.rs`, `declarations/receiver.rs` | [Dynamic names](references.md#dynamic-names) |
| `Native::command_grammar` | `command-grammar/v12` | `grammar.rs`, `grammar/coverage.rs`, `grammar/forms.rs`, `grammar/numeric.rs`, `grammar/ordering.rs`, `grammar/targets.rs`, `declarations/receiver.rs` | [Nested command grammar](command-grammar.md) |
| `Native::declarations` | `command-declarations/v3` | `declarations.rs`, `declarations/composition.rs` | [Effects and triggers](engine-commands.md#effects-and-triggers) |
| `Native::modifiers` | `modifier-declarations/v1` | `modifiers.rs` | [Modifiers](engine-commands.md#modifiers) |
| `Native::modifier_categories` | `modifier-categories/v1` | `modifiers.rs` | [Categories](engine-commands.md#categories) |
| `Native::scopes` | `scope-declarations/v1` | `scopes.rs` | [Scope types](engine-commands.md#scope-types) |
| `Native::scope_links` | `scope-links/v2` | `scopes.rs` | [Scope links](engine-commands.md#scope-links) |
| `Native::localization_declarations` | `localization-declarations/v1` | `localization.rs` | [Localization contexts, commands and links](engine-commands.md#localization-contexts-commands-and-links) |
| `Native::on_actions`, `Native::game_rules` | `callbacks/v1` | `callbacks.rs`, `callbacks/names.rs`, `callbacks/contexts.rs` | [On_actions, game rules and entry scopes](engine-commands.md#on_actions-game-rules-and-their-entry-scopes) |
| `Native::defines` | `defines/v1` | `defines.rs` | [Define read helpers](#define-read-helpers) |
| `Native::modifier_families` | `modifier-families/v3` | `families.rs`, `families/joins.rs`, `families/loading.rs`, `families/strings.rs` | [Generation calls and roots](modifier-families.md#engine-code-m45-release) |
| `Game::loaded_modifiers` | `loaded-modifiers/v1` | `modifier_table.rs`, `src/engine/operations/loaded_modifiers.rs`, `src/session/loaded_modifiers.rs` | [Loaded modifier table](modifier-families.md#the-loaded-modifier-table) |

This page also holds the define read helpers and the reference and dynamic-name counts. The
retired SDK-482 reference method is recorded on [references](references.md#sdk-482-prototype).

## References

On M45-release the reference method runs over every registry field bound to a reference reader:
29 fields in the 164 registries, **21 complete, 7 partial, 1 failed**. Failure shapes: key lists
(6), a reader of another shape (1), and a reader with neither shape nor directory (1). The reader
population, the per-shape counts and the obstacles are on [references](references.md#result-on-m45-release).

The owner-initialization run covers all 309 `PostInit()` functions; 154 name a global instance:
**61 complete, 2 partial, 91 failed**. Failure shapes: another shape (74), several lookups in one
initializer (14), and a getter of another shape (3). Over the command inventory, 16 effects and
8 triggers join a complete lookup to a child key, 41 commands have a lookup without an authored
field, and 112 have an initializer lookup that is not established. The breakdown is on
[references](references.md#owner-initializers).

## Dynamic names

On M45-release `Native::dynamic_names` examines all 2,170 registered commands (1,074 effects,
1,096 triggers) in about four seconds. 12 are not examined because their command object is not
joined (9 `command-vtable`, two `factory-terminal`, one `instruction`). 136 store an interned
flag name: **116 complete** (57 effects define, 29 remove, 30 triggers read, each with every
declared scope's store), **3 partial** (the astral rift flags: a role, but no declared scope set),
**14 failed** (a stored name without a define, remove or read role) and **3 with an unresolved
stored index**. The complete commands form 31 namespaces:
one global store and 30 scope stores, all accepting `name@target`. The failure shapes and
findings are on [references](references.md#dynamic-names).

## Define read helpers

On M45-release, `Native::defines()` (`defines/v1`) finds 2,385 compiled `NDefines` and
`NUncheckedDefines` `ReadDefine` helpers, and follows 2,305 of them to a literal namespace, a
literal name and a typed engine reader, in about four seconds. Resolved types are 1,091
fixed-point, 672 integer, 326 string, 172 float, 23 list, 13 vector and 8 boolean. The other 80
named helpers use a table-search loop that exceeds the path search; they are `UnresolvedReader`
gaps, including `NGraphics.ORBIT_HSV`. There are no failed or unnamed helpers.

The method classifies the target of each direct `GetValue`, `GetArrayValue` or
`ReadDefinesValue` call. `GetValue` takes namespace and name arguments; `GetArrayValue` reads a
named value from a namespace table. Shipped define entries, defaults, comments, bounds and uses
are not established here. SDK-610 owns the broader extraction question, and Atlas owns the
comparison with shipped content and config.

The [SDK-542 architecture review verification](command-grammar-review.md) records confirmed
repairs and the evidence for retained reader, family and observation boundaries.

## Direct numeric conversion (SDK-644)

The exact M45-release run covered all 164 discovered registries. Numeric fields occurred in
63 registries: **0 complete, 184 partial, 0 failed** conversion answers, with no registry query
failures. These are root field counts; repeated joins are not separate fields. The fields divide
into 93 integer, 88 fixed-point and 3 float readers. Nested template storage has its separate
live fixture and static reader control; it is not counted as a discovered root field.

All 11 bound shared numeric readers have partial facts, including signatures with no root field
in this population. The modifier entry boundary joins the direct fixed-point reader. Each numeric
answer carries `GapKind::NumericConversion`. Remaining internal gap shapes are `numeric-overflow`,
`numeric-lexical-boundary`, `numeric-trailing-text`, `numeric-external-library-conversion`, and,
for fixed-point readers, `numeric-raw-value-mode`. Narrow integer signedness remains unresolved.
No accepted range is inferred from storage width. No unsupported shape was encountered among
these 11 bound readers; unmatched wrappers, token bodies, destinations, raw paths and modifier
joins have authored negative controls and explicit unresolved results.

The prior SDK-643 storage population had 181 broadly numeric root fields. The three additional
fields here are float readers, now classified separately. This static run does not establish
live storage decoding for float, byte, short, unsigned integer or long-long fields. The live
matrix covers direct int, direct fixed point (including transfer to armies), and nested template
fixed point: 80 cases, 84 stored occurrences. See [numeric conversion](numeric-conversion.md)
for the observed boundary behavior and limits. The report is `.local/sdk-644/numeric-population.json`;
reproduce it with `cargo run --release --example numeric-population` and `STELLARIS_PATH` set.

## Numeric boundary evidence (SDK-655)

On M451-hotfix (`29fa877366040a528098da39ec7e70b7baac76782a2a6bd161616d691f86fa38`),
the numeric and scoped population examples cover all 164 registries; the scoped example also
covers all 2,170 commands. No registry or command question fails. Int, direct fixed-point and
fixed-point template conversions have known faithful-storage ranges; other conversion gaps
keep their enclosing answers partial. Scoped literals inherit ranges only after their concrete
storage is established.

| Population | Complete / partial / failed | Known ranges | Unresolved ranges |
| --- | --- | --- | --- |
| 184 numeric root fields | 0 / 184 / 0 | 174 | 10 |
| Seven scoped registry destinations | 0 / 7 / 0 | 7 | 0 |
| 302 scoped command arguments | 0 / 267 / 35 | 267 | 35 |

Three of the 11 shared numeric readers have known ranges. The root fields without known ranges
are seven short and three float fields. Scoped storage is two integer and five fixed-point
fields, and 123 integer, 144 fixed-point and 35 unresolved command arguments, after the
[owner-derivation recovery](#owner-derivation-sdk-658). Every destination without established
storage has `Reader.numeric: Unresolved`, so none retains a range. The
[remaining constructor obstacles](scoped-numeric.md#remaining-constructor-obstacles) explain the
unresolved command storage.

Failure shapes, counting each affected destination once per shape:

- Each numeric root field retains `numeric-overflow`, `numeric-lexical-boundary`,
  `numeric-trailing-text` and `numeric-external-library-conversion`; 88 also retain
  `numeric-raw-value-mode`. Three retain `numeric-float-bound-representation`, exposed as a
  public `NumericConversion` gap stating that `NumericBound` cannot represent exact binary32
  endpoints. Narrow signedness remains unresolved.
- All seven scoped fields retain `NumericConversion: Scoped literal conversion boundaries and
  overflow are incomplete.`, the qualified-scope/parameter/reference/evaluation `OutsideMethod`
  gap, and `UnresolvedStorage: Repeat behavior or nested fields remain unresolved.`
- The 267 scoped arguments with established storage retain the same numeric-conversion and
  outside-method gaps. The other 35 retain
  `UnresolvedStorage: Scoped destination vtable is not established.`

[Numeric conversion](numeric-conversion.md#boundary-evidence-on-m451-hotfix-sdk-655) records
matched engine paths, platform boundary checks, agreeing live boundaries and the remaining
obstacles. [Scoped literal ranges](scoped-numeric.md#shared-literal-ranges-sdk-655) records the
registry fixture coverage; it does not establish storage for unresolved command destinations.
No amendment of SDK-544 is made. Reports are
`.local/sdk-655/rebased/numeric-population.json` and
`.local/sdk-655/rebased/scoped-numeric-population.json`; reproduce with `STELLARIS_PATH` set:

```sh
cargo run --release --example numeric-population > .local/sdk-655/rebased/numeric-population.json
cargo run --release --example scoped-numeric-population > .local/sdk-655/rebased/scoped-numeric-population.json
```

## Scoped numeric operands (SDK-645)

The M45-release run queried all **164** discovered registries with no failed registry queries.
Seven fields bind to scoped numeric readers: **0 complete, 6 partial, 1 failed**. The six partial
answers establish concrete storage and shared operand routes: agenda cost, three ship-of-size
limit fields, and the two megastructure cycle/overclock fields. The failed destination is
`pop_decline_rate`: the shared entry is joined but its constructor-installed subtype is not
established. No field name selects a subtype.

Failure shapes: six numeric-conversion gaps (lexical boundaries, range and overflow), seven
unresolved repeat/nested-shape gaps, and one unresolved constructor vtable. Six answers also
retain an outside-method limit for qualified scopes, parameters, lookup outcomes and evaluation.
These counts describe the public answer, not just successful storage derivation. Run
`cargo run --release --example scoped-numeric-population` with `STELLARIS_PATH` to reproduce;
`.local/sdk-645/scoped-population.json` lists every field and gap.

The live matrix covers the 62 retained parser cases through documented root-field substitutions,
plus 14 boundary and transition controls. All 76 have complete parser/storage joins. The three
isolated inline-block inputs have incomplete diagnostic source coverage, retained as typed gaps.
There were no conflicts between the established static width/scale and the observed storage.
See [scoped numeric operands](scoped-numeric.md) for the adaptation and selection boundaries.

## World evaluation of scoped operands (SDK-647)

No static method changed. The M451-hotfix run of `scoped-numeric-population` covers all **164**
registries and all **2,170** commands, with no failed question.

| Population | Destinations | Complete | Partial | Failed |
| --- | ---: | ---: | ---: | ---: |
| Registry fields | 7 | 0 | 6 | 1 |
| Command arguments (207 commands) | 302 | 0 | 133 | 169 |

The partial arguments are 98 integer (32-bit, scale 1) and 35 fixed-point (64-bit, scale 100000)
destinations; the fields are 2 and 4. Failure shapes, by destination:

- 169 arguments and 1 field: the destination's constructor vtable is not established, so the
  storage and the evaluation body are unknown.
- 133 arguments and 6 fields: literal conversion boundaries and overflow are incomplete.
- 133 arguments and 6 fields: qualified scope, parameters and lookup outcomes are outside the
  static method.
- 7 fields: repeat behavior or nested fields are unresolved.

The world matrix evaluates 48 operand cases in an integer and a fixed-point destination (96
evaluations) on the 4.5.1 save, in four world sessions:

- **42 cases** evaluate with no message. Their three sessions give complete answers.
- **6 cases** execute, log and give zero. Their session is partial. Shapes: an unset variable
  (four cases, one with an empty name and one with an unknown prefix as its name), an unresolved
  event target, and a trigger with a wrong scope type at evaluation.
- **5 operands** are rejected at read or validation and are not executed. Shapes: unknown
  scripted trigger, unknown script value, Boolean trigger, and a wrong trigger scope (two).
- **0 conflicts** with the static storage, scale, selection and integer conversion facts.

Nine resource changes and both naval-capacity modifiers are observed in a fifth session, which
is complete. The results, the gaps and the map of the 41 SDK-493 evaluations are on
[scoped numeric](scoped-numeric.md#world-evaluation-on-m451-hotfix-sdk-647); the first-release
accounting is on [numeric conversion](numeric-conversion.md#first-release-numeric-forms-sdk-544).
Run `cargo run --release --example scoped-numeric-population` and `cargo live world_numeric`
with `STELLARIS_PATH` to reproduce; `.local/sdk-647/scoped-population.json` lists every
destination and gap.

## Duration keys (SDK-646)

`CommandGrammar.durations` groups keys that store one count, establishes their factors and
combination, and reports omitted state and consumers independently. A missing property remains
a typed gap. Registry grouping is a developer population question rather than a public field
property. The [duration page](durations.md) holds the build-specific facts and live observations;
the SDK-657 table below gives the current population and comparisons. The original M45-release
baseline is retained in `.local/sdk-646/duration-population.json`.

## Constructor state (SDK-654)

SDK-654 added constructor bodies only when every write was proved confined to the object; the
[owner-derivation proof](#owner-derivation-sdk-658) replaces that rule. The compiler-summary path
supplies the independent baseline; entered bodies cannot remove its facts.

The [numeric boundary population](#numeric-boundary-evidence-sdk-655) records current scoped
counts, storage and failure shapes. The scoped constructor baseline is
`.local/sdk-654/floor/now-scoped.json`; duration baseline is
`.local/sdk-654/floor/now-duration.json`. The SDK-657 scoped comparison is
`.local/sdk-657/rebased/scoped-population.json`.

| Duration population | Main | Pre-review SDK-654 | Current SDK-654 |
| --- | --- | --- | --- |
| Groups, complete / partial / failed | 0 / 31 / 0 | 27 / 4 / 0 | 0 / 31 / 0 |
| Established omitted counts | 0 | 29 | 0 |

Under the confinement rule, `pop_decline_rate` was the only scoped constructor gain over main,
with signed 64-bit storage at scale 100000. The pre-review scoped result (0 / 301 / 1 command
arguments) was not supported by that proof; its report remains a comparison artifact.

## Owner derivation (SDK-658)

Entered constructor bodies now add bytes that no later write may change, using the owner-derivation
proof on [scoped numeric](scoped-numeric.md#owner-derivation-sdk-658). The M451-hotfix populations
cover all 2,170 commands and 164 registries, with no failed question. `.local/sdk-658/compare.py`
compares them field by field with `main` (`34841c4`): no answer decreases, 134 command arguments
and two duration groups gain facts. Each recovered storage agrees with the unconfined SDK-654
report.

| Population | `main` | SDK-658 |
| --- | --- | --- |
| Scoped command arguments, complete / partial / failed | 0 / 133 / 169 | 0 / 267 / 35 |
| Scoped registry destinations, complete / partial / failed | 0 / 7 / 0 | 0 / 7 / 0 |
| Duration groups, complete / partial / failed | 0 / 52 / 0 | 1 / 51 / 0 |
| Established omitted counts | 0 | 2 |

The recovered arguments are 102 `order_by` and 7 other fixed-point operands (64-bit, scale
100000), and 20 event `random`, the relation flag's three units, `set_saved_date.expires` and
`steal_specimens.count` (32-bit, scale 1). `set_timed_relation_flag` gains its initial factor 1;
it and `add_timed_trait` gain omitted count 0.

The 35 failed arguments have one failure shape, `UnresolvedStorage: Scoped destination vtable is
not established`. Arguments by obstacle: 20 event `days` (`CToken` copy), 7 trigger-registration
destinations (`CTrigger`), 5 event-target destinations (`CEventTarget`), 2 string copies
(`CString`) and 1 static guard. The 50 duration groups without an omitted count share the
`CEventTarget` obstacle. Duration failure shapes are 50 omitted-count gaps, 28 flag-update limits,
24 consumption limits and 23 mixed scoped/literal-selection gaps. Duration lists stay 625 known,
516 partial and 1,029 unresolved. Reports are `.local/sdk-658/now-scoped.json` and
`.local/sdk-658/now-duration.json`.

## Stack duration keys and execute bodies (SDK-657)

The M451-hotfix population covers all 2,170 commands and 164 registries, with no failed question.
The [duration page](durations.md#stack-and-execute-facts-on-m451-hotfix-sdk-657) records the exact
member paths, consumers and remaining obstacles. Run `cargo run --release --example
duration-population` with `STELLARIS_PATH`;
`.local/sdk-657/rereview/duration-population.json` holds the current report;
`.local/sdk-657/rebased/duration-population.json` holds the comparison baseline.

| Measurement | SDK-657 before rebase | SDK-657 on `780c1e5` | SDK-657 on `9e2f544` |
| --- | ---: | ---: | ---: |
| Command groups | 52 | 52 | 52 |
| Complete / partial / failed groups | 28 / 24 / 0 | 0 / 52 / 0 | 0 / 52 / 0 |
| Established omitted counts or literals | 52 | 0 | 0 |
| Known / partial / unresolved lists | 627 / 514 / 1,029 | 627 / 514 / 1,029 | 625 / 516 / 1,029 |
| Unclassified command candidates | 0 | 0 | 0 |
| Unclassified registry candidates | 0 | 0 | 0 |
| Registry groups | 0 | 0 | 0 |
| Commands with uncovered unit-named keys | 26 | 26 | 26 |
| Explicit omitted-count gaps | 0 | 51 | 51 |
| Static flag-update-frequency limits | 28 | 27 | 27 |
| Consumption limits | 24 | 24 | 24 |
| Mixed scoped/literal-selection gaps | 21 | 21 | 23 |
| Initial shared-factor gaps | 0 | 1 | 1 |
| Unmatched execute bodies | 0 | 0 | 0 |

Failure shapes count groups and can overlap. The relation flag's `duration-initial-state` gap
also blocks its omitted count, independently of the 51 explicit omitted-count gaps. Its execute
match proves `FlagCountdown`; the timed trait's product reaches `CLeader::AddTimedTrait`, with
consumption outside the method. Their constructor bytes do not establish omitted counts.

The 21 stack groups cover 42 `months` and `years` keys. Their omitted literals are unresolved.
The shared selection layout leaves a possible overlap with scoped `days`, even when the
constructor does not establish a numeric subtype; `duration-scoped-literal` remains a conservative
gap. No subtype or initial value is restored from the old report. The other five uncovered
commands have `days` without a factor sibling. The two command candidates write presence bytes;
registry candidates reset words that proved 4-byte integer readers overwrite. None forms a group.
These results do not establish all of SDK-544 criterion 3.

Byte disjointness covers the span enclosing the scoped vtable, selection fields and literal,
using the established token conversion width. Missing subtype, selection or width evidence is
`duration-byte-storage`; overlap is `duration-byte-factor`. Prefix and continuation controls
cover every byte of the literal. An identity stack transfer cannot seed a duration group, but
can join a sibling's proved scale. Prefix word-reset controls reject narrow and scoped readers.

The tracked duration static expectations retain unresolved omitted counts for all covered
groups. The relation flag has an unresolved initial factor and combination, partial unit factors
and unresolved concrete scoped storage. Its live case has a partial empty duration list. The
full command-grammar candidate matches `tests/expected/m45/command-grammars.json`; no entry
changes.

The duration-list overlap proof compares byte ranges, including the upper word of a 64-bit
scoped literal. Unknown widths retain the gap. `add_modifier` and `add_stage_modifier` therefore
have partial duration lists: their other scoped operands have unproved literal widths. The
continuation walk invalidates bases written back by indexed accesses; stack-transfer controls
cover pre- and post-indexed loads and stores.
The nine `add_modifier` live cases retain the same values and diagnostics with partial lists.

### Shared reader join population

The stack join routes the original reader to a proved word-integer stack destination. The
non-duration answer changes cover `months` and `years` in these 20 effects and one trigger:

- `agreement_event`, `astral_rift_event`, `bypass_event`, `carrier_event`, `colony_event`,
  `cosmic_storm_event`, `cosmic_storm_influence_field_event`, `country_event`,
  `espionage_operation_event`, `first_contact_event`, `fleet_event`, `leader_event`,
  `observer_event`, `planet_event`, `pop_faction_event`, `pop_group_event`, `ship_event`,
  `situation_event`, `starbase_event`, `system_event`;
- `has_passed_resolution`.

Each of the 42 keys identifies integer reader `d7a95ab8c8d44489`, scalar form, no child members,
not-applicable child family, no scoped operand, and partial signed 32-bit conversion with scale 1,
decimal and radix-prefixed forms, and no explicit reader clamp. Repeat behavior, defaults and
domains remain unknown. Event fixed-key lists remain partial. The resolution trigger has a known
fixed-key list and empty child-family, numeric-key, ordering and target properties; its whole
answer remains partial.

The 20 effects share `CFireEventEffect::ReadMember`: calls `0x101d277b0` and `0x101d277cc` route
the original reader to `sp+0x1c0`. The resolution trigger uses `sp+8` at `0x102224768` and
`sp+0xc` at `0x102224794`. All call `CReader::Read(int&)` (`0x1025b6f08`), forwarding to
`CToken::ReadValue(int&) const` (`0x1025bdf4c`) with `%i`. Numeric properties describe this
reader's conversion, excluding caller post-processing. These calls have continuations, so no
final-storage or replace claim follows. This proof does not depend on constructor bytes.

The original non-duration comparisons are retained as
`.local/sdk-657/{scoped,numeric,registry,command}-{parent,current}.json`, with full diffs in
`registry-answer-diff.json` and `command-non-duration-diff.json`. They establish zero registry
answer changes and 21 command answer changes for the shared join on the pre-rebase constructor
baseline. The current scoped population is the [SDK-658](#owner-derivation-sdk-658) one.

## Float and short fixture storage (SDK-656)

On M451-hotfix (`29fa877366040a528098da39ec7e70b7baac76782a2a6bd161616d691f86fa38`),
the numeric population covers all 164 discovered registries: 184 partial numeric root fields,
no complete numeric answers and no failed queries. Float has three root fields in two registries;
short has seven in two registries. Fixture storage derives its decoder from the joined reader
callee and its owner-relative destination. Its token proof uses the selected path's singleton
domain, independently of the compiler's scratch registers. Float storage preserves binary32 bits;
short storage preserves 16 bits without a signed interpretation.

The registry-field population has 1,564 root fields and 38 exposed nested fields, with 8 complete,
156 partial and zero failed registry answers. Searching the field reader and every exposed read
alternative gives this bounded result:

| Shared reader | Root matches / 1,564 | Exposed nested matches / 38 |
| --- | ---: | ---: |
| `CReader::Read(signed char&)` | 0 | 0 |
| `CReader::Read(unsigned char&)` | 0 | 0 |
| `CReader::Read(unsigned short&)` | 0 | 0 |
| `CReader::Read(unsigned int&)` | 0 | 0 |
| `CReader::Read(long long&)` | 0 | 0 |
| `CReader::Read(unsigned long long&)` | 0 | 0 |

These six readers are unavailable to the initial-load fixture method within this population.
The search cannot reach 982 unresolved member descriptions; it does not establish universal
absence. No discovery repair is part of SDK-656. Parent SDK-544 criterion 4 stays open for these
readers until Jackson amends it. The separate inline project loader supplies template fixed-point
cases outside this registry population.

Reproduce the population with `STELLARIS_PATH` set:

```sh
cargo run --release --example numeric-population > .local/sdk-656/numeric-population.json
cargo run --release --example registry-field-sweep -- "$STELLARIS_PATH" > .local/sdk-656/registry-field-sweep.json
cargo test --release --lib -- --ignored numeric_fixture_storage_population --nocapture
```

Current exact-build reports are retained in `.local/sdk-656/`. Finite stored observations and
remaining criterion gaps are on [numeric conversion](numeric-conversion.md).

## Shared fixture-binding population on M451-hotfix (SDK-656)

The parent (`f2c6b56`) and current measurements cover 184 numeric root fields in 164 registries.
Counts are **decoder with loader / decoder without loader / no decoder**; Integer includes short.

| Reader kind | Parent | Current | Total |
| --- | ---: | ---: | ---: |
| Integer | 37 / 8 / 48 | 79 / 12 / 2 | 93 |
| FixedPoint | 28 / 9 / 51 | 79 / 8 / 1 | 88 |
| Float | 0 / 0 / 3 | 3 / 0 / 0 | 3 |

There are 99 storage gains: 39 int, 50 fixed-point, seven short and three float fields.
Seven registries gain a loader. Zero bindings or loaders are lost; all 82 previous bindings
retain their decoder and owner offset.

The token proof comes from the unconditional singleton path. `x8` can hold a jump-table index,
an owner address or a reused scratch value. The joined callee, original reader and owner-relative
destination establish the storage binding.

A loader uses its own template specialization, `true` in these seven registries. Its
`LoadFromReader` reaches the owner constructor through the matching `ReadNewEntry`.
The loader's reader-return boundary also requires `mov x0, sp` followed by the
`CReader::~CReader()` cleanup call.

`common/ship_sizes/max_speed` transforms its stored value after the reader; its final value
is not a shared-reader rule.

The retained comparison is `.local/sdk-656/fixture-binding-diff.json`. Logs in the same directory
are `fixture-storage-population-parent.log`, `fixture-storage-population-parent-float.log` and
`fixture-storage-population-review-proof.log`; the merged parent report is
`fixture-storage-population-parent.json`. The parent float supplement covers the three fields
excluded by its original integer/fixed-point test filter. Detailed joins and addresses stay in
these retained reports; fixture-binding pitfalls are on [early observations](early-observations.md).
