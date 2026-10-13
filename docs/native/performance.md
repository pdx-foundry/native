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

## Registry field input

- **The registry-independent part is read once.** The catalog builds its symbol index (the name
  order of the symbols and every constructor entry, about 4 MB on M452) on the first lookup, and
  the shared field input (key readers, constructor calls, writable pointer slots, functions that
  never return, modifier containers) on the first field input. Each registry's field input then
  finds its constructors, vtable groups and names through the index. The catalog's symbols stay
  in address order (`binary/inventory.rs` sorts them); `Names` and the address lookups rely on
  that order.
- **Each registry's field result is kept.** `BoundAnalysis` keeps the field method's result of a
  registry after its first run, and `registry_fields` and `derived_names` read it. Every read
  checks the executable first; a failed run is not kept. `fixture_registry_fields` and the
  duration groups run the method on their own input and keep nothing. With all 164 M452
  registries kept, the footprint after a `registry_fields` loop is 20–50 MB higher (415 and
  447 MB in two runs, against 395 MB; index and shared input included). Bound the kept results
  if a host must hold a `Native` in less memory than that.
- **Accepted limit:** the map lock is not held while a registry is analyzed, so callers that miss
  the same registry at the same time each run the field method, and the first result is kept.
  The answers are the same; only work repeats. The three all-registry parity tests walk the
  registries in the same order on parallel threads, so they can repeat each other's work. Give
  each registry a once-cell (keeping the rule that a failed run is not kept) if this becomes the
  slowest part of a parity run or a multi-threaded consumer appears; Atlas asks on one thread.
- **Accepted limit:** each field input still reads its text function starts (`Text::read`) and
  its constant data, and copies the symbols and strings into the `FieldInput` that the engine
  owns: about 7 ms of a 270 ms `registry_fields` call. Share them if they grow past about 10% of
  the call.

## Parity costs

Measured on M452 at `5edf7e4` (SDK-774 on top of SDK-771 and SDK-770; `pull/212/head`), on one
Apple Silicon host with 18 cores and 24 GB, release build, nothing else running. `cargo parity` runs two test binaries one after the
other:

| Binary | Wall | CPU | Peak resident | Peak footprint | Slowest tests (in the run) |
| --- | --- | --- | --- | --- | --- |
| lib, 26 ignored tests | 51 s | 145 s | 5.3 GB | 3.4 GB | `numeric_fixture_storage_population` 51 s; the rest 20 s or less |
| `static_questions`, 40 cases | 89 s | 422 s | 6.9 GB | 4.0 GB | `traced_questions_match_untraced_questions` 79 s, `every_tracked_candidate_matches_the_reviewed_tree` 72 s, the three all-registry block tests 49–59 s |

At `13c94ba`, before those three changes, the walls were 79 s and 172 s, the CPU 334 s and
1,254 s, and the peak footprints 10.3 GB and 13.2 GB.

- **The slowest test sets the wall time.** Each binary runs its tests on all cores, so it ends with
  its slowest test. Fewer test threads do not lower the wall time.
