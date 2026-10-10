# Scoped numeric operands

`ScopedOperand` reports routing forms only. The retired representation-selection and
literal-assignment properties, with their analysis versions and expected output, are in Git at
`d8f9d8a`, with the evaluated world results; their pitfalls are on
[ready-world observations](ready-world.md).

## Engine facts and method (M45-release to M452)

`CVariableValue::Read` and `Assign` share one operand reader. A destination's constructor vtable
point selects `CIntVariableValue` (signed 32-bit, scale 1), `CFixedPointVariableValue` (signed
64-bit, scale 100000) or the base type, which has no literal conversion. The reader ID stays the
shared entry's ID. Neither a command nor a key selects the subtype; a destination without
constructor agreement reports `UnresolvedStorage`.

The method matches complete `Read`, `Assign`, prefix, variable, `AssignSimple`, `GetValue` and
`GetValueInternal` bodies, and joins the `AssignSimple` vtable slot to the constructor-selected
address point; captured offsets must agree across the shared and concrete bodies. `forms` lists
the literal, `trigger:`, `modifier:`, `value:` and unprefixed variable routes; the qualified
event-target grammar is not established, and a recognized prefix does not prove that a lookup
finds a name. The `value:name|KEY|value|` parameters are in [script expansion](script-expansion.md).

Stored-representation rules (not evaluated results):

- A literal assignment writes the literal slot and returns before the reference location and
  slots change. A non-literal assignment writes the source location, then tries event target,
  prefixed references and variable text; each route writes its own slot, and clearing the others
  is not established.
- An empty location string makes `GetValue` select the literal. Otherwise `GetValueInternal`
  tries trigger, script value, modifier, then variable; the variable route is last and runs even
  when no name is stored. A failed lookup gives zero, never the literal slot.
- Observed storage agrees: a script-value lookup survives a later literal (the location stays
  nonempty), variable then literal keeps the variable text, and trigger then value stores both.
  A malformed numeric token after `7` keeps the numeric slot and becomes variable text.

