# Duration keys (SDK-646)

`CommandGrammar.durations` groups the child keys of a command that set one duration count, such
as `days`, `months` and `years`. It gives each key's factor, how later keys combine with earlier
ones, the count when no key is written, and what consumes the count. The source stamp is
`command-grammar/v12`. The method is `src/engine/analysis/durations.rs`, bound by
`src/binding/binary/durations.rs` and normalized by `src/session/durations.rs`. Registry field
answers do not report durations; `pdx_native::internals::duration_groups` runs the same grouping
over registry fields for the population run.

A key is a duration unit only by mechanism. Only keys read by an integer or scoped numeric reader
are candidates. Candidates whose reader joins share one callee and final owner destination form
a group when at least one key applies a factor after its reader returns. Integer stack temporaries
join at their final owner store. A scoped operand overlapping a scaled literal group leaves a typed
`duration-scoped-literal` gap; its selection rules are not inferred from literal storage. No key
name, token or command selects a result.

The list of groups is `Known` only when the fixed keys are known, every key is joined, no candidate
is left unclassified, and no nested block, at any depth, may hold a group. A candidate is
unclassified when the code around its reader call cannot be followed. The live table of
`check_script` uses the same rule.

## Engine facts on M45-release

These facts are from executable `07988b4f1b865623becd7a61af1cae92e111be6515d341754af70f02107822cd`
(ARM64 slice `a4cb49ad17a84ef6bf438019a50d3a66362c80731f8359888ddbce47c0d0aab9`). The canonical
lines of the three consumption bodies are retained in `.local/sdk-646/canonical-lines.txt`.

### Shared factor: timed flags

The 28 `set_timed_*_flag` effects share `CSetTimedFlagEffect::ReadMember` (`0x101d99f80`).

- `days`, `months` and `years` all assign the scoped operand at `+0xa8` through
  `CVariableValue::Assign`.
- `months` then stores 30 and `years` stores 360 to `+0x2b0`. `days` is a tail call and stores
  nothing, so it keeps the factor of an earlier key.
- Each factory's `Create` stores 1 to `+0x2b0`.
- `CSetTimedFlagEffect::ExecuteActual` (`0x101d9a178`, effect vtable slot `+0x50`) passes the
  32-bit product of `CIntVariableValue::GetValue` of the operand and the factor to
  `CPdxIntegerFlags::SetFlag(…, CDate const&, int, ESetFlagMode)` in mode 0. The product is not
  checked for overflow.

So `months = 2 days = 3` stores the operand 3 with factor 30, and executes as 90. This agrees with
the SDK-493 trace (literal 3, multiplier 30). A later key replaces the operand only under the
[scoped operand rules](scoped-numeric.md): a literal after a reference keeps the reference's source
location.

