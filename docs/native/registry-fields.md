# Registry fields

These notes are on field discovery, member readers and registry scheduling. The [discovery
index](discovery.md) lists the other method pages.

## Milestone 4 field baseline (SDK-596)

One run on 2026-09-24 of `registry-fields/v3` over all 164 discovered registries on M45-release
returned **28 complete, 136 partial and 0 failed** answers, with **878 fields**. Field discovery,
reader classification and binding code were unchanged from `3777d2c`; SDK-596 changed reporting
and modifier-answer normalization only. No failure was repaired inside the run. The executable
SHA-256 was `07988b4f1b865623becd7a61af1cae92e111be6515d341754af70f02107822cd`.
The [full baseline](milestone-4-field-baseline.md) records the command, exact source identity,
per-registry and per-reader counts, and retained local probe locations.

| Reader kind | Fields |
| --- | ---: |
| Block | 292 |
| Boolean | 86 |
| FixedPoint | 28 |
| Integer | 62 |
| Reference | 7 |
| String | 171 |
| Unknown | 232 |

Of the 232 unknown kinds, **221 fields have no single established reader identity**. The other
11 have an identified callee but an unknown kind. All 19 established reader IDs in the inventory
were joined to exactly one demangled ARM64 definition symbol. The four unknown signatures are:

| Callee | Fields | Examples |
| --- | ---: | --- |
| `CVariableValue::Read(CReader&, EScopeType)` | 6 | `council_agendas#agenda_cost`; three fields in `country_limits/ship_of_size_limits`; `megastructures#overclock_cooldown`; `species_rights/purge_types#pop_decline_rate` |
| `CReader::Read(CColor&)` | 3 | `governments/authorities#color`, `named_colors#color`, `patrons#color` |
| `CReader::Read(CVector2FixedPoint&)` | 1 | `patrons#position` |
| `CReader::Read(float&)` | 1 | `star_classes#icon_scale` |

The example paths above are below `common/`. The signature join uses the same demangler and
reader-ID derivation as Native; it names an already identified reader, not the semantics it accepts.
The large unknown population therefore cannot be solved by classifying these four signatures alone.
Missing joins and unresolved root paths are the larger part of the remaining work.

Failure shapes in the current public answers, excluding `OutsideMethod`:

| Shape | Gap records | Registries |
| --- | ---: | ---: |
| Reader alternatives do not establish one shared reader | 221 | 80 |
| Shared reader identified, broad value form unknown | 11 | 8 |
| Root reader path could not be followed to its end | 101 | 101 |
| Field reader not established on at least one path | 101 | 101 |
| Required function or name table could not be read | 12 | 12 |
| Reader paths have no recovered field name | 15 | 15 |

