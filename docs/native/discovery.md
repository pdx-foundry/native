# Discovery methods

Each row is one method source stamp; the two callback operations share a method. Module paths are
relative to `src/engine/analysis/` unless a full `src/` path is shown. The table names the method
owners, not every shared decoder or evaluator that they use. It covers the static methods and
their live loaded-modifier join; fixture observations are in
[early observations](early-observations.md). The knowledge page of each subject holds its engine
facts, current result, gaps and pitfalls; the [engine knowledge index](../engine-knowledge.md)
lists every page. Use the [method-authoring guide](method-authoring.md) to explore, implement and
check a method in one task.

| Operation | Source stamp | Modules | Knowledge section |
| --- | --- | --- | --- |
| `Native::registries` | `registry-directories/v6` | `discovery.rs`, `discovery/schedule.rs`, `directories.rs` | [Registry candidates and owner joins](registry-fields.md#registry-scheduling-and-owner-joins) |
| `Native::registry_fields` | `registry-fields/v38` | `fields.rs`, `fields/control_flow.rs`, `fields/dispatch.rs`, `fields/inventory.rs`, `fields/nested.rs`, `fields/persistent.rs`, `fields/uses.rs`, `fields/records.rs`, `fields/tokens.rs`, `readers.rs`, `src/binding/binary/fields.rs`, `src/session/loader_kinds.rs` | [Field sweep and stops](registry-fields.md#current-sweep), [event kinds](registry-fields.md#event-kinds) |
| `Field.reference` in `registry_fields` and `command_grammar` | `registry-fields/v38`, `command-grammar/v19` | `references.rs`, `references/initialization.rs`, `references/shapes.rs`, `src/binding/binary/references.rs` | [References and dynamic names](references.md) |
| `Reader.numeric` in fields and command grammar | `registry-fields/v38`, `command-grammar/v19` | `numeric.rs`, `numeric/lexer.rs`, `numeric/modifier.rs`, `src/binding/binary/numeric.rs` | [Numeric conversion](numeric-conversion.md), [text lexer](text-lexer.md) |
| `Reader.scoped_operand` in fields and command grammar | `registry-fields/v38`, `command-grammar/v19` | `scoped_numeric.rs`, `src/binding/binary/scoped_numeric.rs`, `src/session/scoped_numeric.rs` | [Scoped numeric](scoped-numeric.md) |
| `Field.entry_contexts` in `registry_fields` | `registry-fields/v38` | `callbacks/blocks.rs`, `callbacks/climb.rs`, `callbacks/contexts.rs`, `src/binding/binary/callbacks.rs`, `src/binding/binary/type_pointers.rs`, `src/session/field_entries.rs` | [Block entry contexts](registry-fields.md#block-entry-contexts) |
| `FieldMembers::ModifierBlock` in `registry_fields` | `registry-fields/v38` | `modifier_blocks.rs`, `modifier_blocks/name.rs`, `modifier_blocks/reference.rs`, `fields/member.rs`, `src/binding/binary/modifier_blocks.rs`, `src/session/modifier_blocks.rs` | [Modifier blocks](modifier-blocks.md) |
| `FieldMembers::WeightBlock` in `registry_fields` | `registry-fields/v38` | `weight_blocks.rs`, `fields/persistent.rs`, `src/binding/binary/weight_blocks.rs`, `src/session/weight_blocks.rs` | [Weight blocks](weight-blocks.md) |
| `FieldMembers::TriggeredModifier` in `registry_fields` | `registry-fields/v38` | `modifier_blocks/triggered.rs`, `fields/nested.rs`, `src/binding/binary/fields.rs`, `src/binding/binary/triggered_modifiers.rs`, `src/session/triggered_modifiers.rs` | [Triggered modifiers](triggered-modifiers.md) |
| `CommandGrammar.durations` | `command-grammar/v19` | `durations.rs`, `src/binding/binary/durations.rs`, `src/session/durations.rs` | [Duration keys](durations.md) |
| `Native::dynamic_names` | `dynamic-names/v3` | `dynamic_names.rs`, `dynamic_names/routes.rs`, `declarations/receiver.rs` | [Dynamic names](references.md#dynamic-names) |
| `Native::command_grammar` | `command-grammar/v19` | `grammar.rs`, `grammar/coverage.rs`, `grammar/forms.rs`, `grammar/numeric.rs`, `grammar/ordering.rs`, `grammar/targets.rs`, `declarations/receiver.rs` | [Nested command grammar](command-grammar.md) |
| `Native::declarations` | `command-declarations/v3` | `declarations.rs`, `declarations/composition.rs` | [Effects and triggers](engine-commands.md#effects-and-triggers) |
| `Native::modifiers` | `modifier-declarations/v1` | `modifiers.rs` | [Modifiers](engine-commands.md#modifiers) |
| `Native::modifier_categories` | `modifier-categories/v1` | `modifiers.rs` | [Categories](engine-commands.md#categories) |
| `Native::scopes` | `scope-declarations/v1` | `scopes.rs` | [Scope types](engine-commands.md#scope-types) |
| `Native::scope_links` | `scope-links/v2` | `scopes.rs` | [Scope links](engine-commands.md#scope-links) |
| `Native::localization_declarations` | `localization-declarations/v1` | `localization.rs` | [Localization contexts, commands and links](engine-commands.md#localization-contexts-commands-and-links) |
| `Native::on_actions`, `Native::game_rules` | `callbacks/v5` | `callbacks.rs`, `callbacks/names.rs`, `callbacks/contexts.rs`, `callbacks/climb.rs`, `src/binding/binary/callbacks.rs` | [On_actions, game rules and entry scopes](engine-commands.md#on_actions-game-rules-and-their-entry-scopes) |
| `Native::defines` | `defines/v1` | `defines.rs` | [Defines](engine-commands.md#defines) |
| `Native::modifier_families` | `modifier-families/v4` | `families.rs`, `families/joins.rs`, `families/loading.rs`, `strings.rs` | [Generation calls and roots](modifier-families.md#engine-code) |
| `Field.accepted_categories` in `registry_fields` | `registry-fields/v38` | `fields/containers.rs`, `fields/persistent.rs`, `fields/nested.rs`, `src/binding/binary/receivers.rs`, `src/session/container_masks.rs` | [Modifier masks](modifier-masks.md#container-masks) |
| `Native::modifier_category_keys` | `modifier-category-keys/v1` | `category_keys.rs`, `src/binding/binary/language.rs`, `src/session/language.rs` | [Script category keys](modifier-masks.md#script-category-keys) |
| `Native::modifier_nodes` | `modifier-nodes/v1` | `modifier_nodes.rs`, `src/binding/binary/modifier_nodes.rs`, `src/session/modifier_nodes.rs` | [Modifier masks](modifier-masks.md) |
| `Native::script_expansions` | `script-expansions/v1` | `expansions.rs`, `directories.rs`, `src/binding/binary/expansions.rs`, `src/session/expansions.rs` | [Script expansion](script-expansion.md) |
| `Native::loader_rules` | `loader-rules/v5` | `loader_rules.rs`, `directories.rs`, `src/binding/binary/definition_names.rs`, `src/binding/binary/references.rs`, `src/binding/analysis.rs`, `src/session/loader_rules.rs` | [Loader rules](loader-rules.md) |
| `Native::derived_names` | `derived-names/v4` | `names.rs`, `strings.rs`, `src/binding/binary/names.rs`, `src/session/names.rs` | [Derived names](derived-names.md) |
| `Game::loaded_modifiers` | `loaded-modifiers/v1` | `modifier_table.rs`, `src/engine/operations/loaded_modifiers.rs`, `src/session/loaded_modifiers.rs` | [Loaded modifier table](modifier-families.md#the-loaded-modifier-table) |

## Registry sweep (SDK-553)

The Milestone 5 sweep ran the method set unchanged over every listed registry: one live run on
M452 at Native `ee01bf338872290eaaab2ca3c0503fc33244686e`, the commit that Atlas pinned. The
stamps in the table above are that method set; Native stamps no commit on an answer. Atlas made
the run with `record_answers_to` and owns the coverage and the config comparison
([Atlas capture](https://github.com/pdx-foundry/atlas/blob/95e260b9266550994cda441b4939afdbb448cd45/docs/coverage/milestone-5.md),
SDK-798). The recording stays on the capture host; its identity and the commands that repeat the
report are in the capture.

The [sweep report](method-authoring.md#report-the-registry-sweep) over the recording gives the
tracked baseline `tests/population/m452/registry-sweep.json` (each registry's statuses) and these
rates, over 185 registries and 1,855 root fields. Every request has an answer, partial with typed
gaps, or `Unsupported`; none fails, is missing or names an unknown registry.

| Relationship | Complete | Denominator | Rate |
| --- | ---: | ---: | ---: |
| Fields | 105 | 185 | 56.76% |
| Reader kinds | 1,512 | 1,855 | 81.51% |
| Shapes and conditions | 1,349 | 1,855 | 72.72% |
| References | 24 | 336 | 7.14% |
| Numerics | 216 | 517 | 41.78% |
| Weights | 96 | 471 | 20.38% |
| Naming rules | 0 | 185 | 0.00% |
| Scope context | 1,261 | 1,855 | 67.98% |
| **All units** | **4,563** | **7,259** | **62.86%** |

The rates are those of the tracked baseline after SDK-676 and SDK-812 (scope context 1,247 and
all units 4,549 before SDK-812). Against the baseline before SDK-676 (4,483
of 7,268 units, 61.68%; 4,283 open gaps), SDK-676 joined 19 root fields: nine weights complete
their weight unit, and nine modifier fields, which an unknown family had left undetermined for
weights, leave the weight denominator. It removed 43 of the 45 container gaps: 4,243 open gaps,
and SDK-739 accepts 48 more weight-key ranges (687 accepted in all). No naming rule is complete because every `modifier_families` answer keeps
one `UnnamedDeclaration` gap. The rates leave out [accepted limits](#accepted-limits-sdk-819); the
SDK-553 run counted them, with numerics at 206 (34.45%), weights at 0 and all units at 3,985
(53.06%).

**Failure shapes.** The 6,397 gaps of the SDK-553 run that are not `OutsideMethod`, by the
ticket that owns them. The sweep repaired none. SDK-809 and SDK-811 are now
[accepted limits](#accepted-limits-sdk-819), not open work, and so is part of SDK-812.

| Owner | Gaps | Registries | Shapes |
| --- | ---: | ---: | --- |
| SDK-627 | 1,615 | 119 | Repeat behavior (`UnresolvedStorage`) |
| SDK-808 | 1,110 | 121 | Read scope: scope argument, stored scope, zero mask |
| SDK-710 | 1,080 | 150 | No shared reader, reader paths that stop, unnamed keys, unreadable input |
| SDK-807 | 672 | 3 | Reader and read scope of the pre_trigger keys |
| SDK-809 | 387 | 59 | Numeric conversion: accepted |
| SDK-707 | 328 | 116 | `derived_names` paths that stop; [interface callers](derived-names.md#interface-callers-of-name-getters-m452) followed |
| SDK-810 | 326 | 58 | `derived_names` conditions, misses and every-item initialization |
| SDK-713 | 253 | 179 | `modifier_families` unjoined sites and generation conditions: [repaired in part](modifier-families.md#conditions) |
| SDK-811 | 252 | 45 | Loader conditions and use-time selections (`UnresolvedCondition`): accepted |
| SDK-812 | 242 | 66 | Block entry-context stops: of the 248 on `main` `a093883`, 15 repaired, 138 accepted, 95 open on blocks with a claim ([record](registry-fields.md#block-entry-context-stops-on-m452-sdk-812)) |
| SDK-676 | 45 | 14 | Modifier containers of fields with no modifier reader: 2 remain ([nested owners and unjoined fields](modifier-blocks.md#nested-owners-sdk-676)) |
| SDK-741 | 42 | 4 | `description_parameters` |
| SDK-817 | 20 | 1 | Tooltip triggered modifiers of `common/pop_jobs` |
| SDK-816 | 14 | 1 | Container masks that no construction reaches (`common/traits`) |
| SDK-635 | 11 | 6 | Reference lookups |

SDK-707 (2026-10-13) follows the interface callers of item getters. A static recording on M452
gives `derived_names` 346 `UnresolvedPath` gaps in 117 registries (SDK-707) and 300 other gaps in
56 (SDK-810, after SDK-713's every-item change). The callers add 18 path gaps (caller paths that
stop, unfollowed caller names, by-value results that are not stack slots, getters with no path to
their return), and their six new sprite names add 12 condition and miss gaps. One
`derived_names` answer, `common/starbase_levels`, becomes partial (a `getter-path` gap); no
relationship status changes. The
[string fields with a lookup claim](derived-names.md#string-fields-with-a-lookup-claim) keep the
internal path shapes of their registries.

### Accepted limits (SDK-819)

An accepted limit is a gap that the method could close but Atlas does not need. Native still
reports it; the [sweep report](method-authoring.md#accepted-limits) counts it under
`failure_shapes.accepted`, by decision and shape, and leaves it out of the open shapes, the owner
table and the rates. On M452 at `main` `34bd898`:

| Decision | Gaps | Registries | Shapes |
| --- | ---: | ---: | --- |
| SDK-739, [token value methods](numeric-conversion.md#token-value-methods) | 346 | 55 | The range of the weight keys `days`, `months`, `years` and `factor` |
| SDK-809, [numeric accepted limit](numeric-conversion.md#accepted-limit-the-sweeps-numeric-gaps-sdk-809) | 41 | 5 | The range and binary32 bounds of `float` fields; the storage of `short` fields |
| SDK-811, [loader conditions](registry-fields.md#unresolved-loader-conditions-on-m452-sdk-811) | 252 | 45 | 207 loader conditions of listed fields; 45 use-time selections |
| SDK-812, [entry-context stops](registry-fields.md#block-entry-context-stops-on-m452-sdk-812) | 138 | 46 | Entry-context stops of the 93 listed fields with no `replace_scopes` claim; 9 unnamed direct evaluations |

Before and after SDK-819, over the same recording:

| Count | Before | After |
| --- | ---: | ---: |
| Open gaps (not `OutsideMethod`, not accepted) | 5,389 | 4,750 |
| Open `NumericConversion` gaps | 387 | 0 |
| Open `UnresolvedCondition` gaps | 578 | 326 |
| Numerics | 206 / 598 | 216 / 598 |
| Weights | 0 / 561 | 87 / 561 |
| All units | 3,985 / 7,511 | 4,082 / 7,511 |

The 10 new numeric fields are the seven `short` fields and three `float` root fields. The other 382
stay undetermined, because their reader is unknown (SDK-710). The 87 weight fields have no open
gap other than a weight key's range. The 326 open `UnresolvedCondition` gaps are `derived_names`
conditions (SDK-810). The 5,389 are 1,008 fewer than the 6,397 of the SDK-553 run because SDK-807
established the reader of the pre_trigger keys (336 each of `UnresolvedPath`, `UnresolvedReader`
and `UnresolvedStorage`); no status changed (`--diff` against the tracked baseline names no
registry).
`registry-sweep --diff` from the before report names 179 registries: each gains its gaps by shape,
55 change their weight counts and 4 their numeric counts.

SDK-713 repaired the every-item question that `modifier_families` and `derived_names` share, and
the conditions of 13 families that config templates use. Over a new recording at its merge with
`main` `09e2321`, the open gaps went from 4,750 to 4,699: `modifier_families` `UnresolvedPath`
from 74 to 61 and `derived_names` `UnresolvedCondition` from 326 to 288 (the every-item gap from 36
registries to 4, and six name conditions that it blocked). No relationship status or rate changed;
`--diff` against the tracked baseline names the 36 registries whose gaps changed
([modifier families](modifier-families.md#conditions)).

SDK-710 followed Boolean stack temporaries into their member bits and joined the
`CReader::ReadUniform<T>` list reader ([fields with no shared
reader](registry-fields.md#fields-with-no-shared-reader)). Over a new recording at its merge with
`main` `1f876e4`, the open gaps went from 4,729 to 4,283 (`registry_fields` from 3,843 to 3,397).
Reader kinds went from 1,412 to 1,493 of 1,855 (76.12% to 80.49%), shapes and conditions from
1,093 to 1,331 (58.92% to 71.75%) and scope context from 1,152 to 1,234 (62.10% to 66.52%). The
numeric, reference and weight denominators lost 81 undetermined fields each, because those
fields now have a reader kind. `--diff` against the tracked baseline names 50 registries.

SDK-813 answered `common/planet_classes`, whose loader builds a planet class or, for the root key
`random_list`, a random list ([root-key owners](registry-fields.md#root-key-owners-sdk-813)), and
gave the other five `Unsupported` registries a narrower reason. Its root fields are the 70 fields
of a planet class and `random_list`, whose members are the random list's fields. Over a new
recording at its merge with `main` `a093883`, `--diff` against the tracked baseline names
`common/planet_classes` and the four whose reason changed (`prescripted_countries` keeps its own):

| Count | Before | After |
| --- | ---: | ---: |
| Fields: partial / unsupported | 74 / 6 | 75 / 5 |
| Reader kinds | 1,512 / 1,855 | 1,574 / 1,926 |
| Shapes and conditions | 1,349 / 1,855 | 1,376 / 1,926 |
| References | 24 / 336 | 25 / 345 |
| Numerics | 216 / 517 | 233 / 542 |
| Weights | 96 / 471 | 96 / 480 |
| Scope context | 1,247 / 1,855 | 1,309 / 1,926 |
| **All units** | **4,549 / 7,259 (62.67%)** | **4,718 / 7,515 (62.78%)** |
| Open gaps | 4,243 | 4,328 |
| Accepted gaps | 687 | 695 |

The 85 new open gaps and 8 accepted ones are the planet classes' own; one says that the read
scope of `random_list` is not established. One composition site that was reached only from
planet-class code is now joined, so every `modifier_families` answer counts 17 unjoined sites, not
18.

To split the numeric gaps safely, the gap detail names the obstacle: the range, a storage
property, or the conversion path ([answer rule](numeric-conversion.md#answer-rule)). A
conversion-path gap is never accepted. The loader-condition limit lists its 207 fields by registry,
so a loader-condition gap of another field stays open.

SDK-812 (over a new recording at `a093883` plus its repairs) repairs 15 of the 248 entry-context
gaps, accepts 138 and leaves 95 open; open gaps go from 4,243 to 4,088 (two `derived_names` path
gaps of districts and specimens also close with the new `csetm` and switch-table rules), scope
context from 1,247 to 1,261 and all units from 4,549 to 4,563. `--diff` against the baseline names
49 registries: 7 change from the repairs and 46 move gaps to the limit.

Five registries are `Unsupported` for all three registry questions, because their loader builds
no item class or two databases load the directory; each has its reason ([root-key
owners](registry-fields.md#root-key-owners-sdk-813)).
The Atlas `events/` exceptions with no Native shape are SDK-814 (`namespace`) and SDK-815 (Atlas:
the type-wide `event` block). The run proposes no manual exception.

## Read-time block scopes (SDK-549)

`Field.read_scope` and `CommandGrammar.child_scopes` supply read-time `this`. Atlas uses them
for `replace_scopes.this` and command `push_scope`; evaluation `entry_contexts` stays separate
under the [consumer rule](../specs/native.md). The method lives in `grammar/read_scope.rs`,
`fields/member.rs` and `src/session/read_scope.rs`.

**Assumption: outer `Read` passes its incoming scope unchanged to `ReadMember`.** On M451-hotfix
(`29fa877366040a528098da39ec7e70b7baac76782a2a6bd161616d691f86fa38`), hand-read samples show
`CEffect::Read` and `CTrigger::Read` keeping the `x2` parameter in `x20` and passing it in `x3`
to member slot `+0x18`. The root trigger/effect template readers pass the registry's scope to
virtual `Read`. Member instructions still determine whether each child inherits that scope or
receives an explicit set; no target-getter mask or config value produces an answer.

**Checks.** Parity covers `if`, `else`, `else_if`, `hidden_effect`, `and`, `or` and `not`, plus
scope-changing army readers. Independent expectations from cwtools-stellaris-config revision
`85747602a614ad7daa8cc66453777ecb023463a8` cover the three effect conditionals without scope
replacement, `any_owned_army` and `count_owned_army.limit` with `push_scope = army`, and tradition
`possible` and `on_enabled` with `replace_scopes.this = country`. Scope IDs also join to link
outputs. A contradictory reader requires a hand-read and an explicit unresolved result or a
narrow repair before support is claimed; it is not a reason to silently extend the assumption.
Seven config expectations agree (the three conditionals and four explicit scope assignments).
One additional comparison disagrees: config `any_owned_planet` says `planet`, but its member
reader explicitly passes mask `0x10000000000`, which names `colony`. Keep the engine result.
This member-level replacement does not contradict the outer-reader assumption.

**Limits.** Multiple reader paths retain separate scope sets. Zero masks, unnamed bits, missing
boundaries and memory-loaded scopes remain gaps, including `every_owned_army`: its factory
stores the army mask, but the method does not establish that value at member entry. Nested fields
use the same rules and report gaps at their full key path. The removed strict forwarding proof
and its string-helper findings are preserved in `.local/sdk-549/implementation/strict-forwarding-retained`.
An unfinished member walk also keeps each established family's scope alternatives partial.

The focused file-load fixture confirms the engine-selected country scope for a tradition trigger
and effect. Inline fixture validation also passes. File-load diagnostics use the bound log hooks
without waiting for post-load validation (`observe-fixture/v7`). The broader control fixtures
captured country diagnostics but did not close their post-load window on this installation.

**Full inventory, M451-hotfix.** Counts describe the whole public answer, including other grammar
properties. Failed command cases lack a receiver join; their public answers remain partial.

| Inventory | Total | Complete | Partial | Failed |
| --- | ---: | ---: | ---: | ---: |
| Registries | 164 | 8 | 156 | 0 |
| Effects | 1,074 | 146 | 925 | 3 |
| Triggers | 1,096 | 119 | 977 | 0 |

The 1,593 registry fields have scope gaps for unknown arguments and zero masks. Command scope
gaps are unknown arguments, memory-loaded scopes and missing child-family boundaries. The three
failed receiver joins comprise two `factory-terminal` and one unsupported `instruction`; template
constructor summaries (SDK-673) joined the six former `command-vtable` cases. Other grammar gaps are described on [command grammar](command-grammar.md) and
[registry fields](registry-fields.md); the full case lists remain in `.local/sdk-549/implementation`.

## Numeric scanner boundary (SDK-720)

The imported scanner's own conversion and the fixed-point binary input path are `OutsideMethod`
boundaries; only typed limitations make a numeric answer partial
([answer rule](numeric-conversion.md#answer-rule)). The two numeric reporters now classify each
reader by its typed gaps, not by the `Partial` wrapper of `Reader.numeric`. All counts are M452.

| Population | Total | Complete | Partial | Failed |
| --- | ---: | ---: | ---: | ---: |
| `numeric-population`, numeric root fields | 188 | 178 | 10 | 0 |
| `scoped-numeric-population`, registry destinations | 1,932 | 1,922 | 0 | 10 |
| `scoped-numeric-population`, command arguments | 302 | 302 | 0 | 0 |
| `registry-field-sweep`, registries | 164 | 12 | 152 | 0 |
| `command-population`, effects | 1,080 | 163 | 915 | 2 |
| `command-population`, triggers | 1,098 | 122 | 976 | 0 |

Before this change every numeric reader was partial. The field sweep gained
`common/asteroid_belts`, `common/leader_tiers` and `common/resource_regions`; 14 effects and 3
triggers became complete. No answer value changed; only gaps, completeness and source stamps.

Failure shapes, by gap count in the field sweep (and in the command population):

| Shape | Before | After |
| --- | ---: | ---: |
| `NumericConversion`: the old general numeric detail | 1,384 (317) | 0 |
| `NumericConversion`: `Scoped literal conversion boundaries and overflow are incomplete.` | 1,922 (302) | 0 |
| `NumericConversion`: `Numeric conversion is incomplete: a storage property, the accepted range or an engine conversion path is not established.` | 0 | 300 (6) |
| `OutsideMethod`: the platform scanner | 0 | 3,087 (619) |
| `OutsideMethod`: the binary-lexer input path | 0 | 903 (139) |

The triggered-modifier `other_keys` details carry the same change under their prefix (45 each).
Of the 300 remaining typed gaps, 290 are weight-block keys: `days`, `months` and `years` (73 each)
use `CToken::GetInt()`, and one `factor` alternative (71) uses `CToken::GetFloat()`. The other 10
are the seven short and three float fields. SDK-819 split that detail by obstacle: range, storage or conversion path.

## Token value methods (SDK-739)

`CToken::GetInt()` and `CToken::GetFloat()` now have numeric facts
([token value methods](numeric-conversion.md#token-value-methods)): signed 32-bit at scale 1, and
signed 64-bit at scale 100000. Their `accepted_range` stays `Unresolved`, so the 290 weight-key
gaps keep the typed `NumericConversion` gap, now for the range only. M452:

| Population | Change |
| --- | --- |
| `numeric-population`, numeric root fields | none: 188, 178 complete, 10 partial, 0 failed |
| `registry-field-sweep`, registries | none: 164, 12 complete, 152 partial, 0 failed; 47 registries gain facts and gaps |
| `command-population` | none |

In the field sweep, 290 weight-key readers have `Reader.numeric` facts; the 219 `GetInt()` keys add
the scanner `OutsideMethod` gap (3,087 to 3,306). The `factor` key already had it from its
`CReader::Read(CFixedPoint&)` alternative. Typed gaps stay at 300 and 3. The council agenda gate
accepts the four range gaps of `ai_weight` ([registry fields](registry-fields.md)).

## Repeat behavior (SDK-724)

`FieldShape.repeat` comes from one rule for each shared reader, and the storage check runs on the
assembled answer ([repeat behavior](registry-fields.md#repeat-behavior)).

**Full inventory, M452.** `registry-field-sweep` at `registry-fields/v24`, all 164 registries:
**9 complete, 155 partial, 0 failed** (`common/colony_automation_categories` became complete).
Nested fields are the members of `FieldMembers::Fields`, modifier blocks and triggered clauses.

| Fields | Replace | Accumulate | Merges | Unknown |
| --- | ---: | ---: | ---: | ---: |
| Root, before | 556 | 57 | — | 980 |
| Root, after | 799 | 57 | 114 | 623 |
| Nested, before | 730 | 0 | — | 559 |
| Nested, after | 737 | 0 | 145 | 407 |

`UnresolvedStorage` gaps fell from 1,053 to 639. Failure shapes, by gap count:

| Shape | Gaps |
| --- | ---: |
| `Repeat behavior and nested fields remain unresolved.`: a field with no single established reader or an unclassified block | 604 |
| `Repeat behavior remains unresolved.`: a known family with a read that is not a tail call or alternatives that disagree, such as `overlord_weight` in `common/agreement_presets` | 35 |
| `Scoped destination vtable is not established.` (unchanged, [scoped numeric](scoped-numeric.md)) | 10 |

## Weight blocks (SDK-545, SDK-705)

`FieldMembers::WeightBlock` gives the grammar of the shared mean-time reader and of its
`modifier`, `scaled_modifier` and `complex_trigger_modifier` entries. The method runs the member
reader for every token value, as the scope methods do, and runs a key again for every value token
when the stored value depends on it (a keyword). It reads the scope that the owner constructor
stores in the weight object; the [weight blocks](weight-blocks.md) page has the engine facts, the
full failure shapes and the pitfalls.

**Full inventory, M451-hotfix.** The inventory is every root field with a constructor-proven weight
reader, in all 164 registries.

| Inventory | Total | Complete | Partial | Failed |
| --- | ---: | ---: | ---: | ---: |
| Weight fields | 68 | 0 | 68 | 0 |

Two reader identities cover them: 67 fields share `f08cb83d92484a89`, and
`common/country_customization.weight` uses the `CAIMTTHChance` variant `fd8c6ad9ff94a8f2`. Every
key of the two entry grammars has an established reader or a narrow gap. Every field is partial for
the same shapes: conversion limits of numeric keys and scoped operands, the zero-mask read scopes
of the two entries and of `limit` and `potential`, the keyword domains of `calc` and `mode`, the
`trigger` lookup facts and `parameters` (read by the trigger that `trigger` names). Fifteen weight-named persistent blocks have no constructor-proven
reader, so the method does not reach them. The stored-scope join resolves the read scope of every
weight field whose constructors agree; it does not change the registry counts above.

**Entry and modifier gaps (SDK-722), M452.** `registry-field-sweep` at `registry-fields/v25`, all
164 registries: **9 complete, 155 partial, 0 failed**, the same as before; 55 registries changed
answers. The 73 weight fields stay partial for numeric, keyword and entry-context gaps. Each now
has four `OutsideMethod` zero-mask limits (292, D5a), one for the `trigger` key match (73) and one
for `parameters` (73, D3); the `ReaderSemantics` trigger gap and the `UnresolvedReader`
`parameters` gap are gone (73 each). The nine stored-zero root fields keep `UnresolvedPath
read-scope: zero-mask`. Modifier-block fields report `read_scope: Known([])` (D4), which removes
82 `read-scope: scope-argument` gaps (745 to 663). The reference census is unchanged at 188
readers (161 complete, 27 failed), and `trigger_lookup` is established.

**Keyword domains (SDK-725), M452.** `registry-field-sweep` at `registry-fields/v28`: no registry
changes status; the 47 registries with weight fields change answers. All 73 weight fields give
`calc` four names and `mode` thirteen, and their 146 `ReaderSemantics` keyword gaps are gone. The
fields stay partial for numeric and other gaps. `mode` excludes the six operand-free operations,
which `InitPostRead` reports and replaces ([keyword domains](weight-blocks.md#keyword-domains)).

## Modifier fixed-key readers (SDK-675)

The shared modifier walk reads `name`, `key` and `divide_over_pop_groups`
([fixed-key readers](modifier-blocks.md#fixed-key-readers-sdk-675)). `description_parameters` is
split to SDK-741. Two of the repairs are in the shared dispatch walker, so other fields and
commands change too: `CReader::Read(bool&)` joins a stack temporary, and `CReader::SkipToken()`
joins with the reader alone. M452, `registry-fields/v31`:

| Population | Before | After |
| --- | --- | --- |
| `registry-field-sweep`, registries | 164: 12 complete, 152 partial, 0 failed | 164: 13 complete, 151 partial, 0 failed |
| Modifier-block fields | 37: 0 complete, 37 partial | 37: 30 complete, 7 partial |
| Triggered modifier fields | 50 partial | 50 partial |
| `command-population` | — | no status change; 12 effects change answers |

Forty-one registries change answers. `common/cloaking_strength_levels` becomes complete. The
changed reader kinds, with nested modifier keys counted at each use:

| Change | Readers |
| --- | ---: |
| `name`: `Unknown` to `Skipped` | 127 |
| `key`: `Unknown` to `String` | 122 |
| `divide_over_pop_groups`: `Unknown` to `Boolean` | 122 |
| Other bit-field flags: `Unknown` to `Boolean` | 213 |

All four variants report `name` as `Skipped`. The Static and CustomDescription variants, and the
triggered clauses that embed them, report `key` and `divide_over_pop_groups`. The 213 other flags are root fields
such as `common/buildings.can_be_ruined`, `common/country_types.is_mercenary` and the 19
megastructure flags. All 171 that the cwtools config names are `bool` or `= yes` there. In the
command population, 11 effects gain Boolean keys (`add_trait.show_message`,
`create_leader.immortal`, `transfer_carrier.merge_pops`, the nested `settings` keys of the three
fleet effects); `spawn_custom_debris` gains five named Boolean keys that it did not report before.
No other field or command joins through the setter rule or `SkipToken`.

The seven partial modifier fields are the CustomDescription variant (`1c2988588f7e8eaa`); each
keeps the `description_parameters` gaps (14 `instruction`, 14 `reader-routing`). The triggered
clauses stay partial for their read-scope gaps.

## Triggered modifier clauses (SDK-673)

`FieldMembers::TriggeredModifier` gives the clause keys of `CTriggeredModifierBase<T>` and joins
its nested `modifier` block and its direct entries to the shared modifier grammar. The field join
extends the nested-object proof to template constructors, `PdxMakeScopedPtr` factories and
scoped-pointer insertions whose move the binding proves; the [triggered
modifiers](triggered-modifiers.md) page has the engine facts, failure shapes and pitfalls.

**Full inventory, M451-hotfix.** The inventory is every root field whose collected object has a
clause reader, in all 164 registries.

| Inventory | Total | Complete | Partial | Failed |
| --- | ---: | ---: | ---: | ---: |
| Triggered modifier fields | 50 | 0 | 50 | 0 |

Three reader identities cover them: 43 Static fields share `88f77b58ddf77488`, five job fields use
the Tooltip variant `d3a866b65e16b749`, and traditions and ascension perks use the CustomDesc
variant `f9fb8c8f5f2ca714`. Static and CustomDesc join `modifier` and `other_keys` to the SDK-607
identities `ba5f8cddeba0d833` and `1c2988588f7e8eaa`. Every field is partial for the inherited
modifier-block reader and numeric gaps, the scoped-operand gaps of `mult` and `multiplier`, the
read scopes (SDK-549); the Tooltip variant also lacks its embedded
join (a branch-island base constructor). The nested `tradition_swap.triggered_modifier` uses in
traditions and ascension perks join the CustomDesc clause ([nested
owners](modifier-blocks.md#nested-owners-sdk-676)).

The shared fixes change other answers on the same build:

- **Command receivers.** Template constructors now have summaries, so `pop_change_ethic`,
  `pop_force_add_ethic`, `remove_random_starbase_building`, `remove_random_starbase_module`,
  `switch` and `inverted_switch` join their receiver and report child grammars. The dynamic-name
  method examines them; its unresolved-reader gaps fall from 12 to 6.
- **`common/technology`.** Its root `ReadMember` constructs another `CTechnology`, and the object
  collector used to add the root's body a second time, which made the root ambiguous. The collector
  now adds each body once, so the registry reports 29 fields, including two modifier and three
  weight fields.
- **`common/agreement_term_values.triggered_desc`.** The object is built by a factory, so the field
  now reports its child fields.

## Derived names (SDK-546)

`Native::derived_names` gives the names that a registry's own `const` members and post-read
initialization compose from the item key or a string field and then check or look up in the
localisation keys, the sprites or the files. Search runs with unknown item memory establish the
key-only names and their conditions; template runs with planted field text recover field parts.
The [derived names](derived-names.md) page has the engine facts, the full gap shapes, the config
agreement and the pitfalls.

**Full inventory, M451-hotfix.** The inventory is every registry from `Native::registries()`.

| Inventory | Total | Complete | Partial | Failed |
| --- | ---: | ---: | ---: | ---: |
| Registries | 164 | 62 | 102 | 0 |

Eighty-five registries return names, and all of them are partial; the complete answers return
no name. The [derived names](derived-names.md#population) page has the gap shapes, the entry
counts and the findings.
