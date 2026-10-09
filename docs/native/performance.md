# Static analysis and live-test costs

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
- The integrity check before a game starts (`start_game` and each supervisor check before
  spawn) does not use the stamp: it reads and hashes every byte, because a game that starts
  from a changed executable would use layouts and bindings of another build. It costs about one
  second against a launch of 16–21 s.
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

## Live costs

A live case spends 16–21 s loading the game to the registry pause, 2–5 s in supervisor setup and
about 0.5 s in cleanup. A diagnostic window through validation waits about 74 s for all content.
Batch independent validation samples into one session; a batched session is no slower than a
single one (the batching rule is in [command grammar](command-grammar.md)).

## Measure

`tools/profiling/measure.py OUTPUT [--release] [--live]` builds once and runs the static
workloads twice; `tools/profiling/instrument.py` makes an instrumented source copy for phase
timing (never publish it or use its timings as a baseline). The SDK-559 to SDK-561 raw logs and
source copies are in `.local/sdk-559/`, `.local/sdk-560-*` and `.local/sdk-561-*`; no other copy
was found. The second local copy that the [preservation guide](preservation.md) names,
`~/Documents/PDX/evidence/native-2026-09-18/`, is not on this machine now.
