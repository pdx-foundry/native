# Modifier families

`Native::modifier_families(registry)` (`modifier-families/v4`) gives the modifier names that a
registry's code registers for each item. `Game::loaded_modifiers` (`loaded-modifiers/v1`) gives
the modifier table after all content loads, and joins each entry to a declaration or a family.
The module comments of `engine/analysis/families.rs`, `families/joins.rs`, `families/loading.rs`,
`engine/analysis/strings.rs` and `session/loaded_modifiers.rs` describe the methods. This page
holds the engine facts, the M45-release result, the gaps and the pitfalls. The
[engine knowledge index](../engine-knowledge.md) lists the other pages.

## Engine code

This code is from M45-release.

### Generation calls

- `CModifier::TryAddDynamicModifier` receives the name in `x1` and the category mask at `[sp]`
  (recipe `dynamic_modifier_category_offset`). `CModifier::AddDynamicModifier` is the other
  dynamic call.
- `CPdxModifier<…>::AddDefinition` receives the token in `w0`, the `ModifierType&` in `w1` and
  the mask at `[sp,#4]` (recipe `modifier_category_offset`). It writes the new type into its
  `ModifierType&` argument. Before the call, the engine leaves the sentinel `0x23b` there.
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
- A registry that its own loader names ([custom-loader names](registry-fields.md#custom-loader-names))
  takes part with the database and the owner that its loader builds, as a template registry
  does. On M452 the 15 such registries with one owner add one root:
  `CStrategicResource::InitPostRead`, which joins the `CModifierGeneratorBase::GenerateFrom` and
  `CDatabaseModifierGenerator<CStrategicResource>::Generate` sites to
  `common/strategic_resources` (eight families). The other 14 reach no generation call.
- The post-read functions of `CPlanetClass`, `CZoneType::CSerializer` and
  `CEconomicCategory::CTriggeredModifierTable::CSerializer<N>` are roots with no named registry.
  `common/planet_classes` has no one owner (its loader builds `CPlanetClass` and
  `CPlanetClassRandomList`, both with a member reader), so `CPlanetClassModifierGenerator::CreateModifier`
  stays an `UnnamedContent` gap and the registry is `Unsupported`. A join needs an owner rule that
  selects the class that the loader keys by the file's top-level statements.
- `CEconomicCategory::FillResourceModifierMatrix(CStrategicResource const&, …)` takes a second
  content object. A function is a second input when it takes an object of a class whose post-read
  code no template registry owns, except on the contexts of that class's own loader registry.
  When the class is the item class of one named registry, the first such function on a context is
  that registry's input: `CEconomicCategory::GenerateModifiers` loops over
  `CStrategicResourceDatabase::_pInstance` and calls `FillResourceModifierMatrix` for each
  resource, which composes `{category}_` and the resource key (`+0x20`) before
  `FillModifierMatrix<false>` → `FillModifierTable<false>`. The input runs on its own (below), and
  the root run does not enter it. A second input of a class with no named registry, or one context
  through items of two other registries, stays a `SecondInput` gap. A function that takes an item
  of another template registry is not a second input; none generates on M452. Make the rule
  uniform when such a function appears.
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
  in its own storage.

- Some database constructors build items of their own and insert them without a call to `Read`:
  `CJobTypeDatabase` builds `yes`, and the constructors of the `council_agendas`, `deposits` and
  `storm_types` databases construct items from the literals `yes`, `no` (and `random`). No content
  file of those registries defines those keys; a content statement with the same key goes through `ReadExistingEntry`, which reads the
  item. They are the engine's own items, so the method requires the item roots only for the items
  of the other loaders. The `yes` job is the one loaded job without generated names (365 of 366).
- `TSingleObjectGameDatabase<Db, Owner, …>::PostReadInit()` calls `<Owner>::PostReadInit()` for each
  item of the array at `+0x48`/`+0x54`, whatever code built the item; `CPersistent::Read` does not
  call it. The method runs that loop on the database as its constructor leaves it, with two
  distinct items. A root that every returned path calls with both items needs no loader run. This
  rests on the stated rule that a loop which calls a root for both of two items treats every item
  alike: the loops of councilors (`0x100807e6c` on M452), buildings and traditions are
  `for i in 0..count: items[i]->PostReadInit()` with no other test. A loop that tests the count
  or the index before the call would break the rule; follow an arbitrary pass when one appears.
  On M452 this establishes the call for 32 of the 36 `derived_names` registries that had none.

The method does not establish the per-item call where the database's `ReadExistingEntry` and
`ReadNewEntry` construct the item inline, without a constructor call, and then call
`CPersistent::Read` directly (`resolution_categories`, `espionage_operation_categories`). It does
not follow inline construction (an `InlineConstruction` gap). Nor does it for an item root that a
destructor-shaped function names with the item's vtables (`TPdxNullObject<X>::~TPdxNullObject`, a
virtual override thunk of the destructor), or that another class constructs.

### Item keys

The item constructor `<Item>::<Item>(int, CString const&)` copies the key argument (`x2`) inline. The
by-value form `(int, CString)` has the same ARM64 call shape. A custom loader's item can take the
key first: `CStrategicResource::CStrategicResource(CString const&, int)` (`0x100c60930` on M452)
reads the key from `x1`, stores the int at `+0x18` and the key at `+0x20`. The probe runs each
shape with the key in its own register; `COpinionModifier` has the same shape, so
`common/opinion_modifiers` also gets a key place ([derived names](derived-names.md#population)). Buildings, districts,
megastructures, situations and zones store the key at `+0x10`. Bypass stores its id at `+0x10`
and the key at `+0x18`; `map_modes` also uses `+0x18`. The constructor probe establishes 156 of
164 named registries on M45-release. Four have no matching constructor, and four constructor
runs do not establish the key. `espionage_operation_categories` has no constructor symbol.
The worker refuses an item read for those eight; it never reads a key from an unestablished
offset. It also checks key uniqueness, nonempty keys and control characters.

The live case `loaded_modifier_key_layouts` checks the loaded keys of the six generator
registries (buildings, bypass, districts, megastructures, situations and zones) in one
loaded-modifier session, and compares the `common/bypass` and `common/strategic_resources` keys
with their source keys (26 strategic resources on M452). Their
loaders run on the launch thread. An earlier control found the eight `common/map_modes` keys equal
to its top-level source keys.

The engine branches on the key's form: a long key is in a buffer, and a short key is in place in
the string object. Constructors and generators copy string objects through `q` registers from
temporaries that are not fully written, so a copied flag byte can be unknown.

### Inputs from another registry

An input runs as its own entry, not inside its root's run. The economic-category item root
exceeds the path limit in the loops after `GenerateModifiers`; a root run that enters the matrix
spends its paths there and does not reach the matrix registration, also with a limit of 1,024
paths. The input run starts at the input function with `x0` the item and the input's
register (`x1` for the matrix) a new item of the other registry, whose key is at the place that
the other registry's key constructor establishes (`+0x20` for `CStrategicResource`). It runs once
for each pair of the two keys' forms. Its families are never `Always`: the root may skip the call.
On M452 it gives eight families, `{key}_{common/strategic_resources key}_{produces,upkeep,cost,logistics}_{add,mult}`.

`FillModifierTable<false>` is also reached from `CTriggeredModifierTable::CSerializer<N>::InitPostRead`,
so its site stays unjoined with the reason `UnnamedContent`: joining the matrix moved one site from
the second-input count to the unnamed-content count, and the total stays 18.

The live case `loaded_modifiers` checks that `country_minerals_produces_mult` is attributed to
the `country` economic category and the `minerals` strategic resource. On M452 the eight
families explain 6,059 loaded names (table below); the session's 45,583 names have 11,700 that a
family explains and 33,314 unexplained.

### Standard-string move assignment

On M451-hotfix, `basic_string<char, char_traits<char>, CPdxCommonStringAllocator>::__move_assign(
basic_string&, integral_constant<bool, false>)` is at `0x1000113a4`, with the destination in `x0`
and the source in `x1`. It compares the allocator state of the two strings, the 4 bytes at
`+0x18` and the 8 bytes at `+0x20`. When they are equal, it takes the source's 24 bytes and
leaves the source as the empty short string (zero at `+0x17` and `+0`). When they differ, it
copies the text and the source keeps it. `CString::CString(char const*)` (`0x102521f3c`) writes
zero to both words, so strings from the engine's constructors have equal state.

The string model always takes the move: the destination gets the source's text and the source
becomes empty. A source with a different allocator state would keep its text in the engine. If
a method finds a string whose allocator state is not the constructors' zero state, the model
must read both words and take the copy when they differ.

### The loaded modifier table

- **Where to read it.** `CGameApplication::InitGame` calls `CModifier::LogDefinitions()`
  unconditionally (`0x1005e96ec`). It runs after the trigger, effect and event post-inits, and
  before `PrintScriptingDocumentation`. The worker hooks its entry on the launch thread and reads
  the table when it returns; the session pauses there (`GameReadiness::AfterContentLoad`).
- **Layout.** `LogDefinitions` walks `CPdxModifier<…>::_Definitions`, a `CPdxArray` with its data
  at `+0x8` and its count at `+0x14`. Each definition is 0x98 bytes, with the lexer token at
  `+0x78` and the category mask at `+0x84`. `AddDefinition` indexes the array by `ModifierType`.
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
- **Custom-loader databases.** `CStrategicResourceDatabase` is no `TGameDatabase`. Its instance is
  `CStrategicResourceDatabase::_pInstance` (`0x1032ed728` on M452). `CreateInstance` builds it in
  place with `new(0x90)`; there is no constructor symbol. Its items are a `CPdxArray` at `+0x8`
  (data) and `+0x14` (count), which `Init` and `GetResource(int)` use. It holds no directory text,
  so the worker reads its keys without the directory check. The recipe states this layout
  (`M452_LOADER_DATABASES`); a custom-loader registry whose database has no stated layout gets
  no key read, with the reason. State another database's layout when Atlas needs its loaded
  names.
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

## Loaded table result

On M45-release the loaded table has 45,578 entries. The names, their order and every tag list equal the
`modifiers.log` that the engine writes in the same session. The table has 571 declared names
(they join `Native::modifiers`), 5,432 names that a returned family generates for a loaded item,
and 39,576 unexplained names. `terraforming_cost_mult` is both declared and generated (by the
`terraforming` item of `common/economic_categories`), so the three counts overlap by one. No name
has two families. The answer is `Partial`: the unjoined
generation sites and the unexplained names are gaps. The layout derivation gives one complete
table layout, with no partial layout and no failure. Five declared names have other tags in
the loaded table, because content registers them again: `terraforming_cost_mult` has Planets and
AI Economy; `starbase_shipyard_build_cost_mult` has Starbases and AI Economy; the two other
`starbase_shipyard_*` names and `gdf_ship_alloys_cost_mult` have the six ship and station tags
and AI Economy.

`modifier_families` returns 50 families in 22 registries. All 22
answers are `Partial`; none is complete or failed. The
last two columns are from one live `examples/loaded-modifiers.rs` session: the registry's loaded
items, and how many of the names that the template gives for them are loaded.

| Registry | Family | Root | Condition | Items | Loaded names |
| --- | --- | --- | --- | ---: | ---: |
| `common/anomalies` | `{key}_research_speed_mult` | item | Always | 327 | 327 |
| `common/buildings` | `planet_{key}_build_speed_mult` | database | Always | 498 | 498 |
| `common/buildings` | `{key}_max` | item | Unresolved | 498 | 0 |
| `common/bypass` | `{key}_empire_windup_mult`, `_ship_windup_mult`, `_megastructure_bypass_windup_mult` (limit 127) | database | Always | 10 | 10 each |
| `common/country_types` | `damage_vs_country_type_{key}_mult` (limit 511) | item | Always | 101 | 101 |
| `common/districts` | `planet_{key}_build_speed_mult` | database | Always | 147 | 147 |
| `common/districts` | `{key}_max_add`, `{key}_max_mult` | item | Unresolved | 147 | 147 each |
| `common/economic_categories` | `{key}_produces_mult`, `_upkeep_mult`, `_cost_mult`, `_logistics_mult` (tags unresolved) | item | Unresolved | 269 | 46, 53, 35, 1 |
| `common/economic_categories` (M452) | `{key}_{common/strategic_resources key}_produces_add`/`_mult`, `_upkeep_add`/`_mult`, `_cost_add`/`_mult`, `_logistics_add`/`_mult` (tags unresolved) | input | Unresolved | 270 × 26 | `_add`/`_mult`: produces 1,794/1,196, upkeep 858/1,326, cost 0/859, logistics 0/26 |
| `common/espionage_operation_types` | `{key}_speed_mult`, `_skill_add`/`_mult`, `_difficulty_add`/`_mult`, `hostile_{key}_difficulty_add`/`_mult` | item | Always | 27 | 27 each |
| `common/ethics` | `pop_{key}_attraction_mult` | item | Always | 17 | 17 |
| `common/governments/councilors` | `{key}_exp_gain` | database loop | Unresolved | 179 | 179 |
| `common/megastructures` | `megastructure_{key}_build_speed_mult` | database | Always | 164 | 164 |
| `common/patrons` | `add_attunement_{key}`, `{key}_attunement_mult` | database loop | Unresolved | 14 | 5 each |
| `common/pop_categories` | `pop_cat_{key}_happiness`, `_political_power`, `_bonus_workforce_mult` | item | Always | 24 | 24 each |
| `common/pop_jobs` | `job_{key}_add` | item | Always | 366 | 365 |
| `common/pop_jobs` | `job_{key}_per_pop`, `_per_crime`, `_max_workforce_add`/`_mult`, `_automated_workforce_mult`, `pop_{key}_workforce_mult`, `pop_{key}_bonus_workforce_mult` | item | Unresolved | 366 | 365 each; automated 354 |
| `common/resolution_categories` | `{key}_vote_strength_mult` | item | Unresolved | 37 | 37 |
| `common/scripted_modifiers` | `{key}` (tags unresolved) | database loop | Always | 133 | 133 |
| `common/situations` | `{key}_max_progress_add`, `{key}_max_progress_mult` | database | Unresolved | 90 | 1 each |
| `common/strategic_resources` (M452) | `country_resource_max_{key}_add`, `ship_starbase_stockpile_{key}_collection_rate_add`/`_mult`, `starbase_collected_colony_{key}_add`, `starbase_collected_system_{key}_add`/`_mult`, `starbase_stockpile_{key}_capacity_add`/`_mult` | item | Unresolved | 26 | 26 each |
| `common/species_archetypes` | `{key}_species_trait_points_add`, `_picks_add`, `_pop_happiness`, `_logistic_growth_mult`, `_bonus_pop_growth`, `_bonus_pop_growth_mult` | item | Unresolved | 6 | 4, 4, 2, 2, 2, 2 |
| `common/technology/category` | `category_{key}_research_speed_mult`, `category_{key}_draw_chance_mult` | database loop | Always | 13 | 13 each |
| `common/zones` | `planet_{key}_build_speed_mult` | database | Always | 146 | 146 |

`common/leader_classes`, `common/ship_sizes` and `common/espionage_operation_categories` return no
family (see the gaps below). On M452 `common/strategic_resources` adds eight families (23 registries
in `tests/expected/m452/modifier-families.json`), and `common/economic_categories` adds the eight
two-key families of its input; the other 14 custom-loader registries with one
owner answer partial with no family (see [Roots](#roots)), and the six without one owner are
`Unsupported` with the reason of the owner join. The five build-speed and
windup templates of buildings, districts and bypass also equal all 680 registrations that the
SDK-498 prototype hooks observed in two live runs. That control does not cover the district and
building maximum families. The hooked sites on M45-release are `0x1000df770` (buildings),
`0x100431bdc` (districts), and `0x1000fefec`, `0x1000ff05c` and `0x1000ff0c4` (bypass); the final
name was read at `CModifier::LogDefinitions()+584`, before `CLogStream::operator<<(CString const&)`.
`config/modifiers.cwt:728-737` at config commit `8574760` agrees with these five templates and
their tags. The prototype and its runs are in `.local/sdk-498/`.

Tags come from the category mask of the registration. Economic categories and scripted modifiers
pass a mask from an item field (for economic categories `+0x128`, the content's
`modifier_category`), so their tags are unresolved.

### Conditions

A root's condition runs join states at loop heads and end a path once it has registered every name
of its name run (see the module comment of `families.rs`). Each starts the item's cache fields at
their constructor default.

- **Cache fields.** Many roots keep the item's registered modifier type in an item word and
  register only while the word holds the value that the constructor writes: anomalies `+0x1c8`
  (zero), country types `+0x580` (`0x7fffffff`, `0x10033e4ec`), ethics `+0x5c0` (zero), technology
  categories `+0xc0` and `+0xc4` (zero). The method finds the word where a returned path stores a
  type that its own registration created, and takes the value that every returned path of every
  constructor body leaves there. The assumption that no content reader writes a cache field is
  checked on three members by the loaded table (anomalies 327 of 327, ethics 17 of 17, technology
  categories 13 of 13 generate their names; country types 101 of 101). No `registry_fields` field
  of those three stores at the cache offset, but their storage is complete only for technology
  categories (3 of 3; anomalies 12 of 14, ethics 15 of 19), so that is supporting evidence only.
- **Loops before the registration.** `CEspionageOperationType::InitPostRead` loops over its stages
  and `CEthic::InitPostRead` over the ethic categories before they register. An unjoined run of
  such a loop spends the path bound; the condition runs join at the loop head. Ethics still forks
  at each category's key compare: its condition runs return on 210 paths, under the condition runs'
  bound of 256.

**Population (SDK-713).** Over the 185 registries of the sweep recording on M452, the
`modifier_families` gaps that are not `OutsideMethod` went from 253 to 240 (179 registries): 13
families are now `Always`, and six `pop_jobs` gaps name the assumed text. The `derived_names` gaps
went from 654 to 616: the every-item gap from 36 registries to 4, and six name conditions that it
blocked are now established. No registry's sweep status changed (`registry-sweep --diff` against
`tests/population/m452/registry-sweep/`): every family answer keeps the `UnnamedDeclaration`
gap of the unjoined sites.

### Why a condition stays `Unresolved`

- **Item-field gates.** The buildings `{key}_max` family needs bit 2 of `+0x17a8`, which no loaded
  building sets (0 of 498). The situations generator tests `ldrb w8,[x23,#0x508]; cbz` before both
  calls (1 of 90 loaded).
- **Content gates.** Economic categories generate only with `generate_mult_modifiers`. The 11 jobs
  with `can_be_automated = no` (`ldrb [x19,#0xa5a]`) have no `job_<job>_automated_workforce_mult`.
  Archetypes with `uses_modifiers = no` or `robotic = yes`, leader classes without
  `leader_capacity`, and patrons without `add_modifier = yes` have no names. A comparison that
  expands a template over a whole type, or that takes the script value `random_list` as a planet
  class, reports these as missing names; they are not engine omissions.
- **Assumed text.** After `job_{key}_add`, `CJobTypeModifierGenerator::CreateAddIconKey` composes
  `mod_job_` + the string at `+0x970` + `_add`. `SPopJobTypeSwap::CSerializer::InitPostRead`, which
  the run does not follow, writes that string; no `registry_fields` field stores there. The model
  writes the unresolved text as the empty text, so the six later `pop_jobs` registrations that
  every path makes are not established (gap: "only after a name part that the method does not
  follow").
- **Path limit.** The item-root runs of districts, species archetypes and economic categories
  exceed the path limit. `CGovernmentCouncilorType::PostReadInit` forks on two `HasLocalizeKey`
  checks and indirect calls, and its loop around `SetModifierNameHolder` (`0x1008071d0`) does not
  join (`loop-limit`).
- **Per-item call not established.** `resolution_categories` (inline construction, above).
  `common/strategic_resources` has no database constructor symbol, so the loader run has no
  database to start from (`NoDatabaseConstructor`). `CStrategicResourceDatabase::Init` constructs
  each item and calls its `Read` through the vtable; running `CreateInstance` as the database
  constructor, with the database layout above, is the change when Atlas needs `Always` here.
### Gaps

The `UnnamedDeclaration` gap counts the 18 generation sites (M452) that are not joined to a
registry:

| Reason | Sites | Code |
| --- | ---: | --- |
| Inside the registration function | 1 | The `AddDefinition` in `CModifier::TryAddDynamicModifier` |
| Unnamed content | 4 | `CPlanetClassModifierGenerator::CreateModifier`, `CZoneTypeModifierGenerator::CreateMaxModifier` (from `CZoneType::CSerializer`), `CEconomicCategory::FillModifierTable<true>` and `<false>` (also from the triggered-table serializers) |
| No root | 13 | Ten `AddDefinition` calls in `CShipClassModifierHelper::Init` (from `CModifier::InitDefinitions`: the engine's ship-class enum, before content), `CWeaponTagModifierHelper::CreateModifierType` (from `CWeaponTagDatabase::InitInstance`), two in `CLeaderClass::CreateStartingAgeModifiers` (no direct caller) |

Gaps in joined registries:

- `common/leader_classes`: the eight families compose from `+0x1f0`, the string field `name`
  (`registry_fields` storage), not the key. Content writes `name` equal to the key, but the engine
  reads the field. The family runs do not plant string fields, so the names stay unresolved (see
  [config templates](#config-templates)).
- `common/ship_sizes`: `CShipSizeModifierGenerator::CreateModifier(ModifierType)` lower-cases
  the base definition's localization key and replaces `mod_ship_` with `shipsize_{key}_`. The
  string model does not follow `ToLower` or `Replace`. The other call is not reached within the
  path limit.
- `common/districts`: two paths per call use the object at `+0xea0` instead of the item, when a
  virtual call on it returns non-zero. Those names are not the item's.
- Some paths read an unknown object and fail to give a name (districts, councilors). The answer
  counts them by reason.

Outside the methods: modifiers that the engine adds after the documentation point; where a
modifier takes effect; the tags of a declared modifier after content registers it again; a second
input of a class with no named registry, or items of two other registries on one context; joining
states in the name runs, which loses the names that a loop registers on each pass; inline item
construction; the family run of a copied thunk.

### Config templates

Config `modifiers.cwt` (commit `8574760`) has five template groups with no joined family (Atlas
`docs/coverage/gap-triage.md`):

- `<economic_category>`: its two lines are comments on the `<economic_category>_<resource>_…`
  matrix, which the eight two-key families of `common/economic_categories` answer
  (`NamePart::RegistryKey`, SDK-794).
- `<zone>`: the one line is `planet_<zone>_build_speed_mult`, which `common/zones` answers
  `Always`. The unjoined `CZoneTypeModifierGenerator::CreateMaxModifier` site gives no config
  template.
- `<leader_class>` (six lines): the names take the `name` field, not the key (gaps above). A family
  with a field part would need runs that plant string fields, apart from the condition runs, since
  planted text prunes branches. Atlas reads `NamePart::Field` as an uninterpreted part today, so
  such a family would not credit the lines. Add it when Atlas reads field parts.
- `<espionage_category>` (seven lines): `common/espionage_operation_categories` has the root, but
  no constructor symbol, and its loaders construct the item inline, so the key place is not
  established.
- `enum[ship_class]` (ten lines): the ten `AddDefinition` calls in `CShipClassModifierHelper::Init`
  compose names from the engine's ship-class enum before content loads (`NoRoot` above). They are
  no registry's family.

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

**Condition runs.**

- Take names only from unjoined runs. A loop that registers on each pass does not join (its labels
  differ), and a joined run lost the economic-category `logistics_mult` names.
- End a path when it has registered every name of its name run at its chain, not when it has
  reached every call site. `GenerateFrom` is one site for several chains, and ending at the first
  one lost the later `pop_jobs` families.
- Do not enter `CPdxLogFileAndLine::CPdxLogFileAndLine`. Its byte-clearing loop spent the joined
  runs' loop arrivals, as it did for derived names.
- Take a cache field's default from complete constructor runs. The key-place probe stops at the
  first call after the key and zeroes the other arguments.

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

**Assumptions** (not established): an input function is a member function, so the other item
is in the register after `this` for its parameter; the null object is not an item of the database;
the engine runs the database's post-read loop once after loading, and a loop that calls a root for
both of two items treats every item alike; the items that a database constructor builds are the
engine's own; no content reader writes a cache field; an item's key
does not change after its constructor; a store to an unknown address or a call that the run does not
follow changes neither the key, the held definition masks, the database's fields other than its item
array, nor an item's vtable pointers; a thunk runs its root, also when it holds a copy of the root's
body.
