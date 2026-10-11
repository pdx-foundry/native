# Shared modifier-block grammar

`registry_fields` attaches `FieldMembers::ModifierBlock` to root fields whose constructor proves
one modifier reader address point. `fixed_keys` contains named fields; `entries` contains the
numeric and static-modifier-reference forms. Each property independently reports `Known`,
`Partial` or `Unresolved`. There is no new operation or caller-side reader join.

## Exact-build result

Verified on the exact M452 build in [targets](targets.md) at `registry-fields/v30`. The four
reader identities retain the read/member hash used by persistent fields:

| Reader identity | Member implementation | Fields | Address points | Fixed keys | Fixed keys result |
| --- | --- | ---: | ---: | ---: | --- |
| `e327ea91dfe75d65` | `CModifier::ReadMember` | 2 | 1 | 2 | `Known` |
| `5a74c67fe5a4adf9` | `CGraphicalModifier::ReadMember` | 3 | 1 | 8 | `Known` |
| `ba5f8cddeba0d833` | `CStaticModifier::ReadMember` | 25 | 1 | 11 | `Known` |
| `1c2988588f7e8eaa` | `CCustomDescriptionModifier::ReadMember` | 7 | 1 | 13 | `Partial` |

The full population covers 164 registries. Thirty-seven modifier fields share these four
variants: **30 complete, 7 partial, 0 failed**. The seven partial fields are the
CustomDescription variant, which keeps the `description_parameters` gaps (SDK-741). Every
repeated reader identity has the same block. Another 47 generic persistent Block fields have no concrete
reader join; they include thirteen weight-named fields. The four modifier fields of
`common/agreement_term_values` join since the shared evaluator runs `ld1r` (SDK-547). Constructor-proven weight readers have
their own grammar ([weight blocks](weight-blocks.md)).

All variants have two known entry forms. The smallest variant has `name` (skipped) and `data`
(integer). Graphical adds `icon`, `custom_tooltip` (string), `icon_frame` (integer),
`show_only_custom_tooltip`, `important`, `hide_from_country_list` (boolean). Static adds
`apply_modifier_to_other_planets` and `key` (string) and `divide_over_pop_groups` (boolean).
Custom description adds `description` (string) and `description_parameters` (unknown).

The compact selections are in `tests/expected/m452/modifier-blocks.json`; field selections refer
to these variants by reader identity.

## Entry forms

A numeric entry's key is a name in the modifier table. Its value uses the shared fixed-point
reader `a9818fec780f8313` and the ordinary numeric normalization pass. The block reader itself
keeps `numeric: Known(None)`. Conversion limits remain those of [numeric conversion](numeric-conversion.md).

Scripted modifier entries use that same form. `modifier_families("common/scripted_modifiers")`
reports one `[ItemKey]` family with condition `Always` and no name limit. Atlas can compose its
existing family answer with the numeric entry; no separate parser form or repeated registry
relation is needed.

The reference entry names `common/static_modifiers` with value kind `FixedPoint`. Despite its
name, `CToken::GetFloat()` tail-calls `StringToFixedPoint` on the token text. The reference
method checks the database global, key-text source, lookup hit/miss branches, value-token source
and base-member fallback. It does not copy the numeric entry's conversion limits.

## Fixed-key readers (SDK-675)

**`name` (M452).** `CPdxModifier<…>::TryReadMember` (`0x10015c0e4`) handles token `0x1b` with
`ldr x0,[owner,#0xa0]; ldr x8,[x0]; ldr x8,[x8,#0x10]; blr x8`, with the reader in `x1`. The
object at `+0xa0` is a `CPdxModifierName` implementation. Constructors store
`&CPdxModifierName::_EmptyName`; `CStaticModifier::SetKey` installs a clone of a new
`CPdxModifierNameRefCountedLocKeyImpl`, and post-read code installs other implementations. Every
implementation's `Read(CReader&)` is `mov x0,x1; b CReader::SkipToken()`. `SkipToken` skips the
current token, or the whole block when the token is `{`. So `name` accepts one scalar or a block
and keeps nothing: `ReaderKind::Skipped`, `ValueShape::Any`. The live fixture parses both forms
with no diagnostic.

The base member shape binds the read slot (`name_read_slot`). The binding lists every class that
the object can have: the recipe's `modifier_name_base` (`CPdxModifierNameImplBase`) and each class
whose type information names a class in the set as its single base at `+0x10`. A class with no
vtable in the image has no objects; the base has none. Each other class must have one vtable with
one address point and a local function in the read slot. Another reference into the set from a
type-information record (a multiple-inheritance base), a second address point or an unresolved
slot fails closed (`modifier-name-implementations`). The method then requires every function to
be `mov x0,x1; b TARGET` with one shared `TARGET` that joins with the reader alone; otherwise
`modifier-name-reader` remains.

**Stated fact.** That the object in the name slot derives from `CPdxModifierNameImplBase` is
stated in the recipe, not derived. No method follows every store to `+0xa0`. On M452, the
constructors and `SetKey` store only `CPdxModifierName` implementations, and all 31
`CPdxModifierName*Impl*::Read(CReader&)` bodies are the forward to `SkipToken`. To remove the
statement, join the stores to the name slot.

