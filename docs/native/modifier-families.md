# Modifier families

These notes are on the modifier families that content generates and on the loaded modifier table.
The [discovery index](discovery.md) lists the other method pages.

## Modifier families (SDK-540)

SDK-540 reads the modifier names that a registry's database generator registers for each item, as
`Native::modifier_families(registry)`. The SDK-498 prototype traced five templates by hand
([brief](modifier-family-prototype.md)). This method finds them in the executable, with three
other registries.

**Mechanism.** Each `<Database>::GenerateModifiers()` loops over the database's pointer array
(`+0x48`, count `+0x54`, from the template registry layout) and calls
`CModifier::TryAddDynamicModifier` with the name in `x1` and the category mask at `[sp]` (recipe
`dynamic_modifier_category_offset`). The method runs the generator on a zeroed database that holds
one item. The item's memory is unknown, except its key. A string model (`families/strings.rs`)
follows `CString::CString(char const*)`, both `operator+=`, `PdxStrFmt<N>` (`%s` from 8-byte stack
slots; the name keeps at most N−1 bytes), the string allocator, `operator new[]`, `strlen`,
`memmove` and `memcpy`. It writes the real text, so the code takes the branches that it takes for
that text. It also labels each text address with the parts of the text. A call outside the model
that receives a string makes that string unresolved. A call that never returns
(`__stack_chk_fail`, `_Unwind_Resume`, the `__throw_` functions) ends its path; the path does not
count for the condition.

- **Key storage.** The item constructor `<Item>::<Item>(int, CString const&)` copies the key argument
  inline. The method runs it with a labelled long key and finds the buffer pointer in the item.
  Buildings, districts, megastructures, situations and zones store the key at `+0x10`. Bypass stores
  its id at `+0x10` and the key at `+0x18`. The constructors and generators copy string objects
  through `q` registers from temporaries that are not fully written, so a copied flag byte can be
  unknown. The model reads a string by its labels, not by the flag byte.
- **Two key forms.** The engine branches on the key's form. The generator runs with a 32-byte key
  (in a buffer) and with an 8-byte key (in place), and both runs must give the same name and mask.
- **Condition.** A family is `Always` when no path fails, some path returns, and every returned
  path reaches the call. A branch on an unknown item field makes a path that skips the call.
- **Evaluator.** `fmov` with a float immediate, `scvtf`, `fmul` and `fcvtzs` (the generators grow
  an array by ×1.5), the link register on `bl` and `blr` (so the model knows the call site), and
  `Machine::reserve` (memory that no path has written).

**Result on M45-release** (about 0.8 s for each registry):

| Registry | Template | Tags | Condition | Limit |
| --- | --- | --- | --- | --- |
| `common/buildings` | `planet_{key}_build_speed_mult` | Colony | Always | — |
| `common/districts` | `planet_{key}_build_speed_mult` | Colony | Always | — |
| `common/zones` | `planet_{key}_build_speed_mult` | Colony | Always | — |
| `common/megastructures` | `megastructure_{key}_build_speed_mult` | Megastructures | Always | — |
| `common/bypass` | `{key}_empire_windup_mult` | Countries | Always | 127 |
| `common/bypass` | `{key}_ship_windup_mult` | Ships | Always | 127 |
| `common/bypass` | `{key}_megastructure_bypass_windup_mult` | Megastructures | Always | 127 |
| `common/situations` | `{key}_max_progress_add` | Countries | Unresolved | — |
| `common/situations` | `{key}_max_progress_mult` | Countries | Unresolved | — |

The nine calls include the five hook sites of the prototype (`0x1000df770`, `0x100431bdc`,
`0x1000fefec`, `0x1000ff05c`, `0x1000ff0c4`). In both situation runs, one returned path skips both
calls. That path is the item gate `ldrb w8,[x23,#0x508]; cbz` before the calls. No path fails. In
bypass, only the stack-check path is ignored. Every answer is partial: 64 sites are not joined to a
registry. They are the other 53 `TryAddDynamicModifier` and `AddDynamicModifier` calls and the 11
runtime-token `AddDefinition` calls. SDK-566 joined 44 of them (below).

