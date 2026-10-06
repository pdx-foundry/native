# Shared weight-block grammar

`registry_fields` attaches `FieldMembers::WeightBlock` to root fields whose constructor proves one
weight reader address point, and sets the field's `BlockFamily::Weight`. The block reports the bare
value form, fixed keys, arithmetic operations, how a further operation key is stored, and what other
keys are; a nested `modifier`, `scaled_modifier` or `complex_trigger_modifier` key carries its own
`WeightBlock`. Source stamp `registry-fields/v18`. The method is
`src/engine/analysis/weight_blocks.rs`, bound in `src/binding/binary/weight_blocks.rs` and
normalized in `src/session/weight_blocks.rs`; its module comment states the acceptance shapes and
the operation rule. The field's read scope comes from a constructor-stored word that
`fields/persistent.rs` reads on request.

## Engine facts (M451-hotfix)

Executable `29fa877366040a528098da39ec7e70b7baac76782a2a6bd161616d691f86fa38`, read with
`examples/inspect`.

**Readers.** The anchor is `CMeanTimeToHappen::Read(CReader&)` (`0x10092e5d8`). Three vtable
address points pair it with `CMeanTimeToHappen::ReadMember(CReader&, int)` (`0x10092e62c`), reader
identity `f08cb83d92484a89`. One pairs it with `CAIMTTHChance::ReadMember(CReader&, int)`
(`0x10092f188`), identity `fd8c6ad9ff94a8f2`, used by `common/country_customization.weight`: its
`factor` reads a fixed-point value into `+0x38` instead of scaling `base`.

**Read entry.** When the value token's kind is `0xc`, the anchor reads a bare fixed-point value into
`base` (`+0x10`), as in `ai_weight = 7`; otherwise it tail-calls `CPersistent::Read`. It does not
write the stored scope at `+0x30`.

**Member reader.** The token is in `w2`.

| Token | Key | Behavior |
| --- | --- | --- |
| `0x2ef2` | `base` | tail call `CReader::Read(CFixedPoint&)` into `+0x10` |
| `0x2c89`–`0x2c8b` | `days`, `months`, `years` | jump table; `CToken::GetInt()` on the value token, times 1, 30 or 360, stored to `+0x10` |
| `0x2c8c` | `factor` | `+0x10 == 0`: `Read(CFixedPoint&)` into `+0x10`; otherwise `CToken::GetFloat()` (a raw fixed-point value) multiplies `+0x10` |
| `0x3fff` | `modifier` | `new(0x2f8)`, inline `CTriggerMTTHModifier` vtables, virtual `CTrigger::Read(CReader&, EScopeType)` with the stored scope, `InsertAtEmplace` into the array at `+0x18` |
| `0x40b6` | `scaled_modifier` | `new(0x2a0)`, inline vtable, virtual `CScaledMTTHModifier::Read(CReader&)`, insert |
| `0x40b7` | `complex_trigger_modifier` | `new(0x308)`, constructor, virtual `CComplexTriggerMTTHModifier::Read(CReader&)`, insert |
| other | operations | `EScriptMaths TokenToEnum<EScriptMaths>(int const&)` (`0x1006c554c`) on the key token |

A key that the switch does not name (value `0x10`) logs `unknown command '…' for MTTH/script value
in file … line : N` and stores nothing. Every operation inserts a `CTriggerMTTHModifier` with an
always-true root trigger, so operations accumulate. The six operations in mask `0xb380` read no
value; the others read `CVariableValue::Read(CReader&, EScopeType)` with the stored scope.

**Operations.** The switch maps 19 spellings to 16 values; this is the "nineteen token spellings /
sixteen operations" of the council agenda prototype:

| Value | Spellings | Value | Spellings |
| ---: | --- | ---: | --- |
| 0 | `weight`, `set` | 8 | `floor` |
| 1 | `add` | 9 | `ceiling` |
| 2 | `subtract` | 10 | `max` |
| 3 | `factor`, `mult`, `multiply` | 11 | `min` |
| 4 | `divide` | 12 | `abs` |
| 5 | `modulo` | 13 | `square` |
| 6 | `round_to` | 14 | `pow` |
| 7 | `round` | 15 | `square_root` |

Values 7, 8, 9, 12, 13 and 15 take no operand. The top-level fixed `factor` key shadows the
operation spelling, so the outer block reports 18 operation keys and the `modifier` entry all 19.

**Entry reader.** `CTriggerMTTHModifier::ReadMember(CReader&, int, EScopeType)` (`0x10092af3c`)
keeps one operation slot at `+0x2f0`. A second operation logs `script_value/MTTH/weight field:
cannot reassign op` and replaces the first. `desc` (token `0x2c9f`) is a string. Every other key
goes to `CAndTrigger::ReadMember` in the received scope: `id` is a string, and the rest are trigger
conditions.

**Scaled modifier.** `CScaledMTTHModifier::ReadMember(CReader&, int)` (`0x100930fb0`); token in
`w2`, value token at reader `+0x278`.

