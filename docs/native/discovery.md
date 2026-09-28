# Discovery methods

Each method records its engine facts, current result, gaps and pitfalls on the page for
its subject. Add a page when a method starts a new subject. The code and its module comments
describe the methods; these pages hold what the code cannot.

Use the [method-authoring guide](method-authoring.md) to explore, implement and check a method in
one task. Each row below is one method source stamp; the two callback operations share a method.
Module paths are relative to `src/engine/analysis/` unless a full `src/` path is shown. The table
names the method owners, not every shared decoder or evaluator they use. It covers the static
methods and their live loaded-modifier join; other live observations are in
[early observations](early-observations.md).

The SDK-542 extraction, parser checks, population counts and consumer contract are in
[nested command grammar](command-grammar.md).

| Operation | Source stamp | Modules | Knowledge section |
| --- | --- | --- | --- |
| `Native::registries` | `registry-directories/v3` | `discovery.rs`, `directories.rs` | [Registry candidates and owner joins](registry-fields.md#registry-scheduling-and-owner-joins) |
| `Native::registry_fields` | `registry-fields/v8` | `fields.rs`, `fields/control_flow.rs`, `fields/dispatch.rs`, `fields/inventory.rs`, `fields/nested.rs`, `fields/persistent.rs`, `fields/uses.rs`, `fields/records.rs`, `fields/tokens.rs`, `readers.rs` | [Field sweep and stops](registry-fields.md#current-m45-sweep) |
| `Field.reference` in `registry_fields` and `command_grammar` | `registry-fields/v8`, `command-grammar/v9` | `references.rs`, `references/initialization.rs`, `references/shapes.rs`, `src/binding/binary/references.rs` | [References and dynamic names](references.md) |
| `Native::dynamic_names` | `dynamic-names/v2` | `dynamic_names.rs`, `dynamic_names/routes.rs`, `declarations/receiver.rs` | [Dynamic names](references.md#dynamic-names) |
| `Native::command_grammar` | `command-grammar/v9` | `grammar.rs`, `grammar/coverage.rs`, `grammar/forms.rs`, `grammar/numeric.rs`, `grammar/ordering.rs`, `grammar/targets.rs`, `declarations/receiver.rs` | [Nested command grammar](command-grammar.md) |
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
