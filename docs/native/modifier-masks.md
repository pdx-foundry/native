# Modifier category masks

`Native::modifier_nodes()` (`modifier-nodes/v1`) gives the engine's modifier node graph. For each
node it gives the source nodes, the engine types that construct it, and the categories that it
keeps. Atlas maps owners to scopes with its own stated mapping and reviews the differences from
the `supported_scopes` lists of `modifier_categories.cwt`. The parse-time masks of modifier
containers are in [container masks](#container-masks): `Field.accepted_categories`
(`registry-fields/v18`) and `Native::modifier_category_keys()` (`modifier-category-keys/v1`).

The method is `src/engine/analysis/modifier_nodes.rs`; its module comment states the runs and the
limits of the search. `src/binding/binary/modifier_nodes.rs` parses the node type symbols and finds
the constructor calls and the static initializers. `src/session/modifier_nodes.rs` gives the public
answer and its gaps. Category names come from the category switch of
[engine commands](engine-commands.md#categories). The [engine knowledge index](../engine-knowledge.md)
lists the other pages.

## Engine facts (M451-hotfix)

Executable `29fa877366040a528098da39ec7e70b7baac76782a2a6bd161616d691f86fa38`, ARM64 slice
`2aeb9e15241bb114fd9f35a2dd09b454a5df6a0b1948b229d9eb83123e665c21`, read with `examples/inspect`.
The first static probe (2026-10-02, `.local/modifier-node-probe/`) found most of these facts; the
method confirmed them and corrected one.

### Nodes and their sources

- A node type is `NModifierNode::CModifierNode<CModifier, EModifierNodeCategory, N,
  NModifierNode::SDependencies<EModifierNodeCategory, …> >`, with N from 0 to 34. The
  `SDependencies` arguments are the source nodes: the nodes whose totals the node receives. They
  nest, so one symbol can name the sources of several nodes. Every symbol that names a node gives
  it the same sources.
- The base constructor of each node is
  `NModifierNode::CModifierNodeBase<CModifier, EModifierNodeCategory>::CModifierNodeBase<…>`.
  Its template argument is a closure type inside `CModifierNode<…N…>::CModifierNode<OWNER, …>`,
  sometimes nested in the same template again. The owner type follows the last repetition.
- cpp_demangle 0.5.1 cannot read the base constructors of nodes 18, 23 and 31 (`CColony`,
  `CPopGroup`, `CArmy`) with its default limits: parsing needs a recursion limit of 120 (default
  96), and rendering needs 144 (default 128). `display_name` uses 192 and 256. This makes 56 more
  symbols readable, all of them node symbols; 270 symbols still do not demangle at 1,024 and
  2,048.

### Node construction

- An owner constructor calls the base constructor once for each node that it owns: 36 direct calls.
  At each call, `x5` points to the 32-bit category mask, and `x3` points to a closure
  `{owner, calculation function, adjustment}` on the stack. `x1` is a `NModifierNode::EOwnerType`
  and `x2` is a `cset` of a comparison with a null-object instance. The base constructor stores the
  mask at node+0xdc, which is +0xac of the node's embedded `CModifier`. The recipe
  (`modifier_nodes`) holds these positions.
- The mask has four forms:
  - an immediate that the caller stores on its stack (for example `CArmy` 0x80200);
  - a constant in `__TEXT,__const`, such as `CColony::MODIFIER_CATEGORIES`;
  - a pointer to such a constant through the global offset table (the `CStarbase` constants);
  - a word in `__DATA,__common`, which the executable holds as zero-fill. The two `CGalacticObject`
    masks are this form. `__GLOBAL__sub_I_galactic_object.cpp` (0x1005ace30) stores
    `CFleet::MODIFIER_CATEGORIES | 0x402fda7e` at `EXTERNAL_MODIFIER_CATEGORIES` (0x1032e9050) and
    that value `| 0x100000` (Star Systems) at `MODIFIER_CATEGORIES` (0x1032e9054). It does this in
    straight-line code after its last call. `__TEXT,__init_offsets` lists 8,337 initializers.
- **The ship.** `CShip::CalculateModifier` (0x10116fffc), the calculation function of node 32,
  stores `csel(0x400ac27e, 0xc07c) | 0x10000000` (Owned Ships) at node+0xdc (0x1011700ec). The choice
  follows a virtual call (vtable+0x40) on an object found by ID, probably whether the ship carries a
  colony. An earlier branch on a global can skip the store. The constructor's mask, 0x500ac27e, is
  the first choice. No other calculation function stores to its node's mask at that offset.
- **Node 0** (`CCountryRelayNetworkManager`) has no base constructor call. Its constructor
  (0x100ad6024) builds the node inline: it stores an 8-byte literal at node+0xd8, so the mask is its
  upper half, and it calls `CModifierNodeManager::RegisterNode` itself. The probe read the mask as
  0x50bfcffe, the same as the country nodes 1 and 2. A relay network has no script scope, so the
  node 0 gap costs no config parity.

### What the method does not read

- **The filter.** `CPdxModifier<…>::AddModifierInternal` (0x1000a0b58) computes `w27 = [dest+0xac] &
  include` and keeps an entry only when `def[+0x84] & w27 != 0` and `def[+0x84] & exclude == 0`. A
  dropped entry logs nothing. This agrees with the M45-observe beta capture of SDK-497
  (`evidence/application/380aa22b6aac0e6a.txt`); match the capture to the supported build before
  you reuse it.
- **Propagation edges.** There are 54 direct calls to `CModifierNodeBase::AddModifierNode<>`. 35 pass
  include = All and exclude = 0. The constant exceptions found are: Army 0x80200; AstralRift
  0x4088000; Fleet with Owned Ships and 0x400ac2fe on five edges; Planet 0x88000; PopGroup with
  Habitability from one source; Starbase station with Starbases and 0x402ec27e; Ship excluding
  Habitability on one edge. Not resolved: the ship's include (its mask is chosen at run time), one
  starbase edge (`csel`), and six leader-trait sites that take the include as an argument. Content
  sources call `AddModifierInternal` at 611 sites; 530 pass All and 0, and a straight-line scan
  resolves 560 of them to constants (an approximate count).
- **Parse-time containers.** The registry field method reads them; see
  [container masks](#container-masks) below.

## Result on M452

34 nodes. 33 have a followed construction and a resolved mask; node 0 is a gap. Node 14 has two
owners, and node 31 (the ship) has two masks. `tests/expected/m452/modifier-nodes.json` holds the
whole answer. M452 has one country node where M451-hotfix had two: the M451-hotfix node 1, which
had no source nodes, is gone, and the colony node no longer receives it. Every other node keeps its
owner, mask and sources one number lower, so the node numbers in the engine facts above are
M451-hotfix numbers. The mask source of each node:

| Node | Owner | Mask | Mask source |
| --- | --- | --- | --- |
| 1 | `CCountry` | 0x50bfcffe | `CCountry::MODIFIER_CATEGORIES` |
| 32, 33 | `CWaystationNetwork` | 0x50bfcffe | `CCountryWaystationNetworkManager::MODIFIER_CATEGORIES` |
| 2 | `CGalacticObject` | 0x402fdafe | `CGalacticObject::EXTERNAL_MODIFIER_CATEGORIES`, set by its initializer |
| 3–7, 12 | `CGalacticObject`, `CSector` | 0x403fdafe | `CGalacticObject::MODIFIER_CATEGORIES`, set by its initializer |
| 8 | `CLeader` | All | immediate |
| 19 | `CLeader` | 0x80400 | immediate |
| 9 | `CFederation` | 0x400000 | immediate |
| 10 | `CGalacticCommunity` | 0x20000000 | immediate |
| 11, 13 | `CEspionageOperation`, `CSpyNetwork` | 0x880000 | immediate |
| 14 | `CStarbase` | 0x400840fe | `CStarbase::MODIFIER_CATEGORIES_ORBIT_MODIFIER` |
| 14 | `CMegaStructure` | All | immediate |
| 15 | `CCosmicStormInfluenceField` | 0x8000000 | immediate, in two constructors |
| 16 | `CPlanet` | 0x400a8a02 | `CColonyCarrier::PLANET_MODIFIER_CATEGORIES` |
| 17 | `CColony` | 0x400a8202 | `CColony::MODIFIER_CATEGORIES` |
| 18 | `CAstralRift` | 0x4088000 | `CAstralRift::MODIFIER_CATEGORIES_FROM_COUNTRY` |
| 20 | `CMegaStructure` | 0x9417c | immediate |
| 21 | `CMegaStructure` | 0x4008c8fe | `MODIFIER_CATEGORIES_SYSTEM` |
| 22 | `CPopGroup` | 0xa8002 | `CPopGroup::ModifierCategories` |
| 23 | `CFleet` | 0x500ac2fe | immediate |
| 24 | `CStarbase` | 0x402ec27e | `CStarbase::MODIFIER_CATEGORIES_STATION_MODIFIER` |
| 25 | `CStarbase` | All | `CStarbase::MODIFIER_CATEGORIES_COUNTRY_MODIFIER` |
| 26 | `CStarbase` | 0x4008c8fe | `CStarbase::MODIFIER_CATEGORIES_SYSTEM_MODIFIER` |
| 27 | `CStarbase` | 0x400aca7e | `CColonyCarrier::ALL_MODIFIER_CATEGORIES` |
| 28 | `CStarbase` | 0x8407c | `CStarbase::MODIFIER_CATEGORIES_DEFENSE_PLATFORM_MODIFIER` |
| 29 | `CStarbase` | 0x502ec27e | immediate |
| 30 | `CArmy` | 0x80200 | immediate; not `CArmy::MODIFIER_CATEGORIES` (0x84200) |
| 31 | `CShip` | 0x500ac27e, 0x1000c07c | immediate, then `CShip::CalculateModifier` |

Species, pop factions, deposits, ship designs and systems have no node of their own.

## Gaps

- **Where a modifier takes effect** (every answer, `OutsideMethod`). That is decided at application
  by each receiver's mask and by the include and exclude masks of each propagation edge (above). The
  answer has no supported scopes.
- **Categories with no node.** Pop Factions, AI Economy and Ship Design Stats are on no mask other
  than the all-bits masks (nodes 8, 14 for `CMegaStructure`, and 25). While node 0 is unresolved, the
  gap is `UnresolvedPath`, because an unresolved mask could keep a category.
- **Node 0**, `UnresolvedPath`: no base constructor call (above). A second construction shape, a
  direct `RegisterNode` call with the node number from the `std::function` vtable symbol, would
  close it. Add it only if Atlas needs node 0.
- **The ship's run-time choice** (`OutsideMethod`).
- **Accepted search limits.** The recalculation search reads only the calculation function's own
  body, at the mask's own offset from the node argument. A store through an adjusted node pointer,
  or in a called function, is not searched, so a `Constant` answer is not proof. The probe found
  no other store to +0xdc in any `Calculate*` or `Recalc*` function. The initializer read does not
  model the memory writes of the initializer's callees. Extend either search when a new build
  shows such a write.

## Container masks

The SDK-709 investigation found the facts; `.local/sdk-709/` has its scripts and site lists (its
`README.md` gives the commands).

### Engine facts (M451-hotfix)

- **The check.** `CPdxModifier<…>::TryReadMember(CReader&, int)` (0x10015ac00) inserts the entry,
  then tests `[this+0xac] & def[+0x84]` (0x10015ae10). On a zero result it logs `Modifier has entry
  not allowed by category: %s` through `CPdxLogFileAndLine`, with `CReader::GetFileLocationDescription()`
  as the argument. The entry stays stored. The message names the file and line, not the key or
  the category.
- **The word.** A container's category mask is the word at +0xac of its `CModifier`. Only two
  constructors take it as an argument:
  - `CStaticModifier::CStaticModifier(int, ModifierCategory)` (0x1009776f4, alias 0x10097754c)
    stores `w1` at +0xa8 and `w2` at +0xac (`stp w21, w20, [x19, #0xa8]`, 0x100977690), after its
    `SetKey` call. `CCustomDescriptionModifier(int, ModifierCategory)` calls it.
  - `CTriggeredModifierBase<T>(int, ModifierCategory, EScopeType)` passes `w2` to `T`'s category
    constructor at +0x238. `CTriggeredModifierWithTooltipData(int, ModifierCategory, EScopeType)`
    calls the Static one.
- **Default.** `CStaticModifier()` (0x1009770a8) and `CStaticModifier(CString const&)` store the
  8-byte literal at 0x102c35898, (1, 0xffffffff), at +0xa8, before their `SetKey` call. Owner
  constructors that build a `CModifierWithTooltipData` inline store the same literal
  (`CBuildingType::CBuildingType`, 0x1000cded4). A container built in one of these ways accepts
  every category. The test is `tst`, so an entry whose own mask (`def[+0x84]`) is 0 logs the
  message in any container; whether a loaded modifier can have mask 0 is not established.
- **Other writers.** A scan for `stp wA, wB, [xN, #0xa8]` and `str wN, [xM, #0xac]` found only the
  copy constructor, `Clone`, `Swap` and `operator=` of `CPdxModifier` among the modifier functions,
  and no inline category constructor in the constructor or `ReadMember` of a registry type. The
  inline default is an 8-byte store: `CEdict::CEdict` loads the literal (`ldr d0, [0x102c35898]`)
  and stores it at +0x500, the +0xa8 word of its `modifier` member at +0x458 (0x1004505d4). The
  councilor `modifier` is built the same way.
- **Calls.** There are 127 calls of the category constructors: 23 in owner constructors (members)
  and 104 clause creations in `ReadMember` functions. A clause call is direct to a branch-island
  stub (0x1029938d8 and 0x1029938e4 for Static, 0x102993908 and 0x102993914 for CustomDesc), direct
  to the Tooltip constructor, or through a `PdxMakeScopedPtr` factory (0x1000d05d4, 0x1000d0640,
  0x100ce7de8) that receives the category by reference and loads it into `w2`.
- **Argument forms.** An immediate; a `__TEXT,__const` constant (`CPopGroup::ModifierCategories`);
  a global-offset-table bind to a `<weak-def-coalesce>` constant (`CColonyCarrier::ALL_MODIFIER_CATEGORIES`,
  `CCountry::MODIFIER_CATEGORIES`); the `__DATA,__common` word `CGalacticObject::MODIFIER_CATEGORIES`
  (0x1032e9054, which the node method reads as 0x403fdafe); and, in `CStarbaseComponent<T>::ReadMember`,
  the owner's own word at member+0xac, so each starbase clause reuses the mask of the member with
  the same scope.

### Method (`registry-fields/v18`)

`Field.accepted_categories` gives the single categories that a modifier container field accepts.
The engine part is `src/engine/analysis/fields/containers.rs` (the rule), `persistent.rs`
(members) and `nested.rs` (clauses); the answer part is `src/session/container_masks.rs`.

- **Constructors.** The binding (`src/binding/binary/receivers.rs`) names each constructor whose
  first arguments are `(int, ModifierCategory` as a category constructor, and `CStaticModifier()`,
  `CStaticModifier(CString const&)` and `CCustomDescriptionModifier()` as default constructors. A
  branch-island stub has the name of its body, so it is one of them. The mask's offset in a
  container, 0xac, is `PersistentRecipe::container_mask_offset`.
- **Members.** The owner constructor walk labels each container construction at the receiver's
  owner offset: `w2` for a category constructor, every category for a default one, unknown
  otherwise. At the end of each path it also reads the mask word of each requested destination
  whose reader family is `Modifier`, which is how an inline default (the edict) is seen. Both runs
  (baseline and entered) and every owner constructor count.
- **Clauses.** The nested-collection proof labels the clause constructor call with `w2`. Its data
  holds the constant pointer slots (not the writable ones), so a mask behind a global-offset-table
  bind is known, and the initialized zero-fill words, so `CGalacticObject::MODIFIER_CATEGORIES` is
  known.
- **Initialized words.** The node method records each zero-fill mask that a node constructor reads
  and the value that its initializer leaves. The node result is read once per `Native` and shared
  with `modifier_nodes()`. A zero-fill word that no node reads stays unknown.
- **Agreement.** A container's mask is established only when every path gives it one known mask.
  An unknown argument (`category-argument`), a path with no construction (`container-unreached`)
  and different masks (`container-paths-disagree`) are gaps. A field joins the masks of every
  destination of every read alternative; a missing join, a missing destination or two masks
  (`container-destinations-disagree`) is a gap.
- **Answer.** A field of family `Modifier` or `TriggeredModifier` is `Listed` or `Unresolved` with
  an `UnresolvedPath` gap. A `modifier` key of a clause whose destination is the clause's
  `other_keys` object (the embedded `T` at +0x238) is `Enclosing`. A field of unknown family is
  `Unresolved`; the pass adds a gap only when no gap names it or a field that encloses it. Any other
  family is `NotApplicable`. The categories are single categories, so they compare with expanded
  `modifier_categories` names.

Stated assumptions, true for M451-hotfix by the scans above: no code writes a container's mask
between its construction and the check, and the galactic-object initializer runs before any parse.
Extend the walk when a new build shows another writer.

### Result on M452

`tests/expected/m452/modifier-containers.json` holds 24 registries. 19 have container fields: 69
fields (48 restricted, 21 every category) and 45 `Enclosing` clause keys, with no unresolved
modifier field. The global-offset-table form (buildings `triggered_planet_modifier`, planet) and the
`__common` form (psionic aura `triggered_system_modifier`, system) resolve. Tradition `modifier`
accepts every category and tradition `triggered_modifier` has the country mask. Edict and councilor
`modifier` accept every category through the inline default. M452 adds edict
`relay_network_modifier`, which accepts every category. The values agree with the SDK-709 sites
above.

Limits, all gaps:

- **Unjoined members:** the pop category members (`mvni.2s` halt in `CPopCategory::CPopCategory`)
  and the buildings modifier members (read through a `CModifierWithTooltipData::CSerializer` on the
  stack) have unknown family, so they are `Unresolved` with their reader gaps.
- **Nested owners** (SDK-676): `tradition_swap`, `advanced_authority_swap`, situation `stages` and
  `approach`, psionic `intensity_level`, patron `covenant` and `passive_accord`. Their fields have no
  reader family, so they are `Unresolved`; the registry field sweep gained 15 such gaps.
- **Tooltip clauses** (pop jobs): the `modifier` key has no join, so it is `Unresolved`; the clause
  itself has its mask.
- **No field:** districts and zones (no root fields), starbase components and species rights (base
  or inherited reader), traits and component templates (no discovered registry).

### Restricted containers (SDK-709 sites)

The masks of every category constructor call, read by hand; the method's result is above.
"Planet" is 0x400aca7e (`CColonyCarrier::ALL_MODIFIER_CATEGORIES`), "pop group" 0xa8002 (`CPopGroup::ModifierCategories`), "country" 0x50bfcffe (`CCountry::MODIFIER_CATEGORIES`),
"system" `CGalacticObject::MODIFIER_CATEGORIES` and "fleet" 0x400ac2fe (`CFleet::MODIFIER_CATEGORIES`).
`triggered_planet_pop_group_modifier_for_all` and `_for_species` always have the pop group mask; the
owners are buildings, colony types, deposits, districts, pop categories, pop jobs, storm types,
zones and traits.

| Owner (registry) | Key: mask |
| --- | --- |
| `CBuildingType` (`common/buildings`) | `triggered_planet_modifier`: planet; `triggered_country_modifier`, `triggered_waystation_network_country_modifier`: country; `triggered_waystation_network_system_modifier`: All |
| `CColonyType` (`common/colony_types`), `CDepositType` (`common/deposits`) | `triggered_planet_modifier`: planet |
| `CPopCategory` (`common/pop_categories`) | members `pop_group_modifier`: pop group, `planet_modifier`: planet, `country_modifier`: country; `triggered_pop_group_modifier`: pop group, `triggered_planet_modifier`: planet, `triggered_country_modifier`: country |
| `CJobType` (`common/pop_jobs`, Tooltip clauses) | `triggered_planet_modifier`: planet; `triggered_country_modifier`: country; `triggered_system_modifier`: system |
| `CCosmicStormType` (`common/storm_types`) | `triggered_planet_modifier`: planet; `triggered_country_modifier`: country; `triggered_fleet_modifier`: fleet; `triggered_ship_modifier`: 0x407c; `triggered_system_modifier`: system |
| `CEdict`, `CEthic`, `CGovernmentCouncilorType`, `CRelic`, `CSpecimen`, `CMegaStructureType` (`edicts`, `ethics`, `governments/councilors`, `relics`, `specimens`, `megastructures`) | `triggered_country_modifier`: country |
| `CGovernmentAuthorityType` (`common/governments/authorities`) | member `country_modifier`: country |
| `CFederationPerkType` (`common/federation_perks`) | `federation_triggered_modifier`: 0x400000 (Federations); `leader_triggered_modifier`, `member_triggered_modifier`: All |
| `CResolutionType` (`common/resolutions`) | `triggered_modifier`: country |
| `CSituationType` (`common/situations`), and its nested `stages` (`CSituationStage`) and `approach` (`CSituationApproach`) | `triggered_modifier`: country; `triggered_target_modifier`: planet |
| `CTraditionType` (`common/traditions`, `common/ascension_perks`), nested `tradition_swap` (`CTraditionSwap`) | `triggered_modifier`: country (CustomDesc clauses) |
| `CPsionicAuraType` (`common/patrons/psionic_auras`), nested `intensity_level` (`CIntensityLevel`) | `triggered_system_modifier`: system; `owner_`, `neutral_`, `rival_fleet_modifier`: fleet; `owner_`, `neutral_`, `rival_planet_modifier`: planet |
| `CPatronCovenantType`, `CPatronPassiveAccord` (`covenant`, `passive_accord` in `common/patrons`) | `triggered_modifier`: country |
| `CSpeciesRightBase` (all nine `common/species_rights/*` registries) | `triggered_pop_group_modifier`: pop group |
| `CDistrictType` (`common/districts`) | member `planet_modifier`: planet; `triggered_planet_modifier`: planet |
| `CZoneType::CSerializer` (`common/zones`) | `triggered_planet_modifier`, `triggered_district_planet_modifier`: planet; `triggered_country_modifier`, `triggered_district_country_modifier`: country |
| `CStarbaseComponent<CStarbaseModule>`, `<CStarbaseBuilding>` (`common/starbase_modules`, `common/starbase_buildings`) | members `station_modifier`: 0x402ec27e, `country_modifier`: All, `system_modifier`: 0x4008c8fe, `planet_modifier`: planet, `orbit_modifier`: 0x400840fe, `ship_modifier`: 0x4008407e, `defense_platform_modifier`: 0x8407c, `waystation_network_system_modifier`, `waystation_network_country_modifier`: All; `triggered_station_`, `triggered_country_`, `triggered_system_`, `triggered_planet_`, `triggered_waystation_network_system_`, `triggered_waystation_network_country_modifier`: the member mask |
| `CComponentTemplate` (`common/component_templates`, not a discovered registry) | `triggered_ship_modifier`: 0x400ac27e; `triggered_ship_design_modifier`: 0x8407c |
| `CTrait` (`common/traits`, not a discovered registry) | `triggered_planet_modifier`, `triggered_planet_growth_habitability_modifier`, `triggered_background_planet_modifier`: planet; `triggered_system_modifier`, `triggered_sector_modifier`: system; `triggered_self_modifier`, `triggered_leader_modifier`: 0x80400; `triggered_species_modifier`, `triggered_pop_group_modifier`: pop group; `triggered_councilor_modifier`, `triggered_galcom_modifier`: country; `triggered_fleet_modifier`: fleet; `triggered_army_modifier`: 0x84200; `triggered_federation_modifier`: 0x400000; `triggered_modifier`: All |

The table holds every direct, branch-island and factory call of a category constructor; a
container that none of them builds is plain only where its default construction is seen. The
registry field method gives no field for these keys in districts and zones (no root fields),
starbase modules and buildings (their keys are behind the base `CStarbaseComponent<T>::ReadMember`
call) or the species-rights registries (behind `CSpeciesRightBase::ReadMember`). Their keys come
from the token comparisons of those readers. Seven species-rights readers call the base reader;
living standards and military service types inherit it: their vtable member slots (0x10308d7e8,
0x10308d8f0) hold `CSpeciesRightBase::ReadMember` (0x100be29e4). A caller search misses that form.

- **Tradition `modifier` is plain.** `CTraditionType::CTraditionType` builds the member at +0x210
  with `CCustomDescriptionModifier()` (call at 0x100cdc228), which calls `CStaticModifier()`.
  `CTraditionType::ReadMember` reads token 0x3fff (`modifier`) into +0x210 (0x100cdc7d0);
  `CTraditionSwap` reads it into +0x170. So the naval capacity modifiers (category Countries, such as
  `country_naval_cap_add`) are accepted there, and in the country mask too.

### Diagnostic route (live, M451-hotfix)

The message reaches `observe_fixture` diagnostics with stage `engine-parser-log`, joined to the
definition, its field and the entry line:
`Modifier has entry not allowed by category:  file: common/traditions/native_category.txt line: 5`.
The live case `fixture_modifier_category` (`tests/live.rs`) checks it in one file: a
`country_naval_cap_add` in a plain tradition `modifier` gives no category diagnostic, and a
`federation_fleet_cap_add` (Federations only) in a `triggered_modifier` gives exactly one, at its
line, with `DiagnosticCoverage::Complete`. The case also checks that the static answer predicts
both. The SDK-709 answer with a third control is `.local/sdk-709/fixture-answer.json`.

### Script category keys

The method is `src/engine/analysis/category_keys.rs`; its module comment states the rule. It runs
the switch for every named token of the literal token table and keeps each nonzero mask. The empty
key comes from the exact shape of each call site: `ldr w0, [xB, #off]`, the call, `str w0`,
`cbnz w0, D`, a reload of the same token, `cmp` with a constant, `b.eq D`, and then the log
constructor call with no branch before it and `D` after it. Both readers on M451-hotfix have that
shape and compare with 0x165 (`none`). Any other shape, or sites that disagree, is a gap. The
answer gives each key's single categories; on M451-hotfix it has 24 keys (23 nonzero and `none`,
no `pop_job`), and `tests/expected/m452/modifier-category-keys.json` holds it.

The 19 `## modifier_categories` annotations of the config are on `enum[scripted_modifier_category]`
(`enums.cwt`), the value of `category` in `common/scripted_modifiers` and of `modifier_category` in
`common/economic_categories`. They are not container masks. The engine fact is
`GetModifierCategoryFromToken(int)` (0x10095d218), a switch from the value token to a mask.
`CScriptedModifier::ReadMember` stores the result at +0x74; `CEconomicCategory::ReadMember` stores it
at +0x128 and `GenerateModifiers` passes it to `FillModifierMatrix`. A zero result logs `Invalid
modifier category '%s' at %s`, except for `none` (token 0x165).

| Key | Mask | Key | Mask |
| --- | --- | --- | --- |
| `all` | All | `planet` | Planets |
| `pop_group` | Pops (0x2) | `colony` | Colony |
| `ship` | 0x407c (four ship and two station bits) | `deposit` | Deposits |
| `station` | 0xc (Orbital and Space Stations) | `megastructure` | Megastructures |
| `fleet` | Fleets | `habitability` | Habitability |
| `country` | Countries | `starbase` | Starbases |
| `army` | Armies | `economic_unit` | Economic Units |
| `leader` | Leaders | `system` | Star Systems |
| `component` | Ship Components | `federation` | Federations |
| `pop_faction` | Pop Factions | `espionage` | Espionage |
| `waystation` | Waystations | `galactic_community` | Galactic Community |
| `storm_influence_field` | Cosmic Storm Influence Field | `none` | 0, no message |

Differences from the config: `pop_job` (config: `Job`) is a token but maps to 0, so it is an invalid
category on this build; `waystation`, `galactic_community` and `storm_influence_field` are missing
from the config. The config adds AI Economy to every annotated key; the switch does not, so the
key table is the parsed mask, not the final tags of a generated modifier.
`CEconomicCategory::FillModifierTable<true>` and `<false>` compute `category | 0x1000000` (AI
Economy, 0x101c3aba4 and 0x101c3b08c) and pass it to `CModifier::TryAddDynamicModifier`
(0x101c3adb4, 0x101c3b2ac), so every modifier that an economic category generates has AI Economy,
even with `none`. Whether scripted modifiers add it is not established.

### Pitfalls

- **A mask offset of zero reads the vtable point.** The member walk reads the mask word at the
  recipe's offset; a missing offset reads the address point as a mask. Set it in every test input.
- **Composite names are not all listed.** The category-name switch names some masks that
  `modifier_categories` does not list, such as `Stations` (0xc, the `station` key). The key and
  container answers therefore list single categories.
- **The log constructor has two symbols** (complete and base). Match either.
- **The word after construction is not enough alone.** The default constructor stores its word
  before `SetKey`, which a summary call forgets, so the plain tradition container has no word at the
  end of the walk. The method takes the call's argument and the stored word together.
- **Do not list only the factory calls.** The probe found the clause sites through the
  `PdxMakeScopedPtr` factories (39 sites), so it missed the 65 direct calls: 60 to the branch-island
  stubs and 5 to the Tooltip constructor. They include colony types, deposits, edicts, situations,
  traditions, pop jobs and zones.
- **Jump tables hide tokens.** `ReadMember` functions dispatch through `ldrb` or `ldrh` jump tables
  after a range check (`add w8, w2, w8` with `w8 = -base`, then `cmp`, `b.hi` or `b.ls`). Decode the
  table to map a call to its token. The range check can be several blocks before the `br`.
- **A missing message is not acceptance.** A plain container logs the message only for an entry
  whose own mask is 0. A fixture check of a plain container must run a positive control in the
  same window.

## Comparison with `modifier_categories.cwt` (probe, for Atlas)

Atlas owns this review. "Kept" means that the node mask holds the bit.

- **Pops:** six scopes agree. The engine adds fleet, starbase and the ship with a colony. Species has
  no node.
- **Countries:** the config lists federation and colony; the federation mask is Federations only,
  and the colony mask has no Countries bit.
- **Planets:** the config adds colony; the colony mask has no Planets bit.
- **Ship categories:** the config adds colony; neither the colony nor the planet mask has ship bits.
- **Ships (0x407c):** the config's `planet` is not kept; `leader` only through an all-bits node.
- **Agree or nearly agree:** Colony, Armies, Leaders, Starbases, Espionage, Orbital and Space
  Stations, Deposits, Habitability, Astral Rift, Federations.
- **Other differences:** Waystations (the config lists ship and fleet); Owned Ships (the config lists
  galactic object, sector and colony); Galactic Community (the config says country).

The node-to-scope mapping is Atlas's: the galactic object covers nodes 3–8, 22, 27 and 33, and the
starbase has seven nodes.

## Pitfalls

- **The probe's node 15.** The probe gave the `CMegaStructure` owner of node 15 the mask of node 21
  (0x9417c). After the node 21 call, the megastructure constructor stores −1 in the same stack slot
  for node 15. Read each call's own argument.
- **Silent demangle failures.** A symbol that does not demangle keeps its raw name, so a search by
  demangled name misses it without an error. Nodes 18, 23 and 31 were missing until the limits went
  up. Compare the base constructor count with the node count.
- **Initializer selection.** Many initializers form the address of the page that holds the two
  galactic-object masks. Select an initializer only when it stores, at the word's offset, through the
  register that it loaded with the page. Otherwise unrelated initializers run, and some of them fall
  off the end after a call to `__Unwind_Resume`.
- **Path count.** An owner constructor splits on each `cset` of an unknown comparison (one for each
  node that it constructs; 7 in `CStarbase::CStarbase()`) and on each loop pass. The second country
  node needs 1,296 paths. The method raises the run's path limit to 4,096. Read the mask word once
  for each distinct address, not once for each path: an initializer search for each path took
  5 seconds on one galactic-object call.
- **Replicating loads.** The ship, fleet, megastructure and galactic-community constructors run
  `ld1r` before the call. The shared evaluator now runs it; before, it stopped those paths.
