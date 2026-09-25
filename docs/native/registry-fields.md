# Registry fields

`Native::registry_fields(registry)` (`registry-fields/v4`) gives the root fields of a registry
and the reader that each field calls. `Native::registries()` gives the registries. The module
comments of `engine/analysis/fields.rs` and `engine/analysis/discovery.rs` describe the methods.
This page holds the current sweep, the engine facts, the gaps and the prototype findings. The
[discovery index](discovery.md) lists the other method pages.

## Sweep on M45-release

Run on 2026-09-25 at `666f028`, over all 164 registries, in about 67 s:

```sh
cargo run --release --example registry-field-sweep -- "$STELLARIS_PATH" > report.json
cargo run --release --example registry-field-sweep -- --diff before.json report.json
```

Use `--diff` against an earlier report to count the registries that a change affects. The
starting population, before SDK-563, is in the [Milestone 4 field
baseline](milestone-4-field-baseline.md) (878 fields). SDK-563 changed the field lists of 38
registries: 469 fields added and none removed, with no change in completeness.

**28 complete, 136 partial and 0 failed** answers, with **1,347 fields**:

| Reader kind | Fields |
| --- | ---: |
| Block | 379 |
| Boolean | 129 |
| FixedPoint | 51 |
| Integer | 82 |
| Reference | 8 |
| String | 206 |
| Unknown | 492 |

Of the 492 unknown kinds, 477 fields have no single established reader identity. The other 15
call an identified reader of unknown kind. All 19 established reader IDs join exactly one
demangled ARM64 definition symbol (the same demangler and reader-ID derivation as Native):

| Callee | Fields | Examples (below `common/`) |
| --- | ---: | --- |
| `CVariableValue::Read(CReader&, EScopeType)` | 7 | `council_agendas#agenda_cost`; three fields in `country_limits/ship_of_size_limits`; `megastructures#cycle_length_in_days`, `#overclock_cooldown` |
| `CReader::Read(CColor&)` | 3 | `governments/authorities#color`, `named_colors#color`, `patrons#color` |
| `CReader::Read(float&)` | 3 | `star_classes#icon_scale`, two fields in `storm_types` |
| `CReader::Read(CVector2FixedPoint&)` | 2 | `megastructures#entity_offset`, `patrons#position` |

Classifying these four signatures alone will not solve the unknown population. Missing joins
and unresolved root paths are the larger part. Each bit field (below) is also an unknown kind:
it reaches `CReader::Read(bool&)`, but through a temporary.

Public failure shapes, excluding `OutsideMethod`. Rows overlap and cannot be added:

| Shape | Gap records | Registries |
| --- | ---: | ---: |
| Reader alternatives do not establish one shared reader | 477 | 88 |
| Root reader path could not be followed to its end | 85 | 85 |
| Field reader not established on at least one path | 85 | 85 |
| Reader paths have no recovered field name | 16 | 16 |
| Shared reader identified, broad value form unknown | 15 | 9 |
| Required function or name table could not be read | 12 | 12 |

Internal stops, from the same report. A count is one gap for each stopped path, plus one for
each path without a single named token:

| Stop | Gaps | Functions |
| --- | ---: | ---: |
| `bl` to a reader whose arguments lack owner or reader provenance | 428 | 81 |
| Path without a single named token (no stop) | 206 | — |
| `b` (tail call) to a reader without that provenance | 114 | 45 |
| `blr` not run | 30 | 16 |
| `b.lt`, `b.ne`, `b.eq`, `b.hi`, `b.le`, `b.gt`, `b.ls` on unknown flags | 40 | — |
| `ccmp`, `stur`, `lsr`, `movi`, `cmp`, `cset`, `ret`, `ldrh`, `csel`, `movk` not run | 60 | — |
| `stp` with an addressing form that the walker does not read | 11 | 11 |
| Root `ReadMember` missing or ambiguous (no stop) | 12 | — |
| Root function without a reader join (no stop) | 12 | — |

The two `b.hi` stops, in `CEspionageOperationType` and `CStarClass`, compare a value loaded
from the object, not the token, so they correctly stay unknown flags.

**The Milestone 4 gate.** Council agendas has all ten fields, but its answer is partial because
`agenda_cost` uses the unclassified `CVariableValue` reader. Establishing that broad kind alone
does not meet the Milestone 4 semantic gate: normalized conditions, numeric conversion, block
families, scope context and weight grammar have their own acceptance criteria. `CPersistent`
block classification for `ai_weight` and `modifier` does not establish their accepted keys or
member family.

## Compiler jump tables

