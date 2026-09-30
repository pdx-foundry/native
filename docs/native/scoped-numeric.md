# Scoped numeric operands (SDK-645)

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
text, accepted range, overflow, and library conversion remain unresolved as in SDK-644.
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
41 old evaluation results are not rerun or presented as parser results; SDK-647 owns evaluated
values. Hash/capsule checks from the old verification script are preservation checks, not engine
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
