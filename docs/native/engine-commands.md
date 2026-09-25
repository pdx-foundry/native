# Engine commands

These notes are on the commands, scopes, links, localization and callbacks that the engine
declares. The [discovery index](discovery.md) lists the other method pages.

## Engine documentation commands

SDK-488 accepted the bounded engine-produced inventory: 1,096 triggers, 1,074 effects, 99 scope links and 45,578 modifier entries, without config/historical dump seeds. Fourteen live parser-scope checks agree with declarations. `atlas-discovery/prototype/engine-command-discovery/` retains the native documentation boundary, logs, factory bindings, exact normalization, omission/missing-scope controls and offline replay. Frozen inventory precedes historical comparison.

SDK-535 ported effects and triggers to `Native::declarations` (`command-declarations/v1`). Its M45 direct-call scan found 1,067 effect sites and 1,091 trigger sites. It returned 761 named effects and 885 named triggers. 306 effect and 206 trigger sites composed their names at run time. Seven effects and five triggers of the live inventory were outside the direct-call boundary.

SDK-562 (`command-declarations/v2`) recovers all of them. On M45-release it returns 1,074 effects and 1,096 triggers with no unnamed registration. The names are the same as in the SDK-488 live inventory. The documentation of every recovered command is equal to the live log, and so is each scope set that the method resolves. The SDK-488 logs are from M45-observe. Of the names that SDK-535 already had, two trigger descriptions (`is_original_owner`, `original_owner`) changed their wording in the release build ("planet" became "colony carrier"). The 87 scope sets that stay `Unresolved` are all on names from SDK-535 (SDK-568 resolves 46 of them, below). Three registration mechanisms were outside the v1 boundary:

| Mechanism | M45-release names | How the method reads it |
| --- | --- | --- |
| Tail call (`b`, not `bl`) to the register function, at the end of a static initializer | effects `join_war_on_side`, `remove_building`, `remove_zone`; trigger `num_proxy_war` | The same literal token and entry reading as a direct call |
| Out-of-line `CEffectRegistryHelper<T>::CEffectRegistryHelper(int, char const*)` (and the trigger form). The compiler inlined the entry-map insertion, so the helper never calls the register function. | effects `if`, `else`, `else_if` (all `CIfEffect`); triggers `and`, `custom_progress`, `hidden_progress`, `simple_progress` | The caller passes the literal token in `w1` and the documentation text in `x2`. The helper allocates the entry and stores its factory vtable next to the documentation argument. A helper that calls the register function is not an entry helper. |
| Script lists: `__GLOBAL__sub_I_script_lists.cpp` calls `CScriptListsRegistryHelper<Builder>(name, description)` for each of 102 list builders. That helper composes five commands with `GenerateTokenName` and `GenerateDocumentation` of `CRandomInScriptedListEffect`, `COrderedScriptedListEffect`, `CEveryInScriptedListEffect`, `CAnyInScriptedListTrigger<Builder>` and `CCountInScriptedListTrigger<Builder>`. It adds each name with `CStaticLexer::AddDynamicToken` and passes the token and documentation to `CScriptListsDynamicRegistryHelper<…>(int, CString const&)`, which registers the entry. | 307 effects: `random_`, `ordered_`, `every_` for each list, and `weighted_random_owned_pop_group`. 206 triggers: `any_` and `count_` for each list, and `count_` alone for two lists that the initializer composes itself (`count_owned_pop_amount`, `count_owned_workforce`). | Composition (below) |

Registrations are not sites. The 306 effect sites were 306 registrations in v1, but one of them registers two commands. The initializer calls the `random_` registration helper of `COwnedPopGroupListBuilder` a second time, with the literal token of `weighted_random_owned_pop_group` and a literal documentation string. So the v2 answer has one declaration for each chain of callers, not for each call instruction.

