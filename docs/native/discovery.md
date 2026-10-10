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
| `Native::registries` | `registry-directories/v3` | `discovery.rs`, `directories.rs` | [Registry candidates and owner joins](registry-fields.md#registry-scheduling-and-owner-joins) |
| `Native::registry_fields` | `registry-fields/v25` | `fields.rs`, `fields/control_flow.rs`, `fields/dispatch.rs`, `fields/inventory.rs`, `fields/nested.rs`, `fields/persistent.rs`, `fields/uses.rs`, `fields/records.rs`, `fields/tokens.rs`, `readers.rs` | [Field sweep and stops](registry-fields.md#current-m452-sweep) |
| `Field.reference` in `registry_fields` and `command_grammar` | `registry-fields/v25`, `command-grammar/v16` | `references.rs`, `references/initialization.rs`, `references/shapes.rs`, `src/binding/binary/references.rs` | [References and dynamic names](references.md) |
| `Reader.numeric` in fields and command grammar | `registry-fields/v25`, `command-grammar/v16` | `numeric.rs`, `numeric/modifier.rs`, `src/binding/binary/numeric.rs` | [Numeric conversion](numeric-conversion.md) |
| `Reader.scoped_operand` in fields and command grammar | `registry-fields/v25`, `command-grammar/v16` | `scoped_numeric.rs`, `src/binding/binary/scoped_numeric.rs`, `src/session/scoped_numeric.rs` | [Scoped numeric](scoped-numeric.md) |
| `Field.entry_contexts` in `registry_fields` | `registry-fields/v25` | `callbacks/blocks.rs`, `callbacks/climb.rs`, `callbacks/contexts.rs`, `src/binding/binary/callbacks.rs`, `src/binding/binary/type_pointers.rs`, `src/session/field_entries.rs` | [Block entry contexts](registry-fields.md#block-entry-contexts) |
| `FieldMembers::ModifierBlock` in `registry_fields` | `registry-fields/v25` | `modifier_blocks.rs`, `modifier_blocks/reference.rs`, `fields/member.rs`, `src/binding/binary/modifier_blocks.rs`, `src/session/modifier_blocks.rs` | [Modifier blocks](modifier-blocks.md) |
| `FieldMembers::WeightBlock` in `registry_fields` | `registry-fields/v25` | `weight_blocks.rs`, `fields/persistent.rs`, `src/binding/binary/weight_blocks.rs`, `src/session/weight_blocks.rs` | [Weight blocks](weight-blocks.md) |
| `FieldMembers::TriggeredModifier` in `registry_fields` | `registry-fields/v25` | `modifier_blocks/triggered.rs`, `fields/nested.rs`, `src/binding/binary/fields.rs`, `src/binding/binary/triggered_modifiers.rs`, `src/session/triggered_modifiers.rs` | [Triggered modifiers](triggered-modifiers.md) |
| `CommandGrammar.durations` | `command-grammar/v16` | `durations.rs`, `src/binding/binary/durations.rs`, `src/session/durations.rs` | [Duration keys](durations.md) |
| `Native::dynamic_names` | `dynamic-names/v3` | `dynamic_names.rs`, `dynamic_names/routes.rs`, `declarations/receiver.rs` | [Dynamic names](references.md#dynamic-names) |
| `Native::command_grammar` | `command-grammar/v16` | `grammar.rs`, `grammar/coverage.rs`, `grammar/forms.rs`, `grammar/numeric.rs`, `grammar/ordering.rs`, `grammar/targets.rs`, `declarations/receiver.rs` | [Nested command grammar](command-grammar.md) |
| `Native::declarations` | `command-declarations/v3` | `declarations.rs`, `declarations/composition.rs` | [Effects and triggers](engine-commands.md#effects-and-triggers) |
| `Native::modifiers` | `modifier-declarations/v1` | `modifiers.rs` | [Modifiers](engine-commands.md#modifiers) |
| `Native::modifier_categories` | `modifier-categories/v1` | `modifiers.rs` | [Categories](engine-commands.md#categories) |
| `Native::scopes` | `scope-declarations/v1` | `scopes.rs` | [Scope types](engine-commands.md#scope-types) |
| `Native::scope_links` | `scope-links/v2` | `scopes.rs` | [Scope links](engine-commands.md#scope-links) |
| `Native::localization_declarations` | `localization-declarations/v1` | `localization.rs` | [Localization contexts, commands and links](engine-commands.md#localization-contexts-commands-and-links) |
| `Native::on_actions`, `Native::game_rules` | `callbacks/v4` | `callbacks.rs`, `callbacks/names.rs`, `callbacks/contexts.rs`, `callbacks/climb.rs`, `src/binding/binary/callbacks.rs` | [On_actions, game rules and entry scopes](engine-commands.md#on_actions-game-rules-and-their-entry-scopes) |
| `Native::defines` | `defines/v1` | `defines.rs` | [Defines](engine-commands.md#defines) |
| `Native::modifier_families` | `modifier-families/v3` | `families.rs`, `families/joins.rs`, `families/loading.rs`, `families/strings.rs` | [Generation calls and roots](modifier-families.md#engine-code-m45-release) |
| `Field.accepted_categories` in `registry_fields` | `registry-fields/v25` | `fields/containers.rs`, `fields/persistent.rs`, `fields/nested.rs`, `src/binding/binary/receivers.rs`, `src/session/container_masks.rs` | [Modifier masks](modifier-masks.md#container-masks) |
| `Native::modifier_category_keys` | `modifier-category-keys/v1` | `category_keys.rs`, `src/binding/binary/language.rs`, `src/session/language.rs` | [Script category keys](modifier-masks.md#script-category-keys) |
| `Native::modifier_nodes` | `modifier-nodes/v1` | `modifier_nodes.rs`, `src/binding/binary/modifier_nodes.rs`, `src/session/modifier_nodes.rs` | [Modifier masks](modifier-masks.md) |
| `Native::script_expansions` | `script-expansions/v1` | `expansions.rs`, `directories.rs`, `src/binding/binary/expansions.rs`, `src/session/expansions.rs` | [Script expansion](script-expansion.md) |
| `Native::derived_names` | `derived-names/v1` | `names.rs`, `families/strings.rs`, `src/binding/binary/names.rs`, `src/session/names.rs` | [Derived names](derived-names.md) |
| `Game::loaded_modifiers` | `loaded-modifiers/v1` | `modifier_table.rs`, `src/engine/operations/loaded_modifiers.rs`, `src/session/loaded_modifiers.rs` | [Loaded modifier table](modifier-families.md#the-loaded-modifier-table) |

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
traditions and ascension perks are not joined (SDK-676).

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
