# Shared modifier-block grammar

`registry_fields` attaches `FieldMembers::ModifierBlock` to root fields whose constructor proves
one modifier reader address point. `fixed_keys` contains named fields; `entries` contains the
numeric and static-modifier-reference forms. Each property independently reports `Known`,
`Partial` or `Unresolved`. There is no new operation or caller-side reader join.

## Exact-build result

Verified on M451-hotfix, executable
`29fa877366040a528098da39ec7e70b7baac76782a2a6bd161616d691f86fa38`, ARM64 slice
`2aeb9e15241bb114fd9f35a2dd09b454a5df6a0b1948b229d9eb83123e665c21`.
The four reader identities retain the read/member hash used by persistent fields:

| Reader identity | Member implementation | Fields | Address points | Fixed keys |
| --- | --- | ---: | ---: | ---: |
| `e327ea91dfe75d65` | `CModifier::ReadMember` | 1 | 1 | 2 |
| `5a74c67fe5a4adf9` | `CGraphicalModifier::ReadMember` | 2 | 1 | 8 |
| `ba5f8cddeba0d833` | `CStaticModifier::ReadMember` | 25 | 1 | 11 |
| `1c2988588f7e8eaa` | `CCustomDescriptionModifier::ReadMember` | 2 | 1 | 13 |

The full population covers 164 registries and 1,564 root fields. Thirty modifier fields in
17 registries share these four variants: **0 complete, 30 partial, 0 failed**. Every repeated
reader identity has the same block. Another 57 generic persistent Block fields have no concrete
reader join; these include weights and other non-modifier blocks.

All variants have partial fixed keys and two known entry forms. The smallest variant has
`name` (unknown reader) and `data` (integer). Graphical adds `icon`, `custom_tooltip` (string),
`icon_frame` (integer), `show_only_custom_tooltip`, `important`, `hide_from_country_list`
(boolean). Static adds `apply_modifier_to_other_planets` (string), `key` and
`divide_over_pop_groups` (unknown). Custom description adds `description` (string) and
`description_parameters` (unknown).

The compact selections are in `tests/expected/m45/modifier-blocks.json`. The field selections
refer to these variants by reader identity. These are static results on the hotfix image;
M45-release could not be verified because its executable is unavailable. The tracked
`tests/population/m45-release/` field baseline predates `registry-fields/v11` and remains unchanged.

Frozen captures of main `a51cfc8` and candidate `59b26bf` on the same installed build show
30 field-member changes across 17 registries, no removed fields or gaps, and zero command-answer
changes. Other field properties are unchanged. Reports and commit identities are retained under
`.local/population/sdk607/` (`main`, `candidate-v3`, `verified-summary.json`).

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
`description_parameters` still stops on reader routing or an unsupported instruction.

| Gap | Owner / missing fact |
| --- | --- |
| `modifier-name-reader` | Modifier method: polymorphic name reader not established |
| `reader-routing` on `key` | Modifier method: key taken from token text and passed to a setter |
| `reader-routing` on `divide_over_pop_groups` | Modifier method: temporary stack Boolean followed by bit storage |
| `reader-routing` / `instruction` on `description_parameters` | Shared walker: parameter-list loop and nested serializer |
| Numeric conversion | Shared numeric method: incomplete lexical and overflow properties |
| Unresolved storage | Registry fields: repeated-block behavior remains unknown |

Missing default flow or reference anchors retain typed gaps. An ambiguous token name produces no
named field. Equal public reader identities must have equal blocks; both the population report
and installed-build parity test enforce this. Their negative controls reject a removed fixed key,
a changed reference kind and conflicting blocks under one identity.

Root fields without persistent joins, nested `tradition_swap.modifier`, command fixed-key grammar
and triggered modifier clauses (SDK-673) are unchanged. This answer does not establish repeat or
duplicate behavior, icon post-read validation, deferred completion, localisation-key existence,
authored-entry storage, or runtime effects (SDK-547). Historical findings remain on the
[prototype page](modifier-family-prototype.md).

## Reproduce

Use the same executable on both sides and the detached-worktree procedure in
[method authoring](method-authoring.md#capture-a-stable-main-and-branch-pair):

```sh
cargo run --release --example registry-field-sweep -- "$STELLARIS_PATH"
cargo run --release --example command-population -- "$STELLARIS_PATH"
cargo parity modifier_blocks
cargo live fixture_modifier_block
```

The live fixture passed on this build in 37 seconds: all 13 reported fixed keys, a numeric
modifier, the scripted modifier `pop_job_amenities_mult` and the static modifier `gave_up_pop`
completed one observed reader invocation with complete parsing and no immediate diagnostics.
Disposal was confirmed; no game or debugger process remained. This does not test deferred
completion or runtime application.

The sweep's `modifier_blocks` section reports fields and distinct address-point counts per reader,
key differences from the smallest variant, complete/partial/failed counts, failure shapes and
unjoined persistent Block fields. The latter are uses whose grammar is not covered, not proof
that each use is a modifier. Failed point joins retain their existing unresolved answer.