**Composition** (`engine/analysis/declarations/composition.rs`). When the token in `w1` is not a literal, the method runs the function that contains the registration call from its entry. It uses the evaluator and the string model of the modifier-family method. If that run does not give the name and the documentation, it runs from each caller of the function, through the call, and so on, up to two callers. The run enters only the composers (`GenerateTokenName`, `GenerateDocumentation`, which the binding finds by symbol). `AddDynamicToken` returns a new token labelled with its name, `operator new` returns fresh memory, and the database's `CreateInstance` returns. That function takes no argument. Without this rule, a path on which the database does not exist yet lost the documentation string in `x0`. At the registration call the method reads the name from the token, and reads the factory and the documentation text from the entry in `x2`. Every path must agree. A chain that is not established at the greatest depth stays an `UnnamedDeclaration` gap that names where the run stopped.

The recovered declarations are recorded in `tests/expected/m45/declaration-recovered.json` with their mechanism. The method does not establish whether a registration runs, or the order of the static initializers. A registration through a function pointer is outside it.

The static reader joins a registration token to its factory vtable and documentation string, then follows the factory's create method to the command vtable and supported-scope getter. It derives bit names from `NEventScope::GetScopeName` in this exact build. A zero getter mask means `Any`, as the live documentation shows for `if`; other masks list names in bit order. An unresolved link stays on the returned declaration as `DeclaredScopes::Unresolved` with a gap. The method does not establish registration timing or reachability, and does not read config files.

Modifier categories are intended-use tags, not demonstrated application contexts. Real object application and propagation through containers remain unqualified. The native documentation facility supplies observations; Atlas decides which rule claims the evidence supports.

### Target getters, a rejected interpretation, and scope-set fixes (SDK-568)

SDK-568 asked for each command's declared target set: the scope types that its target argument
accepts, read from the target getter as SDK-535 reads the scope getter. The getter was found and
read, but its masks are not argument constraints, so `Declaration` has no target field. Which scope
types an argument accepts belongs to the argument readers (SDK-548). Which scope a child block runs
in, including a block that keeps the scope it is used in (`if`, `else`, `and`), belongs to SDK-549.

**The getter.** `GetSupportedScopeTargets` is the slot after the supported-scope getter in the same
command vtable: `+0x88` for effects and `+0x80` for triggers on M45-release. Every getter that the
method reached returns a constant. Raw masks, read through `NEventScope::GetScopeName` bit names:

| Mask | Effects | Triggers | Where it comes from |
| --- | --- | --- | --- |
| 2 (`planet`) | 834 | 752 | `CEffect::GetSupportedScopeTargets`, and many trigger classes' own getters (`CIfTrigger`) |
| `0xfffc` (bits 2 to 15, `country` to `war`) | 229 | 163 | `CIntEffect`, `CBoolEffect`, `CValueEffect` and the matching triggers |
| 0 | 3 | 165 | `tooltip`, `exists`, `set_home_base` and others |
| An override that names a type | 4 | 12 | See below |
| Vtable not found | 4 | 4 | The scope set is also `Unresolved` |

Why the masks are not target sets:

- Mask 2 is on commands that take a country target (`set_owner`, `end_all_treaties_with`) and on
  commands that take no target (`if`, `else`). `0xfffc` is on `and`.
- The overrides do name a plausible argument type: `has_casus_belli`, `intel` and eight other
  triggers, and `transfer_resources_to_empire` and `transfer_galactic_defense_force_fleets`, give
  `country`; `is_default_species` gives `species`; `is_background_planet` gives `planet, colony`;
  `steal_planet_output` and `transfer_resource_stockpile` give `country, ship`. Sixteen plausible
  values do not make the other 2,146 meaningful.
- The meaning of zero was not established. For scope masks zero means every scope; nothing shows
  that it means every target, or no target.
- A single set for each command cannot say that a command takes no target, and cannot describe a
  command with two target arguments (`join_war_on_side = { war = <target> side = <country> }`).
