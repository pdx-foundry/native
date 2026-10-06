# Engine commands

These methods read, from the executable only, what the engine declares: effects and triggers
(`Native::declarations`), modifiers, categories, scope types and scope links, localization
contexts (`Native::localization_declarations`), on_actions and game rules. The module comments of
`engine/analysis/declarations.rs`, `declarations/composition.rs`, `modifiers.rs`, `scopes.rs`,
`localization.rs` and `callbacks.rs` describe the methods. The engine's documentation logs (from
the SDK-488 prototype in the `atlas-discovery` bundle, and the logs the release build writes) are
the comparison for the static results; the prototype's 14 live parser-scope checks agree with the
declarations.

## Effects and triggers

`Native::declarations` returns 1,080 effects and 1,098 triggers on M452 (1,074 and 1,096 on
M45-release), with no unnamed registration; names and documentation equal the live logs.
`tests/expected/m452/declaration-recovered.json` records each command that a mechanism other than
a direct call registers.

### Registration mechanisms

| Mechanism | M45-release names | How the method reads it |
| --- | --- | --- |
| Direct call to the register function | Most commands | The literal token in `w1` and the entry in `x2` |
| Tail call (`b`, not `bl`) to the register function, at the end of a static initializer | Effects `join_war_on_side`, `remove_building`, `remove_zone`; trigger `num_proxy_war` | As a direct call |
| Out-of-line `CEffectRegistryHelper<T>::CEffectRegistryHelper(int, char const*)` (and the trigger form). The compiler inlined the entry-map insertion, so the helper never calls the register function. | Effects `if`, `else`, `else_if` (all `CIfEffect`); triggers `and`, `custom_progress`, `hidden_progress`, `simple_progress` | The caller passes the literal token in `w1` and the documentation text in `x2`. The helper allocates the entry and stores its factory vtable next to the documentation argument. A helper that calls the register function is not an entry helper. |
| Script lists: `__GLOBAL__sub_I_script_lists.cpp` calls `CScriptListsRegistryHelper<Builder>(name, description)` for each of 102 list builders. The helper composes five commands with `GenerateTokenName` and `GenerateDocumentation` of `CRandomInScriptedListEffect`, `COrderedScriptedListEffect`, `CEveryInScriptedListEffect`, `CAnyInScriptedListTrigger<Builder>` and `CCountInScriptedListTrigger<Builder>`. It adds each name with `CStaticLexer::AddDynamicToken` and passes the token and documentation to `CScriptListsDynamicRegistryHelper<…>(int, CString const&)`, which registers the entry. | 307 effects: `random_`, `ordered_`, `every_` for each list, and `weighted_random_owned_pop_group`. 206 triggers: `any_` and `count_` for each list, and `count_` alone for `count_owned_pop_amount` and `count_owned_workforce`, which the initializer composes itself. | Composition (`declarations/composition.rs`) |

Registrations are not call sites. The initializer calls the `random_` helper of
`COwnedPopGroupListBuilder` a second time, with the literal token of
`weighted_random_owned_pop_group` and a literal documentation string. So the answer has one
declaration for each chain of callers, not for each call instruction.

The method does not establish whether a registration runs, or the order of the static
initializers. A registration through a function pointer is outside it.

### Supported scopes

The static reader joins a registration token to its factory vtable and documentation string. It
follows the factory's create method to the command vtable and the supported-scope getter. Bit
names come from `NEventScope::GetScopeName` of the same build. A zero mask means `Any`, as the
live documentation shows for `if`; other masks list names in bit order.

Every resolved scope set equals the `Supported Scopes:` line of its command in the release build's
`effects.log` and `triggers.log`. The unresolved sets (27 effects and 14 triggers, eight of them at
`command-vtable`) are in `tests/expected/m452/declaration-gaps.json`.

### Target getters are not target sets

`GetSupportedScopeTargets` is the slot after the supported-scope getter in the same command
vtable: `+0x88` for effects and `+0x80` for triggers on M45-release. Every getter that the method
reached returns a constant:

| Mask | Effects | Triggers | Where it comes from |
| --- | --- | --- | --- |
| 2 (`planet`) | 834 | 752 | `CEffect::GetSupportedScopeTargets`, and many trigger classes' own getters (`CIfTrigger`) |
| `0xfffc` (bits 2 to 15, `country` to `war`) | 229 | 163 | `CIntEffect`, `CBoolEffect`, `CValueEffect` and the matching triggers |
| 0 | 3 | 165 | `tooltip`, `exists`, `set_home_base` and others |
| An override that names a type | 4 | 12 | Listed below |
| Vtable not found | 4 | 4 | The scope set is also `Unresolved` |

