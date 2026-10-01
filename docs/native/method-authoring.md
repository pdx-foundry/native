# Write a discovery method

Write one method in one task: explore the executable, record the findings, then deliver the
method with authored tests, parity output and a run over its whole population. Do not start a
separate throwaway prototype. The [development policy](../development-policy.md#write-a-method-in-one-task)
sets this workflow; the [method index](discovery.md) locates the operations and their code.

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

The census reads every matching text body in one process, excludes outlined cold clones, and
prints counts, an example and every member of each group. Symbol aliases at the same address
count once, using the first matching name. It uses the reference matcher's canonical
lines, normalizing template arguments and their occurrences in instantiated return/parameter types.
`--normalize-field-offsets` additionally ignores immediate displacements inside memory operands
based on object registers; stack/frame offsets, constants and branches remain exact. This is a
research grouping, not a proof that the functions have the same semantics.

On M45-release the command gives 87 functions in 7 groups, the largest with 77. SDK-543's wider
census covered 188 reference readers (about 20 shapes) and 151 lookup initializers (59 groups);
see [reference shapes](references.md#lookup-shapes) for the population breakdown.

Derive a draft from exact symbol names or addresses selected from the census:

```sh
cargo run --release --example inspect -- --derive-shape 'NAME1' 'NAME2' 'NAME3' > draft.shape
```

The draft keeps strict canonical lines, replacing differing tokens or target names with numbered
placeholders. Equal columns of differences share a placeholder. Bodies of different lengths fail
with their lengths instead of emitting a truncated shape. Equal lengths mean positional alignment
only: review control flow, name the placeholders, and add the semantics before committing a shape.
Three ordinary deferred readers produce `deferred.shape` with one placeholder, up to its name and
header. Identity and alignment diagnostics are shape comments, so stdout can be saved directly.

Use `--symbols TEXT` to find names, `--function NAME` to inspect instructions, `--callers NAME` for
direct calls, `--strings TEXT` for literal address references, `--slots NAME --count N` for fixed-up
pointer slots, and `--lookup-lines NAME` for strict canonical lines. The inspector prints image
identity and pointer-resolution status; symbol-inferred ends and unresolved indirect branches
limit what the output establishes. See [inspection limits](../engine-knowledge.md#inspecting-an-executable).

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

Keep reviewed expected output in [`tests/expected/m45/`](../../tests/expected/m45/), and check it
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
cargo run --release --example expected -- --compare tests/expected/m45 /tmp/native-expected-candidate --build BUILD_ID
```

`STELLARIS_PATH` must be set. The output directory must be new, its parent must exist, and it
must be outside `tests/expected/`, including through symlinks. The command never overwrites a
file. If generation fails, the output can be incomplete; use a new directory for the next run.
The comparison is fully offline: it reads existing files and never opens the installation,
extracts answers or launches a game. Pass the exact candidate build ID from `Native::build()` or
the [target catalogue](targets.md), without JSON quotes. This supplied ID checks candidate stamps;
it does not independently verify an executable. Exit status is 0 for passing parity, 1 for parity
failures and 2 for invalid arguments, unreadable inputs, malformed JSON or malformed comparison
shapes. Missing or extra static files fail parity.

Reports identify the file and JSON pointer, with reviewed and candidate values (`<absent>` differs
from `null`). `Answer` differences fail, including facts, roles, counts, duplicate rows,
completeness and gaps. `Provenance` differences also fail, except for an explicitly permitted
build change after the candidate stamp is checked. `Ordering` notices permit only moves of complete
dynamic-namespace rows; duplicates and order inside each row remain checked. Other static files
retain byte equality, so `Layout` differences fail even when parsed values match. `Historical`
notices explicitly skip SDK-533 evidence on a different exact build. A skip establishes nothing
about that build. Missing or invalid current stamps fail even if the two files are identical.

Terminal output is limited to 80 lines of 240 characters. If any output is clipped or omitted,
the final line gives the absolute path of a fresh complete report under `.local/parity/`. Open
that file to inspect every difference and full value. Report-writing failures remain errors.
The command never accepts candidates or overwrites inputs or tracked expectations. Parity tests
use the same rules and bounded failure reports.

Stored-duration behavior can be compared against an existing fresh live report without repeating
the session:

```sh
cargo run --release --example expected -- --compare-durations tests/expected/duration-m45/live.json .local/durations/live.json --build BUILD_ID
```

The duration mode checks the candidate build and compares `cases`, including diagnostic and stored
count order. A permitted build difference describes fresh behavior against retained cases; the
historical live observations do not apply to the current exact build. The live test also checks
each observation's current-build and live-basis stamps before selecting its duration cases.

The generator is not verification. Review each changed answer against the method's tests and
engine evidence. Copy only reviewed files back, then run `cargo parity`, for example:

```sh
cp /tmp/native-expected-candidate/fields-traditions.json tests/expected/m45/fields-traditions.json
cargo parity
```

The tracked files supply sample keys, not replacement answers. A missing selected item appears
as `null` in the candidate. Full inventories include new items. The historical live observation
`field-storage-sdk533.json` is copied unchanged; its independent storage checks apply only to the recorded exact build,
with an explicit skip on other builds. Generating static answers does not repeat that live
experiment. Descriptive mechanism labels in declaration samples are also retained for review.
Tests and the generator share [`tests/parity/`](../../tests/parity/mod.rs); layout tests check
all tracked files without a game, and the installed-build test also changes one recorded answer
to check that only the corresponding file and entry change.

## Run over the whole population

Follow [Measuring method transfer](../development-policy.md#measuring-method-transfer). Run the
method over every discovered registry, or the whole command or other inventory it reads.
For fields, use [`registry-field-sweep.rs`](../../examples/registry-field-sweep.rs):

```sh
mkdir -p .local/population
cargo run --release --example registry-field-sweep -- "$STELLARIS_PATH" > .local/population/fields.json
cargo run --release --example registry-field-sweep -- --diff tests/population/m45-release/registry-field-sweep.json .local/population/fields.json
```

The tracked [M45-release population reports](../../tests/population/m45-release/) are the baseline;
no second checkout or build of `main` is needed. The diff compares normalized answers and reports
once which absent members matched their serde defaults. Non-default additions, required removals
and unknown JSON members remain changes. Answers that the current types cannot deserialize receive
no default-member pruning. Method and Native version stamps are excluded from comparison; build and basis
remain compared. The report also groups stops by instruction kind, obstacle and function.

For command grammars, run and compare the whole population:

```sh
cargo run --release --example command-population -- "$STELLARIS_PATH" > .local/population/commands.json
cargo run --release --example command-population -- --diff tests/population/m45-release/command-population.json .local/population/commands.json
```

Review changed answers, then update the affected baseline in the same PR as the method change:

```sh
cargo run --release --example registry-field-sweep -- --baseline "$STELLARIS_PATH" > tests/population/m45-release/registry-field-sweep.json
cargo run --release --example command-population -- --baseline "$STELLARIS_PATH" > tests/population/m45-release/command-population.json
```

`--baseline` stores only comparison inputs, with one subject per line: answers, errors and status,
plus command inventory uncertainty. It omits diagnostic groups, counters and timings. Full reports
are generated into the ignored `.local/population/` directory. The baseline includes the exact build;
use a separate directory for each supported build. An unchanged method gives zero changed answers,
and repeated baseline generation is byte-identical.

To record every command grammar of an installation as recorded answers, then check the record
against a new static run apart from `Basis`:

```sh
cargo run --release --example record-command-grammars -- "$STELLARIS_PATH"
cargo run --release --example record-command-grammars -- --verify .local/sdk-548/recorded-answers "$STELLARIS_PATH"
```

Each question reads and hashes the whole executable once. On M45 that integrity check costs
about 0.4 s of the 0.58 s per grammar, so a full recording takes about 21 minutes. Do not skip
the check to save time.

For commands, [`declaration-list.rs`](../../examples/declaration-list.rs) prints both full
inventories and their gaps when no name filter is supplied:

```sh
cargo run --release --example declaration-list -- "$STELLARIS_PATH" > declarations.txt
```

For another inventory, run its operation over all entries, including those that return no result
or fail. Record the exact build, operation, population and reproducible invocation on its method
page, with complete, partial and failed counts, each failure shape and each distinct finding.
State whether counts refer to operation answers, inventory entries or paths. Keep unresolved
entries in the denominator. Fix shared mechanisms that the run exposes and rerun the affected
method. The separate SDK-553 registry sweep is a gate at a recorded commit; its failures become
follow-up tickets rather than repairs inside that sweep.

## Keep the knowledge in its home

Update the subject's page in `docs/native/`; a new subject gets a new page linked from the
[method index](discovery.md). Use [registry fields](registry-fields.md),
[engine commands](engine-commands.md) and [modifier families](modifier-families.md) as the models.
For new method material, use this order:

1. An intro naming the operations, source stamps and owning modules.
2. Engine facts: addresses, offsets, slots and gates, tied to the exact build.
3. The result on that build, with complete, partial and failed counts and their population.
4. Gaps and the tickets that own them.
5. Pitfalls: failed shapes and the findings that prevent their recurrence.

Keep run chronology out of the page, and keep method explanations in module comments rather
than copying them here. Preserve unique prototype sources and observations until their findings
have a usable retained home, as the [preservation policy](../development-policy.md#preserve-acquired-knowledge)
requires. Native owns platform and build methods; Atlas owns extraction fixtures, rule conclusions
and coverage.

Before delivery, run the default Rust suite and the worker tests as CI does:

```sh
cargo test --workspace --locked
python -m unittest discover -s tools/observation -p 'test_*.py'
```

The default suite includes locality checks but skips the installed-build parity and live cases.
Run parity explicitly for static method changes; live observation changes also need the relevant
`cargo live` cases. The [README checks](../../README.md#checks) list formatting, lint and
documentation checks.

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

Totals count one operation answer per unique `(family, name)`, including runtime-composed names
and named unreadable registrations. They do not count registration observations or token paths.
`complete`, `partial` and `failed` describe operation outcomes. A failed receiver join still
produces a partial public answer; `receiver_join_failed` counts these separately without dropping
them from `named_commands`. This count uses the retained receiver and reader-slot/body join,
not the normalized reader identity: a later grammar symbol failure is not a failed receiver join. An input-construction failure aborts the report rather than inventing
an inventory denominator. Unknown registration observations and input-wide inventory gaps are
listed separately. An unknown observation can stand for several commands or overlap another
observation; its count is never added to the named denominator. `full_denominator_known` is false
when either uncertainty remains.

Each case keeps the full normalized answer and its source stamp, plus developer chain and stop
diagnostics. Public gap groups and internal stop groups list each affected command once per
shape; one command can occur in several groups. Internal shapes use reason, instruction kind,
obstacle and function. The diff compares statuses, normalized answers (including gaps and source),
and inventory uncertainty. It ignores timings and internal addresses. An unchanged report has
`changed: 0`; changed subjects show both old and new values.

The hidden `internals::command_grammar_stops::population` wrapper visits each named command in
order and lets the caller release its raw analysis before visiting the next command. It shares
lookup, receiver analysis and normalization with the existing methods. These tools do not extend
argument grammar extraction, establish parser acceptance or measure Atlas rule coverage.
