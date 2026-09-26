# Registry fields

`Native::registry_fields(registry)` (`registry-fields/v6`) gives root fields, reader and storage shapes,
loader alternatives, nested object fields and local stored-value selections. `Native::registries()` gives the registries. The module
comments of `engine/analysis/fields.rs` and `engine/analysis/discovery.rs` describe the methods.
This page holds the current sweep, the engine facts, the gaps and the prototype findings. The
[discovery index](discovery.md) lists the other method pages.

## SDK-541 sweep on M45-release

The v5 sweep covers all **164 registries: 9 complete, 155 partial, 0 failed**, with
**1,347 root fields and 26 nested fields**. No root field was added or removed from v4.
The exact executable and ARM64 slice are identified in the SDK-541 findings below.
The full run takes about 68 s and 716 MB peak memory on the development host.

| Field fact | Root fields | Nested fields |
| --- | ---: | ---: |
| Scalar value | 476 | 14 |
| Block value | 382 | 10 |
| Unknown value form | 489 | 2 |
| Replaces stored value | 462 | 14 |
| Accumulates entries | 3 | 0 |
| Unknown repeat behavior | 882 | 12 |

There are 20 distinct known root reader identities: 873 fields have one identity and 474 do
not. Of the 489 unknown value kinds, 15 have an identified reader with unknown semantics.
The root reader-kind counts are Block 382, Boolean 129, FixedPoint 51, Integer 82, Reference 8,
String 206 and Unknown 489.

The answers contain 1,335 unconditional root read alternatives and 24 alternatives with
unresolved or composite conditions; a field can have several alternatives. All 26 nested
read alternatives are unconditional. This says nothing about use-time inheritance. There are
10 root and 14 nested local use selections, each retaining its unresolved enclosing context.

The constructed-object method transfers to `tradition_swap` in traditions and ascension perks
(13 child fields each), and to `advanced_authority_swap` in authorities (collection established,
child inventory unresolved). The council presence branches normalize to unconditional integer
reads. The SDK-533 omitted/repeated fixture agrees with `unlocks_agenda` replacing storage:
omission leaves an empty string; two successful occurrences retain the second string. This
fixture does not establish an accepted occurrence limit or a general default rule.

Public failure shapes below exclude `OutsideMethod`, include nested gaps, and overlap:

| Shape | Gap records | Registries |
| --- | ---: | ---: |
| Repeat behavior or nested fields unresolved | 894 | 116 |
| Reader alternatives lack one shared reader | 476 | 88 |
| Root reader path not followed to its end | 86 | 86 |
| Field reader missing on at least one path | 86 | 86 |
| Anonymous or dynamic keys lack a literal field name | 16 | 16 |
| Known reader, unknown value form | 15 | 9 |
| Required function or name table unreadable | 12 | 12 |
| Loader condition unresolved, outcome retained | 12 | 5 |
| Local use test known, enclosing context unresolved | 24 | 2 |
| Bounded use analysis leaves other contexts unresolved | 3 | 3 |

