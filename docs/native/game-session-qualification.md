# SDK-521 Game session qualification status

**Status: recovery verified; full fresh game qualification is running.**

`Native` now provides static installation descriptions without a supervisor or debugger. Async `Game`
sessions capture both registry collections in one launch, preserve each result independently, and
explicitly report a pause during or after registry initialization. Neither state claims a loaded
world or gameplay readiness. The old live client/job API is removed; historical replay remains.
Save loading, explicit-empire resource reads, and UI queries remain future designs.

## Verification completed

The current implementation is commit `f70dbd47f1d0ac2089d9b8f2d836eadf0478d04c`.
Formatting, workspace tests and Clippy passed separately for default, `test-support`,
`maintainer-tools`, and `production`. Windows production cross-Clippy, both Python suites, and
admission/replay boundary checks passed. Historical private replay and both SDK-518 registry replays
matched unchanged. The rebuilt production consumer refuses with `RevisionMismatch` and allocates
no attempt. No acceptance record has been added.

The preceding candidate build passed 14 fresh controls before the game-exit control failed.
Every passed attempt confirmed disposal, resolved its reservation, preserved ordinary profile
manifests, and left unrelated process sentinels alive. Available snapshots were read in both orders
and repeatedly compared with public replay. Normal results contained 234 traditions and 33 categories.

| Control | Termination | Seconds |
| --- | --- | ---: |
| normal-traditions | Completed | 37.488 |
| reverse-traditions | Completed | 40.474 |
| cancel-traditions | Cancelled | 40.929 |
| caller-loss-traditions | CallerLost | 41.717 |
| timeout-traditions | TimedOut | 8.341 |
| idle-timeout-traditions | TimedOut | 45.660 |
| drop-traditions | CallerLost | 62.810 |
| startup-drop-traditions | CallerLost | 31.621 |
| runtime-shutdown-traditions | CallerLost | 55.721 |
| read-cancel-traditions | Completed | 37.767 |
| close-cancel-traditions | Completed | 37.811 |
| final-retention-failure-traditions | Completed | 44.723 |
| snapshot-retention-failure-traditions | Completed | 50.008 |
| worker-loss-held-traditions | WorkerLost | 46.117 |

The 12 per-registry hook, record, terminal, access, and worker-loss controls had not yet run.
The complete 27-control matrix must run again on the corrected implementation. These earlier
passes do not qualify the changed shutdown code.

## Failed control and correction

In `game-exit-held-traditions`, the harness sent SIGKILL to the paused game. Forced debugger shutdown
left the target defunct, and the supervisor’s `waitpid` returned `ECHILD`. The report correctly retained
`Unconfirmed` disposal and left the reservation unresolved. Both completed registry snapshots survived.
The failure report has not been rewritten.

Harmless process fixtures reproduced the failure. Retrying `ECHILD` for 30 seconds did not recover
ownership, and `SBProcess.Detach(true)` was unsupported. An orderly `SBProcess.Kill()` let debugger
exit handling finish; the original parent then reaped its child with status 9. Normal closure and
three external-kill timings passed with that sequence. Apple’s [kernel exit implementation](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/kern/kern_exit.c)
describes returning a traced child to its original parent during reaping; the fixture results are
bounded evidence for this host and debugger.

The supervisor now gives a session worker two seconds for orderly target termination before the
existing forced shutdown fallback. Independent parent reaping remains required. Nonterminal wait
statuses cannot establish disposal. Tests cover cooperative release and an unresponsive worker.
The fresh `game-exit-held` Stellaris control passed in 40.168 seconds: disposal was `Reaped`, the
reservation was resolved, and both snapshots remained complete. The full matrix is being rerun.

## Development recovery

The cleared journal was:
`/Library/Application Support/PDX Native/instances/13027-1789786975079797000.json`.

Its original SHA-256 is `1a5eaf4897b543d9edadda7d89625f737482e5ac1603f733c4a3b0a6d14e7f0d`.
The retained process inventory shows game PID 13037 defunct under PID 1. The supervisor and worker
are absent, and the namespace lock is available. These facts do **not** confirm disposal. Failed
harmless-fixture experiments also left defunct PIDs 13547 and 13596 under PID 1; they hold no Native
reservation. No action against PID 1 or a system restart has been attempted.

The user approved clearance in the SDK-521 task. On 2026-09-19 UTC the agent verified the unchanged
journal and current process state, retained its original bytes, and removed only that journal under
the namespace lock. No other reservation or process changed. The original attempt still has
unconfirmed disposal. The audit is in `.local/sdk-521-reservation-review/clearance-20260919T034943Z`.

The user also replaced the earlier per-action approval rule with a
[standing development policy](../development-policy.md): agents can recover disposable development
state and promote verified changes while preserving unique prototype knowledge. Runtime disposal
and admission checks remain enforced. Complete the fresh matrix, promote its matching record,
run ordinary production controls, then publish the ready PR with `Closes SDK-521`.

## Retained evidence

Bundle `sdk-521-session-development-v1` contains 14,619 files (25,569,240 compressed bytes).
It preserves the failed and superseded candidate runs, earlier development attempts, corrected source
and built consumers, full check logs, debugger fixtures, the unchanged reservation journal, and the
concrete operator-review record. Archive/member verification passed, with a hash-verified second
local copy at `/Users/jackson/Documents/PDX/evidence/native-2026-09-18`.

- Archive SHA-256: `daff56c710c45c4e086310120ef006192a18beb39454369a4d76e714ac3ec676`
- Manifest SHA-256: `3a5cb89b0010fc557d4f4ad8a0e941afb98e2fa6faf7d885ab2c46ccd76bcb67`

The [inventory](source-inventory.json) locates both artifacts. No previous bundle was changed.

```sh
python3 tools/evidence.py sdk-521-session-development-v1
```
