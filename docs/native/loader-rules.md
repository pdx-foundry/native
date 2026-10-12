# Loader rules

`Native::loader_rules(registry)` (`loader-rules/v1`) states how a registry's loader selects its
files: the file-name filters and the subdirectories that it reads. The folder is the registry
name. The duplicate-definition rule, the file order and the mod-over-vanilla rule (SDK-790) and the
definition-name rules (SDK-791) are gaps whose subjects are `duplicate_rule`, `file_order` and
`definition_names`. The code is `src/engine/analysis/loader_rules.rs`,
`directories::enumeration_calls`, `src/binding/analysis.rs` (`loader_enumerations`) and
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

## Result on M452

All 185 listed registries have every file-selection property `Known`; none is unresolved.

| Selection | Registries |
| --- | ---: |
| Every file, folder only | 174 |
| `.txt`, folder only | 8 |
| `.txt`, every level | 2 (`common/global_ship_designs`, `common/terraform`) |
| Every file, every level | 1 (`flags`) |

`tests/expected/m452/loader-rules.json` holds each registry's answer; `cargo parity` checks it,
and `loader_rules_state_the_file_selection_of_every_listed_registry` checks that it lists every
registry in `registries()`.
