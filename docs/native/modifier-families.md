# Modifier families

`Native::modifier_families(registry)` (`modifier-families/v3`) gives the modifier names that a
registry's code registers for each item. `Game::loaded_modifiers` (`loaded-modifiers/v1`) gives
the modifier table after all content loads, and joins each entry to a declaration or a family.
The module comments of `engine/analysis/families.rs`, `families/joins.rs`, `families/loading.rs`,
`families/strings.rs` and `session/loaded_modifiers.rs` describe the methods. This page holds the
engine facts, the M45-release result, the gaps and the pitfalls. The
[discovery index](discovery.md) lists the other method pages.

## Engine code (M45-release)

### Generation calls

- `CModifier::TryAddDynamicModifier` receives the name in `x1` and the category mask at `[sp]`
  (recipe `dynamic_modifier_category_offset`). `CModifier::AddDynamicModifier` is the other
  dynamic call.
- `CPdxModifier<…>::AddDefinition` receives the token in `w0`, the `ModifierType&` in `w1` and
  the mask at `[sp,#4]`. It writes the new type into its `ModifierType&` argument. Before the
  call, the engine leaves the sentinel `0x23b` there.
- `CModifierGeneratorBase::GenerateFrom(base, prefix, key, suffix)` reads the flags and the
  category mask of `_Definitions[base]` (mask at `+0x84`). `base` is a constant, for example
  `0x83` for the district `max_add` family.
- `PdxStrFmt<N>` keeps at most N−1 bytes of a name. This gives the name limits in the result
  (127 for bypass, 511 for country types). What the engine does with a longer name is not known.

There are 73 generation sites: 61 `TryAddDynamicModifier` calls, one `AddDynamicModifier` call
and 11 `AddDefinition` calls whose token is not a literal. Ten of the 11 are in
`CShipClassModifierHelper::Init`, and one is in `TryAddDynamicModifier`.

### Roots

A root is a function that the engine runs for the items of one content database.

| Root | Receives | Can be `Always` |
| --- | --- | --- |
| `<Db>::GenerateModifiers()` | The database. It loops over the pointer array at `+0x48` (count `+0x54`). | Yes |
| `TSingleObjectGameDatabase<Db, Owner, …>::PostReadInit()` | The database. It calls `<Owner>::PostReadInit()` for each item. | Yes |
| `<Owner>::InitPostRead(…)` | One item, through a virtual slot | Only when the per-item call is established (below) |

- Generator classes (`CTechnologyModifierGenerator`, `CSpeciesClassModifierGenerator`, …) are
  stack objects that the root builds. The root stores the item at `+0xb8` and calls the
  `Create*` methods directly, not through the vtable.
- The post-read functions of `CStrategicResource`, `CPlanetClass`, `CZoneType::CSerializer` and
  `CEconomicCategory::CTriggeredModifierTable::CSerializer<N>` are roots with no named registry.
  `common/strategic_resources` and `common/planet_classes` have custom loaders and are not named
  registries (SDK-551). Their sites stay gaps; `NamePart` has no part for them.
- `CEconomicCategory::FillResourceModifierMatrix(CStrategicResource const&, …)` takes a content
  object that no named registry owns. Its names combine two registries' keys.
- The reload path (`ReadExistingEntry`) and the non-virtual thunks hold copies of
  `InitPostRead`. A chain of calls that reaches a copy but no root is not a context.

### The per-item post-read call

