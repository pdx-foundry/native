# Registry fields

`Native::registry_fields(registry)` gives root fields, reader and storage shapes, loader
alternatives, nested object fields and local stored-value selections; `Native::registries()`
gives the registries. The module comments of `engine/analysis/fields.rs` and
`engine/analysis/discovery.rs` describe the methods. `FieldDefault` was removed (only `Unknown` was
ever established; Git `d8f9d8a`); SDK-627 owns enum domains and required fields.

## What complete means

Both static answers are bounded searches. `registries` searches the shared database-template
candidates. It is complete when every candidate inside that boundary has one content directory;
custom, nested and late loaders are outside the search. `registry_fields` searches the root reader
paths of one such registry. It is complete when every path, field name and promised reader
classification is resolved. An `OutsideMethod` gap states the boundary and can accompany
`Complete`. Unnamed candidates, unresolved paths, unknown reader classifications and unreadable
required input make the answer partial.

## Current M452 sweep

`tests/population/m452/registry-field-sweep.json` (recorded at `registry-fields/v26`) holds
the baseline: **164 registries, 12 complete, 152 partial, 0 failed**, 1,593 root and 46 nested
fields. Against M451-hotfix, civics lost `multiply_by_habitability_effect_modifier` and edicts
gained `relay_network_modifier`. Compare a new run with `registry-field-sweep --diff` ([method
authoring](method-authoring.md#run-over-the-whole-population)).

- **Council agendas (SDK-600).** All ten fields are found, but the answer is partial: `agenda_cost`
  uses `CVariableValue::Read`, a scoped operand. `ai_weight` has the shared weight grammar
  ([weight blocks](weight-blocks.md)) and reads in `country`; every key has a reader kind, but it
  stays partial for the keyword-domain gaps and for `days`, `months`, `years` and one `factor`
  alternative, whose `CToken::GetInt()` and `GetFloat()` readers have established storage but
  no faithful-storage range ([token value methods](numeric-conversion.md#token-value-methods)). The
  scanner, zero-mask, `trigger` lookup and `parameters` limits are `OutsideMethod`. `CPersistent` block classification of `modifier` does not
  establish its member family. `potential`, `allow`, `effect`
  and `init_effect` enter only as a country with self-linked root, from and prev
  ([block entry contexts](#block-entry-contexts)). `ai_weight` keeps an entry gap: a template
  helper evaluates it on an array element ([weight blocks](weight-blocks.md#entry-contexts-m452)).
  The acceptance test `council_agenda_fields_are_complete_with_every_milestone_4_fact`
  (`cargo parity council_agenda`) lists each missing fact and typed gap in one run. It accepts
  that one `ai_weight` gap and the range gaps of those four weight keys, when their storage is
  known; no Atlas claim needs either. It accepts no other typed gap.
- **Unknown kinds.** Most unknown kinds come from fields with no single established reader and
  from unresolved root paths, not from unclassified reader signatures (`CVariableValue::Read`,
  `CReader::Read(CColor&)`, `CReader::Read(float&)`, `CReader::Read(CVector2FixedPoint&)`).
- **Compound shapes.** The dispatch walker follows a copied value token constructed as an event
  target and moved to an owner destination (`Target`), a `CString` array emplace read by the shared
  string reader (`String/Accumulate`, for example `common/asteroid_belts.mesh`), and an optional
  `CString` set from token text (`String`). A different token source, a stack move destination or
  an element from an unknown call cannot establish the shape.
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

## Repeat behavior

`FieldShape.repeat` comes from one rule for each shared reader, in `session/fields.rs::repeat`.
Each read alternative gets its own value, and the field keeps it when all alternatives agree. A
rule applies only to a tail call: a call with an unexamined continuation proves no final storage.

| Reader | Repeat | Engine fact (M452) |
| --- | --- | --- |
| Primitive readers (Boolean, integer, fixed-point, float, string) | `Replace` | The reader assigns one destination. A fixture repeats `unlocks_agenda`: two occurrences keep the second. |
| `NParserUtil::ReadTrigger<T>`, `ReadEffect<T>` | `Replace` | `ReadTrigger<CRootTrigger>` (`0x100047bd8`) logs `Duplicate trigger at '%s'` when the count at `+0x7c` is nonzero, calls `CTriggerCollectionBase::DeleteAll()` and reads into the same object. `ReadEffect<CEffect>` (`0x10020cc30`) logs `Duplicate effect`, deletes each child, clears the count at `+0x1c` and reads again. All five instantiations share the shape. |
| `CVariableValue::Read(CReader&, EScopeType)` | `Merges` | A literal replaces the literal slot; variable, trigger and script-value slots stay ([scoped numeric](scoped-numeric.md), live `repeat_*` cases). |
| `CReader::Read(CPersistent&)` of a weight block | `Merges` | The persistent read (`0x1025b820c`) tail-calls the virtual `Read` at slot `+0x20`. `CMeanTimeToHappen::Read(CReader&)` (`0x10092e3f4`) reads a bare value into `base` or calls `CPersistent::Read`, so `base` is replaced and earlier entries stay. |
| `CReader::Read(CPersistent&)` of a modifier block | `Merges` | All four variants reach `CPdxModifier<…>::Read` (`0x100071fcc`). It deletes the child array and clears the entry counts at `+0x1c`, `+0x44` and `+0x9c`, then calls `CPersistent::Read`. Fixed keys are written only when present, so an omitted icon, tooltip or flag stays; `description_parameters` appends. |
| `ReadKeyReferenceDeferred<D>` with an established lookup | `Replace` | Each occurrence registers its key and a lambda that writes the destination on a hit and on a miss. The resolver runs registrations in order ([references](references.md#reference-readers)), so the last occurrence's item is stored. |

Every other reader keeps `Unknown`, including `CTrigger::Read`, `CEffect::Read`, a persistent
block without a constructor-proven weight or modifier reader, and an immediate reference reader,
whose caller stores the result after the call returns.

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

**The default case.** The rule is in the module comment of `fields.rs`. Pitfall: "the most
frequent target is the default" is wrong. `CMissionType::ReadMember` has a table for tokens
11653–11656 with no default slot, and `on_fail` and `on_cancel` share one case that reads the same
effect member (`+0x430`); that case ties for the most frequent target. When a wide interval that
leaves through the table's guard does not end at the rejection, the default may be past its end,
so a case is kept only when it joins a known reader. No table on M45-release gives a `JumpTable`
gap: each default reaches the rejection, or the table has no default slot.

**Bit fields.** Boolean bit fields such as `tooltip_show_star_resources`,
`place_entity_on_planet_plane`, `use_planet_resource`, `can_prevent_crisis_terraformation`,
`is_ruined_orbital_ring` and `hide_name` copy the bit to a stack temporary with `ldr` and `ubfx`,
or with `ldrb w8,[x19,x8]` where `x8` is a constant, then call `CReader::Read(bool&)`. The walker
reads a register-offset load with a constant index and forgets the result of `ubfx` and `and`, so
these paths reach the call and name their fields. The reader join stays missing, because the
destination is a temporary, not the member. Joining the temporary to its member is a separate
repair.

## Members and shared readers

A field's reader identity, kind and family come from every token path whose interval holds the
field's token, not only from its singleton paths. A wider path that is unresolved, or reads with
another reader, leaves the shared claim unknown; each read alternative keeps its own reader.
Rejected paths do not change the claim. A condition on a nested field, on its reference lookup or
on a modifier-block fixed key names the full field path from the root field or command key.

### Reader identities and kinds

`registry_fields` reports one opaque identity for fields that share a joined reader, and one
conservative broad value kind. The kinds promise no complete grammar, valid ranges, occurrence
rules, reference ownership, nested behavior or runtime behavior, so a known identity can still
have kind `Unknown`. A missing identity means that the paths did not establish one shared reader.
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

### Read conditions and use-time inheritance (M45-release)

- `CCouncilAgenda::ReadMember` (`0x10020bfa8`) tests presence bytes at `+0x6d0` and `+0x6d8`. Both
  sides reach `CReader::Read(int&)` after clearing the value at `+0x6d4` or `+0x6dc`: the branches
  initialize presence and do not restrict which occurrences are read. Equal complete outcomes on
  opposite presence branches collapse to `Always`; unresolved outcomes and unequal destinations
  cannot.
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
  policy, physical-file resolution or "last value wins" (SDK-552).
- Failed approaches: one observer form had too much overhead and others failed to start; the first
  transfer to the AI budget failed until the owner rule was revised.

### Registry names

`Native::registries` names each template registry by the content directory that its constructor
passes to the shared base constructor; the module comment of `engine/analysis/directories.rs`
gives the two compiled shapes. On M45 the method named all 164 template registries in about two
seconds, and all 162 directories that the retained live run observed are in the result, with no
conflict.

- Seven template registries load from outside `common/`, such as `map/galaxy`,
  `sound/advisor_voice_types`, `gfx/portraits/sprite_configurations` and
  `interface/resource_groups`. So a registry name is the full content directory.
- 59 more `common/` literals belong to loaders outside the template method, such as
  `common/component_templates`, `common/agendas` and `common/static_modifiers`. They are inputs
  for SDK-551.
- Pitfall: the first version of the method took any directory-shaped literal in the constructor.
  It missed the global `CString` shape of `common/ship_categories`, and it had no tie to the
  meaning of the literal. The base-constructor call anchor replaced it.

### Scheduler table on M45-release

No supported operation reads the scheduling table; the removed scheduler method and its tests are
at `git show 8d9a073:src/engine/analysis/discovery/scheduler.rs` (with `discovery.rs` and
`tests_discovery.rs`).

- `NNullObjAndDatabaseInitUtil::SetupDatabases(CPdxArray<SDatabaseObjectFunctions, int>&, ...)`
  fills the table with literal values from `0x1005eb938` to `0x1005eedf0` (exclusive); scheduling
  begins at that end address, found by hand. After `mov x19, sp`, the table is at `x19 + 96`:
  **198 rows of 48 bytes**, six 8-byte slots each: the name as a C-string address, then
  `TGameDatabase<T>::CreateInstance()`, `DestroyInstance()`, zero (`stp x8, xzr`), `InitInstance()`
  and `PostReadInitInstance()`. Function addresses come from `__DATA_CONST,__got` slots, so nothing
  resolves without chained fixups. Row 0 is `CNamedColorDatabase`.
- 163 rows join exactly one template candidate by the database type in a slot's symbol;
  `CGameScenarioDatabase` joins none. 35 rows are outside the template method, including
  `CStaticModifierDatabase`, `CStrategicResourceDatabase`, `CPlanetClassDatabase`,
  `CSpecialProjectDatabase`, `COnActionDatabase` and `CTraitDatabase` (SDK-551).
- Pitfalls: without `mov x19, sp`, every row is a gap, never an empty table; the stack probe
  `blr x16` (`___chkstk_darwin`) at `0x1005eb960` is an unknown call before it; a new value in `x19`
  invalidates the table owner; the row count is not a registry count.

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
[weight blocks](weight-blocks.md#entry-contexts-m452)), with a scope in `x1`. The method either builds the scope itself
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
are on [weight blocks](weight-blocks.md#entry-contexts-m452). 6 registries have 7 direct
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
([weight blocks](weight-blocks.md#entry-contexts-m452)).
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
  [weight blocks](weight-blocks.md#entry-contexts-m452). By group, read by hand on M452:
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
  (megastructure `potential`, `possible`, `context_menu_potential`); a scope that is a member of a
  parameter or
  heap object (event chain `abort_trigger` and button effect `potential` and `allow` from
  `CExecuteButtonEffectCommand::IsValid`, which copies the command's member at `this + 0x20`); and
  a function that the pass enters and that stops at the path limit (`AddSpentResourcesToScope`
  under buildings `on_queued` and `on_unqueued`; the callers of `CColony::CanAddBuildingType` for
  buildings `potential`).
- Path, loop and step limits are gaps only where a contradiction appears (41 trigger and effect
  blocks at the path limit, 15 at the loop limit); 16 stop at a branch on an unknown value and 4 at
  an instruction. The weight blocks' bounds are on
  [weight blocks](weight-blocks.md#entry-contexts-m452).
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
