# Static analysis and live-test costs

## Hashing dominates static queries

SHA-256 hashing of the executable dominates a static query in both profiles; reading the file is
a small part. The pinned `sha2` 0.10.9 uses its software SHA-256 on ARM64: the hardware path
needs the `asm` feature, which Native does not enable. A public static question costs about one
second per call on M45-release, while the developer population route
(`internals::command_grammar_stops::population`) answers the whole command inventory in about 80
seconds (SDK-651).

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

- Each public static query reads and hashes the executable once, and uses that one verified
  buffer for discovery and field input. The full-file hash covers the ARM64 slice that `open`
  selected and hashed, so a later read needs no second slice hash.
- `BoundAnalysis` computes its catalog once: the named candidates, symbols and strings that
  public questions, bindings and fixture field setup share. It keeps no executable buffer, and
  every read still loads and hashes the executable before it uses the catalog.
- A detected change, a missing file or a path retarget invalidates the `BoundAnalysis`
  permanently, even if the original bytes return. Do not use modification times or file length
  as a substitute for byte integrity.
- The supervisor builds its own `Binding` from the installation and does not accept addresses
  that the caller cached. Live answers, pauses, fixture state and `Complete` witnesses are never
  cached across sessions.

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