- The release dump has no `Supported Targets:` line, and the executable has no such string.
  `CEffectDatabase::GenerateDocumentation` calls only the scope getter. `CEventTargetEffect::Read`,
  which reads the target of `set_owner`, does not call the target getter. No engine code that reads
  the target getter was found; the search covered those functions and direct calls to the getters,
  not every indirect call through the slot, so "unused" is not established.

**Scope-set fixes.** Two create-method shapes were misread. The fixes stay in
`command-declarations/v3`:

- The create methods of 24 declarations load the command vtable through the global offset table
  (`adrp`, then `ldr x8, [x8, #off]`, then `add x8, x8, #0x10`), for example the eight
  `CSetSpeciesRightsEffect<…>` effects and their `has_`/`former_` triggers. The method dropped the
  load and took a vtable from the constructor call, which was the `CPdxArray<CEffect*, int>` vtable
  of a member. Its slot at `+0x80` was an unrelated function, so the scope set stopped at
  `scope-mask` by chance. A load through a known pointer now gives the vtable.
- The create methods of 22 declarations store the vtable through a copy of the object register
  with a post-index store (`mov x8, x19` then `str x9, [x8], #0x68`), for example `exists`,
  `is_surveyed` and `set_name`. The copy is the object until the code writes its register or a call
  can change it.

| Kind | Scope `Unresolved` before | After |
| --- | --- | --- |
| Effects | 40 | 27 |
| Triggers | 47 | 14 |

Every resolved scope set, with the 46 new ones, equals the `Supported Scopes:` line of its command
in the `effects.log` and `triggers.log` that the release build wrote on 2026-09-22 (it has the
release wording of `is_original_owner`). Eight commands still stop at `command-vtable`: effects
`pop_change_ethic`, `pop_force_add_ethic`, `remove_random_starbase_building`,
`remove_random_starbase_module`, and triggers `has_relation_flag`, `is_war_participant`,
`pop_ethic_amount`, `reverse_has_relation_flag`. The unresolved names are recorded in
`tests/expected/m45/declaration-gaps.json`.

### Modifiers, categories, scopes and links (SDK-536)

SDK-536 ports the other four inventories to static questions on M45-release. The prototype read them from the engine's documentation text in a live game; the port runs the same compiled engine code without a game. `engine/analysis/evaluate.rs` runs one compiled function for one known input and follows one path. A branch on an unknown value, an unsupported instruction or a jump outside the decoded code stops the run with a reason. It never guesses a path. The four methods use it.

- **Modifiers** (`Native::modifiers`). Each built-in modifier is one direct call to `CPdxModifier<…>::AddDefinition`, with the token in `w0` and the category mask at `[sp,#4]` (recipe `modifier_category_offset`). The method runs the straight-line code between the previous call and the definition call. M45-release has 586 direct calls:
  - 571 give distinct literal names.
  - Four give a name a second time with the same tags (`bonus_automated_workforce_mult`, `district_automated_workforce`, `country_storm_location_intel_add`, `country_storm_movement_intel_add`).
  - 11 pass a token that is not a literal: ten calls in `CShipClassModifierHelper::Init` compose the name from a string, and one call is inside `CModifier::TryAddDynamicModifier`.

  61 calls to `TryAddDynamicModifier` and one call to `AddDynamicModifier` generate modifier families from content. Each of these 73 sites is an `UnnamedDeclaration` gap.
