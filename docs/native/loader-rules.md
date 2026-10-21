# Loader rules

`Native::loader_rules(registry)` (`loader-rules/v10`) states how a registry's loader selects its
files (the file-name filters and the subdirectories that it reads), what it does with a later
definition of a key that it holds, and how it names each form of definition. The folder is the
registry name. The file order and the mod-over-vanilla rule are stated once on this page, for the
build; each answer has an `OutsideMethod` gap for each (subjects `file_order`,
`mod_over_vanilla`). [Fixture controls](#fixture-controls) lists the rules that a live fixture
checks and the ones that it cannot reach. The code is
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
  A class outside the schedule that enumerates and reads statements is a loader too
  ([custom-loader names](registry-fields.md#custom-loader-names)).
  `CDesignerDatabase::Init` (`common/global_ship_designs`), `CTerraformDatabase::InitInstance`,
  `CNameListDatabase::Init` (`common/name_lists`, the text of its `CString` member) and
  `CSongDatabase::InitInstance` (`music`) pass `".txt"` and `-1`. `CGfxCultureDatabase::Init`
  and `CGameRules::InitInstance` pass `".txt"` and `0`; the two calls of `CRandomNameDatabase`
  pass `null`, `null` and `0`. `CWeaponTagDatabase`, `CAmbientObjectDatabase`, `CShipBehaviorDatabase`,
  `CStartScreenMessageDatabase`, `CWorldGfxDatabase`, `CPrescriptedSpeciesDatabase`,
  `CEmpireDesignDatabase`, `CPlanetClassDatabase`, `CEventManager::ReadEvents` and the three calls
  of `CShipDesignTemplatesDatabase::Init(bool)` pass `".txt"` and `0`. `CEmpireFlagDatabase` (`flags`) passes `null`, `null` and `-1`. The others pass
  `null`, `null` and `0`.

## Method

The loaders of a registry are each template candidate that it names (the functions of the
loader class and the database class) and each custom-loader database whose own loader enumerates
it.
`prescripted_countries` has two (`CEmpireDesignDatabase`, `CPrescriptedSpeciesDatabase`); both
pass `".txt"` and `0`.

A database that loads several directories is a loader of each, and the method reads all of its
enumeration calls: the three directories of `CShipDesignTemplatesDatabase` share its three calls.
Calls that agree give the property; calls that differ leave it `Unresolved`, never another
directory's value. Its definition-name rule is stated per directory.

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

**Reviewed reason (Milestone 5, 2026-10-20 and 2026-10-21).** The config types `alert`,
`alert_icon`, `achievement` and `gamesetup_setting_category` keep no Native answer. "A reason is the last resort"
([roadmap](../roadmap.md#milestone-5-acceptance)); these are the routes and why each fails on
M452:

| Type | Loader (M452) | Static extraction | Fixture probe |
| --- | --- | --- | --- |
| `alert` | `CAlertSystem::InitTypes()` lexes `common/alerts.txt` (`0x1000692e4`) and loops `ReadSimpleStatement()`; root key `alerts` (`0x4096`, branch at `0x1000693d8`) holds one block per alert type | Every registry method starts from a registry, and a registry is a content directory (decision 2026-10-12: single-file loaders are special cases). `CAlertSystem` enumerates no directory, so it stays in the `UnnamedRegistries` gap, and `registry_fields` has no owner to walk. | A fixture writes files "directly under one bounded relative registry path" and asks about a registry's definitions (`FixtureRequest`), so it cannot name this file. A probe file at `common/alerts.txt` would replace the game's file, not join it ([mod over vanilla](#mod-over-vanilla)), so every vanilla alert would be gone in the probe run. |
| `alert_icon` | The same function; root key `icon` (`0xb5`, branch at `0x1000693cc`) reads one entry per priority, whose key the loader lowers and compares as text (`CString::ToLower`, `0x100069510`) | As for `alert`; the entries are text keys, which no registry method names. | As for `alert`. |
| `gamesetup_setting_category` | `CGameSettingsDatabase::Init()` builds a `CString` from `common/gamesetup_settings/gamesetup_settings.txt` (`0x101624370`), lexes it with `CTextLexer` and reads it with `CReader::CReader(CLexer&)`; its members are read through `CGameSettingsTabData::ReadMember` and `CGameSettingsCategoryData::ReadMember` | The scheduled `CGameSettingsDatabase` makes no enumeration call, so it stays in the `UnnamedRegistries` gap; no registry, no owner. | As for `alert`: a fixture cannot name the file, and a probe file at that path would replace the game's file. |
| `achievement` | `CGameApplication::InitGame()` tests `VFSExists("common/achievements.txt")` (`0x1005e8ac4`), lexes the file and calls `CAchievementsManager::InitFromFile(CReader*, bool)` (`0x1005e8b48`), then `CApplication::CalculateGameFilesChecksum()` | No scheduled database and no enumeration loads it, so it is outside the `registries()` search. | As for `alert`; the file also enters the game-files checksum after the read, so a probe changes the checksum of the run. |

Reopen when one of these holds: a new build moves a loader to a directory or a scheduled
database that enumerates one (the registry search then lists it); Atlas decides that a single
named file is a registry identity, and a method names registries by file (`path_file`) and
reads the loader's root switch; or the fixture route gains a request for a named file outside a
registry directory that merges with the game's file.

**Directories the engine never loads (M452, reviewed reason 2026-10-21).** Four config types
name a directory that no M452 loader reads. They are stale CWT config, and Native gives no answer
for them (no new answer kind):

| Type | Directory | Static evidence (M452) |
| --- | --- | --- |
| `sector_focus` | `common/sector_focuses` (the game ships `00_focus.txt`) | Only `RunGame` forms the literal's address (`0x10091bb8c`), to pass it to `VFSPreEnumerateFiles` (`0x10091bb9c`), which warms the file list. No function calls `VFSGetEnumeratedFiles` or a lexer with it, and no class or symbol has `SectorFocus` in its name. |
| `mandate` | `common/mandates` (not shipped) | Only `RunGame` forms it (`0x10091b664`), for `VFSPreEnumerateFiles` (`0x10091b674`); no class has `Mandate` in its name. |
| `ai_force_species` | `common/ai_force_species` (not shipped) | Only `RunGame` forms it (`0x10091bd0c`), for `VFSPreEnumerateFiles` (`0x10091bd1c`); no class reads it. |
| `trade_conversion` | `common/trade_conversions` (not shipped) | No string names the directory. `trade_conversions` exists only as a token, with the effect `set_trade_conversions` (`CSetTradeConversions::Read`), which a script uses, not a file. |

The evidence is the string table and the functions that form each address (`inspect --strings`),
and the symbol names; a pointer slot that holds a literal's address, as `_PING_MAP_DIR` does,
would not show there, and no named global holds these. Reopen when a build loads the directory:
an enumeration or a lexer call that receives it.

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

## Definition text after the read

`keeps_definition_text` says whether the loader or the item keeps a definition's script text after
the read, so that later engine code could read a key that the item's reader does not handle.
Atlas may answer "not a field" for a config key only where it is `Known(false)` and
`registry_fields` is `Complete`.

Facts from `c621723d…` (M452):

- The engine has no kept script tree. `CReader` reads tokens from a `CLexer` once, in order.
- `TSingleObjectGameDatabase<…>::LoadFromReader` (`0x100a46bec` for policy categories) reads each
  statement's key (`CLexer::Tok`, then `CString(char const*)` on the stack, freed after the
  statement) and passes the reader to `ReadNewEntry` (`0x100a4710c`) or to `ReadExistingEntry`.
  Those construct or reset the item and call its read: `CPersistent::Read(CReader&)`
  (`0x1025b43bc`) directly or through slot `0x20`. The direct calls of all 164 `LoadFromReader`
  bodies are only string and compare helpers, `CLexer::Tok`, `_calloc`, the item constructor and
  the array or hash insertion, `CPersistent::Read`, `ReadSimpleStatement`,
  `ParseAdvancedStatement` (an `@variable`), `CreateInlineScriptReader` with a recursive
  `LoadFromReader`, `ReportMalformed` and `CPdxIntegerFlags::CreateFlagIndex`. None copies text.
  The method assumes this for every template database and does not read the bodies.
- `CPersistent::Read` calls `ReadWithoutInitPostRead`, which gives each statement to the item's
  `ReadMember`, then the two post-read hooks. `CPersistent::ReadMember` (`0x1025b4438`) tail-calls
  `CReader::ReportUnexpected()`, which reports and skips the statement.
- Text outlives a read only where a reader copies tokens into a string: `CMetaScriptTemplate::Read`
  (`0x10094b018`, scripted effects, scripted triggers and script values; `BuildFromSource` reads
  the copy later with a new lexer) and `CPostInitModifier::CPostInitModifier(CReader&, CModifier*,
  bool)` (a modifier block's text, read again by `ProcessModifier`). Inline scripts are expanded
  during the read.

**Rule.** `keeps_definition_text` is `Known(false)` when every template database of the registry
has an item whose persistent address points all hold `CPersistent::Read` in the read slot (`0x20`).
A persistent point is one whose slots up to the last post-read hook (`0x38`) all hold pointers;
the primary group of an item with a `CPersistent` base at `+0x38` ends before `0x20` and is not one.
Any persistent point whose read slot names another function, no function or several gives
`Unresolved` (`loader_rules::item_read`), as does an item with its own `Read` (`CMetaScriptTemplate`,
`CZoneType::Read`, `CAstralRiftType::Read`, …), an item with no persistent point and a custom loader.

**Build-wide part (gap `later_text_reads`).** Code other than the loader could reread a file. On
M452, 107 functions other than the template `LoadFile` bodies call a lexer constructor directly
(`CTextLexer`, `CBinLexer`): saves and hotjoin, GUI and assets, settings, console commands
(`OnExecute_EffectFile`, …), the custom loaders' `Init` and `ReadFile` functions, the three
`BuildFromSource` and `ProcessModifier`. The list was reviewed by name, not body by body: no name is
a template database's function or a post-read hook, and a call through a register is not in the
list. A probe can confirm it (below). Review the list again for a new build.

Result on M452 (`loader-rules/v9`): 149 `Known(false)`, including all 19 registries whose
`registry_fields` is `Complete`; 15 `Unresolved` because an item has its own read or no read slot (`common/job_tags`, `common/map_modes`,
`common/trait_tags`, `common/scripted_effects`, `common/scripted_triggers`, `common/script_values`,
`common/zones`, `common/zone_slots`, `common/astral_rifts`, `common/starbase_buildings`,
`common/starbase_levels`, `common/starbase_modules`, `common/ai_espionage/operations`,
`common/ai_espionage/targets`, `common/component_slot_templates`; `common/map_modes` because its
primary group has the persistent layout with another function in the read slot); 30 `Unresolved` custom loaders.

Remove or limit the rule when a template `LoadFromReader` calls anything else, when a post-read
hook builds a reader, or when a lexer constructor gets a new caller.

Fixture probe that would check the build-wide part: in a copy of one definition of each of the 19
registries, add an unlisted root key with a block value; load to the main menu and into a game;
expect exactly one `ReportUnexpected` line for each key at load, and then change the file on disk
and run the game's reload commands, expecting no second report.

## File order

Facts from `c621723d…` (M452). `VFSEnumerateFiles` (`0x102547350`) runs
`EnumerateFilesRecursive`, then `std::__stable_sort` with `CPdxArray<CString, int>::SLessThan`,
which calls `CString::operator<` (`0x102526e28`: `memcmp` over the shorter length, then the
shorter name first), then `std::__unique` with `equal_to`. So the loader receives each name once,
in byte order of its path below the folder: case-sensitive, so `B.txt` before `a.txt`, and
`_` (`0x5f`) after `/` (`0x2f`) and after upper-case letters. The template `Init` loads the names in
array order (index from 0, step `0x28`). A pre-enumerated result has the same arguments and comes
from the same function.

Limits: `loader_rule_file_order` checks the order for a template loader
([fixture controls](#fixture-controls)). A custom loader's own loop over the names is not read.

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

Limits: no fixture verifies these rules. A fixture has no second mount (decision D3; see
[second mount](early-observations.md#second-mount-decision-d3)). The order
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
    SDK-844: `CNameListDatabase::Init` (`0x100996924`) copies the root key into the list's `+0x1c0`
    and hashes it for `GetNameList`; `CGfxCultureDatabase::Init` (`0x100846770`) builds
    `CGfxCulture(CString const&, int)` from the root key into the map that `GetCulture` searches;
    `CGameRules::InitFromFile` (`0x1006c74e8`) compares each root key with its fixed rules and
    reports any other key. `CSystemInitializerDataBase::InitFromFile` (`0x100aac8d8`) hashes the
    root key, and for a new key builds `CSystemInitializer(CString const&)` from it
    (`0x100aaca40`), which `FindInitializer` searches; the root key `random_list` (`0x2714`) goes
    to `ReadRandomList` instead, so the rule has that obstacle and states the root-key form for the
    other statements.
  - One root key, named by a field: `CShipBehaviorDatabase` takes only `ship_behavior`
    (`0x311c`); `CShipBehaviorReader::CreateAndRead` (`0x1011b79b4`) reads `name` into `+8`, which
    `AddBehavior` hashes and compares. `CSpecialProjectDatabase::Init` (`0x100b9ddec`) takes only
    `special_project` (`0x2d8b`); `CSpecialProjectType::ReadMember` reads `key` (`0xdc`) into
    `+8`, which the second pass of `Init` compares. `CWorldGfxDatabase::ReadMember`
    (`0x101bc252c`) takes only `gfx_settings` (`0x2e6f`); `CWorldGraphicalMapData::ReadMember`
    reads `world` (`0x3fe8`) into `+8` and its hash into `+0x30`, which `GetWorldGfxIndex`
    compares. `CShipDesignTemplatesDatabase::Init(bool)` (`0x1011d0438`) takes only
    `ship_section_template` in `common/section_templates`, only `component_set` in
    `common/component_sets`, and `weapon_component_template`, `utility_component_template` and
    `strike_craft_component_template` in `common/component_templates`; each item's reader hashes
    `key` (`0xdc`) into the member that the directory's lookup compares
    ([one loader, three directories](registry-fields.md#one-loader-three-directories-sdk-843)).
    Other root keys are skipped, or reported for `gfx_settings` and the component templates.
    `CSongDatabase::InitFromFile` (`0x1009890dc`) takes only `song` (`0x2dd4`) and builds a
    `CSong`, named by its `name` field. `CRandomNameDatabase::ReadRandomNames()` (`0x100ac0dec`)
    names each `*_name_parts_list` statement by its `key` field, which `GetPartFromList` compares;
    a `*_name_format` statement has no name. `CMessageSystem::ReadMessageTypes`
    (`0x100940d64`) takes only `message_type` (`0x2d99`), reads a stack `CMessageType` through
    `CPersistent::Read` and inserts it into its map by the member at `+8`, which
    `CMessageType::ReadMember` reads from `key` (`0xdc`). `CScriptableLocalizationDatabase::InitFromFile`
    (`0x100b0dec4`) builds a `CScriptableLocalization` for every root statement, whatever its key,
    and maps it by `+0x210`, which `CScriptableLocalization::ReadMember` reads from `name` (`0x1b`):
    `Root` / `Field("name")`.
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
    apart. `CRandomNameDatabase::FillDatabase()` (`0x100ac03e0`, `common/random_names/base`) reads
    each fixed root key's names into a list; under `custom_star_names` and `custom_planet_names`
    each list is keyed by its nested key (`treasure_star_names`).
    `CProjectileGfxDatabase::ReadFile` (`0x1018f5200`) reads each file of `gfx/projectiles` as the
    database object; its `ReadMember` (`0x1018f5524`) builds a definition for each of three root
    keys, and no lookup by name was read.
  - Repeated keys, seen while reading these loaders and not in the answer: `COnActionDatabase`,
    `CPlanetClassDatabase` and `CEmpireDesignDatabase` merge a later definition into the earlier
    one; `CFallenEmpiresDatabase` replaces it; `COpinionModifierDatabase` adds a second item that
    its hash table finds first; `CShipBehaviorDatabase` and `CWorldGfxDatabase` log and keep the
    first; `CDesignerDatabase` logs and keeps both.

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
- `common/country_container`: the config's `skip_root_key = collection` skips the root key. The
  template loader names its item `collection` by the root key, and the item's reader
  (`CMiscEconomicUnitCollection::ReadMember`, `0x101c418cc`) takes the text of each key in its
  block (reader `+0x48`), constructs `CMiscEconomicUnit(CString const&)` with it, inserts the unit
  and reads it through its vtable: an economic unit named by that key. The answer keeps the root-key form
  and an obstacle gap for the units, which no form states (a key at any name inside the block).
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

## Fixture controls

Each control is a `tests/live.rs` case (`tests/live/loader_rules.rs`). It reads the static
answer, writes the fixture in the stated form and predicts from the stated rule what the loader
does with it, and compares the fixture observation with the prediction. A duplicate control
defines one key twice: the first definition sets two string fields, the second only one. The prediction comes from the rule, not from the registry. The worker reads each
definition's key from the item constructor, so a construction at a root statement's line shows
that the loader named it by that statement's key.

| Rule | Case | Registry | Prediction |
| --- | --- | --- | --- |
| `LastWins`, a key repeated in one file | `loader_rule_last_wins` | `common/traditions` | Two constructions at one owner; the field that only the first sets is empty; the second value of the other; one `Object with key: … already exists` at the second definition |
| `Merges`, a key repeated in one file | `loader_rule_merges` | `common/governments/authorities` | One construction; the first value of the field that only the first sets; the second value of the other; no such diagnostic |
| File order, and `LastWins` across two files | `loader_rule_file_order` | `common/traditions` | `native_rule_B.txt` loads before `native_rule_a.txt` (byte order; an order that ignores case would differ); then as `LastWins` |
| `Root` / `Key` | each case above | both | Each construction of the key is at a root statement with that key |
| `RootKey(k)` / `Field(f)` | `loader_rule_field_name` | `common/special_projects` | A definition written as `k = { … f = name }` is observed under `name`; the inline binding reads the name from the item |

Each case also reads back the recorded static answer and fixture observation apart from `Basis`,
and the harness checks that the ordinary profile is unchanged.

Rules that no fixture reaches, with their limits:

- `extension`, `name_contains`, `recursion`: a fixture writes only `.txt` files directly in the
  folder (`FixtureRequest`), so it cannot show which other names or subdirectories a loader reads.
- Mod over vanilla: a fixture has no second mount (decision D3).
- A custom loader's duplicate rule (`Unresolved`), and the `namespace` / `Value` and `Root` /
  `Field("id")` forms of `events`: a fixture binds template loaders and one inline loader
  (`common/special_projects`), not the events loader.
- `RootKey(k)` / `Field(f)` of the other custom loaders (`common/ship_behaviors`, `gfx/worldgfx`):
  the same form as `common/special_projects`, but no fixture binding of their own.
- Swap blocks (`Nested` / `Field("name")` at `tradition_swap`, `technology_swap` and
  `advanced_authority_swap`): a fixture question names a root definition, not a swap.
- The other `Merges` registries: `common/economic_plans` and `common/portrait_sets` have no
  string field with a storage binding; `common/country_limits/ownership_limits` and
  `common/portrait_categories` have no fixture loader. The rule is the same template handler.

## Result on M452

All 185 listed registries have every file-selection property `Known`; none is unresolved.

| Selection | Registries |
| --- | ---: |
| Every file, folder only | 174 |
| `.txt`, folder only | 8 |
| `.txt`, every level | 2 (`common/global_ship_designs`, `common/terraform`) |
| Every file, every level | 1 (`flags`) |

Definition names: 173 `Known`, 3 `Partial` and 9 `Unresolved`; each `Partial` or `Unresolved`
answer has an obstacle gap.

| Forms | Registries |
| --- | ---: |
| Root key | 162 (157 template, 5 custom) |
| Root key, and swaps by `name` | 4 (`common/traditions`, `common/ascension_perks`, `common/technology`, `common/governments/authorities`) |
| One root key, by a field | 3 (`common/ship_behaviors` by `name`, `common/special_projects` by `key`, `gfx/worldgfx` by `world`) |
| `namespace` by value, and other root keys by `id` | 1 (`events`) |
| No named definition (`Known`, no form) | 3 (`common/colors`, `common/start_screen_messages`, `common/terraform`) |
| Partial: root key; the swap has an obstacle | 2 (`common/governments/civics`, `common/pop_jobs`) |
| Partial: root key; the units in its block have an obstacle | 1 (`common/country_container`) |
| Unresolved: an obstacle | 9 (`common/ambient_objects`, `common/component_tags`, `common/diplo_phrases`, `common/global_ship_designs`, `common/planet_classes`, `common/species_names`, `common/static_modifiers`, `flags`, `prescripted_countries`) |

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
`common/traditions`, `events` and `common/diplomacy_economy`, and
`loader_rules_state_a_rule_or_an_obstacle_for_every_custom_loader` checks that no registry is
left with no stated rule. The parity file keeps every `definition_names` gap with its kind.