- **Each binary shares one `Native`.** The lib tests take `installed_native()` (`src/session.rs`)
  and `static_questions` takes `native()`, so the shared fact sets and grammar inputs are
  computed once per binary; kept registry results can be computed more than once (see
  [registry field input](#registry-field-input)). Seven tests open their own, because they need a
  fresh or different instance: in `static_questions` the two recorders (`record_answers_to` takes
  the `Native`) and the traced half of each traced pair (`own_native()`); in the lib
  `cached_static_answers_refuse_changed_or_missing_executables` (it changes a copy of the
  executable), `repeated_public_and_binding_queries_agree` and
  `m452_checks_carry_the_named_receivers_and_their_count_slots` (both need empty caches). They
  build different amounts: `every_tracked_candidate_matches_the_reviewed_tree` computes every
  tracked file, with the full facts and both grammar kinds, while
  `recorded_answers_equal_the_real_answers_apart_from_the_basis` builds only the effect grammar
  and `repeated_public_and_binding_queries_agree` and
  `cached_static_answers_refuse_changed_or_missing_executables` build no command grammar. The
  peak is not attributed per test.
  One `Native` measured step by step at `13c94ba` (the later-registry row with SDK-774, one
  thread):

  | First call on a `Native` | Time | Footprint after |
  | --- | --- | --- |
  | `open` | 0.5 s | 314 MB |
  | `registries` (builds the catalog) | 0.5 s | 270 MB |
  | `registry_fields`, first registry (the shared fact sets, block facts included) | 15 s | 751 MB (falls to about 280 MB after) |
  | `registry_fields`, any later registry | 0.27 s | 415–447 MB after all 164 |
  | `command_grammar`, first effect | 2.9 s | 661 MB |
  | `command_grammar`, first trigger | 2.7 s | 919 MB |
  | `command_grammar`, any later command | 0.02–0.05 s | — |

  The grammar input of each kind keeps about 300 MB, because it copies the symbol list and the
  read-only sections (`binding/binary/grammar.rs`). Borrow them from the catalog if a `Native`
  must hold less memory.
- **Whole-symbol tables are built once per input.** `SymbolIndex`
  (`engine/analysis/discovery/symbol_index.rs`) sorts the symbol positions by address and by name
  hash; the grammar input builds it on first use and every command shares it, and each field,
  modifier-block and triggered-modifier analysis builds one. `DispatchInput` borrows the function
  bodies instead of copying a view of every function. Before this, a map of every symbol was
  rebuilt for each member exploration and a list of every function for each command:
  `known_target_lists_have_covered_arguments`, which walks the 2,178 commands, took 137 s alone
  and now takes 19 s. Do not add a per-command or per-member path that scans or rebuilds a
  whole-symbol table. The per-registry part is in [registry field input](#registry-field-input).
- **The same costs reach Atlas.** At `13c94ba` the Atlas live snapshot took 432 s with 3.8 GB
  resident. Its three game sessions took about 75 s; the rest was Native static analysis on one
  thread, of which the per-member symbol map took about 67 s and `derived_names` repeating each
  registry's field method about 69 s. Both are gone. At `ee01bf3` (185 registries) the Atlas live
  snapshot of the Milestone 5 sweep (SDK-798) took 218 s wall with 4.05 GB peak resident, of which
  `modifier_families` is about 30 s ([below](#modifier-families)).

## Whole-inventory recording

Measured on M452 at `3d76ead` on the host above, release build, nothing else running.
`registry-sweep --record` asks `registries`, then `registry_fields`, `derived_names` and
`modifier_families` for each of the 164 registries on one `Native`, and records every answer:
**118 s wall, 109 s CPU, 1.07 GB peak resident**, 25 MB of answers. The sweep report over that
recording takes under 0.1 s. `record-command-grammars` records all 2,178 commands in 170 s (see
[hashing](#hashing-dominates-the-first-static-query)). Both take minutes, so the cost of
recording a whole inventory blocks no sweep (decision D6 of the Milestone 5 plan; SDK-640,
SDK-651).

## Modifier families

Measured on M452 at `ee01bf3` on the host above, release build, one thread, nothing else running:
a program opens one `Native`, asks `registries`, and asks `modifier_families` for each of the 185
registries twice (SDK-591, over the list that SDK-783 and SDK-786 grew from 164).

| Order | First call | Later calls (median, max) | 185 calls |
| --- | --- | --- | --- |
| First question after `registries` | 2.5 s | 0.18 s, 0.21 s | 33.9 s; again 31.5 s |
| After a `registry_fields` and a `derived_names` loop, as Atlas asks | 0.17 s | 0.17 s, 0.21 s | 30.3 s; again 30.1 s |

Peak resident was 0.48 GB and 1.42 GB. The family index is kept, but each call builds the
modifier input again (`BoundAnalysis::modifier_input`, the declared modifiers and categories), only
to name the categories of the families' masks: a 15 s `sample` during the first loop puts 10,617 of
10,618 samples of `modifier_families` there. The rest of a call costs under 1 ms.

**Accepted:** about 30 s per full inventory, against the 218 s Atlas live snapshot and the
minutes of a whole-inventory recording ([above](#whole-inventory-recording)); it blocks no sweep,
by the rule of decision D6. Keep the modifier input in `BoundAnalysis`, as the family index is,
if a consumer asks `modifier_families` more than once per inventory, or if the per-call cost grows
past about 0.5 s. A batched `modifier_families_all` is not needed: the cost is the repeated input,
not the per-registry work.

## Live costs

A live case spends 16–21 s loading the game to the registry pause, 2–5 s in supervisor setup and
about 0.5 s in cleanup. A diagnostic window through validation reaches its pause about 65 s
later; nearly all of that is the log hooks (below).
Batch independent validation samples into one session; a batched session is no slower than a
single one (the batching rule is in [command grammar](command-grammar.md)).

A session's phase times are in its `session/run-summary.json`. Three cases measured at `13c94ba`
on the host above, each run alone:

| Case | Wall | Sessions | Setup | To the pause | Paused | Cleanup |
| --- | --- | --- | --- | --- | --- | --- |
| `fixture_numeric_conversion_matrix` | 289 s | 10 | 1.3–8.4 s, 27 s in all | 22–26 s, 237 s in all | 0.1 s each | 0.5 s each |
| `fixture_control_triggers` | 108 s | 1 | 1.8 s | 88.2 s (through validation) | 0.1 s | 0.5 s |
| `script_numeric` | 121 s | 1 | 1.2 s | 26.9 s | 82.0 s (163 checks, 0.5 s each) | 0.5 s |

- **Most of a launch is engine startup.** In `fixture_normal` the game reached the first
  `Initializing Database` line in `setup.log` 16.2 s after the spawn, then loaded all 180
  databases in 4.3 s: `common/traditions` (234 items) in 25 ms, and the categories and the
  fixture file in 93 ms. The session paused 20.7 s after the spawn. 10.4 s of the startup has no
  log output and is not identified.
- **A fixture case's time is game launches.** Reading the answers takes about 0.1 s. A slow
  fixture case is slow because it launches the game many times. The conversion matrix gives
  each of its 10 table rows its own fixture. The scoped matrix launches 7 times: a request takes
  one file, so each registry has its own launches; ordinary cases go in batches of 24; and each of
  3 structural cases has a launch of its own, because a block given to a scalar reader can consume
  the definitions after it.
  `script_attribution` launches 5 times, and each `fixture_argument_*` case twice, the first
  through validation.
- **A window through validation costs about 60 s more, because of its log hooks.** The fixture's
  private mod replaces `common/traditions` and `common/tradition_categories` (`replace_path`),
  so vanilla content fails about 4,500 deferred references to the missing traditions, and the
  engine logs 4,895 lines. With diagnostics requested, the worker stops the game at each log
  call. In `fixture_control_triggers` the session took 85.5 s and 86.3 s to its pause with the
  four log hooks armed, and 24.9 s and 25.1 s without them. Both reached the same
  validation-complete pause and wrote the same 4,895 lines: about 12.5 ms per message. 12 cases
  use such a window (the `fixture_control_*` cases, the first launch of each `fixture_argument_*`
  case and `script_expansion_templates`): about 1,080 of the suite's 3,500 s.
- **The rest of a case runs outside the session.** It is the harness's static analysis (about 15 s
  on the first full-facts query of a `Native`) and admission. The `fixture_outcome_*` cases open
  their own `Native` to record answers, so each pays the 15 s again (41–50 s against 27 s for
  `fixture_normal`).
- **A script check costs about 0.5 s in a paused session.** Where that time goes inside the
  worker is not measured; the worker's `session/worker-diagnostics.json` checkpoints are the
  place to time it.

A suite run on the code of `13c94ba` took 3,514 s for 66 cases and about 88 launches: about 1,080 s in 12
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