- **Categories** (`Native::modifier_categories`). The method runs `GetModifierCategoryName` on each single bit, on all bits, and on each mask that a built-in modifier uses. The switch writes a name in one of three ways: a literal assignment, inline short-string bytes, or a 16-byte vector copy. A modifier's tags follow the rule in `CModifier::LogDefinitions`: the name of the whole mask when one exists, otherwise the name of each set bit.
- **Scopes** (`Native::scopes`). Scope types are the bits of `NEventScope::GetScopeName`. The method runs `GetScopeTypeEnumFromToken` on every token value up to the largest literal token, which groups the keywords of each type. It does not use the config's alias groups. A keyword that maps to several bits is a `ScopeGroup`, not a keyword of each type. The same map serves `is_scope_type` (`CIsScopeTypeTrigger::Assign`), the context trigger and effect readers, the scripted-action and event-scope readers, and `TokenToEnum<EScopeType>`, so a group keyword is valid script.
- **Links** (`Native::scope_links`). The method runs one iteration of the loop in `CEventTarget::GenerateEventTargetDocumentation` for each token. A token is a link when the loop body asks for its documentation. Input scopes come from `CEventTarget::GetSupportedScopes` on a target that holds the token (recipe `event_target_token_offset`). One case builds a second target and adds its scopes, and the method follows that call. The output comes from `CEventTarget::GetScopeType`, where 0 is `Various`. Links that take data are the literals ending in `:` in `CEventTarget::ParseForSpecialValues`. SDK-565 gives their scopes (below).

Comparison with the SDK-488 inventory (M45-observe, frozen `runs/20260917-154455`), after the static result was produced:

| Inventory | SDK-536 on M45-release | SDK-488 live | Difference |
| --- | ---: | ---: | --- |
| Modifiers | 571 | 45,578 | The 571 names equal the first 571 entries of the loaded table exactly, in the same set. The other 45,007 entries are generated from content or added at run time (73 generation sites; SDK-540 owns the templates). |
| Modifier tags | 566 equal | — | Five differ: `terraforming_cost_mult`, `starbase_shipyard_build_cost_mult`, `starbase_shipyard_artificial_build_cost_mult`, `starbase_shipyard_space_fauna_build_cost_mult` and `gdf_ship_alloys_cost_mult`. Each loaded entry has `AI Economy` and content-chosen tags. Content registers the same name again: for example `common/economic_categories` `terraforming` has `generate_mult_modifiers` and `modifier_category = planet`. The static answer keeps the executable's declaration. |
| Categories | 32 | 30 printed | The switch also names `Ship Components` (0x1000) and `Cosmic Storm Influence Field` (0x8000000). The live log did not print them, because no loaded modifier uses them alone. |
| Scope links | 99 + 2 prefixes | 99 | The same 99 names, with equal input scopes and outputs, except `carrier`. M45-release declares its output as planet or ship (mask 0xa). The M45-observe beta and the 4.4.1 dump both printed `planet`, so the change came with the full 4.5 release. It agrees with colonies on ships in the Nomads release; the executable does not state the reason. `event_target:` and `parameter:` are the data prefixes (SDK-565: `Any` input, `Various` output). |
| Scopes | 42 types, 41 names | 40 names across the logs | A log prints only the types that a documented command or link uses; the link log alone prints 34 of the 41 names, with `pop job` split into `pop` and `job`. Two types are named `country`: bit 2, keyword `country`, and bit 19, keyword `observer`. Bit 37, `pop job`, has no literal keyword. `alliance` and `federation` name one type. `carrier` maps to planet or ship (mask 0xa) and is the one `ScopeGroup`. |

Keeping `pop job` whole corrects the SDK-535 scope lists as well. Before this change they split the name into `pop` and `job`, which invented a `job` scope and repeated `pop`.

A scope type's identity is its bit, not its name: bits 2 and 19 are both named `country`. Each `ScopeDeclaration` has an opaque `ScopeId`, a hash of the bit that is valid within one build. Every scope reference carries the same identity: command and link scopes, link outputs, and `ScopeGroup` members. A reference also carries the display name, only for reading. Join references to declarations by `id`, never by name.

**Not in SDK-536.** The ticket asked for the full modifier inventory with the SDK-488 count, so it was narrowed. The loaded inventory with its generated families is **SDK-564**, and the name templates are **SDK-540**. The scopes of the data-taking links `event_target:` and `parameter:` are **SDK-565**, in the next section.

