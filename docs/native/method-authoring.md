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
- The inspector reads ARM64 images only. Its entry, `pdx_native::internals::inspect`, is not a
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

The internal results of registry fields, scopes, scope links, localization, modifiers and
modifier families keep the stop diagnostic of `src/engine/analysis/stop.rs`. Callbacks and
defines keep only the reason word, because they combine reasons across paths. Declaration scopes
are read without a walk, so they have no stop.
`pdx_native::internals::registry_field_stops::run` runs the registry field method once and
returns its internal result with the public answer derived from it.

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
- `command-population --trace INSTALLATION` traces the whole inventory and groups the
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

Keep reviewed expected output in [`tests/expected/m451/`](../../tests/expected/m451/), and check it
in [`tests/static_questions.rs`](../../tests/static_questions.rs). Those ignored tests read the
exact M45 executable and start no game:

```sh
cargo test --release --test static_questions -- --ignored
```

`STELLARIS_PATH` must be set for this command; `cargo parity` is its existing alias. Check source
stamp, values, completeness and gaps, including an unresolved or invalid case. Inspect a changed
answer before changing its expected file. A parity sample does not replace the population run.

Generate a candidate tree with the same questions and selections as the parity tests:

```sh
cargo run --release --example expected -- --out /tmp/native-expected-candidate
cargo run --release --example expected -- --compare tests/expected/m451 /tmp/native-expected-candidate --build BUILD_ID
```

The output directory must be new and outside `tests/expected/`; the command never overwrites a
file, and the comparison is offline. Pass the exact candidate build ID from `Native::build()` or
the [target catalogue](targets.md), without JSON quotes. Exit status is 0 for parity, 1 for parity
failures, 2 for invalid input. `Answer` and `Provenance` differences fail (a permitted build change
is checked against the candidate stamp); `Ordering` permits only moves of complete
dynamic-namespace rows; other files keep byte equality (`Layout`). A clipped terminal report ends with the
path of the full report under `.local/parity/`.

Stored-duration behavior can be compared against an existing fresh live report without repeating
the session:

```sh
cargo run --release --example expected -- --compare-durations tests/expected/duration-m451/live.json .local/durations/live.json --build BUILD_ID
```

The duration mode compares `cases`, including diagnostic and stored count order, after checking the
candidate build.

The generator is not verification. Review each changed answer against the method's tests and
engine evidence. Copy only reviewed files back, then run `cargo parity`, for example:

```sh
cp /tmp/native-expected-candidate/fields-traditions.json tests/expected/m451/fields-traditions.json
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
            if [ "$example" = command-population ]; then
                cargo run --release --example "$example" -- "$STELLARIS_PATH"
            else
                cargo run --release --example "$example"
            fi
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
diagnostics are ignored. The debug-formatted `Err` values of registry duration groups are
unresolved facts, so a changed error diagnostic is not a regression. The tool does not synthesize
serde defaults for older report schemas; use `command-population --diff` for those.

## Run over the whole population

Follow [Measuring method transfer](../development-policy.md#measuring-method-transfer). Run the
method over every discovered registry, or the whole command or other inventory it reads.
For fields, use [`registry-field-sweep.rs`](../../examples/registry-field-sweep.rs):

```sh
mkdir -p .local/population
cargo run --release --example registry-field-sweep -- "$STELLARIS_PATH" > .local/population/fields.json
cargo run --release --example registry-field-sweep -- --diff tests/population/m451-hotfix/registry-field-sweep.json .local/population/fields.json
```

The tracked [M451-hotfix population reports](../../tests/population/m451-hotfix/) are the
baseline. The diff compares normalized answers, ignoring method and Native version stamps but not
build and basis, and reports once which absent members matched their serde defaults.

For command grammars, run and compare the whole population:

```sh
cargo run --release --example command-population -- "$STELLARIS_PATH" > .local/population/commands.json
cargo run --release --example command-population -- --diff tests/population/m451-hotfix/command-population.json .local/population/commands.json
```

Review changed answers, then update the affected baseline in the same PR as the method change:

```sh
cargo run --release --example registry-field-sweep -- --baseline "$STELLARIS_PATH" > tests/population/m451-hotfix/registry-field-sweep.json
cargo run --release --example command-population -- --baseline "$STELLARIS_PATH" > tests/population/m451-hotfix/command-population.json
```

`--baseline` stores only comparison inputs (answers, errors, status, inventory uncertainty), one
subject per line, with the exact build; use a separate directory for each supported build.
Repeated baseline generation is byte-identical.

To record every command grammar of an installation as recorded answers, then check the record
against a new static run apart from `Basis`:

```sh
cargo run --release --example record-command-grammars -- "$STELLARIS_PATH"
cargo run --release --example record-command-grammars -- --verify .local/sdk-548/recorded-answers "$STELLARIS_PATH"
```

Each question reads and hashes the whole executable, so a full recording takes about 21 minutes;
do not skip the check (SDK-640).

For commands, [`declaration-list.rs`](../../examples/declaration-list.rs) prints both full
inventories and their gaps when no name filter is supplied:

```sh
cargo run --release --example declaration-list -- "$STELLARIS_PATH" > declarations.txt
```

For another inventory, run its operation over all entries, including those that return no result
or fail, and keep unresolved entries in the denominator. State whether counts refer to operation
answers, inventory entries or paths.

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
decide whether an answer is complete.

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
cargo run --release --example command-population -- "$STELLARIS_PATH" > before.json
cargo run --release --example command-population -- "$STELLARIS_PATH" > after.json
cargo run --release --example command-population -- --diff before.json after.json
```

Totals count one operation answer per unique `(family, name)`. A failed receiver join still gives a
partial public answer; `receiver_join_failed` counts it separately. Unknown registration
observations and input-wide gaps are listed separately and never added to the named denominator;
`full_denominator_known` is false when either remains. The diff compares statuses, normalized
answers and inventory uncertainty, and ignores timings and addresses.

`internals::command_grammar_stops::population` visits each named command in order and lets the
caller release its raw analysis before the next.