**Freeze and revision.** The method was frozen (`bf44567`) before its first run on the executable.
Post-freeze revision 1: the first run gave both situation families as paths-disagree. With a short
key, the generator copies the key in place into a stack string, and that copy labels the object.
The model then made the object a long string, but the object's own label stayed. The short-key run
therefore read the bare key. The model now removes that label. A regression test fails without the
change. Post-freeze revision 2, from review: a `memmove` or `memcpy` of
unknown length now makes the strings that it receives unresolved, as another call does. The M45
result did not change.

**Match rate.** A development script (`.local/sdk-540/measure.py`, `results.json`) applies the
answer to item keys and compares with the loaded inventory (`modifiers.log`) of the two SDK-498
live runs on the same build:

- Keys that the SDK-498 hooks observed (buildings 499, districts 148, bypass 11): in each run,
  680 of 680 observed registrations equal the returned template applied to the observed key. All
  680 names are loaded with the returned tags, including the renamed `sdk498_*` items.
- Keys from the top-level definitions of the installed content files (not engine observations):
  megastructures 164 of 164 and zones 146 of 146 are loaded with the returned tags. For situations,
  1 of 90 is loaded for each family, which agrees with the item gate and the `Unresolved`
  condition.
- The 45,585 loaded names are 571 static (they join `modifiers()`), 992 explained and 44,022
  unexplained. No loaded name comes from the templates of two registries. The district
  `sdk498_district_*_max_add` and `*_max_mult` names stay unexplained. No name is assigned by
  similarity. SDK-564 returns the loaded inventory and owns the classification of each entry.
- The SDK-540 session that selected the six registries returned `Unsupported` for all six, with
  the reason "initial loader did not run before the session paused". SDK-573 established the
  cause (below): that session paused at the worker's deadline, not after the loaders. A fresh
  registry session observes all six initial loads, complete, about 24 seconds after launch. The
  earlier note that "each initial loader runs after the session pauses" was wrong.
  SDK-567 removed the shared `+0x10` key offset. A constructor probe now
  establishes the key storage of each selected registry before the worker can read its items.
  On M45-release it established 148 of 164 named registries. `common/bypass` and
  `common/map_modes` use `+0x18`; the other established keys use `+0x10`. Eleven registries have
  no matching constructor symbol and five constructor runs do not establish key storage. Those
  16 refuse an item read with a reason if their loader runs; no item name is read from an
  unestablished offset. An authored constructor test covers `+0x18`. On M45-release,
  `common/map_modes` returned eight live keys equal to its eight top-level source keys in one
  session; another session paused before its loader ran and returned `Unsupported` (SDK-573
  repeated the session three times; each returned the eight keys, complete, about 22 seconds
  after launch). The method gives no name from an unestablished offset in either case. A session
  that pauses at the documentation point (SDK-564, below) observes the initial loads of
  `common/buildings` (498 items) and `common/zones` (146).

**Why the SDK-540 session was not loaded (SDK-573).** A registry session has two pause paths.
The worker holds the game when every registry whose hook was active has returned from its
initial loader, or, when the worker's deadline (the startup budget less a margin; 170 seconds of
the default 180) passes first, it stops the game where it is and holds it there. Only the second
path pauses with no loader returned, and a registry with an active hook that the game never
reached is then `NotLoaded`. The SDK-540 session's recorded files date it at 186 seconds from
start to answers: the 170-second deadline plus launch setup. Its `Unsupported` answers therefore
came from the deadline, and the loaders were not reached in that time. A fresh session with the
same six registries (2026-09-23, M45-release) entered and returned all six loaders on the launch
thread in the order bypass, buildings, zones, districts, situations, megastructures, paused after
registry initialization about 24 seconds after launch, and returned 10, 498, 146, 147, 90 and
164 complete items, in that order. Why the earlier game did not reach the loaders within 170 seconds is not
established: the session's work directory was removed after its confirmed disposal, and the
answers did not say which path paused the session. That is the repair: the worker's pause record
now carries its cause (`loaders-returned`, `content-loaded` or `deadline`), the reducer refuses a
loaders-returned pause that omits an active loader, and a `NotLoaded` answer says whether the
startup deadline, the other selected loaders or the content load ended the session. A live case
(`generator_registries`) requires the six complete answers, and `nonstandard_key` now requires
the eight `common/map_modes` keys.

**Not in SDK-540.** Generator classes and shared helpers (SDK-566, below); the classification of each
loaded entry (SDK-564); a condition named by its item field; engine behavior for a name longer than
the formatter keeps; tags that a later registration of the same name gives.

## The loaded modifier inventory (SDK-564)