The lower complete count reflects the added storage and condition obligations. It is not a
loss of known field names. Defaults, exhaustive enum domains and accepted occurrence limits
remain explicit `Unknown` and are tracked in [SDK-627](https://linear.app/unnamed-system/issue/SDK-627).
Full selection predicates remain tracked in [SDK-628](https://linear.app/unnamed-system/issue/SDK-628).
SDK-542 owns deeper member grammars; the reader and root-path failures remain in the field
method's gaps and the Milestone 4 coverage gate.

The four tracked parity inventories contain 11 tradition root fields plus 13 swap members,
7 tradition-category fields, 10 council-agenda fields and 82 megastructure fields. They retain
unknown shapes and conditions instead of converting them to successful reads.

## Historical v4 sweep on M45-release

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

### Read conditions and use-time inheritance (SDK-541)

Inspected on M45-release: executable SHA-256
`07988b4f1b865623becd7a61af1cae92e111be6515d341754af70f02107822cd`, ARM64 slice
`a4cb49ad17a84ef6bf438019a50d3a66362c80731f8359888ddbce47c0d0aab9`.

- `CCouncilAgenda::ReadMember` at `0x10020bfa8` tests presence bytes at `+0x6d0`
  and `+0x6d8`. Both sides reach `CReader::Read(int&)`, after clearing the value at
  `+0x6d4` or `+0x6dc`. The branches initialize presence; they do not restrict which
  occurrences are read. The public v4 `conditional` flag does not express this distinction.
- `CTraditionType::ReadMember` at `0x100cdcf38` allocates and reads a swap, then inserts
  its pointer into the collection at `+0x5c8`. The v4 walk stopped at allocation. The v5
  continuation proves construction, virtual read and insertion of that same object on all paths.
- `CTraditionSwap::ReadMember` at `0x100cdc0b4` routes tokens `0x397e`, `0x397f` and
  `0x3980` to Boolean storage at `+0x4f0`, `+0x4f1` and `+0x4f2`. The token constructor
  literals are `inherit_effects`, `inherit_name` and `inherit_icon`. These flags do not
  gate the other fields in that reader.
- `CTraditionType::GetName` at `0x100cdd944` chooses a swap using possibility and weight,
  then checks its validity and byte `+0x4f1`. The zero branch at `0x100cdda28` uses the
  swap's name at `+0xf8`; the other branch calls `GetBaseName`. `GetIconKey` at
  `0x100ce0028` likewise checks `+0x4f2` at `0x100ce0104`, returning the swap name at
  `+0xf8` on zero or the base key at `+0x10` otherwise. These are use-time selections,
  not parser acceptance conditions. Swap validity and selection must remain part of
  the condition, or explicit unresolved context.
- `CTraditionType::CalcAIWeight` at `0x100ce2cdc` calls
  `CMeanTimeToHappen::GetRawFactor` on its own `+0x568` member. This does not establish
  that a swap inherits that weight; SDK-545 owns weight semantics.

`OnEnabled` at `0x100ce25b8` and `OnDisabled` use the same swap selection. In `OnEnabled`,
`ldrb` at `0x100ce26ac` tests `+0x4f0`; `csel` at `0x100ce26b8` chooses swap effect `+0x378`
on zero and base effect `+0x418` otherwise. The same flag selects modifier and tooltip members.
Use analysis admits direct `const` owner methods: their signatures establish the receiver.
Static, nested-class and other unproven receiver shapes remain outside this bounded method
and are retained in SDK-628. This includes `CTraditionType::PostReadInit()`: its selections
need independent receiver proof before inclusion. A branch outside an insertion function leaves
its buffer offset unresolved instead of discarding the branch.
The local flag proof follows copies of the receiver at the flag load; a shared loop load
address alone does not establish object identity. A conditional select must receive its flags
from the adjacent comparison on every incoming path.
The pointer insertion specialization at `0x100ce42ac` stores the incoming pointer in the
collection buffer (`+8`) and increments its count (`+0x14`). The binding derives the buffer
member from that shared specialization; no registry-specific offset enters the method.

The v5 method returns separate loader alternatives and local stored-value selections. Equal
complete reader outcomes on opposite presence branches can collapse to `Always`; unresolved
outcomes and unequal destinations cannot. Primitive tail readers establish replacement;
block calls alone do not. The constructed-object path establishes accumulation and depth-one
members. Reconstruction before or after insertion, owner writes that could reset the collection,
and unknown-pointer writes invalidate that proof. Enum domains and omitted defaults stay explicit `Unknown`, with no occurrence rule
inferred from replacement. Unknown/anonymous key paths retain their named registry gaps.

Use selections carry `All(Unresolved, FieldZero(...))`. The unresolved part covers choice of
swap, validity, other branches and method bounds. Empty selections do not prove no runtime
condition. Local selection analysis does not interpret naming templates, weight arithmetic or
application effects. SDK-627 tracks defaults, enum domains and occurrence acceptance; SDK-628
tracks complete enclosing selection predicates. SDK-542 owns deeper block grammar.
SDK-546 depends on SDK-541 use-time relationships for conditional name and icon templates;
SDK-597 must preserve processing stage and unresolved conditions in Atlas. The SDK-600 gate
still includes SDK-541's inheritance criterion. The dependency audit therefore keeps this work
in Milestone 4 rather than deferring it. Name lookup and miss behavior remain SDK-546's work;
weight evaluation remains SDK-545's, and modifier application observations remain SDK-547's.

Reproduce each inspection with `cargo run --release --example inspect -- --function NAME`,
using the full demangled name when cold clones make a shorter name ambiguous. Use
`--strings inherit_` to locate the inheritance token literals. No CWT rule supplies these facts.

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


## SDK-542 block families

Version 6 adds conservative block families to fields and their conditional read alternatives.
Generic persistent destinations are joined to constructor-installed virtual readers; a shared
`CPersistent::Read` call alone does not give a concrete identity. The full 164-registry sweep,
unknown-family denominator, constructor limits and parser checks are recorded in
[nested command grammar](command-grammar.md#population-measurement).