### Scopes of the data-taking links (SDK-565)

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
literal, and no literal name starts with a prefix, so a prefixed target holds a token that no
literal names.

The method does not choose that token. It runs both functions on every token value that no literal
names, up to the largest literal and one value after it. It keeps a result only when all of the
values agree. When they do not agree, the result is `token-dependent`; when a literal starts with
the prefix, it is `literal-prefix`.

**Result on M45-release.** `event_target:` and `parameter:` both declare `Any` input and `Various`
output. `scope_links()` has no gap other than `OutsideMethod`, so it is `Complete`. The scope-type
argument does not change the scopes of a link.

Forms that are not links:

| Form | Reason |
| --- | --- |
| `A.B` | A chain of targets. Each part is parsed as its own target, and `ValidateScope` checks each part in turn. |
| trailing `?` | An option on the target and on its chain (`+0x189`, passed to the chained constructor). `CEventTarget::GetScope` reads it at run time. |
| `@` in an `event_target:` value | The dynamic-flag form of the value (`ReadAsDynamicFlag`, as in `has_country_flag = name@target`). It names the saved target and is not a link. |
| `value:`, `trigger:` and other value prefixes | `CVariableValue::ReadTriggerModifierOrScriptValue` splits them on `:` and reads a number, not a scope. Their rules belong to SDK-550. |

Which saved target or parameter a value names, and whether it exists in a running game, belong
to SDK-543 and SDK-550.

### Localization contexts, commands and links (SDK-537)

SDK-537 reads the localization language (the `[Root.GetName]` bracket commands) statically on
M45-release as `Native::localization_declarations`. No prototype had read it from the engine
before; SDK-609 holds the open live question (from the closed prototype SDK-500).

**Mechanism.** `CGameApplication::PrintScriptingDocumentation` writes `localizations.log` from
`CGameText::GenerateDocumentation`. A *context* is an `ECURRENT_POINTER` value: the kind of object
that a text statement points at. The `CGameText` constructor fills three function tables, indexed
by context: the link-row getter at `+0x318`, the link function at `+0x498`, and the command-row
getter at `+0x618` (a fourth table at `+0x798` holds the property getters). A row getter such as
`GetCountryPromotionTargets(int&)` writes a count and returns 16-byte rows of a name pointer and an
index. A link function such as `PromoteCountry(void const*, CGameText&, int)` dispatches on the
index and reaches a setter that writes the new context to `CGameText+0x8`. The scope join is
`CGameText::SetScopeObject`, which switches on the scope-type value (`1 << bit`, the bits of
`Native::scopes`) and selects one context. This differs from the SDK-535 registration method: there
is no registration call and no factory.

**Method** (`engine/analysis/localization.rs`, `localization-declarations/v1`). It runs the
constructor, each row getter and each context-name call with the evaluator, and follows each link
with the new `Machine::run_paths`, which continues both sides of a decision on an unknown value and
reports every path's end. A decision on unknown flags splits the flag states that remain possible
on the path, so later decisions on the same flags stay consistent. A link's output is the set of
contexts that its paths leave; `Various` when a path hands the text to `SetScopeObject`; `Unchanged`
when every path returns without a new context; `Unresolved` when any path cannot be followed or
when some paths change the context and others do not. A context that a link or a scope type
selects is in the answer even when its tables are empty, so every reference joins. A call that the method does not follow makes
the context unknown until a setter writes it. Code never loads writable data: the rows are read
from the initial image, and the pointer slots that the fixup chain binds to another image are
unknown.

**Transfer test.** The method extension was frozen in its own commit before it ran on any
executable. Each later repair is its own commit in the SDK-537 pull request; the counts are link
rows (context, name):

