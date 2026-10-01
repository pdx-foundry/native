# Scoped numeric operands (SDK-645, SDK-647)

On M45-release, `CVariableValue::Read` and `Assign` share an operand reader. A destination's
constructor vtable point selects `CIntVariableValue`, `CFixedPointVariableValue`, or the base
type. The reader ID stays the shared entry's ID. The base type has no literal conversion.

The static method matches complete `Read`, `Assign`, prefix, variable, `AssignSimple`,
`GetValue`, and `GetValueInternal` bodies. It joins the `AssignSimple` vtable slot to the
constructor-selected address point. Captured offsets must agree across the shared and concrete
bodies. A missing body, name, pointer, constructor path, or changed instruction leaves its
property unresolved. The full `GetValue` match includes the short and long source-location
tests, every path around the single literal load, and the dynamic call. It does not infer
selection from the literal load alone.

`ScopedOperand.forms` is partial: the recognized literal, `trigger:`, `modifier:`, `value:`,
and unprefixed variable routes are reported. The qualified event-target and parameter grammar
is not established. A recognized prefix does not prove that a lookup finds a name.

A successful literal assignment writes the literal slot and returns before the reference
location and slots are changed. A non-literal assignment writes the source location, then tries
event target, prefixed references, and variable text. Each successful reference route writes
its own slot; the method does not establish that it clears other slots. If the location string
is empty, `GetValue` selects the literal. Otherwise, on the reachable dynamic path,
`GetValueInternal` tries trigger, script value, modifier, then variable. These are selection
rules for stored representations, not evaluated results or guarantees of valid names.