`GameOptions::loaded_modifiers` and `Game::loaded_modifiers` (`loaded-modifiers/v1`) return the
modifier table as the engine holds it after all content loads.

**Documentation point.** `CGameApplication::InitGame` calls `CModifier::LogDefinitions()`
unconditionally (`0x1005e96ec` on M45-release), after the trigger, effect and event post-inits and
before `PrintScriptingDocumentation`. The worker hooks its entry on the launch thread and reads the
table when it returns. The session then pauses there (`GameReadiness::PausedAfterContentLoad`),
about 20 seconds after launch; the read takes about 0.4 seconds.

**Layout (M45-release, derived by SDK-572).** `LogDefinitions` walks
`CPdxModifier<…>::_Definitions`, a `CPdxArray` with its data at `+0x8` and its count at `+0x14`,
of 0x98-byte definitions: the lexer token at `+0x78` and the category mask at `+0x84`. The beta
prototype's offsets are unchanged. `AddDefinition` indexes the array by `ModifierType`. Each
definition's own strings are localization keys (`MOD_COUNTRY_SCAVENGE_DEBRIS_MULT`), not names; the
name is `CStaticLexer::GetString(token)`, element `token` of the lexer's lookup. The lookup is a
`CPdxArray<CString>` of 0x28-byte elements in unnamed globals at `0x103796d70`. `GetString` first
rebuilds it when its count differs from the size at `0x103796d88`. At the return of
`LogDefinitions` the lookup is current, because the function named every entry; the worker requires
the two counts to be equal. `LogDefinitions` and `_Definitions` are resolved by symbol; the lookup
has no symbol.

SDK-572 derives the header, entry stride, token and category fields from the documentation
loop, and the lookup globals and stride from `GetString`. Candidate instruction shapes are
checked with the evaluator: zero, one and two entries, two opposite sets of offset labels,
and lookup tokens 0, 1 and 7 with matching and mismatching counts. Changed headers, strides,
fields and globals are authored test inputs. Unknown code or calls, transformed fields,
inconsistent iteration and a different array-header shape refuse the operation before launch.
No modifier-table address or offset remains in a binding group. The shared CString layout
still comes from the registry binding.

The exact-build parity run established one complete table layout, zero partial layouts and
zero failures. All eight values equal the earlier disassembly findings above. This method
reads the one documentation loop and its lexer reader; it has no registry-specific cases.
The live controls pass: 45,578 entries match the engine's `modifiers.log`, and worker loss
and a missing registry hook keep their existing failure behavior.
Cold lexer initialization, lookup rebuilding itself, and logger code after the loop are
outside the method.

**Generator registries.** The join applies each `modifier_families` template to the registry's
loaded keys. The worker reads them from the database at the same point:
`TGameDatabase<Db>::_pInstance` (resolved by symbol), the database's directory, which must equal
the registry, and the items at the established key offset (SDK-567). The registries are those whose
database has `<Db>::GenerateModifiers()`; SDK-566 extends the set to every registry that returns a
family (20 on M45-release).

**Transport.** The worker writes the table and the keys once to `loaded-modifiers.json` (at most
32 MiB; the M45-release table is about 3.2 MiB) and puts its size and SHA-256 in its stream, followed by a
terminal. The supervisor accepts the table only when the stream has no hole before the terminal,
the hook was active before resume, the documentation was entered and returned on the launch thread,
the file matches, and every name is distinct.

**Result on M45-release.** 45,578 entries, the SDK-488 count. The names, their order and every tag
list equal the `modifiers.log` that the engine wrote in the same session (the live test compares
them; Native does not read the log). The five re-registered names have their loaded tags:
`terraforming_cost_mult` Planets and AI Economy; `starbase_shipyard_build_cost_mult` Starbases and
AI Economy; the two other `starbase_shipyard_*` names and `gdf_ship_alloys_cost_mult` the six ship
and station tags and AI Economy.

| Class | Entries | SDK-540 measure (SDK-498 runs, 45,585) |
| --- | ---: | ---: |
| Declared (joins `modifiers()`) | 571 | 571 |
| Generated by a returned family for a loaded item | 987 | 992 |
| Unexplained | 44,020 | 44,022 |

The differences are the seven names of the SDK-498 private mod: five explained (one building, one
district, three bypass names) and two unexplained (`sdk498_district_*_max_add`, `*_max_mult`). Every
declared name is loaded, and no loaded name is marked declared without a declaration. No name has
two families. The match rate, measured through the API with `examples/loaded-modifiers.rs`:

| Registry | Template | Loaded items | Names loaded |
| --- | --- | ---: | ---: |
| `common/buildings` | `planet_{key}_build_speed_mult` | 498 | 498 |
| `common/districts` | `planet_{key}_build_speed_mult` | 147 | 147 |
| `common/zones` | `planet_{key}_build_speed_mult` | 146 | 146 |
| `common/megastructures` | `megastructure_{key}_build_speed_mult` | 164 | 164 |
| `common/bypass` | each of the three windup templates | 10 | 10 |
| `common/situations` | `{key}_max_progress_add`, `{key}_max_progress_mult` | 90 | 1 each |

The situations result agrees with the item gate and the `Unresolved` condition of SDK-540. The
answer is partial: the 64 unjoined generation sites (SDK-566) and the 44,020 unexplained names are
gaps. SDK-566 (below) explains 5,432 names and leaves 39,576 unexplained.

**DLC archives.** No DLC archive of the installation holds a script file. 22 of the 32 archives hold
only `music/` and `sound/` files. The other ten (`dlc002`, `dlc004`, `dlc013`, `dlc015`, `dlc029`,
`dlc030`, `dlc033`, `dlc035`, `dlc039`, `dlc042`) are empty 22-byte zip files: these are the ten
archives that failed to mount with "unsupported" in the SDK-488 run. Native's sessions set a private
`HOME`, so the game finds no Steam client, and they print no mount failure. Mount state therefore
cannot change the modifier table, and the prototype's failures do not explain any difference. The
answer states its content as `LoadedContent::Installation`, or the fixture's registry and files.

**Config probe (development check, outside Native).** A throwaway script
(`.local/sdk-564/cwt/compare.py`) compared the recorded answer with `config/modifiers.cwt` of
cwtools-stellaris-config at `8574760`, to look for a modifier that the config knows and the table
lacks. It compared names only. None is missing:

- All 572 literal names are loaded. Two differ only by case: the config and the content write
  `biological_logistic_growth_mult` and `lithoid_logistic_growth_mult`; the table holds
  `BIOLOGICAL_logistic_growth_mult` and `LITHOID_logistic_growth_mult`. A generated name keeps the
  case of its item key (206 loaded names contain upper case; no two differ only by case).
  `CStaticLexer::AddDynamicToken` compares lower-cased characters when it looks a token up; that
  content reads resolve the lower-case spelling through the same lookup is not established here.
- The 179 templates, expanded over the installed content keys of each `<type>` and the values of
  each enum, give 37,565 names; 51 are absent. 37 come from expanding a subtype over its whole type
  (archetypes with `uses_modifiers = no` or `robotic = yes`, leader classes without
  `leader_capacity`, patrons without `add_modifier = yes`) and 3 from the script taking
  `random_list` as a planet class. The other 11 are `job_<job>_automated_workforce_mult` for the
  exactly 11 jobs with `can_be_automated = no`. The engine gates that family on the job field;
  the config template has no condition.

**Not in SDK-564.** Modifiers that the engine adds after the documentation point, such as during a
game; the generation sites outside database generators (SDK-566); where a modifier takes effect.

## Modifier families from post-read code and shared helpers (SDK-566)

SDK-566 joins the generation sites outside database generators to their registries.
`modifier_families` is now `modifier-families/v2`. The public types did not change.

**Roots.** Nearly every unjoined site is one to three direct calls below a function that the
engine runs for the items of one content database. The method calls these functions *roots*:

| Root | Receives | Condition |
| --- | --- | --- |
| `<Db>::GenerateModifiers()` (SDK-540) | the database | can be `Always` |
| `TSingleObjectGameDatabase<Db, Owner, …>::PostReadInit()` | the database; it loops over `+0x48`/`+0x54` and calls `<Owner>::PostReadInit()` for each item | can be `Always` |
| `<Owner>::InitPostRead(…)` | one item | never `Always`: the engine calls it through a virtual slot, and that it runs for every item is not established |

SDK-575 (below) replaced the last row: an item root can be `Always` when the method establishes
that the engine runs it for every item that the database loads.