| Token | Key | Behavior | Answer |
| --- | --- | --- | --- |
| `0x2c8d` | `scope` | `CToken(CToken const&)` of the value token, `CEventTarget::CEventTarget(CToken)`, `CEventTarget::operator=` into `+0x28` | `Target` |
| `0x34ee` | `calc` | the value token is compared with `0x2d36`, `0x34f2`, `0x47d2` and `0x47d3` (`0x47d1` is only a pivot of the compare tree); each stores a byte constant at `+0x29a`; another value tail-calls `CReader::ReportMalformed` | `Keyword` |
| `0x3fc7` | `limit` | tail call `CTrigger::Read(CReader&, EScopeType)` on `+0x1e0` with scope `0` | `Block`, trigger family |
| `0x2e2f`, `0x34f3`, `0x34f4` | `add`, `mul`, `div` | `Read(CFixedPoint&)` into `+0x10`, `+0x20`, `+0x18`; `div` of zero logs `scaled_modifier: cant divide by zero` | `FixedPoint` |
| other | | `CPersistent::ReadMember` of the base at `+8` | rejected |

**Complex trigger modifier.** `CComplexTriggerMTTHModifier::ReadMember(CReader&, int)`
(`0x10092cfd0`).

| Token | Key | Behavior | Answer |
| --- | --- | --- | --- |
| `0x2ca2` | `trigger` | `__assign_external` of the value token's text into `+0x60`, then `CTriggerDatabase::CreateTriggerOrScriptedPlaceholder(CToken const&, CString const&)` with the value token; the result goes to `+0x88` | `Reference`, target `Triggers` |
| `0x3377` | `parameters` | `+0x88` null: logs `specify trigger before parameters`; otherwise a tail branch through vtable slot `+0x30` of the `+0x88` object, with the reader and scope `0` | gap: read by the object that `trigger` stores |
| `0x3378` | `trigger_scope` | the `scope` event-target chain into `+0x148` | `Target` |
| `0x399e` | `mode` | `TokenToEnum<EScriptMaths>` on the **value** token, stored at `+0x2e0` | `Keyword` |
| `0x2d21` | `potential` | tail call `ReadTrigger<CRootTrigger>` on `+0x90` with scope `0` | `Block`, trigger family |
| `0x2c9f` | `desc` | `Read(CString&, bool)` | `String` |
| `0x2d5c`, `0x3c59`, `0x2d5d`, `0x2d5e` | `multiplier`, `mult`, `min_value`, `max_value` | `Read(CFixedPoint&)`; the last two set a flag at `+0x2e8` first | `FixedPoint` |
| other | | `CPersistent::ReadMember` of the base at `+8` | rejected |

`CreateTriggerOrScriptedPlaceholder` searches the trigger database by token and calls the found
trigger's factory; for a name that no trigger has, it makes a `CScriptedTrigger` with that name.
`mode` stores `0x10` for a name that the switch does not know, without a diagnostic.

