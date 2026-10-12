# Loader rules

`Native::loader_rules(registry)` (`loader-rules/v3`) states how a registry's loader selects its
files (the file-name filters and the subdirectories that it reads) and how it names each form of
definition in them. The folder is the registry name. The duplicate-definition rule, the file order
and the mod-over-vanilla rule (SDK-790) are gaps whose subjects are `duplicate_rule` and
`file_order`. The code is `src/engine/analysis/loader_rules.rs`, `directories::enumeration_calls`,
`src/binding/binary/definition_names.rs`, `src/binding/analysis.rs` (`loader_evidence`) and
`src/session/loader_rules.rs`.

## The file enumeration on M452

Facts from `c621723d…` (M452). Every registry loader lists its files with
`VFSGetEnumeratedFiles(char const* directory, CPdxArray<CString, int>& files, char const* suffix,
char const* substring, int depth)` (`0x102542e70`).

- The function formats `"%s|%s|%s|%d"` from the four values as a key into a table of
  pre-enumerated results (`SPreEnumerateFilesData`). On a miss it logs
  `Failed to find pre specified enumerate` and calls
  `VFSEnumerateFiles(…)` (`0x102547350`) with the same arguments. Both give the same files, since
  the key holds every argument.
- `VFSEnumerateFiles` calls `EnumerateFilesRecursive(…, CString const& prefix, int depth = 0,
  int max)` (`0x102547968`) with `max` = the `depth` argument, then sorts the names and removes
  repeats. The callback is `VFSRecursiveEnumerator<CPdxArray<CString, int>>::Enumerate`
  (`0x102548108`), through `PHYSFS_enumerate`.
- The callback skips a name that starts with `.`. It keeps a directory entry
  (`PHYSFS_FileType` 1) for recursion only when `max != 0` (`ldr w8,[x20,#0x3c]; cbz`).
- For a file, `suffix` (`+0x20`) must equal the end of the name (`strlen`, then `strcmp` of the
  tail), and `substring` (`+0x28`) must occur in it (`strstr`). A null pointer is no filter.
- `EnumerateFilesRecursive` returns with `Reached max depth when enumerating files recursive`
  when `max >= 1 && depth > max`; each subdirectory is read with `depth + 1`. So `max` 0 reads
  only the folder, `n > 0` reads the folder and `n` levels below it, and a negative `max` reads
  every level. The answer states these as `Recursion::None`, `Levels(n)` and `Unlimited`.

The callers:

- **Template loaders.** Each of the 164 `TSingleObjectGameDatabase<…>::Init()` passes the
  `CString` member at `this + 0x10` (the base-constructor directory), `null`, `null` and `0`. It
  then calls `LoadFile(char const*, bool)` through vtable `+0x20` for every name, and `LoadFile`
  lexes the file with no name check. A template registry therefore reads every file of any
  extension, in the folder only. A database class that overrides `Init` (`CArmyTypeDatabase`,
  `CBuildingTypeDatabase`, …) calls the template `Init`; no template database class enumerates on
  its own.
- **Custom loaders.** The database class's own functions pass the folder as a literal, or load
  it from a global `char const*` (`CPlanetClassDatabase`, `CEmpireFlagDatabase`).
  `CDesignerDatabase::Init` (`common/global_ship_designs`) and `CTerraformDatabase::InitInstance`
  pass `".txt"` and `-1`. `CWeaponTagDatabase`, `CAmbientObjectDatabase`, `CShipBehaviorDatabase`,
  `CStartScreenMessageDatabase`, `CWorldGfxDatabase`, `CPrescriptedSpeciesDatabase`,
  `CEmpireDesignDatabase`, `CPlanetClassDatabase` and `CEventManager::ReadEvents` pass `".txt"`
  and `0`. `CEmpireFlagDatabase` (`flags`) passes `null`, `null` and `-1`. The others pass
  `null`, `null` and `0`.

## Method

