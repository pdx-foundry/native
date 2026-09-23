# SDK-559: static analysis and live-test costs

The SDK-535 declaration reader is a separate static method. On the installed M45 build, one release example run measured 1.39 seconds for effects (including the first catalog read) and 0.75 seconds for triggers on the same `Native`. With SDK-562 composition, one release run on M45-release measured 1.90 s for effects (including the first catalog read) and 1.24 s for triggers. The SDK-559 registry timings below do not apply to it. The SDK-536 language questions on the same `Native`, in one release example run, took 0.46 s for modifiers, 0.45 s for categories, 0.35 s for scopes and 0.38 s for scope links. The SDK-537 localization question took 0.42 s in the same example run. In a later release example run on the M45-release build, the SDK-538 questions took 1.01 s for on_actions and 0.72 s for game rules. The SDK-540 modifier-family question took 1.36 s for the first registry, which includes the first catalog read, and about 0.78 s for each later registry on the same `Native`. Most of that time builds the modifier and category input again for each registry.

Measured on 2026-09-21. The main fix is to optimize `sha2` and `cpp_demangle` in the dev
profile. It reduces a first static registry query from about 60 seconds to 6.1–6.6 seconds
in the isolated experiment. Native's own code remains unoptimized and debuggable. No query,
cache, integrity, completeness, or cleanup rule changes.

Jackson redirected this investigation to the static `registries` example after the live
baseline. The existing live measurements are below; further live profiling and batching
trials were stopped. They do not justify a live optimization claim.

## Machine and method

- Apple M4, 10 CPU cores, 32 GiB memory; macOS 26.6.2 (25G83).
- Rust 1.98.1 (`48a229cea`, LLVM 22.1.8), `aarch64-apple-darwin`.
- Baseline source: `3d3810bf75f5a4beb030a2d1a3e157f6375c5c3b` (SDK-529).
- Installed M45-observe universal executable: 162,737,608 bytes;
  SHA-256 `3d4c8a7046d87175ce7e3b513b1a2ce589050d654d332744518a49d13ac82216`.
- ARM64 slice SHA-256:
  `1e0c9aec45650272fcaecba2eb47f8dce8f17bc08ef2b992be18c99ae098c623`.
- Installation: `/Users/jackson/Library/Application Support/Steam/steamapps/common/Stellaris`.

The runner measures whole subprocess wall time with a monotonic clock. It builds first and
records compilation separately. The explicit `cargo run` measurements below include Cargo;
its logs show no rebuild (0.02–0.37 seconds of Cargo setup). Runs were sequential, with no
other benchmark or owned game running during the static measurements. These are ordinary
filesystem-cache conditions, not forced cold-cache tests. The host was not otherwise isolated.

Temporary timing spans were added to a separate source copy. They write small per-process
JSON logs, without changing Native's normal API or protocol. Timings of parent spans include
children: **do not add the whole-function totals together**. The phase table below uses
non-overlapping costs. The instrumented debug runs were 62.32 and 59.36 seconds; release
runs were 3.13 and 2.87 seconds. All static outputs, including source and gaps, equal the
baseline answer: 164 names, `Partial`, with the existing outside-method gap.

[Measurement results](performance/sdk-559.json) contain the commands, exit codes, machine,
phase totals and repeated-query results. Raw logs and the temporary source copies are retained
at `.local/sdk-559/`. These are investigation outputs, not a new Native result or evidence API.

## The exact command

```sh
cargo run --package pdx-native --example registries \
  "/Users/jackson/Library/Application Support/Steam/steamapps/common/Stellaris/stellaris.app/Contents/MacOS/stellaris"
```

| Baseline workload | First run | Second run |
| --- | ---: | ---: |
| Exact command, dev profile | 117.60 s | 61.46 s |
| Same command, `--release` | 6.45 s | 3.09 s |
| Instrumented example, dev, prebuilt | 62.32 s | 59.36 s |
| Instrumented example, release, prebuilt | 3.13 s | 2.87 s |
| Loader-parity test, dev, prebuilt | 73.70 s | 73.71 s |
| Loader-parity test, release, prebuilt | 3.71 s | 3.51 s |

The 117.60-second first run is real, and was not compilation. Its cause was not established;
it is retained rather than replaced by the faster repeats. The repeat and phase runs establish
the sustained static cost. The ticket's 73-second loader-parity time is reproduced by the dev
profile; it is not a 73-second release analysis.

## Where the static time goes

| Non-overlapping work | Dev runs | Release runs | Dev with two dependency overrides |
| --- | ---: | ---: | ---: |
| SHA-256 hashing | 44.80–46.09 s | 2.13–2.18 s | 2.14–2.27 s |
| Symbol extraction, demangling, stubs and sorting | 12.01–13.38 s | 0.51 s | 1.80–1.81 s |
| Chained fixups | 0.58–0.61 s | 0.03 s | 0.28 s |
| String inventory | 0.03–0.04 s | 0.004 s | 0.035 s |
| Scheduler code and vtable input | 1.34 s | 0.05 s | 1.32–1.33 s |
| Constructor-body reads | 0.11–0.12 s | 0.01 s | below 0.2 s |
| Global initializer reads and resolution | 0.08–0.09 s | 0.007 s | below 0.1 s |

