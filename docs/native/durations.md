# Duration keys (SDK-646)

`CommandGrammar.durations` groups the child keys of a command that set one duration count, such
as `days`, `months` and `years`. It gives each key's factor, how later keys combine with earlier
ones, the count when no key is written, and what consumes the count. The source stamp is
`command-grammar/v12`. The method is `src/engine/analysis/durations.rs`, bound by
`src/binding/binary/durations.rs` and normalized by `src/session/durations.rs`. Registry field
answers do not report durations; `pdx_native::internals::duration_groups` runs the same grouping
over registry fields for the population run.

A key is a duration unit only by mechanism. Only keys read by an integer or scoped numeric reader
are candidates. Candidates whose reader joins share one callee and owner destination form a group
when at least one key applies a factor after its reader returns. No key name, token or command
selects a result.

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

`add_timed_trait` has the same read shape, but its execute body is different. Its combination
therefore stays unresolved, with per-key factors reported as `Partial`.

### Flag store countdown

- `CPdxIntegerFlags::SetFlag` (`0x1022a2688`), in mode 0, replaces an existing flag's date and
  count. Mode 1 keeps the earlier date and the larger count; the timed-flag effect does not use it.
- `CPdxIntegerFlags::UpdateFlags()` (`0x1022a29f4`) skips negative counts. It decrements the other
  counts and removes a flag when the decremented count is zero.
- Both bodies use the count array at `+0x40`. The method requires this agreement.

A positive count is therefore removed on its count-th update. Zero becomes -1 and is never
removed, and so is any negative count. An omitted duration writes operand 0 (SDK-493 observation),
so `set_timed_*_flag` without a duration would set a permanent flag. This is a static reading.
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
Consumption, including `add_modifier`'s `time_multiplier` (`CAddModifierEffect::GetDays`), is
outside this method.

### Unidentified keys

- The 20 `*_event` effects share `CFireEventEffect::ReadMember`. It reads `months` and `years`
  into a stack temporary (`sp+0x1c0`), scales it, and stores it at owner `+0x2d8`. `days` is a
  separate `CVariableValue::Read` into another slot. The shared reader join requires an owner
  destination, so `months` and `years` have no join, and no group forms.
- A `days` key with no factor sibling cannot be told apart from any integer by mechanism.
  `add_casus_belli`, `add_intel_report`, `create_message`, `give_fleet` and
  `prolong_fleet_contract` are such keys.
- `CHasPassedResolutionTrigger::ReadMember` has the same stack-temporary shape: `months` and
  `years` read to `sp+0xc` and `sp+8`, scale, and store at `+0x470`, while `days` reads a scoped
  operand at `+0x270`.

## Result on M45-release

The population run asked `command_grammar` for all 2,170 registered effects and triggers and ran
the grouping over all 164 discovered registries. No question failed.

| Population | Groups | Complete | Partial | Failed |
| --- | ---: | ---: | ---: | ---: |
| Commands | 31 | 0 | 31 | 0 |
| Registry fields and nested collections | 0 | 0 | 0 | 0 |

- **Groups.** 28 timed flags, `add_modifier`, `add_stage_modifier` and `add_timed_trait`.
  - 27 timed flags have `SharedFactor` with initial factor 1 and `FlagCountdown`.
  - `set_timed_relation_flag` (which also reads `who`) and `add_timed_trait` have execute bodies
    that do not match, so their combination is unresolved (`duration-execute-body`).
  - The two modifier effects are `ScaledAtRead` with factors 1, 30 and 360.
- **Failure shapes**, by group:
  - 29: the omitted count is not established. Factory state does not model the operand's
    constructor or a constructor that is not inlined.
  - 27: the flag update frequency, and so the expiry date, is outside the method.
  - 2: consumption is outside the method (the modifier effects).
  - 2: the execute body is not matched.
- **Unclassified candidates**, 2 commands: after the reader call, `transfer_resources_to_empire`
  (`percentage`) and `while` (`count`) run code that the method does not follow. Their lists are
  partial.
- **Duration lists**, all commands: 626 known, 515 partial and 1,029 unresolved. A list is never
  more certain than the command's fixed keys, so most partial and unresolved lists follow them.
- **Unit-named keys that no group covers**, 26 commands:
  - the 20 `*_event` effects and `has_passed_resolution`: a stack-temporary destination;
  - five `days`-only commands: no factor sibling.

The registry run found no group. Two registries, `common/council_agendas` and the
`advanced_authority_swap` collection of `common/governments/authorities`, have candidates with an
owner store before the reader call, so a group there is not ruled out.

A group is complete only when its combination, every factor, its omitted count and its
consumption are established. No group meets that on this build.

Reproduce with `cargo run --release --example duration-population` and `STELLARIS_PATH` set.
The run takes about three minutes. The report is `.local/sdk-646/duration-population.json`.

## Live parser observations

`Game::check_script` reports `stored_durations` for each top-level child after reading, before
validation. Nothing is executed. `tests/live/durations.rs` runs one effect session in country
scope, and `tests/expected/duration-m45/live.json` holds the reviewed results. Every case classified
its one child, and every stored value agrees with the static groups. The `add_modifier` answers are
complete. The `set_timed_country_flag` lists are partial, because the static list is partial: its
`flag` key has no reader join, so the method cannot rule out another group.

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

## Gaps

- Update frequency outside the country flag store. The [SDK-650 live run](ready-world.md)
  closed the country expiry gap on 4.5.1: mixed units produced 90 daily updates, one day expired
  after one update, and zero, negative and overflowed counts remained through day 90. No conflict
  with the static `FlagCountdown` reading was observed.
- Consumption of scaled-at-read counts, including `time_multiplier`, and event delays.
- The stack-temporary readers, `days`-only keys and the unclassified candidates.
- Omitted counts where factory state lacks the initial bytes.

## Pitfalls

- A store before a tail call counts. The method checks owner stores on the whole token path,
  not only after the call, and leaves a key with a store before its reader call unresolved.
- Do not claim `SharedFactor` from the read paths alone. A constant store beside an operand is a
  multiplier only when the execute body multiplies that operand by that slot.
- Each public static query verifies the executable again, so asking `command_grammar` once per
  command takes about one second per command. Run whole-inventory measurements through
  `internals::command_grammar_stops::population`, as `examples/duration-population.rs` does.
  [SDK-651](https://linear.app/unnamed-system/issue/SDK-651) investigates the per-query cost.