**Entry read entries.** `CScaledMTTHModifier::Read` and `CComplexTriggerMTTHModifier::Read` move
`GetFileLocationDescription` into the owner (`+0x1b8`, `+0x38`) and call `CPersistent::Read` on the
base at `+8`. Neither checks the value token, so a bare value is not converted and parsing goes on
as a block. `CTrigger::Read(CReader&, EScopeType)`, the `modifier` entry's read, logs `Expected "k
= {", but got "k = v" at <location>` through `CLogger` for a value that is not a block, and then
reads as a block. All three entries report `scalar: None`.

**Later scope checks.** `CScaledMTTHModifier::ValidateScope` checks the `scope` target chain and then
`limit` in the target's scope type; `CComplexTriggerMTTHModifier::ValidateScope` checks `potential`
in the received scope, `trigger_scope`, and the trigger through vtable slot `+0x90`. The method does
not read these functions; the read scope of `limit` and `potential` stays a zero-mask gap.

**Stored scope.** `CMeanTimeToHappen(EScopeType, CFixedPoint, bool)` stores `x1` (the scope) at
`+0x30` and `x2` (the default base) at `+0x10`. The council agenda, tradition and tradition
category constructors pass `4` (country) and `_VHUNDRED`. An omitted `base` keeps that default
without a diagnostic.

## Result on M451-hotfix

Seventy-three fields in the population of 164 registries have a weight reader: 71 share
`f08cb83d92484a89` and two use `fd8c6ad9ff94a8f2`. All are **partial: 0 complete, 73 partial, 0
failed**; equal identities have equal blocks, for the roots and for the three nested entry readers
(five identities). The compact selections are in `tests/expected/m452/weight-blocks.json`, where
field selections refer to them by reader identity.
The read scope is the stored scope: agenda and tradition `ai_weight` read in `country`. A key or
condition that reads the block's own stored scope reports `Enclosing`, so the grammar does not
depend on the owner.

Failure shapes, by field count:

| Shape | Fields |
| --- | ---: |
| Numeric and scoped-literal conversion limits ([numeric conversion](numeric-conversion.md), [scoped numeric](scoped-numeric.md)) | 73 |
| Zero-mask read scope of `scaled_modifier`, its `limit`, `complex_trigger_modifier` and its `potential` | 73 |
| Keyword domain of `calc` and `mode` (`ReaderSemantics`, SDK-627) | 73 |
| `trigger` lookup stage and match, and the scripted-trigger placeholder (`ReaderSemantics`) | 73 |
| `parameters` read by the object that `trigger` stores | 73 |
| Field repeat behavior (`Repeat behavior or nested fields remain unresolved`) | 73 |

Nine root fields also have a zero-mask read scope of their own, because their owner constructor
stores scope `0`: the five weight fields of `common/buildings`, `planet_damage` in
`common/bombardment_stances`, `weight_modifier` in `common/colony_types`, and `random_weight` in
`common/ethics` and `common/governments/authorities`.

Thirteen fields with weight names are persistent blocks without a constructor-proven reader, so the
method does not see them: `ai_weight` in `common/federation_laws`, `common/federation_types`,
`common/governments/civics`, `common/leader_classes` and `common/resolutions`; `random_weight` in
civics; `ai_location_weight` in leader classes; `weight` in `common/galactic_focuses`,
`common/pop_categories` and `common/pop_jobs`; `network_weight` and `strategy_weight` in
`common/ai_espionage/spynetworks`; `spawn_weight` in `common/storm_types`. The owner constructors of
`ai_weight` in `common/war_goals` and `weight_modifier` in `common/personalities` run `ld1r`; they
join since the shared evaluator runs it (SDK-547). The sweep lists them under
`modifier_blocks.failed_persistent_fields`.

## Gaps

- `factor` keeps both of its readers with an unresolved condition. Both read a fixed-point value,
  so the key reports `FixedPoint` with no reader identity.
- `calc` and `mode` are `Keyword`; their domains stay `Unknown` (SDK-627).
- The read scope of `limit` and `potential` stays unresolved: the engine reads them with scope mask
  0, and a zero mask never establishes a scope.
- `parameters` is read by the trigger that `trigger` names; its grammar is that trigger's, which
  the method does not follow.
- Repeat behavior of a whole weight field stays `Unknown`: a repeated block replaces `base` and
  keeps earlier entries.
- Weight evaluation, operation semantics and the default base value are outside the method.

## Pitfalls

- **Operations are an inference with checks.** The method calls an accepted key an operation when
  a word written after the switch returned holds the switch's value. A word that holds the same
  value by chance satisfies it, most likely `0` for `weight` and `set`. The parity test checks the
  result against the 19 spellings, the six operand-free operations and the disjoint key sets.
- **A keyword needs a second pass.** The key pass leaves the value token unknown, so `calc` and
  `mode` have no accepted shape there. The method reruns only such a key for every value token;
  the pass costs about a quarter of a second for the whole build. A value that equals the key
  token is still a value: the switch's role comes from the pass, never from a numeric match, so
  `mode` is not an operation.
- **A dependent reader is named, not followed.** `parameters` branches through a function pointer
  of the object at `+0x88`; the path stops there, and the method names the key whose stored
  destination is that word.
- **A bare nested value is not rejected.** `CPersistent::Read` has no scalar branch, so
  `scalar: None` means that no bare value is read, not that the engine reports one.
- **The domain is every token value.** The method evaluates each value from zero to the largest
  literal token and the first value after it, as `scopes.rs` does. Sampling only the literal
  tokens would miss an interior value that a compare tree accepts.
- **A log call is not a rejection.** The entry reader logs a reassignment and then stores the
  operation, so a key is rejected only when a path returns with no accepted shape after a
  diagnostic.
- **The unknown-key message is not source-joined.** The fixture joins a log line to its source by
  `file: ` and ` line: `; `meantimetohappen.cpp:853` writes `in file … line : N`, so the message
  appears only in `error.log`. The live control uses a malformed `base` instead, which
  `CReader::ReportMalformed` reports on its line.
- **An entry's scope is relative.** `spawn_chance` and `overlord_weight` share a reader but their
  owners store different scopes, so the absolute scope belongs on `Field.read_scope`, never in the
  shared grammar.

## Reproduce

```sh
cargo run --release --example registry-field-sweep -- "$STELLARIS_PATH"
cargo parity weight_blocks
cargo live fixture_weight_block
cargo live fixture_control_weight_scope
```

The sweep's `weight_blocks` section gives fields by status and failure shapes per reader identity.
`fixture_weight_block` parses `ai_weight = { add = 2 modifier = { factor = 0.5 always = yes } }`,
with no `base`, completely and without a diagnostic; the malformed `base` control gets a
`reader-malformed-report` on its line. `fixture_control_weight_scope` checks that a planet trigger
in a weight `modifier` logs `Current Scope: country` on its line.