Hashing dominates both profiles. Reading the executable is not the main cost: subtracting
nested hashes from `Installation::open`, `executable_bytes`, and `identify` leaves only a
small fraction of the total. These enclosing spans also include path checks and parsing;
the remainder is an upper bound on file-read time, not a separate disk benchmark.
The default content snapshot costs about 0.01 seconds in dev.

There are nine large hashes before the first answer:

| Call path | Full executable hashes | Slice hashes |
| --- | ---: | ---: |
| `Installation::open` then `Binding::open` → `identify` | 2 | 1 |
| Public `Native::named_candidates` → `BoundAnalysis::executable` | 2 | 1 |
| Cache miss → `BoundAnalysis::named_candidates` → `executable` again | 2 | 1 |
| Total | 6 | 3 |

This hashes about 1.24 GB to answer from a 163 MB file. The timing log has 77 hash calls;
the other 68 are small content files. `Installation::executable_bytes` hashes the file,
and `BoundAnalysis::executable` then calls `identify`, which hashes the same bytes again
plus the slice. The first public query repeats that whole validation before doing discovery.
See [installation](../../src/binding/installation.rs), [binary identity](../../src/binding/binary.rs),
[bound analysis](../../src/binding/analysis.rs), and [public queries](../../src/session/questions.rs).

The pinned `sha2` 0.10.9 dependency selects its software SHA-256 implementation on ARM64
with the current default features. Its ARM64 hardware path is gated by the dependency's
`asm` feature, which this project does not enable. The applied change optimizes the existing
implementation; it does not switch hash algorithms or add a feature. Source inspected:
`sha2-0.10.9/src/sha256.rs` and its Cargo manifest in the local Cargo registry.

Discovery has a second repeated-work boundary. Public registry questions share
`Native.candidates`. `Binding::registry_bindings` and `BoundAnalysis::registry_fields` call
`BoundAnalysis::named_candidates` directly, while `field_input` decodes the binary inventory
again. The parity test deliberately exercises both discovery and bindings, so it discovers
all candidates twice. On release, each inventory rebuild costs roughly 0.6–0.7 seconds before
constructor processing and apart from executable verification.

## The small change applied here

```toml
[profile.dev.package.sha2]
opt-level = 3

[profile.dev.package.cpp_demangle]
opt-level = 3
```

| Prebuilt dev example in isolated copy | First run | Second run |
| --- | ---: | ---: |
| Optimize `sha2` only | 17.96 s | 16.92 s |
| Optimize `sha2` and `cpp_demangle` | 6.64 s | 6.12 s |

The two small overrides remove about 90% of the measured dev query cost. They also apply to
the ordinary test profile inherited from dev. The release profile is unchanged. Debugging
inside these dependencies becomes less direct; Native's own code keeps its normal debug
settings. Rebuilding the dependencies is a one-time cost, measured separately in the results.

A small temporary example opens one `Native`, asks for registries, then asks twice more and
asserts exact answer equality. It shows why merely adding a public answer cache will not
remove the original delay:

| Same-object operation | Original dev | Dev with both overrides |
| --- | ---: | ---: |
| `Native::open` | 15.41 s | 0.74 s |
| First `registries()` | 44.28 s | 5.36 s |
| Second `registries()` | 14.87 s | 0.72 s |
| Third `registries()` | 14.85 s | 0.74 s |

The existing cache works. Its mandatory integrity read still performs three large hashes
on each hit. Separate processes do not share that cache.

## Verification of the applied settings

On the working branch, the user's exact command took 9.01 and 6.54 seconds after the initial
rebuild. The prebuilt runner measured 6.60 and 6.17 seconds. The dev loader-parity test fell
from 73.70–73.71 seconds to 11.38 and 10.05 seconds. All exit codes were zero.
The final instrumented smoke run took 8.25 seconds, so individual runs still vary with host
conditions; the 6-second figure is a repeated observation, not a hard latency guarantee.

Final validation passed: formatting, Clippy with warnings denied, the full default test suite,
all four ignored static parity tests, both ignored bound-analysis tests (including changed and
missing executable invalidation), and all five Python protocol tests. Final example and probe
answers equal the baseline exactly. The profiling driver was exercised end to end. The code-style
review found a copied-source command path error and an instrumentation organization issue;
both were corrected. No PR was created, as requested.

## Ranked follow-ups and safe sharing

1. **Dev dependency optimization — applied.** About 53 seconds saved per first static query;
   two build settings; low risk. Target: prebuilt dev `registries` below 8 seconds on this
   machine, with byte-for-byte equal normalized answers. Verify the command above and the
   full default suite plus ignored static parity and invalidation cases.