**`key` (M452).** `CStaticModifier::ReadMember` handles token `0xdc` with
`ldr x1,[reader,#0x288]` (the value token's text), `CString::CString(char const*)` into a stack
temporary, then `CStaticModifier::SetKey(CString const&)` with the owner and the temporary.
`SetKey` assigns the text to `+0x158`, logs a key that names an existing modifier definition and
a key with no localisation, and installs a name implementation.

**Stated rule.** An owner call (`x0` is the owner) that receives a stack `CString` made from the
value token's text, with no stack store between them, reads the value as a string. The join
gives `String` with no destination and an unknown repeat: the method does not follow the callee,
so storage and the accepted domain are not established. A path that makes such a string and
reaches no owner call keeps its earlier `reader-routing` stop at the constructor, so its field
stays named. Checks on M452: no earlier answer changed; the only field or command key that joins
through the rule is `key`, through `SetKey`; the cwtools config has no modifier-block `key`, but
gives the triggered clause's own `key` as `localisation`, a string; and the live fixture reads
`key` with complete parsing. To remove the
rule, derive the setter's string assignment from its body.

**`divide_over_pop_groups` (M452).** Token `0x3ecd` copies bit 2 of `+0x1a8` to a stack byte,
calls `CReader::Read(bool&)` on it, then inserts the byte back into bit 2. `CReader::Read(bool&)`
now joins a stack destination, as the 4-byte integer reader already did. The join gives the kind
only; the walk ends at the call, so the bit storage is not joined. The same rule names 213 other
bit-field flags ([population](discovery.md#modifier-fixed-key-readers-sdk-675)).

**`description_parameters`.** It is a `{ <any key> = <scalar> }` block. Its list loop and entry
serializer, and the obstacle to a public form, are recorded on SDK-741.

**Fixed-key read scope (SDK-749).** Each fixed key's `read_scope` is set where the block is built, for
root and embedded blocks alike. A scalar key reads in no scope (`Known([])`). A key whose reader the
method does not classify stays `Unresolved` with a `read-scope` gap at its path. On M452 the modifier
fixed keys that stay open are `description_parameters` (11 places).

## Proof boundaries and gaps

The virtual trailer is followed only with a constructor-proven address point, an owner-derived
call target and the original owner, reader and token arguments. The shared member walk limits
depth and detects cycles. A store overlapping the owner vtable stops the path before virtual
resolution or member delegation. The modifier walk opts into `orr` updates that drop result
provenance; existing field and command walks keep their prior instruction boundary. This permits
the subsequent string reader for `apply_modifier_to_other_planets` to join.

The description serializer constructor stores its wrapped owner at offset 8. A stack-object
member call with that proven wrapper resolves the description destination. A wrong receiver,
non-owner constructor argument or intervening store prevents this join. Stores inside the
serializer that overlap its wrapped-pointer slot also stop the path, including partial and
aliased stores; writes beside the slot do not invalidate it. The list loop for
`description_parameters` still stops on reader routing or an unsupported instruction (SDK-741).

| Gap | Owner / missing fact |
| --- | --- |
| `modifier-name-implementations` / `modifier-name-reader` | Modifier method: a name implementation outside single inheritance, or a read slot that is not the shared forward (none on M452) |
| `reader-routing` / `instruction` on `description_parameters` | Shared walker: parameter-list loop and nested serializer (SDK-741) |
| Numeric conversion | Shared numeric method: incomplete lexical and overflow properties |

Missing default flow or reference anchors retain typed gaps. An ambiguous token name produces no
named field. Equal public reader identities must have equal blocks; both the population report
and installed-build parity test enforce this. Their negative controls reject a removed fixed key,
a changed reference kind and conflicting blocks under one identity.

Root fields without persistent joins, nested `tradition_swap.modifier` and command fixed-key grammar
are unchanged. Triggered modifier clauses embed these blocks; their join is on the [triggered
modifiers](triggered-modifiers.md) page. A repeated block `Merges` ([repeat
behavior](registry-fields.md#repeat-behavior)). This answer does not establish icon post-read
validation, deferred completion, localisation-key existence, authored-entry storage, or runtime
effects (see [modifier masks](modifier-masks.md#gaps)).

Findings of the SDK-498 prototype that the method does not report:

- **Inherited keys.** On M45-release, `CPdxModifier::TryReadMember` handles token 27 (`name`)
  through a polymorphic name reader and token 240 (`data`) through an integer reader. On M452 the
  method reads both ([fixed-key readers](#fixed-key-readers-sdk-675)).
- **Icon check after read.** `InitPostRead` checks a nonempty icon. A `GFX_` prefix bypasses the
  file check; any other string calls `VFSExists` and can log a missing-icon diagnostic.
- **Repeated blocks.** A blanket "last block wins" rule is wrong. In a council agenda probe on
  M45-observe, a repeated graphical block reset its numeric entries, but omitted tooltip and flag
  metadata persisted. The evidence is `prototype/council-agenda-reconstruction/review.md` and run
  `20260917-023136` in the `atlas-discovery` bundle. On M452, `CPdxModifier<…>::Read`
  (`0x100071fcc`), the `Read` of all four variants, clears the entries before it reads, and the
  member readers write a fixed key only when it is present; the method now reports `Merges`.

## Reproduce

```sh
cargo run --release --example registry-field-sweep -- "$STELLARIS_PATH"
cargo parity modifier_blocks
cargo live fixture_modifier_block
```

The sweep's `modifier_blocks` section reports fields and address points per reader, key differences
from the smallest variant, results, failure shapes and unjoined persistent `Block` fields (uses whose
grammar is not covered, not proof that each is a modifier). The live fixture reads all 13 fixed
keys, a numeric modifier, the scripted modifier `pop_job_amenities_mult` and the static modifier
`gave_up_pop` with complete parsing and no diagnostic. It gives `name` once as a scalar and once
as a block, which shows that a block value is skipped too. It does not test deferred completion or
application.
