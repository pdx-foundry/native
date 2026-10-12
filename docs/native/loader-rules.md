# Loader rules

`Native::loader_rules(registry)` (`loader-rules/v3`) states how a registry's loader selects its
files (the file-name filters and the subdirectories that it reads), what it does with a later
definition of a key that it holds, and how it names each form of definition. The folder is the
registry name. The file order and the mod-over-vanilla rule are stated once on this page, for the
build; each answer has an `OutsideMethod` gap for each (subjects `file_order`,
`mod_over_vanilla`) that says that no fixture verifies it. The code is
`src/engine/analysis/loader_rules.rs`, `directories::enumeration_calls`,
`src/binding/binary/definition_names.rs`, `src/binding/binary/references.rs`
(`existing_entry_rules`), `src/binding/analysis.rs` (`loader_evidence`) and
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

Outside the method: a file that the loader opens by a literal name and a check of the name after
the enumeration. The mounted directories that the enumeration searches are the
[mod-over-vanilla rule](#mod-over-vanilla).

## Single named files

The config reads three types from one named file in `common` (`path_file`, in
`common/zzz_common_folder_elements.cwt`). Their loaders are not registries in `registries()`:

- `common/alerts.txt`: `CAlertSystem::InitTypes` (`0x1000692e4`) forms the literal. The
  scheduled `CAlertSystem` has no enumeration call, so it stays in the `UnnamedRegistries` gap.
- `common/achievements.txt`: `CGameApplication::InitGame` (`+0x29c`, `+0x2c4`). No scheduled
  database loads it, so it is outside the `registries()` search.

`loader_rules` gives `UnknownRegistry` for these names. A single-file loader becomes a registry
question only when `registries()` lists it.

## Duplicate definitions

Facts from `c621723d…` (M452).

- `TSingleObjectGameDatabase<Db, Item, B>::LoadFromReader` scans the item array (`+0x48`, count
  `+0x54`) for an equal key: length, then bytes. For a hit it calls the database's vtable slot at
  `+0x28` from the address point with the existing item in `x3`. A later definition in the same
  file and one in a later file take the same path; the registry holds one item per key.
- The last argument (`w4`) is `false` at the first load. `Init` calls `LoadFile` (slot `+0x20`)
  with `mov w2, #0`; `LoadFile` passes its flag to `LoadFromReader` (`mov x19, x2` …
  `mov x2, x19`); `LoadFromReader` passes `mov w4, #0` on the path for a database word `+0x70` of
  1 and its own flag (`mov x22, x2` … `mov x4, x22`) on the other. A text survey of all 164
  template databases with the inspector found this chain in each. The method assumes it and does
  not read it; a reload that passes `true` is outside it.
- Every one of the 164 database vtables holds a template `ReadExistingEntry` at that slot: 158
  `TSingleObjectGameDatabase<…>::ReadExistingEntry` (151 with `false`, 7 with `true`) and 6
  `TExtendableObjectGameDatabase<…>::ReadExistingEntry`.
- `TSingleObjectGameDatabase<…>::ReadExistingEntry` (`0x10003ff78` for advisor voice types): with
  `false` it logs `Object with key: %s already exists, using the one at %s` (the later
  definition's location), calls the item's destructor (slot 0), zeroes the item, constructs it
  with its old index (`ldr w24, [x19, #8]`) and the key, and reads the later definition into it
  (`CPersistent::Read`, slot `0x20` of the vtable at `+0x38`). So the last definition wins, and the
  item keeps the earlier one's index and place. With `true` it does the same without the log.
- `TExtendableObjectGameDatabase<…>::ReadExistingEntry` (`0x1007f8f28` for authorities): with
  `false` it tail-calls `CPersistent::Read` on the existing item, with no destructor and no log,
  so the later definition merges into the earlier item. With `true` it destroys, zeroes,
  constructs and reads, as the template above.

### Method

`existing_entry_rules` (binding) finds the one function in each template database's primary
vtable whose name holds `::ReadExistingEntry(CReader&, CString const&, `.
`existing_entry_rule` (engine) reads its body. The test of `w4` must be the first branch
(`tbz`/`tbnz` bit 0, or `cbz`/`cbnz`); the scan runs the entry block, then the `false` path in
address order, steps over each conditional branch and follows each `b` inside the body. It tracks
the registers that hold the item (`x3`) or a subobject, a vtable pointer of a subobject, a slot of
that vtable, the reader (`x1`) and constants. The read is the first call that receives an item
subobject in `x0` and the reader in `x1`.

- No destruction before the read gives `Merges`. A destruction, then a rebuild, then the read gives
  `LastWins`. A destruction with no rebuild, any other call through a register, or a branch out
  of the body that is not the read gives no rule (`UnreadBody`).
- Destruction: a call through slot 0 of the primary vtable with the whole item in `x0`, or a
  direct call that receives the item or a subobject (an inline destructor calls each member's
  destructor). Rebuild: after a destruction, a store at the item's start (the primary vtable
  pointer, or the zeroing) or a direct call that receives the whole item.
- A return after a rebuild also gives `LastWins`: the handlers of `common/job_tags` and
  `common/trait_tags` (database word `+0x70` is 1) build the item again and read nothing.
- The registry's template databases must agree. A registry with no template database (a custom
  loader) gives `NotFollowed`.
