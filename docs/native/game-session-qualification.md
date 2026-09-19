# SDK-521 Game session qualification

**Status: candidate and ordinary production verification passed under the standing development policy.**

`Native` provides installation-bound registry descriptions without launching a game or probing a
debugger. Async `Game` captures both supported collections in one process and stays paused at a
witnessed initialization boundary. Availability and completeness are independent for each registry.
Neither readiness state claims a loaded world, main-menu readiness, or gameplay operations.

## Fresh candidate controls

All 27 controls passed on 2026-09-19 UTC. The implementation is
`f70dbd47f1d0ac2089d9b8f2d836eadf0478d04c`. Every game was independently reaped, every reservation was resolved,
ordinary profile manifests were unchanged, and unrelated process sentinels survived. Normal results
contained 234 traditions and 33 categories. Consumer reads in both orders, repeated snapshots,
public replay, idempotent close, and rejection of reads after close passed.

“After” and “during” refer to the two registry initialization readiness values. “—” means the harness
did not receive a final consumer report with readiness; it does not infer whether a world was loaded.
Registry columns show final completeness and item count. Every row has `Reaped` disposal.

| Control | Termination | Readiness | Traditions | Categories | Seconds |
| --- | --- | --- | --- | --- | ---: |
| normal-traditions | Completed | after | complete / 234 | complete / 33 | 37.164 |
| reverse-traditions | Completed | after | complete / 234 | complete / 33 | 36.991 |
| cancel-traditions | Cancelled | after | complete / 234 | complete / 33 | 36.632 |
| caller-loss-traditions | CallerLost | — | complete / 234 | complete / 33 | 36.724 |
| timeout-traditions | TimedOut | — | incomplete / 0 | incomplete / 0 | 10.491 |
| idle-timeout-traditions | TimedOut | after | complete / 234 | complete / 33 | 38.357 |
| drop-traditions | CallerLost | — | complete / 234 | complete / 33 | 54.653 |
| startup-drop-traditions | CallerLost | — | incomplete / 0 | incomplete / 0 | 31.610 |
| runtime-shutdown-traditions | CallerLost | — | complete / 234 | complete / 33 | 54.637 |
| read-cancel-traditions | Completed | after | complete / 234 | complete / 33 | 37.119 |
| close-cancel-traditions | Completed | after | complete / 234 | complete / 33 | 37.091 |
| final-retention-failure-traditions | Completed | after | retention failed | complete / 33 | 39.913 |
| snapshot-retention-failure-traditions | Completed | after | complete / 234 | complete / 33 | 41.124 |
| worker-loss-held-traditions | WorkerLost | after | complete / 234 | complete / 33 | 42.536 |
| game-exit-held-traditions | Completed | after | complete / 234 | complete / 33 | 45.516 |
| missing-hook-traditions | Completed | during | unavailable / 0 | complete / 33 | 41.208 |
| missing-hook-tradition_categories | Completed | during | complete / 234 | unavailable / 0 | 43.329 |
| late-hook-traditions | Completed | during | unavailable / 0 | complete / 33 | 43.332 |
| late-hook-tradition_categories | Completed | during | complete / 234 | unavailable / 0 | 44.518 |
| dropped-record-traditions | Completed | after | incomplete / 233 | complete / 33 | 44.196 |
| dropped-record-tradition_categories | Completed | after | complete / 234 | incomplete / 32 | 45.653 |
| missing-terminal-traditions | Completed | after | incomplete / 234 | complete / 33 | 48.347 |
| missing-terminal-tradition_categories | Completed | after | complete / 234 | incomplete / 33 | 45.537 |
| access-failure-traditions | Completed | after | unavailable / 0 | complete / 33 | 44.873 |
| access-failure-tradition_categories | Completed | after | complete / 234 | unavailable / 0 | 45.373 |
| worker-loss-traditions | WorkerLost | — | worker-lost / 1 | worker-lost / 0 | 43.705 |
| worker-loss-tradition_categories | WorkerLost | — | complete / 234 | worker-lost / 1 | 48.853 |

Missing or late hooks leave the other registry available and report partial initialization readiness.
Access failures leave one registry unavailable while loader-return witnesses can establish full
initialization readiness. Missing entries or terminal records affect only the relevant completeness
claim. Worker loss before a safe pause returns a startup failure with established partial results.