The compiler chooses between a jump table and direct comparisons on each build, so the method
reads both. On M45-observe (the 4.5 beta), `CMegaStructureType::ReadMember` used a table for
tokens 18066–18115 (`overclock_loc_key`, `overclock_cooldown`, `dismantle_possible`,
`dismantle_potential`, `should_ai_dismantle`); the release compiler used direct comparisons for
these five.

On M45-release, `CMegaStructureType::ReadMember` at `0x101122c70` dispatches two token ranges
through halfword jump tables:

```text
mov  w8,#-base            ; 14112 at 0x101122ca4, 17766 at 0x101122dd4
add  w8,w2,w8             ; the index: token - base, a zero-extended word
cmp  w8,#last             ; 188 and 19
b.hi <out of range>
adrp x9,<table> ; add x9,x9,#off   ; 0x102cd49d8 and 0x102cd4b52, in __TEXT,__const
adr  x10,<entry base>     ; 0x101122ccc and 0x101122dfc
ldrh w11,[x9,x8,lsl#1]
add  x10,x10,x11,lsl#2
br   x10
```

165 of the 189 entries in the first table, and 12 of the 20 in the second, reach `0x101123830`,
which tail-calls `CPersistent::ReadMember`, the verified base rejection. The `b.hi` side is split
by unsigned intervals of `token - base`, and it holds more direct comparisons
(`dismantle_cost`, `ai_weight`).

**The default case.** Every token has a name, so a default slot that reaches a call other than
the rejection, such as an inherited reader, would give false fields. "The most frequent target is
the default" is wrong. `CMissionType::ReadMember` has a table for tokens 11653–11656 with no
default slot, and `on_fail` and `on_cancel` share one case that reads the same effect member
(`+0x430`). That case ties for the most frequent target. The rule that holds: the switch's default
block also serves the wide token intervals that no case handles, so a case whose address a wide
interval also reaches is the default, and it may only reject. When a wide interval that leaves
through the table's guard does not end at the rejection, the default may be past its end, so a
case is kept only when it joins a known reader. No table on M45-release gives a `JumpTable` gap:
each default reaches the rejection, or the table has no default slot.

**Bit fields.** Boolean bit fields such as `tooltip_show_star_resources`,
`place_entity_on_planet_plane`, `use_planet_resource`, `can_prevent_crisis_terraformation`,
`is_ruined_orbital_ring` and `hide_name` copy the bit to a stack temporary with `ldr` and `ubfx`,
or with `ldrb w8,[x19,x8]` where `x8` is a constant, then call `CReader::Read(bool&)`. The walker
reads a register-offset load with a constant index and forgets the result of `ubfx` and `and`, so
these paths reach the call and name their fields. The reader join stays missing, because the
destination is a temporary, not the member. Joining the temporary to its member is a separate
repair.

## Members and shared readers

These findings are from the prototypes.

- **SDK-487.** Token-dispatch paths are discovered without field or config seeds, and the method
  transfers to AI attitudes. It partitions paths, recovers concrete token readers and records
  unresolved helpers and member paths. Omission and unsupported-root or branch controls keep
  unknown obligations instead of removing earlier members.
- **Council agendas.** The reconstruction recovers ten root fields with reader bindings, exact
  replay and 21 live cases. Five shared-reader contracts still prevent a complete answer:
  `scoped-integer-value`, `trigger-clause`, `effect-clause`, `graphical-modifier` and
  `ai-weight`. SDK-569 removed those labels, and the owner-class branch that attached them, from
  the code. The open contracts continue in SDK-541 to SDK-545 and SDK-549.
- **SDK-492.** Factories bind to inherited readers: six create-starbase and three add-district
  fields. Forty-five isolated parser cases separate storage from validation, repeated fields and
  child scopes. The timed-flag method finds four fields but no qualified value readers.
- **SDK-493.** One shared numeric operand grammar serves agenda cost and timed-flag time units,
  and transfers to the add-trust amount with a fixed-point type; cooldown is a plain integer.
  Full numeric semantics are open in SDK-544. The consumer grammars are Atlas conclusions; Native
  keeps the instruction, reader and ABI mechanisms.

Evidence: `atlas-discovery/prototype/{engine-registry-discovery,council-agenda-reconstruction}/`,
`atlas-command-grammar/prototype/command-grammars/` and
`atlas-numeric-grammar/prototype/numeric-grammar/` (see [retrieval](retrieval.md)).

## Registry scheduling and owner joins

From the SDK-489 prototype on M45-observe (`atlas-ownership/prototype/registry-ownership/`,
commit `efba955e47897cf2b01773ade542ba3289151bd1`):

