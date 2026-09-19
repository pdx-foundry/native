# SDK-519 Atlas live and replay consumer

Atlas now has a thin caller for Native's shipped installation/session API. The Atlas-owned
working copy is `/Users/jackson/Developer/pdx-atlas/prototypes/native-registry`. At the user’s
request, the caller stays local and is not copied into this PR. It includes a README, a frozen
request contract, and a file-hash manifest for later portability work. Existing Atlas files were
preserved. No Atlas repository was published and no rule snapshot was released.

## Separate changes and frozen contract

Atlas owns the standalone crate, category fixture, observation processing, request contract,
and consumer tests. It uses `Native::open`, static `get_registry`, a dedicated executable role
calling public `supervisor::serve`, `GameOptions`, `start_game().await`, independent item reads,
and `close().await`. It has no platform/version branches or knowledge of Native's method.
Atlas’s `prototypes/native-registry/request-contract.json` fixes both registry names,
request order, default budgets, readiness meaning, evidence handling, and excluded claims.

Native owns the external import/feature/freeze checker, verification controls,
and this report. Native library source, exported API, qualification records, root dependency
lock, and CI are unchanged. Native checks take the local caller path explicitly; Native CI
does not claim to test an unpublished Atlas checkout. The caller pins shipped Native commit
`beadaa4a2bbbf4a3890fbedf7e7b6aa4283b0e13`. Its content revision is
`8031c404a753b3fac1a7c42265453b1fc4fb298c519fa072362c65fc6d9e88f6`, with every file hashed in
Atlas’s `prototypes/native-registry/freeze.json`. This identifies the caller without inventing an
Atlas Git commit. Its separate Cargo lock pins the actual consumer dependency graph.
The capture revision in the verification record differs only in README delivery instructions;
compiled source, fixture, request contract, and lockfile are unchanged.

## What the observations establish

One processor handles live queries, final reports, and offline `Engine::replay_registry` results.
It retains the complete `RegistryResult` rather than rebuilding provenance or inferring identities.
Startup readiness and per-registry availability are retained separately from static descriptions.
Queries do not short-circuit on failure. Returned Games are always closed before output serialization.
Startup failures retain any `GameReport`, including partial results, final replay references,
termination, disposal, reservation status, and diagnostics.

Startup references are copied before reads; final references and observations come from the final
report. These snapshots are never merged. Offline replay does not open an installation or start a
supervisor, and it does not infer readiness from a registry result. A missing/corrupt artifact or a
reference under the wrong registry name fails independently while other results survive.

The category fixture remains the authored `atlas_early_category` input with `tree_template` and
an empty `traditions` list. The shipped registry API enumerates pinned installed collections and
does not accept fixture injection. Therefore this exercise retains the fixture and checks only
whether its key was observed. The fresh normal capture did not observe it. Fixture execution,
stored fields, relationships, validation, runtime behavior, schema completeness, and rule coverage
remain explicit gaps. No config fallback or inferred engine validation fills them.

## Verification

The [machine-readable record](atlas-consumer-verification.json) pins the caller, Native source,
executable, commands, result summaries, and hashes of private verification output.

| Check | Result |
| --- | --- |
| Fresh production normal | One paused Game; 234 traditions and 33 categories; full registry initialization; independent final `Reaped` disposal and resolved reservation. |
| Startup and final replay | Four normal snapshots matched original Native results exactly apart from the required `Live` to `Replay` origin change; startup disposal stayed unconfirmed. |
| One-second startup deadline | Nonzero caller exit with `StartupFailed`/`TimedOut`, partial final observations, two replayable snapshots, `Reaped` disposal, resolved reservation. |
| Fresh per-registry retention failure | Traditions read returned `ObservationUnavailable`; categories remained complete; close confirmed `Reaped` disposal and resolved reservation; final evidence recovered both registries; all three retained snapshots replayed exactly. |
| Historical SDK-521 controls | 32 snapshots across nine normal, missing-hook/partial initialization, access-failure, dropped-record, and worker-loss controls matched Native's retained outputs exactly. Both registry failure directions were covered. |
| Authored tests | Five tests cover untouched provenance/subjects, origin handling, separate snapshot identities/disposal, incomplete/unavailable/worker-loss gaps, readiness/availability retention, wrong registry references, and missing/corrupt descriptors. CLI replay runs with an empty PATH and no installation. |
| Native checks | Workspace tests and Clippy passed in all four feature modes; formatting, replay/admission boundary checks, and both Python suites passed. |
| Atlas boundary | Standalone formatting, Clippy, tests, frozen source identities, allowed public imports, exact Native dependency, and production-only Native feature closure passed. |

The normal and deadline runs left the ordinary Stellaris profile unchanged. The first live attempt
was inadvertently run concurrently with Native's isolation tests; both rejected the external process.
That attempt retained `StartupFailed`, unavailable registry results and confirmed reaping. Native's
production suite and the fresh live controls passed when rerun sequentially. The failed output is
retained separately; it is not relabelled as the successful normal control.

The earlier SDK-483 `typed-extraction` bundle was verified before fixture reuse (1,103 files,
SHA-256 `70fce0ce8dae5dbb473937b08ef77e711218e34a1e2d6406519ad3b574dbfe79`). Historical SDK-521
artifacts were verified by Native's public replay while processing each snapshot. Raw new runs
are in `.local/sdk-519`; SDK-521 controls remain in `.local/sdk-521-session-qualification-3`.
No source or historical evidence was removed.

## Reproduce

```sh
atlas_caller=../pdx-atlas/prototypes/native-registry
python3 tools/check-atlas-consumer.py "$atlas_caller"
cargo test --manifest-path "$atlas_caller/Cargo.toml" --locked
cargo clippy --manifest-path "$atlas_caller/Cargo.toml" --locked --all-targets -- -D warnings
cargo build --manifest-path "$atlas_caller/Cargo.toml" --locked --release
python3 tools/check-atlas-evidence.py "$atlas_caller/target/release/atlas-native-consumer" .local/sdk-521-session-qualification-3
```

The retained-evidence command needs private SDK-521 captures and launches no game. See the caller
README for static, live, and offline commands. Run fresh live controls separately from Native tests.
The Native-owned `tools/check-atlas-live.py BINARY INSTALLATION NEW_OUTPUT` deliberately prevents one
startup snapshot from being retained, then verifies independent reads, close, final evidence recovery,
and replay. It launches a game and must use a new output directory.

Atlas's accepted composition/coverage findings and release investigations remain authoritative.
This ticket supplies a frozen portability input, not Windows live support, engine field extraction,
a production SDK migration, or qualification of any new native operation.
