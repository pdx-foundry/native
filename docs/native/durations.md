# Duration keys

`CommandGrammar.durations` groups the child keys of a command that set one duration count, such
as `days`, `months` and `years`, with each key's factor, how later keys combine, and the count
when no key is written. Registry field answers do not report durations;
`pdx_native::internals::duration_groups` runs the grouping over registry fields for population
runs. Consumers are outside the API; the retired consumer matcher is in Git at `d8f9d8a`.

A key is a duration unit only by mechanism. Only keys read by an integer or scoped numeric reader
are candidates. Candidates whose reader joins share one callee and final owner destination form a
group when at least one key applies a factor after its reader returns; integer stack temporaries
join at their final owner store. No key name, token or command selects a result. The group list
is `Known` only when the fixed keys are known, every key is joined, no candidate is unclassified
(its surrounding code cannot be followed), and no nested block at any depth may hold a group. The
live `stored_durations` of `check_script` uses the same rule.

## Shared factor: timed flags (M45-release)

The 28 `set_timed_*_flag` effects share `CSetTimedFlagEffect::ReadMember` (`0x101d99f80`).

- `days`, `months` and `years` all assign the scoped operand at `+0xa8` through
  `CVariableValue::Assign`.
- `months` then stores 30 and `years` stores 360 to `+0x2b0`. `days` is a tail call and stores
  nothing, so it keeps the factor of an earlier key. Each factory's `Create` stores 1 to `+0x2b0`.
- `CSetTimedFlagEffect::ExecuteActual` (`0x101d9a178`, effect vtable slot `+0x50`) passes the
  32-bit product of `CIntVariableValue::GetValue` and the factor to
  `CPdxIntegerFlags::SetFlag(…, CDate const&, int, ESetFlagMode)` in mode 0, with no overflow
  check.

So `months = 2 days = 3` stores operand 3 with factor 30 and executes as 90. A later key replaces
the operand only under the [scoped operand rules](scoped-numeric.md). `add_timed_trait` has the
same read shape but passes the product to `CLeader::AddTimedTrait`.

## Flag store countdown (M45-release)

- `CPdxIntegerFlags::SetFlag` (`0x1022a2688`), in mode 0, replaces an existing flag's date and
  count. Mode 1 keeps the earlier date and the larger count; the timed-flag effect does not use it.
- `CPdxIntegerFlags::UpdateFlags()` (`0x1022a29f4`) skips negative counts, decrements the others,
  and removes a flag when the decremented count is zero. Both bodies use the count array at
  `+0x40`; the method requires this agreement.

