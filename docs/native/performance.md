# Static analysis, parity and live-test costs

## Hashing dominates the first static query

SHA-256 hashing of the executable dominates the first static query in both profiles; reading the
file is a small part. The pinned `sha2` 0.10.9 uses its software SHA-256 on ARM64: the hardware
path needs the `asm` feature, which Native does not enable. `open` hashes once, and later queries
compare a metadata stamp (below). Before that change each public query hashed again and cost
about 1.2 s on M452-release; `record-command-grammars` over all 2,178 commands took about 43
minutes, and now takes 170 s.

The dev profile therefore optimizes two dependencies; Native's own code stays debuggable:

```toml
[profile.dev.package.sha2]
opt-level = 3

[profile.dev.package.cpp_demangle]
opt-level = 3
```

They cut a first dev `registries` query from about 60 seconds to about 6. To repeat the
experiment, override them with `cargo --config 'profile.dev.package.sha2.opt-level=0'` (and the
same for `cpp_demangle`) and prebuild before measuring.

## The static-query invariant

- `open` reads the executable once, hashes it, and keeps that buffer with the file's metadata
  stamp: length and modification time, and on Unix also device, inode and status-change time.
  The full-file hash covers the ARM64 slice that `open` selected, so no read needs a slice hash.
- Each public static query checks the locator and compares the stamp, then uses the kept buffer
  for discovery and field input. When the stamp differs, it reads and hashes the whole file
  again, and refuses with `TargetChanged` if the hash differs. A file whose stamp changed but
  whose bytes did not is hashed again on each query.
- `BoundAnalysis` computes its catalog once: the named candidates, symbols and strings that
  public questions, bindings and fixture field setup share. Every read checks the stamp before
  it uses the catalog.
- A detected change, a missing file or a path retarget invalidates the `BoundAnalysis`
  permanently, even if the original bytes return.
- The integrity check before a game starts does not use the stamp: it reads and hashes every
  byte, because a game that starts from a changed executable would use layouts and bindings of
  another build. It costs about one second against a launch of 16–21 s. `start_game` runs it
  once. A supervisor session hashes the executable at four points: its `open`, its admission,
  just before the spawn and after cleanup. The worker setup takes the hash bound at `open`
  and does not read the file again; the worker compares the file that it finds with that hash.
- **Accepted limit:** a byte change that leaves the stamp equal is not detected between static
  queries; static answers then come from the bytes verified at `open`. On Unix every write sets the status-change time, but the file system can update it
  late (for example through a writable shared mapping) or at a granularity that hides a second
  change. On Windows, a write that keeps the length and restores the modification time is not
  detected, and neither is a loss of read access. Steam updates replace the file, so they change the inode and the status-change
  time; the 4.5.1 to 4.5.2 update under an open session on 2026-10-06 is that case. The cost
  that this removes is about one second per query (below). Return to a full hash on each query,
  or add one at a different point, if a supported host or file system does not change these
  fields on write (such as a network file system with cached metadata), or if an update path is
  seen that writes the executable in place and keeps its stamp.
- The kept buffer holds the whole executable (about 160 MB on M452) for the life of the
  `Native`.
- The supervisor builds its own `Binding` from the installation and does not accept addresses
  that the caller cached. Live answers, pauses, fixture state and `Complete` witnesses are never
  cached across sessions.

## Block entry contexts

The block entry context method (`BoundAnalysis::block_facts`) runs once per `Native`. On
M452-release it takes about 8.3 s: 4.4 s to read its input and 3.9 s to run, timed around
`block_input` and `analyze_blocks` under `inspect --entry-contexts`. SDK-732 part 1 added about
1.5 s to the input and 0.3 s to the run (from 2.5 s and 3.3 s), and part 2 (helpers, tooltip
calls, offset getters) about 0.4 s and 0.3 s. The binding decodes the functions with a type
pointer that hold a direct evaluator or tooltip builder call or a `blr` (554,000 instructions in
2,561 functions before part 2) and runs the name pass over them to keep those that name a block;
the kept functions bring more callers and receivers (2,404 decoded functions; 1,992 before part 2
and 1,674 before part 1), and the run has 564 entries (534 and 414). Accepted because the result is computed once per `Native`. Narrow the candidates
before the name pass, for example to the calls whose register a load at an evaluation slot's
displacement fills, if the block input grows past about 5 s or a query that runs per registry
needs it.

## Parity costs

Measured on M452 at `13c94ba` (SDK-769), on one Apple Silicon host with 18 cores and 24 GB, release
build, nothing else running. `cargo parity` runs two test binaries one after the other:

| Binary | Wall | CPU | Peak resident | Peak footprint | Slowest test (alone) |
| --- | --- | --- | --- | --- | --- |
| lib, 26 ignored tests | 79 s | 334 s | 8.7 GB | 10.3 GB | `numeric_fixture_storage_population`, 65 s |
| `static_questions`, 40 cases | 172 s | 1,254 s | 9.8 GB | 13.2 GB | `known_target_lists_have_covered_arguments`, 137 s |

- **The slowest test sets the wall time.** Each binary runs its tests on all cores, so it ends with
  its slowest test. With 6 test threads the walls stay at 84 s and 173 s and the peaks fall only
  to 6.3 GB and 8.5 GB resident: a few heavy tests hold most of the memory.