`add_timed_trait` has the same read shape, but its execute body passes the product to
`CLeader::AddTimedTrait` rather than the flag store. Its M451-hotfix offsets and consumption limit
are given under [remaining execute bodies](#remaining-execute-bodies).

### Flag store countdown

- `CPdxIntegerFlags::SetFlag` (`0x1022a2688`), in mode 0, replaces an existing flag's date and
  count. Mode 1 keeps the earlier date and the larger count; the timed-flag effect does not use it.
- `CPdxIntegerFlags::UpdateFlags()` (`0x1022a29f4`) skips negative counts. It decrements the other
  counts and removes a flag when the decremented count is zero.
- Both bodies use the count array at `+0x40`. The method requires this agreement.

A positive count is therefore removed on its count-th update. Zero becomes -1 and is never
removed, and so is any negative count. The country live omitted case observes operand 0 and
factor 1, whose product is permanent under this countdown. The static method does not establish
omitted counts from constructor bytes.
The expiry date also depends on how often each owner's update runs. `UpdateFlags` is called from
`CGameState::DailyUpdate` lambdas and from many owner `UpdateFlags` methods, and the method does
not establish that frequency. [SDK-650](https://linear.app/unnamed-system/issue/SDK-650) owns the
world route that runs a flag to expiry. Its [4.5.1 country observations](ready-world.md)
confirmed one countdown update per engine day for the five tested cases.

### Scaled at read

`add_modifier` and `add_stage_modifier` read into owner `+0xb0` with `CReader::Read(int&)`.
`months` multiplies the slot in place by 30 (`lsl #5`, then `sub …, lsl #1`), and `years` by 360
(`mul`). `days` is a tail call. The last key read replaces the count, so `months = 2 days = 3`
gives 3. The multiplication is 32-bit, so a large `months` value wraps while it is read.
Consumption, including `add_modifier`'s `time_multiplier`, is outside this method. The
consumers are recorded under [modifier and trait consumers](#modifier-and-trait-consumers-on-m451-hotfix-sdk-672).

## Constructor state on M451-hotfix

The exact build is the M451-hotfix executable in [targets](targets.md). Constructor owner
derivation is shared with [scoped operands](scoped-numeric.md#owner-derivation-sdk-658-sdk-660). A
constructor-written byte is claimed only when every returning path agrees on it after every later
write that may change it; old observed values do not initialize the method's byte map.

The compiler-summary path establishes initial factor 1 for the 27 ordinary timed flags and
`add_timed_trait`. Entered constructor bodies establish the relation flag's initial factor 1 and
the omitted count of all 52 groups, under the
[member-confined rule](scoped-numeric.md#assumption-code-changes-only-the-object-that-it-is-given)
(SDK-660):

| Groups | Omitted count |
| --- | ---: |
| The 27 ordinary timed flags, `set_timed_relation_flag`, `add_timed_trait` | 0 |
| The 20 event effects' `months`/`years` literal word | 0 |
| `has_passed_resolution`'s `months`/`years` literal word | 0 |
| `add_modifier`, `add_stage_modifier` | -1 |

A later `CEventTarget` member calls `CStaticLexer::GetString` `0x1004f6f10` with only a token id,
and `CEventTarget::PopulateTokenString` `0x1004f6f44` with the event target's own address. Neither
is given an earlier member's address, so the count word survives. Under SDK-658's escape rule,
these calls lost the count word of 49 groups (see the
[superseded obstacle table](scoped-numeric.md#constructor-obstacles-under-the-escape-rule-superseded-by-sdk-660)).
`has_passed_resolution`'s factory, `NTrigger::Create<CHasPassedResolutionTrigger>` `0x1021e95b4`,
builds both operands with `CIntVariableValue::CIntVariableValue()` `0x100d1cc74`, at owner `+0x68`
and `+0x270`. That constructor stores 0 at operand `+0x200` (`0x100d1ccc4`) after it builds its
`CEventTarget` member. Under the escape rule, the trace named the factory's `_bzero`,
`CTrigger::CTrigger()` and both operand constructor calls (`0x1021e95d4`-`0x1021e9618`) as places
that lost the word at `+0x470`.

The modifier operands `mult` and `multiplier` are signed 64-bit at scale 100000 in both
`add_modifier` and `add_stage_modifier`. Their literal words do not overlap the duration count, so
neither group has the `duration-scoped-literal` gap.

## Stack and execute facts on M451-hotfix (SDK-657)

These facts apply to executable `29fa877366040a528098da39ec7e70b7baac76782a2a6bd161616d691f86fa38`,
ARM64 slice `2aeb9e15241bb114fd9f35a2dd09b454a5df6a0b1948b229d9eb83123e665c21`.

### Stack temporaries and scoped literals

- `CFireEventEffect::ReadMember` starts at `0x101d27360`. The 20 event effects read `months`
  at `0x101d277b0` and `years` at `0x101d277cc`, through `sp+0x1c0`. They scale by 30 and 360,
  respectively, then store at owner `+0x2d8` (`0x101d277dc`). These keys form one scaled-at-read
  literal group per command. Its omitted literal is 0 (see [constructor state](#constructor-state-on-m451-hotfix)).
- `CHasPassedResolutionTrigger::ReadMember` starts at `0x102224708`. `years` reads at
  `0x102224768` through `sp+8`, and `months` at `0x102224794` through `sp+0xc`. Their factors
  are 360 and 30; both store at owner `+0x470`. Its omitted literal is 0.
- These stores overlap the **shared numeric scoped layout**, rather than proved independent
  scalar duration members.
  The event `days` path reads the operand at `+0xd8`; the shared numeric layout places a literal
  at operand `+0x200`, hence owner `+0x2d8`. The resolution `days` operand is at `+0x270`, whose
  possible literal is owner `+0x470` under that layout. The command's concrete subtype is not
  established by its constructor state. The shared selection bodies establish the common literal
  offset independently of constructor bytes; this is a possible overlap, not a subtype recovery.
  The event operand at `+0x2e0` is the separate random-delay operand, not the `days` operand.
  `CFireEventEffect::ExecuteActual` evaluates both `+0xd8` and `+0x2e0`, and uses the latter
  as a random-delay bound when the first is positive. Runtime event delays remain outside this ticket.

`months` and `years` overwrite the literal word, but do not clear the operand's variable,
script value, trigger, modifier or source location. A scoped `days` read preserves its own
selection rules. The current public combinations do not describe this mix of scoped selection
and scaled literal stores. The method reports the literal groups and a conservative `ReaderSemantics` gap
(`duration-scoped-literal`) for each of the 21 commands. Their duration lists are partial in both
static and live answers. `days` is not folded into those groups. Removal requires a shared proof
that joins the whole scoped operand and reports the mixed selection behavior; a change to the
public combination type needs the lead's decision. The parent criterion is not met for this mix.

### Remaining execute bodies

Both bodies match complete canonical shapes, including their validity guards and consumer calls:

- `CSetTimedRelationFlagEffect::ExecuteActual` resolves `who`, creates or accesses the country
  relation, and checks its validity. It evaluates operand `+0x3f8`, loads factor `+0x600`, and
  passes their wrapping signed 32-bit product to `CPdxIntegerFlags::SetFlag` in mode zero.
  Its flag consumer is `FlagCountdown`. The initial factor is unresolved, so the combination
  has `duration-initial-state` and its omitted count is unresolved. Relation update frequency is
  outside the static method.
- `CAddTimedTraitEffect::ExecuteActual` accesses and checks a leader, evaluates operand `+0xa8`,
  loads factor `+0x2e0`, and passes their wrapping signed 32-bit product to
  `CLeader::AddTimedTrait(CTrait const*, int)`. Its combination is the same shared-factor form
  with initial factor 1; its omitted count is unresolved. This is not a flag store. Trait
  consumption remains an `OutsideMethod` limit; no flag-countdown claim is made. The trait
  consumer is recorded under [modifier and trait consumers](#modifier-and-trait-consumers-on-m451-hotfix-sdk-672).

### Candidates without a group

- `transfer_resources_to_empire.percentage` reads the scoped operand at `+0x268`, then writes
  byte one at `+0x470` (`0x101de22ec`). `while.count` reads the scoped operand at `+0x150`, then
  writes byte one at `+0x358` (`0x101d31e00`). Neither path scales the count or stores a word
  multiplier. A byte presence write alone does not establish a duration factor.
- `CCouncilAgenda::ReadMember` resets words `+0x6d4` and `+0x6dc` to zero before tail-calling
  the integer reader into those same words. Presence bytes are `+0x6d0` and `+0x6d8`.
- `CAdvancedAuthoritySwap::ReadMember` does the same for words `+0x344`, `+0x34c` and `+0x354`,
  with presence bytes `+0x340`, `+0x348` and `+0x350`.

The reader overwrites each word reset; none of these paths applies a factor. The two commands
and both registry candidates have no group and no unclassified duration candidate. Byte writes
remain recorded during grouping: an overlap with a sibling's factor or count word is a typed
`duration-byte-factor` gap, so a byte reset cannot silently mean factor preservation.

### Population on M451-hotfix

The population covers all 2,170 commands and all 164 registries, with no failed question.

| Population | Groups | Complete | Partial | Failed |
| --- | ---: | ---: | ---: | ---: |
| Commands | 52 | 28 | 24 | 0 |
| Registry fields and nested collections | 0 | 0 | 0 | 0 |

Unit factors, read combinations and omitted counts are known for all 52 groups. The 28 complete
groups are the 27 ordinary timed flags and `set_timed_relation_flag`. Failure shapes count groups
and can overlap:

- 28 static flag-update-frequency limits;
- 24 consumption limits;
- 21 possible mixed scoped/literal-selection gaps (`duration-scoped-literal`).

Duration lists are 627 known, 517 partial and 1,026 unresolved. `add_modifier` and
`add_stage_modifier` became known, and one other list became partial. No command or registry
candidate has an unclassified prefix or continuation. **26 commands have uncovered unit-named
keys**: 21 have the scoped `days` mix above, and five have `days` without a factor sibling. The 21
groups cover 42 `months` and `years` keys. These gaps do not amend SDK-544. Before/after counts are
in the [discovery index](discovery.md#member-confined-calls-sdk-660). Run
`cargo run --release --example duration-population` with `STELLARIS_PATH`;
`.local/sdk-660/after/duration.json` holds each answer.

## Live parser observations

`Game::check_script` reports `stored_durations` for each top-level child after reading, before
validation. Nothing is executed. `tests/live/durations.rs` runs one session with effect and trigger
checks in country, leader and astral rift scopes. `tests/expected/duration-m45/live.json` holds the
reviewed results. Every case classifies one child. The 29 cases with decoded counts agree with the
static storage proofs. Each `omitted` case must store exactly one count, and the test fails unless
it equals the static omitted count of the same group. The relation case `months = 2 days = 3`
stores count 3 with factor 30: the later `days` replaces the count but keeps the factor. Omitted
live values confirm the static constructor counts; they do not establish them. The `add_modifier`
and `add_stage_modifier` answers are complete. The `set_timed_country_flag` lists are partial,
because the static list is partial: its `flag` key has no reader join, so the method cannot rule
out another group.

| `set_timed_country_flag` input | Operand literal | Factor | Other storage | Diagnostics |
| --- | ---: | ---: | --- | --- |
| omitted | 0 | 1 | | none |
| `days = 7` | 7 | 1 | | none |
| `months = 2` | 2 | 30 | | none |
| `years = 2` | 2 | 360 | | none |
| `months = 2 days = 3` | 3 | 30 | | none |
| `days = 0`, `days = -1` | 0, -1 | 1 | | none |
| `days = 2147483647` | 2147483647 | 1 | | none |
| `years = 5965233` | 5965233 | 360 | product overflows at execution | none |
| `days = 2.75` | 2 | 1 | | none |
| `days = 7 days = x` | 7 | 1 | variable `x`, source location set | none |
| `months = value:native_missing_value days = 3` | 3 | 30 | script value stored, source location set | validation: invalid script value |
| `days = native_variable days = 4` | 4 | 1 | variable kept, source location set | none |

| `add_modifier` input | Count | Diagnostics |
| --- | ---: | --- |
| omitted | -1 | none |
| `days = 7`, `months = 2`, `years = 2` | 7, 60, 720 | none |
| `months = 2 days = 3` | 3 | none |
| `days = -1`, `days = 2.75` | -1, 2 | none |
| `months = 71582789` | -2147483626 | none; the product wraps while it is read |
| `days = 7 days = x` | 7 | read: `Malformed token: x` |

The observations separate storage from acceptance:
- `days = x` after a literal is not a parser error for the scoped operand. It stores variable text
  and a source location, so selection moves away from the literal (see
  [scoped numeric](scoped-numeric.md)).
- The direct integer reader keeps 7 and reports `Malformed token`.
- A fraction is truncated silently by both readers.

The M451-hotfix live table also covers these seven shapes:

| Input shape | Observed storage | Diagnostics |
| --- | --- | --- |
| `country_event`, `months = 2 years = 1` | literal word 360, partial group list | validation: deliberately missing event |
| `has_passed_resolution`, `months = 2 years = 1` | literal word 360, partial group list | none |
| `set_timed_relation_flag`, `months = 2 days = 3` | partial empty duration list; initial factor unproved | none |
| `add_timed_trait`, `months = 2 days = 3` | scoped literal 3, factor 30, known group list | none |
| `country_event`, omitted | literal word 0, the static omitted count | validation: deliberately missing event |
| `add_stage_modifier`, omitted, astral rift scope | count -1, the static omitted count, known group list | none |
| `has_passed_resolution`, omitted | literal word 0, the static omitted count | none |

The stack cases observe literal storage only; they do not establish scoped selection or execute
an event or trigger. The trait case reads in leader scope. No new runtime meaning is inferred.

## Modifier and trait consumers on M451-hotfix (SDK-672)

These are static findings on the M451-hotfix executable. No method reports them; the public
answer keeps an `OutsideMethod` consumption limit for these three groups. They are recorded as
documentation and as input for later lint rules. None of them is the flag countdown.

| Shape | Reached by | Form | Count behavior |
| --- | --- | --- | --- |
| Modifier countdown | `add_modifier` in every scope except astral rift | Countdown | Every negative count is never removed. 0 and 1 are both removed on the first update; n > 0 on the n-th. |
| No countdown found | `add_stage_modifier`; `add_modifier` in astral rift scope | Count stored, not consumed | No count has special handling. The owner or stage sets the lifetime. |
| Trait expiry date | `add_timed_trait` | Expiry date | Removed at the first update where the wrapped expiry is at or before the current date. No count is permanent. |

### Shared modifier path and `time_multiplier`

Both modifier effects use `CAddModifierEffect::ReadMember` (`0x101e5cac4`, the same slot in both
vtables). `time_multiplier` reads into operand `+0x2c0`; `mult` and `multiplier` read into
`+0xb8`. The constructor (`0x101e5c984`) stores count -1 and builds both operands from `_VONE`
(`0x102d9b0a0`, raw 100000, so 1.0).

Each execute body (`0x101e5cd18`; stage `0x101e5dcb8` and `0x101e5dd3c`) evaluates the raw
multiplier m. When m equals 100000 exactly, the count goes to the consumer unchanged. Otherwise
the count becomes trunc(count × m / 100000), toward zero, and the consumer reads the low 32 bits.
The fast path (|count| ≤ 30370 and |m| ≤ `0xb504f333`) is exact. The slow path splits the signed
larger of count × 100000 and m, and its 64-bit `mul` and `madd` can wrap (`0x101e5cd7c`–`0x101e5cdac`):
count -2000000000 with m 0.5 gives 844674407, not -1000000000. `GetDays` (`0x101e5cc30`) holds
the same arithmetic and has no direct caller.

The multiplier changes the class of a count: m < 0 reverses its sign; 0 < m < 1.0 turns -1 into
0; a large product is cut to 32 bits. An omitted `time_multiplier` is exactly 1.0.

### Modifier countdown

`CTimedModifierCollection::AddTimedModifier` (`0x100979e84`) stores the count unchanged in a
0x28-byte entry (modifier `+8`, count `+0x10`, multiplier `+0x18`, flag `+0x20`). For a modifier
that is already present, it keeps the larger signed count (`0x100979ef0`) and replaces the
multiplier and flag. A later non-negative count therefore replaces a permanent entry, and a later
-1 cannot make a timed entry permanent. `CollectModifier` (`0x100979c14`) applies every entry and
ignores the count.

`CTimedModifierCollection::DailyUpdate` (`0x100979aa4`) skips a negative count (`0x100979ae8`).
It decrements any other count, and removes the entry when the count before the decrement was
below 2, unsigned (`0x100979af4`). So 0 and 1 behave the same, unlike the flag store, where 0
becomes permanent.

Every owner wrapper passes the count through unchanged, and a direct countdown call exists for
each collection:

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

Some owner countdowns are conditional. The espionage operation skips it on two paths
(`0x10048ac94`, `0x10048acc0`), and the megastructure and planet calls run only when the collection
is not empty. `CFederationProgression::AddTimedModifier` does not insert a `*_cooldown` modifier
when a cheat-manager byte is set (`0x10052df40`). A fleet that is not a starbase applies the
modifier to each of its ships.

### No countdown found

`add_stage_modifier` stores into espionage operation `+0xd0` (`0x101e5ddfc`) or, through
`CAstralRift::AddStageTimedModifier` (`0x100f3a574`), into rift `+0x450`. `add_modifier` in astral
rift scope stores into rift `+0x430` (`0x100f3a4f4`). No path from these three collections to the
countdown was found:

- The countdown has 17 direct callers, and none uses these offsets.
- It is not a virtual function. No code forms an address in its page, and no data fixup targets it.
- A pattern scan of the whole executable finds one copy of the countdown body.

This is strong evidence, not a complete write analysis: a differently compiled update of entry
`+0x10` would not be found. Lifetime comes from the owner instead.
`CEspionageOperation::FinishCurrentStage` (`0x10048b068`) and `ResetCurrentStage` (`0x10048b680`)
clear the operation's stage collection. `CAstralRift::FireAstralRiftEventById` (`0x100f370d0`)
clears the rift stage collection after a successful event lookup. Rift `+0x430` is cleared only by
an explicit removal or when the rift is destroyed. The count is still merged by maximum and is
returned by `CalcDaysLeft` (`0x10097a0d4`).

### Trait expiry date

`CLeader::AddTimedTrait` (`0x1008ebb44`) adds the count to the current date with
`CStellarisDate::AddDays` (`0x100c5e218`, 24 date units per day, 32-bit `madd`). It stores a
`{trait, date}` pair at leader `+0x9c8`. Adding the trait again replaces the date (`0x1008ebc10`)
instead of keeping the later one. `CLeader::UpdateTimedTraits` (`0x1008e512c`) keeps a pair while
its date is after the current date (signed, `0x1008e5190`). Otherwise it removes the pair and
every equal entry in the leader's trait list (`+0x9b0`).

The expiry is int32(now + int32(24 × count)), where the count is already the wrapping product of
operand and factor. The rule applies to the wrapped value, not to the sign of the count: 0 and
small negatives expire on the first update, -178956968 adds 64 date units and survives it, and
536870912 adds nothing.

`CAddTimedTraitEffect::PostValidate` (`0x101da6108`) logs `Invalid or no duration for … effect.`
when `CIntVariableValue::IsSet` (`0x100d1cdb8`) is false. That is true for an omitted count and
for a literal 0 with no variable, so `days = 0` and `months = 0` log. A negative count does not. The
result does not stop execution: the effect-database validation loop ignores it (`0x100456128`).
The ready-world route rejects any diagnostic before execution, so it would refuse those cases.

## Gaps

These limits are accepted at closure by the
[SDK-544 AC3 amendment](https://linear.app/unnamed-system/issue/SDK-544) of 2026-10-02. AC3
requires parser facts: readers, unit factors, combination, omitted counts, accepted range,
truncation and wrap. Consumer meaning is required only where a shared method proves it.

- **Five `days`-only commands:** `add_casus_belli`, `add_intel_report`, `create_message`,
  `give_fleet` and `prolong_fleet_contract` have no factor sibling. Their numeric mechanisms do
  not distinguish a duration from an ordinary integer. No key-name rule is used. Each `days` key
  stays an integer key with no duration group.
- **21 mixed scoped/literal readers:** the 20 event effects and `has_passed_resolution` share
  literal storage between scoped `days` and scaled integer `months`/`years`. Each has the typed
  `duration-scoped-literal` gap above. The constructor does not establish the subtype, so the
  overlap remains conservative. Their `days` unit and mixed selection behavior are not
  reported as established. Removal needs a shared whole-operand proof and a public
  representation of the mixed selection.
- **24 consumption limits:** the 21 stack-literal groups, `add_modifier`, `add_stage_modifier`
  and `add_timed_trait` keep `OutsideMethod` consumption. The modifier and trait consumers are
  described [above](#modifier-and-trait-consumers-on-m451-hotfix-sdk-672); event delays were not
  investigated. Reporting a consumer needs a representation for scope-dependent consumers
  (`add_modifier` differs in astral rift scope) and a live route that reads them. The ready-world
  route reads only country flags and variables.
- **28 flag consumers:** the static method does not establish update frequency or expiry
  dates outside the country observation. The [SDK-650 live run](ready-world.md) closes the country
  expiry gap on 4.5.1 for its five cases; it does not establish relation or other owner frequencies.
- Duration-list completeness still follows unknown child dispatch, reader joins and nested blocks.
  Accepted ranges and the live numeric widths are on [numeric conversion](numeric-conversion.md).
  These limits do not amend the parent criterion.

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
  the protected span encloses its vtable, selection fields and literal, with literal width from
  the numeric conversion proof. An overlap leaves `duration-byte-factor`; missing subtype,
  selection or width evidence leaves `duration-byte-storage`. Prefix and continuation writes
  obey the same disjointness check.
- Conditional presence writes need not occur on every path. Alternatives agree on count
  scales and word factors; their byte-write footprints are joined conservatively. Requiring equal
  footprints falsely leaves the council agenda resets unresolved.
- Do not carry flag-countdown meaning to another consumer. A stored -1, a shared read shape or a
  shared factor does not establish what the count means. Zero is permanent in the flag store but
  expires on the first update in the modifier countdown and the trait date.
- A consumer can depend on the scope type at execution. `add_modifier` reaches the modifier
  countdown in most scopes, but no countdown in astral rift scope. One command can have more than
  one consumer.
- Do not claim `SharedFactor` from the read paths alone. A constant store beside an operand is a
  multiplier only when the execute body multiplies that operand by that slot.
- Each public static query verifies the executable again, so asking `command_grammar` once per
  command takes about one second per command. Run whole-inventory measurements through
  `internals::command_grammar_stops::population`, as `examples/duration-population.rs` does.
  [SDK-651](https://linear.app/unnamed-system/issue/SDK-651) investigates the per-query cost.