| Run | After | Link rows: listed / various / unchanged / unresolved | Cause of the next repair |
| --- | --- | --- | --- |
| 1 | Freeze | — (the reader panicked) | A section shorter than 8 bytes gave an invalid pointer range. |
| 2 | Revision 1: reader repair | 45 / 14 / 0 / 43 | 5 conditional selects on unknown flags; 11 virtual calls (`blr`); 26 calls to text helpers that the reader did not decode; 1 link that selects nothing. |
| 3 | Revision 2: fork on unknown flags | 50 / 14 / 0 / 38 | |
| 4 | Revision 3: an unknown indirect call is an unknown call (the same commit adds the public API, which does not change the method) | 61 / 14 / 0 / 27 | |
| 5–7 | Revisions 4–6: follow every function that takes `CGameText&`; `mul` and traps; `ubfx`, `bfi` and multiply-add | 61 / 14 / 0 / 27 | The 24 dead-object links move from "unknown call" to "unsupported instruction" to the path limit. |
| 8 | Revision 7: a link that selects nothing is `Unchanged` | 61 / 14 / 1 / 26 | Code review: a link whose paths both select a context and leave it unchanged was reported as listed; forks kept one sample flag state for each outcome; bound slots were guessed from the top bit; a stop at a resolved indirect call was unresolved; a selected context without tables could be referenced but absent. |
| 9 | Revision 8: code-review repairs | 60 / 14 / 1 / 27 | — |

The contexts, the 245 command rows and the scope join were the same from run 2 onward: they
transferred with no change. Only link outputs needed repairs, all of them evaluator coverage, not
per-link interpretation. The run takes about 0.9 s.

**Result on M45-release.** 48 contexts; 151 command names in 245 command rows; 102 link rows. 28 contexts join scope types (`Ship (and Starbase)` joins
`ship` and `starbase`; `System` joins `galactic_object`); 20 are `Missing`: `Base Scope`, the 12
dead-object contexts, `Diplomacy`, `Building`, `Job Swap Data`, `Pop Category Swap Data`,
`Patron Relation`, `Specimen` and `Timeline Event`. Their commands and links stay in the answer.
The 14 `Various` rows are the 12 `Base Scope` promotions (`This`, `Root`, `From`, `Prev` in three
spellings) and `Target` from `Espionage Operation` and `Situation`.

**Comparison with the engine dump** (`localizations.log`, same build, 519 lines, after the result
was produced):

| Part | Answer | Dump | Difference |
| --- | ---: | ---: | --- |
| Documented contexts | 44 | 44 | Same names. |
| Command rows in them | 234 | 234 | Same names in each context. |
| Link rows in them | 99 | 99 | Same names in each context. |
| Other contexts | 4 | 0 | `GenerateDocumentation` skips contexts 3, 13, 31 and 32 (mask `0xfffe7fffdff7`), but their tables are filled: `Diplomacy` (4 commands, 3 links), `Building` (1), `Job Swap Data` (3) and `Pop Category Swap Data` (3). |
| Unscoped forms in the prose | gap | 5 names | `GetYear` and `LastKilledCountryName` are also `Base Scope` rows and `GetDate` a `Timeline Event` row. `GetMidGameDate` and `GetLateGameDate` are in no table; the unscoped forms are an `OutsideMethod` gap. |

The dump prints no outputs and no scope join, so those are not compared with it.

**Limits.** 27 link rows stay `Unresolved`, each with a gap:

- 24 links find a saved or dead object by a run-time identifier through a hash-table probe whose
  exit depends on run-time data, so the paths reach the 64-path limit: `EVENT_TARGET_0` to
  `EVENT_TARGET_9` from `Timeline Event` and from `Specimen`, `Target` and `Owner` from
  `Dead Situation`, `MainAttacker` and `MainDefender` from `Dead War`.
- `Planet` and `Ship` from `Colony` return on a path after a carrier lookup that the method does not
  follow, so the context is unknown on that path.
- `Planet` from `Deposit` selects planet or ship on some paths and returns without a new context on
  another.