The loaders of a registry are each template candidate that it names (the functions of the
loader class and the database class) and each scheduled database whose own loader enumerates it.
`prescripted_countries` has two (`CEmpireDesignDatabase`, `CPrescriptedSpeciesDatabase`); both
pass `".txt"` and `0`.

For each call, `directories::enumeration_calls` keeps the values that the call's own basic block
sets in `x2`, `x3` and `w4`: an `adrp` and `add` to a string literal, `mov xN, #0` or
`mov xN, xzr` (a null pointer), or `mov wN, #imm`. A property is `Known` when every call passes the
same value. A value that is not a literal, calls that disagree, or no call leave it `Unresolved`
with an `UnresolvedPath` gap whose subject is the property (`extension`, `name_contains`,
`recursion`).

- Pitfall: the template `Init` loads the directory on two paths (the short-string `csel` shape,
  `tbnz`, `b`) and sets `x2`, `x3` and `w4` after they join. The registry-name scan of
  `directories.rs` stops at the `b` and does not read the call. The block scan starts a new block
  with every register unknown at each branch target and after each unconditional branch.
- Pitfall: `w4` is an `int`; `mov w4, #-1` is `movn` and gives `0xffffffff` in the register.

Outside the method: a file that the loader opens by a literal name, a check of the name after
the enumeration, and the mounted directories that the enumeration searches.

## Single named files

The config reads three types from one named file in `common` (`path_file`, in
`common/zzz_common_folder_elements.cwt`). Their loaders are not registries in `registries()`:

- `common/alerts.txt`: `CAlertSystem::InitTypes` (`0x1000692e4`) forms the literal. The
  scheduled `CAlertSystem` has no enumeration call, so it stays in the `UnnamedRegistries` gap.
- `common/achievements.txt`: `CGameApplication::InitGame` (`+0x29c`, `+0x2c4`). No scheduled
  database loads it, so it is outside the `registries()` search.

`loader_rules` gives `UnknownRegistry` for these names. A single-file loader becomes a registry
question only when `registries()` lists it.

## Definition names

Facts from `c621723d…` (M452), read with `examples/inspect`.

- **Template loaders: the root key.** `TSingleObjectGameDatabase<D, I, b>::LoadFile` lexes the
  file and calls `LoadFromReader(CReader&, bool)`. For each root statement it builds a `CString`
  from the statement key (`[reader, #0x48]`), or from the token text after an advanced statement
  (`CLexer::Tok()` `+0x10`), searches the items by that key (`ReadExistingEntry` on a hit), and
  otherwise builds the item from it: `I::I(int, CString const&)` in
  `CTraditionTypeDatabase`'s loader (`0x100ce5e0c`), or `ReadNewEntry(CReader&, CString const&)`
  with inlined construction in `CDiplomacyEconomyDatabase`'s (`0x100403f1c`). A statement that is
  not `key = {` logs `Expected format is "key={"`. `inline_script` is expanded, not a definition.
- **Events: two forms.** `CEventManager::ReadEventsFile(CReader&, int&, bool)` (`0x1004d5944`)
  switches on the root key. `namespace = x` builds a `CString` from the statement's value
  (`[reader, #0x288]`) and registers a `CNameSpace` through
  `CHashTable<CString, CNameSpace>::Find` (`0x1004d5b8c`). An event kind builds a `CEvent` and
  reads it; `CEvent::ReadMember` reads `id` with `CEventManager::IsValidIDTag` and
  `GetIDInNameSpace` (`0x1004bc9e8`) and stores the handle at `+8`. The loader then logs
  `Event at %s has no ID.` or `an event with id [%s] already exists!` and adds the event to its
  lists by that handle. `inline_script` is expanded and any other key logs
  `Corrupt Event Table Entry`. The accepted event keys are SDK-702's.