- **Each test opens its own `Native`; no test shares one.** About 64 opens run per parity run.
  Opening costs 0.5 s and about 280 MB (the kept executable and the catalog). What repeats is the
  analysis after it. One `Native` measured step by step:

  | First call on a `Native` | Time | Footprint after |
  | --- | --- | --- |
  | `open` | 0.5 s | 314 MB |
  | `registry_fields`, first registry (the shared fact sets, block facts included) | 15.3 s | 751 MB (falls to about 280 MB after) |
  | `registry_fields`, any later registry | 0.7 s | about 340 MB |
  | `command_grammar`, first effect | 2.9 s | 661 MB |
  | `command_grammar`, first trigger | 2.7 s | 919 MB |
  | `command_grammar`, any later command | 0.02–0.05 s | — |

  The grammar input of each kind keeps about 300 MB, because it copies the symbol list and the
  read-only sections (`binding/binary/grammar.rs`). A test with full facts and both grammar
  kinds holds about 0.9 GB; alone, the heavy tests peak at 1.3–3.2 GB resident. A parity run's
  10–13 GB is several such tests at once.
- **Per-command and per-registry work rebuilds whole-symbol tables.** In
  `known_target_lists_have_covered_arguments`, which walks the 2,178 commands,
  `fields::tokens::names_by_address` rebuilt a map of every symbol for each member exploration:
  half of the test. Built once, the test took 64 s instead of 137 s with the same answers. Of the
  rest, `DispatchInput::command` rebuilding the list of every function for each command is 60%.
  In `numeric_fixture_storage_population`, the field input of each registry scans every symbol
  for persistent constructors (`binding/binary/receivers.rs`, `constructor_class`), and
  `fields::persistent::constructor_points` copies the read-only sections.
- **The same costs reach Atlas.** The Atlas live snapshot takes 432 s with 3.8 GB resident. Its
  three game sessions take about 75 s; the rest is Native static analysis on one thread.
  `names_by_address` alone takes about 67 s, and `derived_names` computes each registry's field
  input again after `registry_fields` did (about 69 s).

## Live costs

A live case spends 16–21 s loading the game to the registry pause, 2–5 s in supervisor setup and
about 0.5 s in cleanup. A diagnostic window through validation waits about 74 s for all content.
Batch independent validation samples into one session; a batched session is no slower than a
single one (the batching rule is in [command grammar](command-grammar.md)).

A session's phase times are in its `session/run-summary.json`. Three cases measured at `13c94ba`
on the host above, each run alone:

| Case | Wall | Sessions | Setup | To the pause | Paused | Cleanup |
| --- | --- | --- | --- | --- | --- | --- |
| `fixture_numeric_conversion_matrix` | 289 s | 10 | 1.3–8.4 s, 27 s in all | 22–26 s, 237 s in all | 0.1 s each | 0.5 s each |
| `fixture_control_triggers` | 108 s | 1 | 1.8 s | 88.2 s (through validation) | 0.1 s | 0.5 s |
| `script_numeric` | 121 s | 1 | 1.2 s | 26.9 s | 82.0 s (163 checks, 0.5 s each) | 0.5 s |

- **A fixture case's time is game launches.** Reading the answers takes about 0.1 s. A slow
  fixture case is slow because it launches the game many times: the conversion matrix 10 times
  and the scoped matrix 7 times, because a request takes one file and at most 32 questions.
  `script_attribution` launches 5 times, and each `fixture_argument_*` case twice, the first
  through validation.
- **A window through validation costs about 65 s more than a file-load window.** 12 cases use one
  (the `fixture_control_*` cases, the first launch of each `fixture_argument_*` case and
  `script_expansion_templates`): about 1,080 of the suite's 3,500 s.
- **The rest of a case runs outside the session.** It is the harness's static analysis (about 15 s
  on the first full-facts query of a `Native`) and admission. The `fixture_outcome_*` cases open
  their own `Native` to record answers, so each pays the 15 s again (41–50 s against 27 s for
  `fixture_normal`).
- **A script check costs about 0.5 s in a paused session.** Where that time goes inside the
  worker is not measured; the worker's `session/worker-diagnostics.json` checkpoints are the
  place to time it.

A suite run at `63946ba` took 3,514 s for 66 cases and about 88 launches: about 1,080 s in 12
windows through validation, about 1,900 s in the other launches, about 140 s in script checks
and the rest in static analysis and fault cases that end early.

## Measure

`tools/profiling/measure.py OUTPUT [--release] [--live]` builds once and runs the static
workloads twice; `tools/profiling/instrument.py` makes an instrumented source copy for phase
timing (never publish it or use its timings as a baseline).

- **Per-test time:** run a built test binary with `--ignored -Z unstable-options --report-time`
  and `RUSTC_BOOTSTRAP=1`; add `--exact NAME --test-threads=1` to time one test alone, and wrap it
  in `/usr/bin/time -l` for its peak memory.
- **Where the time goes:** `sample PID SECONDS -file OUT` on the running test or snapshot process.
  The release profile keeps symbol names. Sum each function's inclusive samples per call path;
  the report's top-of-stack list alone hides callers.
- **Live phases:** the harness removes a passing case's work directory, so copy each
  `$TMPDIR/pdx-native-*/session/run-summary.json` while the case runs; poll every 20 ms or less,
  because a one-session case can write and remove it within 200 ms.

The SDK-559 to SDK-561 raw logs and
source copies are in `.local/sdk-559/`, `.local/sdk-560-*` and `.local/sdk-561-*`; no other copy
was found. The second local copy that the [preservation guide](preservation.md) names,
`~/Documents/PDX/evidence/native-2026-09-18/`, is not on this machine now.