`Third_party` from `Diplomacy` is `Unchanged`: `PromoteAction` handles indexes 0 and 1 and returns
for index 2. Whether a command gives useful text at run time, argument forms, formatting, scripted
localization and fallback between `Base Scope` and a typed context are not tested (SDK-609).

### On_actions, game rules and their entry scopes (SDK-538)

SDK-538 reads the callbacks that the engine calls by name on M45-release as `Native::on_actions`
and `Native::game_rules`, with the scopes that each call site supplies. No prototype had read them
from the engine; SDK-608 holds the open question (from the closed prototype SDK-496). The engine
has no documentation dump for either.

**Mechanism.** Engine code fires an on_action with
`COnActionDatabase::PerformEvent(CString const&, CEventScope&, …)`. Nearly every call site builds
the name as a stack `CString` from a text literal. A deferred command,
`COnActionCommand(CString const&, CEventScope const&, …)`, fires later from `Execute`.
`COnActionDatabase::Init` caches 14 pulse lists in database fields after a `strcmp` of each list
name, and `CGameState::MonthlyUpdate` and `YearlyUpdate` fire them with
`PerformEvent(COnActionList const*, …)`. A scope is a `CEventScope`: its type is one `EScopeType`
bit at `+0x08` (the bits of `Native::scopes`), and root, from and prev links are at `+0x30`,
`+0x38` and `+0x40`. The fresh constructors write type 0 and point every link back to the scope
itself; `IsFromFromSet` tests the type of the linked scope, not the pointer. Typed setters, such
as `CScopeObjectReference::SetCountry`, write one type constant. A game rule is a member of the
rule set: `CGameRules::CanColonizePlanet` builds a scope and calls
`CScriptedRule::Evaluate(this + 20 * 0xc0, scope, …)`. Weighted rules start at `this + 0x9cc0`,
`0x40` apart. `__GLOBAL__sub_I_game_rules.cpp` fills the rule declaration tables, and
`FindRuleDeclarationByEnum` returns the row, with its token, for a rule's enumeration.

**Method** (`engine/analysis/callbacks.rs`, `callbacks/v1`). Each direct call or tail call to an
anchor, or to one of five checked forwarders, is a site. A name pass runs a dataflow over the whole
function: it gives every literal that the name string can hold at the site (joined at branches,
replaced when the string is built again), a lookup's or a cached list's name, or the rule's offset
in the rule set. A context pass runs the new `Machine::run_paths_to` from the function entry: it
follows only paths that can reach the site, runs the scope constructors and setters on a copy, and
reads the scope at the site. A scope object stays known until its address reaches a call that the
method does not follow or other memory; from then on, every such call makes its slots unknown.
Rule names come from running the initializer and the declaration lookup. A self-link is reported
as `SelfLink`. Different contexts for one name stay separate.

**Transfer test.** The method was frozen in its own commit before it ran on any executable. Each
later repair is its own commit in the SDK-538 pull request:

| Run | After | On_actions: names / with entries | Game rules: names / with entries | Cause of the next repair |
| --- | --- | --- | --- | --- |
| 1 | Freeze | 278 / 232 | 0 / 0 | 43 names stopped at unsupported instructions; the rule initializer stopped at `dup`; no pulse list joined, because the `strcmp` stub keeps its raw name `_strcmp`. |
| 2 | Revisions 1 (instructions) and 4 (stub name) | 293 / 255 | 223 / 54 | 166 rule-set functions add the rule offset from a register; `on_monthly_pulse`'s field address is formed by a write-back and spilled to a stack slot. |
| 3 | A first form of revision 2 | 281 / 255 | 223 / 220 | 12 names were lost: a dynamic stack allocation made every stack fact unknown, and a store was taken as 32 bytes wide. |
| 4–6 | Revisions 2 (stack offsets from the entry stack pointer, store widths, write-back) and 3 (unknown stack pointer in the evaluator) | 294 / 264 | 223 / 220 | 30 names had no context: a loop with an unknown exit used the whole path budget. |
| 7 | Revision 5: loop limit | 294 / 282 | 223 / 220 | Code review: branches through a register did not reach the site; `bic` and other `b…` instructions kept their destination; a string that one path did not build kept the other path's text at a join; a scope function at the call depth, or one that passed the scope on, kept the slots; a decode failure of a rule site was counted as an on_action; rules of two families with one name would merge. |
| 8 | Revision 6: code-review repairs | 294 / 281 | 223 / 220 | — |