The concrete literal uses the ordinary token conversion. Integer storage is signed 32-bit,
scale 1; fixed storage is signed 64-bit, scale 100000. Numeric lexical boundaries, trailing
text, overflow, and library conversion remain incomplete. Established literal ranges are
reported in [shared literal ranges](#shared-literal-ranges-sdk-655).
Script-value arithmetic, parameters, and evaluated values are outside this method. Duration
units are in [duration keys](durations.md). A destination with incomplete constructor agreement
reports `UnresolvedStorage` instead of guessing from its field name.

The retained SDK-493 binding controls map to the authored routing and subtype controls and
the M45 exact-build parity test. The case map and reviewed live output are tracked under
`tests/expected/scoped-numeric-m45/`. The original prototype bundle remains preserved.

## Live fixture adaptation

The retained M45-observe run has 62 case identities. `tests/expected/scoped-numeric-m45/cases.json`
keeps each original body beside its M45-release fixture body. Agenda operand cases retain their
field. Timed-flag operands move to `agenda_cost`; `add_trust.amount` operands move to the
fixed-point `cycle_length_in_days`. Static parity separately checks the original effect reader
joins. These substitutions observe the shared parser representation, not duration multipliers,
world evaluation, or effect application. Five direct-integer cooldown contrast inputs move to
`sensor_range`: `agenda_cooldown` has conditional storage routing outside the current fixture
binding. Reference lookup objects are observed without asserting that their names resolve.

A failed batch established a fixture isolation requirement: passing `{ base = 2 add = 3 }` to
this scalar reader stores `{` as variable text and disrupts parsing of subsequent definitions.
The three inline-block cases therefore run alone. Parser diagnostics and their coverage stay
separate from stored values; an incomplete diagnostic source join is not accepted as complete
coverage. The failed sessions remain in the test-reported temporary directories.

## Verification and retained controls

The original 20 native controls become 12 owner-routing controls (root Read, timed Assign and
held-out Assign, each with a positive, absent body, unknown boundary and wrong-reader case), four
shared-evidence controls (missing prefix token, vtable pointer, prefix-dispatch body and selection
body), and four observation controls (integer/fixed-point missing record and missing authority).
The Rust method replaces old replay qualification. Full arithmetic's missing-MTTH-body control
belongs to SDK-545; here a missing GetValueInternal body must leave selection unresolved. The
41 old evaluation results are not parser results; [world evaluation](#world-evaluation-on-m451-hotfix-sdk-647)
maps each of them. Hash/capsule checks from the old verification script are preservation checks, not engine
method tests.

M45-release observations retain a script-value lookup after a later literal (integer 9 or fixed
raw 900000); the location stays nonempty. Variable then literal also retains variable text.
Trigger then value stores both; value then variable also stores both. The matched GetValue
bodies establish selection priority; these storage observations do not assert evaluation success.
Boundary controls store i32::MAX and i64::MAX (fixed scale 100000); `-1.234567` stores -1 or
-123456 respectively. A malformed numeric token after `7` retains the numeric slot and becomes
variable text. No finite result establishes universal overflow or accepted ranges.

Commands: `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo parity`,
`cargo test --release --lib m45_scoped_numeric -- --ignored`, and
`cargo live fixture_scoped_numeric`. Exact-build and live commands require `STELLARIS_PATH`.

## Transfer to the 4.5.1 hotfix

The exact M451-hotfix identity is recorded in [targets](targets.md). Fresh canonical bodies align
instruction for instruction with M45-release. Diagnostic strings carry a different compiler
source directory, and both numeric `GetValue` bodies form relocated local jump-table bases with
`adr`. Pinning those diagnostic paths and absolute local addresses made forms and selection
unresolved, although the operand paths were unchanged.

The matcher now captures a nonempty named diagnostic string, requiring repeated uses in a body
to agree. It matches each local `adr` by its exact relative instruction position; a different
instruction target, an address outside the body, or any changed instruction still fails the
complete-body match. Calls, branch targets, reference slots, literal loads and layout joins
remain checked. This preserves the old compiler shape without a version branch or a relaxed
substring match. Missing diagnostic names remain unresolved.

Authored controls cover relocated bases, wrong local targets, external targets, changed source
paths, inconsistent source strings, and missing or empty strings. Fresh shape captures and their
comparison with the old shapes remain under `.local/sdk-650/hotfix/scoped-*-shape.txt` and
`scoped-*-diff.txt`. These checks establish static parser forms and stored representation
selection; they do not establish live world evaluation.

On the installed hotfix, all three existing `m45_scoped_numeric` static checks pass: the shared
proof with negative controls, registry/effect parity, and timed-flag command parity. The six
default targeted scoped-number tests and `cargo clippy --lib -- -D warnings` also pass. No
recorded expectation was refreshed for this repair.

## Constructor state on M451-hotfix (SDK-654)

The executable and ARM64 identities are the M451-hotfix pair in [targets](targets.md).
`engine/analysis/receivers.rs` owns constructor state; command factories consume it through
`declarations/receiver.rs`, and registry destinations through `fields/persistent.rs`.
The binding follows a bounded graph of constructor symbols, including constructor aliases.
Neither a command nor a key selects a numeric subtype.

A summary of a class's own vtable-group points alone misses the vtables of numeric members
in an out-of-line owner constructor and the literal initialized by a directly called numeric
constructor. The exact-build examples cover these shapes:

| Shape | Engine example | Fact absent from a vtable-only summary |
| --- | --- | --- |
| Direct numeric member construction | `CSetTimedCountryFlagEffect` factory `0x101d9ab98`, operand `+0xa8`, integer constructor `0x100d1cc74` | The constructor's literal at operand `+0x200`; its primary point alone did not supply the omitted count |
| Out-of-line owner with integer members | `CCountryEventEffect` factory `0x101d46d5c` calls `CFireEventEffect` constructor `0x101d2713c` | Integer points at owner `+0xd8` and `+0x2e0`; `0x100d1cd40` stores the supplied integer at operand `+0x200` |
| Out-of-line owner with fixed-point members | `COrderedListEffect` constructor `0x101d251b8` calls `0x100d1bd80` at owner `+0x160`; `CAddModifierEffect` constructor `0x101e5ca24` calls `0x100d1be44` at `+0xb8` and `+0x2c0` | Embedded fixed-point points, absent from the owner's vtable group |
| Registry constructor cleanup fragment counted as an owner | `CPurgeType` constructor `0x100be48e8` and outlined cleanup `0x1028debe8` | The real constructor establishes the operand point at `+0x690`; intersecting it with the cleanup fragment removed it |

Outlined `[clone .cold.*]` cleanup fragments are not independent constructors. The ordinary
one-instruction alias at `0x100be4ae4` delegates to the full `CPurgeType` constructor. Its numeric
constructor call at `0x100be4a14` establishes `pop_decline_rate` as signed 64-bit, scale 100000.

Compiler vtable-group summaries are an independent baseline. Command factories and registry
owners evaluate that summary-only path separately from the path that enters constructor bodies.
The baseline's established points and bytes take precedence; an entered body only adds facts.

### Owner derivation (SDK-658)

The entered path tracks which values may point into the fresh owner (`evaluate/owner.rs`). The
owner starts at the factory's `operator new` and at the registry owner's first argument. A value
is owner-derived when it is computed from a derived register, vector or memory byte, or when a
call that could reach the owner returns it. A store or call is judged by these rules:

| Write | Effect on owner bytes | Other effects |
| --- | --- | --- |
| Known address | Exact, including earlier members | Outside the owner and private frame: caller memory is forgotten after return; a derived value escapes |
| Unknown, underived address | Kept: a pointer that existed before the owner cannot point into it | Every other byte becomes unknown; a derived value escapes |
| Unknown, derived address | Every owner byte becomes unknown | Every other byte becomes unknown |
| Call that is not entered | Unknown when the call may reach the owner | Every byte outside the owner becomes unknown |

A call may reach the owner when the owner escaped, or when an argument register (`x0`-`x8`,
`v0`-`v7`), a preserved register (`x19`-`x29`, `v8`-`v15`) or any stack byte is derived. A
reaching call makes the owner escape and derives every register it does not preserve. After an
escape, a load of an unknown byte is derived. Only a store of an underived value to a known address
clears a byte's derivation; a forgotten byte may keep its earlier value. Loop-head joins and
returning paths take the union of derivations and escapes.

An owner byte is claimed when every returning path agrees on it after every later write that may
change it. An unknown write therefore does not withdraw the walk: a later store to a known
address establishes its bytes again. A walk with a path that does not return, a stack pointer
moved by an unknown amount, a receiver outside the owner or a summary point that disagrees with a
constructor-written word is not followed. Its call is treated as one that is not entered, and the
compiler summary is installed.

Entered constructors report the whole owner, so a write to an earlier member through another
argument gives the new value, never a stale one. Callee machines inherit the caller's stack
position, argument values and established memory with their derivations. Preserved registers
keep their derivation but not their values. A callee's write to a caller stack argument makes the
caller forget its memory. While the entered path runs, a factory or registry call that is not
entered also forgets caller memory, so a changed stack slot cannot reach a constructor as stale
evidence.

Command and registry constructor tail branches use the same join as direct calls, even when the
callee's code is decoded. Summary points are checked and completed at the constructor's receiver
offset in the owner. New embedded points require constructor-written words; missing nested code
cannot borrow a point from metadata. Partly written points are never completed from metadata.
Reserved objects have no read-only backing; the method assumes neither zeroed allocation nor
`_bzero` behavior.

The [current population](discovery.md#owner-derivation-sdk-658) records storage, known ranges and
failure shapes. The comparison with `main` finds no decreased answer; 134 command arguments gain
storage, and every recovered storage agrees with the earlier unconfined run. Reports and the
field-by-field comparison are in `.local/sdk-658/`.

### Remaining constructor obstacles

Most factories call `_bzero` on the new owner, and every effect owner's `CEffect::CEffect()`
passes the address of a stack slot that holds `this` to `CPdxArray::InsertAtEmplace`
(`0x10045691c`). Both calls may reach the owner, so the owner has escaped before the derived
constructor's members are built. Every later call that is not entered may then write any owner
byte. An operand is established only when its member's bytes are written after the last such
call. The remaining 35 arguments lose their bytes to a later member constructor:

| Later constructor | Calls that may write the owner | Lost destinations |
| --- | --- | --- |
| `CEventTarget::CEventTarget()` `0x1004f6ed0` | `CStaticLexer::GetString` `0x1004f6f10`, `CEventTarget::PopulateTokenString` `0x1004f6f44` | `add_modifier` and `add_stage_modifier` `mult` / `multiplier`; `set_saved_date.days_from_present` |
| `CToken::CToken(int, CString const&)` `0x1025bc848`, built by a later `CIntVariableValue` | `_memcpy` `0x1025bc92c` into the token's buffer with an unknown length, then `strb` `0x1025bc930` at an unknown index | The 20 event effects' `days` at owner `+0xd8`, lost to the `random` member at `+0x2e0` |
| `CTrigger::CTrigger()` `0x100d0613c` | Trigger database registration (`0x100d06194`-`0x100d061ac`) | `closest_system` and `num_neighbor_systems` steps and distances, `effect_on_blob.owned_planets_percentage` |
| `CString::CString(char const*)` `0x102521fec` | `_memmove` `0x102522074`, then `strb` `0x102522078` at an unknown index | `create_pop_group.size`, `spawn_megastructure.orbit_distance` |
| `CReleaseVivariumFaunaCountEffect` constructor `0x101e16b84` | `___cxa_guard_acquire` `0x101e16c10` and `___cxa_guard_release` `0x101e16c2c` | `release_vivarium_fauna_count.count` |

The `ldaprb` at `0x101e16bcc` is evaluated as a byte load; the guard calls are the obstacle.
Recovery needs a bounded model of these calls' writes, such as a copy length proved below the
token's capacity, not a subtype name or a cached initial byte. These destinations remain an unmet
parent criterion; only Jackson can amend it.

## World evaluation on M451-hotfix (SDK-647)

The static method gives storage and selection. It gives no evaluated number. The numbers below
come from the engine's own effect execution in a loaded world, read through
[`Game::observe_world`](ready-world.md). Native does not call `GetValue` itself. The build is the
M451-hotfix executable in [targets](targets.md); the world is the tracked 4.5.1 save, with United
Nations of Earth as the country scope on 2200.01.01.

### Witness effects

Two effects turn an operand into a value that the world observation reads. Both facts are from the
hotfix executable.

- **Integer.** `set_timed_country_flag = { flag = F days = X }`. The execute body stores
  `CIntVariableValue::GetValue(scope)` times the factor as the flag count
  ([duration keys](durations.md)). With `days` alone the factor is 1, so the day-zero count of
  flag `F` is the evaluated integer.
- **Fixed point.** `set_variable = { which = V value = X }`. `CSetVariableEffect::ExecuteActual`
  (`0x101e0e570`) passes `CFixedPointVariableValue::GetValue(operand +0xd0, scope)` to
  `CVariables::SetVariable` with no other arithmetic. The raw value of variable `V` is the
  evaluated fixed-point number, scale 100000.

`export_trigger_value_to_variable`, `export_modifier_to_variable` and
`export_resource_stockpile_to_variable` give the same numbers by a second engine route.

### Evaluation bodies

The matched `GetValue` and `GetValueInternal` bodies (`0x100d1cdf0`, `0x100d1d3a4` for the integer
type; `0x100d1bf00`, `0x100d1c4bc` for the fixed-point type) read as follows. The live cases
agree with each line that they reach.

- An empty source location returns the literal slot. This is the only literal load.
- Otherwise the body resolves the stored event-target chain, then tries trigger, script value,
  modifier and variable in that order. The variable route is last and unconditional: it runs
  even when no variable name is stored.
- The integer body divides the fixed-point result of the script-value, modifier and variable
  routes by 100000 and truncates toward zero. A trigger has separate integer and fixed-point
  getters. A literal converts when it is read, not when it is evaluated.
- A failed lookup returns zero, never the literal slot.
  - `CVariableValue::GetVariableValue` (`0x100d1b594`): an unset variable logs
    `Variable <name> is not set for scoped <type> '<object>' at <location>`.
  - `CVariableValue::GetModifierValue` (`0x100d1b3d4`): a modifier that the scope does not have
    returns zero with no message. By the static reading, a definition flag clamps some
    modifiers to 0..1; no live case reaches that path.
  - A trigger whose scope type is wrong at evaluation logs
    `Invalid Scope type for trigger <name> used as a variable at <location>, got <type>`.
  - An event target that does not resolve logs `Invalid event target scope '<target>' at <location>`.

### Result

`tests/expected/world-numeric-m451/cases.json` holds every case with its operand text, the parser
storage of its integer operand, and both results. `cargo live world_numeric` runs it: one
`check_script` session and five world sessions. Each case is evaluated in both destinations.

| Group | Cases | Result |
| --- | ---: | --- |
| Literals and a global `@` constant | 10 | The stored literal. `2.75` gives 2 and raw 275000; `2147483648` gives -2147483648 and raw 214748364800000; 24 nines give -1 and raw -100000; an omitted operand gives 0 |
| Variable | 2 | `7.9` gives 7 and raw 790000; `-2.75` gives -2 and raw -275000 |
| Numeric trigger | 2 | `num_owned_planets` 1, `empire_size` 50; equal to the export effect, and `num_owned_planets = 1` is true |
| Modifier | 4 | `country_edict_fund_add` 15 and raw 1500000; `pop_cat_specialist_bonus_workforce_mult` 0 and raw 10000; `ships_upkeep_mult` 0 and raw -2000; an absent modifier 0 with no message. Each equals the export effect |
| Script value | 1 | Vanilla `tech_weight_likelihood` (1.25) gives 1 and raw 125000 |
| Two assignments | 11 | Two literals give the later one. A literal with any reference gives the reference, in both orders. Two references of different kinds give the earlier one in the static priority, in both orders. A second script value replaces the first |
| Qualified scope | 6 | `root.` and `owner.` give the country's variable; `capital_scope.` and a saved event target give the planet's variable, modifier (-3) and trigger (`planet_size` 18) |
| Logged fallback | 6 | Zero in both destinations, one message for each statement; see below |

Six more cases repeat a single form as the baseline of their session, which gives 48 cases and
96 evaluations.

There is **no conflict** with the static facts. Each check uses the facts of its own destination
(`set_timed_country_flag.days`, `set_variable.value`), not a reader ID.

- The variable store's scale (the world recipe) equals the static scale of `set_variable.value`.
- For each integer operand, the static selection rule applied to its observed parser storage
  names the form whose value the world shows. This holds for all 48 cases.
- For the script-value, modifier and variable routes, the integer result equals the fixed-point
  result divided by 100000 and truncated.
- The retained reference-then-literal case is confirmed in execution: `days = n647_positive
  days = 4` stores literal 4 and evaluates to 7.

Comparison with observed parser storage covers the integer destination only. `check_script`
reports storage for duration groups and for nothing else, so the stored state of the fixed-point
operands was not observed; they are compared with the static facts alone.

### Fallback and rejection

These forms execute, log and give zero. The answer is partial, and the values stay in the sample.

| Operand | Message |
| --- | --- |
| An unset variable | `Variable n647_unset is not set for scoped country …` |
| `modifier:` with an unknown name | `Variable  is not set …`: no modifier and no name is stored, so the variable route runs with an empty name |
| An unknown prefix, `bogus:n647` | `Variable bogus:n647 is not set …`: the whole token is variable text |
| A saved event target that does not exist | `Invalid event target scope 'event_target:n647_absent'` |
| `capital_scope.` with an unset variable | `Variable n647_unset is not set for scoped colony 'Earth'` |
| A country trigger through a saved target that is a planet | `Invalid Scope type for trigger num_owned_planets used as a variable …, got colony` |

These forms are rejected while the engine reads or validates them. A world does not execute a
rejected effect, so their runtime value is **not observed**. The static reading is zero.

| Operand | Stage | Message |
| --- | --- | --- |
| `trigger:` with an unknown name | validation | `Scripted Trigger … is invalid`; `Error in scripted trigger, cannot find: …` |
| `value:` with an unknown name | validation | `Script Error: Invalid script value: …` |
| A Boolean trigger, `trigger:is_ai` | read, validation | `Trigger is_ai cannot be used in this context`; the Boolean value message |
| A planet trigger in the country scope | read | `Trigger planet_size used in wrong scope …, got country` |
| A country trigger after `capital_scope.` | read | `Trigger num_owned_planets used in wrong scope …, got colony` |

### SDK-493 evaluations

The prototype evaluated 41 operands in an empty scope on M45-observe. 26 now run in the world
scope with the same results where the input is the same.

| SDK-493 evaluations | World case |
| --- | --- |
| 1, 24 | `omitted` |
| 2, 25; 3; 4, 28, 36; 5; 6; 7; 8; 9 | `literal`, `literal_negative`, `literal_fraction`, `literal_negative_fraction`, `literal_hex`, `literal_octal`, `literal_overflow`, `literal_very_large` |
| 10, 29, 37 | `constant`, with the vanilla global `@ruler_job_weight` |
| 11, 30, 38 | `script_value`, with the vanilla `tech_weight_likelihood` |
| 16, 33; 17; 18, 34, 41; 19 | `literal_literal`, `literal_value`, `value_literal`, `value_value` |
| 26, 27, 35 | Duration factors: the [stored durations](durations.md) and the [expiry run](ready-world.md) |
| 15, 32, 40 | A missing script value is rejected at validation; not rerun |
| 12, 13, 14, 20–23, 31, 39 | Custom script values with arithmetic; not rerun |

### Gaps

- **Rejected references at run time.** The world route never executes an effect that has a read
  or validation message. A forced evaluation would be a different observation contract.
- **Custom content.** A world session loads installed content and no mod files. Custom script
  values and file-level `@` constants cannot be prepared. Script-value arithmetic is SDK-545.
- **Qualified-scope grammar.** The static `forms` list stays partial. The six live cases are
  observations with no static form to compare. Scope availability and identity are SDK-549.
- **Fixed-point parser storage in a paused game.** `check_script` does not report the storage of
  an operand outside a duration group.
- **The unknown-modifier check.** The engine string `Invalid modifier %s at %s` belongs to the
  deferred variable-value database. Neither `check_script` nor the world route runs that
  validation, so neither gives a validation message for `modifier:` with an unknown name. The
  world still logs the failed variable lookup when the effect executes.

### Pitfalls

- One rejected statement stops the whole prepared effect. Read every statement with
  `check_script` first; `world_numeric_stored` does this for the table.
- A quiet read is not a valid reference. An unknown prefix, an unknown modifier and an absent
  saved target all read quietly and fail only when the effect executes.
- Equality at zero proves nothing about a modifier: an absent modifier also gives zero on both
  routes. Use a nonzero value that a content definition also gives.
- A modifier added by `add_modifier` is not visible to `modifier:` in the next statement. In the
  observed run it became visible after a `random_country` statement. Application and propagation
  are SDK-547; do not read a rule from this.
- `num_pops` is not a trigger on this build. A trigger name from an older build can reject the
  effect.
- A plain country flag has count -1. A timed flag whose operand evaluates to zero has count 0 on
  day zero.

## Shared literal ranges (SDK-655)

Scoped numeric attachment copies `Reader.numeric` from the shared token-reader facts only
when constructor evidence establishes the concrete literal storage. Those literals inherit the
int or direct fixed-point range in [numeric conversion](numeric-conversion.md#faithful-storage-and-endpoint-requirements).
Destinations with unresolved storage retain `Reader.numeric: Unresolved` and have no range.
The [current population](discovery.md#numeric-boundary-evidence-sdk-655) records the known-range
counts separately from overall answer completeness.

The live matrix has 88 cases, including twelve boundary additions to the earlier 76 cases.
`overclock_cooldown` and `cycle_length_in_days` each cover the inward neighbor, endpoint and
outward neighbor of both limits. The live storage check also requires the inherited range.
Outside-limit wrap remains a finite observation; the scoped literal conversion gap stays.