- Pitfall: most handlers inline the constructor, the destructor or both (strings, member arrays,
  vtable stores), so a rule that asks for an item-constructor call misses 36 registries.
- Pitfall: the read is often devirtualized: `add x0, x20, #0x38; mov x1, x19;
  bl CPersistent::Read(CReader&)`, or the subobject address comes from a pre-index store
  (`str x9, [x0, #0x38]!`) or a register constant (`mov w8, #0x1a18; add x0, x19, x8`).
- Pitfall: on Apple ARM64 a constructor or destructor returns `this`, and the code then uses `x0`
  without a move. The scan keeps the item in `x0` after a direct call that received it.
- Pitfall: `x1` holds the reader on entry, so the destructor call (which ignores `x1`) has the
  reader in `x1`. The scan tests for the destructor before it tests for the read.

The method does not read `common/component_slot_templates`: its handler reads through a
serializer object on the stack (`CReader::Read(CPersistent&)` with the reader in `x0`). Its rule
stays `Unresolved`.

### Custom loaders

A registry whose own database class loads it has no `ReadExistingEntry`, so its rule is
`Unresolved` with a `duplicate_rule` gap. Each class has its own insert code; examples read by
hand, which no method checks:

- `CTraitDatabase::ReadTraits` constructs each trait and appends it to the array, with no check of
  the key.
- `CShipBehaviorDatabase::AddBehavior` finds an existing behavior by the key's hash; for a valid
  one it logs `A behavior with the name "%s" already exists! names should be unique!` and deletes
  the new one.
- `CDesignerDatabase::Init` (`A ship design named "%s" already exists! %s`) and
  `CEventManager::ReadEventsFile` (`an event with id [%s] already exists! %s`) log a duplicate;
  what they keep is not read.

## File order

Facts from `c621723d…` (M452). `VFSEnumerateFiles` (`0x102547350`) runs
`EnumerateFilesRecursive`, then `std::__stable_sort` with `CPdxArray<CString, int>::SLessThan`,
which calls `CString::operator<` (`0x102526e28`: `memcmp` over the shorter length, then the
shorter name first), then `std::__unique` with `equal_to`. So the loader receives each name once,
in byte order of its path below the folder: case-sensitive, so `B.txt` before `a.txt`, and
`_` (`0x5f`) after `/` (`0x2f`) and after upper-case letters. The template `Init` loads the names in
array order (index from 0, step `0x28`). A pre-enumerated result has the same arguments and comes
from the same function.

Limits: no fixture verifies the order. A custom loader's own loop over the names is not read.

## Mod over vanilla

Facts from `c621723d…` (M452).

- Every mount passes `appendToPath = 0` to `PHYSFS_mount` (`VFSMount(char const**, int)` at
  `0x102545f7c`, `VFSMount(char const*, char const*)`, `VFSMountBaseDir`), so each new mount goes
  first in the search path.
- `StartVFS` calls `VFSInit` (`+0x38`), which mounts the game directory, and then
  `CDLCManager::AppendVerifiedToPath` (`+0x1e8`). That function sorts the DLC and mod descriptors
  with `std::__stable_sort` and `CDLCGreater`, mounts each, and adds each `replace_path` folder with
  `VFSAddSuppressedFolder`.
- PhysFS lists a name from every search-path entry, and the sort and `__unique` above keep it
  once. PhysFS opens a name from the first search-path entry that holds it (library behavior; the
  open path is not decoded here).

So a mod's file with the same path below the folder as a game file replaces it, and other files
join the game's files in one sorted list. Then the duplicate rule applies to keys across the
files.

