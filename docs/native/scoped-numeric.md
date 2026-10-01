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
The baseline's established points and bytes take precedence. An unconfined or incomplete entered
walk contributes no bytes, returned registers or control-flow facts to the baseline. The entire
owner's entered-body contribution is withheld, including earlier and later constructor additions;
this conservative rule does not erase facts established without entering those bodies.

New constructor bytes and return values require agreement at every normal return and confinement
of every entered walk. A store must fit within that walk's remaining receiver span or private
stack frame; the private frame must also be disjoint from the whole owner. Earlier-member stores
through another argument, unknown-address stores, unmodeled calls and unavailable nested bodies
reject the added proof. Call-time invalidation propagates through failed wrappers in the entered
path. The summary-only path remains unaffected. Callee machines inherit the caller's stack
position and established memory state; unestablished caller memory stays unknown, and the private
frame cannot overlap a caller stack argument. While confinement is watched, SP updates retain
that proof only for known immediate offsets from SP, including immediate stack write-back.
Moving another register into SP or using a register offset rejects the walk, even if SP is restored
before return.

Command and registry constructor tail branches use the same join as direct calls, even when the
callee's code is decoded. Missing, changed, bounded-out or conflicting code contributes no
initial-value evidence. New embedded points require constructor-written words; missing nested
code cannot borrow a point from metadata. Partly written points are never completed from metadata.
Reserved objects have no read-only backing; the method assumes neither zeroed allocation nor
`_bzero` behavior.

The evaluator does not establish freshness-based disjointness. A pre-existing database pointer or
a heap buffer returned without an owner argument is not enough: recovery requires tracking owner
derivation through known and unknown registers, memory, vectors, calls and path joins. A pointer
loaded after storing `this`, and a pointer returned by an unmodeled call that received `this`,
must remain potentially owner-derived. Until that shared proof exists, external registration and
string-copy stores reject the constructor additions even when their real destinations may be
separate allocations.

The [current population](discovery.md#numeric-boundary-evidence-sdk-655) records storage,
known ranges and failure shapes. The main comparison finds no decreased storage answer:
command storage and six registry storage results are unchanged; `pop_decline_rate` gains established
storage. Reports and the field-by-field comparison are in `.local/sdk-654/floor/`.

A shared obstacle is `CToken::CToken(int, CString const&)` at `0x1025bc848`: unmodeled lexer
and string calls leave the copied length or buffer unknown, and `strb` at `0x1025bc930` may write
outside the member span. `CEffect::CEffect()` registers the object through external database
pointers, including a store at `0x100456928`. Neither chain has a confinement or freshness proof.
Their compiler summaries remain usable, but the entered bodies cannot establish additional
operand points or initial values. Recovery requires a shared call/alias proof, not a subtype name
or a cached initial byte. The parent storage criterion remains unmet for unresolved destinations.

### Remaining constructor obstacles

`release_vivarium_fauna_count.count` retains `UnresolvedStorage: Scoped destination vtable is
not established.` Its factory `0x101e16b38` calls the owner constructor `0x101e16b84`. That body
calls a fixed-point constructor at owner `+0xa8`, then reaches the unmodeled acquire-byte load
`ldaprb w8,[x8]` at `0x101e16bcc`. The subsequent paths use the mutable sentinel guard and
`___cxa_guard_acquire` / `___cxa_guard_release`. The bounded evaluator establishes neither this
instruction nor those calls' effects on the owner. The initial operand cannot be retained from
its earlier store without establishing the remaining paths. Removal requires a shared proof of
this atomic/guard shape and negative controls. This destination remains an unmet parent
criterion; only Jackson can amend it. No subtype is inferred from the constructor name.

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
