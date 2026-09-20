# SDK-517 candidate observation controls

The final `batch-05` ran the exact M45-observe installation through the consumer-hosted Rust
supervisor and selected LLDB worker on 2026-09-18. All ten attempts retained replayable evidence,
confirmed independent game reaping, and durably resolved their host reservations. Ordinary-profile
inventories/hashes matched before and after every attempt; an unrelated sentinel remained alive.
Every descriptor was also read through the public `Engine::replay` example.

| Control | Attempt | Replay completion | Controller wall seconds | Disposal |
| --- | --- | --- | ---: | --- |
| normal | `34157-1789764920982471000` | complete | 37.303 | Reaped |
| missing-hook | `34568-1789764958438184000` | unavailable | 8.518 | Reaped |
| late-hook | `34611-1789764967248955000` | unavailable | 8.322 | Reaped |
| dropped-record | `34651-1789764975937581000` | incomplete | 36.356 | Reaped |
| missing-terminal | `35078-1789765012583922000` | incomplete | 36.128 | Reaped |
| access-failure | `35496-1789765048998920000` | unavailable | 36.226 | Reaped |
| worker-loss | `35912-1789765085661198000` | worker-lost | 9.146 | Reaped |
| cancel | `35961-1789765094983298000` | worker-lost | 6.300 | Reaped |
| caller-loss | `35978-1789765101597505000` | worker-lost | 6.312 | Reaped |
| timeout | `35990-1789765108222560000` | worker-lost | 7.343 | Reaped |

Controller wall intervals include preparation, supervisor startup, capture, and public replay;
they are not worker or disposal durations. Worker and owner clocks are not compared.

Normal capture demonstrated activation and retained three registration entries plus the two
category field reads, joined to one source, owner, and thread. Missing/late hooks returned unavailable
without resume. The dropped record and missing terminal prevented completion. Access failure used
an actual failed native memory read at address zero; it did not change host debugger permissions.
Worker loss sent SIGKILL while LLDB was stopped after registration. Cancellation, caller loss, and
timeout retain their distinct owner outcomes; replay separately reports the worker's abnormal exit.

- Linked Native build: `304f091ac83a914ca801773ac5dd0ef2be3102279d1716021b4f5154baaa3d3b`
- Final normal composition: `1e82e54a25ba538d3046e1fce823b942a16078d71694c666176c4f408c057be6`
- Final normal descriptor SHA-256: `00c10a808adba80565584f42ae23b7bd0929eec798e96a8e53936a7af754f154`

No accepted qualification record was added. Public live operations remain unavailable. These
are bounded call/read-entry observations, not complete registries, stored-value validation, or
runtime semantics. The implementation (design page removed 2026-09-20; see Git history) describes the API,
generated protocol, artifact format, bounds, and reproduction command.

## Failed attempts and interventions

All five batches remain retained. Batch 01 completed the observation window and reaped the game,
but macOS rejected signaling the already-exited LLDB group. The owner conservatively left the
reservation unresolved. After a fresh process inventory confirmed that the owner, game, worker,
and worker group were absent, the user explicitly approved clearing only that reservation. Its
unchanged journal and the operator-clearance evidence remain in the bundle. Runtime cleanup now
checks exited/empty groups before signaling and preserves direct-child identity until reaping;
game-free process tests cover natural exit and forced termination.

Batch 02 passed all ten controls. Batch 03 overlapped with the test suite's deliberately named
`stellaris` conflict fixture. The owner correctly reported invalidated isolation, reaped its game,
and resolved the reservation. The unrelated test process was not signaled. Subsequent live batches
ran after the process tests. Batch 04 passed all ten controls; batch 05 repeated them after atomic
resume-acknowledgement publication and the build-source cache exclusion. Each attempt has a new
identity; none of the earlier records was rewritten.

The retained launch uses `-debug_mode`, matching the historical observation launcher. The initial
batch omitted it but still reached the fixture; no stronger causal claim is made about that flag.

## Preservation and checks

Private bundle `sdk-517-observations` contains 2,464 files and 11,296,608 compressed bytes, including
all attempts, per-batch source/executable snapshots, original profile inputs, normalized/raw traces,
worker/tool identities, owner records, ordinary-profile comparisons, and the test log.

- Archive SHA-256: `fd1e2526e04399e14d0f80ea3b0c6a11093430c736515d4912cd0c5e7f2dc560`
- Manifest SHA-256: `0eed566bf57a8251a2954affa3ccf5b33f327805bcd4c538eeac67580b35af52`

Archive and file verification passed. Archive and manifest have a hash-verified second local copy
at the existing preservation location. Private remote preservation remains outstanding under the
existing policy. The tracked [inventory](source-inventory.json) pins both identities.

```sh
python3 tools/evidence.py sdk-517-observations
# Optional restore to a new directory; no game launch:
python3 tools/evidence.py sdk-517-observations --restore
```

Default, test-support, and maintainer-tools suites passed, along with formatting, Clippy, the Windows
maintainer cross-compilation check, both Python suites, replay/admission boundaries, generated-binding
drift, and historical private SDK-483 replay. Unsupported-host compilation is not live qualification.
