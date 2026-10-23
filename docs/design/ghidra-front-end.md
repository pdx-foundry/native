# Design: a Ghidra analysis front end, split from game interaction

Approved by Jackson on 2026-10-22 with decisions D1 to D10 as recommended (below). This page is the
target design and the migration rule. The [technical design](architecture.md) describes the code as
it is now; it changes step by step as the migration lands. Linear project P-SDK-29 holds the
tickets.

## Why

Native does two different things: static analysis of one exact build for Atlas, and interaction
with a running game through per-build bindings. The static part uses an ARM64 interpreter, "the
walker", that matches compiled instruction shapes. About ten source idioms cause 97% of the
claims that a reader stop leaves open, and the compiler makes many shapes of each. A walker repair
clears one shape. A Ghidra export walks the decompiler's basic blocks instead: it read 272 readers
in 83 seconds after one analysis of the build, and an adapter that filled only what Native left
unresolved covered 548 more Milestone 5 claims with no loss.

## Decisions

| | Decision |
| --- | --- |
| D1 | Keep `pdx-native` as the facade that Atlas links, through Milestone 5. Decide again at the update rehearsal. |
| D2 | Adopt the Ghidra front end first; split the package second. |
| D3 | A per-build data directory with provenance is a build input, like the census; it is not an evidence archive ([simplification](simplification.md#decisions-of-2026-10-22)). The committed facts files hold typed rows, addresses and names only, never decompiled code. |
| D4 | Add `FieldCondition::ValueForm` and `ReaderKind::Presence` in step 3b, each with an exporter proof and an Atlas change. |
| D5 | No adapter trait until a second platform exists in the repository. |
| D6 | Milestone 5 stops on claim classes with decided end states plus a usefulness gate on vanilla content ([roadmap](../roadmap.md#milestone-5-acceptance)). |
| D7 | Commit `builds/<build>/` in this repository, one release at a time. |
| D8 | Generate the fixture section of `runtime.json`, with explicit unresolved states, for ordinary, inline and nested fixtures. |
| D9 | Keep `decode.rs` and the core of `evaluate/` as a table evaluator, with the retirement condition below. |
| D10 | Negative answers come from the whole-block contract and Atlas's existing rule, not from per-key negative facts. |

## Packages

One repository, one Cargo workspace, four packages. Atlas keeps one pin on a `main` commit and keeps
its `pdx-native` dependency.

| Package | Responsibility | Public surface | Depends on |
| --- | --- | --- | --- |
| `pdx-build` | Exact build identity (the universal-file SHA-256, the slice hash and architecture), the target catalogue's exact match, the per-build directory locator and `provenance.json` check, the `tables.json` schema (tokens, scope types and bits, manual exceptions), `sha256`. No analysis, no process. | `BuildId`, `ExecutableIdentity::read`, `Catalogue::lookup`, `BuildDirectory::open`, `Tables` | `object`, `sha2`, `serde` |
| `pdx-facts` | Every static operation that Atlas calls, with the `readers.json` and `census.json` loaders; the executable table readers; the table evaluator; recorded answers; the generator of `runtime.json` (a tool, never at question time) | Today's static `Native` methods and value types, `from_recorded_answers`, `record_answers_to`, `supports`, `category_tags(masks)` | `pdx-build`, `object`, `cpp_demangle`, `capstone`, `serde` |
| `pdx-game` | Launch, isolation, hooks, calls, observation, cleanup; the supervisor, the worker protocol, fixtures and script checks, the host-wide lock, the reducers; the `runtime.json` schema; one adapter for each catalogued build and host platform. `loaded_modifiers` returns the observed table (`ObservedModifiers`) with no static join. | `Game`, `GameOptions`, `start_game`, `Game::{observed_modifiers, check_script, observe_fixture, close}`, `ObservedModifiers`, `supervisor::serve`, `adapter::for_build` | `pdx-build`, `tokio`, `libc`; never `pdx-facts` |
| `pdx-native` | The facade. `Native::open` opens the installation once, holds a facts session and a game adapter, and forwards. It owns the live modifier join: it prepares the static tables before launch, so a static failure starts no game, and joins `ObservedModifiers` with the declared names, the families and `category_tags`. | Unchanged API names | all three |

A workspace test reads `cargo metadata` and fails when `pdx-game` depends on `pdx-facts`. The
fake-worker session test starts from a `runtime.json` in `pdx-game` alone.

**Category names stay bounded.** The category mask has 32 bits. `category_tags(masks)` runs the
table evaluator for the 32 single bits, the all-bits mask and the masks given, and returns the
`Tags` rule's result with its unresolved cases: a named compound mask, an unnamed combination of
named bits, or an unresolved name. Nothing is computed over the whole mask space.

## The per-build directory

```text
builds/<slice-sha256-prefix>/   one directory for each catalogued build
  provenance.json               universal and slice sha256, version label, architecture, schema
                                versions, Ghidra version, export script sha256, date
  tables.json                   pdx-build: tokens, scope types and bits, manual exceptions by
                                reader identity
  readers.json                  pdx-facts: the normalized export
  census.json                   pdx-facts: the required-field and subtype census
  runtime.json                  pdx-game: loader pins, the fixture section, the modifier table
                                layout, duration receivers, script-check bindings, hook sites
```

- `pdx-facts` reads `readers.json`, `tables.json` and `census.json`; `pdx-game` reads
  `runtime.json` and `tables.json`. No crate holds a literal engine address in Rust: the addresses
  in `recipes.rs` and `groups.rs` move into the directory. The locality gate scans all three crates;
  the directory is the binding authority, which it does not scan.
- Each file names the tool that wrote it (`ghidra-export`, `native-tables`,
  `runtime-projection`, `census-probe`, or by hand). An entry written by hand is a recorded manual
  exception (claim, conditions, obstacle, removal route) on its knowledge page.
- `open` hashes the executable, looks up the identity, opens the directory and refuses a
  different `provenance`. The supervisor opens the installation again, hashes it, opens the same
  directory and checks each pin it uses with the checks that exist: the symbol name at an address,
  the instruction bytes at a return pin, the inline loader's instruction checks, the `_pInstance`
  symbol. A failed check is `Unsupported`, never a fallback. The script-check receiver exception in
  the [technical design](architecture.md#target-records) ends when the receivers are in the
  directory; the count check stays.
- Git history keeps a retired build's directory.

### What the runtime takes from the static side

One `Recipe` holds static fields (`declarations`, `loader_reasons`, `field_requirements`) and
runtime fields (`strategy`, `fixture`, `script_checks`, `registry_layout`, `loader_databases`,
`default_registries`). The runtime derives the rest by running static analysis in the caller and
in the supervisor. Each value goes to the directory:

| Runtime value | Where it goes |
| --- | --- |
| Loader entry and key storage offset of each registry | `runtime.json` registries table, from the discovery that the `registries` answer uses |
| Ordinary fixture loader hooks and field storage | `runtime.json` fixture section |
| Inline fixture loaders and nested fixture fields | `runtime.json` fixture section |
| Modifier table layout, instance globals and item key offsets | `runtime.json`; the symbol check stays |
| Scope identity to scope bit | `tables.json`, shared with the `scopes` answer |
| Duration receivers of `check_script` | `runtime.json` receivers table, for each command; the supervisor keeps the count check |
| Script-check calls, object sizes, vtable writes, logger entry; fixture hook sites | `runtime.json` |
| Launch switches and environment | Stay in the platform leaf: they are host facts |

The [lifecycle page](../native/lifecycle.md#static-analysis-in-a-session) lists the code that
derives each value today.

### The fixture section (D8)

For each registry that a fixture can observe:

```text
loader:   Ordinary { load_entry, reader_entry, reader_return, constructor_entry, member_entry }
        | Inline   { loader_entry, root_return, key_storage: Storage, member_entry,
                     reader_call, object_read }
fields:   for each root field, by token:
          Bound        { storage: Storage }
        | Conditional  { reason }     more than one read alternative, or a condition
        | Incompatible { reason }     no decoder represents the reader's storage
        | Unavailable  { reason }     no unconditional direct read into owner storage is proved
nested:   for each (parent field, member field):
          { parent_token, owner_offset, member_entry, member: Bound | Conditional | Incompatible | Unavailable }
Storage = { offset, decoder: String | Integer | Integer16 | Float | FixedPoint{scale}
                          | ScopedNumeric{literal: Integer | FixedPoint{scale},
                                          layout: {literal, location, trigger, script_value,
                                                   modifier, modifier_unset, variable}} }
```

This is today's `FixtureOutcomeRegistryBinding`, `FixtureInlineLoader`, `FixtureNestedField`,
`FixtureStorageBinding` and `FixtureStorageDecoder`, with the unresolved states made explicit. The
facts tooling generates it from the export's `store` rows, the numeric facts and the scoped-object
layout proofs, under today's rules: one joined reader, unconditional paths whose domain is the
field's token, a reader argument relative to the owner, a compatible decoder, one persistent parent
reader for a nested member. Before the runtime reads it, a tool compares the section with the
output of `fixture_storage_binding`, `scoped_fixture_storage`, `inline_fixture` and
`nested_fixture_storage` for every registry of the field sweep and every inline and nested case,
by state. The switch needs no unexplained difference and the live fixture cases green, including
the inline `common/special_projects` cases and the nested `fleet_power` numeric case.

## The analysis pipeline

### Tools

- `tools/ghidra/Export.java` and `tools/ghidra/export.py` read a Ghidra project; `rows.py`
  classifies the walked events; `normalize.py` turns the raw export into `readers.json` and
  `provenance.json`; `schema.py` checks a build directory; `conformance/` holds the expectations
  and the check; [`README.md`](../../tools/ghidra/README.md) holds the setup (Ghidra 12.1.4,
  OpenJDK 25, 8 GB heap, about 18 minutes of analysis for each build, a 1.4 GB project) and the
  procedure for a build.
- `examples/ghidra-inputs` writes `tables.json` and each registry's root readers from Native's own
  analysis (`internals::build_tables`, `internals::registry_field_stops::root_readers`).
- The Ghidra project, the binary slice and the raw export are a local cache in the private Ghidra
  workspace. They are never committed. Only the normalized file is.
- Token identities come from Native's own lexer-table reader (`GetTokenArray()`), through
  `tables.json`.
- Ghidra is never a dependency of a library call. `cargo test` in CI loads the committed directory.

### Export schema

`readers.json` has the registries, each with the readers of its root keys, and the readers. Each
reader has a dispatch summary, a completeness flag with the failed conditions, and rows. A
registry or a nested reader names a reader by its entry address; a reader that the export did not
walk has `null` and its name. This is a reader of M452, shortened:

```json
{
  "schema": 1, "tool": "ghidra-export", "build": "c621723d…",
  "registries": {
    "common/situations": {"roots": [{"key": "", "class": "CSituationType", "reader": "0x100b7e9cc",
                                     "error": null}], "error": null}
  },
  "readers": {
    "0x100b7e9cc": {
      "name": "CSituationType::ReadMember", "class": "CSituationType", "walked": true, "error": null,
      "dispatch": {"form": "token-switch", "domain_enumerated": true,
                   "default": {"kind": "persistent", "reader": null, "followed": false},
                   "delegates": [], "key_text_read": false, "loops": 0, "truncated": false},
      "complete": true, "incomplete_because": [], "default_routed": [11637, 11638],
      "rows": [
        {"token": 794, "key": "category", "kind": "enum", "value": "Scalar", "reader_kind": "Keyword",
         "repeat": "Replace", "conditional": false, "family": null, "items": null, "branches": null,
         "nested": null, "scope_mask": null,
         "enum": {"converter": "TokenToEnum<EPolarityType>", "tokens": [11257, 11258, 12642],
                  "keys": ["positive", "negative", "neutral"], "complete": true, "unknown": "silent"},
         "lookup": null, "paths": {"leaves": 1, "unknown": 0, "truncated": false},
         "store": {"offset": 2856, "size": 4}},
        {"token": 15660, "key": "stages", "kind": "object-list", "value": "Block", "…": "…",
         "nested": {"reader": "0x100b7c374", "name": "CSituationStage::ReadMember", "walked": true}}
      ]
    }
  }
}
```

- `dispatch.form` is `token-switch`, `compare-chain` or `entry` (the recognized forms), `key-text`
  (a reader that compares or looks up the key's text), or `value-token` (a reader that tests the
  value's token, such as `ReadEntry`).
- `dispatch.default.kind` is `persistent` (`CPersistent::ReadMember`, which reports the key as
  unexpected), `log-and-skip`, `base-reader` (one other member reader, in `reader`), `entry` (the
  path of an entry reader), `return` (no call), or `helper` (anything else).
  `dispatch.delegates` are the other member readers that the paths call; the export follows the
  ones that token paths call, and not the extra readers of an unresolved default.
- An entry reader has one row with `token` and `key` `null`: the entry that it reads for any key.
- `kind: store` says only that the token's path stores something. It never shows that a value is
  accepted.
- `family` is `Trigger`, `Effect`, or `Weight` (a script value); `items` is the item kind of a
  list.
- `branches` keeps each outcome of a row whose path tests the value's token, for a block value
  token and for any other: kind, value shape, reader kind, repeat and nested reader. The proofs that
  turn them into paired answers are step 3b.
- `scope_mask` is the literal `EScopeType` argument of the row's reading call, such as
  `ReadTrigger`, `ReadEffect` or a weight reader; `null` when the argument is not a literal.
- `enum` is the `TokenToEnum<E>` table of a keyword row: the accepted tokens, their text, and
  whether an unknown name falls back silently or logs.
- `lookup` is the lookup function that a string member reaches at a use site (export phase 2).
- `store` is the owner offset and size, for the fixture section and the census traces.
- The export never writes decompiler text.

`tables.json` holds the token table, the scope types with their bits and public identities, the
reader offsets of the key text and the value token, and the manual exceptions (D5(a)).
`provenance.json` holds the build's identity (universal and slice SHA-256, version label,
architecture), the schema revisions, the Ghidra version, the SHA-256 of each export script, the
date, and the tool that wrote each file.

### Completeness

Completeness is a property of the dispatch analysis, not of the rows. A reader is `complete` only
when all of these hold:

1. The dispatch form is known (a token switch, a compare chain, or an entry-mode reader that reads
   the key text and compares no token), and its domain is enumerated: every case value is known
   and every compared constant is a token.
2. The default path is exactly one of: a base `ReadMember` that was followed and is complete; a
   log and skip; `CPersistent::ReadMember`. A default path that calls another helper is
   `default-unresolved`.
3. No path reached the budget and no loop was cut. A loop on a token path is `unknown` unless its
   body is a known helper (`ReadUniform*`, `InsertAtEmplace`, a statement loop whose body was
   walked).
4. Every delegate and every inherited reader was followed and is complete.
5. No path reads the key text, except in entry mode.
6. Every nested reader that a row names was resolved (by name or by vtable slot 5) and walked.

`incomplete_because` lists the conditions that fail: `dispatch-form`, `domain-not-enumerated` (1);
`default-unresolved`, `default-base-unfollowed`, `default-base-incomplete` (2); `budget`,
`loop-cut` (3); `delegate-unfollowed`, `delegate-incomplete` (4); `key-text-read` (5);
`nested-unresolved` (6); and `not-walked` for a reader that the export could not walk. A row that
reads its value into an object (`persistent`, `serializer`, `object`, `object-list`, `entry`)
without a resolved reader fails condition 6. A default path's value token is any token that the
reader does not enumerate: the walk decides an equality with an enumerated token, follows the
switch's default destination and both ways of any other test.

### The whole-block contract (D10)

Atlas does not see `default_routed`. It answers "not a field" for each eligible unlisted literal
key of a registry whose `registry_fields` answer is `Complete` with only `OutsideMethod` gaps,
whose naming is established, and whose loader drops the definition text. So `Complete` must mean
that the block's whole key domain is proved:

- The loader gives a registry `Completeness::Complete` only when every reader in the registry's
  tree, root and nested at every depth that the answer reports, is `complete`, has a default of a
  log and skip or a complete base reader, and is not in entry mode. An entry-mode block accepts
  any key; its `AnyKey` answer is complete on its own terms but gives no negative for an unlisted
  key.
- Any other reader in the tree makes the answer `Partial`, with a gap that names the reader and the
  failed condition.
- The rows of a complete base reader join the derived reader's table, so a key that the base
  reads is a field. Delegation is not rejection.
- `default_routed` stays in the export as a diagnostic.

Tests: a registry whose tree is complete is `Complete` and lists no unlisted key; a key outside the
exported tokens under that answer is absent; a key that the complete base reader reads is a field;
one incomplete nested reader makes the registry `Partial`.

### Conformance

`tools/ghidra/conformance/m452.json` lists readers with the flags that the export must give on
M452: `CNameListCharacter::ReadMember` (entry mode), `CNameListShipClass` and `CNameListShip` (the
key text is compared, so incomplete and not entry mode), a reader with a statement loop
(`CSituationType::ReadMember` `stages`), a reader with an unfollowed delegate, a reader whose
default calls a helper, the `CTriggeredEventDescription<T>::ReadEntry` family, and ten readers
that Native answers completely now (complete). A new export must match each expectation before it
is committed. This tests how the exporter makes the flags; the loader's negative control (a cut
reader gives `Partial`) tests only how they are used.

On M452 the unnamed cases are: the unfollowed delegate, `CSwappable<SPopJobTypeSwap>::CSerializer`,
whose default path calls `CSwappableBase::ReadMember` beside its serializer's reader (no token path
of M452 calls a reader that the export does not follow); the default that calls a helper,
`CAgreementPreset::ReadMember` (`CAgreementTermData::SetPresetData`); and the ten complete readers,
the root readers of ten of the 19 registries that Native answers `Complete` (`asteroid_belts`,
`colony_automation_categories`, `deposit_categories`, `ethic_categories`,
`game_concept_categories`, `leader_tiers`, `missions/mission_categories`, `policy_categories`,
`resolution_categories`, `timeline_events`). For all 19 the export lists the same keys as Native.

### The M452 export

The export of 2026-10-22 walks 275 readers in about 100 seconds; 166 are complete. Of the 187
registries under `common/` and `events`, 181 have root readers, and **55 have a complete reader
tree** under the whole-block contract: every reader from the roots down is complete, rejects the
keys that it does not enumerate, and is not an entry reader. For each of the 55, every root field
that Native's sweep lists is a key of the tree. The conditions that fail most often, by reader:
`default-unresolved` 49, `loop-cut` 45, `nested-unresolved` 34, `budget` 17,
`default-base-incomplete` 14. If too few registries become `Complete` for the negative answers that
Atlas needs, the next export work is these conditions by frequency, not a weaker contract.

### The table evaluator (D9)

Six methods evaluate compiled code for results, not destinations: category keys return masks,
modifier categories build strings, defines recover arguments at reader calls, localization runs
constructors, getters and link functions, scopes evaluate the keyword switch, and modifier nodes
run the node constructors for their category masks and the words they initialize. The live
modifier join names observed masks through the same category function. Jump tables alone cannot
give these results. These methods keep `decode.rs` and the core of `evaluate/` as the table
evaluator in `pdx-facts`. The export has no `switches` section.

**Retirement condition.** The table evaluator goes only when the export carries each method's
results, with the unresolved outcomes of each method, and that method's parity output does not
change.

### From the export to the answer types

Python shapes the export into a tree for each registry, once for each build. The Rust loader maps
the trees to the public types at question time:

| Export | Public answer | First proof (step 3) | Later |
| --- | --- | --- | --- |
| Scalar row with `reader_kind` | `ReaderKind` (Boolean, Integer, FixedPoint, Float, String, Reference, Keyword), `ValueShape::Scalar`, `RepeatBehavior::{Replace, Accumulate}` | yes | |
| Block row with a nested reader | `ValueShape::Block`, `FieldMembers::Fields` | yes | |
| Trigger or effect row | `BlockFamily::{Trigger, Effect}` | yes | |
| List row with an item kind | `FieldMembers::Items` | yes | |
| Entry-mode reader | `FieldMembers::AnyKey` | yes | |
| Row with `enum` | `FieldDomain::Listed` | yes | |
| Persistent read into a member that is not reset | `RepeatBehavior::Merges` | yes | |
| Row `scope_mask` | `Field.read_scope: Known(types)` through `tables.json`; mask 0 is `Known([])`, except for the D5(a) readers (below) | yes | |
| Whole-block completeness | `Completeness::Complete` under the contract; a Native gap that the export answers is removed | yes | |
| Row with `branches` | `ValueShape::Any`, reader `Unknown`, a gap: Atlas accepts one `Always` alternative only | yes | 3b: `FieldCondition::ValueForm { block }` with two `FieldReadAlternative`s (D4) |
| Row `kind: store` | The field exists; `FieldReadOutcome::Unresolved` with a `ReaderSemantics` gap | yes | 3b: `ReaderKind::Presence` only where the exporter proves that the path reads no value and stores a constant (D4) |
| Row `conditional` | `FieldCondition::Unresolved` | yes | Phase 2: `FieldZero` or `FieldKeyword` |
| A field that Native has and the export does not | The existing `Field` | Step 3 only adds | Step 4's field-level union keeps it |

`Source.method` is `registry-fields/ghidra-v<schema>`. `Basis` stays `StaticAnalysis`. An answer
carries no artifact hash; provenance is in the build directory.

**D5(a) zero-mask weight entries.** `scaled_modifier` and its `limit`, and
`complex_trigger_modifier` and its `potential`, keep `Unresolved` read scope with their
`OutsideMethod` gap, as the [recorded exception](simplification.md#decisions-of-2026-10-07) says.
`tables.json` lists them as a manual exception by reader identity, and the loader applies it before
the mask-0 rule. One regression test covers the ordinary all-zero case, one the exception.

### Versions

- `provenance.json` and each loader carry schema versions; a loader refuses a newer schema.
- A new export for the same build is a PR that replaces `readers.json`. Its review artifacts are the
  conformance check and the [measure](#measure), not the file diff.
- The local Ghidra project's cache key is the slice sha256 and the Ghidra version. A changed export
  script needs a new export.
- Parity tests that read the executable stay ignored behind `STELLARIS_PATH` and compare with
  `tests/expected/m452/` and `tests/population/m452/`.

## Facts that reader tables cannot give

| Open class | Source | Where it lands |
| --- | --- | --- |
| Required fields (`cardinality_minimum`) | The one-launch census probe; traced checks from the export's `store` offsets joined to post-read hooks (phase 2) | `census.json`; `FieldRequirement` unchanged |
| Scope names | `scope_mask` on reader rows and the scope table | `Field.read_scope`, `CommandGrammar.child_scopes` |
| Value forms at the use site and lookup categories | Export phase 2: references from each stored member offset to its getters and lookup function | `Field.uses`, `DerivedName`, `FieldReference`; the lookup-category decision is Atlas's |
| CWT placeholder keys and type-level lines | Atlas joins | Atlas |
| Fields absent at the claim path, types with no table | Atlas join defects (Atlas); the export's walk; the whole-block contract; root readers from the registry schedule | Atlas or `readers.json` |
| Keys that the build never reads | Absence under a `Complete` registry answer, by Atlas's rule | A negative answer |

## The runtime part

### What moves to `pdx-game`

With no change in behavior: `src/game.rs` and `src/game/driver.rs`, `src/execution/`,
`src/protocol/`, `src/supervisor.rs`, `src/fixture.rs`, `src/script.rs`, `src/work_directory.rs`
(without `sha256`), `src/engine/operations/`, `src/binding/platform/` with its worker files and
`build.rs` steps, `tests/live.rs` and `tests/live/`, and the `tools/observation/` tests. The join in
`src/session/loaded_modifiers.rs` moves to the facade.

Couplings to cut at the split:

- `protocol::observation` imports `Reader` and `ReaderKind` for `FixtureQuestionStorage::resolve`;
  the fixture section's decoder replaces them.
- `Game` keeps `Arc<Binding>` only for duration receivers; the receivers table replaces it.
- `loaded_modifiers.rs` imports `engine::analysis::{families, modifiers}` and `protocol`; in the
  facade it imports the facts answers, `category_tags` and `ObservedModifiers`.

### Adapter

One host platform is selected at compile time and one build is catalogued; `compose` resolves
`StrategyId::MacSuspendedChildLoaderEntry`. The adapter keeps that shape:

```rust
// pdx-game
pub struct Adapter {
    build: BuildId,
    runtime: RuntimeBinding,        // runtime.json and tables.json, typed and pin-checked
    strategy: platform::Strategy,   // the compiled host platform; a strategy that the host
                                    // cannot run is Unsupported, never a fallback
}
pub fn for_build(target: &Target, directory: &BuildDirectory) -> Result<Adapter, Unsupported>;
```

There is no trait (D5). The Windows prototypes use a different ownership model, and the wire types
assume ARM64 (`Machine.registers`, `CallBinding.widths`, `ObjectWrite`). The Windows step adds a
`StrategyId`, the platform trait and a machine-neutral wire. Atlas's end-to-end test runner uses
`pdx-game`'s `Adapter` and `Game` and needs nothing from `pdx-facts`.

`ExecutionPlan` is the one door from the runtime into the static side. Its `registry_bindings`,
`fixture_setup`, `observer` and `script_scopes` become reads of `runtime.json` and `tables.json`
with pin checks. Atlas's census probe becomes a `tools/census/` program on `pdx-game` that writes
`census.json`.

## The walker and its callers

"The walker" is the ARM64 path interpreter (`engine/analysis/evaluate/`, with `stop.rs` for its
stops) and the member walk built on it: `fields/`, `readers.rs`, `receivers/` and `grammar/`. The
method modules that run their own walk on the interpreter (`modifier_blocks/`, `weight_blocks/`,
`durations/`, `names/`, `dynamic_names/`, `families/`, `callbacks/`, `references/`,
`declarations/`, `modifier_nodes/`, `loader_rules.rs`, `expansions/`, `scoped_numeric/`,
`numeric/`) are listed by operation in [discovery](../native/discovery.md). The runtime callers are on the
[lifecycle page](../native/lifecycle.md#static-analysis-in-a-session). The table below lists each
entry point of the shared modules with every caller outside them, and two more interpreter walks
that other code depends on; test code is left out.

| Entry point | What it runs | Callers |
| --- | --- | --- |
| `fields::analyze` (`fields.rs:89`) | The member walk of one owner from its loader | `BoundAnalysis::owner_field_result` (`binding/analysis.rs:954`), kept per registry, for `registry_field_result` (`:898`) and `keyed_field_results` (`:915`), which serve `Native::registry_fields` (`session/questions.rs:308`, `:378`), the field stops (`session/registry_field_stops.rs:72`) and `derived_names` (`session/names.rs:42`); `fixture_registry_fields` (`binding/analysis.rs:871`); `inline_fixture` (`binding/analysis/fixtures.rs:47`) and `nested_fixture_storage` (`fixtures.rs:188`); `registry_field_input_and_result` (`session/questions.rs:694`) for `internals::duration_groups` (`session/duration_groups.rs:36`) |
| `fields::explore_member` (`fields/dispatch.rs:1182`) | The walk of one member reader | Command grammar (`grammar.rs:254`), field read scope (`grammar/read_scope.rs:48`), modifier blocks (`modifier_blocks.rs:146`) |
| `fields::follow_member` (`fields/member.rs:11`) | A nested member reader that a path calls | Command grammar (`grammar.rs:410`), field read scope (`grammar/read_scope.rs:87`), modifier blocks (`modifier_blocks.rs:167`), triggered modifiers (`modifier_blocks/triggered.rs:178`) |
| `fields::fields_and_gaps` (`fields/inventory.rs:9`) | Fields and gaps from walked paths | Command grammar (`grammar.rs:444`), modifier blocks (`modifier_blocks.rs:314`) |
| `fields::constructor_points` (`fields/persistent.rs:230`) | Constructor points of embedded objects | Triggered modifiers (`modifier_blocks/triggered.rs:110`) |
| `fields::serializer_read`, `serializer_owner_slot` (`fields/member.rs:103-124`) | A serializer's read and owner slot | Field input reader (`binding/binary/fields.rs:408`), modifier block input (`binding/binary/modifier_blocks.rs:184`) |
| `fields::named_key`, `recover_decoded`, `storage_offset`, `has_owner_receiver` | Token names, decoded tokens, storage offsets, receiver names | Grammar coverage and ordering (`grammar/coverage.rs`, `grammar/ordering.rs`), declarations input (`binding/binary/declarations.rs:19`), derived names (`names.rs:65`, `binding/binary/names.rs:13`) |
| `fields` result types (`RegistryFieldResult`, `RootField`, `TokenPath`, `ReaderJoin`, `Value`, `PersistentInput`, `ConcreteReader`) | The walk's output and input | Normalizers in `session/`: `questions.rs`, `fields.rs`, `read_scope.rs`, `scoped_numeric.rs`, `container_masks.rs`, `field_entries.rs`, `loader_kinds.rs`, `triggered_modifiers.rs`, `weight_blocks.rs`, `modifier_blocks.rs`, `durations.rs`, `duration_groups.rs`, `grammar.rs`, `numeric.rs`; methods `durations.rs:18`, `names.rs:65`, `weight_blocks.rs:51`, `grammar/forms.rs:13`, `modifier_blocks/{name,parameters}.rs`; fixture storage (`binding/analysis.rs:1150-1191`); input readers in `binding/binary/` |
| `readers.rs` predicates (`classify`, `destination`, `entry`, `scope_argument`, `arguments_join`, `classify_callee`, `scalar_width`, `is_member`, `converter` and the others) | Reader identity and storage by callee name | `fields/`, `grammar/`, `modifier_blocks/`, `weight_blocks.rs`, `durations.rs`, `names.rs`, `references.rs:480`; `binding/binary/{fields,families,grammar,references}.rs`; `binding/analysis/fixtures.rs:7`; `session/{fields,read_scope,scoped_numeric,container_masks,field_entries,triggered_modifiers,weight_blocks,modifier_blocks,durations}.rs` |
| `receivers` (`install_vtables`, `accept_entered_path`, `join_byte_traces`, `ConstructorCalls`, `ConstructorImage`) | Constructor and vtable state for a walk | `fields/persistent.rs`, `fields/records.rs:545`, `declarations/receiver.rs:10`, `declarations.rs:17`, `weight_blocks.rs:992`, `grammar/numeric.rs:113` |
| `grammar::analyze` (`grammar.rs:199`) | The command grammar walk of one factory | `registered_grammar` (`session/grammar.rs:67`) for `Native::command_grammar` (`session/grammar.rs:59`), `internals::command_grammar_stops` (`session/command_grammar_stops.rs:197`) and the duration receivers of `Game::check_script` (`session/script_durations.rs:53`) |
| `grammar::targets`, `grammar::forms` types | Target getters and value forms | `internals::target_getters` (`session/target_getters.rs:2`), `session/{grammar,command_grammar_stops,scoped_numeric,read_scope,durations}.rs` |
| `member_storage` (`binding/analysis/fixtures.rs:224`) | A direct interpreter walk of a member reader for one token, to the reader that stores it | The inline fixture key (`fixtures.rs:53`) and nested fixture fields (`fixtures.rs:194`); the fixture section replaces it in step 5a |
| `modifier_nodes::analyze` (through `BoundAnalysis::modifier_node_result`, `binding/analysis.rs:1673`) | Interpreter runs of the modifier node constructors | `Native::modifier_nodes` (`session/modifier_nodes.rs:31`) and the registry field input (`binding/analysis.rs:982`, the initialized words). It is a [table evaluator](#the-table-evaluator-d9) method and keeps `evaluate/` under that retirement condition |
| `evaluate::Machine` and `stop` | The interpreter and its stops | Every method module above; `families::item_key_offset` and `modifier_table::derive` for the runtime; `internals::inspect` (`binding/inspect.rs:25`) and `internals::trace_causes` |

Two dependencies run the other way and the migration must keep them: the registry field input takes
the scopes that weight blocks store and the words that modifier nodes initialize
(`registry_field_input`, `binding/analysis.rs:974-990`), and the `registry_fields` answer reads the
reference, scoped numeric, modifier block, weight block, triggered modifier and block-context facts
(`session/questions.rs:414-420`).

## Migration

**Rule.** A walker module is deleted only after every caller of it, static or runtime, reads the
export or the build directory. A method's step retires its own entry points only; the shared
modules go last, when `cargo` shows no caller. A method whose export step does not pass the gate
keeps its walker and the shared modules that it needs, and its knowledge page says so.

1. Inventory, measure and design page (this page).
2. `tools/ghidra/` with conformance, and the M452 facts directory, with no answer change.
3. First proof: a loader fills `registry_fields` where Native is unresolved or has no field, under
   the whole-block contract. Gate: at least +548 on the measure against the frozen baseline, none
   lost. 3b: paired outcomes and presence (D4), with Atlas, at a pin move.
4. Field by field, the export wins where it is established; the walker fills where the export is
   unresolved; each disagreement and each missing field is read by hand.
5. Replace, then retire, one method at a time: 5a registry fields and fixtures, 5b modifier, weight
   and triggered-modifier blocks, 5c references and read scope, 5d command grammar, durations and
   script-check receivers, 5e derived, dynamic and family names, 5f loader rules, expansions and
   callbacks, 5g the shared modules. `decode.rs` and the table evaluator stay.
6. The workspace split.
7. Export phase 2: use-site lookups, read conditions, traced requirement checks.
8. The Windows adapter, with Atlas's test runner.

## Measure

`tools/measure/m5-static.sh` measures a static change. It builds Atlas at a fixed commit against a
Native commit or checkout (a copy of Atlas with a `[patch]` from `--config`; the Atlas checkout is
read only), takes a static snapshot (`pdx-atlas snapshot --static`) with the session answers from a
live recording of the same build, runs `pdx-atlas ledger` with the accepted run's `covered/` as its
baseline, and counts the Atlas-owned claims under `common/` and `events/` (owners `engine_fact` and
`content_derived`; 24,610 at the pin). Each run makes two comparisons with the same recording,
config revision, Atlas commit, build and installed content; the analyzer refuses a reference run
with other inputs:

- **Gate:** against the accepted result, which is the last merged change. Lost must be 0.
- **Cumulative:** against the frozen baseline at the Atlas pin, to report progress.

```sh
tools/measure/m5-static.sh run <name> <native commit or checkout>  # measure, gate, cumulative
tools/measure/m5-static.sh accept <name>    # after the change merges; it passed its gate against
                                            # the current accepted run
tools/measure/m5-static.sh run --new-pin <name> <native>  # at a pin move: new Atlas, config or
                                            # recording; no comparison
tools/measure/m5-static.sh freeze <name>    # that run becomes the baseline and the accepted result
```

Results are in `.local/m5/` of the main checkout: `runs/<name>/` (inputs, snapshot, reports,
`measure.json`, `covered.txt`, `gaps.md`), `baseline` and `accepted`. The default recording is
`.local/m5/recording`, a copy of Atlas's pin move B live recording (its session answers are the
same as those at Native `9fc7ced`). A static run equals a live one only while no covered claim
rests on a live observation; Atlas checks that at each pin move, and the milestone number still
comes from a live capture. Each new disagreement between Native and the export, and each field that
the export does not have, is read by hand before a merge.