Startup and final retention failures were injected independently. A failed traditions artifact did
not remove categories or disposal confirmation. Active snapshots were replayed again after close:
their results were unchanged and disposal remained unconfirmed. Separate final artifacts confirmed
disposal. No returned artifact was overwritten.

## Other verification

Formatting, workspace tests and Clippy passed separately for default, `test-support`,
`maintainer-tools`, and `production`. Windows production cross-Clippy, both Python suites, and
admission/replay boundary checks passed. Historical private replay and both SDK-518 registry replays
matched unchanged. Static queries are tested with a debugger probe that panics if called; unknown
names and irreversible input invalidation are covered. The production consumer refused the new
implementation with `RevisionMismatch` and allocated no attempt before promotion.

## Pinned boundary

| Identity | Value |
| --- | --- |
| Executable | `3d4c8a7046d87175ce7e3b513b1a2ce589050d654d332744518a49d13ac82216` |
| ARM64 slice | `1e0c9aec45650272fcaecba2eb47f8dce8f17bc08ef2b992be18c99ae098c623` |
| Composition | `8925d1ec8995ee82013381bb1da5bcadf33c80814b6f1a9237994dccda63dd76` |
| Shared implementation | `fbb53b020c98eef7dfe13881117cc8eb66809d0ccb764ef1835ccd9a70729b76` |
| Candidate linked build | `42479617ac7778647f25347efaa1b0905e930563576c8fc883ad4bf2e5b28e8c` |
| Compiler | `e794146d99fad54562fdcbd4a7c8b22b0240d2b0312ff6d879d67939b3131850` |
| Debugger | `7c781d24665c41b2e23a404612e317932111a6887f012c5046ce280dc74f5a07` |
| Method | `tradition-registry-session/v1` |

The boundary remains the exact M45-observe ARM64 target and 68 pinned installed files. Each run
retains private content copies, worker identities, raw events, pause challenges, and owner records.
Field and reader discovery, other targets, mods/DLC additions, save loading, resource/UI/gameplay
operations, and Windows live support remain outside this verification.

## Shutdown correction and recovery

An earlier external-kill control left the Mach-stopped target defunct after forced debugger shutdown.
The owner returned `ECHILD`, reported unconfirmed disposal, and correctly left its reservation
blocking. Retrying did not recover ownership; detaching while keeping the target stopped was
unsupported. Harmless fixtures showed that orderly debugger target termination allowed the original
parent to reap the child. The supervisor now gives that path two seconds, then falls back to forced
worker shutdown. Only independent terminal child reaping establishes disposal.

The focused Stellaris game-exit recheck and the complete fresh matrix above passed with the correction.
The original failure remains unconfirmed historical evidence. The user approved removal of only its
blocking reservation, `13027-1789786975079797000`, after process inspection and under the namespace lock.
The clearance did not mark the old attempt disposed or signal another process. Its audit is retained
in `.local/sdk-521-reservation-review/clearance-20260919T034943Z`.

## Development promotion and retained knowledge

The maintainer’s [standing development policy](../development-policy.md) authorizes agents to verify
and promote project changes without another approval. The matching record
`sdk-521-m45-paused-registry-session-v1` cites the immutable tracked
[verification summary](game-session-qualification.json). This is agent verification under that policy,
not a claim of a new human qualification review. All 15 [ordinary production controls](game-session-production.md)
passed after promotion.

The candidate and production consumers use the same owner thread, supervisor, worker, capture, replay,
and cleanup paths. Production adds tracked admission and `Live` result origin; deliberate fault controls
remain maintainer-only. Linked build identities differ with feature selection and the added record;
operation, target, content, compiler, release profile, and debugger identities must match.

Fresh captures are in `.local/sdk-521-session-qualification-3`. Earlier development and failure
knowledge is preserved in `sdk-521-session-development-v1`, listed in the
[inventory](source-inventory.json), with a verified external local copy. No historical prototype
bundle was deleted. Routine development captures remain disposable under the new policy.

The [SDK-518 migration](../design/live-observations.md#migration-from-sdk-518) supplies SDK-519’s consumer
flow. No Atlas repository files changed.
