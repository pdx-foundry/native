# Registry fields

`Native::registry_fields(registry)` gives root fields, reader and storage shapes, loader
alternatives, nested object fields and local stored-value selections; `Native::registries()`
gives the registries. The module comments of `engine/analysis/fields.rs` and
`engine/analysis/discovery.rs` describe the methods. `FieldDefault` was removed (only `Unknown` was
ever established; Git `d8f9d8a`); SDK-627 owns enum domains, repeat behavior and required fields.

## What complete means

Both static answers are bounded searches. `registries` searches the shared database-template
candidates. It is complete when every candidate inside that boundary has one content directory;
custom, nested and late loaders are outside the search. `registry_fields` searches the root reader
paths of one such registry. It is complete when every path, field name and promised reader
classification is resolved. An `OutsideMethod` gap states the boundary and can accompany
`Complete`. Unnamed candidates, unresolved paths, unknown reader classifications and unreadable
required input make the answer partial.

## Current M451 sweep

`tests/population/m451-hotfix/registry-field-sweep.json` (recorded at `registry-fields/v18`) holds
the baseline: **164 registries, 8 complete, 156 partial, 0 failed**, 1,593 root and 46 nested
fields. Compare a new run with `registry-field-sweep --diff` ([method
authoring](method-authoring.md#run-over-the-whole-population)).

- **Council agendas (SDK-600).** All ten fields are found, but the answer is partial: `agenda_cost`
  uses `CVariableValue::Read`, a scoped operand. `ai_weight` has the shared weight grammar
  ([weight blocks](weight-blocks.md)) and reads in `country`; every key has a reader kind, but it
  stays partial for the conversion, zero-mask, keyword-domain, `trigger` lookup, `parameters` and
  repeat gaps listed there. `CPersistent` block classification of `modifier` does not
  establish its member family. `potential`, `allow`, `effect`
  and `init_effect` enter as a country with self-linked root, from and prev; `potential` and
  `allow` keep two contradicted call sites ([block entry contexts](#block-entry-contexts)).
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
- **Repeat behavior.** A fixture agrees with `unlocks_agenda` replacing storage: omission leaves an
  empty string and two occurrences keep the second. This sets no occurrence limit or default rule.

Pitfalls:

- A state branch before token discrimination would give a false registry-wide `UnresolvedPath`
  gap: the token-partition check needs one contiguous chain. No M45 loader has that shape.
- `b.hi` stops in `CEspionageOperationType` and `CStarClass` compare a value loaded from the object,
  not the token, so they correctly stay unknown flags.
- Unresolved concrete readers do not remove established field names or broad `Block` kinds.

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
must get its flags from the adjacent comparison on every incoming path. Primitive tail readers
establish replacement; block calls alone do not. SDK-546 depends on these relationships for
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

`Field.entry_contexts` gives, for each root trigger and effect block, the scopes that the engine's
direct evaluation calls supply for `this`, `root`, the `from` chain and the `prev` chain
(`registry-fields/v13`; `callbacks/blocks.rs`, `callbacks/contexts.rs`,
`src/binding/binary/callbacks.rs`, `src/session/field_entries.rs`). The answer keeps
`EntryScope::SelfLink`; read it by the [self-link rule](engine-commands.md#on_actions-game-rules-and-their-entry-scopes).
`this` is the scope at the call; the scope that the engine reads the block in is SDK-549's.

**How the engine evaluates a stored block.** A registry item evaluates its own blocks in its
methods: it passes `this` plus the block's storage offset to a trigger evaluator or an effect
executor (`CTrigger::Evaluate`, `EvaluateExtended`, `CEffect::Execute`, `ExecuteExtended`,
`CRootEffect::Execute`), with a scope in `x1`. The method either builds the scope itself
(`CTraditionType::IsPotential(CCountry const*)`: a fresh scope, `SetCountry`, then `this + 0x108`)
or takes it from its caller (`CCouncilAgenda::IsPotential(CEventScope&, CString*)` evaluates
`this + 0x1c8` with its scope parameter; `CGovernment::UpdateCouncilAgenda` builds that scope).
The field method's storage offset is the offset from the method's `this`.

**The method.** The name pass attributes a call to `(owner, offset)` only when `x0` holds `this`
plus one offset on every path. A method whose scope is its own parameter is a *wrapper*; the method
goes up its direct callers, at most 2, to the call that builds the scope. From that call it runs
the caller to the call and through it, and runs every wrapper and every function that receives a
scope, up to 6 calls deep, inline on the path with the caller's arguments and memory. Before the
selected call, evaluations act only through their effects. A call reads the argument registers that
its demangled signature uses; a call through an import pointer to the stack probe
(`___chkstk_darwin`) reads none; a call to a function that never returns ends the path. A copy of a
scope pointer in the stack reaches a call only in the frame of a stack address that the call
receives, at or above that address.

**Assumptions.** Beside the self-link rule:

- [Evaluation leaves a scope as it found it](engine-commands.md#on_actions-game-rules-and-their-entry-scopes):
  trigger and effect code that receives a scope is a scope reader.
- A bounded search with no contradiction: when the paths of a call stop only at the path, loop or
  step limit, and every evaluation that the followed paths reach receives a scope that the method
  can read, the found contexts stand with no gap. A path that reaches an evaluation with an
  unreadable scope is a contradiction; then the bounds are gaps too.

**Result on M451-hotfix.** 237 root trigger and effect blocks in 164 registries: 109 have contexts
and no entry gap, 53 have contexts and a gap, 75 have none. 24 blocks keep several readable
contexts, 47 name a typed `from` and 2 a typed `prev`. 3 registries have 4 evaluation calls whose
block the method cannot name. These call sites were checked by hand in the disassembly:

- council agenda `potential` and `allow`: `CGovernment::UpdateCouncilAgenda` builds a country scope
  and calls `IsPotential` and then `IsAllowed` with it (`this + 0x1c8`, `this + 0x280`).
- council agenda `init_effect`: `CGovernment::SetCouncilAgenda` builds a country scope;
  `ExecuteInitialEffect` tail-calls `CEffect::Execute` on `this + 0x580`.
- tradition `potential`: `CTraditionType::IsPotential` builds its own country scope.
- diplomatic action `on_accept`: `CDiplomaticActionType::OnAccept` builds two country scopes, links
  the second as the first's from and runs `this + 0x2f8` with the first.
- tradition `on_enabled`: `OnEnabled` runs the swap's or its own effect through vtable slot `+0x48`,
  a virtual call, so the block has a gap.

**Comparison with the config's `replace_scopes`.** Read through the self-link rule, comparing the
keys that the config states (`system` is the engine's `galactic_object`; `any` matches any scope):
107 blocks have a readable context and a config expectation. 82 agree, 5 agree on the stated keys
where the engine also sets a first link that the config omits, and 20 disagree. 103 blocks have no
readable context and 27 have no `replace_scopes`. Each disagreement was read by hand:

- **Source errors (10).** The engine agrees with Native. Armies `on_built`, `on_queued` and
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

**Gaps.**

- 63 blocks have no direct evaluation in the owner's methods: they run through a virtual call
  (tradition `on_enabled` and `on_disabled`), outside the owner's methods, or only as nested blocks.
- 44 blocks keep an unreadable context: a call that the method cannot see into receives the scope.
  Council agenda `potential` and `allow` keep two. `ExecuteTradition` passes the scope to
  `CTraditionType::GetUnlocksAgenda`, which, for a tradition with no swaps, makes a virtual call
  while `x1` still holds the scope; the swap paths stop at the loop limit. The AI's
  `SelectByWeightedRandom<CCouncilAgenda>` keeps the scope pointer in a callee-saved register that
  `IsPotential` saves in its own frame above a local string that it passes to `CString::operator+=`.
- Path, loop and step limits are gaps only where a contradiction appears (45 path, 17 loop).
- 7 wrappers have no direct caller (`no-caller`) and 4 pass the scope on past 2 callers
  (`caller-depth`). Weights, script values, modifier blocks and nested blocks are outside the method.

Pitfalls:

- A scope pointer left in an argument register is not an argument when the callee's signature does
  not take it; an unknown virtual call cannot be checked this way.
- A callee-saved register spill is stack memory like any other; telling it from an object field
  needs object extents, which the method does not have.
- `run_paths_to` skips the site checks inside an entered call, and `follow` runs a setter outside the
  entered calls; without them a prefix that enters a wrapper is lost or runs into the caller.