Generator classes (`CTechnologyModifierGenerator`, `CSpeciesClassModifierGenerator`, …) are stack
objects that the root builds. The root stores the item at `+0xb8` and calls the `Create*` methods
directly, not through the vtable. The post-read function of every other content class
(`CStrategicResource`, `CPlanetClass`, `CZoneType::CSerializer`,
`CEconomicCategory::CTriggeredModifierTable::CSerializer<N>`) is a root with no named registry.

**Joins** (`families/joins.rs`). From the function that contains each generation call, the method
climbs direct calls and tail calls, at most four, to the first root of each chain. A chain is a
*context*. A chain that meets no root is not a context: this ignores the reload path
(`ReadExistingEntry`) and the non-virtual thunks, which hold copies of `InitPostRead`. A context is
joined when its root belongs to a named registry and no function on it takes a content object
that no named registry owns. The parameter types of the demangled name show that; the one case is
`CEconomicCategory::FillResourceModifierMatrix(CStrategicResource const&, …)`. A site is joined
only when every context is joined. Each other site keeps one reason, in this order:
registration function, unnamed matrix input, unnamed content, no root.

**Run.** Each root runs as in SDK-540: one item, two key forms, the string model. The evaluator
now *enters* a call (`Call::Enter`). The path runs the callee with a return stack of its own, and
a tail call out of the decoded code returns from the entered frame only. The run enters the
functions on the joined contexts, and *composers*. A composer is a leaf, such as
`CString::GetSize` or `GetNameFromEnum`, or a function that composes text with the modelled
string functions, such as `CModifierGeneratorBase::BuildModifierTag`. Calls to its own `.cold`
parts, which only unwinding reaches, are allowed. A function that only calls memory and copy
functions is not a composer. The first version entered
`basic_string::__assign_external`, whose store through an unknown buffer pointer made every known
byte unknown. That removed the SDK-540 families of districts, zones and megastructures until the
rule changed. Every registration a path makes is recorded with the calls that the path is inside.
A family is one name at one chain of calls; conditions are computed per root, and a template that
two roots give is `Always` when one root establishes it.