The 16 overrides: `has_casus_belli`, `intel` and eight other triggers, and the effects
`transfer_resources_to_empire` and `transfer_galactic_defense_force_fleets`, give `country`;
`is_default_species` gives `species`; `is_background_planet` gives `planet, colony`;
`steal_planet_output` and `transfer_resource_stockpile` give `country, ship`.

These masks are not the scope types that a target argument accepts, so `Declaration` has no
target field:

- Mask 2 is on commands that take a country target (`set_owner`, `end_all_treaties_with`) and on
  commands that take no target (`if`, `else`). `0xfffc` is on `and`.
- Sixteen plausible overrides do not make the other 2,146 masks meaningful. What zero means is not
  established.
- One set for each command cannot say that a command takes no target, and cannot describe a
  command with two target arguments (`join_war_on_side = { war = <target> side = <country> }`).
- The release dump has no `Supported Targets:` line, and the executable has no such string.
  `CEffectDatabase::GenerateDocumentation` calls only the scope getter. `CEventTargetEffect::Read`,
  which reads the target of `set_owner`, does not call the target getter. No engine code that
  reads the target getter was found; the search did not cover every indirect call through the
  slot.

Which scope types an argument accepts is the `targets` property of `command_grammar`, with the
stage that checks it; see [target arguments](command-grammar.md#target-arguments-and-their-checks). Which scope a
child block runs in, including a block that keeps its parent's scope (`if`, `else`, `and`),
belongs to SDK-549.

## Modifiers, categories, scopes and links

### Modifiers

Each built-in modifier is one direct call to `CPdxModifier<…>::AddDefinition`; its arguments are
under [generation calls](modifier-families.md#generation-calls). M45-release has 586 direct calls:

- 571 give distinct literal names. They equal the first 571 entries of the loaded modifier table.
- Four give a name a second time with the same tags: `bonus_automated_workforce_mult`,
  `district_automated_workforce`, `country_storm_location_intel_add`,
  `country_storm_movement_intel_add`.
- 11 pass a token that is not a literal.

The 11 calls and the other sites that generate modifiers at run time are `UnnamedDeclaration` gaps
here. The [modifier families](modifier-families.md#generation-calls) page lists them and joins
them to registries. Five declared names have other tags in the
[loaded table](modifier-families.md#result-on-m45-release), because content registers the same
name again. The static answer keeps the executable's declaration.

### Categories

`GetModifierCategoryName` is a switch that writes a name in one of three ways: a literal
assignment, inline short-string bytes, or a 16-byte vector copy. It names 32 categories, including
`Ship Components` (0x1000) and `Cosmic Storm Influence Field` (0x8000000). The live log does not
print these two, because no loaded modifier uses them alone. A modifier's tags follow
`CModifier::LogDefinitions`: the name of the whole mask when one exists, otherwise the name of
each set bit.
The categories that each modifier node keeps are on [modifier masks](modifier-masks.md).

### Scope types

Scope types are the bits of `NEventScope::GetScopeName`. `GetScopeTypeEnumFromToken`, run on every
token value up to the largest literal token, groups the keywords of each type. M45-release has 42
types and 41 names:

- Bits 2 and 19 are both named `country`; their keywords are `country` and `observer`. So a scope
  type's identity is its bit, not its name. Each `ScopeDeclaration` has an opaque `ScopeId`, a
  hash of the bit that is valid within one build, and every scope reference carries it. Join
  references to declarations by `id`, never by name.
- Bit 37, `pop job`, has no literal keyword. Keep the name whole; splitting it invents a `job`
  scope.
- `alliance` and `federation` name one type.
- `carrier` maps to planet or ship (mask 0xa). It is the one `ScopeGroup`: a keyword that maps to
  several bits is not a keyword of each type. The same map serves `is_scope_type`
  (`CIsScopeTypeTrigger::Assign`), the context trigger and effect readers, the scripted-action and
  event-scope readers, and `TokenToEnum<EScopeType>`, so a group keyword is valid script.

### Scope links

A token is a link when one iteration of the loop in
`CEventTarget::GenerateEventTargetDocumentation` asks for its documentation. Input scopes come from
`CEventTarget::GetSupportedScopes` on a target that holds the token (recipe
`event_target_token_offset`); one case builds a second target and adds its scopes. The output
comes from `CEventTarget::GetScopeType`, where 0 is `Various`.

M45-release has the same 99 links as the SDK-488 log, with equal input scopes and outputs, except
`carrier`. M45-release declares its output as planet or ship (mask 0xa); the M45-observe beta and
the 4.4.1 dump printed `planet`. The executable does not state the reason.

### Links that take data

`CEventTarget::ParseForSpecialValues(EScopeType, CString const&)` is at `0x1004f7ca8` on
M45-release. `CEventTarget(CToken)` calls it with scope type 0; `CEventTarget(CToken, EScopeType,
CString const&)` passes its own. The function does these steps, in this order:

1. It returns at once when the target's token (`+0x58`) is in a compiled list of literal tokens.
2. It compares the target's text (`+0x68`) with `event_target:` and then with `parameter:`. These
   are the only two literal prefixes.
3. `event_target:V` sets `+0x188`. When `V` contains `@`, the function calls `ReadAsDynamicFlag`
   with a new sub-target at `+0x50`; this is the only use of the scope-type argument. Otherwise it
   removes a trailing `?` (`+0x189`), keeps `CPdxIntegerFlags::CreateFlagIndex` of the part before
   the first `.` at `+0x180`, and makes the rest a chained target at `+0x178`.
4. `parameter:N` keeps `CStaticLexer::AddDynamicToken` of the part before the first `.` at `+0x184`,
   and makes the rest a chained target.
5. Other text: it removes a trailing `?`, gives the part before the first `.` its own token
   (`FindTok`), and makes the rest a chained target.

Neither prefix branch writes the token. `GetSupportedScopes()` and `GetScopeType(int, char const*)`
switch on the token only; the second ignores its `char*`. `ValidateScope` and `CheckScopeSupport`
read scopes only through these two functions. A supported mask of 0 skips the check, and an output
of 0 skips the check of the next target in the chain. `FindTok` gives 12 to text that is not a
literal, and no literal name starts with a prefix.

Result: `event_target:` and `parameter:` both declare `Any` input and `Various` output, and
`scope_links()` is `Complete`. The scope-type argument does not change the scopes of a link.

Forms that are not links:

| Form | Reason |
| --- | --- |
| `A.B` | A chain of targets. `ValidateScope` checks each part in turn. |
| Trailing `?` | An option on the target and its chain (`+0x189`). `CEventTarget::GetScope` reads it at run time. |
| `@` in an `event_target:` value | The dynamic-flag form (`ReadAsDynamicFlag`, as in `has_country_flag = name@target`). It names the saved target. |
| `value:`, `trigger:` and other value prefixes | `CVariableValue::ReadTriggerModifierOrScriptValue` splits them on `:` and reads a number, not a scope ([script expansion](script-expansion.md)). |

Which saved target or parameter a value names, and whether it exists in a running game, are not
established: saved event targets are an `OutsideMethod` gap of
[dynamic names](references.md#dynamic-name-gaps). The parameters of `value:` and of scripted
effects and triggers are in [script expansion](script-expansion.md).

## Localization contexts, commands and links

`CGameApplication::PrintScriptingDocumentation` writes `localizations.log` from
`CGameText::GenerateDocumentation`. A context is an `ECURRENT_POINTER` value: the kind of object
that a text statement points at. There is no registration call and no factory.

- The `CGameText` constructor fills four function tables, indexed by context: the link-row getter
  at `+0x318`, the link function at `+0x498`, the command-row getter at `+0x618` and the property
  getters at `+0x798`.
- A row getter such as `GetCountryPromotionTargets(int&)` writes a count and returns 16-byte rows
  of a name pointer and an index.
- A link function such as `PromoteCountry(void const*, CGameText&, int)` dispatches on the index
  and reaches a setter that writes the new context to `CGameText+0x8`.
- `CGameText::SetScopeObject` switches on the scope-type value (`1 << bit`, the bits of
  `Native::scopes`) and selects one context. That is the scope join.
- `GenerateDocumentation` skips contexts 3, 13, 31 and 32 (mask `0xfffe7fffdff7`), but their
  tables are filled: `Diplomacy` (4 commands, 3 links), `Building` (1), `Job Swap Data` (3) and
  `Pop Category Swap Data` (3).
- The rows are in the initial image. Pointer slots that the fixup chain binds to another image are
  unknown.

**Result on M45-release.** 48 contexts, 151 command names in 245 command rows, and
102 link rows. The 44 documented contexts have the same command and link names as the dump. 28
contexts join scope types (`Ship (and Starbase)` joins `ship` and `starbase`; `System` joins
`galactic_object`). 20 are `Missing`: `Base Scope`, the 12 dead-object contexts, `Diplomacy`,
`Building`, `Job Swap Data`, `Pop Category Swap Data`, `Patron Relation`, `Specimen` and
`Timeline Event`. Their commands and links stay in the answer. The 14 `Various` rows are the 12
`Base Scope` promotions (`This`, `Root`, `From`, `Prev` in three spellings) and `Target` from
`Espionage Operation` and `Situation`. `Third_party` from `Diplomacy` is `Unchanged`:
`PromoteAction` handles indexes 0 and 1 and returns for index 2.

**Gaps.** 27 link rows stay `Unresolved`:

- 24 links find a saved or dead object by a run-time identifier, through a hash-table probe whose
  exit depends on run-time data, so the paths reach the 64-path limit: `EVENT_TARGET_0` to
  `EVENT_TARGET_9` from `Timeline Event` and from `Specimen`, `Target` and `Owner` from
  `Dead Situation`, `MainAttacker` and `MainDefender` from `Dead War`.
- `Planet` and `Ship` from `Colony` return on a path after a carrier lookup that the method does
  not follow.
- `Planet` from `Deposit` selects planet or ship on some paths and returns without a new context
  on another.

The prose of the dump names five unscoped forms. `GetYear` and `LastKilledCountryName` are also
`Base Scope` rows and `GetDate` is a `Timeline Event` row; `GetMidGameDate` and `GetLateGameDate`
are in no table. The unscoped forms are an `OutsideMethod` gap. Whether a command gives useful
text at run time, argument forms, formatting, scripted localization and fallback between
`Base Scope` and a typed context are not tested (SDK-609).

## On_actions, game rules and their entry scopes

The engine has no documentation dump for either.

- **On_actions.** Engine code fires an on_action with
  `COnActionDatabase::PerformEvent(CString const&, CEventScope&, …)`. Nearly every call site builds
  the name as a stack `CString` from a text literal. A deferred command,
  `COnActionCommand(CString const&, CEventScope const&, …)`, fires later from `Execute`.
- **Pulse lists.** `COnActionDatabase::Init` caches 14 pulse lists in database fields after a
  `strcmp` of each list name. `CGameState::MonthlyUpdate` and `YearlyUpdate` fire them with
  `PerformEvent(COnActionList const*, …)`.
- **Scopes.** A `CEventScope` has its type, one `EScopeType` bit, at `+0x08`, and root, from and
  prev links at `+0x30`, `+0x38` and `+0x40`. The fresh constructors write type 0 and point every
  link back to the scope itself. Typed setters, such as `CScopeObjectReference::SetCountry`, write
  one type constant. The answer keeps a self-link as `SelfLink`.
- **Game rules.** A game rule is a member of the rule set: `CGameRules::CanColonizePlanet` builds
  a scope and calls `CScriptedRule::Evaluate(this + 20 * 0xc0, scope, …)`. Weighted rules start at
  `this + 0x9cc0`, `0x40` apart. `__GLOBAL__sub_I_game_rules.cpp` fills the rule declaration
  tables, and `FindRuleDeclarationByEnum` returns the row, with its token, for a rule's
  enumeration.

**How script reads a link.** `CEventTarget::GetScope(CEventScope&, char const*)` (`0x1004f9860` on
M451-hotfix) resolves the `root`, `from` and `prev` tokens (`0x2c92`, `0x2c78`, `0x2c93`). The
self-link rule below is an assumption, checked by hand at these cases and by the comparisons
below; Atlas applies it to the raw answer.

- `root` (`0x1004fa8f8`) copies the root link with no check, so a self-linked root is the scope
  itself, with the type of `this`.
- `from` (`0x1004fa8e8`) and `prev` (`0x1004fa900`) give no scope when the link is the scope
  itself.
- `fromfrom` (`0x1004faba4`) is guarded by `IsFromFromSet`, which tests the types of `from` and
  `from.from`, not the pointers. So a self-link later in the `from` chain is the scope that holds
  it, and a typed scope whose `from` links to itself has itself as `fromfrom` although its `from`
  is no scope. A `from` chain ends at its first scope with no type.
- `prevprev` (token `0x2cfa`, `0x1004fabb8`) follows two links and compares the end with the entry
  scope only. So the `prev` chain passes a scope with no type, and a later self-link is the scope
  that holds it.

The `EEffectUserDataKey` map that the firing functions take is not visible to script; its named
reader is `NAIUtil::GetSpecialOfferData`.

**Evaluation leaves a scope as it found it.** A second assumption, used by the registry field
block method ([block entry contexts](registry-fields.md#block-entry-contexts)): trigger and effect
code that receives a scope does not change the type or links of any scope object and keeps no
pointer to one. It covers the evaluators, such as `CTrigger::Evaluate` and `CRootEffect::Execute`,
and the other `const` members of trigger and effect classes that take a scope, such as
`CAndTrigger::BuildToolTip`. Checked by hand on M451-hotfix:

- `CTrigger::Evaluate` (`0x100d063f0`) and `CEffect::Execute` (`0x100458118`) only forward the
  scope through virtual calls: `ActualEvaluate` at vtable `+0x20` and `ExecuteActual` at `+0x50`.
- A scope-changing trigger, `CAnyInScriptedListTrigger<CAmbientObjectListBuilder>::ActualEvaluate`
  (`0x101eb29a0`), copies the scope it receives into a local child, copies root and from, links the
  child's prev to the received scope and sets the child's type. It only reads the received scope.
- A scope-changing effect, `CEveryInListEffect::ExecuteActual` (`0x101d225cc`), does the same.

**Result on M451-hotfix.** 294 on_actions; 281 have at least one context and 207 have at least
one context with no unresolved scope. 13 names keep several contexts with no unresolved scope: for
example, a fleet enters `on_fleet_enter_orbit` with a megastructure, a planet, a starbase or an
astral rift as from. Three name a typed prev: `on_modification_complete`,
`on_subspecies_integration_step` and `on_subspecies_integration_complete` link the colony as prev.
223 game rules (209 scripted, 14 weighted); 220 have a context and 204 a context with no unresolved
scope. These call sites were checked by hand in the disassembly:

- `on_game_start` and `on_monthly_pulse`: a new scope with no type.
- `on_leader_level_up`: country, from leader.
- `on_planet_returned`: planet, from country, fromfrom country.
- `on_modification_complete`: country, from and fromfrom species, prev colony.
- `can_colonize_planet`: planet, root country.
- `can_add_claim`: galactic object, root country.
- `can_orbital_bombard`: fleet, from planet.
- `leader_election_weight`: weighted, leader.

**Comparison with independent sources.** Read through the self-link rule:

- 184 of 294 on_actions agree with the vanilla scope comments in `common/on_actions`.
- 181 of 223 game rules agree with the config's `replace_scopes`.

Each disagreement was read by hand, and the engine agrees with Native in every case. Use these
shapes when a source and the answer differ:

- The config gives 9 rules a `from` of the `this` type, for example `is_mercenary`. The engine
  builds one fresh scope, so `from` self-links and script sees no `from`; vanilla never reads
  `from` in them.
- The config writes `planet` where the engine passes `colony` (`can_ai_assign_governor`), and
  `carrier` where it passes `colony` or `planet`. `dismiss_leader_cost` is evaluated on a leader,
  not a country.
- A comment names an object that the engine never sets (`on_rebels_take_planet` has no war;
  `on_specialist_subject_conversion_aborted` passes the agreement's target country). A comment
  says fleet where the engine passes a ship (`on_system_survey`).
- A comment describes the call site whose scope the method cannot follow (`on_ship_built`), or a
  branch past the path limit (`on_ship_quantum_catapult`).

The engine fires some names that the config lacks, such as `on_leaving_system_fleet`,
`on_colony_transfer`, `on_fleet_went_mia`, `on_waystation_lost`, and the rules `can_jump_drive`
and `can_scavenge_debris`. Most on_actions that only the config has are fired by script content
(`fire_on_action`) or at a site whose name the method cannot recover.

**Gaps.**

- 31 sites fire a name that is not one text literal: a conditional select between two literals
  (`on_add_to_imperial_council` or `on_remove_from_imperial_council`), a name that a wrapper that
  is not pinned receives (`CArmy::PerformBuildingOnAction`), or a name built at run time
  (`_queued`). One list site fires a list that an object holds.
- 13 on_actions have no context: 9 reach the path limit (including
  `on_war_participant_leaves_early`), 2 are not reached from their function entry, 1 stops at an
  instruction that the method cannot follow, and `on_press_begin`'s command builds its own scope.
- 74 on_actions have only unresolved contexts. Most reuse one scope for several firing calls: the
  first call receives the scope, and the method cannot show that the event system leaves its type
  and links unchanged, so the later sites are unresolved (the pulse lists after
  `on_yearly_pulse`, `on_leader_death`, `on_planet_surveyed`). Some fill the scope with a helper
  whose type depends on a run-time value (`CDepositHolderRefCaster::FillEventScope`,
  `CFleetOrbitalSlotHandle::SetupScopeObject`).
- 3 declared rules have no call site that the method follows, 16 have only unresolved contexts, and
  2 rule sites pass a rule object that is not a constant.
- On_actions that content defines are an `OutsideMethod` gap. Events and their `push_scope` are
  SDK-702; pre_trigger key sets are SDK-703.

## Defines

On M452, `Native::defines()` finds 2,386 compiled `NDefines` and `NUncheckedDefines`
`ReadDefine` helpers and follows 2,306 of them to a literal namespace, a literal name and a typed
engine reader; `tests/expected/m452/defines.json` holds the counts by type. The other 80 named
helpers use a table-search loop that exceeds the path search; they are `UnresolvedReader` gaps,
including `NGraphics.ORBIT_HSV` (SDK-610). No helper fails or is unnamed.

The method classifies the target of each direct `GetValue`, `GetArrayValue` or
`ReadDefinesValue` call. `GetValue` takes namespace and name arguments; `GetArrayValue` reads a
named value from a namespace table. Shipped define entries, defaults, comments, bounds and uses
are not established. Atlas owns the comparison with shipped content and config.

## Pitfalls

Each item below gave a wrong or missing answer once.

**Reading code and vtables.**

- A create method can load the command vtable through the global offset table (`adrp`, then
  `ldr x8, [x8, #off]`, then `add x8, x8, #0x10`), for example the eight
  `CSetSpeciesRightsEffect<…>` effects. When the load was dropped, the method took the
  `CPdxArray<CEffect*, int>` vtable of a member from its constructor call, and read an unrelated
  slot. A load through a known pointer gives the vtable.
- A create method can store the vtable through a copy of the object register with a post-index
  store (`mov x8, x19` then `str x9, [x8], #0x68`), for example `exists`, `is_surveyed` and
  `set_name`. The copy is the object until the code writes its register or a call can change it.
- A section shorter than 8 bytes gave an invalid pointer range.
- The `strcmp` stub keeps its raw symbol name, `_strcmp`.
- 166 rule-set functions add the rule offset from a register.
- Composition: the database's `CreateInstance` takes no argument and must return. Otherwise a path
  on which the database does not exist yet loses the documentation string in `x0`.

**Evaluator.**

- Fork on unknown flags, and keep every flag state that stays possible on each side, not one
  sample state. Later decisions on the same flags then stay consistent.
- An indirect call (`blr`) to an unknown destination is an unknown call. Follow one whose
  destination is known.
- Follow every function that takes `CGameText&`. The first run did not follow 26 calls to text
  helpers.
- Instructions that were missing once: `mul`, traps, `ubfx`, `bfi`, multiply-add, `dup`, `bic`.
  Every `b…` form must take its own destination.
- Stack offsets are from the entry stack pointer. A dynamic stack allocation made every stack fact
  unknown until the evaluator allowed an unknown stack pointer.
- Keep the width of each store; a store taken as 32 bytes wide lost names.
- `on_monthly_pulse`'s field address is formed by a write-back and spilled to a stack slot.
- A loop with an unknown exit used the whole path budget; the method needs a loop limit.
- Branches through a register must reach the site.

**Answers.**

- A link whose paths both select a context and leave it unchanged is `Unresolved`, not listed. A
  link that selects nothing is `Unchanged`.
- A context that a link or a scope type selects is in the answer even when its tables are empty,
  so every reference joins.
- Do not guess bound pointer slots from their top bit.
- At a join, a string that one path did not build must not keep the other path's text.
- A scope function at the call depth, or one that passes the scope on, makes the scope's slots
  unknown.
- A decode failure at a rule site is not an on_action. Rules of two families with one name stay
  separate.