The last row accounts for 61 unnamed paths. Rows overlap and cannot be added as independent
failures; the 101 root-path gap records are not a count of all stopped paths. This baseline groups
the public reasons; [the stops behind them](#where-the-baseline-paths-stop-sdk-581) group the same
run by internal stop.

Council agendas has all ten fields, but its answer is partial because `agenda_cost` uses the
unclassified `CVariableValue` reader. Establishing that broad kind alone will not meet the M4
semantic gate: normalized conditions, numeric conversion, block families, scope context and weight
grammar still have their own acceptance criteria. `CPersistent` block classification for `ai_weight`
and `modifier` does not establish their accepted keys or member family.

**Modifier-answer repair (R1):** duplicate registrations now retain unresolved or conflicting
category tags, independent of which known/unknown registration comes first. Regression tests cover
equal tags, conflicts, known then unresolved and unresolved then known. The existing M45 modifier
parity test passed without changes to its expected count, gaps or samples; no returned modifier has
unresolved tags on this build. The explicit M45-release count assertion also passed at 164.

### Where the baseline paths stop (SDK-581)

Date: 2026-09-25, same executable and `registry-fields/v3` population as the baseline above; the
public answers of all 164 registries are unchanged. `examples/registry-field-sweep.rs` groups each
internal gap by the stop instruction's mnemonic, the method's reason and the obstacle. Counts are
internal gaps: one per stopped path, plus one per path without a single named token.

| Stop | Gaps | Functions |
| --- | ---: | ---: |
| Path without a single named token (no stop) | 341 | — |
| `bl` to a reader whose arguments lack owner or reader provenance | 202 | 72 |
| `ubfx` not run | 84 | 15 |
| `b` (tail call) to a reader without that provenance | 80 | 37 |
| `b.hi` on unknown flags | 47 | 33 |
| `blr` not run | 27 | 16 |
| `b.lt`, `b.ne`, `b.eq`, `b.ls`, `b.gt`, `b.le` on unknown flags | 36 | — |
| `and`, `ccmp`, `stur`, `cmp`, `lsr`, `ret`, `cset`, `csel`, `ldrh`, `movi`, `movk` not run | 63 | — |
| `ldrb` or `stp` with an addressing form that the walker does not read | 23 | 13 |
| Root `ReadMember` missing or ambiguous (no stop) | 12 | — |

The unknown-flags stops on `b.hi` are the bounded unsigned compare in front of a compiler jump
table, the shape SDK-563 repairs. No path reached a table load, because each stopped at the
guard. The `ldrb` rows are bit-field loads with a constant register index, not table loads
([SDK-563](#jump-tables-and-bit-fields-in-the-root-reader-sdk-563) corrects this). Rerun the
sweep and use `--diff` against a report of this run to count the registries a repair changes.

### Jump tables and bit fields in the root reader (SDK-563)

Date: 2026-09-25, M45-release (executable SHA-256 above). `CMegaStructureType::ReadMember` at
`0x101122c70` dispatches two token ranges through halfword jump tables:

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
which tail-calls `CPersistent::ReadMember`, the verified base rejection. The method follows every
token in the guarded range to its own case, so a default case is rejected like any other unexpected
token. The `b.hi` side is split by unsigned intervals of `token - base`. It holds further direct
comparisons, which gave two more fields: `dismantle_cost` and `ai_weight`.

Every token has a name, so a default slot that reaches a call other than the rejection, such as an
inherited reader, would give false fields. The ticket's rule, "the most frequent target is the
default", fails on `CMissionType::ReadMember`. Its table for tokens 11653–11656 has no default slot,
and `on_fail` and `on_cancel` share one case that reads the same effect member (`+0x430`). That case
ties for the most frequent target. The method uses a different rule. The switch's default block also
serves the wide token intervals that no case handles. So a table case whose address a wide interval
also reaches is the default, and it may only reject. When a wide interval that leaves through
the table's guard does not end at the rejection, the default may be past its end. A case is then kept only when it joins a known reader,
which a default does not do. Frequency is not used: a default can hold one slot, and an alias can
hold as many as the default. In `CMissionType` all wide intervals end at the rejection, so the
alias gives both fields.

28 of the ticket's 32 table fields reached a reader call once the table was decoded. The other four
(`tooltip_show_star_resources`, `place_entity_on_planet_plane`, `use_planet_resource` and
`can_prevent_crisis_terraformation`) are boolean bit fields. The reader copies the bit to a stack
temporary with `ldr` and `ubfx`, or with `ldrb w8,[x19,x8]` where x8 is a constant, then calls
`CReader::Read(bool&)`. The walker now reads a register-offset load with a constant index, and it
forgets the result of `ubfx` and `and`. So these paths reach the call and name their fields. The
reader join stays missing because the destination is a temporary, not the member. Direct
comparisons reach the same shape, for example `is_ruined_orbital_ring` and `hide_name`.

**M45-observe** (4.5 beta, no longer catalogued; ARM64 slice at
`.local/executables/stellaris-m45-observe-arm64`): the same reader used a jump table for tokens
18066–18115, which held `overclock_loc_key`, `overclock_cooldown`, `dismantle_possible`,
`dismantle_potential` and `should_ai_dismantle`. The release compiler used direct comparisons for
these five, so before this repair Native found them on the release build only. The compiler
chooses between a table and comparisons on each build, so the method must read both.

**Registry population** (`registry-fields/v4`, all 164 registries, `--diff` against the SDK-581
report of `registry-fields/v3`; the stamp changes on every registry, so the counts below are
registries whose field lists changed):

| Run | Registries changed | Fields added | Fields removed |
| --- | ---: | ---: | ---: |
| Jump tables only | 32 | 300 | 0 |
| Jump tables and bit-field reads | 38 | 469 | 0 |

The totals stay at 28 complete, 136 partial and 0 failed. Fields go from 878 to 1,347. No
registry lost a field or changed completeness, and no jump table in the population gave a
`JumpTable` gap. So each table's default slots reach the verified rejection, or the table has no
default slot. Queries with an unresolved root path go from 101 to 85. The sweep time was the same, about 65 s.

| Reader kind | Before | After |
| --- | ---: | ---: |
| Block | 292 | 379 |
| Boolean | 86 | 129 |
| FixedPoint | 28 | 51 |
| Integer | 62 | 82 |
| Reference | 7 | 8 |
| String | 171 | 206 |
| Unknown | 232 | 492 |

The unknown count grows most, because each bit field now appears with an unestablished reader.
Of the 492, 477 fields have no single reader identity. Joining a bit-field temporary to its member
is a separate repair. The `ubfx` stops (84) and the `b.hi` table guards (47) are gone from the stop
table. Two `b.hi` stops remain in `CEspionageOperationType` and `CStarClass`. Both compare a
value loaded from the object, not the token, so they correctly stay unknown flags.

## Members and shared readers

SDK-487's earlier engine-only method discovers token-dispatch paths without supplied field/config seeds and transfers to AI attitudes. It partitions paths, recovers concrete token readers and records unresolved helpers/member paths. Omission and unsupported-root/branch controls retain unknown obligations rather than removing prior members. Static symbols and demangled classes are discovery inputs; unresolved fields cannot receive config-filled answers.

The later council-agenda reconstruction recovers ten root fields with reader bindings, exact replay and 21 live cases. Five shared-reader contracts still prevent complete reconstruction: scoped integers, triggers, effects, graphical modifiers and AI weight. The consumer refuses complete schema generation, including a forged completeness claim. The installed corpus is comparison evidence after extraction, not a seed. This accepted experiment retains a **failed completeness result**. The five contract labels (`scoped-integer-value`, `trigger-clause`, `effect-clause`, `graphical-modifier`, `ai-weight`) were attached to council-agenda field results only, through a branch on the owner class. SDK-569 removed that branch and the labels from the code. The open contracts continue in SDK-541 to SDK-545 and SDK-549.

SDK-492 binds factories to inherited readers and finds six create-starbase and three add-district fields. Forty-five isolated parser cases separate storage/validation, repeated fields and child scopes. The held-out timed-flag method finds four fields but no qualified value readers. SDK-493 then demonstrates one shared numeric operand grammar reused for agenda cost and timed-flag time units, transferring to add-trust amount with fixed-point type; cooldown is a plain-integer contrast. Its final 62 cases, 41 empty-scope evaluations and 20 controls remain bounded: full numeric/world semantics are open in SDK-544 (from the closed prototype SDK-508). These consumer grammars remain Atlas conclusions; Native retains their instruction/reader/ABI mechanisms and qualification evidence.

Evidence: `atlas-discovery/prototype/{engine-registry-discovery,council-agenda-reconstruction}/`; `atlas-command-grammar/prototype/command-grammars/`; `atlas-numeric-grammar/prototype/numeric-grammar/`. Sibling helpers are preserved within each bundle because dispatch, parser replay and live capture import them. Original per-attempt source, raw successes/failures, freezes and capsule manifests remain intact.

## Registry scheduling and owner joins

SDK-489 is explicitly accepted and Done. Source: `atlas-ownership/prototype/registry-ownership/`, SDK branch `prototype/registry-ownership`, commit `efba955e47897cf2b01773ade542ba3289151bd1`. The retained capsule hashes 262 files. Offline replay has 40 experiment controls plus its capsule integrity check.

Symbol enumeration finds 164 exact template `LoadFile` candidates, including owners without a separately named member reader. Static literal initialization recovers 198 scheduling records; both live runs exactly match all names/function-slot addresses. The qualified target layout is **198 × 48-byte rows at x19 + 96**, with a manually checked stop site. Literal address arithmetic, relocated pointer loads and scalar stores are replayed; this is target adaptation, not an inferred general engine layout. The live observer confirms the table before scheduling. Of template candidates, 162 are observed; two remain unobserved. Thirty-five scheduling entries lie outside this template method and can be services. Neither count proves the complete universe of registries.

Owner qualification uses the actual loader receiver/directory, enumeration/file activity, root constructor key, concrete owner, persistent base, vtable offset-to-top and shared member-dispatch slot. A filename appearing somewhere on a stack is insufficient. Development found stale top-frame names; the final qualifier uses the direct caller and receiver directory. The static-modifier custom loader has a key-only phase then full-read phase. A revised frozen qualifier transfers to six economic-plan roots. Its other 243 reader occurrences remain observations, not named definitions.

Normal/reversed two-mod runs demonstrate shared virtual-filename mount selection. Separate duplicate files process in observed a/b order. Category duplicates reconstruct the same object while preserving its numeric ID; modifier duplicates reuse the owner, without measuring final merged/reset values. The category loader reads a `.bin` fixture. These do not establish a universal extension policy, complete physical-file resolution or general “last value wins” rule.

Retained failures include excessive observer overhead, observer-startup failures and an engine exception before the end marker with incomplete supporting category fixtures. The exception was not fully diagnosed. The first held-out AI-budget attempt was not a successful frozen transfer; the economic-plan transfer follows the revised freeze. Config comparison occurred only after engine output was frozen.

SDK-551 owns custom/nested/late discovery; SDK-552 owns mounted selection and duplicates (both took over from the closed prototypes SDK-509 and SDK-510). SDK-543 owns identifier grammar. Full field/rule composition, stripped-target discovery, symbol-renaming correspondence and cross-build behavior remain qualified separately. Use stable publication identities with target-local symbols/addresses as evidence locators; the demo's synthetic identity proposal is not a demonstrated cross-build match.