- Symbol enumeration finds 164 exact template `LoadFile` candidates, including owners without a
  separately named member reader. 162 are observed live; two are not.
- Static literal initialization gives 198 scheduling records. Two live runs match all names and
  function-slot addresses. The layout is **198 rows of 48 bytes at `x19 + 96`**, with a stop site
  checked by hand. The live observer confirms the table before scheduling. 35 scheduling entries
  are outside the template method and can be services. Neither count proves that all registries
  were found.
- An owner is established by the loader's receiver and directory, file activity, the root
  constructor key, the concrete owner, the persistent base, the vtable offset-to-top and the
  shared member-dispatch slot. A filename somewhere on a stack is not sufficient: top-frame names
  can be stale, so use the direct caller and the receiver directory.
- The static-modifier custom loader has a key-only phase and then a full-read phase. The owner
  rule transfers to six economic-plan roots; the other 243 reader occurrences are observations,
  not named definitions.
- With two mods in normal and reversed order, the game selects one shared virtual filename.
  Separate duplicate files are processed in a/b order. A category duplicate reconstructs the same
  object and keeps its numeric ID; a modifier duplicate reuses the owner. On M45-observe the
  category loader reads a `.bin` fixture. The final merged values were not measured, and these
  runs do not establish a general extension policy, physical-file resolution or "last value
  wins".
- Failed approaches: the observer's overhead was too high in one form, and the observer failed
  to start in others. The first held-out transfer to the AI budget failed; the owner rule was
  revised before the economic-plan transfer. An engine exception before the end marker, with
  incomplete category fixtures, was not diagnosed.

### Scheduler table on M45-release

Native had a static scheduler method. No supported operation used it, and SDK-602 removed it. The
code is at `git show 8d9a073:src/engine/analysis/discovery/scheduler.rs`, with its join in
`discovery.rs` and its tests in `tests_discovery.rs` at the same commit. The facts below are for
M45-release: executable SHA-256 `07988b4f1b865623becd7a61af1cae92e111be6515d341754af70f02107822cd`,
ARM64 slice `a4cb49ad17a84ef6bf438019a50d3a66362c80731f8359888ddbce47c0d0aab9`.

- **Where.** The table is filled with literal values in
  `NNullObjAndDatabaseInitUtil::SetupDatabases(CPdxArray<SDatabaseObjectFunctions, int>&, ...)`.
  The fill runs from the function entry `0x1005eb938` to `0x1005eedf0` (exclusive): 13,496 bytes,
  3,374 instructions. Scheduling begins at the end address, which was found by hand. After
  `mov x19, sp`, the table is at `x19 + 96`: **198 rows of 48 bytes**.
- **Row.** Six 8-byte slots: the name as a C-string address, then
  `TGameDatabase<T>::CreateInstance()`, `DestroyInstance()`, zero (`stp x8, xzr`),
  `InitInstance()` and `PostReadInitInstance()`. The code loads each function address from a
  `__DATA_CONST,__got` slot. Without chained fixups, no function slot resolves. Row 0 is
  `CNamedColorDatabase`: name `0x102df6f2f`, `CreateInstance` `0x10064c050`, stores at
  `0x1005eb998`, `0x1005eb9a4` and `0x1005eb9b8`.
- **Method.** Track `adrp`, `add #imm` and `ldr` through fixed-up pointers. A `str` or `stp` to
  `[x19, #offset]` fills slots. A `mov` carries no value. A `bl` or `blr` clears the slots and the
  volatile registers and records an unknown call. A `str`, `stp`, `stur` or `sub` that uses `sp`
  or `x29` is stack or frame work and is skipped. Any other instruction clears every register
  and records an unsupported instruction. A new value in `x19` invalidates the table owner. A row
  is recovered when all six slots and the name string are known; a stored `xzr` is a known zero.
- **Join.** A row names a candidate when a symbol at one of its function-slot addresses contains
  the candidate's database type (a `C…Database` or `C…Manager` word).