2. **[SDK-560: remove duplicate hashes within an operation](https://linear.app/unnamed-system/issue/SDK-560/avoid-repeated-hashing-within-one-static-binary-query) — applied.**
   One verified buffer supplies each static query. The warmed release query fell from
   2.97 to 1.77 seconds, and cached public queries make one full-file integrity hash.
   See the [SDK-560 measurements](performance/sdk-560.md).
3. **[SDK-561: share immutable discovery at the analysis layer](https://linear.app/unnamed-system/issue/SDK-561/reuse-immutable-static-analysis-across-registry-queries-and-bindings) — applied.**
   One candidate computation per `BoundAnalysis`, including public questions,
   bindings, and fixture field setup. Repeated field queries reuse decoded symbols
   and strings. The combined release probe fell from 8.38–8.57 to 4.39–4.48 seconds,
   with no peak or sampled RSS increase. See the [SDK-561 measurements](performance/sdk-561.md).

Keep fresh content-based executable checks at public operation boundaries. Preserve exact
full-file identity, ARM64 slice selection, path-retarget checks, unavailable-file errors, and
permanent invalidation after any detected change, even if the original bytes return. Do not
use modification times or file length as a substitute for byte integrity.

A supervisor must independently bind and check its installation. Its static inputs can be
reused inside that supervisor after validation; caller-supplied addresses are not authority.
Each live session still needs fresh content snapshots, verified private copies, fixture
selection, an owned process and reservation, fresh hook/loader/terminal observations, and
confirmed cleanup. Live answers, pauses, fixture state, and `Complete` witnesses cannot be
cached across sessions. Combining independent public reads in one already-prepared session
is possible; fault controls and different fixtures still require isolated sessions.

## Live baseline retained from the initial scope

| Release workload | First run | Second run |
| --- | ---: | ---: |
| `outside_common` (one case, three registries) | 36.59 s | 34.97 s |
| Full report, batch size 16, 164 registries | 448.38 s | 449.09 s |
| Full suite, 52 cases | 1,832.86 s, passed | 2,012.55 s, one failure |

Both reports returned 161 complete and three unsupported results. The report's own timer
starts after `open` and `registries`, so it reports 445/446 seconds; the table includes those
static costs. Successful cases retain the existing item, isolation, and disposal assertions.
The first suite passed all 52 cases. The second failed `fixture_outcome_unknown_field` after
186 seconds: the worker ended without reaching a valid pause. Disposal was confirmed and the
reservation resolved. A targeted retry passed in 38.54 seconds. The failure data is preserved;
it is not counted as a successful timing sample.

A temporary instrumented live attempt also failed with `WorkerLost` and confirmed disposal;
its timings do not establish normal live-phase costs. Jackson then stopped further live
work. No successful phase split for content copying, debugger setup, loader waits, observation,
and cleanup was obtained, and no batching improvement is claimed. The default report starts
11 sessions; reducing that count may save setup time, but mixed late loaders, trace bounds,
deadlines, and unchanged result coverage must be measured before changing the batch size.
The `late_only` baseline case itself takes about 65 seconds because it exercises a 60-second
observation budget. Sharing that session with ordinary cases would change what it tests.

## Reproduce and verify

From the repository root, set `STELLARIS_PATH` to the exact preserved installation or executable.
The runner builds once, stores command logs and timings in a new output directory, then runs
the prebuilt registry example and loader-parity test twice. It starts no game by default.

```sh
export STELLARIS_PATH="/path/to/Stellaris"
python3 tools/profiling/measure.py .local/perf-dev
python3 tools/profiling/measure.py .local/perf-release --release
```

For phase timing, create a new source copy and run the same driver there. The original sources
are untouched. The instrumentation script fails if an expected source boundary has moved.
Only the copied source contains the timer module. Do not use its timings as uninstrumented
baseline results or publish the copied source as a library change.

```sh
python3 tools/profiling/instrument.py .local/perf-source
cd .local/perf-source
python3 ../../tools/profiling/measure.py ../perf-phases
```

To reproduce the old dev settings without editing the manifest, run the exact example with
`cargo --config 'profile.dev.package.sha2.opt-level=0' --config
'profile.dev.package.cpp_demangle.opt-level=0' run --example registries "$STELLARIS_PATH"`.
Use each override independently to repeat the dependency experiment. Prebuild before measuring;
Cargo settings changes can trigger a rebuild.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
STELLARIS_PATH="$STELLARIS_PATH" cargo test --test static_questions -- --ignored
STELLARIS_PATH="$STELLARIS_PATH" cargo test --lib binding::analysis::tests -- --ignored
python3 tools/observation/test_protocol.py
```

The optional historical live baselines can be reproduced with
`python3 tools/profiling/measure.py .local/perf-live --release --live --suite-repeats 2`.
Run that separately from lifecycle unit tests. It is not needed to measure the static command.
