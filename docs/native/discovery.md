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
| `Native::registry_fields` | `registry-fields/v12` | `fields.rs`, `fields/control_flow.rs`, `fields/dispatch.rs`, `fields/inventory.rs`, `fields/nested.rs`, `fields/persistent.rs`, `fields/uses.rs`, `fields/records.rs`, `fields/tokens.rs`, `readers.rs` | [Field sweep and stops](registry-fields.md#current-m45-sweep) |
| `Field.reference` in `registry_fields` and `command_grammar` | `registry-fields/v12`, `command-grammar/v13` | `references.rs`, `references/initialization.rs`, `references/shapes.rs`, `src/binding/binary/references.rs` | [References and dynamic names](references.md) |
| `Reader.numeric` in fields and command grammar | `registry-fields/v12`, `command-grammar/v13` | `numeric.rs`, `numeric/modifier.rs`, `src/binding/binary/numeric.rs` | [Numeric conversion](numeric-conversion.md) |
| `Reader.scoped_operand` in fields and command grammar | `registry-fields/v12`, `command-grammar/v13` | `scoped_numeric.rs`, `src/binding/binary/scoped_numeric.rs`, `src/session/scoped_numeric.rs` | [Scoped numeric](scoped-numeric.md) |
| `FieldMembers::ModifierBlock` in `registry_fields` | `registry-fields/v12` | `modifier_blocks.rs`, `modifier_blocks/reference.rs`, `fields/member.rs`, `src/binding/binary/modifier_blocks.rs`, `src/session/modifier_blocks.rs` | [Modifier blocks](modifier-blocks.md) |
| `CommandGrammar.durations` | `command-grammar/v13` | `durations.rs`, `src/binding/binary/durations.rs`, `src/session/durations.rs` | [Duration keys](durations.md) |
| `Native::dynamic_names` | `dynamic-names/v2` | `dynamic_names.rs`, `dynamic_names/routes.rs`, `declarations/receiver.rs` | [Dynamic names](references.md#dynamic-names) |
| `Native::command_grammar` | `command-grammar/v13` | `grammar.rs`, `grammar/coverage.rs`, `grammar/forms.rs`, `grammar/numeric.rs`, `grammar/ordering.rs`, `grammar/targets.rs`, `declarations/receiver.rs` | [Nested command grammar](command-grammar.md) |
| `Native::declarations` | `command-declarations/v3` | `declarations.rs`, `declarations/composition.rs` | [Effects and triggers](engine-commands.md#effects-and-triggers) |
| `Native::modifiers` | `modifier-declarations/v1` | `modifiers.rs` | [Modifiers](engine-commands.md#modifiers) |
| `Native::modifier_categories` | `modifier-categories/v1` | `modifiers.rs` | [Categories](engine-commands.md#categories) |
| `Native::scopes` | `scope-declarations/v1` | `scopes.rs` | [Scope types](engine-commands.md#scope-types) |
| `Native::scope_links` | `scope-links/v2` | `scopes.rs` | [Scope links](engine-commands.md#scope-links) |
| `Native::localization_declarations` | `localization-declarations/v1` | `localization.rs` | [Localization contexts, commands and links](engine-commands.md#localization-contexts-commands-and-links) |
| `Native::on_actions`, `Native::game_rules` | `callbacks/v1` | `callbacks.rs`, `callbacks/names.rs`, `callbacks/contexts.rs` | [On_actions, game rules and entry scopes](engine-commands.md#on_actions-game-rules-and-their-entry-scopes) |
| `Native::defines` | `defines/v1` | `defines.rs` | [Defines](engine-commands.md#defines) |
| `Native::modifier_families` | `modifier-families/v3` | `families.rs`, `families/joins.rs`, `families/loading.rs`, `families/strings.rs` | [Generation calls and roots](modifier-families.md#engine-code-m45-release) |
| `Game::loaded_modifiers` | `loaded-modifiers/v1` | `modifier_table.rs`, `src/engine/operations/loaded_modifiers.rs`, `src/session/loaded_modifiers.rs` | [Loaded modifier table](modifier-families.md#the-loaded-modifier-table) |
