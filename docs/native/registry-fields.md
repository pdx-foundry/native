# Registry fields

`Native::registry_fields(registry)` gives root fields, reader and storage shapes, loader
alternatives, nested object fields and local stored-value selections; `Native::registries()`
gives the registries. The module comments of `engine/analysis/fields.rs` and
`engine/analysis/discovery.rs` describe the methods. `FieldDefault` was removed (only `Unknown` was
ever established; Git `d8f9d8a`); SDK-627 owns enum domains and required fields, except the
weight keyword domains, which the [weight blocks](weight-blocks.md) method establishes (SDK-725).

## What complete means

Both static answers are bounded searches. `registries` searches the shared database-template
candidates and the scheduled databases outside them. It is complete when every candidate and every
scheduled database has one content directory; nested and late loaders are outside the search.
`registry_fields` searches the root reader paths of one registry's owner: the template argument of a
template loader, or the one owner that a custom loader builds ([custom-loader
owners](#custom-loader-owners)). A listed registry with no one owner is `Unsupported` with the
reason. It is complete when every path, field name and promised reader
classification is resolved. An `OutsideMethod` gap states the boundary and can accompany
`Complete`. Unnamed candidates, unresolved paths, unknown reader classifications and unreadable
required input make the answer partial.

## Current sweep

This sweep ran on M452.

`tests/population/m452/registry-field-sweep/` holds the baseline. Its `header.json` has the
counts (`summary`): registries by completeness, root fields and reader identities. Against `v31`, `events` has its 22 [kinds](#event-kinds) as root fields in place of the
30 `CEvent` fields, and the recording takes in SDK-786: `flags` and `common/planet_classes` are
listed, and `common/ship_sizes.carries_colony` looks up `common/planet_classes`. The 164 template answers did not
change when the 19 custom-loader names joined (SDK-787);
their counts are in [custom-loader owners](#custom-loader-owners). Against `v33`, only the 336
[pre_trigger keys](#pre-trigger-blocks) changed (SDK-807). Against `v34`, the
[uniform lists](#uniform-lists) and the [bit fields](#bit-fields) changed (SDK-710). Against
`v35`, 22 registries changed (SDK-676): the nested owners of traditions, ascension perks and
authorities gained their modifier, clause and weight grammar; 33 root fields that embed an object
of another class gained `accepted_categories: NotApplicable`; and 19 root fields of nine
registries joined their reader once a delegating constructor's stack string no longer stops the
constructor walk ([nested owners and unjoined fields](modifier-blocks.md#nested-owners-sdk-676)). Against `v36`, SDK-813 answered `common/planet_classes`: its 70 planet-class fields and the
reserved key `random_list` ([root-key owners](#root-key-owners-sdk-813)). Against M451-hotfix, civics lost `multiply_by_habitability_effect_modifier` and edicts
gained `relay_network_modifier`. Compare a new run with `registry-field-sweep --diff` ([method
authoring](method-authoring.md#run-over-the-whole-population)).

A reference member is an answer, not a default. Compared with reports from before default-member
normalization (regenerated at `f184f08` and `ab9cc7f`), 20 registries change, not 12: twelve
change readers, shapes or gaps, and eight change only non-default reference lookups
(`common/anomalies`, `common/archaeological_site_types`, `common/astral_rifts`,
`common/council_agendas`, `common/event_chains`, `common/planet_modifiers`, `common/situations` and
`common/starbase_levels`). Dropping every reference member gives 12, but hides those established
lookups, so the diff drops only members that match their serde defaults.

- **Council agendas (SDK-600).** All ten fields are found, but the answer is partial: `agenda_cost`
  uses `CVariableValue::Read`, a scoped operand. `ai_weight` has the shared weight grammar
  ([weight blocks](weight-blocks.md)) and reads in `country`; every key has a reader kind, and
  `calc` and `mode` list their accepted names. It stays partial for `days`, `months`, `years` and one `factor`
  alternative, whose `CToken::GetInt()` and `GetFloat()` readers have established storage but
  no faithful-storage range ([token value methods](numeric-conversion.md#token-value-methods)). The
  scanner, zero-mask, `trigger` lookup and `parameters` limits are `OutsideMethod`. `CPersistent` block classification of `modifier` does not
  establish its member family. `potential`, `allow`, `effect`
  and `init_effect` enter only as a country with self-linked root, from and prev
  ([block entry contexts](#block-entry-contexts)). `ai_weight` keeps an entry gap: a template
  helper evaluates it on an array element ([weight blocks](weight-blocks.md#entry-contexts)).
  The acceptance test `council_agenda_fields_are_complete_with_every_milestone_4_fact`
  (`cargo parity council_agenda`) lists each missing fact and typed gap in one run. It accepts
  that one `ai_weight` gap and the range gaps of those four weight keys, when their storage is
  known; no Atlas claim needs either. It accepts no other typed gap.
- **Unknown kinds.** Most unknown kinds come from fields with no single established reader and
  from unresolved root paths, not from unclassified reader signatures (`CVariableValue::Read`,
  `CReader::Read(CColor&)`, `CReader::Read(float&)`, `CReader::Read(CVector2FixedPoint&)`). Their
  shapes are in [fields with no shared reader](#fields-with-no-shared-reader).
- **Compound shapes.** The dispatch walker follows a copied value token constructed as an event
  target and moved to an owner destination (`Target`), a `CString` array emplace read by the shared
  string reader (`String/Accumulate`, for example `common/asteroid_belts.mesh`), and an optional
  `CString` set from token text (`String`). A different token source, a stack move destination or
  an element from an unknown call cannot establish the shape. A stack `CString` made from the
  value token's text and passed to an owner call reads `String` with no destination, by a stated
  rule ([modifier blocks](modifier-blocks.md#fixed-key-readers-sdk-675)).
- **Constructed objects.** The method reads `tradition_swap` in traditions and ascension perks (13
  child fields each); in `advanced_authority_swap` the collection is established but not its
  children. The council presence branches normalize to unconditional integer reads. Objects built
  by a `PdxMakeScopedPtr` factory and moved into a scoped-pointer array join the same way; when the
  object's reader has a block family, as the 50 triggered modifier clauses do, the field carries that
  reader and its grammar instead of loader fields ([triggered modifiers](triggered-modifiers.md)).
- **Repeat behavior.** The [repeat rules](#repeat-behavior) give the eight block, operand and
  reference fields of council agendas their repeat; none keeps an `UnresolvedStorage` gap.

Pitfalls:

- A state branch before token discrimination would give a false registry-wide `UnresolvedPath`
  gap: the token-partition check needs one contiguous chain. No M45 loader has that shape.
- `b.hi` stops in `CEspionageOperationType` and `CStarClass` compare a value loaded from the object,
  not the token, so they correctly stay unknown flags.
- Unresolved concrete readers do not remove established field names or broad `Block` kinds.

### Fields with no shared reader

The sweep report's `reader_identity_stops` groups every root field, and every nested field of
`FieldMembers::Fields`, without a reader identity by the distinct shapes of its read
alternatives; these are the fields that carry the `UnresolvedReader` gap. Keys of an attached
modifier, weight or triggered-modifier grammar are outside it: their gaps come from that
grammar's own method. A stopped alternative gives its reason
and the callee or mnemonic where it stopped. `registries_by_root_gap` counts the registries with
an unfollowed root path, unnamed keys or unreadable input; `stop_shapes` gives their internal
stops, with a distinct registry count for each. Event-kind members count once for each kind.

M452, SDK-710: before at Native `34bd898` (`registry-fields/v34`), after at `registry-fields/v35`.

| Count | Before | After |
| --- | ---: | ---: |
| `UnresolvedReader` "no shared reader" gaps (fields, root and nested) | 873 in 113 registries | 779 in 100 |
| Root fields without identity | 419 | 338 |
| Root fields of kind `Unknown` | 382 | 301 |
| Root `Boolean` fields with repeat `Unknown` | 243 | 5 |
| Gaps other than `OutsideMethod` | 4,482 | 4,036 |
| Registries with an unfollowed root path / unnamed keys / unreadable input | 74 / 15 / 12 | 74 / 15 / 12 |

The shapes, grouped by family. Fields and registries are after the repair. Every census shape
belongs to one row.

| Family (census shapes) | Fields | Registries | Disposition |
| --- | ---: | ---: | --- |
| `CReader::ReadUniform<T>` lists | 98 → 0 | 41 → 0 | Resolved: [uniform lists](#uniform-lists). |
| Boolean stack temporaries (identity, repeat `Unknown`) | 240 → 2 root | | Resolved: [bit fields](#bit-fields); two named obstacles remain. |
| Owned objects: `operator new`, `make_shared`, `PdxMakeScopedPtr`, `InsertAtEmplace`, `SetSizeAndEmplace`, `shared_ptr::reset` | 352 | 40 | Gap. The value is read into an object that an unfollowed call allocates or inserts, then through a pointer. The event members (`trigger`, `immediate`, `option`, …, 8 × 22 kinds) are most of it. Repair: extend the collection pass (allocate, construct, virtual read, insert) to one owned pointer and to `shared_ptr` members. |
| Value text: `CString::CString(char const*)`, `strlen`, `CEventTarget::CEventTarget(CToken)`, `CReader::Read(CString&, bool)` into a non-owner destination | 171 | 29 | Gap. The value token's text goes to a name lookup, a comparison or an event target that no compound shape covers (event `id`, `title`, `location`, …). Repair: one compound shape per idiom, as for `Target` and `String` above. |
| Stack serializers: `X::CSerializer::CSerializer`, and `CReader::Read(CPersistent&)` of a stack object | 77 | 44 | Gap. The persistent read's object is a serializer on the stack (`CModifierWithTooltipData::CSerializer`, `CEconomicUnitTemplate::CSerializer`); no owner constructor installs its vtable, so no constructor join gives the reader. Repair: take the concrete reader from the vtable constant stored in the stack object. |
| Persistent read with no constructor-agreed reader | 54 | 25 | Gap. The read joins an owner destination, but no constructor-agreed vtable at that destination gives a reader; the constructor walk's own stops are in `stop_shapes`. Repair: the constructor join of that destination. One named case below. |
| Enum readers: `TokenToEnum<E>`, `Get*FromToken` | 20 | 14 | SDK-627 keyword domains. |
| Statement loops: `CReader::ReadSimpleStatement()` with `blr` | 21 | 16 | Gap. Keys of the block are read in a loop through virtual calls; the root-token walk has no key. Repair: a loop-key method. |
| Ignored keys: a scalar path that returns with no call, and `CReader::SkipBody()` for a block (`is_colony`, `is_boss`, `hide_size`) | 16 | 10 | Gap. The rule "the caller consumed the scalar, so a return accepts and keeps nothing" is not stated; with it the kind is `Skipped`. |
| Log or singleton calls before the reader (`CPdxLogFileAndLine`, `CLogger`, `GetInstance`) | 14 | 7 | Gap. The walk ends at the first call that is not a reader. Repair: go on past a call that receives neither the owner nor the reader, as the weight value pass does. |
| `CTriggeredEventDescription<…>::ReadEntry` (`desc`, `tooltip`) | 12 | 8 | Gap. A nested description grammar with its own trigger; repair with a description block reader. |
| Other single helpers (`ReadColor`, `ReadTexture`, `ReadShipClass`, `ReadBeamState`, `CCompareOperator::Read`, joins of different readers) | 42 | 20 | Gap. One helper each; repair when Atlas needs the field. |

**Inline `CString` copies in constructors.** `common/governments/civics` `modifier` and
`common/star_classes` `modifier` (handed over from SDK-676) read an inline `CModifier` slot whose
vtable the owner constructor installs, but the walk loses it on 24 of 32 paths. Before the slot,
the constructor copies `CString` members inline. On the long path, the copy's allocator is given
the member, so its result may be an owner address. A later store through it at an unknown index
may reach every owner byte after the member, the modifier's vtable included. The receivers test
`exact_build_string_and_token_constructor_effects` asserts this. Obstacle: no stated rule says
that the string allocator returns memory outside the owner. Repair: state that rule for the
`CString` allocator and check it on three constructors. It is an evaluator rule for every
constructor walk, command grammar included, so it is outside this walker repair (SDK-822).

Registry-wide gaps keep their owners: the unfollowed root paths stop at the shapes above and at
unsupported instructions (`blr`, `ret`, `cset`, `lsr`, `stur`) and branch conditions (`b.lt`,
`b.le`, `b.gt`, `b.hi`, `b.ls`); the unnamed keys are non-singleton token paths; the unreadable
input is the twelve owners with no `ReadMember` (`CJobTag`, `CTraitTag`, `CZoneType`, …).

## Repeat behavior

`FieldShape.repeat` comes from one rule for each shared reader, in `session/fields.rs::repeat`.
Each read alternative gets its own value, and the field keeps it when all alternatives agree. A
rule applies only to a tail call: a call with an unexamined continuation proves no final storage.

| Reader | Repeat | Engine fact (M452) |
| --- | --- | --- |
| Primitive readers (Boolean, integer, fixed-point, float, string), except `CPreTriggers::ReadValue` | `Replace` | The reader assigns one destination. A fixture repeats `unlocks_agenda`: two occurrences keep the second. |
| `NParserUtil::ReadTrigger<T>`, `ReadEffect<T>` | `Replace` | `ReadTrigger<CRootTrigger>` (`0x100047bd8`) logs `Duplicate trigger at '%s'` when the count at `+0x7c` is nonzero, calls `CTriggerCollectionBase::DeleteAll()` and reads into the same object. `ReadEffect<CEffect>` (`0x10020cc30`) logs `Duplicate effect`, deletes each child, clears the count at `+0x1c` and reads again. All five instantiations share the shape. |
| `CVariableValue::Read(CReader&, EScopeType)` | `Merges` | A literal replaces the literal slot; variable, trigger and script-value slots stay ([scoped numeric](scoped-numeric.md), live `repeat_*` cases). |
| `CReader::Read(CPersistent&)` of a weight block | `Merges` | The persistent read (`0x1025b820c`) tail-calls the virtual `Read` at slot `+0x20`. `CMeanTimeToHappen::Read(CReader&)` (`0x10092e3f4`) reads a bare value into `base` or calls `CPersistent::Read`, so `base` is replaced and earlier entries stay. |
| `CPreTriggers::ReadValue(CReader&, EPreTriggers)` | `Merges` | `0x1004d0008` logs `Pre-trigger "%s" is specified more than once` and still ORs the key's bit into the `yes` or `no` mask; no bit is cleared ([pre-trigger blocks](#pre-trigger-blocks)). |
| `CReader::Read(CPersistent&)` of a modifier block | `Merges` | All four variants reach `CPdxModifier<…>::Read` (`0x100071fcc`). It deletes the child array and clears the entry counts at `+0x1c`, `+0x44` and `+0x9c`, then calls `CPersistent::Read`. Fixed keys are written only when present, so an omitted icon, tooltip or flag stays; `description_parameters` appends. |
| `ReadKeyReferenceDeferred<D>` with an established lookup | `Replace` | Each occurrence registers its key and a lambda that writes the destination on a hit and on a miss. The resolver runs registrations in order ([references](references.md#reference-readers)), so the last occurrence's item is stored. |

A Boolean read into a stack temporary gets `Replace` from the walk, not from a rule: the walk
follows the temporary into its member bit ([bit fields](#bit-fields)).

Every other reader keeps `Unknown`, including `CTrigger::Read`, `CEffect::Read`, a persistent
block without a constructor-proven weight or modifier reader, an immediate reference reader,
whose caller stores the result after the call returns, and the [uniform list
reader](#uniform-lists).

**A recorded manual exception.** The rules are stated per reader, not derived: the method reads no
reader or resolver body for storage. Conditions: the exact M452 build and a tail call. Obstacle:
fixtures decode no block or pointer storage, and no shape reads the clear-and-reread or the resolver
order. Removal route: block and pointer storage decoders, or body shapes for those two facts. The
checks are the disassembly above, the live scoped matrix, an M45-observe probe (repeated trigger
and effect blocks change their child counts, a repeated graphical modifier resets its entries and
keeps omitted metadata, two deferred keys resolve to the second), and `cargo live
fixture_block_parsing`: a repeated `potential` logs `Duplicate trigger` on the second
occurrence's line, and a repeated `ai_weight` reads twice with no diagnostic.

**The storage check** runs on the assembled answer, after every grammar is attached. A field at
any `FieldMembers::Fields` depth gets an `UnresolvedStorage` gap when its repeat is `Unknown`, or
when its members are `Unresolved` and its reader family is `Unknown`. A trigger or effect family is
the members answer; a weight, modifier or triggered modifier family whose grammar did not attach
already has the attach step's `UnresolvedReader` gap.

Pitfalls:

- `CMeanTimeToHappen::Read(CReader&, EScopeType)` (`0x10092e328`) resets `base` and the entries,
  but the persistent path never calls it. Read the slot that `CReader::Read(CPersistent&)` calls.
- A deferred reader's key string is a temporary; storing it proves nothing. The destination that
  the lambda captures, the resolver order and the lambda's write decide the result.
- A logged duplicate is not a rejection; the repeat is read.

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

**An unknown index (SDK-812).** A path walk forks to every case of a table whose index is unknown
only when the code before the `br` proves the bound in this straight-line shape: the guard
`cmp wJ,#K; b.hi`; then only moves, address arithmetic and loads that write no base register,
which set no flags and do not branch; a frame that began at or before that run, and no direct
branch into it; and the index copied once from `wJ` by a 32-bit `mov` after the guard or, with no
copy, set by a 32-bit write before the `cmp`, so its upper half is zero. The adr's base must not be
the index or the table register, and the load must not overwrite the base. Any other shape keeps
the `branch-value` stop, and one path of `Machine::run` stops there too. Assumption: a branch
through a register does not enter a guard run. Checked on `CSpatialObjectRefCaster::FillEventScope`
(byte table, 11 cases, `mov w9,w0`), `CFleetOrbitalSlotHandle::ExecFunctor<SSetupScopeObjectFunctor&,
CEventScope&>` (byte table, 4 cases, `sub w8,w8,#1` before `cmp w8,#3`) and
`CreateFleetOrderButtons` (halfword table, 39 cases, `mov w8,w0`). On M452 it changes block entry
contexts, one `derived_names` gap, one on_action and five game-rule gaps
([SDK-812](#block-entry-context-stops-on-m452-sdk-812)).

**The default case.** The rule is in the module comment of `fields.rs`. Pitfall: "the most
frequent target is the default" is wrong. `CMissionType::ReadMember` has a table for tokens
11653–11656 with no default slot, and `on_fail` and `on_cancel` share one case that reads the same
effect member (`+0x430`); that case ties for the most frequent target. When a wide interval that
leaves through the table's guard does not end at the rejection, the default may be past its end,
so a case is kept only when it joins a known reader. No table on M45-release gives a `JumpTable`
gap: each default reaches the rejection, or the table has no default slot.

### Bit fields

Boolean bit fields such as `tooltip_show_star_resources`,
`place_entity_on_planet_plane`, `use_planet_resource`, `can_prevent_crisis_terraformation`,
`is_ruined_orbital_ring` and `hide_name` copy the bit to a stack temporary with `ldr` and `ubfx`,
or with `ldrb w8,[x19,x8]` where `x8` is a constant, then call `CReader::Read(bool&)`. The walker
reads a register-offset load with a constant index and forgets the result of `ubfx` and `and`, so
these paths reach the call and name their fields. `CReader::Read(bool&)` joins a stack
destination, as the 4-byte integer reader does.

After such a call the walk goes on (`fields/dispatch.rs`, `follow_temporary`, SDK-710). On M452,
240 of the 242 paths merge the temporary into its member with one idiom:

```text
bl    CReader::Read(bool&)       ; x1 = sp + t
ldrb  w8,[sp,#t]                 ; the value read
ldr   w9,[x19,#off]              ; the owner word: ldrb, ldrh, ldr w or ldr x
and   w9,w9,#~(1 << k)           ; clears exactly bit k
orr   w8,w9,w8,lsl #k            ; no shift for bit 0
b     <shared store>             ; optional
str   w8,[x19,#off]              ; same width and offset as the load
ret                              ; after ldp and add sp restores
```

or copy the byte itself (`ldrb w8,[sp,#t]; strb w8,[x19,#off]`). The path then joins
`Stored { CReader::Read(bool&), Boolean, destination, Replace }`: the bit gets the value that the
reader wrote, so a second occurrence replaces the first. `destination` is the member's offset for
a copy and `None` for a bit, because a storage offset names a whole word, and the presence test
of [read conditions](#read-conditions-and-use-time-inheritance) would read a byte test of the word
as a test of the bit. Any other instruction on the way, a call, a second store to the word, a
register loaded from the temporary before the call, an `ldp` that moves the stack pointer, or a
path bound keeps the plain join at the call. Root Boolean fields with repeat `Unknown` fall from
243 to 5.

Two temporaries keep the join at the call, with a `Repeat behavior remains unresolved.` gap:

- `common/governments/civics` `pickable_at_start`: the temporary is never stored; the path restores
  the frame and returns.
- `common/global_ship_designs` `alliance_ship_design`: the copy (`str w8,[x20,#0x114]`) is followed
  by a second read of the value token's type and, for a block, `CReader::ReadUniform<CString>`.

The other three (`common/agreement_presets` `hidden`, `should_ai_use_for_proposals`,
`can_preset_be_changed`) read straight into the owner and then call
`CAgreementTermData::SetPresetData`, so their call is not a tail call.

## Members and shared readers

A field's reader identity, kind and family come from every token path whose interval holds the
field's token, not only from its singleton paths. A wider path that is unresolved, or reads with
another reader, leaves the shared claim unknown; each read alternative keeps its own reader.
Rejected paths do not change the claim. Storage offsets, storage selections and derived-name
storage come from the singleton paths only, so they hold only when `partition_accounted` is true:
an overlapping wider path could join another reader, and then no offset is proven. No M45 registry
has an overlap. A condition on a nested field, on its reference lookup or
on a modifier-block fixed key names the full field path from the root field or command key.

### Reader identities and kinds

`registry_fields` reports one opaque identity for fields that share a joined reader, and one
conservative broad value kind. The kinds promise no complete grammar, valid ranges, occurrence
rules, reference ownership, nested behavior or runtime behavior, so a known identity can still
have kind `Unknown`. A missing identity means that the paths did not establish one shared reader.
The gap follows the field's own `Field.reader`, which the method builds once: no identity gives an
`UnresolvedReader` gap, and an identity with kind `Unknown` gives a `ReaderSemantics` gap. A nested
field of a collection gets its gap under its dotted name, and a collection field itself, which
always has an identity, gets none. A reader path gap on the same field adds no second
`UnresolvedReader` gap. A persistent read whose destination holds no known reader has no identity,
so its field has the gap too. On M452, 402 of the 1,770 root and nested fields have no identity
and 5 have an identity with kind `Unknown`; each has its gap. A field that a gap already names
gets no container-category gap of its own ([modifier masks](modifier-masks.md)).
`Reader.family` identifies the child family separately. Conditional read alternatives keep their
own families; conflicting or unresolved alternatives cannot establish an unconditional family.
Constructor joins refine generic persistent reader identities. Modifier fields carry
[modifier blocks](modifier-blocks.md); conversion facts are on
[numeric conversion](numeric-conversion.md).

A reader ID is the first 16 hexadecimal digits of the SHA-256 of the demangled callee name
(`ReaderId::from_callee`). So the same callee gives the same ID on every build: the
`registry-fields/v2` sweep on M45-observe and the `registry-fields/v3` run on M45-release found the
same 19 IDs. The hashed name of each `NParserUtil` template keeps its `void ` return type.
`cargo run --release --example reader-kinds -- "$STELLARIS_PATH" [REGISTRY…]` counts fields by
kind; the default registries are traditions, tradition categories and council agendas.

Pitfalls:

- **Identity does not establish value kind.** A command reader proves the constructor-installed
  virtual `Read` and `ReadMember` targets. It does not prove what the `Read` override accepts, so
  marking every such receiver `Block` adds an unsupported fact.
- **An outer reader's family is not the set of its child families.** `random_list` is an effect
  reader whose outer numeric keys lead to a separate effect-child grammar.
- **The family checks answer different questions.** Shared reader-entry classification gives the
  broad value of established helper signatures; constructor joins refine a generic persistent
  destination; conditional normalization also accounts for unresolved and rejected paths. Do not
  merge them: removing the last check promotes conditional facts.

### Uniform lists

`CReader::ReadUniform<T>(CPdxArray<T, int>&)` appends each value of a `key = { a b c }` list to the
array in `x1` (`0x10004e608` for `CString`: `InsertAtEmplace`, then `CReader::Read(CString&,
bool)`, until the closing brace or the end of input; it clears nothing first). Any element type
and both `enable_if` forms of the name are one shared reader shape (`readers.rs`,
`is_uniform_list`): `x0` the reader and `x1` an owner array, or a stack array that the loader
converts after the call (17 fields on M452, such as `common/traits` `requires_traits`). The kind is
`Block` and the family `NotApplicable`: the values are not commands, so no modifier container is
read and the read scope is `Known([])`. The identity keeps the element type. Storage and repeat stay unknown; a stack array has no
destination. `NSerializer::ReadUniformWithCustomSerializer` is not this reader. Command
grammar shares the reader rule: nested lists such as `create_species` `adjective.variables`
join. 17 command answers change (`tests/population/m452/command-population/`): list and
bit-field joins remove `reader-routing` gaps or give `Replace`, and `has_active_event` now stops
later, at its value acceptance.

### Read conditions and use-time inheritance

These facts are for M45-release.

- `CCouncilAgenda::ReadMember` (`0x10020bfa8`) tests presence bytes at `+0x6d0` and `+0x6d8`. Both
  sides reach `CReader::Read(int&)` after clearing the value at `+0x6d4` or `+0x6dc`: the branches
  initialize presence and do not restrict which occurrences are read. Equal complete outcomes on
  opposite presence branches collapse to `Always`; unresolved outcomes and unequal destinations
  cannot.
- A presence test names its field (`FieldZero`) only when the loader tests, with `cbz` or `cbnz`, a
  word of the field's own width at the offset where an integer or Boolean reader stores it. A
  float or fixed-point reader at that offset leaves the condition `Unresolved`: a zero word is not
  shown to be the scalar's zero for them (`-0.0` is a nonzero float word). Remove the limit when
  a loader tests such a field and the test is read.
- `CTraditionType::ReadMember` (`0x100cdcf38`) allocates and reads a swap, then inserts its pointer
  into the collection at `+0x5c8`; the method proves construction, virtual read and insertion of the
  same object on all paths. The pointer insertion specialization (`0x100ce42ac`) stores into the
  buffer at `+8` and increments the count at `+0x14`; the binding derives the buffer member from it.
  Reconstruction before or after insertion, owner writes that could reset the collection, and
  unknown-pointer writes invalidate the proof.
- `CTraditionSwap::ReadMember` (`0x100cdc0b4`) routes tokens `0x397e`, `0x397f` and `0x3980`
  (`inherit_effects`, `inherit_name`, `inherit_icon`) to Booleans at `+0x4f0`, `+0x4f1` and
  `+0x4f2`. These flags do not gate the other fields in that reader.
- Use-time selections, not parser conditions: `CTraditionType::GetName` (`0x100cdd944`) picks a
  swap by possibility and weight, checks its validity and `+0x4f1`, and on zero uses the swap's name
  at `+0xf8` (`0x100cdda28`), otherwise `GetBaseName`. `GetIconKey` (`0x100ce0028`) checks `+0x4f2`
  (`0x100ce0104`) and returns the swap name or the base key at `+0x10`. `OnEnabled` (`0x100ce25b8`)
  and `OnDisabled` test `+0x4f0` (`ldrb` `0x100ce26ac`) and `csel` (`0x100ce26b8`) the swap effect
  `+0x378` or the base effect `+0x418`; the same flag selects modifier and tooltip members.
  `CTraditionType::CalcAIWeight` (`0x100ce2cdc`) reads its own `+0x568`, which does not show that
  a swap inherits the weight; the SDK-545 amendment removed that question.

Use selections carry `All(Unresolved, FieldZero(...))`: the unresolved part covers the choice of
swap, validity, other branches and method bounds. An empty selection does not prove no runtime
condition. Use analysis admits direct `const` owner methods, whose signatures establish the
receiver; static, nested-class and other receiver shapes, including
`CTraditionType::PostReadInit()`, are not proven. The local flag proof follows copies of the
receiver; a shared loop load address does not establish object identity, and a conditional select
must get its flags from the adjacent comparison on every incoming path. SDK-546 depends on these relationships for
conditional name and icon templates. Locate the inheritance tokens with `inspect -- --strings inherit_`.

Prototype findings, in the `atlas-discovery`, `atlas-command-grammar` and `atlas-numeric-grammar`
bundles (see [retrieval](retrieval.md)): token dispatch is discovered without field or config seeds
and transferred to AI attitudes; factories bind to inherited readers (six `create_starbase` and
three `add_district` fields); one shared numeric operand grammar serves agenda cost and the
timed-flag units, while `agenda_cooldown` is a plain integer.

### Unresolved loader conditions on M452 (SDK-811)

An accepted limit. On M452 the sweep keeps 207 "loader condition remains unresolved" gaps: 132 on
the six `CEvent` members of each of the 22 event kinds, and 75 root fields in 33 other registries.
By the test that stops:

| Tested value | Outcomes | Gaps | Registries | Examples |
| --- | --- | ---: | ---: | --- |
| Owner word with no established named-field destination (pointer members, unnamed bytes) | Every one unresolved | 107 | 8 | `events` `desc`, `mean_time_to_happen`; `common/governments/civics` `potential` |
| Reader input (token kind or reader state) | Every one unresolved | 88 | 30 | `events` `picture`; `common/colony_automation` `buildings` |
| Owner word with no established named-field destination | Some unresolved | 5 | 1 | `common/special_projects` `on_start` |
| Reader input | Some unresolved | 4 | 4 | `common/traits` `icon`; `common/ship_sizes` `graphical_culture` |
| Reader input: block token or scalar | Established, unequal destinations | 2 | 2 | `common/ship_sizes` `hero_ship`; `common/pop_categories` `can_change_category` |
| Global load and reader input | Every one unresolved | 1 | 1 | `common/traits` `leader_class` |

No tested owner offset matches an established float or fixed-point destination, so the presence
word limit of [read conditions](#read-conditions-and-use-time-inheritance) has no member here. A reader-input test chooses a value form; `FieldCondition` states
stored values only. Atlas credits a field's value form and repeat only when `read` is exactly
`[Always]`, and reads no `FieldCondition` term, so naming a condition credits no claim. A branch
collapses to `Always` only when complementary tests reach equal complete outcomes; no gapped field
has them. The other 205 gaps have at least one unresolved outcome: reader paths that SDK-710 owns.
The 45 use-time gaps also stay: Native's `Field::uses` publishes the selections, and Atlas credits
no claim from them.

`ACCEPTED_LIMITS` in `examples/registry-sweep.rs` lists the 207 fields by registry, and accepts
the use-time gaps by their details, so the sweep counts them apart from open work (SDK-819); a
loader-condition gap of another field stays open. Remove a field from the list when its reopen
condition holds. Revisit the 205 when SDK-710 gives complementary paths equal complete outcomes (rerun the sweep:
the existing collapse removes those gaps), or when Atlas consumes condition terms, for example to
select a config subtype (SDK-716). The two block-or-scalar fields already have complete, unequal
outcomes, so neither trigger changes them: revisit them when Atlas needs a value form that depends
on the input token, and decide then how an answer states a reader-input condition.

## Registry scheduling and owner joins

From the SDK-489 prototype on M45-observe (`atlas-ownership/prototype/registry-ownership/`):

- Symbol enumeration finds 164 exact template `LoadFile` candidates, including owners without a
  separately named member reader. Neither the candidate count nor the scheduling table proves that
  all registries were found.
- An owner is established by the loader's receiver and directory, file activity, the root
  constructor key, the concrete owner, the persistent base, the vtable offset-to-top and the
  shared member-dispatch slot. A filename on the stack is not enough: top-frame names can be stale,
  so use the direct caller and the receiver directory.
- The static-modifier custom loader has a key-only phase and then a full-read phase. The owner rule
  transfers to six economic-plan roots; the other 243 reader occurrences are not named definitions.
- With two mods in normal and reversed order, the game selects one shared virtual filename, and
  separate duplicate files are processed in a/b order. A category duplicate reconstructs the same
  object and keeps its numeric ID; a modifier duplicate reuses the owner. The category loader reads
  a `.bin` fixture. Final merged values were not measured: these runs establish no extension
  policy, physical-file resolution or "last value wins". On M452 the
  [loader rules](loader-rules.md) state these from the code.
- Failed approaches: one observer form had too much overhead and others failed to start; the first
  transfer to the AI budget failed until the owner rule was revised.

### Registry names

`Native::registries` names each template registry by the content directory that its constructor
passes to the shared base constructor; the module comment of `engine/analysis/directories.rs`
gives the two compiled shapes. On M45 the method named all 164 template registries in about two
seconds, and all 162 directories that the retained live run observed are in the result, with no
conflict. The two candidates that the M45-observe startup run did not observe are
`CAIEspionageTargetPoliciesDatabase` (`common/ai_espionage/targets`) and `CGameScenarioDatabase`
(`common/game_scenarios`) (`atlas-ownership` bundle,
`prototype/registry-ownership/review.md`). Both are template registries on M452: a loader that
did not run before the startup boundary is not a missing registry.

- On M452, six template registries load from outside `common/`, such as `map/galaxy`,
  `sound/advisor_voice_types`, `gfx/portraits/sprite_configurations` and
  `interface/resource_groups`. So a registry name is the full content directory.
- On M452, code forms the address of 43 directory literals under `common/` that no template
  registry names (51 literals; 8 of them repeat a directory with a trailing `/`). Find them with
  `inspect -- --strings common/` and remove the 164 names of `registries()`. Examples are
  `common/component_templates`, `common/static_modifiers`, `common/traits` and `common/on_actions`.
  17 of the 43 are named by their database's own loader ([Custom-loader names](#custom-loader-names)).
  A literal is not a loader: `RunGame` forms the address of 157 of the 158 `common/` template
  directories and of `events`, and for 13 of the 43 (such as `common/agendas`, `common/planet_classes` and `common/subjects`) it is
  the only function that does. The "59" of the M45-observe count had no list.
- `common/lawsuits` is the template registry of `CLawsuitTypeDatabase`, and the game installs
  that directory. The config's `lawsuit` type gives `game/common/lawsuit_type`, a directory that
  the engine does not load and the game does not have: a config path error, not a missing
  registry.
- **Nested definitions have no identity of their own (M452).** A swap object is built by its
  owner's reader and kept in the owner's item. No swap class (`CTraditionSwap`,
  `CTechnologySwap`, `CAdvancedAuthoritySwap`, `CGovernmentCivicTypeSwap`, and the
  `CSwappable` data of jobs) has a database, a `LoadFile`, a `TGameDatabase` instance or a key
  reference reader; each has only a null object. `CTraditionType::ReadMember` inserts each swap
  pointer into the item's collection (buffer `+0x5d0`, count `+0x5dc`). The one swap-name lookup
  read, the deferred lambda of `CHasActiveTraditionTrigger::Read` (`0x1021bedb0`), walks
  `TGameDatabase<CTraditionTypeDatabase>`: it compares the key with each tradition's key (`+0x10`),
  then with the name (`+0xf8`) of each swap whose `inherit_effects` byte (`+0x4f0`) is zero, and
  stores the tradition at `+0x68` and the swap at `+0x70`. So a swap name resolves only as the
  pair (owner item, swap), through its owner, and a swap that inherits effects is not found by
  name. The config's `swapped_tradition`, `swapped_ascension_perk`, `swapped_technology`,
  `swapped_authority`, `swapped_civic` and `swapped_job` types are these objects. A registry is
  named by its content directory, which a swap shares with its owner, so a nested definition
  stays a field under its owner's path (`FieldMembers::Fields`). The other `skip_root_key` types
  of the config skip a wrapper key (`collection`, `named_lists`, `alerts`, the graphics and
  interface families); they are not nested definitions.
- Pitfall: the first version of the method took any directory-shaped literal in the constructor.
  It missed the global `CString` shape of `common/ship_categories`, and it had no tie to the
  meaning of the literal. The base-constructor call anchor replaced it.
- Pitfall: a scan that stops at every conditional branch loses loaders. `CSpeciesNamesDatabase::Init`
  runs a loop before it passes `common/species_names` to the file enumeration. The scan continues
  past a conditional branch, and the target of one forgets every register and built object.

### Database schedule

`Native::registries` reads the table of `NNullObjAndDatabaseInitUtil::SetupDatabases` to find
the scheduled databases that no template candidate covers (`discovery/schedule.rs`; its module
comment gives the scan). Each unjoined row is a custom-loader candidate, named as the next
section says; the answer counts the rows it cannot name in its `UnnamedRegistries` gap, with no
class name. The scan finds the function by its name
and derives the end of the fill from the code; no recipe holds an address.

- On M452 the function starts at `0x1005eb000`. After `mov x19, sp`, the table is at `x19 + 96`:
  **198 rows of 48 bytes**, six 8-byte slots each: the name as a C-string address, then
  `CreateInstance()`, `DestroyInstance()`, a fourth slot, `InitInstance()` and
  `PostReadInitInstance()`. The fourth slot is zero (`stp x8, xzr`) or a `CanInitialize()`
  function; 126 rows have the function. Row 0 is `CNamedColorDatabase`. Every slot of every row
  resolves.
- The fill ends at `0x1005ee4b8` (`ldrsw x8, [x0, #0x14]`), the first instruction that is not a
  literal step. The code after it copies `0xc6` (198) rows of `0x30` bytes from `x19 + 0x60`.
  The compiler reorders the stores: a slot can be stored long after its row, such as
  `str x9, [x19, #0x1f90]` near the end.
- A template row's functions are `TGameDatabase<T>::…` in `__DATA_CONST,__got` slots, so they
  resolve only through the chained fixups. A custom-loader row's functions are the database's
  own (`CStaticModifierDatabase::CreateInstance()`), often through `adrp` and `add`.
- 163 rows join exactly one template candidate by the database type in their function symbols;
  `CGameScenarioDatabase` is the one candidate with no row. 35 rows are outside the template
  method: `CStaticModifierDatabase`, `CWeaponTagDatabase`, `CStrategicResourceDatabase`,
  `CShipBehaviorDatabase`, `CAmbientObjectDatabase`, `CEmpireFlagDatabase`, `CGfxCultureDatabase`,
  `CProjectileGfxDatabase`, `CPortraitDatabase`, `COpinionModifierDatabase`, `CPlanetClassDatabase`,
  `CAdvisorDatabase`, `CPingMapDatabase`, `CFallenEmpiresDatabase`, `CShipDesignTemplatesDatabase`,
  `CSpeciesNamesDatabase`, `CNameListDatabase`, `CDesignerDatabase`, `CEventManager`,
  `CSpecialProjectDatabase`, `COnActionDatabase`, `CTraitDatabase`, `CPrescriptedSpeciesDatabase`,
  `CDiploPhraseDatabase`, `CMessageSystem`, `CAlertSystem`, `CTerraformDatabase`,
  `CStartScreenMessageDatabase`, `CSystemInitializerDataBase`, `CGalaxyTemplateDatabase`,
  `CWorldGfxDatabase`, `CColorsDatabase`, `CGameSettingsDatabase`, `CEmpireDesignDatabase` and
  `CScriptableLocalizationDatabase`. Some are not script registries (`CMessageSystem`,
  `CAlertSystem`); the row count is not a registry count. M45-release had the same counts.
- Pitfalls, checked again on M452: the stack probe `blr x16` (`___chkstk_darwin`) at
  `0x1005eb028` is a call before the table owner, so the scan accepts calls before `mov x19, sp`
  and stops at the first branch; without that move the answer is a gap, never an empty table. A
  new value in `x19` during the fill invalidates the owner and is a gap. An `ldr` from a `__got`
  slot gives nothing without the chained fixups.
- The M45-release reader decoded a hand-found window (`0x1005eb938` to `0x1005eedf0`) and was
  removed in `e9261a1`; the current reader derives the end instead.

### Custom-loader names

For each unjoined row, `loader_directory` (`directories.rs`) reads every function qualified by the
database class (`CTraitDatabase::…`) and takes the literal first argument of each
`VFSGetEnumeratedFiles` call. The literal can also come through a global `char const*`: an `ldr`
from a known address loads the pointer slot, and the slot's rebase target is the literal. Exactly
one distinct literal names the registry; several, none, or an argument that the scan does not
establish leave the row in the `UnnamedRegistries` gap.
`registry_fields`, `derived_names` and `modifier_families` read these names through their
[owner](#custom-loader-owners) ([derived names](derived-names.md#population),
[modifier families](modifier-families.md#roots)). `UnknownRegistry` is for names that `registries()` does not list. The
references and modifier-block methods use the same join (`references::database_directories`).

On M452, 22 of the 35 rows are named, with 21 names: `common/ambient_objects`, `common/colors`,
`common/component_tags` (`CWeaponTagDatabase`), `common/diplo_phrases`, `common/fallen_empires`,
`common/global_ship_designs` (`CDesignerDatabase`), `common/on_actions`,
`common/opinion_modifiers`, `common/planet_classes` (through `_PLANET_CLASSES_DIR`),
`common/ship_behaviors`, `common/special_projects`,
`common/species_names`, `common/start_screen_messages`, `common/static_modifiers`,
`common/strategic_resources`, `common/terraform`, `common/traits`, `events` (`CEventManager`),
`flags` (`CEmpireFlagDatabase`, through `_pEMPIRE_FLAG_FOLDER`), `gfx/worldgfx` and
`prescripted_countries`. None repeats a template name.

- **One directory, two databases.** `CEmpireDesignDatabase` and `CPrescriptedSpeciesDatabase`
  both enumerate `prescripted_countries`, so it is one name. The owner join does not select one of
  the two: `registry_fields` gives `Unsupported` ("2 databases load this directory").
- **A global `char const*` is read at its initial value.** `CPlanetClassDatabase::Init()` loads
  `_PLANET_CLASSES_DIR` (`0x1032af358`, rebased to `"common/planet_classes"`) with `adrp x8,
  #0x1032af000; ldr x0, [x8, #0x358]` and passes it to the enumeration. Both globals are in
  writable `__DATA,__data`, so the method assumes that nothing changes them before the loader
  runs. M452 checks: every instruction that uses either address after an `adrp` of its page
  loads from it (8 loads; one `add` that a load follows), and no fixup targets either slot
  (`dyld_info -fixups`). If a store or an escaped address is found, or a run reads another
  directory, a slot is no longer proof. A load into `xzr` is not a slot value.
- **The `RunGame` caller is not the loader.** `RunGame` passes `"common/planet_classes"` and
  `_pEMPIRE_FLAG_FOLDER` to `VFSPreEnumerateFiles(char const*, char const*, char const*, int)`, a
  warm-up of the file list; the database's own `Init()` enumerates the files. The ticket's caller
  shape was therefore not built. Whether a caller supplies the directory of one of the 13 rows
  below was not checked; their obstacles are in their own functions.
- **The 13 rows that stay gaps**, by obstacle:
  - The directory is the text of a `CString` member or a built path (the short-string `csel` on
    the object): `CGfxCultureDatabase`, `CNameListDatabase`, `CGalaxyTemplateDatabase`
    (`ReadFiles`), `CMessageSystem` (`InitTypes`), `CPingMapDatabase`, `CProjectileGfxDatabase`
    (`InitInstance`), `CScriptableLocalizationDatabase` and `CSystemInitializerDataBase`.
  - Several directories: `CShipDesignTemplatesDatabase::Init(bool)` enumerates
    `common/section_templates`, `common/component_sets` and `common/component_templates`;
    `CPortraitDatabase` enumerates a global path, a member path and
    `gfx/portraits/portraits/substitutions`. One database that loads several config types is not
    one registry.
  - No enumeration call in the class's own functions: `CAdvisorDatabase`, `CAlertSystem` and
    `CGameSettingsDatabase`.
- Pitfall: a row with one literal is not proof that the database owns the items there; it is the
  directory the loader reads. `prescripted_countries` shows that a directory can feed two
  databases.

### Custom-loader owners

`registry_fields` needs only an owner class: its root is `<owner>::ReadMember(CReader&, int)`. A
template loader names its owner as a template argument. For a custom-loader registry,
`loader_owner` (`discovery/loader_owner.rs`; its module comment gives the rule) reads the
database's own functions. The loader route is each of them that calls the file enumeration and
each of them that the route calls directly. The owner is the one class with a member reader that
the route builds: it calls the class's constructor or forms its primary vtable address point, as
an inlined constructor does. Two built classes are owners [by root key](#root-key-owners-sdk-813)
when one root key selects between them. None, or several in any other shape, is `Unsupported`
with the reason, never a choice.

- **Representation.** The join lives in `ScheduledDatabases::loader_owners`, beside
  `loader_directories`, and is read once with the schedule. It is not a `NamedCandidate`: the
  candidate slice feeds the reference, modifier-block and expansion readers and the
  `RegistryRoot` hosts of `script_expansions`, and a custom loader has no `LoadFile`, initial
  loader or template class for them. The field input carries only the owner (`FieldInput::owner`),
  since every field reader used only `owner_candidate` of the candidate record. The block-entry
  method takes the joined owners with the template owners, so the blocks of a custom owner get
  entry contexts (`common/strategic_resources` `potential` enters as a country).
- **Validated assumption, not a proof.** A built class is taken as the item that the loader
  reads; the method does not follow the built object to the read call. On M452 each joined route
  builds the object, then reads it: `CStaticModifierDatabase::Init(bool)` allocates, forms `vtable
  for CStaticModifier`+0x10 and calls slot `+0x20` of the object (`CPdxModifier<…>::Read(CReader&)`,
  whose member dispatch at slot `+0x28` is `CStaticModifier::ReadMember`);
  `CAmbientObjectDatabase::InitInstance` forms the `CAmbientObjectData` point and calls
  `CPersistent::Read(CReader&)`; `CTerraformDatabase::InitInstance` forms the `CTerraformLink`
  point and calls slot `+0x20`; `ReadEventsFile` constructs `CEvent` ([the events
  loader](#the-events-loader-on-m452)). The parity test
  `m452_schedule_rows_outside_the_template_method_are_custom_loader_candidates` fixes every owner.
  Follow the object to its read call when a build has a route that builds a persistent object
  that it does not read as the item.
- **Static modifiers.** The key-only phase of `CStaticModifierDatabase::Init(bool)` calls
  `CStaticModifier::ReadOnlyKey(CReader&)` and reads no definition. The full-read phase reads
  through `CStaticModifier::ReadMember(CReader&, int)`, the root. That reader handles `key`,
  `apply_modifier_to_other_planets` and `divide_over_pop_groups`, and tail-calls
  `CGraphicalModifier::ReadMember` for every other token; the method does not follow an inherited
  reader, so the answer is partial with an `UnresolvedPath` gap on the registry.
- **Function budget.** `CTrait` has 174 functions in its field input, above the old limit of 128
  functions (`fields.rs`); the limit is 256 now. The 4 MiB code limit is unchanged (the input is
  0.4 MiB). Raise the count again only with a registry that exceeds it.

On M452, 16 of the 21 names join an owner (sweep at `registry-fields/v31`, and `v34` for
`common/planet_classes`; fields, then the answer):

| Registry | Owner | Fields | Answer |
| --- | --- | --- | --- |
| `common/ambient_objects` | `CAmbientObjectData` | 7 | partial |
| `common/diplo_phrases` | `CDiploPhrase` | 5 | partial |
| `common/fallen_empires` | `CFallenEmpireType` | 6 | partial |
| `common/global_ship_designs` | `CShipDesign` | 32 | partial |
| `common/on_actions` | `COnActionList` | 2 | partial |
| `common/opinion_modifiers` | `COpinionModifier` | 10 | partial |
| `common/planet_classes` | `CPlanetClass`, by root key | 70 | partial |
| `common/special_projects` | `CSpecialProjectType` | 26 | partial |
| `common/species_names` | `CSpeciesNames` | 5 | partial |
| `common/start_screen_messages` | `CStartScreenMessage` | 3 | partial |
| `common/static_modifiers` | `CStaticModifier` | 3 | partial |
| `common/strategic_resources` | `CStrategicResource` | 22 | partial |
| `common/terraform` | `CTerraformLink` | 10 | partial |
| `common/traits` | `CTrait` | 105 | partial |
| `events` | `CEvent` | 30 | partial |
| `flags` | `SFlagColor` | 4 | partial |

None is complete and none failed. Six were `Unsupported`; SDK-813 answers `common/planet_classes`
([root-key owners](#root-key-owners-sdk-813)) and states a narrower reason for the other five.

- Pitfall: a constructor anywhere in the database's functions is not an owner.
  `CDiploPhraseDatabase::GetRandomValidPhrase` and `CTerraformDatabase::HasAvailableLinks` build a
  `CEventScope` (which has a `ReadMember`) on the stack to test triggers; they are off the route.
- Pitfall: "any class whose method the loader calls" gives `CGameState`, `CEventScope` and
  `CScopeObjectReference` for `CEventManager`.

### Root-key owners (SDK-813)

`CPlanetClassDatabase::Init()` (`0x10115a26c`) builds two classes with a member reader. After
each `CReader::ReadSimpleStatement()` (reader at `sp+0x70`) it loads the token at `sp+0xa8`
(reader + `0x38`), compares it with `w23`, which holds `0x2714` (`random_list`), and branches
`b.ne`. The equal side allocates `0x90` bytes, forms `vtable for CPlanetClassRandomList`+0x10 and
calls `CPersistent::Read(CReader&)`. The other side builds `CPlanetClass(CString const&, int)` from
the key, or finds the class of that key and reads into it again, and reads through slot `+0x20`.
Both go back to the statement read.

- **Rule.** `loader_owner` takes two built classes as owners by root key when a route function
  has one statement read; the next three instructions load the word at the reader argument plus
  the token offset, compare it with a constant (an immediate, or a register whose last write
  before the comparison, in code order, is a `mov` of one; a call replaces a caller-saved
  register) that names a token, and branch on equality; and the code from each side up to the
  next statement read builds exactly one of the two classes, a different one on each side. Any
  other shape stays `Several`. Validated on M452 (the exact-build test above), not a proof: the method
  does not follow a path-dependent write of the constant.
- **Answer.** The registry's owner is the class of every other key (`CPlanetClass`), and its
  fields are the registry's root fields. The reserved key is one more root field, `random_list`:
  a root statement, as an [event kind](#event-kinds) is, not a member of a planet class. Its
  members are the fields of `CPlanetClassRandomList`, `name` and `planets`, from a field run of
  their own (the kept results are by registry and owner); their conditions, member gaps and registry-wide gaps move
  under the key, and the run's `OutsideMethod` gaps, which repeat the owner's, are dropped. The
  key's reader is a block with no identity, and its read scope is unresolved with a gap: the
  method does not follow the inlined construction to `CPersistent::Read(CReader&)`.
  `derived_names` and `modifier_families` answer `CPlanetClass` with one `OutsideMethod` gap
  that names the reserved key. Atlas maps `random_list` to `type[planet_class_random_list]` (12
  claims) and the other root fields to `type[planet_class]` (375) in SDK-818.
- **The random list's reader.** `CPlanetClassRandomList::ReadMember` (`0x10115ce44`) first tests
  the sign bit of a string's last byte (`ldrsb w8,[x0,#0x1f]`, `tbnz w8,#31`) and, when its
  location string is empty, sets it from `CReader::GetFileLocationDescription()`; then `planets`
  (`0x323b`) reads with `ReadUniform<CString>` into `+0x60` and `name` (`0x1b`) with
  `Read(CString&)` into `+0x30`. The walker loads `ldrsb` as a byte and follows a `tbz`/`tbnz`
  of a value that is not the token both ways, under an unresolved condition (a bit test of the
  token is a gap). The paths through the location call stop there, so each key keeps an
  unresolved alternative. Across the sweep these two rules changed only this answer.
- **Pitfall.** A consumer that reads every root field as a member of an item reads `random_list`
  as a planet-class field. The reserved key is a root statement; a planet class has no
  `random_list` member.

The other registries keep `Unsupported`, with the reason that the build's recipe states for each
loader (`LoaderReason`, `binding/groups.rs`); a loader with no stated reason gets the general one,
so another build never reuses an M452 reason. What would answer each:

| Registry | Loader on M452 | Reason | What would answer it |
| --- | --- | --- | --- |
| `common/colors` | `CColorsDatabase::Init()` (`0x1012fb45c`) reads 7 fixed root keys (`0x2ec8`, `0x2b52`, `0x2c88`, `0x2e95`, `0x2ec9`, `0x3066`, `0x42f6`), each a block of `rgb`/`hsv` entries read with `CReader::Read(CColor&)` into the database | Fixed keys of unnamed colours; no item | An answer form for a block of unnamed values. Atlas's `type[country_color]` has 3 open claims |
| `common/component_tags` | `CWeaponTagDatabase::InitInstance()` passes each token to `SComponentTag::RegisterTag` | Each token is a tag; no item | Nothing: Atlas reads it as `enum[component_tag]` |
| `common/ship_behaviors` | `CShipBehaviorDatabase::ReadFile` (`0x1011b78b4`) takes `ship_behavior` (`0x311c`) and calls `CShipBehaviorReader::CreateAndRead(CReader&)` (`0x1011b79b4`), which builds `CShipBehavior` (no `ReadMember`), keeps it at `[x19]` and reads its own statements: token at `[x20,#0x38]`, compares and two byte jump tables | Own statement loop, no member reader | A field walk that starts after that statement read, with `[x19]` as the owner and a return to the loop as unresolved, not rejected. Only `Read(CString&)`, `Read(bool&)` and `Read(CFixedPoint&)` are shared readers there; the other keys use `CToken::GetFloat`, `CreateMovementPatternFromToken` on the value token or `ParseAdvancedStatement`, so most fields would stay unresolved. Atlas's `type[ship_behavior]` has 94 claims |
| `gfx/worldgfx` | `CWorldGfxDatabase::ReadMember` builds `CWorldGraphicalMapData` on the stack and reads it with `CReader::Read(CPersistent&)` | The database is the read object | Milestone 6 |
| `prescripted_countries` | Two databases enumerate it | "2 databases load this directory" | Milestone 6 |

### Owner vtables on M45-release

For a new owner join, use `vtable_group` in `binding/binary/families.rs`: it reads one class and
checks its typeinfo. The removed image-wide scan is at `git show 8d9a073:src/binding/binary/discovery.rs`.

- The owner rule needs the persistent base's offset-to-top and the shared member-dispatch slot:
  slot 5, the address point + 40. For 159 of the 164 candidate owners that slot holds
  `ReadMember(CReader&, int)`, through a `virtual override thunk` for 149; the base's offset-to-top
  is -56 for 155 owners. `CComponentSlotTemplate`, `CJobTag`, `CTraitTag`, `CStarbaseBuilding` and
  `CStarbaseModule` have no such vtable. Example: `CTraditionCategory` address point `0x103095310`,
  offset-to-top -56, member slot `0x100cd92b0`.
- Pitfalls: a scan that does not check that the second word is the class's own typeinfo accepts
  false address points (`CMegaStructureType` `0x1030b08f8` is `~CMegaStructureTypeDatabase()`;
  `CCouncilAgenda` `0x10301b9d8` holds its typeinfo in the member slot). Slot 5 holds the reader
  only in the persistent base.
- The chained fixups bind 29,695 slots to other images, 10,994 of them named; `internals::inspect`
  names imports from the fixups.

### The events loader on M452

`events` is not a template registry: `CEventManager` has no `LoadFile`. `registries()` lists it
through its own loader, and `registry_fields` reads it from `CEvent` ([custom-loader
owners](#custom-loader-owners)). Its route:

- `CEventManager::InitInstance` calls `ReadEvents(CGameGraphics*, bool)` (`0x1004d51fc`), which
  enumerates `events` for `.txt` files (`VFSGetEnumeratedFiles("events", …, ".txt")`) and calls
  `ReadEventsFile(CString const&, bool)` for each. `FileWasChanged` calls the same function. Only
  these two `CEventManager` functions, `RunGame` and the token table form the address of `"events"`.
- `ReadEventsFile(CReader&, int&, bool)` (`0x1004d5944`) reads each root statement and switches
  on its token, with direct comparisons and two byte jump tables (`0x102c784ca` for tokens
  `0x2c80`–`0x2c84`, `0x102c784cf` for `0x3eea`–`0x3eee`). An event kind sets one `EScopeType`
  constant; then the loader allocates `0x630` bytes, calls `CEvent::CEvent(EScopeType)` with that
  constant and reads the event through slot `+0x20` of its vtable. Other root keys:
  `namespace` registers a `CNameSpace` in the manager's hash table; `inline_script` reads the
  expanded script with a recursive `ReadEventsFile`; any other key logs `Corrupt Event Table Entry`.
- **One owner reader.** Every kind builds the same class, so `CEvent::ReadMember(CReader&, int)`
  (`0x1004bbc9c`) is the one member reader for all kinds. The kind is only the scope constant.
- **The scope.** `CEvent::CEvent` passes the constant to `SEventData(EScopeType)`, which stores
  it at `CEvent + 0x68`. `ReadMember` loads that word as the scope argument of
  `CEventOption::CEventOption(EScopeType)` (three sites), of
  `CTriggeredEventDescription<CDescOption>::ReadEntry(CReader&, int, EScopeType)`, and of two
  virtual block reads: the trigger at `+0x508` (slot `+0x30`) and the effect at `+0x518` (slot
  `+0x10`). So the child blocks read in the kind's scope.

The 22 kinds and their scope constants, named by `NEventScope::GetScopeName`:

| Root key | Token | `EScopeType` | Scope |
| --- | --- | --- | --- |
| `event` | `0x1b8` | `0` | none ("scope not set") |
| `country_event` | `0x2c80` | bit 2 | country |
| `observer_event` | `0x3714` | bit 19 | country (the second country type; scope keyword `observer`) |
| `planet_event` | `0x2c81` | bit 1 | planet |
| `colony_event` | `0x2c82` | bit 40 | colony |
| `carrier_event` | `0x2c83` | bits 1 and 3 | planet or ship |
| `ship_event` | `0x2d4f` | bit 3 | ship |
| `pop_event` | `0x2d6b` | bit 4 | pop |
| `pop_group_event` | `0x2d6c` | bit 5 | pop_group |
| `fleet_event` | `0x2ef6` | bit 6 | fleet |
| `system_event` | `0x3eeb` | bit 7 | galactic_object |
| `leader_event` | `0x3eec` | bit 8 | leader |
| `bypass_event` | `0x422a` | bit 12 | bypass |
| `pop_faction_event` | `0x33f2` | bit 14 | pop_faction |
| `starbase_event` | `0x3eea` | bit 17 | starbase |
| `first_contact_event` | `0x3e58` | bit 23 | first_contact |
| `espionage_operation_event` | `0x294e` | bit 25 | espionage_operation |
| `agreement_event` | `0x3eee` | bit 28 | agreement |
| `situation_event` | `0x3eed` | bit 29 | situation |
| `astral_rift_event` | `0x4223` | bit 31 | astral_rift |
| `cosmic_storm_event` | `0x2991` | bit 33 | cosmic_storm |
| `cosmic_storm_influence_field_event` | `0x2992` | bit 34 | cosmic_storm_influence_field |

The 20 `*_event` effects on M452 are the typed kinds without `pop_event`. The config at
`8574760` has 21 kinds: no `pop_event`, and `carrier` as the push scope of `carrier_event`, which
the engine reads as planet or ship. No vanilla file in `events/` has a `pop_event` key.

**`namespace` gets no field answer (SDK-814).** A decided limit of the events answer. A
`namespace = x` root statement registers a `CNameSpace` in the manager's hash table and is not a
`CEvent` field, so `registry_fields("events")` does not list it. `loader_rules("events")` already
answers the statement: `RootKey(namespace)` with `DefinitionName::Value`, so the value is the
namespace's name. The sweep counts no Native gap for it, so `ACCEPTED_LIMITS` has no entry. Atlas's
`events/` exception "`event_namespace`" keeps its 3 claims (`type_existence`, `field_existence`
and `value_form` of `namespace`), because Atlas does not join that `loader_rules` answer to them.
Add the join in Atlas, and add a Native answer only if the join needs more than the definition
name, when Atlas needs `namespace` beyond these 3 claims.

### Event kinds

`registry_fields("events")` answers one root field per kind (SDK-702, decision D4). Each kind's
`read_scope` is the scope that the loader passes to `CEvent::CEvent(EScopeType)`: a known empty
list for `event` (scope `0`), one alternative with planet and ship for `carrier_event`. Its
members are the `CEvent` fields, the same for every kind, and a gap of a member is repeated
below each kind (`country_event.trigger`). `namespace` and `inline_script` are not fields. The
parity test `each_event_kind_is_a_root_field_read_in_the_scope_that_the_kind_selects` compares
`tests/expected/m452/event-kinds.json` with the table above. Atlas joins kind `K`'s read scope to
`type[event]/subtype[K]/$annotation:push_scope` through the kind's `type_key_filter`, and its
known empty list to `scopeless`.

- **The loader root.** The kind switch is in the file loader, not in an owner `ReadMember`, so the
  field input has a second root (`FieldInput::loader_root`). The binding takes the one function
  of the owner's database that calls `CReader::ReadSimpleStatement()` and an `(EScopeType)`
  constructor of the owner; the walk starts after the statement read, with the reader in the
  callee-saved register that the call's argument is copied from (`x23`). The token is the word at
  the recipe's `reader_token_offset`, so the existing compares and byte jump tables partition it.
- **The walk.** A call to the owner's constructor ends a path and joins with its arguments; `x1`
  is the kind's scope. Every other call clobbers `x0`–`x18` and the walk goes on: the ABI keeps
  `x19`–`x28`, and the case holds the scope in `w21`. A branch back to the statement read ends the
  path as rejected. A loader with none of these functions has no loader root, and its answer is
  unchanged.
- **Validated assumption.** As for every custom-loader owner, the built object is taken as the
  item that the loader reads; the walk ends at the constructor and does not follow the virtual
  read. The kind's reader identity is the owner's primary vtable point (`CPersistent::Read` with
  `CEvent::ReadMember`). Repeat behavior is not established: the duplicate rule belongs to
  `loader_rules`.
- **Known gaps.** The default case (`Corrupt Event Table Entry`) stops at a `movk`, and the
  `namespace` case at an `ldursb`, so the answer has one registry gap for a root path that the
  walk could not finish. The walk names every accepted kind regardless.

`registry_fields` over `events/` at `registry-fields/v33`: 22 kinds, each read in an established
scope; of the 30 `CEvent` fields, 7 are complete (`custom_gui`, `custom_gui_option`,
`notification_event_icon`, `diplomatic_title`, `event_picture_background`, `specimen`,
`event_message_type`), 4 are partial (`event_chain` with unresolved storage, `difficulty` and
`notification_event_icon_frame` with the scanner's conversion outside the method, and
`pre_triggers`, see [pre-trigger blocks](#pre-trigger-blocks)) and 19 have no established
reader. Their failure shapes, from `examples/inspect --registry-fields events`:

- the path reaches a call that is not a reader: `operator new` for an object held by a
  `shared_ptr` (`trigger`, `immediate`, `option`, `after`, `abort_trigger`, `abort_effect`,
  `picture_event_data`, `desc`); `make_shared<…>(EScopeType const&)` (`picture`, `show_sound`,
  `mean_time_to_happen`, `weight_multiplier`); `CEventTarget::CEventTarget(CToken)` (`situation`,
  `location`); `strlen` or `CString::CString` of the token text (`id`, `title`, `message_desc`,
  `base`); `CTriggeredEventDescription<…>::ReadEntry(CReader&, int, EScopeType)` (`desc`);
  `shared_ptr<…>::reset` (`desc_clear`);
- an unsupported instruction (`stur` to the frame, `cset`) on an alternative path and on 16
  unnamed token paths, and three unsupported branch conditions.

The block children take their scope from `CEvent + 0x68`, where the constructor stores the kind's
scope, but none of their readers joins, so none is `Enclosing` yet. When a child's reader joins
with scope `Load(Owner(0x68))`, it reads in the kind's scope once the constructor is shown to
store its argument there.

Pitfalls:

- A constructor of a nested class (`CEvent::SEventData::SEventData`) is a constructor of
  `CEvent::SEventData`. A class name taken before the first `::` misses it; the walk then does
  not enter it, and `pre_triggers` has no reader.
- The kind switch is in the file loader, not in an owner `ReadMember`. Each case reads no value;
  it sets a constant that a later constructor call receives. Read it from the loader root, not
  from the owner's member walk.
- The `*_event` effect declarations do not establish the loader's kinds: `pop_event` has a kind
  and no effect, and `event` has neither an effect nor a scope.
- A token number is per build. Name tokens from `GetTokenArray()` (the constant in `w1` after
  each name string), and scope bits from `NEventScope::GetScopeName`.

## Embedded object keys

A root field that reads one embedded persistent object on every token path that holds its token,
including a wider path, where every owner
constructor installs the same reader there and that reader has no block family, gets the keys of
the object's own `ReadMember` as `FieldMembers::Fields`, to depth one (`fields/persistent.rs`,
`embedded`). The binding adds the member reader of each such reader that the owner's constructors
can install to the field input. A walk that names no key leaves the members `Unresolved`. The keys take the read-scope,
scoped-operand and duration passes that a collection's fields take. A gap of
the object's walk as a whole names the parent field; a gap of one key names `parent.key`. The same
rule renames the whole-reader gaps of a collection's object, which earlier carried the registry
name after the dot.

On M452 (`registry-fields/v31`), 17 fields in 11 registries get keys this way, 131 keys in all.
Three walks name no key: `common/anomalies` `on_success` and
`common/country_customization` `graphical_culture_for_random_empire` keep `Unresolved` members.

Pitfalls:

- The constructor join must succeed first. `CJobType`'s constructor used `mvni` (vector move of
  an inverted immediate), which the evaluator did not run, so no `common/pop_jobs` field joined a
  persistent reader. `mvni` is now evaluated like `movi`; `common/pop_jobs` and
  `common/pop_categories` gained their modifier and weight readers with it.
- A key whose value goes to a class helper that the dispatch walk does not follow keeps an
  `UnresolvedReader` gap. The key is still established: its path is a singleton.

### Pre-trigger blocks

`common/pop_jobs` `possible_pre_triggers` and `common/pop_faction_types` `can_join_pre_triggers`
read one embedded `CPreTriggers` object. Both fields have reader identity `87346602eebc8fd3`
(`CPersistent::Read`, `CPreTriggers::ReadMember`), so the blocks share one key table.
Every event kind reads the same object: `CEvent::ReadMember` sends token `pre_triggers` to
`CReader::Read(CPersistent&)` on `CEvent + 0x4e0`, so all 22 kinds accept the block, not only the
config's three (`colony`, `carrier` and `country` subtypes in `events.cwt`). Each
`<kind>.pre_triggers` has the same reader identity and the same 14 keys.
`tests/expected/m452/pre-triggers.json` holds the keys of the two registry fields and of the
config's three kinds; `cargo parity pre_trigger` checks them, and
`every_event_kind_reads_the_pre_trigger_table` checks all 22 kinds.

Facts on M452:

- `CPreTriggers::ReadMember` (`0x1004d0168`) maps 14 tokens to `EPreTriggers` values 0 to 13 and
  tail-calls `CPreTriggers::ReadValue` (`0x1004d0008`); any other token goes to
  `CPersistent::ReadMember`. `TokenToEnum<EPreTriggers>` (`0x1004cfe3c`) has the same 14 cases.
  The keys: `has_ground_combat`, `has_owner`, `has_planet`, `is_ai`, `is_being_assimilated`,
  `is_being_purged`, `is_capital`, `is_enslaved`, `is_homeworld`, `is_idle`, `is_occupied_flag`,
  `is_robotic`, `is_sapient`, `original_owner`.
- `ReadValue` compares the id of the reader's value token (`+0x278`) with `0x3ff8` (`yes`) and
  `0x2cac` (`no`) directly, not through `CReader::Read(bool&)`. `GetTokenArray()` names them at
  `0x100dd26f8` and `0x100db0b1c`; they are the recipe's `boolean_tokens`. `yes` sets the key's
  bit in the required-true mask at `+0x8`, `no` in the required-false mask at `+0xa`. Any other
  value is ignored silently: no store and no diagnostic, where `CReader::Read(bool&)` reports
  `Malformed token`. A key whose bit is already set in either mask logs
  `Pre-trigger "%s" is specified more than once` and still sets the bit, so `yes` then `no`
  sets both masks.
- The config splits the keys into colony, pop and country sets (13 names; no `is_idle`). The
  reader has one table and no scope split. The `PreCalc*Value` functions that fill the values per
  scope object are use-time code; which keys each one sets is not established.
- The blocks' read scope is the [read-scope](#block-entry-contexts) answer (SDK-549); it stays
  unresolved with a `read-scope: scope-argument` gap.
- `CEvent::CEvent(EScopeType)` builds its data in the nested constructor
  `CEvent::SEventData::SEventData(EScopeType)` at `this + 0x68`, which stores the `CPreTriggers`
  vtable point at its `+0x478`. The owner constructor walk enters a nested class's constructor
  like any other called constructor, and takes reader candidates from the vtable pages of the
  entered bodies as well as the owner's.

Each of the two fields and each of the 22 event `pre_triggers` fields is listed with all 14 keys,
and every key is `Boolean` with repeat `Merges` (`registry-fields/v34`, SDK-807). The rule is
stated, not derived: `readers::PRE_TRIGGER_VALUE` classifies a tail call to
`CPreTriggers::ReadValue(CReader&, EPreTriggers)` with the owner in `x0` and the reader in `x1`,
like the stated `CReader::Read(bool&)` rule. The key has no one destination, because the value
goes to one of two masks. A repeat keeps the earlier bit, so `Merges`, not `Replace`. `Boolean`
fits because the values are the Boolean pair; the `Keyword` rule of
[weight blocks](weight-blocks.md) is for an engine keyword set. This removed 672 key gaps
(`UnresolvedReader` and `UnresolvedStorage`); each block keeps its own
`read-scope: scope-argument` and repeat gaps. Derive the rule from the body (run the helper under `yes`, `no`
and another token, as command grammar forms do) when a second helper of this shape appears, or
when `ReadValue` changes on a new build.

## Block families

Fields and their conditional read alternatives carry conservative block families. Generic
persistent destinations are joined to constructor-installed virtual readers; a shared
`CPersistent::Read` call alone does not give a concrete identity. Command block families are on
[nested command grammar](command-grammar.md).

## Block entry contexts

`Field.entry_contexts` gives, for each root trigger, effect and weight block, the scopes that the
engine's evaluation calls supply for `this`, `root`, the `from` chain and the `prev` chain
(`registry-fields/v13`; `callbacks/blocks.rs`, `callbacks/climb.rs`, `callbacks/contexts.rs`,
`src/binding/binary/callbacks.rs`, `src/binding/binary/type_pointers.rs`,
`src/session/field_entries.rs`). The answer keeps `EntryScope::SelfLink`; read it by the [self-link rule](engine-commands.md#on_actions-game-rules-and-their-entry-scopes).
`this` is the scope at the call; the scope that the engine reads the block in is SDK-549's.

**How the engine evaluates a stored block.** A registry item evaluates its own blocks in its
methods: it passes `this` plus the block's storage offset to a trigger evaluator, an effect
executor or a weight evaluator (`CTrigger::Evaluate`, `EvaluateExtended`, `CEffect::Execute`,
`ExecuteExtended`, `CRootEffect::Execute`, and, called directly, `CAndTrigger::ActualEvaluate`,
`SafeExecuteEffect` and `SafeExecuteEffectExtended`; the weight evaluators are on
[weight blocks](weight-blocks.md#entry-contexts)), with a scope in `x1`. The method either builds the scope itself
(`CTraditionType::IsPotential(CCountry const*)`: a fresh scope, `SetCountry`, then `this + 0x108`)
or takes it from its caller (`CCouncilAgenda::IsPotential(CEventScope&, CString*)` evaluates
`this + 0x1c8` with its scope parameter; `CGovernment::UpdateCouncilAgenda` builds that scope).
The field method's storage offset is the offset from the method's `this`. Another class's method
can evaluate the block through the block object's own vtable, on a pointer to the item that it
received or keeps: `CCountry::AddEdict(CEdict const*)` runs `[[x1 + 0x2e0]] + 0x48` (`Execute`)
on `x1 + 0x2e0`, and `CMission::Start` loads its mission type from `this + 0x18` and runs slot
`+0x48` of the effect at `+0x190`. Some direct evaluator calls are in another class too
(`CTechnologyStatus::GetTechWeight(CTechnology const*)` evaluates `x1` plus an offset).

**The method.** A *type pointer* is a register that points at an owner's item at entry: the
receiver of a member of the owner, by the function's exact qualifier (a member of
`CAnomalyType::COutcomeEffect` is not a member of `CAnomalyType`); a parameter of type `O const*`,
`O*`, `O const&` or `O&`; or the word at offset `k` of an object of class `C`, when some
constructor of `C` stores there a parameter of one of those types, no constructor stores a
parameter of another type there, and every constructor of `C` decodes (`CMission` `+0x18`,
`CResolution` `+0x18`). A parameter's register is known up to the first parameter that one
general register may not pass (pointers, references, integers, `bool` and `TPdxRef<…>` take one);
an enumeration, which a demangled name does not tell from a class, ends them, and a function whose
qualifier is not a known class has none. A class is a qualifier with a vtable, a type info, a
constructor, a destructor or a `const` member function: `CSubjectSpecialization` has only `const`
members. The candidate calls are the direct calls to an evaluator or a tooltip builder and every
`blr`, in the functions that have a type pointer. The name pass names a load from an argument by
its chain of loads, three loads deep (`Member`, `Deref`, `DerefAt`), with or without pre-index
writeback (`ldr x8,[x0,#0x40]!` loads the vtable of the object at `x0 + 0x40` and leaves that
address in `x0`), and attributes a call to
`(owner, offset, family)` only when `x0` holds one type pointer plus one offset other than zero on
every path. A `blr` must also call a slot of the vtable at that same address whose displacement is
an evaluation slot. The binding derives the slots from the vtables: each slot of an evaluator
class's primary vtable that holds the evaluator, by its displacement (M452: trigger `+0x10` and
`+0x18` `Evaluate`, `+0x20` `ActualEvaluate`; effect `+0x48` `Execute`). The family is the
evaluator's or the slot's; a field joins only a block of its reader family, so a trigger field
does not join a call through effect slot `+0x48`, and an effect's `Read` at `+0x10` joins no
effect field. Every owner has a vtable, so the word at offset zero of its item is the item's
vtable pointer and holds no block. Callers and receivers are decoded only for the functions with
an attributed call. A function whose scope is its own parameter is a *wrapper*; the method
goes up its direct callers, at most 2, to the call that builds the scope. From that call it runs
the caller to the call and through it, and runs every wrapper and every function that receives a
scope, up to 6 calls deep, inline on the path with the caller's arguments and memory. Before the
selected call, evaluations act only through their effects. A call through a vtable slot is read on
arrival and then applied as a call that the pass does not follow: the pass does not know the
block's family there, and the same slot of another family's block is not an evaluation. A scope
that one virtual evaluation receives is therefore unknown to the later ones on the path. A call
reads the argument registers that its demangled signature uses; a call to a C library function, direct to its stub or through an
import pointer, reads those that the C or POSIX signature uses (`LIBRARY_ARGUMENTS`: `_strlen`
one, `_memmove` three, the stack probe `___chkstk_darwin` none) and not `x8`; a call to a
function that never returns ends the path. A copy of a
scope pointer in the stack reaches a call only in the frame of a stack address that the call
receives, at or above that address, and not in a register save: a store of `x19` to `x30` to the
stack in a function's prologue, before its first other instruction, that no later store has
overwritten. A virtual call on the object that a proven instance pointer holds calls the slot of
that object's vtable, so it reads the registers that the slot's signature uses. A call receives
`x8` only when its target may read it ([engine commands](engine-commands.md#pitfalls)), and a
call that fires an on_action or evaluates a game rule is a scope reader
([engine commands](engine-commands.md#on_actions-game-rules-and-their-entry-scopes)), whose
method shares this caller climb (`callbacks/climb.rs`). A copy
constructor of a scope that the pass can read, `CopyInternalScopes` and a function that builds a
scope in the object that `x8` addresses are followed as the engine runs them (same page); a copy
into an object that the pass does not know lets its source escape.

**Offset getters, helpers and tooltip calls** (SDK-732 part 2). An *offset getter* is a function
whose whole body is `add x0, x0, #k; ret`, such as
`CSpecialistSubjectType::GetOnProgressCompleteEffect` (`k = 0xa0`). The binding lists the getters
that the decoded functions call; the name pass gives a call to one the result `x0 + k`, so the
evaluation that follows names one block. A *helper* is a
function whose evaluation's block is its own parameter: `x0` is `Argument(n, 0)` for a parameter
`n` that leads to no owner, such as `CMission::Stop(CRootEffect const&, EMissionStatus)`, which
runs slot `+0x48` of `x1` on a scope that it builds. At each direct caller the name pass names the
block in register `n` as at a site, and that call becomes an entry that runs through the helper.
Only the contexts that the run reads at the helper's own evaluation go to that caller's block; the
run reads no other evaluation, and other runs do not enter the helper. The method goes one caller
up: a caller that passes its own parameter on (`block-caller-depth`), a caller whose register names
no block (`unattributed`) and a helper with no caller (`no-caller`) give no block a context and are
counted for the helper in `BlockEntries::helpers`, for Native's developers; a helper whose scope
is a parameter too charges the caller's block `block-and-scope-from-caller`. A helper must have a
type pointer, as every function with a site must, because the candidates are the functions with
one; give helpers a candidate rule of their own if an Atlas-needed block is passed to a helper
outside that set. A *tooltip call* is a direct call to a `BuildToolTip(CEventScope&, bool, int,
CSimpleBitMask<…>) const` of any class, or a call through the tooltip slot, which the binding
derives from the `CTrigger` and `CAndTrigger` vtables as it derives the evaluation slots (M452:
`+0x58`). It names a block of the trigger family but evaluates nothing. A block that a tooltip call
names and no attributed evaluation does gets the gap "only tooltip calls name this block; the
method established no evaluation of it".

On M452 the binding finds 27 offset getters that the decoded functions call. 49 functions are
helpers; 17 calls to them name a block: 12 calls to `CMission::Stop` and 5 to
`CScriptedActionInfo::IsClickable` and `TestScopeDependentTrigger` (whose blocks join no root
field). The others count 446 `unattributed` caller evaluations, 64 `block-caller-depth` and 8
`no-caller`: most are virtual calls at an evaluation slot's displacement on a parameter that is
not a block (`CPdxArray<…>::InsertAt`, `CCountryEthos::ShiftTowardsEthic`), whose callers pass no
owner item plus an offset.

**Instance pointers** (`callbacks/instances.rs`, `src/binding/binary/instances.rs`). A global word
that holds one object's address, such as `TPdxNullObject<CTraditionSwap>::_pInstance`, is set at
run time (`Allocate` stores the address that `ProtectedMemoryAccessBuffer` returns), so the image
does not hold the object. One forward pass over a function's branches (`register_flow`) keeps
what each register may hold: a slot's page, the pointer's address, its object, the object's
vtable and a vtable slot. The binding takes the pointer slots through which a virtual call in the
decoded functions may load its receiver, and every function that loads such a slot and has an
`adrp` of a page that holds a vtable. The same pass finds the ones that may write through the
slot: a store whose base or stored register may hold the pointer's address or its object, or a
call that may receive the pointer's address. Each of those runs with the
pointer holding a scratch object, calls not followed. The pointer is proven when every path of
every such writer returns, leaves the object in place or clears the pointer (the destructor), and
either leaves its first word unwritten or stores one vtable address point there, and no call
receives the object or the pointer after that store; a path that stops at a search bound, or a
writer that does not decode, rejects it. The method then places the object in the read-only data
with only that word known. On M452 the block input proves one pointer,
`TPdxNullObject<CTraditionSwap>::_pInstance`. The null ship's pointer is not proven: 66 functions
may store it and their searches stop at the path limit. The proof runs about 2,900 writers and
adds about 1.4 s to the block input; narrow the writer test if that grows. It serves one call
site on M452, `ExecuteTradition` → `CTraditionType::GetUnlocksAgenda` (council agenda `potential`
and `allow`), and stays because no other rule reads that virtual call's registers. None of the
SDK-712 shapes goes through an instance pointer, so it was not extended; prove more pointers only
with a plan review, because a deeper search or a weaker writer rule changes a stated soundness
rule.

**Assumptions.** Beside the self-link rule:

- Every function that writes a member word that leads to an owner keeps the owner's layout: it
  stores an item of that owner, its null object, or null. Checked by hand on M452 for `CMission`
  (its constructors, and `ReadMember`, which stores a lookup result or
  `TPdxNullObject<CMissionType>::_pInstance` at `+0x18`), `CResolution` (the constructor from a
  `CResolutionType const&` stores it at `+0x18`; the default constructor stores
  `TPdxNullObject<CResolutionType>::_pInstance`; `ReadMember` stores the result of
  `ReadKeyReference<CResolutionTypeDatabase>`) and `CCosmicStorm` (`+0x328`: the constructor
  from a `CCosmicStormType const*`, the default constructor, which passes it the null object, and
  `ReadMember`, which stores a lookup result or `TPdxNullObject<CCosmicStormType>::_pInstance`).
  Remove the assumption with a writer check that covers every function that stores the word.
- A demangled name does not say whether a member function is static; a function of a class is
  read as taking its receiver in `x0`, as the owner test did before SDK-732.
- [Evaluation leaves a scope as it found it](engine-commands.md#on_actions-game-rules-and-their-entry-scopes):
  trigger, effect and weight code that receives a scope is a scope reader.
- A bounded search with no contradiction: when the paths of a call stop only at the path, loop or
  step limit, and every evaluation that the followed paths reach receives a scope that the method
  can read, the found contexts stand with no gap. A path that reaches an evaluation with an
  unreadable scope is a contradiction; then the bounds are gaps too.
- A prologue's register save is not memory that a callee can reach through a lower stack address
  that it receives. Checked by hand at three of the eleven call sites that the rule changes:
  `CDiplomaticActionType::IsPotential(CEventScope const&, CString*)` saves the caller's `x23`, the
  from scope, at `sp+0x88`; `CSystemType::IsPotential` saves `x22`, the from country, at `sp+0x60`;
  `CColonyType::IsPotential` saves `x20`, the from country, at `sp+0x90`. Each then passes a local
  string below the saves to `CString::CString` on the script profiler's path.
- The object that an instance pointer holds gets its vtable only in code that loads the pointer
  and forms an address on a vtable's page, keeps that vtable once set (a call on it, such as
  `IsValid`, does not change it), and is never written through an unknown address or through a
  copy of the pointer in memory. Checked by hand for `TPdxNullObject<CTraditionSwap>`,
  `<CCouncilAgenda>` and `<CShip>`: `Initialize` clears the object, runs its base constructor and
  then stores `vtable for TPdxNullObject<T>` + 0x10 at word 0; the calls after it only change
  memory protection, and the destructor stores null in the pointer.

**Result on M452.** The field sweep's `entry_contexts` section gives these counts
([method authoring](method-authoring.md#run-over-the-whole-population)). 312 root trigger,
effect and weight blocks in 88 of the 164 registries. Of the 239 trigger and effect blocks, 154
have contexts and no entry gap, 54 have contexts and a gap, and 31 have none (SDK-735;
154, 50 and 35 before); 26 keep several
contexts with a known `this`, 66 name a typed `from` and 4 a typed `prev`. The 73 weight blocks
are on [weight blocks](weight-blocks.md#entry-contexts). 6 registries have 7 direct
evaluation calls in the owner's methods whose block the method cannot name. SDK-726's two rules
changed 10 blocks in 6 registries, each only removing an unresolved context and its gaps: the
register saves those of astral action `potential` and `is_exhausted`,
colony type, observation mission and system type `potential`, and diplomatic action `potential`,
`possible` and `proposable`; the instance pointer those of council agenda `potential` and
`allow`. SDK-712 changed 15 blocks in 9 registries, and each change only adds a context, removes
an unresolved one or narrows a gap: the new direct evaluators add armies `potential` and
`potential_country`, megastructure `outliner_trigger`, `dismantle_potential` and
`should_ai_dismantle`, and button effect `effect` (an unreadable context, from
`CButtonEffect::ExecuteEffect`); `x8` and the firing readers resolve megastructure
`on_build_start` and `on_build_complete`; `ands` and `bics` open pop faction `can_join_faction`,
pop job `possible` and the buildings `on_queued` paths, and situation `on_abort` gains a context.
SDK-729 changed 3 blocks: ship size limit `show` gains a country context, which a stale value hid
(no `replace_scopes`); the path to ai budget `potential` and to megastructure `on_build_complete`'s
context with a starbase `prev` now passes the path limit. The budget keeps its bound gaps; the
megastructure keeps its other, readable context, so by the bounded-search assumption below its
bound is not a gap.
SDK-730, which follows copies and scope factories
([engine commands](engine-commands.md#on_actions-game-rules-and-their-entry-scopes)), changed the
8 psionic aura blocks: each gains a galactic object context with a country from, from the scope
that `CPsionicAura::InitAuraScope` returns. The config agrees on the stated keys
(`this = galactic_object`) and omits the from. SDK-734, which gives C library stubs their argument
registers, changed 4 blocks. Psionic aura `on_gain_level` and `on_lose_level`
(`CIntensityLevel::OnEnter`, which the lambda in `CPsionicAura::UpdateIntensityLevel` calls first)
and casus belli `on_proxy_war_start` (`CCasusBelliType::OnProxyWarStart`, `0x100102d18`) each call
`_strlen` on the script profiler's path while `x1` still holds the scope; each loses an unreadable
context and its gaps. Ai budget `potential` gains a country context with no from:
`CCountryAI::UpdateUpkeepBudget` calls `_bzero` with a stack address in `x8` (`mov x8,sp`,
`0x100e00c38`), which let the frame that holds the scope escape. It keeps the bound gaps of
`UpdateExpenditureBudget`, which reaches no evaluation.
Starbase buildings and modules each gain one unnamed evaluation: `GetEquippedComponents` evaluates
a trigger in an element of a component list, not `this` plus an offset.
SDK-731, which shares the caller climb with on_actions and game rules, makes each run read only
the evaluations of the blocks that its entry carries, and changed 3 blocks. Casus belli `is_valid`
loses an unreadable context and its gap: a run that carries another block read its evaluation
with a scope that the run's entry did not build. Artifact action and astral action `potential` gain a
`path-limit` gap: `IsAllowed` passes its scope on to `IsPotential`, and the runs from its callers
that carry `potential` reach no `potential` evaluation before the path limit; the `allow`
evaluation that those runs reach had hidden the bound. SDK-733, which makes the weight
evaluators block evaluators, changed one trigger block: planet modifier `potential` gains a planet
context and loses its path-limit gap. Its run starts at
`CPlanetModifierDatabase::GetRandomModifier`, which builds the planet scope and calls
`CPlanetModifier::GetSpawnChance(CEventScope&)`, which calls `IsPotential`. Before, the run entered
`GetRawFactor` as a scope receiver, and its modifier loop used up the path limit before a path
reached the evaluation in `IsPotential`. Anomalies and civics gain unnamed weight evaluations
([weight blocks](weight-blocks.md#entry-contexts)).
SDK-732 part 1, which attributes evaluations through the block's own vtable and direct evaluator
calls in any function with a type pointer, changed 28 blocks in 13 registries; no block lost a
context. 25 trigger and effect blocks and 2 weight blocks (technology `ai_weight` and
`weight_modifier`) gain contexts where they had none: archaeological site `on_create`,
`on_roll_failed` and `on_visible`, armies `allow`, artifact action and astral action `effect`, dust
cloud `condition`, `on_system_added` and `on_system_removed`, edict `effect` and `on_disabled`,
espionage operation `on_create` and `on_roll_failed`, mission `on_daily`, `on_monthly`, `on_start`
and `on_stop`, relic `possible` and `active_effect`, resolution `effect` and `fail_effects`, and
storm `on_start`, `on_monthly`, `on_moved` and `on_finished`. Seven of them have only an unreadable
`from` or context (missions, through `CSpatialObjectRefCaster::FillEventScope`'s jump table;
espionage operations; archaeological site `on_visible`). Buildings `potential` gains a second,
unreadable context: `CColony::CanAddBuildingType(CEventScope const&, CBuildingType const&, CString*)`
evaluates `x2 + 0x108` through slot `+0x18` with its scope parameter, and both of its callers stop
at the path limit. Anomalies lose their 2 unnamed evaluations (the exact receiver rule). These
call sites were checked by hand in the disassembly:

- armies `potential`: `CArmyType::IsPotentialTrigger` builds a colony scope at `sp+0x170`, sets a
  second scope's type to species (`Set(0x800, …)`), links it as from (`str x21,[sp,#0x1a8]`) and
  passes `this + 0x4f8` to `CAndTrigger::ActualEvaluate`.
- council agenda `potential` and `allow`: `CGovernment::UpdateCouncilAgenda` builds a country scope
  and calls `IsPotential` and then `IsAllowed` with it (`this + 0x1c8`, `this + 0x280`).
  `CAIInteriorMinister::HandleCouncilAgenda` passes its country scope through
  `SelectByWeightedRandom<CCouncilAgenda>`, which keeps it in `x20`. `ExecuteTradition` passes its
  country scope to `CTraditionType::GetUnlocksAgenda`, which, for a tradition with no swaps, calls
  slot `+0x40` of the null tradition swap (`TPdxNullObject<CTraditionSwap>::IsValid() const`) while
  `x1` still holds the scope.
- council agenda `init_effect`: `CGovernment::SetCouncilAgenda` builds a country scope;
  `ExecuteInitialEffect` tail-calls `CEffect::Execute` on `this + 0x580`.
- tradition `potential`: `CTraditionType::IsPotential` builds its own country scope.
- diplomatic action `on_accept`: `CDiplomaticActionType::OnAccept` builds two country scopes, links
  the second as the first's from and runs `this + 0x2f8` with the first.
- tradition and ascension perk `on_enabled` and `on_disabled` (SDK-735): both registries use
  `CTraditionType`. `OnEnabled` (`0x100ce3648`) builds a country scope at `sp + 0x60`, selects
  the swap's block `+0x378` or its own `+0x418` at `0x100ce3748`, then calls slot `+0x48` of
  that same selected object at `0x100ce3848`. `OnDisabled` (`0x100ce3a80`) builds the same scope,
  selects the swap's `+0x420` or its own `+0x4c0` at `0x100ce3b7c`, and calls the slot at
  `0x100ce3c7c`. Each owner's block gains a country with self-links. The indexed swap load is
  still unknown; those arrivals retain `selected-block-identity`, which also keeps the path-limit
  gap. The two `on_enabled` contexts agree with the config's country `this` and `root`.
- edict `effect`: `CCountry::AddEdict(CEdict const*)` (`0x10028d49c`) keeps the edict in `x21`,
  builds a scope at `x19 + 0x60` (`CEventScope(int)`, `SetCountry` of `this`) and calls
  `[[x21 + 0x2e0]] + 0x48` with `x0 = x21 + 0x2e0`: a country with self-links. `RemoveEdict` runs
  `on_disabled` at `+0x388` the same way.
- resolution `effect`: `CGalacticCommunity::PassResolution(CResolution&, bool, bool)`
  (`0x10055ce08`) keeps the resolution type `[x1 + 0x18]` in `x23` across its calls and calls
  `[[x23 + 0x3a8]] + 0x48` with `x1 = sp + 0x358`. When bit 0 of the type's byte `+0x168` is set
  (a targeted resolution), that scope is the target country (`GetTarget`) with the resolution's
  country at `sp + 0x78` as its from (`str x8,[sp,#0x390]`); otherwise it is the resolution's
  country with no from. `FailResolution` runs `fail_effects` at `+0x450` the same way.
- mission `on_start`: `CMission::Start` (`0x1009531d4`) builds four scopes, fills them with
  `BuildEffectScopeForOperator`, and calls `[[[this + 0x18] + 0x190]] + 0x48` with
  `x1 = sp + 0x450`. The country `this` is read; the from goes through
  `CSpatialObjectRefCaster::FillEventScope`, a jump table on a run-time type, so it stays
  unresolved, and a path on which the operator sets no country gives `this` no type.

SDK-732 part 2 (offset getters, helpers, tooltip calls, pre-index loads and the `const` member class
rule) changed 6 blocks in 3 registries; no block lost a context and no other answer changed. Read
by hand:

- specialist subject type `on_progress_complete` gains an agreement with self-links:
  `CSubjectSpecialization::FinishConversion(CSpecialistSubjectType const&, CAgreement const&)`
  (`0x100c6f8a0`) builds a scope at `sp + 0x30` (`CEventScope(int)`, `SetAgreement(x20)`), calls the
  getter `GetOnProgressCompleteEffect` (`add x0,x0,#0xa0; ret`) on `x1`, and calls slot `+0x48` of
  the result with `x1 = sp + 0x30`. Its class has no vtable or constructor symbol; the `const`
  member rule makes `x1` a type pointer.
- mission `on_success`, `on_fail` and `on_cancel` gain one unreadable context each (`this` of no
  type, an unresolved from) with path-limit and branch-value gaps: `CMission::Succeed`, `Fail`,
  `Abort`, `TimeOut`, `ForceStop`, `AbortIfNeeded` and the two daily updates pass
  `[this + 0x18] + 0x388` (`on_success`) or `+ 0x430` (`on_fail`, `on_cancel`) in `x1` to the
  helper `CMission::Stop`, which builds its scope with `BuildEffectScopeForOperator` and calls slot
  `+0x48` of `x1` at `Stop+0x150`. From the callers, the paths that set a country stop at the path
  limit, and the from goes through `FillEventScope`'s jump table.
- mission `on_issue` gains a country with an unresolved from:
  `CContractManager::IssueContract(CCountry&, CMission&)` builds a country scope from `x1`, loads
  the mission type `[x2 + 0x18]` and calls slot `+0x48` of it plus `0x40` through
  `ldr x8,[x0,#0x40]!`; the from goes through `FillEventScope`.
- decision `custom_tooltip` has the tooltip gap: `CDecision::GetToolTip(CToolTip&,
  CColonyCarrier const&, bool) const` calls `CCustomTooltipTrigger::BuildToolTip` on
  `this + 0x198`, and no evaluation names the block.

**Comparison with the config's `replace_scopes`** (M451-hotfix; the SDK-726 changes above remove
only unresolved contexts, and of the SDK-712 changes only armies `potential` disagrees). Read through the self-link rule, comparing the
keys that the config states (`system` is the engine's `galactic_object`; `any` matches any scope):
107 blocks have a readable context and a config expectation. 82 agree, 5 agree on the stated keys
where the engine also sets a first link that the config omits, and 20 disagree. 103 blocks have no
readable context and 27 have no `replace_scopes`. Each disagreement was read by hand:

- **Source errors (10).** The engine agrees with Native. Armies `potential` (SDK-712, M452) is an
  eleventh: the config gives a country, but `CArmyType::IsPotentialTrigger` evaluates it on a
  colony with a species from. Armies `on_built`, `on_queued` and
  `on_unqueued` run on a colony with no from (`CArmyType::OnBuilt`: `SetColony`), not a planet with a
  species from. Starbase building and module `abort_trigger` and `destroy_trigger` run on a starbase
  with no from (`ShouldAbort`, `ShouldDestroy`: `SetStarbase`). Overclock `potential` and `possible`
  run on the megastructure (`COverclockType::IsPotential`: `SetMegaStructure`), not a country.
  Megastructure `on_cycle_complete` runs on the megastructure with no links (`OnCycleComplete`).
- **The config states one of several contexts (9).** Bypass `potential` and `country_can_use` link
  the second country as from only when it is set (`CBypassType::CanPotentiallyBeUsedByCountry`).
  Megastructure `on_build_queued` and `on_build_unqueued` link the fleet as fromfrom only when it is
  set (`OnBuildQueued`). Colony type `potential` also runs on a planet from the surface view
  (`CSurfaceView::RefreshDesignations`). Observation mission `potential` and `valid` also run from
  the order button with a from of no type. Button effect `potential` and `allow` also run from
  `CEffectButton::PerFrameUpdate` on a scope with no type.
- **A Native limit (1).** Deposit `can_be_cleared_potential` is filled by
  `CDepositHolderRefCaster::FillEventScope`, whose type is a run-time value: the followed fallback path
  has no type and the typed paths stop at a jump table, which keeps a gap.

The five omissions name a from or prev that the engine sets and the config leaves out: bombardment
stance `trigger` (from planet on one path), casus belli `potential` (from country), diplomatic action
`possible` (prev country, from the three-country `IsPossible`), pop faction `on_destroy` (from pop
faction) and system type `potential` (from country). Per-name conclusions go to the Atlas ledger
(SDK-704).

The 27 blocks that SDK-732 part 1 gives contexts (M452, same rules): 9 agree (archaeological site
`on_create` and `on_roll_failed`, artifact action `effect`, edict `effect` and `on_disabled`, relic
`possible` and `active_effect`, technology `ai_weight` and `weight_modifier`), 3 disagree, 8 have no
`replace_scopes` (astral action `effect`, the three dust cloud blocks, the four storm blocks), and 7
have no readable context. The disagreements, read by hand:

- Resolution `effect` and `fail_effects`: the config states the targeted resolution's context
  (`from = country`); an untargeted resolution has no from. The config states one of several
  contexts.
- Armies `allow`: `CCountry::CalcIsArmyTypeAllowedOnAnyColony(CArmyType const&)` sets each colony
  (`Set(1 << 40, …)`) on a fresh scope and calls slot `+0x20` of `x1 + 0x440`: a colony with no
  from. The config gives a planet with a species from, as for armies `potential`. A source error,
  unless another evaluation that the method does not find supplies the config's context.

Of the SDK-732 part 2 changes (M452, same rules), 1 block has a readable context and it disagrees;
the 4 mission blocks have no readable context. Specialist subject type `on_progress_complete`: the
config gives `this = country`, but `FinishConversion` runs it on the agreement (above). The vanilla
blocks are empty, so no script shows the intended scope; read it as a source error.

**Conditional selections (SDK-735).** Against `30574fe`, the full sweep changes only these four
blocks in two registries; no existing context or other answer changes. Across all 312 blocks,
187 have contexts without an entry gap, 57 have contexts with a gap, and 68 have no context.
The registry totals remain 8 complete, 156 partial and 0 failed. Unknown receiver memory is
reserved only for the selection fallback, so existing singleton runs retain their former memory
behavior. A test confirms that storing a scope pointer in this receiver still lets it escape at
an opaque call.

The fallback covers positive receiver offsets below 1 MiB, with at most 16 candidate offsets per
register, and a direct evaluator or a locally proved same-receiver virtual slot. Receiver spills,
array elements and caller-supplied scopes need further origin tracking. Keep these limits until
an Atlas-required block needs those shapes; increasing bounds alone cannot establish an origin.
The local dispatch proof must discard a vtable identity after a 32-bit write: `mov w8,w8`
truncates the pointer even though the register number stays the same.

**Gaps.**

- Four effect blocks retain `selected-block-identity` and `path-limit`: the swap paths above do
  not establish which block they evaluate. A readable owner context does not erase this uncertainty.

- 21 trigger and effect blocks have no attributed evaluation and 1 is named only by tooltip calls
  (25 before SDK-735). The 35 weight blocks with this gap are on
  [weight blocks](weight-blocks.md#entry-contexts). By group, read by hand on M452:
  - *Offset getters.* Specialist subject type `on_progress_complete` resolves (above). Agreement
    term value `activate_effect` and `deactivate_effect` keep the gap:
    `CAgreementManager::CreateAgreement` calls `GetActivateEffect` on each element of the term
    data's term arrays (`ldr x0,[x22]`), and `CAgreement::SetTermData` and `DestroyAgreement` call
    the getters on `[x23, #8]`, array elements that no type pointer leads to. Specialist subject
    perk `activate_effect` and `deactivate_effect` have no getter:
    `CSpecialistSubjectLevel::OnLevelUp` and `DeactivatePerks` call slot `+0x48` of each perk of the
    level's perk array through `ldr x8,[x0,#0x40]!` and `#0xe8]!` on an agreement scope.
  - *Helper.* Mission `on_success`, `on_fail` and `on_cancel` gain an unreadable context (above).
  - *Members that no type pointer leads to.* First contact `on_roll_failed`:
    `CFirstContact::HandleRollFailed(int)` calls slot `+0x48` of `[this + 0x38] + 0x1d0`, and no
    `CFirstContact` constructor fills `+0x38` with a stage parameter. Pop faction `on_create`:
    `CPopFaction::Initialize(CCountry const&, CPopFactionType const&)` calls slot `+0x48` of the
    type that it stored at `this + 0x18`, plus `0x6c0`; `valid`: `CPopFaction::IsAlive() const`
    calls slot `+0x10` of `[this + 0x18] + 0x608`. `Initialize` is not a constructor, so the member
    rule does not establish `+0x18`. Crisis level `on_unlock`:
    `CCrisisProgression::UnlockLevel(CCrisisLevelType const*, CCountry*)` spills its parameter
    (`stur x1,[x29,#-0x60]`), and the name pass forgets the spill at a call that receives a lower
    stack address.
  - *Arrays and lookups.* Menace perk `on_unlock`: `UnlockLevel` calls slot `+0x48` of
    `x28 + 0x190`, where `x28 = [x26]` is an element of the level's perk list. Tradable action
    `on_traded_effect`, `on_deal_ended_sender_effect` and `on_deal_ended_recipient_effect`:
    `TradeActions` and `EndActions` (anonymous namespace) call slot `+0x48` of an element of a
    `CPdxArray<CTradableAction const*>` parameter plus `0x268`, `0x310` or `0x3b8`. Mission
    `on_accept`: `CContractManager::PickupContract(TPdxRef<CCountry>, TPdxRef<CMission>)` calls
    slot `+0x48` through `ldr x8,[x0,#0xe8]!` on a mission that it looks up from a `TPdxRef`.
    Portrait sprite configuration `trigger`:
    `CPortraitSpriteType::EvaluateConfigurationIndex(CEventScope&) const` calls slot `+0x10` of an
    element of the type's configuration array at `this + 0x240`, plus `0x40`, with its scope
    parameter.
  - *Tooltip calls.* Decision `custom_tooltip` has the narrower gap (above). System tooltip
    `custom_tooltip` keeps the wider one: `CGalacticObject::GetToolTip` calls the tooltip slot
    `+0x58` of `x21 + 0x40`, where `x21` is a database element, which no type pointer leads to.
  - *No evaluation found.* A scan of every function for an `add`, or a pre-index `ldr`, of the
    block's offset followed by a call finds only constructors, destructors, `ReadMember`, database
    loading and interface text for espionage asset `possible` and `potential`, lawsuit `effect`,
    galactic focus `effect` (only `CGalacticCommunityView::BuildGalacticFocusTooltip`, a
    description call), and first contact `on_create` and `on_abort`.
- 37 trigger and effect blocks keep an unreadable context: a call that the method cannot see into
  receives the scope. The main shapes: a setter or filler whose scope type is a run-time value
  (`SetColonyCarrierRef` for decisions and deposit `on_cleared`; `FillEventScope`,
  `SetupScopeObject` and `CSelectable::DetermineScope` jump tables, which also hide the mission
  and espionage operation from, and give mission `on_success`, `on_fail` and `on_cancel` a `this`
  of no type through the helper `CMission::Stop`); a `from` that is the caller's scope parameter
  (megastructure `potential`, `possible`, `context_menu_potential`, which
  [SDK-812](#block-entry-context-stops-on-m452-sdk-812) resolves); a scope that is a member of a
  parameter or
  heap object (event chain `abort_trigger` and button effect `potential` and `allow` from
  `CExecuteButtonEffectCommand::IsValid`, which copies the command's member at `this + 0x20`); and
  a function that the pass enters and that stops at the path limit (`AddSpentResourcesToScope`
  under buildings `on_queued` and `on_unqueued`; the callers of `CColony::CanAddBuildingType` for
  buildings `potential`).
- Path, loop and step limits are gaps only where a contradiction appears (41 trigger and effect
  blocks at the path limit, 15 at the loop limit); 16 stop at a branch on an unknown value and 4 at
  an instruction. The weight blocks' bounds are on
  [weight blocks](weight-blocks.md#entry-contexts).
- 7 trigger and effect blocks and 1 weight block keep `no-caller`, and 4 trigger blocks keep
  `caller-depth`. These gaps are already narrow; the wrapper that each one ends at:
  - `no-caller`: casus belli `destroy_if` (`CCasusBelli::ShouldDestroyScripted`), colony
    automation `available` (`CColonyAutomationDatabase::GetNextDistrictToBuildPrio`), decision
    `potential` (`CSelectPlanetDecisionEffect::ExecuteActual`), dynamic text `available`
    (`CDynamicTextDatabaseEntry::IsAvailable`), greeting overlay sound `possible`
    (`CGreetingOverlaySound::IsPossible`), pop faction `is_potential`
    (`CEnableFactionTypeEffect::ExecuteActual`), `on_set_leader` (`CPopFactionType::OnSetLeader`)
    and the weight `leader` (`CPopFactionType::GetLeaderWeight`). An effect's `ExecuteActual` is
    reached only through its vtable, and the others through a call that the method does not find.
  - `caller-depth`: astral action `potential` and `is_exhausted` (`CAstralAction::GetTooltip`),
    casus belli `is_valid` (`CCasusBelli::ShouldDestroy`) and pop job `possible`
    (`CPopJob::IsPossible`).

  Script values and modifier blocks are outside the method.

Pitfalls:

- A call with the item itself in `x0` through slot `+0x10` is the item's own virtual call: before
  the offset-zero rule, such calls on owners made 840 of 1,372 entry runs and doubled the block
  method's time. Destructors and `ReadMember` functions still make runs on other members (an
  effect's `Read` is slot `+0x10`, a trigger slot); they join no field of another family.
- The member rule reads constructors only. A class that fills the word in an initializer, such as
  `CPopFaction::Initialize`, leads to no owner.
- A type pointer spilled to the stack is forgotten at a call that receives a lower stack address,
  such as the scope object, by the name pass's stack rule (crisis level `on_unlock`).
- Most helpers are not block helpers: a virtual call on any parameter at an evaluation slot's
  displacement (`+0x10`, `+0x48`) makes a function a helper. A call to one names a block only when
  the caller passes an owner's item plus an offset, and a field joins only its own storage offset,
  so these helpers add no answer; they add runs only where a caller passes an item plus an offset.
- A pre-index load (`ldr x8,[x0,#k]!`) both loads the block's vtable and moves `x0` to the block:
  without it the name pass loses the vtable word (mission `on_issue`, the specialist subject
  perks).
- A scope pointer left in an argument register is not an argument when the callee's signature does
  not take it; an unknown virtual call cannot be checked this way, but a call on a proven instance
  pointer's object can. A C library stub keeps its raw name (`_strlen`), so only
  `LIBRARY_ARGUMENTS` gives its count; it lists the stubs that the decoded code calls on M452, and a
  stub that it lacks reads every argument register. Add a stub when a gap names a call to it.
- Only a prologue's save is a register save. A save after the function's first other instruction,
  and a spill of the same pointer in the body, are stack memory like any other; telling them from
  an object field needs object extents, which the method does not have.
- An instance pointer's writers include its destructor, which clears the pointer; rejecting a
  cleared pointer rejects every null object. Tooltip builders such as `CTraditionType::GetDesc`
  load the null swap and form vtable addresses but only read it, and their searches stop at the
  path limit, so a writer must be one that may store through the pointer. A pass in address order
  that keeps a register's value through later writes marks them as writers, because they reuse
  the register; one that clears a register on any write misses a store on a branch around that
  write.
- The stand-in objects lie 1 MiB apart in the read-only data, so a load from one at a field
  offset reads an unknown word, not the next object's vtable.
- `run_paths_to` skips the site checks inside an entered call, and `follow` runs a setter outside the
  entered calls; without them a prefix that enters a wrapper is lost or runs into the caller.

### Block entry-context stops on M452 (SDK-812)

**Atlas need.** Atlas credits a block's `scope_context` against a config `replace_scopes` claim
only when the read scope is known, the block has an entry context, every slot resolves, and the
field has no `UnresolvedPath`, `UnreadableInput` or `UnnamedDeclaration` gap (Atlas
`assemble_scope_context`). In the SDK-798 comparison (Atlas `95e260b`, config `8574760`), 143
registry `replace_scopes` claims were blocked by such a gap. 56 of them, on 53 blocks in 25
registries, had a known read scope and no blocking gap other than an entry-context stop, so only
these could gain from this method. The other 87 also wait on the read scope (SDK-808) or the reader
(SDK-710). The 16 type-level `replace_scopes` claims have no Atlas subject.

**Repairs.** Three rules, each with authored tests:

- `csetm` runs as `cset` with all ones for true. Councilor `possible` resolves
  (`CCouncilView::RefreshCouncilPickingView` sets a mask with it before the evaluation).
- A switch table with an unknown index forks to each case when its guard proves the bound
  ([compiler jump tables](#compiler-jump-tables)). Observation mission `potential` resolves: the
  order button's `CFleetOrbitalSlotHandle::ExecFunctor` now gives each orbital type. Button effect
  `potential` and `allow` gain the types of `SetupScopeObject`'s cases but keep a `branch-value`
  stop on a walk that begins after a guard, and deposit `can_be_cleared_potential` gains planet,
  ship and astral rift (one case stays unreadable, so its `branch-value` gap becomes an unresolved
  scope). The mission blocks lose `branch-value`, but
  `FillEventScope`'s eleven cases then take their runs past the path limit.
- A scope parameter linked into the entry's scope is read at the callers (module comment of
  `callbacks/blocks.rs`). `CMegaStructureType::IsPotential(CCountry const&, CEventScope&)` builds a
  country scope and stores `x2` as its from; `IsPotentiallyBuildable` passes a fleet scope
  (`Set(0x40, …)`) and `IsPotentiallyUpgradableFrom` a megastructure. Megastructure `potential`
  (from fleet or megastructure), `possible` (country then fleet, or country then megastructure) and
  `context_menu_potential` (country then fleet) resolve. A retry that the climb charges, or whose
  run stops for a reason or reads an unread slot, is dropped and the entry's own run stands: this
  keeps the gaps of buildings `potential` and the decision blocks. A retry replaces the run only
  when each context that the run read comes back at the same evaluation with its read slots
  unchanged and its unread ones read, so a bound cannot hide an arrival that the retry missed.

Before the accepted limit, `registry-sweep --diff` against the baseline names 7 registries: five
of the six above (button effects keeps its gap counts), and districts and specimens, whose `derived_names` each lose one path gap (an
`instruction` stop at `csetm`, a `branch-value` stop at a switch table) with the same names. With
the limit, it names 49: 46 move gaps to the limit, and 10 change their scope context counts (3 from
the repairs). No command grammar changes (`command-population --diff`), and the field sweep changes
only these entry contexts.

Parity changes the three megastructure blocks and two callback answers. On_action
`on_entering_war` gains a country with a country from: `CWar::SetupParticipant` builds a scope at
`sp + 0x200`, links the one at `sp + 0x90` as its from, sets a country in each and fires the
on_action, and its two switch tables (`0x100d29a68`, `0x100d29b2c`) no longer stop the run; its
path-limit gap stays. The five `can_fill_*_job` game rules lose their `branch-value` gap; their path
and loop bounds stay.

**Credit.** 5 of the 56 claims are now creditable: councilor `possible`, megastructure `potential`,
`possible` and `context_menu_potential`, and observation mission `potential`. Each field has a
known read scope, established contexts and no blocking gap.

**Before and after.** Over the M452 recordings, before at `main` `77847bb` (the tracked baseline):

| Stop | Before | After |
| --- | ---: | ---: |
| no attributed evaluation | 79 | 79 |
| path-limit | 54 | 56 |
| some entry scopes unresolved | 46 | 44 |
| loop-limit | 21 | 21 |
| branch-value | 16 | 2 |
| N unnamed direct evaluations (registry) | 10 | 10 |
| no-caller | 8 | 8 |
| caller-depth | 5 | 5 |
| instruction | 5 | 4 |
| selected-block-identity | 4 | 4 |
| only tooltip calls | 1 | 1 |
| **Total (registries)** | **249 (69)** | **234 (69)** |

The SDK-553 run had 242 in 66 registries; SDK-710, SDK-676 and SDK-813 brought more blocks into
the method before this work. Of the 234, an accepted limit covers 138 (below) and 96 stay open: 95
on 61 fields with a field-level `replace_scopes` claim, and the planet classes' unnamed-evaluation
gap, a registry that the limit does not list. Scope context goes from 1,309 to 1,323 complete
fields: five from the repairs, the rest because the accepted gaps leave the counts.

**Accepted limit.** The entry-context gaps of the 93 fields with no field-level `replace_scopes`
claim (129 gaps), and the 9 registry gaps of direct evaluations whose block the method cannot name,
credit no Atlas claim: `ENTRY_CONTEXTS_WITHOUT_CLAIM` in `examples/registry-sweep.rs` lists the
fields. Reopen a field's gaps when the config gives it a claim, or when Atlas maps the type-level
`replace_scopes` claims onto blocks.

**Open, with a claim.** For the 51 claims that remain, by stop:

| Stop | Disposition | Checked members |
| --- | --- | --- |
| Path and loop limits | A larger bound reaches no new evaluation: 1,024 paths take over 20 minutes for one registry and remove one gap. The runs fork on script-profiler and string-length branches. Reopen with a path join that merges states that differ only in dead registers. | astral action `potential`, casus belli `potential`, megastructure `tooltip_system_score` (`BuildBestSystemsText` reaches no evaluation) |
| caller-depth | One more caller turns casus belli `is_valid` into path and loop bounds and an unreadable context (measured at depth 3). | casus belli `is_valid`; astral action `potential` and `is_exhausted` (`GetTooltip`) |
| No attributed evaluation | The block's item comes from an array or a lookup that no type pointer leads to. Reopen paths: an element type of a typed array parameter (3 claims), an owner's offset getter naming its block whatever the receiver (2 claims). | tradable action `on_traded_effect` (`TradeActions`, an element of `CPdxArray<CTradableAction const*, int>`), agreement term value `activate_effect` (`CreateAgreement`, through `CAgreementTermValue::GetActivateEffect`), terraform `ai_weight` (`CAIInteriorMinister::Terraform`, `x23 + 0x350` from a database array); ethic attraction has no direct evaluator call on its offsets |
| instruction | The astral `IsAllowed` run from `CAlertManager::Update` stops in floating-point alert arithmetic (`fminnm`, `fmaxnm`, `fcmp`, `fadd`, `fcsel` in turn), and the same run reaches its loop bound, so more instructions change no answer. | astral action `allow`, `potential`, `is_exhausted` |
| selected-block-identity | The indexed swap load in `CTraditionType::OnEnabled` stays unknown, and the run also reaches the path bound. | tradition and ascension perk `on_enabled` (one function) |
| Some entry scopes unresolved | Missions: the operator's eleven spatial types reach the path bound. The others pass their scope through a call that the pass does not follow and also reach a bound. | mission `on_start`, terraform `on_queued`, anomaly `on_spawn` |