- **Tradition swaps: the `name` field.** `CTraditionSwap::ReadMember` (`0x100cdd144`) reads
  token `0x1b` (`name`) with `CReader::Read(CString&, bool)` into `+0xf8`. The resolver lambda of
  `has_active_tradition` (`0x1021bedb0`) compares the key with each tradition's key and then
  with `+0xf8` of each swap that does not inherit effects. `CTraditionType` is also the item of
  `common/ascension_perks`, so its swaps have the same name. A swap's name identifies it only
  together with its owner ([registry fields](registry-fields.md#registry-names)).
- **Other swap blocks.** Each swap class reads `name` (`0x1b`) into its own member.
  - `CTechnology::ReadMember` (`0x100c8098c`) reads `technology_swap` into a `CTechnologySwap`;
    `CTechnologySwap::ReadMember` (`0x100ca7d9c`) reads `name` into `+0x308`, and `InitPostRead`
    (`0x100ca8278`) forms the swap's localization key from it. No lookup compares it.
  - `CGovernmentAuthorityType::ReadMember` (`0x1007f3b5c`) reads `advanced_authority_swap` into a
    `CAdvancedAuthoritySwap`, whose `ReadMember` (`0x1007f28f4`) reads `name` into `+0x2c8`.
    `CGovernmentAuthorityType::GetSwapByKey(CString)` (`0x1007f6d50`) and `GetSwapTooltip`
    (`0x1007f7a44`) compare a key with `+0x2c8` of each of the owner's swaps.
  - `CGovernmentCivicType::ReadMember` (`0x1007fc04c`) reads `swap_type` into a
    `CGovernmentCivicTypeSwap` (`name` into `+0xc0`). `Initialize(CString const&)`
    (`0x1007fad94`) uses the owner's name when `name` is empty.
  - `CJobType::ReadMember` (`0x1008b426c`) reads `swappable_data`; its serializer
    (`CSwapCollection<SPopJobTypeSwap, …>::CSerializer::ReadMember`, `0x1008bea40`) reads each
    `swap_type` into a `CSwappableBase`, whose constructor (`0x100c7185c`) copies the job's key
    into the name (`+0xf0`) before `CSwappableBase::ReadMember` (`0x100c71e44`) reads `name`.
  - `SPopCategorySwap` (`common/pop_categories`) has the same collection shape; it is not stated.
- **Custom loaders.** Each loader below loops `CReader::ReadSimpleStatement()` over a file unless
  stated. `ReadSimpleStatement` does not expand `inline_script`; `CPersistent::Read` does, inside
  a definition's block.
  - Root key (the loader builds the item from the statement key, and the lookup compares it):
    `CFallenEmpiresDatabase::InitFromFile` (`0x10050e054`, `GetFallenEmpireByName`),
    `COnActionDatabase::Init` (`0x1009b09a4`, `GetOnActionList`; a repeated key merges),
    `COpinionModifierDatabase::Init` (`0x1009c1cf4`, `GetModifier`),
    `CStrategicResourceDatabase::Init` (`0x100c647f4`, `GetResource`) and
    `CTraitDatabase::ReadTraits` (`0x100cffb7c`, `GetTrait`; it expands `inline_script` itself).
  - One root key, named by a field: `CShipBehaviorDatabase` takes only `ship_behavior`
    (`0x311c`); `CShipBehaviorReader::CreateAndRead` (`0x1011b79b4`) reads `name` into `+8`, which
    `AddBehavior` hashes and compares. `CSpecialProjectDatabase::Init` (`0x100b9ddec`) takes only
    `special_project` (`0x2d8b`); `CSpecialProjectType::ReadMember` reads `key` (`0xdc`) into
    `+8`, which the second pass of `Init` compares. `CWorldGfxDatabase::ReadMember`
    (`0x101bc252c`) takes only `gfx_settings` (`0x2e6f`); `CWorldGraphicalMapData::ReadMember`
    reads `world` (`0x3fe8`) into `+8` and its hash into `+0x30`, which `GetWorldGfxIndex`
    compares. Other root keys are skipped, or reported for `gfx_settings`.
  - No name: `CColorsDatabase::Init` (`0x1012fb45c`) reads six fixed keys (`species`, `country`,
    …) of anonymous `rgb`/`hsv` entries, read by index. `CStartScreenMessageDatabase::InitInstance`
    (`0x100c5db64`) reads `part` blocks into an array by their `location`; `localization` is text
    that `InitPostRead` localizes, and no function compares it. `CTerraformDatabase::InitInstance`
    (`0x100cb0254`) reads `terraform_link` blocks, read by index (`GetLink(int)`).
  - A field that changes the name: `CAmbientObjectData::ReadMember` (`0x100f34ef0`) reads `name`
    (`0x1b`) and `tag` (`0x3ff4`) into the same member, and `FindObjectData` compares its hash;
    the loader takes only `ambient_object` (`0x3109`). `CShipDesign::ReadMember` (`0x1011bd5e4`)
    reads `name` as a `CPersistentName` (`CPersistentName::Read`, `0x100a1c0d8`): a block value
    `name = { key = x … }` names the design by `key`; `FindShipDesignByName` compares it; the
    loader takes only `ship_design` (`0x2c6f`). `CStaticModifier::ReadMember` (`0x100977690`)
    calls `SetKey` for a `key` field (`0xdc`) after the root key set it.
    `CEmpireDesign::ReadMember` (`0x100468d84`) reads `key` into the root key's member `+8`.
  - A reserved root key: `CPlanetClassDatabase::Init` (`0x10115a26c`) builds a
    `CPlanetClassRandomList` from `random_list` (`0x2714`) and a planet class from any other key.
    `CEmpireDesignDatabase::InitFromFile` (`0x10046cb90`) reads `default` into the database.
  - Two loaders of `prescripted_countries`: the design loader names every other root key a
    design; `CPrescriptedSpeciesDatabase::InitInstance` (`0x100aa72d4`) takes only
    `prescripted_country` (`0x2d93`) and names it by `key` (`Find`, `0x100aa77a4`).
  - Names that no statement form holds: `CWeaponTagDatabase::InitInstance` (`0x1001d9f8c`) passes
    every token of the file to `SComponentTag::RegisterTag`. `CSpeciesNamesDatabase::Init`
    (`0x100bab6e0`) names a collection by a species-class root key or by a key under
    `named_lists`, and each name set by its key inside a collection. `CDiploPhraseDatabase::Init`
    (`0x1003fe2bc`) names a phrase by its key under `greetings`, or two levels under any other
    root key. `CEmpireFlagDatabase::Init` (`0x10047c768`) takes each `.dds` file as a flag named by
    its file name, in a category named by its folder, and reads `colors.txt` and `usage.txt`
    apart.

### Stated rules (recorded manual exception)

- **Claim.** A template loader names each root definition by its key
  (`Root` / `Key`). The events loader names a `namespace = x` statement by its value
  (`RootKey("namespace")` / `Value`) and every other accepted root statement by its `id` field
  (`Root` / `Field("id")`). An item of class `CTraditionType`, `CTechnology` or
  `CGovernmentAuthorityType` names each `tradition_swap`, `technology_swap` or
  `advanced_authority_swap` block by its `name` field (`Nested { [block] }` / `Field("name")`).
  The custom loaders above name their definitions by root key (`Root` / `Key`), by a field of
  one root key (`RootKey(k)` / `Field(f)`), or not at all (no form). Each other custom loader,
  and the civic and job swaps, has an obstacle that no form states.
- **Conditions.** Each rule's functions are bound: the candidate's `LoadFromReader`;
  `ReadEventsFile` and the `CNameSpace` hash `Find`; `ReadEventsFile`, `CEvent::ReadMember` and
  `GetIDInNameSpace`; the item's `ReadMember` and the swap reader; and the loader and lookup
  functions of each custom rule (`src/binding/binary/definition_names.rs`). A rule whose
  function is missing makes the property `Partial`, or `Unresolved` when no form holds, with an
  `UnresolvedPath` gap whose subject is `definition_names`.
- **Obstacles.** A rule with an obstacle states no form for the definitions it affects; each
  obstacle is an `OutsideMethod` gap whose subject is `definition_names` and whose detail names
  it. A form is stated only when it names every definition it selects: a field that renames a
  definition, a second field that sets the name, a block-valued name, a reserved root key or a
  second loader of the same statements would make a stated form wrong for some definitions.
- **Obstacle.** The name reaches the item, the namespace table or the swap's member through
  stack copies, inlined construction and a later lookup or list insertion. No method follows a
  key or a field value into the member that the loader or a lookup compares, and a constructor's
  signature does not show where its string comes from.
- **Removal route.** A reader that follows a root key or a field value into the member that the
  loader or a lookup compares.

The checks are the disassembly above for both template shapes, the fixture observation (a
template definition is found by its root key), and parity.

The answer has no file-name form, no bare-token form, no nested-key form at a variable depth and
no skipped root key; the loaders that need one have an obstacle gap. A `Known` answer with no
form means that the loader names none of its definitions.

Pitfall: a constructor that takes `(int, CString const&)` is not proof of the root key; some
template loaders reach the item through `ReadNewEntry`, and the signature does not show where the
string comes from.

### Where the config differs

- `common/diplomacy_economy`: the config's `type_per_file` and `name_from_file` make each file one
  definition named by the file. The template loader names each root key (`once`,
  `per_relation`) as a definition.
- `common/country_container`: the config's `skip_root_key = collection` skips the root key; the
  template loader names the definition `collection`.
- `swapped_technology` and `swapped_authority` agree (`name`). `swapped_civic` and `swapped_job`
  take `name`, but a swap without it takes its name from its owner.
- `common/ambient_objects` (`name_field = name`), `common/global_ship_designs` (`name`),
  `common/ship_behaviors` (`name`), `common/special_projects` (`key`) and `gfx/worldgfx` (`world`)
  agree with the loader. For ambient objects `tag` also sets the name, and a design's `name` can
  be a block named by its `key`; these two keep an obstacle gap.
- `common/start_screen_messages` (`name_field = localization`): the loader names no part;
  `localization` is display text that no function compares, and parts with the same text are all
  kept.
- `common/species_names` (`name_field = name`): the loader names collections and name sets by
  keys (a species class, a key under `named_lists`, and each set's key), not by a `name` field.

## Result on M452

All 185 listed registries have every file-selection property `Known`; none is unresolved.

| Selection | Registries |
| --- | ---: |
| Every file, folder only | 174 |
| `.txt`, folder only | 8 |
| `.txt`, every level | 2 (`common/global_ship_designs`, `common/terraform`) |
| Every file, every level | 1 (`flags`) |

Definition names: 174 `Known`, 2 `Partial` and 9 `Unresolved`; each `Partial` or `Unresolved`
answer has an obstacle gap.

| Forms | Registries |
| --- | ---: |
| Root key | 163 (158 template, 5 custom) |
| Root key, and swaps by `name` | 4 (`common/traditions`, `common/ascension_perks`, `common/technology`, `common/governments/authorities`) |
| One root key, by a field | 3 (`common/ship_behaviors` by `name`, `common/special_projects` by `key`, `gfx/worldgfx` by `world`) |
| `namespace` by value, and other root keys by `id` | 1 (`events`) |
| No named definition (`Known`, no form) | 3 (`common/colors`, `common/start_screen_messages`, `common/terraform`) |
| Partial: root key; the swap has an obstacle | 2 (`common/governments/civics`, `common/pop_jobs`) |
| Unresolved: an obstacle | 9 (`common/ambient_objects`, `common/component_tags`, `common/diplo_phrases`, `common/global_ship_designs`, `common/planet_classes`, `common/species_names`, `common/static_modifiers`, `flags`, `prescripted_countries`) |

`tests/expected/m452/loader-rules.json` holds each registry's answer; `cargo parity` checks it,
`loader_rules_state_the_file_selection_of_every_listed_registry` checks that it lists every
registry in `registries()`, and
`loader_rules_state_the_definition_name_forms_of_template_and_event_loaders` checks
`common/traditions`, `events` and `common/diplomacy_economy`, and
`loader_rules_state_a_rule_or_an_obstacle_for_every_custom_loader` checks that no registry is
left with no stated rule. The parity file keeps every `definition_names` gap with its kind.
