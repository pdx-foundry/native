# Shared modifier-block grammar

`registry_fields` attaches `FieldMembers::ModifierBlock` to root and nested fields whose owner's
constructor proves one modifier reader address point. `fixed_keys` contains named fields; `entries` contains the
numeric and static-modifier-reference forms. Each property independently reports `Known`,
`Partial` or `Unresolved`. There is no new operation or caller-side reader join.

## Exact-build result

Verified on the exact M452 build in [targets](targets.md) at `registry-fields/v40`. The four
reader identities retain the read/member hash used by persistent fields:

| Reader identity | Member implementation | Fields | Address points | Fixed keys | Fixed keys result |
| --- | --- | ---: | ---: | ---: | --- |
| `e327ea91dfe75d65` | `CModifier::ReadMember` | 17 | 1 | 2 | `Known` |
| `5a74c67fe5a4adf9` | `CGraphicalModifier::ReadMember` | 3 | 1 | 8 | `Known` |
| `ba5f8cddeba0d833` | `CStaticModifier::ReadMember` | 29 | 1 | 11 | `Known` |
| `1c2988588f7e8eaa` | `CCustomDescriptionModifier::ReadMember` | 18 | 1 | 13 | `Known` |

The full population covers 185 registries. Sixty-seven modifier fields share these four variants:
64 root fields (**64 complete**, with `common/planet_classes.modifier` since SDK-813) and 3 nested
fields (**3 complete**), 0 failed. Every repeated reader identity has the same block, root or
nested. The 23 root Block fields whose persistent read still has no constructor-proven reader
are in [unjoined persistent fields](#unjoined-persistent-fields-sdk-676). The four modifier fields of
`common/agreement_term_values` join since the shared evaluator runs `ld1r` (SDK-547).
Constructor-proven weight readers have their own grammar ([weight blocks](weight-blocks.md)).

All variants have two known entry forms. The smallest variant has `name` (skipped) and `data`
(integer). Graphical adds `icon`, `custom_tooltip` (string), `icon_frame` (integer),
`show_only_custom_tooltip`, `important`, `hide_from_country_list` (boolean). Static adds
`apply_modifier_to_other_planets` and `key` (string) and `divide_over_pop_groups` (boolean).
Custom description adds `description` (string) and `description_parameters` (a block of any
key with one scalar value, `FieldMembers::AnyKey`).

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

**`description_parameters` (SDK-741, M452).** `CCustomDescriptionModifier::ReadMember` sends
`description_parameters` (`0x39a4`) and `description` (`0x317a`) to a stack
`CCustomDescription::CSerializer` that wraps the owner. The serializer's `ReadMember`
(`0x10035e8b0`) reads `description` with `CReader::Read(CString&, bool)`. For the list token it
requires the value token kind 3 (`{`), or it reports `Expected list start` and keeps nothing.
The loop calls lexer slot `0x10` until kind 4 (`}`) or `0x13` (end of input). For each entry it
appends an `SParam` to the array at wrapped `+0x28` (`InsertAtEmplace` at the count `+0x3c`)
and reads it with a stack `SParam::CSerializer` through `CReader::Read(CPersistent&)`. Nothing
clears the array, so a repeated key and a repeated block accumulate.

`CCustomDescription::SParam::CSerializer::Read(CReader&)` (`0x10035eb84`), the vtable's read
slot, calls `ReadSimpleStatement` and copies the key token's text with `CToken::ReadValue`. It
accepts a value token whose kind is at most `0x14` with its bit in both `0x1ff000` and
`0xfff0ffff` (`0xc`, `0xd`, `0xe`, `0xf`, `0x14`), or the keyword kinds `0x167`, `0x414` and
`0x29c` (`long_float`, `num64`, `unum64`), and copies it to the parameter (`CToken::operator=`).
Any other value gets `ReportMalformed("Unexpected value type for custom description modifier
parameter")` and an empty token; the entry stays.

The method `engine/analysis/modifier_blocks/parameters.rs` matches the serializer member from
its entry to the `Expected list start` report (`shapes/parameter_list.txt`), finds the entry
reader in the read slot of the vtable that the loop stores, and matches the entry reader's kind
test (`parameter_entry.txt`) and keyword tests (`parameter_keywords.txt`). It reads the kinds
from the matched immediates. A stopped walker path of the list token that passes the list route
becomes a `Block` reader with repeat `Accumulate`, and the key gets `FieldMembers::AnyKey`.

**Stated rule (written forms).** The written form of a kind comes from the [text
lexer](text-lexer.md): `0xc` is a number word, `0xf` a quoted string or a word that names no
keyword, and a keyword word has its token number. `0xd`, `0xe` and `0x14` come only from tokens
built from a value (`CToken(float)`, `CToken(bool)`, `CToken(unsigned int)`); they have no
written form. So the forms are `Number`, `Text` and the three `Keyword`s, and a bare `yes`, `no`
or other keyword is malformed. The forms are `Known` only while the lexer boundary holds; else
they are `Unresolved` with a `ReaderSemantics` gap. An accepted kind outside these sets stops the
method (`parameter-entry-kind`). A `@` value is replaced in `ReadSimpleStatement` before the
check, so the forms do not cover it. To remove the rule, join the built-token kinds to their
constructors.

**Fixed-key read scope (SDK-749).** Each fixed key's `read_scope` is set where the block is built, for
root and embedded blocks alike. A scalar key reads in no scope (`Known([])`). A key whose reader the
method does not classify stays `Unresolved` with a `read-scope` gap at its path. On M452 the modifier
fixed key that stayed open was `description_parameters`; a parameter list reads in no scope
(`Known([])`, SDK-741).

## Nested owners (SDK-676)

A nested owner is an object that a root field constructs and inserts into a collection, and
whose reader has no block family, such as `CTraditionSwap`. The binding gives it its own
constructor evidence (the persistent binding of its class, with the shared stored-scope and
initialized words) and its own inserted objects, one level deep. The method then walks it as it
walks a root owner (`owner_result` in `fields.rs`), so its fields get persistent readers, address
points, container masks, embedded keys and clause collections by the same code. Nested objects
of a nested owner get neither, which was the depth of every nested owner before.

The session attaches modifier, triggered modifier, container and weight grammar at every
`FieldMembers::Fields` depth with the nested object's own result. Gaps name the full key path,
and the field conditions inside an attached grammar name the path from the root
(`tradition_swap.modifier.<key>`). A nested field keeps its nesting; nothing is copied to the root.

On M452 the method finds three nested owners with fields:

| Registry | Nested field | Result |
| --- | --- | --- |
| `common/traditions`, `common/ascension_perks` | `tradition_swap.modifier` | `1c2988588f7e8eaa`, the root `modifier` block and categories |
| | `tradition_swap.triggered_modifier` | `f9fb8c8f5f2ca714`, the root clause and the country mask |
| | `tradition_swap.weight` | weight identity `f08cb83d92484a89` |
| `common/governments/authorities` | `advanced_authority_swap.modifier` | `ba5f8cddeba0d833`, every category |
| | `advanced_authority_swap.weight` | weight identity `f08cb83d92484a89` |

`advanced_authority_swap.election_type` keeps its reader-routing stop (SDK-710).

**Not discovered.** Civics `swap_type`, situation `stages` and `approach`, psionic aura
`intensity_level`, and patron `covenant` and `passive_accord` construct their objects through a
path that stops before the collection is proven (`reader-routing` at `operator new`, or an
indirect call). They are not nested owners for the method, so their members stay unresolved.
Whether they hold modifier fields is not established; the discovery path belongs to SDK-551.

## Unjoined persistent fields (SDK-676)

A Block field whose persistent read has no constructor-agreed reader has no family, so it gets
no block grammar; it also has the gap "The field's alternatives do not establish one shared
reader." The class whose vtable a constructor stores at the destination on some paths gives each
one's use. The constructor walk, not the modifier method, stops all of them.

**Delegating constructors (repaired).** A default constructor that builds a stack `CString` and
then delegates to the full constructor stopped the walk at the `CString` constructor, whose
receiver is outside the owner (`persistent-constructor-owner`); the owner then had no agreed point.
Such a constructor is now an ordinary call for the owner: the walk forgets what an unknown call
may write and continues, so the delegated constructor's points join. This joined 19 fields:

| Use | Fields |
| --- | --- |
| Modifier (`CCustomDescriptionModifier`, `1c2988588f7e8eaa`) | federation_laws `modifier`; federation_perks `envoy_modifier`, `federation_modifier`, `leader_modifier`, `members_modifier`; resolutions `modifier`; specialist_subject_perks `overlord_modifier`, `overlord_relay_network_modifier`, `subject_modifier` |
| Weight | federation_laws `ai_weight`, `required_centralization`; federation_types `ai_weight`; galactic_focuses `weight`; leader_classes `ai_weight`, `ai_location_weight`; resolutions `ai_weight`; spynetworks `network_weight`, `strategy_weight` |
| Other object (`SBeamFade`) | planet_destruction `fade`, whose keys `fade_in` and `fade_out` are now listed |

Each joined field gets the grammar of its reader identity, its read scope, and its container
categories (`Listed` for a modifier, `NotApplicable` otherwise); five weights also gain entry
contexts. `common/sector_types` loses its registry `UnresolvedReader` gap, but its two fields
stay unjoined (B below).

**Still unjoined (23 fields).**

| Use (class at the destination) | Fields | Obstacle |
| --- | --- | --- |
| Modifier (inline `CModifier`) | civics `modifier`; star_classes `modifier` | B; prerequisite SDK-710 |
| Weight (`CMeanTimeToHappen`, `CScriptableValue`) | civics `ai_weight`, `random_weight`; diplomatic_actions `ai_acceptance`, `should_ai_propose`; sector_types `system_scores`, `local_capital_scores` | B |
| Not established | ship_sizes `modifier`, `ship_modifier`, `map_icon_override` | path limit of the constructor walk |
| Not established | storm_types `spawn_weight`, `storm_speed`, `storm_min_radius`, `storm_max_radius`, `storm_min_steps`, `storm_max_steps`, `storm_activation_period_in_months`, `storm_monthly_added_devastation`; situations `on_monthly`; global_ship_designs `name`; diplo_phrases `weight` | no constructor path keeps a point at the destination |
| Not established | patrons `covenant` | `float-width` in the constructor walk |

- **B.** The constructor copies `CString` members inline. The copy into the return of
  `CPdxCommonStringAllocator::allocate` (an opaque call given an owner address), and a byte store
  at an unknown index, forget the owner on most paths (civics: 24 of 32 entered paths). A repair
  models what the string helpers write; it belongs to the shared constructor walk, which SDK-710
  owns. Civics `modifier` (`+0x488`) and star_classes `modifier` (`+0x140`) are proven modifier
  uses: some paths keep the `CModifier` vtable at their destination. They stay unsupported until
  SDK-710 lands.

## Proof boundaries and gaps

The virtual trailer is followed only with a constructor-proven address point, an owner-derived
call target and the original owner, reader and token arguments. The shared member walk limits
depth and detects cycles. A store overlapping the owner vtable stops the path before virtual
resolution or member delegation. Every member walk forgets the result of `orr`, as it does for
`ubfx` and `and`. This permits the subsequent string reader for `apply_modifier_to_other_planets`
to join.

The description serializer constructor stores its wrapped owner at offset 8. A stack-object
member call with that proven wrapper resolves the description destination. A wrong receiver,
non-owner constructor argument or intervening store prevents this join. Stores inside the
serializer that overlap its wrapped-pointer slot also stop the path, including partial and
aliased stores; writes beside the slot do not invalidate it. The walker stops in the list loop
for `description_parameters`; the [parameter-list method](#fixed-key-readers-sdk-675) reads it
instead.

| Gap | Owner / missing fact |
| --- | --- |
| `modifier-name-implementations` / `modifier-name-reader` | Modifier method: a name implementation outside single inheritance, or a read slot that is not the shared forward (none on M452) |
| `parameter-list-shape`, `parameter-entry-shape`, `parameter-entry-body`, `parameter-entry-kind` | Parameter-list method: a serializer member, entry reader or accepted kind that the shapes do not establish (none on M452) |
| Numeric conversion | Shared numeric method: incomplete lexical and overflow properties |
| `modifier-numeric-member-body`, `modifier-numeric-insert-body`, `modifier-reference-member-body`, `modifier-reference-conversion-body` | A function that the build lacks (no single symbol of its name); a present body that does not match gives `-flow` or `-store` instead |

Missing default flow or reference anchors retain typed gaps. An ambiguous token name produces no
named field. Equal public reader identities must have equal blocks; both the population report
and installed-build parity test enforce this. Their negative controls reject a removed fixed key,
a changed reference kind and conflicting blocks under one identity.

Command fixed-key grammar is unchanged. Triggered modifier clauses embed these blocks; their join is on the [triggered
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
cargo parity modifier_blocks nested_owners
cargo live fixture_modifier_block
```

The sweep's `modifier_blocks` section reports fields and address points per reader, key differences
from the smallest variant, results (`fields_by_depth` separates root and nested fields), failure
shapes and unjoined persistent root `Block` fields (uses whose grammar is not covered, not proof
that each is a modifier). The live fixture reads all 13 fixed
keys, a numeric modifier, the scripted modifier `pop_job_amenities_mult` and the static modifier
`gave_up_pop` with complete parsing and no diagnostic. It gives `name` once as a scalar and once
as a block, which shows that a block value is skipped too. It does not test deferred completion or
application.
