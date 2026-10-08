# Triggered modifier clauses

`registry_fields` attaches `FieldMembers::TriggeredModifier` to root fields whose collected object
has a triggered modifier clause reader, such as `triggered_modifier`,
`triggered_country_modifier` or `triggered_planet_modifier`. The field has
`BlockFamily::TriggeredModifier` and `repeat: Accumulate`; each occurrence is a new clause. The
block reports the clause's own keys and `other_keys`, the shared modifier block that reads every
other key. A `modifier` key carries that block too. Source stamp `registry-fields/v19`.

The method is `src/engine/analysis/modifier_blocks/triggered.rs`, bound in
`src/binding/binary/triggered_modifiers.rs` and normalized in `src/session/triggered_modifiers.rs`.
The field join is the nested-object proof (`fields/nested.rs`, `src/binding/binary/fields.rs`).

## Engine facts (M451-hotfix)

Executable `29fa877366040a528098da39ec7e70b7baac76782a2a6bd161616d691f86fa38`, read with
`examples/inspect`.

**One class template.** `CTriggeredModifierBase<T>` covers every clause. There is no separate
planet or pop-group class; the owner and the scope it passes decide the use. Three vtables hold a
clause reader:

| Class | ReadMember | `ReadModifier` (slot AP+0x40) | Users |
| --- | --- | --- | --- |
| `CTriggeredModifierBase<CStaticModifier>` | `0x100d0f104` | `add x0, x0, #0x238`; tail call `CStaticModifier::ReadMember` | most owners |
| `CTriggeredModifierBase<CCustomDescriptionModifier>` | `0x100d105c0` | the same, to `CCustomDescriptionModifier::ReadMember` | traditions, ascension perks |
| `CTriggeredModifierWithTooltipData` | inherits Static's | `0x100d0efa8`: static-modifier lookup, then two member reads | jobs |

The read slot (AP+0x20) holds the generic `CPersistent::Read`, so the recipe anchors the family on
the member slot. The delegate slot target tells apart Static and Tooltip, which share read and
member; it is part of the reader identity.

**Constructor.** `CTriggeredModifierBase(int, ModifierCategory, EScopeType)` builds a
`CFixedPointVariableValue(_VONE)` at `+0x30`, the embedded `T` at `+0x238`, the stored scope word
(`+0x3e8`, CustomDesc `+0x428`) and a `CRootTrigger(true)` after it.

**Member reader (Static).**

| Token | Key | Behavior |
| --- | --- | --- |
| `0xdc` | `key` | `CReader::Read(CString&, false)` into `+8` |
| `0x2d21` | `potential` | `CTrigger::Read(CReader&, EScopeType)` on the root trigger, scope from the stored word |
| `0x2d22` | `show_if_not_potential` | `CReader::Read(bool&)` |
| `0x2d23` | `not_potential_override_text_key` | `CReader::Read(CString&, false)` |
| `0x2d5c`, `0x3c59` | `multiplier`, `mult` | `CVariableValue::Read(CReader&, EScopeType)` into `+0x30`; both keys write the same value |
| `0x3fff` | `modifier` | `CReader::Read(CPersistent&)` on the embedded `T` |
| other | direct entries | tail call through slot AP+0x40 |

The nested `modifier` block and the direct entries reach the same embedded object, so both use the
SDK-607 grammar of `T` ([modifier blocks](modifier-blocks.md)): `ba5f8cddeba0d833` for Static and
`1c2988588f7e8eaa` for CustomDesc. `custom_tooltip`, `show_only_custom_tooltip`, `description`,
`description_parameters` and `divide_over_pop_groups` are keys of `T`, not of the clause.
`divide_over_pop_groups` is a `CStaticModifier` key, so every variant accepts it. The clause's
`key` handler runs first, so a direct `key` never reaches `T`; inside `modifier = { }` it is `T`'s.

**Owner side.** Each occurrence is created, read through the virtual `Read`, and appended to a
`CPdxArray<CPdxScopedPtrImpl<C, false>, int>` with `InsertAtEmplace(int, …&&)`. The moved-from
pointer's virtual destructor follows behind a null check. There are two creation forms:

- `operator new` and a direct constructor call (traditions, jobs);
- `PdxMakeScopedPtr<C, …>`, which allocates, constructs and stores the pointer through `x8`
  (megastructures and most others).

The insertion clears its source on every return (`ldr x8, [x22]; str xzr, [x22]` with `x22` the
entry `x2`), on both the grow and the in-place path.

**Not clause readers.** `common/federation_perks.triggered_modifier` skips its body
(`CReader::SkipBody`) or returns without storing. `economic_category_triggered_modifier`
(`CEconomicCategory::CTriggeredModifierTable::CSerializer<N>`) is a different reader with `key`,
`trigger`, `use_parent_icon` and `modifier_types`.

## Result on M451-hotfix