A positive count is removed on its count-th update. Zero becomes -1 and is never removed, and so
is any negative count. `UpdateFlags` is called from `CGameState::DailyUpdate` lambdas and many
owner `UpdateFlags` methods, so expiry depends on each owner's update frequency, which the method
does not establish. The retired world route observed one update per engine day for country flags
([ready-world](ready-world.md#observed-country-flag-countdown)).

## Scaled at read (M45-release)

`add_modifier` and `add_stage_modifier` read into owner `+0xb0` with `CReader::Read(int&)`.
`months` multiplies the slot in place by 30 (`lsl #5`, then `sub …, lsl #1`), and `years` by 360
(`mul`); `days` is a tail call. The last key read replaces the count, so `months = 2 days = 3`
gives 3. The multiplication is 32-bit, so a large `months` value wraps while it is read
(`months = 71582789` stores -2147483626).

## Constructor state on M451-hotfix

Owner derivation is shared with [scoped operands](scoped-numeric.md#owner-derivation). The
compiler summary establishes initial factor 1 for the 27 ordinary timed flags and
`add_timed_trait`; entered constructor bodies establish the relation flag's initial factor 1 and
every omitted count:

| Groups | Omitted count |
| --- | ---: |
| The 27 ordinary timed flags, `set_timed_relation_flag`, `add_timed_trait` | 0 |
| The 20 event effects' `months`/`years` literal word | 0 |
| `has_passed_resolution`'s `months`/`years` literal word | 0 |
| `add_modifier`, `add_stage_modifier` | -1 |

A later `CEventTarget` member calls `CStaticLexer::GetString` (`0x1004f6f10`) with only a token id
and `CEventTarget::PopulateTokenString` (`0x1004f6f44`) with its own address; neither is given an
earlier member's address, so the count word survives under the
[member-confined rule](scoped-numeric.md#assumption-code-changes-only-the-object-that-it-is-given).
`NTrigger::Create<CHasPassedResolutionTrigger>` (`0x1021e95b4`) builds both operands with
`CIntVariableValue::CIntVariableValue()` (`0x100d1cc74`) at owner `+0x68` and `+0x270`; that
constructor stores 0 at operand `+0x200` (`0x100d1ccc4`). The `mult` and `multiplier` operands of
the two modifier effects are signed 64-bit at scale 100000, and their literal words do not overlap
the count.

## Stack and execute facts on M451-hotfix

- `CFireEventEffect::ReadMember` (`0x101d27360`): the 20 event effects read `months` at
  `0x101d277b0` and `years` at `0x101d277cc` through `sp+0x1c0`, scale by 30 and 360, and store at
  owner `+0x2d8` (`0x101d277dc`): one scaled-at-read literal group per command.
- `CHasPassedResolutionTrigger::ReadMember` (`0x102224708`): `years` at `0x102224768` through
  `sp+8`, `months` at `0x102224794` through `sp+0xc`, factors 360 and 30, stored at owner `+0x470`.
- These stores overlap the shared scoped numeric layout: the event `days` operand is at `+0xd8`,
  whose literal is at operand `+0x200`, hence owner `+0x2d8`; the resolution `days` operand at
  `+0x270` places its literal at owner `+0x470`. Both `days` operands are signed 32-bit scoped
  integers. `months` and `years` overwrite the literal word but do not clear the operand's variable,
  script value, trigger, modifier or source location, so the method reports the literal groups and a
  `duration-scoped-literal` gap for each of the 21 commands.
- The event operand at `+0x2e0` is a separate random-delay operand, used as a bound when the first
  is positive.
- `CSetTimedRelationFlagEffect::ExecuteActual` evaluates operand `+0x3f8`, loads factor `+0x600` and
  passes the wrapping 32-bit product to `SetFlag` in mode 0.
  `CAddTimedTraitEffect::ExecuteActual` evaluates operand `+0xa8`, loads factor `+0x2e0` and passes
  the product to `CLeader::AddTimedTrait(CTrait const*, int)`.
- No group: `transfer_resources_to_empire.percentage` (operand `+0x268`, presence byte `+0x470`,
  `0x101de22ec`) and `while.count` (operand `+0x150`, presence byte `+0x358`, `0x101d31e00`) write
  only a presence byte; `CCouncilAgenda::ReadMember` resets words `+0x6d4` and `+0x6dc` (presence
  `+0x6d0`, `+0x6d8`) and `CAdvancedAuthoritySwap::ReadMember` words `+0x344`, `+0x34c` and `+0x354`
  before the integer reader overwrites them. A byte presence write does not establish a factor.

## Live parser observations

`tests/live/durations.rs` runs one `check_script` session in country, leader and astral rift
scopes; `tests/expected/duration-m45/live.json` holds the results. An omitted case must store
exactly the static omitted count. Live values confirm the static constructor counts; they do not
establish them. Pitfalls from these cases:

- `days = 7 days = x` is not a parser error for the scoped operand: it stores variable text and a
  source location, so selection moves away from the literal. The direct integer reader of
  `add_modifier` keeps 7 and reports `Malformed token`.
- A fraction (`days = 2.75`) is truncated silently by both readers.
- `set_timed_country_flag` lists stay partial because its `flag` key has no reader join.
- The stack cases observe literal storage only; they do not establish scoped selection.

## Modifier and trait consumers on M451-hotfix

Static findings that no method reports; input for later lint rules. None is the flag countdown.

| Shape | Reached by | Count behavior |
| --- | --- | --- |
| Modifier countdown | `add_modifier` in every scope except astral rift | Negative counts are never removed; 0 and 1 are both removed on the first update; n > 0 on the n-th. |
| No countdown found | `add_stage_modifier`; `add_modifier` in astral rift scope | Count stored, not consumed; the owner or stage sets the lifetime. |
| Trait expiry date | `add_timed_trait` | Removed at the first update where the wrapped expiry is at or before the current date. No count is permanent. |

**`time_multiplier`.** Both modifier effects use `CAddModifierEffect::ReadMember` (`0x101e5cac4`):
`time_multiplier` reads into operand `+0x2c0`, `mult` and `multiplier` into `+0xb8`. The constructor
(`0x101e5c984`) stores count -1 and builds both operands from `_VONE` (`0x102d9b0a0`, 1.0). Each
execute body (`0x101e5cd18`; stage `0x101e5dcb8` and `0x101e5dd3c`) passes the count unchanged when
the raw multiplier m is exactly 100000, else trunc(count × m / 100000) toward zero, cut to 32 bits.
The fast path (|count| ≤ 30370 and |m| ≤ `0xb504f333`) is exact; the slow path's 64-bit `mul` and
`madd` can wrap (`0x101e5cd7c`–`0x101e5cdac`): count -2000000000 with m 0.5 gives 844674407. So
m < 0 reverses a count's sign, and 0 < m < 1.0 turns -1 into 0.

**Modifier countdown.** `CTimedModifierCollection::AddTimedModifier` (`0x100979e84`) stores the
count in a 0x28-byte entry (modifier `+8`, count `+0x10`, multiplier `+0x18`, flag `+0x20`); for a
modifier already present it keeps the larger signed count (`0x100979ef0`), so a later -1 cannot
make a timed entry permanent. `DailyUpdate` (`0x100979aa4`) skips a negative count and removes the
entry when the count before the decrement was below 2, unsigned (`0x100979af4`).
`CollectModifier` (`0x100979c14`) ignores the count. Owner wrappers pass the count unchanged:

| Owner | Wrapper | Collection | Countdown call |
| --- | --- | --- | --- |
| Country | `0x100264bac` | `+0x1a40` | `0x1002453a8` |
| Pop group | `0x100a610c0` | `+0x398` | `0x100a56314` |
| Federation progression | `0x10052dee4` | `+0x1c8` | `0x10052c644`, `0x10052c688` |
| Galactic object | `0x10059a6d0` | `+0x660` | `0x1005841c8` |
| Megastructure | `0x101117820` | `+0x660` | `0x101117df0`, `0x101118120` |
| Planet, ship (`CColonyCarrier`) | `0x100fc036c` | planet `+0x2c8`, ship `+0x2e8` | planet `0x10114155c`, `0x101141618`, `0x101141b64`; ship `0x1011770f8` |
| Starbase fleet | `0x101016510` | `+0x2a8` | `0x101007f54` |
| Pop faction | `0x100a787a8` | `+0xc8` | `0x100a794e0` |
| Starbase | `0x100c27e48` | `+0x13d0` | `0x100c17030` |
| Cosmic storm field | `0x100fe5e00` | `+0x40` | `0x100fe56ac` |
| Spy network | effect `0x101e5cf00` | `+0x78` | `0x100c078f0` |
| Espionage operation | effect `0x101e5d184` | `+0xb0` | `0x10048acc8` |

The espionage countdown is skipped on two paths (`0x10048ac94`, `0x10048acc0`); the megastructure
and planet calls run only for a nonempty collection; `CFederationProgression::AddTimedModifier`
skips a `*_cooldown` modifier when a cheat-manager byte is set (`0x10052df40`); a non-starbase fleet
applies the modifier to each ship.

**No countdown found.** `add_stage_modifier` stores into espionage operation `+0xd0`
(`0x101e5ddfc`) or, through `CAstralRift::AddStageTimedModifier` (`0x100f3a574`), rift `+0x450`;
`add_modifier` in astral rift scope stores into rift `+0x430` (`0x100f3a4f4`). None of the 17 direct
countdown callers uses these offsets, the countdown is not virtual, and the executable holds one
copy of its body; a differently compiled update would not be found. `FinishCurrentStage`
(`0x10048b068`), `ResetCurrentStage` (`0x10048b680`) and `CAstralRift::FireAstralRiftEventById`
(`0x100f370d0`) clear the stage collections.

**Trait expiry date.** `CLeader::AddTimedTrait` (`0x1008ebb44`) stores `{trait, date}` at leader
`+0x9c8` with date = int32(now + int32(24 × count)) (`CStellarisDate::AddDays` `0x100c5e218`); adding
the trait again replaces the date. `CLeader::UpdateTimedTraits` (`0x1008e512c`) keeps a pair while
its date is after now. So 0 and small negatives expire on the first update, -178956968 adds 64
date units, and 536870912 adds nothing. `CAddTimedTraitEffect::PostValidate` (`0x101da6108`) logs
`Invalid or no duration for … effect.` when `CIntVariableValue::IsSet` (`0x100d1cdb8`) is false (an
omitted count, or a literal 0 with no variable); validation ignores the result.

## Gaps

The SDK-544 AC3 amendment accepts these limits: AC3 requires parser facts (readers, unit factors,
combination, omitted counts, accepted range, truncation and wrap), and consumer meaning only where
a shared method proves it.

- **Five `days`-only commands:** `add_casus_belli`, `add_intel_report`, `create_message`,
  `give_fleet` and `prolong_fleet_contract` have no factor sibling, so no group; no key-name rule
  is used. `add_intel_report` and `create_message` read an integer, the other three a scoped
  integer operand.
- **21 mixed scoped/literal readers:** the 20 event effects and `has_passed_resolution` keep the
  `duration-scoped-literal` gap. Removal needs a shared whole-operand proof and a public
  representation of the mixed selection.
- **Consumers and expiry:** answers carry no consumer, flag-update frequency or expiry date; event
  delays were not investigated.
- Duration-list completeness still follows unknown child dispatch, reader joins and nested blocks.

## Pitfalls

- A store before a tail call counts. A word reset is harmless only for a proved 4-byte integer
  reader at that destination. Narrow integer and scoped readers leave `duration-prefix-store`.
- An unscaled stack copy has factor 1 and cannot seed a duration group. A non-identity scale or
  a factor constant must establish the group; identity keys can join an established group.
- Indexed accesses invalidate their written-back base in the continuation walk. A later stack
  load cannot reuse the base's previous offset.
- Scoped literal overlap compares the word store's byte range with the literal's proved byte
  range. A store into the upper word of a 64-bit literal still overlaps; an adjacent word does
  not. Unknown literal widths retain `duration-scoped-literal`.
- Presence bytes must miss factor words and the proved count storage. For a shared scoped count,
  the protected span encloses its vtable, selection fields and literal. An overlap leaves
  `duration-byte-factor`; missing subtype, selection or width evidence leaves
  `duration-byte-storage`. Prefix and continuation writes obey the same check.
- Conditional presence writes need not occur on every path; their byte-write footprints are joined
  conservatively. Requiring equal footprints falsely leaves the council agenda resets unresolved.
- Do not carry flag-countdown meaning to another consumer. Zero is permanent in the flag store but
  expires on the first update in the modifier countdown and the trait date.
- A consumer can depend on the scope type at execution: `add_modifier` has no countdown in astral
  rift scope.
- Do not claim `SharedFactor` from the read paths alone. A constant store beside an operand is a
  multiplier only when the execute body multiplies that operand by that slot.
- Asking `command_grammar` once per command costs about one second per command. Run
  whole-inventory measurements through `internals::command_grammar_stops::population`, as
  `examples/duration-population.rs` does (SDK-651).