Limits: no fixture verifies these rules (decision D3; SDK-803 may add a second mount). The order
between two mods (`CDLCGreater`) and the effect of a suppressed folder on the enumeration are not
established.

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
  together with its owner ([registry fields](registry-fields.md#registry-names)); the other swap
  classes have no name lookup and are not stated.

### Stated rules (recorded manual exception)

- **Claim.** A template loader names each root definition by its key
  (`Root` / `Key`). The events loader names a `namespace = x` statement by its value
  (`RootKey("namespace")` / `Value`) and every other accepted root statement by its `id` field
  (`Root` / `Field("id")`). A registry whose item is `CTraditionType` names each
  `tradition_swap` block by its `name` field (`Nested { ["tradition_swap"] }` / `Field("name")`).
- **Conditions.** Each rule's functions are bound: the candidate's `LoadFromReader`;
  `ReadEventsFile` and the `CNameSpace` hash `Find`; `ReadEventsFile`, `CEvent::ReadMember` and
  `GetIDInNameSpace`; `CTraditionType::ReadMember` and `CTraditionSwap::ReadMember`. A rule whose
  function is missing makes the property `Partial`, or `Unresolved` when no rule holds, with a
  gap whose subject is `definition_names`.
- **Obstacle.** The name reaches the item, the namespace table or the swap's member through
  stack copies, inlined construction and a later lookup or list insertion. No method follows a
  key or a field value into the member that the loader or a lookup compares, and a constructor's
  signature does not show where its string comes from.
- **Removal route.** A reader that follows a root key or a field value into the member that the
  loader or a lookup compares.

The checks are the disassembly above for both template shapes, the fixture observation (a
template definition is found by its root key), and parity.

The method states no file-name form and no skipped wrapper key: no stated rule shows one. That
does not prove that the 20 unstated custom loaders have none.

Pitfall: a constructor that takes `(int, CString const&)` is not proof of the root key; some
template loaders reach the item through `ReadNewEntry`, and the signature does not show where the
string comes from.

### Where the config differs

- `common/diplomacy_economy`: the config's `type_per_file` and `name_from_file` make each file one
  definition named by the file. The template loader names each root key (`once`,
  `per_relation`) as a definition.
- `common/country_container`: the config's `skip_root_key = collection` skips the root key; the
  template loader names the definition `collection`.
- The `swapped_*` types other than `swapped_tradition` and `swapped_ascension_perk` have no
  stated form.

## Result on M452

All 185 listed registries have every file-selection property `Known`; none is unresolved.

| Selection | Registries |
| --- | ---: |
| Every file, folder only | 174 |
| `.txt`, folder only | 8 |
| `.txt`, every level | 2 (`common/global_ship_designs`, `common/terraform`) |
| Every file, every level | 1 (`flags`) |

Definition names: 165 `Known`, 20 `Unresolved`, none `Partial`.

| Forms | Registries |
| --- | ---: |
| Root key | 162 |
| Root key, and `tradition_swap` by `name` | 2 (`common/traditions`, `common/ascension_perks`) |
| `namespace` by value, and other root keys by `id` | 1 (`events`) |
| Unresolved: a custom loader with no stated rule | 20 |

The 20 unresolved registries are the custom loaders other than `events`: `common/ambient_objects`,
`common/colors`, `common/component_tags`, `common/diplo_phrases`, `common/fallen_empires`,
`common/global_ship_designs`, `common/on_actions`, `common/opinion_modifiers`,
`common/planet_classes`, `common/ship_behaviors`, `common/special_projects`, `common/species_names`,
`common/start_screen_messages`, `common/static_modifiers`, `common/strategic_resources`,
`common/terraform`, `common/traits`, `flags`, `gfx/worldgfx` and `prescripted_countries`.

| Duplicate rule | Registries |
| --- | ---: |
| `LastWins` | 157 |
| `Merges` | 6 (`common/country_limits/ownership_limits`, `common/economic_plans`, `common/governments/authorities`, `common/portrait_categories`, `common/portrait_sets`, `common/species_classes`) |
| `Unresolved`, custom loader | 21 |
| `Unresolved`, handler not read | 1 (`common/component_slot_templates`) |

`tests/expected/m452/loader-rules.json` holds each registry's answer; `cargo parity` checks it,
`loader_rules_state_the_file_selection_of_every_listed_registry` checks that it lists every
registry in `registries()`, and
`loader_rules_state_the_definition_name_forms_of_template_and_event_loaders` checks
`common/traditions`, `events` and `common/diplomacy_economy`.
