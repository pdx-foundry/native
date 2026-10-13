# Write a discovery method

Write one method in one task: explore the executable, record the findings, then deliver the method
with authored tests, parity output and a run over its whole population
([development policy](../development-policy.md#write-a-method-in-one-task)). The
[method index](discovery.md) locates the operations and their code.

Always run examples with `cargo run --release --example NAME -- …`, never by invoking
`target/release/examples/NAME` directly. Cargo rebuilds the example when its source changes;
a previously built binary can silently report stale results after an edit.

## Inspect the exact build

Start with a shape census before choosing a method. Match the executable to [targets](targets.md),
read the subject's method page, and set `STELLARIS_PATH` to the installation or executable.
[The developer inspector](../../examples/inspect.rs) reads ARM64 images without starting a game;
its image modes also work on uncatalogued builds. `--image PATH` overrides the environment.

```sh
cargo run --release --example inspect -- --lookup-census 'NParserUtil::ReadKeyReference<'
```

The census groups every matching body (cold clones excluded, aliases counted once) by the reference
matcher's canonical lines; `--normalize-field-offsets` also ignores object-register displacements. A
group is a research grouping, not proof of equal semantics.

Derive a draft from exact symbol names or addresses selected from the census:

```sh
cargo run --release --example inspect -- --derive-shape 'NAME1' 'NAME2' 'NAME3' > draft.shape
```

The draft replaces differing tokens or targets with numbered placeholders; bodies of different
lengths fail. Equal lengths mean positional alignment only: review control flow, name the
placeholders and add the semantics before committing a shape.

Use `--symbols TEXT` to find names, `--function NAME` to inspect instructions, `--callers NAME` for
direct calls, `--strings TEXT` for literal address references, `--slots NAME --count N` for fixed-up
pointer slots, and `--lookup-lines NAME` for strict canonical lines.

Limits of the inspector output:

- Each run first prints the image hashes and whether chained fixups were read. Without them, no
  data slot is resolved, and the run prints why.
- Function extents come from symbols, so every end is an inferred boundary. Indirect branches stay
  unresolved, and a jump table shows only the addresses that the code forms.
- Callers are direct `bl` and `b` only. A string reference is `adr`, or `adrp` then `add` in one
  function with no branch or write between them.
- The inspector reads ARM64 Mach-O images only. Its entry, `pdx_native::internals::inspect`, is not a
  consumer API.

On a catalogued build, inspect a field method's stopped token paths with:

```sh
cargo run --release --example inspect -- --registry-fields common/megastructures
```

This shows each stop's reason, obstacle, instruction, symbol and offset, code entry and last
instructions, followed by the internal gaps before normalization. `--trigger-grammar NAME` and
`--effect-grammar NAME` do the same for one command's child grammar. They also show the
obstruction when the command's receiver join stops, and list each child key whose initial owner
storage the factory does not establish: a scoped destination's vtable point or a duration group's
omitted count. Record new engine facts and failed shapes on the method page as you find them.

For the [block entry contexts](registry-fields.md#block-entry-contexts) of one registry's trigger
and effect fields, use `--entry-contexts`:

```sh
cargo run --release --example inspect -- --entry-contexts common/council_agendas
```

It lists each field with its block offsets and entry gaps, then each entry run that reaches one
of the owner's blocks: the entry call, the wrapper or evaluator that it calls, each evaluation
reached with its block and context, each unresolved stop, and each bound with whether it was
charged. A bound is charged only on a contradiction: no evaluation reached, or one with an
unreadable context. A reason charged while climbing to the entry (`no-caller`, `caller-depth`)
belongs to no run; it appears only in the field's gaps.

The internal results of registry fields, scopes, scope links, localization, modifiers and
modifier families keep the stop diagnostic of `src/engine/analysis/stop.rs`, and so do the block
entry runs (`EntryRun`). Callbacks and defines keep only the reason word, because they combine
reasons across paths. Declaration scopes
are read without a walk, so they have no stop.
`pdx_native::internals::registry_field_stops::run` runs the registry field method once and
returns its internal result with the public answer derived from it, and the block entry runs and
entry gaps of the registry's owner.

Add `--trace` to learn where a needed value may have been lost:

```sh
cargo run --release --example inspect -- --effect-grammar pop_change_ethic --trace
```

A stop at an unknown register or flags, the `factory-return` and `command-vtable` receiver
checks, and each initial-state stop then list each cause with its instruction and code entry:

- a call that returned no known value, or clobbered a caller-saved register or the flags;
- memory that the method invalidated, such as an object after an unrecognized call;
- a store to an unknown address that may have overwritten memory;
- a loop head, or the return of an entered constructor, where paths disagreed on the value;
- a bound constructor whose body the method could not follow, such as one with a path that does
  not return.

The run follows the value through moves, loads and stores on its own path, and through the
constructor bodies that it enters: a byte that a member constructor loses names the instruction
and function inside that constructor, at any depth. A memory byte lists, in order, each place that
may have overwritten it since its last definite store. The first is where it stopped being known;
the last is its latest loss, so a recovery must repair each one. Memory that was never known stays
marked as partly unrecorded. Limits:

- A trace keeps at most `CAUSE_LIMIT` (4) causes and says when the list is incomplete. A memory
  byte then keeps its first three losses and its latest.
- `command-population --trace` traces the whole inventory and groups the
  initial-state stops by the functions of their first and latest loss (`state_obstacles`).
- More than one cause means any of them may apply. Where joined paths lost a value for different
  reasons, the trace lists each.
- A path that went on from a loop head does not learn the causes of a later arrival there that
  its facts already cover. That arrival's causes reach only paths that widen the facts later.
- An unrecorded part means that the value was unknown when the walk began, was in memory that
  the path never wrote, or passed through a vector register. Vector registers are not traced.
- Only walks of the shared evaluator (`evaluate.rs`) are traced. Registry field token paths
  come from the dispatch walker, which says that it records no causes.
- Tracing changes no answer, stop or comparison. It applies to analysis that runs inside the
  traced call; a `Native` that already cached an analysis does not rerun it.

In code, `pdx_native::internals::trace_causes(|| ...)` turns tracing on for the closure.
`Unresolved::trace` holds the result.

## Implement the shared method and its stops

Use the existing module comments to locate the method's inputs, bounds and result. Decide what
the operation establishes, how it treats invalid input and what must remain a gap. Repair the
shared module that owns an unfamiliar instruction or shape. An executable-derived fact belongs
in that method; a fact specific to a build belongs in `src/binding/` target records or recipes.
Never select behavior by a registry, command, field, class or build name.

Use [`Unresolved` and `Stop`](../../src/engine/analysis/stop.rs) for a walk that cannot finish.
`Unresolved::at` carries the instruction, the most recent code entry and the obstacle: an unknown
register or flags, a spent bound, unsupported code, an address outside the read code, a cycle or
a call not followed. Use `Unresolved::new` when no walk located the obstruction. Keep addresses
in internal diagnostics; public gaps quote the reason only. Callbacks and defines currently
combine reasons across paths and retain only the reason word.

Run the [locality gate](../../tests/locality.rs) while implementing:

```sh
cargo test --test locality
```

It scans production code outside the binding authority for content directories, engine-subject
comparisons, build registry counts and version checks. Tests may name build-specific regression
expectations. A manual exception needs its claim, conditions, obstacle and removal route in the
knowledge page and a matching gate entry; a special case is not a repair of the shared method.

## Author ARM64 tests

Use the test-only [`arm64!` macro and `Arm64` builder](../../src/engine/analysis/assembler.rs)
for instruction bytes. Test the shape that establishes the result, then a negative control that
removes a required fact or makes it ambiguous. Assert the resulting gap and stop where relevant.

```rust
use crate::engine::analysis::assembler::{Arm64, arm64};

let bytes = arm64!(at 0x1000;
    mov w1, #7; // token 7
    adrp x2, extern 0x8000; // "new_engine_field"
    add x2, x2, #0;
    bl extern 0x3000; // CToken::CToken
    ret
);

const NAME: u64 = 0x8010;
const CONSTRUCTOR: u64 = 0x3000;
let mut code = Arm64::at(0x1000);
arm64!(code; mov w1, #7);
code.address(2, NAME).call(CONSTRUCTOR);
arm64!(code; ret);
let named_bytes = code.bytes();
```

Write an `adrp` target as its page: dynasm rounds an unaligned target up. `Arm64::address` and
`Arm64::load` handle the page and offset for a named address. The assembler's module comment
lists the other syntax differences from the decoder. [AGENTS.md](../../AGENTS.md) permits inline
assembly comments that explain test intent, such as the token or field name above.

Use [`analysis_support.rs`](../../src/engine/analysis/analysis_support.rs) when a test needs an
image: `macho_with_text` wraps authored bytes at `0x1000`, the examples' assembly base;
`macho_with_fixups` supplies symbols, strings, a jump table and a chained pointer. Keep these
authored tests independent of an installed game.

## Check parity on the supported build

Keep reviewed expected output in [`tests/expected/m452/`](../../tests/expected/m452/), and check it
in [`tests/static_questions.rs`](../../tests/static_questions.rs). Those ignored tests read the
exact catalogued executable and start no game. The ignored library tests read the same executable:

```sh
cargo test --release --lib --test static_questions -- --ignored
```

`STELLARIS_PATH` must be set for this command; `cargo parity` is its alias. Check source
stamp, values, completeness and gaps, including an unresolved or invalid case. Inspect a changed
answer before changing its expected file. A parity sample does not replace the population run.

Each parity binary opens the installation once, and its tests share that `Native` and its cached
analysis: use `native()` in `tests/static_questions.rs` and `session::installed_native()` in the
library. A test that records answers, traces causes, changes the executable or must start from an
empty analysis opens its own `Native`.

Generate a candidate tree with the same questions and selections as the parity tests:

```sh
cargo run --release --example expected -- --out /tmp/native-expected-candidate
cargo run --release --example expected -- --compare tests/expected/m452 /tmp/native-expected-candidate --build BUILD_ID
```

To review one or a few files, add `--only` with their names, for example
`--out /tmp/native-expected-candidate --only triggered-modifiers.json`. Each file is the same as
the file from a full run, and an unknown name lists the known names. The tree comparison
reports the files that were not written as missing.

The output directory must be new and outside `tests/expected/`; the command never overwrites a
file, and the comparison is offline. Pass the exact candidate build ID from `Native::build()` or
the [target catalogue](targets.md), without JSON quotes. Exit status is 0 for parity, 1 for parity
failures, 2 for invalid input. `Answer` and `Provenance` differences fail, and a build stamp
that differs from the reviewed one is a `Provenance` failure in every file (`read-scopes.json`,
`command-grammars.json` and the duration file); after a build change, regenerate those files
before parity passes. `Ordering` permits only moves of complete dynamic-namespace rows; other files
keep byte equality (`Layout`). `on-actions.json` and `game-rules.json` compare by name: one
difference for each name whose entries changed (`/names/<name>`) and for each gap kind and subject
whose details changed (`/gaps/<kind>/answer_item/<name>`), so one removed gap is one line; the same
content in another order is one `Ordering` failure. A clipped terminal report ends with the path of
the full report under `.local/parity/`.

Stored-duration behavior can be compared against an existing fresh live report without repeating
the session:

```sh
cargo run --release --example expected -- --compare-durations tests/expected/duration-m452/live.json .local/durations/live.json --build BUILD_ID
```

The duration mode requires the candidate build to equal `BUILD_ID` and the reviewed build to equal
the candidate build, then compares `cases`, including diagnostic and stored count order. A new
build needs a fresh live run to refresh the reviewed duration file.

The generator is not verification. Review each changed answer against the method's tests and
engine evidence. Copy only reviewed files back, then run `cargo parity`, for example:

```sh
cp /tmp/native-expected-candidate/fields-traditions.json tests/expected/m452/fields-traditions.json
cargo parity
```

The tracked files supply sample keys, not replacement answers: a missing selected item appears as
`null`, and full inventories include new items. Tests and the generator share
[`tests/parity/`](../../tests/parity/mod.rs).

## Iterate on selected commands and registries

The scoped numeric and duration examples accept repeatable exact-name filters:

```sh
cargo run --release --example scoped-numeric-population -- --command Effect/country_event
cargo run --release --example duration-population -- --command Effect/country_event --command Trigger/has_country_flag
cargo run --release --example scoped-numeric-population -- --registry common/megastructures
```

With no filters, both examples measure the full population; with filters, only the union of the
requested commands and registries, in the full report shape. Names are case-sensitive, and unknown
names fail. Use the same selection on both sides of a focused comparison.

While iterating, measure the affected commands and registries. Before delivery, freeze the final
source and run the full populations once, then compare with `main`. Review fixes can use focused
runs until the final source is ready for another full measurement.

### Capture a stable main and branch pair

Commit the candidate changes first. Resolve `main` (or a freshly fetched `origin/main`) to a commit,
and use separate detached worktrees for both captures. Do not edit either capture worktree or the
installation during the run. Separate worktrees prevent an edit between examples from silently
rebuilding only the remaining examples from different source. Cargo's separate target directories
also keep the two builds independent.

Run this from the candidate checkout, with `STELLARIS_PATH` exported:

```sh
set -e
capture_root=$(mktemp -d "${TMPDIR:-/tmp}/native-population.XXXXXX")
main_commit=$(git rev-parse main)
candidate_commit=$(git rev-parse HEAD)
git worktree add --detach "$capture_root/main-tree" "$main_commit"
git worktree add --detach "$capture_root/candidate-tree" "$candidate_commit"
printf '%s\n' "$main_commit" > "$capture_root/main-commit.txt"
printf '%s\n' "$candidate_commit" > "$capture_root/candidate-commit.txt"
for side in main candidate; do
    mkdir "$capture_root/$side"
    for example in scoped-numeric-population duration-population command-population; do
        (
            cd "$capture_root/$side-tree"
            cargo run --release --example "$example"
        ) > "$capture_root/$side/$example.json.tmp"
        mv "$capture_root/$side/$example.json.tmp" "$capture_root/$side/$example.json"
    done
done
for example in scoped-numeric-population duration-population command-population; do
    python3 tools/population/compare.py \
        "$capture_root/main/$example.json" "$capture_root/candidate/$example.json" \
        > "$capture_root/$example-comparison.json"
done
```

Keep the reports and commit files until delivery. Remove the two clean capture worktrees with
`git worktree remove` when finished. Run long captures through the repository's run-and-queue
workflow. A focused capture can use the same isolated checkouts once both revisions support filters.

### Compare existing population reports

[`tools/population/compare.py`](../../tools/population/compare.py) reads two reports without a game:

```sh
python3 tools/population/compare.py before.json after.json > comparison.json
```

It reads scoped numeric, duration and command reports, including compact command baselines, of the
same type and exact build. Exit status is 0 with no regressions, 1 with regressions and 2 for
invalid input. A regression is a removed entry, a lost `Known` or `Partial` property, `Known`
downgraded to `Partial`, or a changed established value; `Known(null)` is an established absence.
Alternatives match one-to-one by preserved facts; semantic sequences keep their order. Unresolved
markers can gain facts. Command comparisons count gap changes, but only lost or changed facts,
weaker completeness or increased inventory uncertainty fail; stamps, timings and internal
diagnostics are ignored. Registry duration groups carry `{"Known": value}` or `"Unresolved"`
like every other property, so a changed error diagnostic is not a regression; a report that
still has the older Debug text (`Ok(Some(1))`) is invalid input, so capture `main` again. The tool
does not synthesize serde defaults for older report schemas; use `command-population --diff` for
those.

## Run over the whole population

Follow [Measuring method transfer](../development-policy.md#measuring-method-transfer). Run the
method over every discovered registry, or the whole command or other inventory it reads.
For fields, use [`registry-field-sweep.rs`](../../examples/registry-field-sweep.rs):

```sh
mkdir -p .local/population
cargo run --release --example registry-field-sweep -- "$STELLARIS_PATH" > .local/population/fields.json
cargo run --release --example registry-field-sweep -- --diff tests/population/m452/registry-field-sweep .local/population/fields.json
```

The tracked [population reports](../../tests/population/m452/) are the
baseline. The diff compares normalized answers, ignoring method and Native version stamps but not
build and basis, and reports once which absent members matched their serde defaults. It fails on a
report that is of another kind, has no `cases` or `inventories` array, has a row without a name or
repeats a subject, and on a baseline directory without the build in its `header.json`; `tools/population/compare.py`
rejects the same inputs. An empty report still compares.

For command grammars, run and compare the whole population. `command-population` reads
`STELLARIS_PATH`:

```sh
cargo run --release --example command-population > .local/population/commands.json
cargo run --release --example command-population -- --diff tests/population/m452/command-population .local/population/commands.json
```

A tracked baseline can be stale on `main`. To separate what your change did from what `main`
already changed, run the same example on a `main` worktree and diff the two reports. The worktree
goes in the ignored `.local/main/` and builds into its own `target/`; the first build is a full
release build. One example builds before it runs, so the branch side can use the current
checkout; for a multi-example capture before delivery, use
[a stable main and branch pair](#capture-a-stable-main-and-branch-pair).

```sh
git fetch origin main
git worktree add --detach .local/main origin/main
(cd .local/main && cargo run --release --example command-population) > .local/population/commands-main.json
cargo run --release --example command-population > .local/population/commands.json
cargo run --release --example command-population -- --diff .local/population/commands-main.json .local/population/commands.json
git worktree remove .local/main
```

The field report's `entry_contexts` section holds the block counts that
[registry fields](registry-fields.md#block-entry-contexts) records: trigger and effect blocks with
contexts and no entry gap, with contexts and a gap, and with none; several known `this`, typed
`from` and `prev`; contexts with an unresolved scope; entry gaps by detail; and unnamed evaluation
calls by registry. The on_action and game rule counts that
[engine commands](engine-commands.md#on_actions-game-rules-and-their-entry-scopes) records come
from the parity files, offline:

```sh
python3 tools/population/callbacks.py tests/expected/m452/on-actions.json tests/expected/m452/game-rules.json
```

`registry-field-sweep` takes the same steps. `dynamic-name-population` has no `--diff`; compare its
text output without the timing line:

```sh
diff <(grep -v '^method run' .local/population/names-main.txt) <(grep -v '^method run' .local/population/names.txt)
```

Keep the worktree to compare later changes against the same `main`. Remove it when you finish; that
also deletes its build.

Review changed answers, then update the affected baseline in the same PR as the method change:

```sh
cargo run --release --example registry-field-sweep -- --baseline tests/population/m452/registry-field-sweep "$STELLARIS_PATH"
cargo run --release --example command-population -- --baseline tests/population/m452/command-population
```

`--baseline DIRECTORY` replaces the directory with only comparison inputs (answers, errors,
status, inventory uncertainty): one `<subject>.json` file per subject, where each `/` in the subject
is a directory, and `header.json` with the exact build and the report's totals (the field sweep's
`summary`, the registry sweep's `relationships`). Use a separate directory for each supported
build. An answer keeps its build and basis but not its method or Native version: a method revision
would otherwise change every subject, and `--diff` ignores them. Repeated baseline generation is
byte-identical. PRs that change answers of different subjects change different files, so they
merge with at most a `header.json` conflict. Knowledge pages link to the header totals and do not
restate them; a ticket's section states its own change.

When a merge of `main` conflicts in a baseline, or to bring every baseline up to date at once,
merge `origin/main` and regenerate all three baselines:

```sh
python3 tools/population/regenerate.py
```

It stops before it generates anything when a conflict is outside the three generated baselines.
It stages the baselines and commits nothing; review the changed answers and totals, then commit.
After you resolve a conflict that it reported, run it again to continue the same merge.

To record every command grammar of an installation as recorded answers, then check the record
against a new static run apart from `Basis`:

```sh
cargo run --release --example record-command-grammars -- "$STELLARIS_PATH"
cargo run --release --example record-command-grammars -- --verify .local/sdk-548/recorded-answers "$STELLARIS_PATH"
```

A full recording takes about three minutes; do not skip the check (SDK-640).

For commands, [`declaration-list.rs`](../../examples/declaration-list.rs) prints both full
inventories and their gaps when no name filter is supplied:

```sh
cargo run --release --example declaration-list -- "$STELLARIS_PATH" > declarations.txt
```

For another inventory, run its operation over all entries, including those that return no result
or fail, and keep unresolved entries in the denominator. State whether counts refer to operation
answers, inventory entries or paths.

## Report the registry sweep

[`registry-sweep.rs`](../../examples/registry-sweep.rs) gives the automatic rate of the SDK-553
sweep for each of eight relationships, in total, per registry and per reader identity. It reads a
recorded-answers directory through `Native::from_recorded_answers` and the public API only, so
Native and Atlas get the same number from one recording without a game. `Basis::Recorded` changes
no completeness or gap. One `registry_fields` answer carries seven of the relationships, and a
`Complete` answer can still have `OutsideMethod` gaps, so the report never copies an answer's
completeness to a relationship. It classifies each unit from the public properties and gaps
below.

### Classification rules

A unit is one registry for the registry relationships, and one root field (an item of the
`registry_fields` value) for the field relationships. Nested fields are not units: their facts
count only through their root field's weight rule. Change this when Atlas needs rates for nested
claims.

Each applicable unit is **complete**, **partial** or **failed**. A registry relationship can also
be **unsupported** (`Error::Unsupported`) or **missing** (`Error::NotRecorded`). A missing answer
is not a method failure. `Error::Method`, `Error::UnknownRegistry` and every other error that a
query returns are failed with the error text as the reason. The report refuses `Error::Recorded`:
an unreadable recording is no answer.

A field relationship that depends on the reader is **undetermined** for a field whose
`reader.kind` is `Unknown`: the method did not establish whether the relationship applies. The
rate is `complete / denominator`, and the denominator holds every unit except missing ones, so an
undetermined unit counts against the rate. A registry whose `registry_fields` question has no
answer has no field units; the report counts such registries by status beside the rates.

| Relationship | Unit and public inputs | Applies to | Complete | Failed |
| --- | --- | --- | --- | --- |
| Fields | Registry: the `registry_fields` answer and its gaps | Every listed registry | An answer, and no gap of kind `UnresolvedPath`, `UnnamedField`, `UnresolvedReader` or `UnreadableInput` whose subject is the registry or none: such a gap is a path that can hold a field the answer does not name | No answer |
| Reader kinds | Field: `reader.id`, `reader.kind` | Every field | An identity, and a kind other than `Unknown` | No identity, and kind `Unknown` |
| Shapes and conditions | Field: `shape.value`, `shape.repeat`, each `read` alternative | Every field | No `Unknown` shape part, no `Unresolved` term in any condition, and no `Unresolved` outcome | Both shape parts `Unknown`, and every outcome `Unresolved` |
| References | Field: `reference` | Kind `Reference`, or a `Read` alternative whose reader is `Reference` | At least one lookup; every target a registry or `Triggers`; no `Unresolved` stage, key match, empty key or missing result | `NotEstablished`, or no lookup with a target |
| Numerics | Field: `reader.numeric` | Kind `Integer`, `FixedPoint`, `Float` or `ScopedNumeric` | Conversion facts (`Known(Some)` or `Partial(Some)`; the `Partial` wrapper does not count, as [numeric conversion](numeric-conversion.md#answer-rule) states) | `Unresolved`, or no conversion |
| Weights | Field: `members` | `reader.family` `Weight`, or `WeightBlock` members; undetermined also for a `Block` of family `Unknown` | A `WeightBlock` whose `scalar`, `fixed_keys` and `operations` are `Known`, whose operation repeat is known when it has operations, whose other keys are `Rejected` or `Known` triggers, and whose nested weight keys follow the same rule | No `WeightBlock` |
| Naming rules | Registry: the `derived_names` and `modifier_families` answers | Every listed registry | Both answers complete | Neither question has an answer |
| Scope context | Field: `read_scope`, `entry_contexts`, `accepted_categories` | Every field | Each part established: `read_scope` is `Known`; for family `Trigger`, `Effect` or `Weight`, at least one entry context and no `Unresolved` scope in any; `accepted_categories` is not `Unresolved` | No part established |

Every other applicable unit is partial. For naming rules, a missing answer makes the registry
missing, and two `Unsupported` errors make it unsupported. For scope context, `read_scope`
`Partial` and entry contexts with an `Unresolved` scope are partly established.

The properties do not mark every limit, so a gap that names the field also makes a complete
relationship partial. It never makes one complete or failed. Gaps of kind `OutsideMethod` never
count, and neither do accepted gaps. An `UnresolvedCondition` gap does not count: a loader condition is already a `read`
term, and a use-time selection is not a loader shape.

| Gap kind, subject the field | Relationship left partial |
| --- | --- |
| `UnresolvedReader` | Reader kinds |
| `UnresolvedStorage` | Shapes and conditions |
| `ReaderSemantics` | References |
| `NumericConversion` | Numerics |
| `UnresolvedPath` | Scope context: entry-context and read-scope searches report a missing call path this way |
| Any kind, with a key path that starts at the field | Weights: a nested key's deficit |

Two known limits make the rate lower than the method's own result, never higher. The entry-context
search reports a block that it cannot name as an `UnresolvedPath` gap on the registry, so that
registry's fields are partial. On M452, `common/colony_automation` and `common/map_modes` are
partial for this reason only. Six `UnresolvedPath` gaps on M452 fields are about a reader path
or a triggered modifier, not scope context. Change the rule when a gap kind or subject separates
them.

### Accepted limits

An accepted limit is a gap that the method could close but Atlas does not need. Native keeps
reporting it; only the report counts it apart. `ACCEPTED_LIMITS` in the example lists each limit
once: its gap kind, its exact details (in a gap on an answer item, `{name}` stands for the item's
name), the subjects it covers when the decision names them, and the
ticket and knowledge page of the decision. The first limit that covers a gap accepts it. A gap whose
detail, kind or subject differs stays open, so a limit never hides a new obstacle: for example, a
numeric conversion-path gap is never accepted. Add a limit only with the decision that accepts it,
and remove it when its reopen condition holds.

### The report

The report lists the rates per relationship, then each registry's statuses, each reader identity's
counts (fields without an identity under `none`), each field's statuses, and the failure shapes:
each open gap (not `OutsideMethod` and not accepted), by kind and detail with its digits replaced
by `N`; each accepted gap, by its limit's decision and the same shape; and each error by operation
and reason.

Record the inventory and the three registry questions, then report over the recording:

```sh
cargo run --release --example registry-sweep -- --record .local/population/sweep-answers "$STELLARIS_PATH"
cargo run --release --example registry-sweep -- .local/population/sweep-answers > .local/population/sweep.json
cargo run --release --example registry-sweep -- --diff tests/population/m452/registry-sweep .local/population/sweep.json
```

`--record` writes into a new directory; the report needs no installation. `--diff` takes full
reports or baselines and lists each registry and relationship whose statuses changed. Update the
tracked baseline in the PR that changes the answers:

```sh
cargo run --release --example registry-sweep -- --baseline tests/population/m452/registry-sweep .local/population/sweep-answers
```

The baseline holds one file for each registry: its registry statuses, for each field relationship
its counts by status and its rate, and its open and accepted gaps by shape (`open_gaps`,
`accepted_gaps`). So `--diff` names a registry whose gaps move between open and accepted, or whose
open shapes change, even when no status changes.

## Keep the knowledge in its home

Update the subject's page in `docs/native/`; a new subject gets a new page linked from the
[engine knowledge index](../engine-knowledge.md). Use [registry fields](registry-fields.md),
[engine commands](engine-commands.md) and [modifier families](modifier-families.md) as the models.
For new method material, use this order:

1. An intro naming the operations, source stamps and owning modules.
2. Engine facts: addresses, offsets, slots and gates, tied to the exact build.
3. The result on that build, with complete, partial and failed counts and their population.
4. Gaps and the tickets that own them.
5. Pitfalls: failed shapes and the findings that prevent their recurrence.

Keep run chronology, counts that tracked data holds and method explanations that module comments
hold out of the page. Preserve unique prototype sources until their findings have a retained home
([preservation policy](../development-policy.md#preserve-acquired-knowledge)).

Before delivery, run the default Rust suite and the worker tests as CI does:

```sh
cargo test --workspace --locked
python -m unittest discover -s tools/observation -p 'test_*.py'
python -m unittest discover -s tools/population -p 'test_*.py'
```

The default suite includes locality checks but skips the installed-build parity and live cases.
Run parity explicitly for static method changes; live observation changes also need the relevant
`cargo live` cases. The [README checks](../../README.md#checks) list formatting, lint and
documentation checks.

### Live cases

The live harness (`tests/live.rs`) sets the hidden `GameOptions::keep_work_directory`, so
`close` keeps the work directory. A passing case removes it; a failing case keeps it and prints
`kept <dir>; run summary <dir>/session/run-summary.json: outcome …, last completed phase …, reason …`,
also when a check fails after a clean `close`. The directory also holds `raw-trace.jsonl`,
`owner-events.jsonl`, worker and game output, and the private profile's engine logs.

A live observation has separate checks in the worker and in the reducer. The reducer must reject
an incoherent or misplaced observation even when the normal worker would not send it, so keep
stage-window checks on both sides. The protocol owns the stage vocabulary; the worker does not
decide whether an answer is complete. The worker owns the size bounds of a record (a registry's
entry count, a key's length, the occurrences of a field); a reducer does not repeat them.

## Command inspection and population reports

Use the grammar inspector on an exact supported build:

```sh
cargo run --release --example inspect -- --effect-grammar if
cargo run --release --example inspect -- --trigger-grammar branch_office_value --trace
```

The inspector retains registration instructions, the selected factory and its create method,
then the concrete receiver and its read/member slots. A failed join names its stage and retains
all preceding joins. The grammar lists observed member-to-delegate calls, nested numeric
readers, normalized properties, internal gaps and stops. A missing stop location says
`no instruction`; it is not an inferred instruction. Delegate calls describe observed routes,
not proof that every route or property was resolved. Addresses remain developer diagnostics;
the printed normalized answer is the same address-free answer as `Native::command_grammar`.

Run both unfiltered inventories with one analysis input per family:

```sh
cargo run --release --example command-population > before.json
cargo run --release --example command-population > after.json
cargo run --release --example command-population -- --diff before.json after.json
```

Totals count one operation answer per unique `(family, name)`. A stopped command still gives a
partial public answer, counted as `failed` when every property is unresolved; the stop in its
diagnostics says where it stopped, because the method gives the receiver only with a result and
the report reruns no stage. Unknown registration
observations and input-wide gaps are listed separately and never added to the named denominator;
`full_denominator_known` is false when either remains. The diff compares statuses, normalized
answers and inventory uncertainty, and ignores timings and addresses.

`internals::command_grammar_stops::population` visits each named command in order and lets the
caller release its raw analysis before the next.