**String model additions.** `CString::operator+=(CPdxStringView)` (the view is its text only when
its length equals the text's length), `operator+=(char)`, `Reserve` (no effect), and
`basic_string::__assign_external`, which only reads its text. Before this, an unfollowed
`__assign_external` of the key into `CJobType+0x920` made the key unresolved. An all-zero string
object is the empty text. The model writes unresolved text as the empty text so that the code
runs on (its label keeps it unresolved in every name). From review: after that point a path takes
the branches of an empty text, which the real text may not take, so only the registrations that
the path made before it count for `Always`. Leaving the length unknown instead removed the
economic-category families: the inline copy of `"mod_"` + an unknown field then stores through an
unknown address, which makes the run's whole memory unknown.

**Definition table.** `GenerateFrom(base, prefix, key, suffix)` reads the flags and the category
mask of `_Definitions[base]` (mask at `+0x84`, the SDK-572 layout). `base` is a constant, such as
`0x83` for the district `max_add`. The modifier method now records the type (`w1`) of each direct
`AddDefinition` call. The run holds a definition array with the mask of each declared type, and
room for 256 more. The registration writes a new type into its `ModifierType&` argument, as the
engine does. The type is the path's own: its count of registrations, so forked paths never share
one (from review). The stub first left the engine's sentinel `0x23b` there. The code after the call
then read `_Definitions[0x23b]`, which was outside the held array; it ended in the item and made
the key unresolved.

**Assumptions.** An item's key does not change after its constructor. A store to an unknown
address, or a call that the model does not follow, leaves the key object, its text and its label.
This recovered the anomaly families (an unfollowed
`NLocalizationUtil::CheckLocalizationExists(key, …)`) and those of councilors, species archetypes
and ship sizes, which store through unknown item fields before they compose names. A store to an
unknown address also does not change the held definition masks. Both are authored tests with
negative controls: without the protection, the test fails.

**Key storage.** The item constructor may take the key by value: `<Owner>::<Owner>(int, CString)`
has the same ARM64 call shape as `const&`. With it, the constructor probe establishes 156 of 164
named registries (148 before). Four have no matching constructor and four runs do not establish
key storage. This also serves live key reads (SDK-567).

**Accounting on M45-release.** 73 sites: 62 dynamic calls (61 `TryAddDynamicModifier`, one
`AddDynamicModifier`) and 11 runtime-token `AddDefinition` calls. 9 were joined by SDK-540; 53
are joined now. The `UnnamedDeclaration` gap counts the other 20 by reason:

| Reason | Sites | Code |
| --- | ---: | --- |
| Inside the registration function | 1 | the `AddDefinition` in `CModifier::TryAddDynamicModifier` |
| Unnamed matrix input | 1 | `CEconomicCategory::FillModifierTable<false>` (economic category × strategic resource) |
| Unnamed content | 5 | `CModifierGeneratorBase::GenerateFrom` (also called by `CStrategicResourceModifierGenerator`), `CDatabaseModifierGenerator<CStrategicResource>::Generate`, `CPlanetClassModifierGenerator::CreateModifier`, `CZoneTypeModifierGenerator::CreateMaxModifier` (from `CZoneType::CSerializer`), `CEconomicCategory::FillModifierTable<true>` (also from the triggered-table serializers) |
| No root | 13 | ten `AddDefinition` calls in `CShipClassModifierHelper::Init` (from `CModifier::InitDefinitions`: engine ship-class enum, before content), `CWeaponTagModifierHelper::CreateModifierType` (from `CWeaponTagDatabase::InitInstance`), two in `CLeaderClass::CreateStartingAgeModifiers` (no direct caller) |

`common/strategic_resources` and `common/planet_classes` have custom loaders and are not named
registries (SDK-551). Decided with Jackson on 2026-09-24: their sites stay gaps and `NamePart`
does not change. SDK-551 has an acceptance criterion for this case.

**Result on M45-release** (22 registries, 50 families). The last two columns are from one
live `examples/loaded-modifiers.rs` session: the registry's loaded items, and how many of the names
that the template gives for them are loaded.

| Registry | Families (template) | Root | Items | Loaded names |
| --- | --- | --- | ---: | ---: |
| `common/anomalies` | `{key}_research_speed_mult` | item | 327 | 327 |
| `common/buildings` | `{key}_max` | item | 498 | 0 |
| `common/country_types` | `damage_vs_country_type_{key}_mult` (limit 511) | item | 101 | 101 |
| `common/districts` | `{key}_max_add`, `{key}_max_mult` | item | 147 | 147 each |
| `common/economic_categories` | `{key}_produces_mult`, `_upkeep_mult`, `_cost_mult`, `_logistics_mult` (tags unresolved) | item | 269 | 46, 53, 35, 1 |
| `common/espionage_operation_types` | `{key}_speed_mult`, `_skill_add`/`_mult`, `_difficulty_add`/`_mult`, `hostile_{key}_difficulty_add`/`_mult` | item | 27 | 27 each |
| `common/ethics` | `pop_{key}_attraction_mult` | item | 17 | 17 |
| `common/governments/councilors` | `{key}_exp_gain` | database loop | 179 | 179 |
| `common/patrons` | `add_attunement_{key}`, `{key}_attunement_mult` | database loop | 14 | 5 each |
| `common/pop_categories` | `pop_cat_{key}_happiness`, `_political_power`, `_bonus_workforce_mult` | item | 24 | 24 each |
| `common/pop_jobs` | `job_{key}_add`, `_per_pop`, `_per_crime`, `_max_workforce_add`/`_mult`, `_automated_workforce_mult`, `pop_{key}_workforce_mult`, `pop_{key}_bonus_workforce_mult` | item | 366 | 365 each; automated 354 |
| `common/resolution_categories` | `{key}_vote_strength_mult` | item | 37 | 37 |
| `common/scripted_modifiers` | `{key}` (tags unresolved; `Always`) | database loop | 133 | 133 |
| `common/species_archetypes` | `{key}_species_trait_points_add`, `_picks_add`, `_pop_happiness`, `_logistic_growth_mult`, `_bonus_pop_growth`, `_bonus_pop_growth_mult` | item | 6 | 4, 4, 2, 2, 2, 2 |
| `common/technology/category` | `category_{key}_research_speed_mult`, `category_{key}_draw_chance_mult` | database loop | 13 | 13 each |

The six SDK-540 registries keep their nine families. Districts add the two maximum families that
SDK-540 left unexplained. The loaded table has 45,578 entries: 571 declared, 5,432 generated by a
returned family (987 before), 39,576 unexplained. Every family's names are loaded for at least one
item, except `{key}_max` of buildings. `CBuildingType::InitPostRead` registers it only when bit 2
of `+0x17a8` is set, and no loaded building sets it. Its condition is `Unresolved`, as it should
be. The other partial rates belong to families whose condition is `Unresolved`: the code checks
an item field first. The SDK-564 config probe names the content side: economic categories generate
with `generate_mult_modifiers`, 11 jobs have `can_be_automated = no` (12 jobs lack the automated
family here), and some archetypes and patrons do not use modifiers.

Tags come from the category mask of the registration. Economic categories and scripted modifiers
pass a mask from an item field (for economic categories, `+0x128`, the content's
`modifier_category`), so their tags are unresolved.

**Gaps that remain in joined registries.**

- `common/leader_classes`: the eight families compose from `+0x1f0`, a string field of the item
  that is not the key. The method does not name it.
- `common/districts`: two paths per call use the object at `+0xea0` instead of the item, when a
  virtual call on it returns non-zero; those names are not the item's.
- `common/ship_sizes`: `CShipSizeModifierGenerator::CreateModifier(ModifierType)` lower-cases the
  base definition's localization key and replaces `mod_ship_` with `shipsize_{key}_`; the model
  does not follow `ToLower` or `Replace`. The other call is not reached within the path limit.
- `common/espionage_operation_categories`: no item constructor symbol, so no key storage.
- The districts, pop categories and espionage types have one failed name per path that reads an
  unknown object; these are counted by reason in the answer.

**Tests.** Authored inputs cover entering, nested entering, a fork inside an entered call, tail
calls in entered frames, the item root with a generator field, views, the base-type mask, a loop
that registers two families at one call, per-root conditions, and the negative controls: a view
of the wrong length, a part from another object, a root that is not entered, and the key after an
unknown store. `joins.rs` tests a helper with a named and an unnamed caller, a matrix input, and
the depth limit. The parity test (`tests/expected/m45/modifier-families.json`) holds the 22
registries. The live `loaded_modifiers` case asserts the attribution of `job_miner_add`,
`district_mining_max_add` and `category_computing_research_speed_mult`.

**Not in SDK-566.** Registries with custom loaders (SDK-551); a key part from a second named
registry (none on M45-release); conditions of item roots; content fields other than the key
(leader classes); names composed from another modifier's name (ship sizes); the tags of a declared
base modifier after content registers it again.

## Item post-read code for every loaded item (SDK-575)

SDK-575 establishes, for each registry with an item root, whether the engine calls
`<Owner>::InitPostRead` for every item that the database loads. `modifier_families` is
`modifier-families/v3` (`families/loading.rs`). The public types did not change.

**Engine code (M45-release).** `TSingleObjectGameDatabase<Db, Owner, …>::LoadFromReader` reads the
statements of a file in a loop. For a new key it allocates the item, calls the key constructor
and, unless the database's word at `+0x70` is 1, calls slot `0x20` of the item's vtable at `+0x38`
(the item's `CPersistent` base), then inserts the item into the array at `+0x48`/`+0x54`. Some
instances move the new-key code into `ReadNewEntry`. A key that exists goes through the
database's vtable to `ReadExistingEntry`, which destroys the item, zeroes it, constructs it again
and makes the same gated call. Slot `0x20` is `CPersistent::Read`. It calls
`ReadWithoutInitPostRead`, then slot `0x30` (`InitPostRead()`) and, as a tail call, slot `0x38`
(`InitPostRead(CString const&, int, int)`), on every path. In each owner's vtable those slots
hold a thunk of the owner's override, which adjusts `this` by `-0x38`. For `CBuildingType` the
thunk is `sub x0, x0, #0x38; b CBuildingType::InitPostRead`. For `CCountryType` the non-virtual
thunk holds a copy of the whole body. Only the database constructors write the `+0x70` word:
0 for every registry here, 1 for `CJobTagDatabase` and `CTraitTagDatabase`.

**Method.** The functions that call a constructor of the owner, directly or through a
constructor that delegates to it, must be functions of the database's classes (`<Db>::…` and
`TSingleObjectGameDatabase<Db, Owner, …>::…`) or the null object's initializer
(`TPdxNullObject<Owner>::Initialize`, which constructs the null object with index -1 and an empty
key in its own storage). No other function may form an address point of the owner's vtables:
such code can construct an item inline. The binding decodes each function with an `adrp` of a
point's page and reads it in address order (`adrp`, `add` of an immediate and `mov` carry a
value; any other write clears it). A function that does not decode is read as words with
`adrp` and `add` only. On M45-release only the owners' constructors and destructors form the
address points, and the `resolution_categories` readers, which construct inline. Each function of the database's classes runs from its entry with
fresh registers. Its database is the memory that the database constructor leaves, with an item
array of unknown length, and its pointer arguments are unknown memory. The run handles calls as
follows:

