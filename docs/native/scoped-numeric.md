# Scoped numeric operands (SDK-645, SDK-647)

## API scope, 2026-10-02

`ScopedOperand` reports routing forms only. Representation selection and literal assignment preservation are retired public properties. Their findings below remain available. Internal layout proofs still support parser storage and duration overlap checks; they are not runtime results. The retired properties, the versions of `src/engine/analysis/scoped_numeric.rs`, `src/session/scoped_numeric.rs`, `tests/expected/m45/command-grammars.json` and `tests/expected/numeric-m45/readers.json` that produce them, and the world controls are in Git at `d8f9d8a`.

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

### Owner derivation (SDK-658, SDK-660)

The entered path tracks which values may point into the fresh owner (`evaluate/owner.rs`). The
owner starts at the factory's `operator new` and at the registry owner's first argument. Tracking
starts conservative: every register, vector and held byte may hold the owner's address. A registry
constructor's caller is not analysed, so its run keeps this state. The one narrowing is an
allocator's return (`Machine::return_allocated_owner`): a value held before the allocation is
underived. A pointer loaded from constant image data is underived. A value is owner-derived when it
is computed from a derived register, vector or tainted memory byte, or when a call that is given an
owner address returns it.

#### Assumption: code changes only the object that it is given

The method assumes that a call or store changes only the object that it is given. C++ builds
members in address order, so a later member's code leaves the members before it intact. The
assumption replaces SDK-658's escape rule, under which any call after `CEffect` registration could
write any owner byte. It is a named assumption, not a proof. Jackson approved it on 2026-10-01
after the [SDK-671 investigation](#global-lexer-string-sdk-671-investigation) showed that a proof
needs a whole-program invariant. It applies the same way on every build and has no branch on a
class, command or build.

A call that is not entered is **given** the owner addresses that it receives:

- each known value in `x0` to `x8` that lies inside the owner;
- the owner's start, when `x0` is owner-derived and its value is unknown.

`Machine::given_owner_address` returns the lowest. Other registers are ignored. A derived register
with an unknown value can be left over from earlier code, and a call's arity is not known. A
derived register with a known value outside the owner names another object. A store or call is
judged by these rules:

| Write | Effect on owner bytes | Other effects |
| --- | --- | --- |
| Known address | Exact, including earlier members | Outside the owner and private frame: caller memory is forgotten after return |
| Unknown, underived address | Kept: a pointer that existed before the owner cannot point into it | Every other byte becomes unknown |
| Unknown address, base register a known owner address | Bytes before the base are kept; bytes from it on become unknown | Every other byte becomes unknown |
| Unknown, other derived address | Every owner byte becomes unknown | Every other byte becomes unknown |
| Call that is not entered, given an owner address | Bytes from the lowest given address to the owner's end become unknown and derived | Every byte outside the owner becomes unknown; the object at each other address in `x0` to `x8` becomes derived; every register that the call does not preserve may be derived |
| Call that is not entered, given no owner address | Kept | Every byte outside the owner becomes unknown; no returned register is derived |

The modeled C calls follow the same return rule. `strlen` and a bounded `memcpy` or `memmove`
keep their exact memory effects, but their returns are derived only when they are given an owner
address. An allocation is given only its size, so a non-owner allocation returns nothing derived.

A call given an owner address may store one in the objects that it is given, such as a member's
self pointer or a stack out-parameter, so those bytes are tainted. An object outside the owner has
an unknown size. On the stack, it is taken to reach the end of the frame that holds it; after the
stack pointer moved by an unknown amount, no frame bounds it and the walk adds no facts. Elsewhere,
it is a pointer-sized slot and any run of held bytes that continues it; memory that the machine
does not hold falls under the escaped-address pitfall below. A load is derived when its address is derived
or when it reads a tainted byte. Only a store of an
underived value to a known address clears a byte's taint; a forgotten byte may keep its earlier
value. Loop-head joins and returning paths take the union of derivations. An entered constructor
returns its taint outside the owner as well as inside it (`Machine::install_owner_derived_memory`),
so a member that stores `this` at a known global leaves that global derived for its caller.

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
evidence. A rebased pointer slot outside `__DATA_CONST` and the read-only sections is writable.
Earlier code may have replaced its target, so the entered path reads it as unknown. A factory or
registry run whose stack pointer moved by an unknown amount adds no facts. Both routes take these
rules from `receivers.rs`: `ConstructorImage` gives each run its image, and `accept_entered_path`
applies the stack rule.

Command and registry constructor tail branches use the same join as direct calls, even when the
callee's code is decoded. Summary points are checked and completed at the constructor's receiver
offset in the owner. New embedded points require constructor-written words; missing nested code
cannot borrow a point from metadata. Partly written points are never completed from metadata.
Reserved objects have no read-only backing; the method assumes neither zeroed allocation nor
`_bzero` behavior.

#### Pitfalls

- **An owner address stored at an unknown address is lost.** The store taints no byte, so a later
  load of it is underived. Registration arrays are the intended case. Code that stores `this` at an
  unknown place and then reads it back to write an earlier member would leave a stale byte.
- **Out-parameters are counted only in `x0` to `x8`.** A call that writes an earlier member through
  a pointer in a stack argument or in a preserved register is not seen.
- **An unknown derived `x0` loses the whole owner.** The long path of
  `CString::CString(char const*)` gives the string member to `CPdxCommonStringAllocator::allocate`.
  The allocator's return is then derived, and the later copy into it loses every owner byte. The
  long `CToken` path calls `operator new[]` with a size only and keeps earlier members.
- **Do not taint every byte that a call may forget.** A call forgets all memory outside the owner,
  including the caller's saved-register slots. Tainting them makes the epilogue restore `x19` and
  `x20` as derived unknown values, and the next member call then loses the whole owner. This lost
  the six `closest_system` and `num_neighbor_systems` arguments to `CTriggerDatabase::AddTrigger`
  `0x100d061ac`, which is given only the database and the new trigger.
- **Compiler summary disagreement still rejects a walk.** Every recovered point agrees with the
  vtable summary and with the [constructor table](#constructor-state-on-m451-hotfix-sdk-654).
- Live observations confirm the static answers; they do not initialize constructor state.

The [current population](discovery.md#member-confined-calls-sdk-660) records storage, known ranges
and failure shapes.

### Constructor obstacles under the escape rule (superseded by SDK-660)

This section records where SDK-658's escape rule lost the last 35 arguments. The
[member-confined rule](#assumption-code-changes-only-the-object-that-it-is-given) recovers all of
them, because none of these calls is given the lost member's address. The table names the calls
that a method without that assumption would have to model.

Under the escape rule, most factories call `_bzero` on the new owner, and every effect owner's
`CEffect::CEffect()` passes the address of a stack slot that holds `this` to
`CPdxArray::InsertAtEmplace` (`0x10045691c`). Both calls made the owner escape before the derived
constructor's members were built, and every later call that was not entered could then write any
owner byte. `command-population --trace` names every place that may have overwritten a lost
member's bytes (see [method authoring](method-authoring.md)).

| First loss | Latest loss | Lost destinations |
| --- | --- | --- |
| `CEventTarget::CEventTarget()` `0x1004f6ed0`: `CStaticLexer::GetString` `0x1004f6f10` | `CEventTarget::PopulateTokenString` `0x1004f6f44`, in the same constructor | The 20 event effects' `days` at owner `+0xd8`; `add_modifier` and `add_stage_modifier` `mult` / `multiplier`; `set_saved_date.days_from_present` |
| `GetString` `0x1004f6f10` | `CEffect::CEffect()` `0x10045680c` of a later member: `str w8,[x21,#0x58]` `0x100456928` to the registration array | `effect_on_blob.owned_planets_percentage`, `spawn_megastructure.orbit_distance` |
| `GetString` `0x1004f6f10` | `CTrigger::CTrigger()` `0x100d0613c` of a later member: trigger database registration (`0x100d06194`-`0x100d061ac`) | `closest_system.min_steps` (effect and trigger), `num_neighbor_systems.min_distance` |
| `CTrigger::CTrigger()` registration `0x100d06194` | `CTrigger::CTrigger()` registration `0x100d061ac` | `closest_system.max_steps` (effect and trigger), `num_neighbor_systems.max_distance` |
| `GetString` `0x1004f6f10` | `CReleaseVivariumFaunaCountEffect` constructor `0x101e16b84`: `___cxa_guard_acquire` `0x101e16c10` | `release_vivarium_fauna_count.count` |
| `CEffect::CEffect()`: `CPdxArray::InsertAtEmplace` `0x10045691c` | `CString::CString(char const*)` called at `0x101e69f6c`, whose walk is not followed | `create_pop_group.size` |

Between `GetString` and `PopulateTokenString`, `CEventTarget::CEventTarget()` builds a `CToken`
(`CToken::CToken(int, CString const&)` `0x1025bc848`). Its `CString::GetTCharPtr` `0x1025bc894`,
`CString::GetSize` `0x1025bc8a0`, `_memcpy` `0x1025bc92c` and `strb` `0x1025bc930` at an unknown
index each lose the same bytes again. A trace keeps four causes, so the population shows the first
three and the latest; the full list comes from `inspect --effect-grammar country_event --trace`
with a larger `CAUSE_LIMIT` in a local build.

The `ldaprb` at `0x101e16bcc` is evaluated as a byte load; the guard calls were the obstacle.

### Bounded string and token construction (SDK-659 prerequisite)

On the same M451-hotfix executable, constructor walks execute the bound `CString` accessors
and model the C library imports `strlen`, `memcpy` and `memmove`. Engine function names select
bodies; they do not establish effects. An accessor with an unknown receiver remains opaque.
`strlen` preserves memory but returns a length only when every byte through the terminator is
known within the 4,096-byte analysis bound. This bound is not an engine capacity.

Copies require known source, destination and length, checked end addresses and a length at most
4,096. `memcpy` additionally requires nonoverlapping spans; `memmove` snapshots its source.
Byte values, loss traces and owner derivation move together. Unknown source bytes invalidate the corresponding
destination bytes. Missing arguments, overflowing spans and unsupported copies retain opaque-call
invalidation. A bounded write can overwrite an earlier member; only bytes outside its span survive.
The terminator is a separate executed store. A copy that is not bounded is an opaque call: under
the member-confined rule it loses the owner from its lowest given address on.

The exact-build controls execute both ABI entries of each constructor. With established source
state, `CString(char const*)` and `CToken(int, CString const&)` retain earlier owner bytes for
lengths 0, 3 and 22 and for 0, 3 and 255, and write their text and terminator within the inline
buffer. The long paths are not bounded and do not claim inline text. Length 23 gives the string
member to `CPdxCommonStringAllocator::allocate`, whose derived return makes the later copy lose
every owner byte. Length 256 calls `operator new[]` with only a size, so the earlier member
survives under the [member-confined rule](#assumption-code-changes-only-the-object-that-it-is-given).
Capacity and destination follow from executed initialization, loads and branches, not from the
class name. Unknown lengths do not establish the bounded path. These controls are independent of
`CEventTarget` and its global lexer source.

These bounded effects alone recovered no destination. With the SDK-660 rule, the later
event-target and registration calls keep earlier members, so all 35 arguments, including
`create_pop_group.size` and `spawn_megastructure.orbit_distance`, have storage.

### Global lexer string (SDK-671 investigation)

`CEventTarget::CEventTarget()` (`0x1004f6ed0`) gets its token text from
`CStaticLexer::GetString(0x165)` (`0x1004f6f10`). Its other ABI entry, `0x1004f71c8`, is one `b`
to `0x1004f6ed0` and has 784 direct callers. The facts below are from the M451-hotfix executable
and ARM64 slice in [targets](targets.md). The inspector (`--function`, `--callers`, `--symbols`)
read each body. A whole-image `llvm-objdump` pass found the address and call-site lists. The
results that the executable does not state are named as such. No method code reads these facts
yet.

#### Lexer storage

The lexer is a function-static object at `0x103796d08` (`L`). Its guard byte is `0x103796d00`.
Every accessor checks the guard with `ldaprb` and, on first use, calls `___cxa_guard_acquire`,
`CStaticLexer::CStaticLexer()` (`0x1025abe18`), `___cxa_atexit` and `___cxa_guard_release`.
`__GLOBAL__sub_I_lexer.cpp` (`0x1025af854`) does the same, and `__TEXT,__init_offsets` lists it
(entry 8256 of 8337). `__GLOBAL__sub_I_tokens.cpp` (`0x100de2918`, entry 2311) tail-calls
`GetTokenArray()`. These initializers run before `main`. A code walk cannot prove that a factory
runs after them, so the first-use path stays reachable for the method.

| Address | Field | Writers |
| --- | --- | --- |
| `L+0x0`, `L+0x8` | `CTokenTreeContainer` vtable, ternary-tree root | Constructor; `CTernary::Add`; `DestroyTokens` |
| `L+0x48` | Tokens initialized (byte) | `InitTokens` sets it; `DestroyTokens` clears it |
| `L+0x50`, `L+0x58`, `L+0x64` (`0x103796d6c`) | Dynamic `CPdxArray<CToken*>`: vtable, data, count | Constructor; `InsertAtEmplace` (`0x1025af640`) sets count to old + 1; `DestroyTokens` sets 0 |
| `L+0x68`, `L+0x70` (`0x103796d78`), `L+0x7c` (`0x103796d84`) | Lookup `CPdxArray<CString>`: vtable, data, size | Constructor; `RebuildLookup` through `SetSizeAndEmplace` (`0x1000b8cd8`) |
| `L+0x80` (`0x103796d88`) | Token count: highest id + 1 | `InitTokens` (max + 1); `AddDynamicToken` (new id + 1) |
| `L+0x84` (`0x103796d8c`) | Highest static id | `InitTokens` only |

The constructor zeroes `L+0x78` to `L+0x88` and then calls `InitTokens` (`0x1025ac9b4`).
`InitTokens` stores the highest static id, `0x4a04`, and the count, `0x4a05`. It also adds each
static text to the ternary tree.

`GetString(id)` (`0x1025ac638`) compares the size at `L+0x7c` with the count at `L+0x80`. It calls
`RebuildLookup` (`0x1025ac0e4`) when they differ. Then it returns `[L+0x70] + id * 0x28` with
`smaddl`. There is no bounds check.

#### Q1: the id-to-text relation

`GetTokenArray()` (`0x100d9e2a8`) has one guard and then 9,994 straight-line calls of
`CToken::CToken(int, char const*)` (`0x1025bc774`). Each call forms its receiver from
`0x10333d380` with constant `adrp`, `add`, `add …, lsl #12` or `mov` + `add` steps. Each id is a
`mov w1, #imm`, and each text is an `adrp` + `add` pair to a `__cstring` literal. The body has no
other branch. The census shows:

- Positions 0 to 9,993 each occur once at stride `0x120`.
- Ids are distinct and range from `0xb` to `0x4a04`. No position equals its id: position 0 has
  id `0xb` (`"id"`).
- Id `0x165` occurs once, at position 583 (`0x100da22c4`), with `"none"` at `0x102dcead9`.
- The longest text is 66 bytes. 2,433 texts are longer than 22 bytes.

`CToken(int, char const*)` stores the id at `+0x0` and points `+0x10` to its 256-byte inline
buffer at `+0x20`. It copies the text with `strlen` and `memcpy`, and adds a terminator.
`GetTokenType(i)` (`0x100d9e278`) returns `0x10333d380 + i * 0x120`, so it indexes by
**position**.

`RebuildLookup` fills the table in this order:

1. It calls `CString::Clear` on each existing entry. A short entry gets byte 0 and byte `0x17`
   set to zero. A long entry keeps its heap buffer and gets size 0.
2. It calls `SetSizeAndEmplace(count)`. When the array grows, this function moves the entries
   bitwise into a new `new[]` block, frees the old block through vtable slot `+0x20`, and zeroes
   the new entries.
3. For each position `i` below `GetNoOfTokenTypes()` (`0x270a`), it sets
   `table[GetTokenType(i)->id]` to `GetTokenType(i)->text` with `strlen` and
   `__assign_external`. The table index is the **id**.
4. For each dynamic token, it sets `table[token->id]` to `token->text`.
5. It stores 16 fixed texts.

The fixed texts in step 5 replace whatever steps 3 and 4 wrote:

| Id | Text | Static text it replaces |
| --- | --- | --- |
| 1, 2, 3, 4, 5, 6 | `=` `"` `{` `}` `(` `)` | none: not a static id |
| 8, 9, 16, 17, 18 | `,` `#` `\n` `\t` space | none: not a static id |
| `0x1d3`, `0x1d4` | `>` `<` | `greater_than`, `less_than` |
| `0x3cb`, `0x3cc` | `>=` `<=` | `greater_eq_than`, `less_eq_than` |
| `0x427` | `!=` | `not_eq` |

The relation can be derived without a branch on a class, command or build. Run the
`GetTokenArray` call sequence, then apply the `RebuildLookup` override stores. `0x165` is not an
override, so its text is `"none"`.

#### Q2: dynamic ids

`AddDynamicToken` (`0x1025acf9c`) returns the existing id when the ternary tree already has the
text. Otherwise it computes the new id as `[L+0x64] + [L+0x84] + 1` (`0x1025ad194`-`0x1025ad1a8`),
which is the dynamic count + `0x4a04` + 1. Then it allocates a `0x120`-byte token with
`operator new`, constructs it with that id, inserts it at the end of the dynamic array and stores
`id + 1` in `L+0x80`. The count starts at zero and grows by one for each insertion. The two
additions (`0x1025ad1a4`, `0x1025ad1a8`) are 32-bit, so the sum overflows before the count does:
count `0x7fffb5fb` gives id `0x80000000`, which is negative. A dynamic id is at least `0x4a05`
only while the count is below `0x7fffb5fb`. The executable has no check that enforces this. It is
a condition, not a derived bound, although it needs about 2.1 billion dynamic tokens. `IsDynamic`
(`0x1025ac914`) tests `L+0x84 < id < L+0x80` and agrees with this range.

`DestroyTokens` (`0x1025ad3fc`) runs only from the lexer destructor, which `___cxa_atexit` calls.
It sets the dynamic count to zero and clears the tree and the initialized flag. It does not change
`L+0x80` or `L+0x84`.

The identified writers of the lookup table and of the dynamic array are the lexer functions above.
The whole-image scan for code that forms an address in `0x103796d00`-`0x103796d97` found 14
functions:

- the 11 `CStaticLexer` functions;
- `CLexer::FindTok`;
- `__GLOBAL__sub_I_lexer.cpp`;
- `CStaticLexer::GetLookUpArray` (`0x1025aeea0`).

`GetLookUpArray` returns `L+0x68`. Its only direct caller, `CBinLexer::GetTok`, reads the size and
then copies a `GetString` result. `AccessStaticLexer` and `GetStaticLexer` return `L` and have no
direct callers. Calls through a register and pointer slots in data were not scanned. The scan
finds an address formed with `adrp` on page `0x103796000` and then `add` or a load offset; it
does not find an address formed from another page.

#### Q3: stores that the `GetString` call tree can make

| Path | Stores |
| --- | --- |
| Guard set, size equals count | None. Loads `L+0x7c`, `L+0x80` and `L+0x70` |
| Guard set, size differs | `RebuildLookup`: `CString::Clear` on entries; `SetSizeAndEmplace` (`operator new[]`, bitwise moves, an indirect free through `[L+0x68]+0x20`, and zeroing); `__assign_external` (`0x100010be4`) into each entry; `CPdxCommonStringAllocator::allocate` / `deallocate` for the 2,433 long texts; `GetTokenType` → `GetTokenArray` (guarded first use: 9,994 stores into `0x10333d380`) |
| Guard not set | `___cxa_guard_acquire`; the constructor's stores to `L`; `InitTokens` (stack `CString`, `operator new(4)`, `CTernary::Add` node allocation and stores through tree pointers, `CTernary::Unlock`); `___cxa_atexit`; `___cxa_guard_release`; and then the rebuild |

The rebuild path is live during content loading. Many readers and defines call
`AddDynamicToken` (563 direct call sites). Each new token makes the size differ from the count, so
the next `GetString` call does a rebuild.

All stores except the fast path go through pointers loaded from lexer memory: `[L+0x70]`, tree
nodes, and dynamic token pointers. The SDK-658 rules derive each such pointer from the escaped
owner, so the current walk cannot keep earlier owner bytes. The proof that is necessary is a
**confinement invariant** about the program, not a fact about the body of one function:

1. Every instruction that forms an address in the lexer object is in a known set of functions,
   and so is every pointer slot to it.
2. Those functions store into lexer-reachable memory only constants, results of their own
   allocations, values loaded from lexer memory, and copied text bytes. They never store a
   pointer from an argument or from outside the lexer.
3. Each interior pointer that leaves the lexer is used only to read. These are the `GetString`
   result, `GetLookUpArray`, `GetTokenType`, the tree values and the indirect calls through
   `L+0x0`, `L+0x50` and `L+0x68`.
4. The allocators, the C++ runtime guards and `___cxa_atexit` write only their own state and the
   block that they return. `CPdxCommonStringAllocator::allocate` (`0x1025127ec`) uses its pool
   (a spin lock and an indirect call) only when the mode word at the string's `+0x18` is 1 and
   the pool pointer at `+0x20` is not null. Otherwise it tail-calls `operator new`.
   `SetSizeAndEmplace` zeroes each new entry, so with intact lexer storage a table entry takes the
   `operator new` path. The ordinary allocator and runtime contracts are still necessary.

From items 1 to 4, no lexer pointer can hold an owner address, because only the factory's
allocation creates that address. Then the call tree cannot write owner bytes. The scan agrees
with item 1 but does not prove it: it misses addresses formed from another page, indirect
references and pointer slots. The lexer bodies above agree with item 2. For item 3,
`GetString` has 2,244 direct call sites: 2,239 `bl` and 5 tail `b`. The tail calls, such as
`TokenToString` (`0x1006c257c`) and the enum-name helpers (`0x1002fd9f0`, `0x1006c3238`,
`0x1006c3240`), return the pointer to their own callers, so their callers' uses count too. A use
classification of 2,238 sites sorts the result as follows:

| Use | Sites |
| --- | ---: |
| A load of byte `0x17` (inlined `GetTCharPtr` or `GetSize`) | 1,508 |
| A move to an argument register | 696 (`x2`: 653, `x1`: 41, `x3`: 2) |
| A move to a preserved register | 27 |
| The receiver of `CString::Find` | 2 |
| Not classified | 5 |

This census is incomplete: it does not include 6 sites (`PostReadInit` bodies and
`CPatronType::ParseCountryEventID`) or the callers of the tail wrappers. It is also not a proof
that every site only reads. Item 4 is outside the executable's local code. No current Native
mechanism proves any of the four items.

The derived table is the text that a **completed** rebuild writes. The text that a later call
returns also needs these conditions, which the executable does not establish:

- **Intact lexer storage.** No writer outside the census changes the table, the counters or the
  dynamic tokens (items 1 and 3). The lexer is alive: its destructor, which `___cxa_atexit`
  calls, has not run.
- **No retained incomplete rebuild.** `SetSizeAndEmplace` stores the new size
  (`0x1000b8db4`) before the static loop assigns texts with allocating calls (`0x1025ac198`). If
  an exception leaves a rebuild and execution continues, the size equals the count. Later calls
  then take the fast path, and entry `0x165` can stay `""` from the `Clear` pass. This can occur
  with sequential execution.
- **Sequential execution.** `RebuildLookup` has no lock. A concurrent reader can see an entry
  between `Clear` and its new assignment. It can also hold a pointer into a block that
  `SetSizeAndEmplace` moved and freed (`0x1000b8d4c`-`0x1000b8d80`).

#### Q4: the returned object

`__assign_external` writes a short string when the entry is short and the length is 22 or less.
It copies the text to entry `+0x0`, stores the length in byte `+0x17` and writes a terminator at
`+length`. A long entry keeps its heap buffer: the pointer is at `+0x0`, the size at `+0x8` and
the capacity at `+0x10`, with the high bit set. A `Clear` does not make a long entry short. Entry
`0x165` starts zeroed (short). With intact lexer storage it only receives `"none"`, so it stays
short.

For entry `0x165`, the evaluator therefore needs 0x28 bytes outside the owner with:

- `"none"` at `+0x0`;
- a zero byte at `+0x4`;
- the value 4 at `+0x17`.

The other bytes can stay unknown. `CString::GetTCharPtr` (`0x102523e94`) and `CString::GetSize`
(`0x102524808`) then run unchanged: byte `+0x17` is not negative, so they return the entry address
and 4. The returned pointer must be **underived**, the same as a pointer from constant image data,
so that the `CToken` copy does not count as an owner escape. `PopulateTokenString` then runs
`strlen` on the token's own buffer and calls `__assign_external` into its short `CString` at
`+0x28`. The constructor stored null at `+0x178`, so the parent loop does not run.

#### Q5: other users

Of the 2,238 classified `GetString` call sites, 669 pass a constant id (60 distinct ids) and 1,569
pass a computed id. Id `0x165` occurs at 72 sites: `CEventTarget::CEventTarget()`, four enum-name
helpers and 67 `TPdxRefDatabase<…>::WriteMembers`.

Command construction reaches the lexer through two event-target routes:

- **`CEventTarget::CEventTarget()`, id `0x165`.** A reverse walk of direct calls, to depth 6,
  finds 89 `C…Effect` or `C…Trigger` constructors that reach it. In the traced population
  (`command-population --trace`, `state_obstacles`), this constructor is the first loss of:
  - the 20 event `days` and `months/years` groups;
  - the 27 timed-flag `days/months/years` groups;
  - `add_modifier` and `add_stage_modifier` (`mult`, `multiplier` and the duration group);
  - `set_saved_date.days_from_present`, `effect_on_blob.owned_planets_percentage`,
    `spawn_megastructure.orbit_distance`, `release_vivarium_fauna_count.count`,
    `closest_system.min_steps` and `num_neighbor_systems.min_distance`.

  It is also the latest loss of `create_pop_group.size`, whose first loss is `CEffect::CEffect()`.
- **`CEventTarget::CreateFromToken(int)` → `CEventTarget::CEventTarget(int)` (`0x1004f8e88`).**
  The id is an argument. Effect and trigger factories pass these ids:

| Id | Text |
| --- | --- |
| `0x2c91` | `this` |
| `0x2c92` | `root` |
| `0x2c9b` | `random` |
| `0x346` | `default` |
| `0x2a2` | `auto` |
| `0x165` | `none` (`CCreateSpecies`) |
| `0x6b` | `target` |
| A computed id | Unknown (`CMemberOfFactionTrigger::PostInit` loads it from `+0xc0`) |

  This route then calls `ParseForSpecialValues`, which the `CEventTarget()` route does not call.

Every constant id above is a static id that is not an override, and each text is 22 bytes or
shorter. A table model therefore serves all of them. A computed id stays opaque.

#### Pitfalls

- The array position is not the token id. A table indexed by `GetTokenArray` position gives the
  wrong text for every id.
- A table derived only from `GetTokenArray` is wrong for `0x1d3`, `0x1d4`, `0x3cb`, `0x3cc` and
  `0x427`. `RebuildLookup` replaces them with operator text.
- `GetString` has no bounds check. An id at or above the count reads outside the table.
- `GetLookUpArray` returns the array object at `L+0x68`. It does not return the data pointer at
  `L+0x70`.
- `CStaticLexer::CStaticLexer()` has two bodies, `0x1025abe18` and `0x1025ad568`. Both call
  `InitTokens`. Only `0x1025abe18` has direct callers.
- A live read of the lexer confirms a static finding. It does not establish the state at the time
  of a factory call.

#### Outcome: the member-confined rule (SDK-660)

The investigation's designs needed either a lexer model with a recorded exception or a
whole-program write analysis. SDK-660 uses neither. It adopts the
[member-confined rule](#assumption-code-changes-only-the-object-that-it-is-given): `GetString` is
given only the token id in `w0`, so it cannot change the owner, and its return is underived. The
lexer facts above are not used by any method. They remain the reference for a future method that
must prove the lexer's text.

A throwaway experiment tested the rule first; its patch and outputs are in `.local/sdk-671/`. It
counted only `x0` in part 1, ignored callee taint outside the owner, left the modeled C calls and
the allocator handler unchanged, and did not taint what a reaching call writes. The delivered
method also counts known owner addresses in `x1` to `x8`, returns a callee's outside taint,
applies the return rule to every handler and taints the objects that a reaching call is given.
These stricter parts lose no destination: the 35 arguments and 50 omitted counts are the same as
the experiment's. They change fewer other answers: 137 commands instead of 347.

## World evaluation on M451-hotfix (SDK-647)

The world route was retired on 2026-10-02. Its code, the case file and the `world_numeric` live
cases are in Git at `d8f9d8a`; see [ready-world observations](ready-world.md) to restore them.
The results below remain knowledge.

The static method gave storage and selection. It gave no evaluated number. The numbers below
came from the engine's own effect execution in a loaded world, read through the retired
`Game::observe_world`. Native does not call `GetValue` itself. The build is the
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

`tests/expected/world-numeric-m451/cases.json` at `d8f9d8a` holds every case with its operand
text, the parser storage of its integer operand, and both results. The `world_numeric` live cases
ran it: one `check_script` session and five world sessions. Each case was evaluated in both
destinations.

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
  `check_script` first; the retired `world_numeric_stored` case did this for the table.
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