Fifty root fields in the population of 164 registries have a clause reader: **0 complete, 50
partial, 0 failed**. Equal identities have equal clauses.

| Identity | Variant | Fields | Registries |
| --- | --- | ---: | --- |
| `88f77b58ddf77488` | Static | 43 | buildings, colony types, deposits, edicts, ethics, federation perks, councilors, megastructures, psionic auras, pop categories, relics, resolutions, situations, specimens, storm types |
| `d3a866b65e16b749` | Tooltip | 5 | pop jobs |
| `f9fb8c8f5f2ca714` | CustomDesc | 2 | traditions, ascension perks |

Static and CustomDesc report all seven clause keys as known: `key` and
`not_potential_override_text_key` (`String`, no reference), `potential` (`Block`, family
`Trigger`, the shared `CTrigger::Read` identity), `show_if_not_potential` (`Boolean`), `mult` and
`multiplier` (`ScopedNumeric` with the ordinary numeric and operand facts) and `modifier` (the
SDK-607 block). `other_keys` is known and names the same block. Every field stays partial for the
gaps below, most of them inherited from the modifier block.

The Tooltip variant is a stated variant. Its constructor calls the Static constructor through a
branch-island stub whose body the method does not enter, so the members that the base constructor
builds (`+0x30`, `+0x238`) stay unknown: `modifier` has no join, `mult` and `multiplier` have no
scoped destination, and `other_keys` is `Unresolved` because the Tooltip delegate looks up a static
modifier and reads two members (`triggered-delegate`). Its seven clause keys and their kinds are
the same as Static's.

Failure shapes, by field count:

| Shape | Fields |
| --- | ---: |
| Modifier-block reader gaps (`key`, `divide_over_pop_groups`, `name`; CustomDesc also `description_parameters`), at the `modifier` key and on the field for `other_keys` | 45 |
| Numeric conversion of modifier entries and fixed keys ([numeric conversion](numeric-conversion.md)) | 45 |
| Scoped-literal conversion and operand method of `mult` and `multiplier` | 45 |
| Read scope of `potential` (stored scope) and of `modifier` and the field (scope argument) | 50 |
| Field repeat behavior (`Repeat behavior or nested fields remain unresolved`) | 50 |
| Tooltip: embedded point, scoped destination and delegate | 5 |

**Not covered.** The sweep lists triggered-named fields with no clause reader:

- `tradition_swap.triggered_modifier` in traditions and ascension perks. Collection discovery
  does not run inside a nested owner; `nested.rs` builds a nested result with no collections.
  SDK-676 carries the nested-owner path.
- `common/federation_perks.triggered_modifier`, which the engine skips.
- The four `common/economic_categories` triggered fields, which use a different reader.
- Owners outside the population: `CTrait` (no registry), starbase buildings and modules (their
  sites are behind the base `CStarbaseComponent<…>::ReadMember` call), districts and zones.

The compact selections are in `tests/expected/m452/triggered-modifiers.json`.

## Gaps

- `potential` and `modifier` keep their read-scope gaps; the scope is a constant at each creation
  site and stays with SDK-549.
- `mult` and `multiplier` keep the scoped-literal conversion and operand-method gaps of
  [scoped numeric](scoped-numeric.md).
- The embedded block keeps every SDK-607 gap. For `other_keys` the gaps are stated on the field,
  because the block's key paths would name the clause's own keys, such as `key`.
- Repeat behavior of the whole field stays the collection's `Accumulate`, with the field's repeat
  gap; occurrence bounds are not established.
- Condition timing, how a multiplier scales the modifier and where it takes effect are runtime
  behavior (see [modifier masks](modifier-masks.md#gaps)).

## Pitfalls

- **The delegate is virtual.** The default route is `ldr x8, [x0]; ldr x8, [x8, #0x40]; br x8`.
  The dispatch walk joins it only as the recipe's bound delegate, on a tail call that passes the
  owner, reader and token unchanged; no other virtual callee becomes a reader.
- **Branch-island stubs.** Some constructor symbols are stubs outside `__text` that only branch to
  a body with the same name. A body that is only `b stub` is an alias of that body, and a stub has
  no body to enter; its summary still installs its vtables. Without these rules the clause's two
  constructors disagree and no embedded point survives.
- **Template constructors.** `Class<T>::Class(` names its class with the template arguments; the
  shared matcher strips them only to compare the method name.
- **A move is a proof, not a model.** The nested proof writes null to the moved-from pointer only
  for an insertion whose body the binding proved clears its source. Any other insertion is an
  unmodelled call, and the field is not joined.

## Reproduce

```sh
cargo run --release --example registry-field-sweep -- "$STELLARIS_PATH"
cargo parity triggered_modifiers
cargo live fixture_triggered_modifier
```

The sweep's `triggered_modifiers` section gives fields by status and failure shapes per reader
identity, and the triggered-named fields that no clause reader is bound to.
