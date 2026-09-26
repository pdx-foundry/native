# Write a discovery method

Write one method in one task: explore the executable, record the findings, then deliver the
method with authored tests, parity output and a run over its whole population. Do not start a
separate throwaway prototype. The [development policy](../development-policy.md#write-a-method-in-one-task)
sets this workflow; the [method index](discovery.md) locates the operations and their code.

## Inspect the exact build

Match the executable to [targets](targets.md) before reusing an engine finding. Read the subject's
method page for known facts and failed shapes. Set `STELLARIS_PATH` to the installation or
executable being examined.

[The developer inspector](../../examples/inspect.rs) reads ARM64 images without starting a game.
Its image inspection modes also work on uncatalogued builds. Use `--image PATH` to override
`STELLARIS_PATH`:

```sh
cargo run --release --example inspect -- --symbols 'CMegaStructureType'
cargo run --release --example inspect -- --function 'CMegaStructureType::ReadMember'
```

Use `--callers NAME` for direct calls, `--strings TEXT` for literal address references, and
`--slots NAME --count N` for fixed-up pointer slots. The inspector prints image identity and
pointer-resolution status; function ends inferred from symbols and unresolved indirect branches
are limits on what the output establishes. See [inspection limits](../engine-knowledge.md#inspecting-an-executable).

On a catalogued build, inspect a field method's stopped token paths with:

```sh
cargo run --release --example inspect -- --registry-fields common/megastructures
```

This shows each stop's reason, obstacle, instruction, symbol and offset, code entry and last
instructions, followed by the internal gaps before normalization. `--trigger-grammar NAME` and
`--effect-grammar NAME` do the same for one command's child grammar. They also show the
obstruction when the command's receiver join stops. Record new engine facts and failed shapes on
the method page as you find them.

Add `--trace` to learn where a needed value stopped being known:

```sh
cargo run --release --example inspect -- --effect-grammar pop_change_ethic --trace
```

A stop at an unknown register or flags, and the `factory-return` and `command-vtable` receiver
checks, then list each cause with its instruction and code entry:

- a call that returned no known value, or clobbered a caller-saved register or the flags;
- known memory that the method invalidated, such as an object after an unrecognized call;
- a store to an unknown address that may have overwritten known memory;
- a loop head where the joined paths disagreed on the value.

The run follows the value through moves, loads and stores on its own path. Memory that becomes
unknown again keeps its first recorded cause. Memory that was never known gains the first cause
recorded for it and stays marked as partly unrecorded. Limits:

- A trace keeps at most `CAUSE_LIMIT` (4) causes and says when it dropped more.
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

## Run over the whole population

Follow [Measuring method transfer](../development-policy.md#measuring-method-transfer). Run the
method over every discovered registry, or the whole command or other inventory it reads.
For fields, use [`registry-field-sweep.rs`](../../examples/registry-field-sweep.rs):

```sh
cargo run --release --example registry-field-sweep -- "$STELLARIS_PATH" > after.json
cargo run --release --example registry-field-sweep -- --diff before.json after.json
```

Save `before.json` with the same command before the change. The diff compares normalized answers;
the report also groups internal stops by instruction kind, obstacle and function.

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