`TSingleObjectGameDatabase<Db, Owner, …>::LoadFromReader` reads the statements of a file in a
loop. For a new key it allocates the item and calls the key constructor. Then, unless the
database's word at `+0x70` is 1, it calls slot `0x20` of the item's vtable at `+0x38` (the
item's `CPersistent` base). Last, it inserts the item into the array at `+0x48`/`+0x54`.

- Some instances move the new-key code into `ReadNewEntry`.
- A key that exists goes through the database's vtable to `ReadExistingEntry`. That function
  destroys the item, zeroes it, constructs it again and makes the same gated call.
- Slot `0x20` is `CPersistent::Read`. On every path it calls `ReadWithoutInitPostRead`, then
  slot `0x30` (`InitPostRead()`), then slot `0x38` (`InitPostRead(CString const&, int, int)`) as
  a tail call.
- In each owner's vtable, those slots hold a thunk of the owner's override, which adjusts `this`
  by `-0x38`. For `CBuildingType` the thunk is `sub x0, x0, #0x38; b CBuildingType::InitPostRead`.
  For `CCountryType`, the non-virtual thunk holds a copy of the whole body.
- Only the database constructors write the `+0x70` word: 0 for every registry here, and 1 for
  `CJobTagDatabase` and `CTraitTagDatabase`.
- `TPdxNullObject<Owner>::Initialize` constructs the null object with index -1 and an empty key
  in its own storage. It is not an item.

Two registries break the per-item call:

- `CJobTypeDatabase::CJobTypeDatabase()` constructs one `pop_jobs` item and inserts it without a
  call to `Read`. That is the one loaded job without generated names (365 of 366).
- The `ReadExistingEntry` and `ReadNewEntry` of `resolution_categories` construct the item inline
  and call `CPersistent::Read` directly.

### Item keys

The item constructor `<Item>::<Item>(int, CString const&)` copies the key argument inline. The
by-value form `(int, CString)` has the same ARM64 call shape. Buildings, districts,
megastructures, situations and zones store the key at `+0x10`. Bypass stores its id at `+0x10`
and the key at `+0x18`; `map_modes` also uses `+0x18`. The constructor probe establishes 156 of
164 named registries on M45-release. Four have no matching constructor, and four constructor
runs do not establish the key. `espionage_operation_categories` has no constructor symbol.

The engine branches on the key's form: a long key is in a buffer, and a short key is in place in
the string object. Constructors and generators copy string objects through `q` registers from
temporaries that are not fully written, so a copied flag byte can be unknown.

### The loaded modifier table

- **Where to read it.** `CGameApplication::InitGame` calls `CModifier::LogDefinitions()`
  unconditionally (`0x1005e96ec`). It runs after the trigger, effect and event post-inits, and
  before `PrintScriptingDocumentation`. The worker hooks its entry on the launch thread and reads
  the table when it returns. The session pauses there (`GameReadiness::PausedAfterContentLoad`)
  about 20 seconds after launch. The read takes about 0.4 seconds.
- **Layout.** `LogDefinitions` walks `CPdxModifier<…>::_Definitions`, a `CPdxArray` with its data
  at `+0x8` and its count at `+0x14`. Each definition is 0x98 bytes, with the lexer token at
  `+0x78` and the category mask at `+0x84`. `AddDefinition` indexes the array by `ModifierType`.
  The beta layout is the same.
- **Names.** A definition's own strings are localization keys (`MOD_COUNTRY_SCAVENGE_DEBRIS_MULT`),
  not names. The name is `CStaticLexer::GetString(token)`: element `token` of a
  `CPdxArray<CString>` of 0x28-byte elements in unnamed globals at `0x103796d70`. `GetString`
  rebuilds the lookup when its count differs from the size at `0x103796d88`. At the return of
  `LogDefinitions` the lookup is current, because the function named every entry; the worker
  requires the two counts to be equal.
- **Symbols.** `LogDefinitions` and `_Definitions` have symbols; the lookup does not.
  `engine/analysis/modifier_table.rs` derives the layout and the lookup from their readers, so no
  modifier-table address or offset is in a binding group.
- **Generator registries.** The worker reads each registry's keys at the same point, from
  `TGameDatabase<Db>::_pInstance`. The database's directory must equal the registry, and the
  keys are at the established key offset.
- **Case.** A generated name keeps the case of its item key. The table holds
  `BIOLOGICAL_logistic_growth_mult` and `LITHOID_logistic_growth_mult`, and content writes them
  in lower case. `CStaticLexer::AddDynamicToken` compares lower-case characters when it looks up a
  token. That content reads resolve the lower-case form through the same lookup is not
  established.
- **DLC archives.** No DLC archive of the installation holds a script file. 22 of the 32 archives
  hold only `music/` and `sound/`. The other ten (`dlc002`, `dlc004`, `dlc013`, `dlc015`,
  `dlc029`, `dlc030`, `dlc033`, `dlc035`, `dlc039`, `dlc042`) are empty 22-byte zip files. Mount
  state therefore cannot change the modifier table. Native's sessions set a private `HOME`, so
  the game finds no Steam client and prints no mount failure.

## Result on M45-release

The loaded table has 45,578 entries. The names, their order and every tag list equal the
`modifiers.log` that the engine writes in the same session. The table has 571 declared names
(they join `Native::modifiers`), 5,432 names that a returned family generates for a loaded item,
and 39,576 unexplained names. No name has two families. Five declared names have other tags in
the loaded table, because content registers them again: `terraforming_cost_mult` has Planets and
AI Economy; `starbase_shipyard_build_cost_mult` has Starbases and AI Economy; the two other
`starbase_shipyard_*` names and `gdf_ship_alloys_cost_mult` have the six ship and station tags
and AI Economy.

`modifier_families` returns 50 families in 22 registries, in about 0.8 s for each registry. The
last two columns are from one live `examples/loaded-modifiers.rs` session: the registry's loaded
items, and how many of the names that the template gives for them are loaded.

| Registry | Family | Root | Condition | Items | Loaded names |
| --- | --- | --- | --- | ---: | ---: |
| `common/anomalies` | `{key}_research_speed_mult` | item | Unresolved | 327 | 327 |
| `common/buildings` | `planet_{key}_build_speed_mult` | database | Always | 498 | 498 |
| `common/buildings` | `{key}_max` | item | Unresolved | 498 | 0 |
| `common/bypass` | `{key}_empire_windup_mult`, `_ship_windup_mult`, `_megastructure_bypass_windup_mult` (limit 127) | database | Always | 10 | 10 each |
| `common/country_types` | `damage_vs_country_type_{key}_mult` (limit 511) | item | Unresolved | 101 | 101 |
| `common/districts` | `planet_{key}_build_speed_mult` | database | Always | 147 | 147 |
| `common/districts` | `{key}_max_add`, `{key}_max_mult` | item | Unresolved | 147 | 147 each |
| `common/economic_categories` | `{key}_produces_mult`, `_upkeep_mult`, `_cost_mult`, `_logistics_mult` (tags unresolved) | item | Unresolved | 269 | 46, 53, 35, 1 |
| `common/espionage_operation_types` | `{key}_speed_mult`, `_skill_add`/`_mult`, `_difficulty_add`/`_mult`, `hostile_{key}_difficulty_add`/`_mult` | item | Unresolved | 27 | 27 each |
| `common/ethics` | `pop_{key}_attraction_mult` | item | Unresolved | 17 | 17 |
| `common/governments/councilors` | `{key}_exp_gain` | database loop | Unresolved | 179 | 179 |
| `common/megastructures` | `megastructure_{key}_build_speed_mult` | database | Always | 164 | 164 |
| `common/patrons` | `add_attunement_{key}`, `{key}_attunement_mult` | database loop | Unresolved | 14 | 5 each |
| `common/pop_categories` | `pop_cat_{key}_happiness`, `_political_power`, `_bonus_workforce_mult` | item | Always | 24 | 24 each |
| `common/pop_jobs` | `job_{key}_add`, `_per_pop`, `_per_crime`, `_max_workforce_add`/`_mult`, `_automated_workforce_mult`, `pop_{key}_workforce_mult`, `pop_{key}_bonus_workforce_mult` | item | Unresolved | 366 | 365 each; automated 354 |
| `common/resolution_categories` | `{key}_vote_strength_mult` | item | Unresolved | 37 | 37 |
| `common/scripted_modifiers` | `{key}` (tags unresolved) | database loop | Always | 133 | 133 |
| `common/situations` | `{key}_max_progress_add`, `{key}_max_progress_mult` | database | Unresolved | 90 | 1 each |
| `common/species_archetypes` | `{key}_species_trait_points_add`, `_picks_add`, `_pop_happiness`, `_logistic_growth_mult`, `_bonus_pop_growth`, `_bonus_pop_growth_mult` | item | Unresolved | 6 | 4, 4, 2, 2, 2, 2 |
| `common/technology/category` | `category_{key}_research_speed_mult`, `category_{key}_draw_chance_mult` | database loop | Unresolved | 13 | 13 each |
| `common/zones` | `planet_{key}_build_speed_mult` | database | Always | 146 | 146 |

`common/leader_classes`, `common/ship_sizes` and `common/espionage_operation_categories` are the
other three registries; they return no family (see the gaps below). The templates of buildings,
districts and bypass also equal all 680 registrations that the SDK-498 prototype hooks observed
in two live runs.

Tags come from the category mask of the registration. Economic categories and scripted modifiers
pass a mask from an item field (for economic categories `+0x128`, the content's
`modifier_category`), so their tags are unresolved.

### Why a condition stays `Unresolved`

- **Item-field gates.** `CCountryType::InitPostRead` registers only when the field at `+0x580`
  equals `0x7fffffff`; every loaded country type has that value (101 of 101). The buildings
  `{key}_max` family needs bit 2 of `+0x17a8`, which no loaded building sets (0 of 498). The
  situations generator tests `ldrb w8,[x23,#0x508]; cbz` before both calls (1 of 90 loaded).
- **Content gates.** Economic categories generate only with `generate_mult_modifiers`. The 11 jobs
  with `can_be_automated = no` have no `job_<job>_automated_workforce_mult`. Archetypes with
  `uses_modifiers = no` or `robotic = yes`, leader classes without `leader_capacity`, and patrons
  without `add_modifier = yes` have no names. A comparison that expands a template over a whole
  type, or that takes the script value `random_list` as a planet class, reports these as missing
  names; they are not engine omissions.
- **Path limit.** The item-root runs of districts, espionage types, ethics, species archetypes
  and economic categories exceed the path limit.
- **Per-item call not established.** `pop_jobs` and `resolution_categories` (above).
- **Not traced.** Anomaly root runs have no failed path, but half of their returned paths skip
  the registration.

### Gaps

The `UnnamedDeclaration` gap counts the 20 generation sites that are not joined to a registry:

| Reason | Sites | Code |
| --- | ---: | --- |
| Inside the registration function | 1 | The `AddDefinition` in `CModifier::TryAddDynamicModifier` |
| Unnamed matrix input | 1 | `CEconomicCategory::FillModifierTable<false>` (economic category × strategic resource) |
| Unnamed content | 5 | `CModifierGeneratorBase::GenerateFrom` (also called by `CStrategicResourceModifierGenerator`), `CDatabaseModifierGenerator<CStrategicResource>::Generate`, `CPlanetClassModifierGenerator::CreateModifier`, `CZoneTypeModifierGenerator::CreateMaxModifier` (from `CZoneType::CSerializer`), `CEconomicCategory::FillModifierTable<true>` (also from the triggered-table serializers) |
| No root | 13 | Ten `AddDefinition` calls in `CShipClassModifierHelper::Init` (from `CModifier::InitDefinitions`: the engine's ship-class enum, before content), `CWeaponTagModifierHelper::CreateModifierType` (from `CWeaponTagDatabase::InitInstance`), two in `CLeaderClass::CreateStartingAgeModifiers` (no direct caller) |

Gaps in joined registries:

- `common/leader_classes`: the eight families compose from `+0x1f0`, a string field of the item
  that is not the key. The method does not name it.
- `common/ship_sizes`: `CShipSizeModifierGenerator::CreateModifier(ModifierType)` lower-cases
  the base definition's localization key and replaces `mod_ship_` with `shipsize_{key}_`. The
  string model does not follow `ToLower` or `Replace`. The other call is not reached within the
  path limit.
- `common/districts`: two paths per call use the object at `+0xea0` instead of the item, when a
  virtual call on it returns non-zero. Those names are not the item's.
- Some paths read an unknown object and fail to give a name (districts, councilors). The answer
  counts them by reason.

Outside the methods: modifiers that the engine adds after the documentation point; where a
modifier takes effect; the tags of a declared modifier after content registers it again; a key
part from a second named registry (none on M45-release); joining states in the item-root runs,
which would remove their path-limit failures; inline item construction; the family run of a
copied thunk.

## Pitfalls

Each of these gave a wrong answer once. Each now has an authored test with a negative control.

**String model.**

- When the model makes a short string long, remove the object's old label. The short-key run
  copies the key in place into a stack string, and the stale label made that run read the bare
  key.
- A `memmove` or `memcpy` of unknown length makes the strings that it receives unresolved.
- Do not enter a function that only calls memory and copy functions. Entering
  `basic_string::__assign_external` stored through an unknown buffer pointer, made every known
  byte unknown, and removed the families of districts, zones and megastructures. The model reads
  its text instead.
- Write unresolved text as the empty text. When the length stayed unknown, the inline copy of
  `"mod_"` and an unknown field stored through an unknown address and made the whole memory
  unknown, which removed the economic-category families.
- A `CPdxStringView` is the string's text only when its length equals the text's length.

**Definition types.**

- The `AddDefinition` stub must write a new type. When it left the sentinel `0x23b`, the code
  read `_Definitions[0x23b]`, which was outside the held array, ended in the item and made the
  key unresolved.
- The new type is the path's own count of registrations, so forked paths never share one.

**The per-item call.**

- Follow every pass of a loop: join states at loop heads. One pass missed a later pass that
  skipped the read. Share the joins between paths; per-path joins exceeded 64 paths.
- Keep both sides of a flag split at a loop head. Join the flag states that each path allowed.
- Check only returned paths. In `ReadExistingEntry` of espionage types a backward branch is not a
  loop, and a covered path ended there before the covering path read the item.
- Run each loader with fresh registers. The registers that the database constructor left, such as
  a zero `bool`, decided a branch.
- Credit a thunk only at the subobject whose vtable holds it, and also when a branch inside the
  decoded code reaches it.
- One root does not discharge another; each item needs every root.
- Account for every function that constructs an item or forms a vtable address point, not only
  the database's functions.
- New memory of a loader must not have the address of memory that the database constructor
  reserved.

**Assumptions** (not established): an item's key does not change after its constructor; a store
to an unknown address or a call that the run does not follow changes neither the key, the held
definition masks, the database's fields other than its item array, nor an item's vtable
pointers; a thunk runs its root, also when it holds a copy of the root's body.