- **Result.** 198 of 198 rows recovered, with no row gaps. One unknown call: `blr x16` at
  `0x1005eb960`, the `___chkstk_darwin` stack probe before `mov x19, sp`. 163 rows join exactly
  one candidate, and no row joins two. 163 of the 164 candidates join a row;
  `CGameScenarioDatabase` joins none. 35 rows are outside the template method: 2
  `CStaticModifierDatabase`, 7 `CWeaponTagDatabase`, 10 `CStrategicResourceDatabase`,
  26 `CShipBehaviorDatabase`, 27 `CAmbientObjectDatabase`, 28 `CEmpireFlagDatabase`,
  29 `CGfxCultureDatabase`, 31 `CProjectileGfxDatabase`, 32 `CPortraitDatabase`,
  36 `COpinionModifierDatabase`, 54 `CPlanetClassDatabase`, 69 `CAdvisorDatabase`,
  73 `CPingMapDatabase`, 79 `CFallenEmpiresDatabase`, 84 `CShipDesignTemplatesDatabase`,
  85 `CSpeciesNamesDatabase`, 86 `CNameListDatabase`, 89 `CDesignerDatabase`, 95 `CEventManager`,
  97 `CSpecialProjectDatabase`, 99 `COnActionDatabase`, 101 `CTraitDatabase`,
  104 `CPrescriptedSpeciesDatabase`, 105 `CDiploPhraseDatabase`, 106 `CMessageSystem`,
  107 `CAlertSystem`, 108 `CTerraformDatabase`, 109 `CStartScreenMessageDatabase`,
  110 `CSystemInitializerDataBase`, 112 `CGalaxyTemplateDatabase`, 113 `CWorldGfxDatabase`,
  115 `CColorsDatabase`, 119 `CGameSettingsDatabase`, 123 `CEmpireDesignDatabase` and
  128 `CScriptableLocalizationDatabase`.
- **Authored cases.** These were the removed tests' inputs and outcomes. A value-clearing
  instruction (`mov w8, #0`) before the name store leaves the name slot unknown. An unknown call
  in the same place does the same and records an unknown-call gap. `add x19, x19, #8` makes a new
  table owner and gives a row gap. With no symbols, the row stays and is outside the template.
- **Pitfalls.** Without `mov x19, sp`, every row is a gap: never treat that as an empty table.
  An empty range still gives every row, as gaps. The method rejected a stride other than 48, a
  range over 64 KiB and an unaligned table offset. The row count is not a registry count.

### Owner vtables on M45-release

The owner rule above needs the persistent base's offset-to-top and the shared member-dispatch
slot. SDK-602 removed an image-wide scan of these, which no live method read. It is at
`git show 8d9a073:src/binding/binary/discovery.rs` (`vtables`). For new owner joins, use
`vtable_group` in `binding/binary/families.rs`. It reads one class and checks its typeinfo.

- **Scan.** From each `vtable for <class>` symbol up to the next symbol (at most 64 KiB), in
  8-byte steps, the scan took a position as a vtable when two things held: its word was an
  offset-to-top in `-4096..=0`, and the next word was a fixed-up pointer. The address point is
  the position + 16. The member slot is the fixed-up pointer at the position + 56: the address
  point + 40, which is slot 5.
- **Result.** 15,371 address points in 12,876 classes; 2,593 have a non-zero offset-to-top. For
  159 of the 164 candidate owners, the member slot of some vtable holds `ReadMember(CReader&,
  int)`: through a `virtual override thunk` for 149, directly for 10. That base's offset-to-top is
  -56 for 155 owners, -112 for 2, -64 for 1 and 0 for 1. There is no such vtable for
  `CComponentSlotTemplate`, `CJobTag` and `CTraitTag`, which have no named reader, or for
  `CStarbaseBuilding` and `CStarbaseModule`, which have one.
- **Example.** `CTraditionCategory`: address point `0x103095310`, offset-to-top -56, member slot
  `0x100cd92b0`, the thunk to `CTraditionCategory::ReadMember(CReader&, int)`.
  `CMegaStructureType`: `0x1030b0860` (0, `InitPostRead`), `0x1030b08a8` (-56, the `ReadMember`
  thunk).
- **Pitfalls.** The scan did not check that the second word is the class's own typeinfo, so it
  accepted false address points. For example, `CMegaStructureType` got `0x1030b08f8` (-64,
  `~CMegaStructureTypeDatabase()`), and `CCouncilAgenda` got `0x10301b9d8`, whose member slot
  holds `typeinfo for CCouncilAgenda`. Slot 5 holds the reader only in the persistent base;
  other bases hold other functions there.
- **Imports.** The chained fixups bind 29,695 slots to other images; 10,994 of them have a name
  (for example `0x102ff4000` `_AcronymTag`). There are 303,281 local pointers. The catalogue keeps
  the bound slots, and `internals::inspect` names imports from the fixups directly.

SDK-551 owns custom, nested and late loaders; SDK-552 owns mounted selection and duplicates;
SDK-543 owns identifier grammar. Symbols and addresses locate evidence on one build only; the
prototype's synthetic identity is not a cross-build match.