- An allocation returns new memory.
- An owner constructor gives its object the address points of the owner's vtable group,
  parsed from `vtable for <Owner>` (ABI: a complete object after its constructor).
- The run enters the functions in the group's slots.
- A thunk of an item root counts as the root when `this` is the item's subobject whose vtable
  holds the thunk.

States join at loop heads (`Machine::run_paths_joining`): each head keeps the facts that every
arriving path knew with the same values, and the flag states that any of them allowed. A path
that the kept facts cover ends there, and the other paths continue from the kept facts, so the
run covers every pass of a loop. The two sides of a branch on unknown flags at a loop head
resume there without a second join. The call is
established when every item that a returned path constructed received every item root. The
item families then follow the database-root rule of SDK-540/566 (every returned path of both
key forms registers the family; failed paths and paths that assumed text do not count).

**Assumptions.** The null object is not an item of the database. A store to an unknown
address, or a call that the run does not enter, changes neither the database's fields other
than its item array nor an item's vtable pointers. A thunk runs its root, also when it holds a
copy of the root's body.

**Freeze and revisions.** An adversarial plan review found four holes before the first
executable run. The first plan followed one pass of each loop, so a later pass that skipped the
read went unseen. It let one root discharge the others. It excluded constructions outside the
database's classes without accounting for them. It missed a root that a thunk reaches by a
branch inside the decoded code. Each hole is now an authored test with a negative control. Two
revisions followed the first M45 run. With per-path joins, the loaders exceeded 64 paths; joins
are now shared between paths, and a path that the joined facts cover does not count toward the
limit. `ReadExistingEntry` of espionage types has a backward branch that is not a loop, and a
covered path ended there before the covering path read the item. Only returned paths are now
checked; a covered path's items are the covering path's, which carries the same labels.
Two reviews of the implementation found six more holes, each now an authored test:

- The two sides of a flag split at a loop head ended as covered by the arrival that split.
- The joins ignored the flag states that a path allowed.
- A loader received the registers that the database constructor left, such as a zero `bool`.
- A thunk was credited at any subobject offset.
- Code that constructs an item inline was not accounted for.
- A loader's new memory could have the address of memory that the database constructor reserved.

The review also named the database assumption above; it stays an assumption, as the key
assumption of SDK-566 does. After these changes the M45 result is unchanged, except the reason
for `resolution_categories`.

**Result on M45-release.**

| Registry | Per-item call | Families now `Always` | Live (items, loaded names) |
| --- | --- | --- | --- |
| `common/anomalies`, `buildings`, `country_types`, `districts`, `economic_categories`, `espionage_operation_types`, `ethics`, `ship_sizes`, `species_archetypes` | established | none: a path skips each registration | unchanged from SDK-566 |
| `common/pop_categories` | established | `pop_cat_{key}_happiness`, `_political_power`, `_bonus_workforce_mult` | 24 items, 24 names each |
| `common/pop_jobs` | not established: `CJobTypeDatabase::CJobTypeDatabase()` constructs one item and inserts it without calling `Read` | none | 366 items, 365 names each (automated 354) |
| `common/resolution_categories` | not established: its `ReadExistingEntry` and `ReadNewEntry` construct the item inline (and call `CPersistent::Read` directly) | none | 37 items, 37 names |

The `pop_jobs` gap agrees with the live measure: the database constructor's item is the one item
without generated names. The families that stay `Unresolved` in established registries have
these causes:

- An item-field gate: `CCountryType::InitPostRead` registers only when the field at `+0x580`
  equals `0x7fffffff`. Every loaded country type has that value (101 of 101).
- The buildings `{key}_max` gate on bit 2 of `+0x17a8`, which no loaded building sets (0 of 498).
- Root runs that exceed the path limit of SDK-566: districts, espionage types, ethics, species
  archetypes, economic categories.

Anomaly root runs have no failed path, but half of their returned paths skip the registration.
Why was not traced.

**Not in SDK-575.** Joining states in the item-root runs themselves, which would remove their
path-limit failures; following inline construction; a copied thunk's own family run.
