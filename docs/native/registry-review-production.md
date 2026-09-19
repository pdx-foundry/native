# SDK-518 production verification after renewed acceptance

The maintainer accepted the [candidate qualification](registry-review-qualification.md) before its reviewed
record was added to source. The ordinary production matrix then passed for both supported names.
The replacement report was accepted after the PR-review fixes. No execution implementation or
external harness changed during this promotion; only the reviewed acceptance record was added.

## Results

Twelve live attempts and four admission refusals passed. Normal retrieval returned 234 tradition
items and 33 category items. Every live attempt independently reaped the game, resolved its durable
reservation, and left no supervisor process. Ordinary-profile snapshots were unchanged and unrelated
sentinels survived. All returned live results matched retained public replay after changing only
`origin` from `live` to `replay`. Caller loss has no live caller result; its independent owner and
replay establish cleanup. Missing-helper unavailability was tested before allocation; the accepted
candidate matrix separately covers live memory-access failures in the shared implementation.

| Registry | Control | Owner termination / refusal | Completion | Items | Disposal |
| --- | --- | --- | --- | ---: | --- |
| traditions | normal | Completed | complete | 234 | confirmed |
| traditions | unsupported | Unsupported | — | 0 | not-launched |
| traditions | unavailable | PrerequisiteMissing | — | 0 | not-launched |
| traditions | incomplete | Completed | incomplete | 234 | confirmed |
| traditions | cancel | Cancelled | worker-lost | 0 | confirmed |
| traditions | caller-loss | CallerLost | worker-lost | 0 | confirmed |
| traditions | timeout | TimedOut | worker-lost | 0 | confirmed |
| traditions | worker-loss | WorkerLost | worker-lost | 0 | confirmed |
| tradition_categories | normal | Completed | complete | 33 | confirmed |
| tradition_categories | unsupported | Unsupported | — | 0 | not-launched |
| tradition_categories | unavailable | PrerequisiteMissing | — | 0 | not-launched |
| tradition_categories | incomplete | Completed | incomplete | 33 | confirmed |
| tradition_categories | cancel | Cancelled | worker-lost | 0 | confirmed |
| tradition_categories | caller-loss | CallerLost | worker-lost | 0 | confirmed |
| tradition_categories | timeout | TimedOut | worker-lost | 0 | confirmed |
| tradition_categories | worker-loss | WorkerLost | worker-lost | 0 | confirmed |

Unknown registry names and an unavailable debugger prerequisite caused refusal without allocating
an attempt. The latter used an external `DEVELOPER_DIR` setting pointing to an absent directory.
The incomplete-result control removed a raw trace record while the worker was stopped; worker loss
sent SIGKILL to Native's retained worker identity after registry load began. Neither control appears
in the public request. Cancellation, caller loss, and timeout retain distinct operation reasons even
when their recorded abnormal worker exit produces `worker-lost` completion.

All controls passed on their first attempt in this matrix. Earlier native inventory-timeout and
harness setup failures remain explicit in the historical qualification and production reports.
Their sealed archives were not modified.

## Identity and artifact independence

Production linked-build identity: `41f05bfba84f5df2bb4450fdff718d1a73edc2c9c14dd9a2ca23637a7345861c`.

The production composition, shared implementation, method, machine, bindings, compiler, release
profile, worker package, target/slice, and debugger identities equal the accepted candidate's.
The full linked build differs because it includes production features and the reviewed acceptance.
Its source snapshots match the promotion commit; candidate evidence remains separately immutable.

A process sandbox denied all reads beneath `.local/evidence`, with a failing direct-read control
confirming enforcement. Ordinary capability admission still returned `Qualified` and `Available`.
Public replay received a readable descriptor reference outside that directory and refused when the
required artifacts were inaccessible. Admission therefore does not depend on historical bundles;
replay continues to enforce its artifact boundary.

Formatting, Clippy, and workspace tests passed after promotion for default, `test-support`,
`maintainer-tools`, and `production`. Both Python suites, admission/replay boundary checks, Windows
cross-Clippy, and retained private replay passed. Real-game controls ran separately from process tests.
Cross-compilation establishes no Windows live support.

## Retention

Bundle `sdk-518-registry-review-production` contains 2,750 files (11,023,558 compressed
bytes), including live results, replay outputs, source/binary snapshots, profiles, reservations,
external controls, the sandbox check, and complete test logs.

- Archive SHA-256: `bcceaba1bc80b3ea98a2f1a2b75a233b91b901f7d18d7047a08908a612cea285`
- Manifest SHA-256: `e2bbf7d73d1a6169783a32ea44cced83933de23a725192c19c9ad9ab3ea8577b`

Archive/member verification passed. The archive and manifest also have a hash-verified second local
copy at `/Users/jackson/Documents/PDX/evidence/native-2026-09-18`. See the [inventory](source-inventory.json).
The machine-readable `production-report.json` is inside the bundle.

```sh
python3 tools/evidence.py sdk-518-registry-review-production
```

Support remains limited to this accepted target, content, toolchain, implementation, and release
profile. Registry items are witnessed at initial loader return, before validation. Field descriptions,
static installation queries, and the live Game session API remain
[SDK-521](https://linear.app/unnamed-system/issue/SDK-521/separate-installation-queries-from-live-game-sessions-in-native).