Literals inherit the int or direct fixed-point range in
[numeric conversion](numeric-conversion.md#faithful-storage-and-endpoints) only when
constructor evidence establishes the concrete storage; unresolved destinations keep
`Reader.numeric: Unresolved`. The live matrix (`tests/expected/scoped-numeric-m452/`) checks the
inherited range at `overclock_cooldown` and `cycle_length_in_days`.

## Result on M452

With the SDK-721 change (`e238523`), `scoped-numeric-population` covers all 164 registries and
2,178 commands with no failed question; a capture of `main` gives the same report. Registry
destinations, including nested key paths: 107 in 21 registries, **0 complete, 97 partial, 10
failed**. The 97 partial ones are 95 signed 64-bit fixed point at scale 100000 and two signed
32-bit integer, all with a known range. The 10 failed ones are the `multiplier` and `mult` keys of
five triggered modifier blocks of `common/pop_jobs`, whose scoped destination vtable is not
established. Command arguments: 302, **0 complete, 302 partial, 0 failed**: 150 signed 32-bit
integer and 152 signed 64-bit fixed point at scale 100000, all with a known range. Every partial
answer keeps the scoped-literal conversion-boundary gap and the outside-method limit for qualified
scopes, parameters, lookup outcomes and evaluation. The token readers no longer keep
`numeric-lexical-boundary` ([text lexer](text-lexer.md)); the public gap text stays as it is
until SDK-720. A repeated operand `Merges` ([repeat behavior](registry-fields.md#repeat-behavior)).

## Live fixture pitfalls

- **An inline block corrupts the next definitions.** `{ base = 2 add = 3 }` given to this scalar
  reader stores `{` as variable text and disrupts parsing after it. Run inline-block cases alone.
- **`agenda_cooldown` has conditional storage routing** outside the fixture binding; its contrast
  inputs use `sensor_range`. Timed-flag operands use `agenda_cost` and `add_trust.amount` operands
  use `cycle_length_in_days` (`tests/expected/scoped-numeric-m452/cases.json` maps each case).
- An incomplete diagnostic source join is not complete coverage.

## Transfer to the 4.5.1 hotfix

The canonical bodies align instruction for instruction with M45-release, but diagnostic strings
carry another compiler source directory, and both numeric `GetValue` bodies form relocated local
jump-table bases with `adr`. Pinning those paths and absolute addresses made forms unresolved. The
matcher now captures a nonempty named diagnostic string (repeated uses in a body must agree) and
matches each local `adr` by its relative instruction position; a different target, an address
outside the body or any changed instruction still fails. Shape captures are in
`.local/sdk-650/hotfix/scoped-*`.

## Transfer to M452

The canonical bodies again align instruction for instruction. Two literals broke the match:

- The prefix body stores its new `CPostInitVariableValueDatabase` through an `adrp` page. The
  store's page offset was a literal in the shape; canonical shapes now name a global stored through
  a page, as they name one loaded through it.
- Both `GetValue` bodies switch on lexer token IDs, and M452 moved most IDs in the switch up by 9.
  The shapes pin every token, so `int_get.txt` and `fixed_get.txt` hold the M452 values. The method
  needs only the literal, location and size offsets from these bodies. Accepted limit: a build that
  adds keywords below these tokens needs the shapes derived again
  (`examples/inspect --derive-shape`). Replace the pinned tokens with captures if this recurs on
  the SDK-557 rehearsal.

## Constructor state on M451-hotfix

`engine/analysis/receivers.rs` owns constructor state; command factories consume it through
`declarations/receiver.rs`, registry destinations through `fields/persistent.rs`. The binding
follows a bounded graph of constructor symbols, including aliases. Compiler vtable-group
summaries are an independent baseline: its established points and bytes take precedence, and an
entered constructor body only adds facts. A summary of a class's own vtable-group points misses
these shapes:

| Shape | Engine example | Fact absent from a vtable-only summary |
| --- | --- | --- |
| Direct numeric member construction | `CSetTimedCountryFlagEffect` factory `0x101d9ab98`, operand `+0xa8`, integer constructor `0x100d1cc74` | The constructor's literal at operand `+0x200`; its primary point alone did not supply the omitted count |
| Out-of-line owner with integer members | `CCountryEventEffect` factory `0x101d46d5c` calls `CFireEventEffect` constructor `0x101d2713c` | Integer points at owner `+0xd8` and `+0x2e0`; `0x100d1cd40` stores the supplied integer at operand `+0x200` |
| Out-of-line owner with fixed-point members | `COrderedListEffect` constructor `0x101d251b8` calls `0x100d1bd80` at owner `+0x160`; `CAddModifierEffect` constructor `0x101e5ca24` calls `0x100d1be44` at `+0xb8` and `+0x2c0` | Embedded fixed-point points, absent from the owner's vtable group |
| Registry constructor cleanup fragment counted as an owner | `CPurgeType` constructor `0x100be48e8` and outlined cleanup `0x1028debe8` | The real constructor establishes the operand point at `+0x690`; intersecting it with the cleanup fragment removed it |

Outlined `[clone .cold.*]` cleanup fragments are not constructors. The one-instruction alias at
`0x100be4ae4` delegates to the full `CPurgeType` constructor, whose numeric constructor call at
`0x100be4a14` establishes `pop_decline_rate` as signed 64-bit, scale 100000.

### Owner derivation

The entered path tracks which values may point into the fresh owner; the module comment of
`evaluate/owner.rs` states the derivation and write rules. The owner starts at the factory's
`operator new` and at the registry owner's first argument; a registry constructor's caller is
not analysed. A pointer loaded from constant image data is underived.

#### Assumption: code changes only the object that it is given

This is a named assumption, not a proof; a proof needs a whole-program invariant (see the
[lexer investigation](#global-lexer-string)). It replaced an escape rule under which any call after
`CEffect` registration could write any owner byte. It has no branch on a class, command or build.
Rules that the module comment does not state:

- The modeled C calls follow the same return rule. `strlen` and a bounded `memcpy` or `memmove`
  keep their exact memory effects, but their returns are derived only when they are given an
  owner address. An allocation is given only its size, so a non-owner allocation returns nothing
  derived.
- An object outside the owner has an unknown size. On the stack, it reaches the end of the frame
  that holds it; after the stack pointer moved by an unknown amount, the walk adds no facts.
  Elsewhere, it is a pointer-sized slot and any run of held bytes that continues it.
- An owner byte is claimed when every returning path agrees on it after every later write that
  may change it. A walk with a path that does not return, a stack pointer moved by an unknown
  amount, a receiver outside the owner or a summary point that disagrees with a
  constructor-written word is not followed; its call is treated as not entered.
- An entered constructor returns its taint outside the owner as well as inside it
  (`Machine::install_owner_derived_memory`). A callee's write to a caller stack argument makes the
  caller forget its memory.
- A rebased pointer slot outside `__DATA_CONST` and the read-only sections is writable; the
  entered path reads it as unknown.
- Constructor tail branches use the same join as direct calls. New embedded summary points need
  constructor-written words; partly written points are never completed from metadata. Neither
  zeroed allocation nor `_bzero` behavior is assumed.

#### Pitfalls

- **An owner address stored at an unknown address is lost.** The store taints no byte, so a later
  load of it is underived. Code that stores `this` at an unknown place and reads it back to write
  an earlier member would leave a stale byte.
- **Out-parameters are counted only in `x0` to `x8`.** A call that writes an earlier member through
  a pointer in a stack argument or a preserved register is not seen.
- **An unknown derived `x0` loses the whole owner.** The long path of
  `CString::CString(char const*)` gives the string member to `CPdxCommonStringAllocator::allocate`;
  its derived return makes the later copy lose every owner byte. The long `CToken` path calls
  `operator new[]` with a size only and keeps earlier members.
- **Do not taint every byte that a call may forget.** Tainting the caller's saved-register slots
  makes the epilogue restore `x19` and `x20` as derived unknown values, and the next member call
  loses the whole owner. This lost the six `closest_system` and `num_neighbor_systems` arguments
  at `CTriggerDatabase::AddTrigger` `0x100d061ac`.
- **Compiler summary disagreement rejects a walk.** Every recovered point must agree with the
  vtable summary.
- Live observations confirm static answers; they do not initialize constructor state.

### Bounded string and token construction

Constructor walks execute the bound `CString` accessors and model `strlen`, `memcpy` and
`memmove`. `strlen` returns a length only when every byte through the terminator is known within
the 4,096-byte analysis bound (not an engine capacity). Copies need known source, destination and
length, checked ends and a length of at most 4,096; `memcpy` also needs nonoverlapping spans,
and `memmove` snapshots its source. Unknown source bytes invalidate the destination bytes; any
other copy is an opaque call. With known source state, `CString(char const*)` keeps earlier owner
bytes for lengths up to 22 and `CToken(int, CString const&)` up to 255; length 23 takes the
allocator path above. Capacity follows from executed code, not the class name.

### Global lexer string

No method reads these facts; they are the evidence that the assumption above rests on, and a
future method that must prove lexer text needs them (M451-hotfix).

- `CEventTarget::CEventTarget()` (`0x1004f6ed0`, also reached by a one-`b` entry at `0x1004f71c8`
  with 784 callers) gets its token text from `CStaticLexer::GetString(0x165)` (`0x1004f6f10`).
- The lexer is a function-static object at `0x103796d08` (`L`), guard byte `0x103796d00`;
  `__GLOBAL__sub_I_lexer.cpp` initializes it before `main`, but a code walk cannot prove a factory
  runs after that, so the first-use path stays reachable.
- `GetString(id)` (`0x1025ac638`) compares the lookup size `L+0x7c` with the token count `L+0x80`,
  calls `RebuildLookup` (`0x1025ac0e4`) when they differ, then returns `[L+0x70] + id * 0x28` with
  no bounds check. `AddDynamicToken` (563 direct call sites) changes the count, so rebuilds happen
  during content loading.
- `GetTokenArray()` (`0x100d9e2a8`) constructs 9,994 static tokens at `0x10333d380`, stride
  `0x120`; ids range from `0xb` to `0x4a04` and never equal their position. `RebuildLookup` indexes
  the table by **id**, adds the dynamic tokens, then overrides 16 fixed texts, including `0x1d3`,
  `0x1d4`, `0x3cb`, `0x3cc` and `0x427` (`>`, `<`, `>=`, `<=`, `!=`). Id `0x165` is `"none"`.
- A dynamic id is `[L+0x64] + [L+0x84] + 1` (dynamic count + `0x4a04` + 1) in 32-bit arithmetic;
  it overflows to negative at count `0x7fffb5fb`, which nothing checks.
- `CEventTarget::CreateFromToken(int)` passes constant ids `0x2c91` `this`, `0x2c92` `root`,
  `0x2c9b` `random`, `0x346` `default`, `0x2a2` `auto`, `0x165` `none` and `0x6b` `target`, or a
  computed id (`CMemberOfFactionTrigger::PostInit`); every constant is a non-override static id of
  22 bytes or less.
- Proving that the `GetString` call tree cannot write owner bytes needs a confinement invariant:
  every address into the lexer is formed in a known set of functions; they store only constants,
  their own allocations, lexer values and text into lexer memory; interior pointers that leave the
  lexer are only read; allocators and runtime guards write only their own state. No Native
  mechanism proves these, and a rebuild left incomplete by an exception or a concurrent reader can
  make the text differ.

Pitfalls: the array position is not the token id; a table from `GetTokenArray` alone is wrong for
the five overridden operator ids; `GetLookUpArray` returns the array object at `L+0x68`, not the
data pointer at `L+0x70`; `CStaticLexer::CStaticLexer()` has two bodies (`0x1025abe18` and
`0x1025ad568`); a live read of the lexer does not give its state at a factory call. The full
investigation is in Git before this page was condensed, and its throwaway experiment is in
`.local/sdk-671/`.