All repairs are evaluator or name-pass coverage; none is an interpretation of one name. The
SDK-535/536/537 parity tests passed unchanged after each revision. The two questions take about
1.0 s and 0.7 s.

**Result on M45-release.** 294 on_actions; 281 have at least one context and 207 have at least one
context with no unresolved scope. 18 names keep several contexts, such as `on_fleet_enter_orbit`,
which a fleet enters with a megastructure, a planet, a starbase or an astral rift as from. 223 game
rules (209 scripted, 14 weighted); 220 have a context and 204 a context with no unresolved scope.
These call sites were checked by hand in the disassembly before the expected files were made:
`on_game_start` and `on_monthly_pulse` (a new scope with no type), `on_leader_level_up` (country,
from leader), `on_planet_returned` (planet, from country, fromfrom country),
`can_colonize_planet` (planet, root country), `can_orbital_bombard` (fleet, from planet) and
`leader_election_weight` (weighted, leader).

**Comparison with the config** (`on_actions.cwt` and `game_rules.cwt` of the config fork, after the
result was produced; the comparison is not part of Native):

| | Both | Engine only | Config only |
| --- | ---: | ---: | ---: |
| On_actions | 276 | 18 | 114 |
| Game rules | 207 | 16 | 1 |

The 18 engine-only on_actions include `on_leaving_system_fleet`, `on_colony_transfer`,
`on_fleet_went_mia` and `on_waystation_lost`. Most config-only on_actions are fired by script
content (`fire_on_action`), are templated, or are fired at a site that this method cannot name.
The 16 engine-only rules include `can_jump_drive` and `can_scavenge_debris`; the config-only rule is
`can_build_military_station_around`. Of 221 shared on_actions with an established context, `this`
agrees with the config's `replace_scopes` for 185 and from for 182. Most differences are names, not
scopes: the config writes `carrier` where the engine passes a `colony` or `planet` scope type.

**Limits.**

- 31 sites fire a name that is not one text literal: a conditional select between two literals
  (`on_add_to_imperial_council` or `on_remove_from_imperial_council`), a name that a wrapper that
  is not pinned receives (`CArmy::PerformBuildingOnAction`), or a name built at run time
  (`_queued`). One list site fires a list that an object holds.
- 13 on_actions have no context: 9 reach the path limit, 1 stops at a floating-point instruction,
  2 are not reached from their function entry, and `on_press_begin`'s command builds its own scope.
  (SDK-540 added float immediates, `scvtf`, `fmul` and `fcvtzs` to the evaluator.
  `on_war_participant_leaves_early` then went past its floating-point stop and reached the path
  limit. It still has no context.)
- 50 on_actions have only unresolved contexts. Most reuse one scope for several firing calls: the
  first call receives the scope, and the method cannot show that the event system leaves its type
  and links unchanged, so the later sites are unresolved (the pulse lists after
  `on_yearly_pulse`, `on_leader_death`, `on_planet_surveyed`). Some fill the scope with a helper
  whose type depends on a run-time value (`CDepositHolderRefCaster::FillEventScope`).
- 3 declared rules have no call site that the method follows, and 11 have only unresolved contexts.
- What the event system does with a self-linked root or from, the prev chain, events and their
  `push_scope`, pre_triggers, and on_actions that content defines are not tested (SDK-608).
