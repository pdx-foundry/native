# SDK-521 ordinary production verification

All 15 ordinary production controls passed on 2026-09-19 UTC after promoting the
agent-verified `sdk-521-m45-paused-registry-session-v1` record under the
[development policy](../development-policy.md). The [candidate report](game-session-qualification.md)
establishes the 27 full and partial controls for the same implementation.

Every production attempt confirmed game disposal and resolved its reservation. Ordinary profile
manifests remained unchanged, unrelated sentinels survived, and public replay matched retained results.
Normal captures returned 234 traditions and 33 categories from one paused process. Both query orders,
repeated reads, idempotent close, and rejection of reads after close passed. Active snapshots were
replayed unchanged after close and still reported disposal as unconfirmed; final evidence separately
confirmed disposal.

| Control | Termination | Disposal | Seconds |
| --- | --- | --- | ---: |
| normal-traditions | Completed | Reaped | 42.489 |
| reverse-traditions | Completed | Reaped | 45.066 |
| cancel-traditions | Cancelled | Reaped | 48.941 |
| caller-loss-traditions | CallerLost | Reaped | 50.924 |
| timeout-traditions | TimedOut | Reaped | 11.845 |
| idle-timeout-traditions | TimedOut | Reaped | 49.164 |
| drop-traditions | CallerLost | Reaped | 66.609 |
| startup-drop-traditions | CallerLost | Reaped | 31.653 |
| runtime-shutdown-traditions | CallerLost | Reaped | 61.860 |
| read-cancel-traditions | Completed | Reaped | 40.408 |
| close-cancel-traditions | Completed | Reaped | 46.925 |
| final-retention-failure-traditions | Completed | Reaped | 53.324 |
| snapshot-retention-failure-traditions | Completed | Reaped | 53.066 |
| worker-loss-held-traditions | WorkerLost | Reaped | 51.712 |
| game-exit-held-traditions | Completed | Reaped | 52.557 |

Retention failures were externally induced and remained scoped to their affected registry/artifact.
Worker loss preserved established snapshots. The game-exit control verified the corrected orderly
debugger shutdown followed by independent parent reaping. Cancellation, caller/runtime loss, startup
and idle deadlines, and interrupted read/close futures used the ordinary consumer interface. No
maintainer fault-control surface or serialized admission bypass was enabled.

| Identity | Value |
| --- | --- |
| Composition | `8925d1ec8995ee82013381bb1da5bcadf33c80814b6f1a9237994dccda63dd76` |
| Shared implementation | `fbb53b020c98eef7dfe13881117cc8eb66809d0ccb764ef1835ccd9a70729b76` |
| Production linked build | `1eda448d6668dd5542077e447f7c5dd5a52db0015939e44e88ba0ccda9819a1b` |
| Candidate linked build | `42479617ac7778647f25347efaa1b0905e930563576c8fc883ad4bf2e5b28e8c` |

Target, ARM64 slice, method, compiler, release profile, debugger, content, and operation identities
match the candidate qualification. The linked build differs because production features and the
qualification record are included. The supported boundary remains the exact target and 68 installed
files, with paused initial-loader registry snapshots only. No world, gameplay, save, resource, UI,
field-discovery, or Windows live capability follows from these controls.

Formatting, workspace tests and Clippy passed separately for all four feature modes after promotion,
with Windows production cross-Clippy, both Python suites, and both admission/replay boundary checks.
Historical private replay passed again. Fresh captures and machine-readable results are retained in
`.local/sdk-521-session-production`; the harness is `tools/check-game-sessions.py --production`.
