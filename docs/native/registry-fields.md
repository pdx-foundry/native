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
baseline](milestone-4-field-baseline.md) (878 fields).

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

## Members and shared readers (prototypes)

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
- An engine exception before the end marker, with incomplete category fixtures, was not
  diagnosed.

SDK-551 owns custom, nested and late loaders; SDK-552 owns mounted selection and duplicates;
SDK-543 owns identifier grammar. Symbols and addresses locate evidence on one build only; the
prototype's synthetic identity is not a cross-build match.
