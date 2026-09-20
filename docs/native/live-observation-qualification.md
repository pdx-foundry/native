# SDK-518 bounded observation qualification report

**Superseded for promotion:** the consumer contract has changed to registry retrieval. These retained
early-observation controls remain historical evidence; they do not qualify the revised implementation.
The current registry qualification has separate maintainer acceptance. See the current [registry qualification report](registry-qualification.md).

**Status: awaiting maintainer acceptance.** The public API and shared implementation are committed at
`00a873bb43dbc199d9102e238be5af94be580aa1`. The tracked acceptance registry is still empty. This report requests
acceptance of the exact bounded candidate qualification below before adding its reviewed source record.
Ordinary production live controls and PR publication remain required after that promotion.

## Outcome and scope

The final fresh candidate matrix passed all ten controls on 2026-09-18. Normal capture demonstrated
activation and retained three registration call entries plus `tree_template` and `traditions` read
entries, with source/owner/thread joins. Missing/late hooks refused resume; stream damage prevented
completion; actual memory-read failure was unavailable; worker loss preserved three earlier facts.
Cancellation, caller loss, and timeout retained distinct owner termination reasons.

Every attempt confirmed game reaping and resolved its durable reservation. All ordinary-profile
inventories/hashes matched before and after; the unrelated sentinel survived. Public replay verified
every descriptor. No operator intervention or reservation clearance was needed.

Scope is the exact M45-observe ARM64 macOS image, retained 68-file content boundary, exact two-field
fixture, and 1–180 second observation deadline. These are call/read entries, not successful returns,
stored values, validation, complete registries, or rules. No Windows/stable portability, complete
tradition coverage, Atlas integration, clean pinned-build reproduction, or Atlas release is claimed.

## Final control matrix

All rows have confirmed independent disposal. Wall time includes preparation, startup, capture, and
replay; it is not the observation or disposal budget.

| Control | Owner termination | Activation | Observation completion | Facts | Wall seconds |
| --- | --- | --- | --- | ---: | ---: |
| normal | Completed | demonstrated | complete | 5 | 35.799 |
| missing-hook | Completed | not-established | unavailable | 0 | 9.017 |
| late-hook | Completed | not-established | unavailable | 0 | 10.819 |
| dropped-record | Completed | demonstrated | incomplete | 4 | 34.984 |
| missing-terminal | Completed | not-established | incomplete | 5 | 34.387 |
| access-failure | Completed | not-established | unavailable | 3 | 34.291 |
| worker-loss | WorkerLost | not-established | worker-lost | 3 | 9.128 |
| cancel | Cancelled | not-established | worker-lost | 0 | 6.687 |
| caller-loss | CallerLost | not-established | worker-lost | 0 | 6.688 |
| timeout | TimedOut | not-established | worker-lost | 0 | 7.773 |

Worker loss in the replay of cancellation/timeout is the recorded abnormal worker exit. It does not
replace the distinct owner termination reason. Missing terminal evidence cannot establish activation
or completion under the existing observation validator.

## Exact implementation and authority

| Identity | Value |
| --- | --- |
| Executable SHA-256 | `3d4c8a7046d87175ce7e3b513b1a2ce589050d654d332744518a49d13ac82216` |
| ARM64 slice SHA-256 | `1e0c9aec45650272fcaecba2eb47f8dce8f17bc08ef2b992be18c99ae098c623` |
| Composition | `7c4eb88e274f84b3345f462a00d0ca94bc8a44500c523dbea6f43bb0df9285fe` |
| Shared implementation | `810f0685fe7feed40bd6f47f0d2f82e8879bf98bcc6c0d62380ca337911c05a2` |
| Rust compiler identity | `e794146d99fad54562fdcbd4a7c8b22b0240d2b0312ff6d879d67939b3131850` |
| Candidate linked build | `f72a3f17e6ce17439141f435a468622be51a24c2d5e075a5685388d87cb26f25` |
| Debugger identity | `7c781d24665c41b2e23a404612e317932111a6887f012c5046ce280dc74f5a07` |
| Production linked build | `76dcc8c9517bbcc71eca8ea2438737be92a776010ef2c49cd9636549464678d3` |
| Method | `registration-category-read-entries/v2` |
| Strategy | `mac-suspended-child-loader-entry/v3` |
| Machine revision | `arm64-sdk483-read-entries/v2` |
| Profile | `release` |

The final candidate and production composition identities match. Their selected worker/guard package
bytes, machine data, bindings, method, compiler identity, and release profile are identical. Full linked
build identities differ because Cargo feature modes differ. Qualification also pins build flags;
a debug build or changed compiler does not inherit this release-profile qualification.

The compiled host resolver supplies the strategy/package. The recipe/composer supplies machine
mechanisms, bindings, and content prerequisites. Shared execution receives these values. A synthetic
recipe/binding variation reaches the selected strategy through the same observation preparation call,
including changed machine/package provenance and content refusal. It establishes structure only.

The ordinary entry repeats tracked admission and pins the accepted debugger; the maintainer entry
bypasses acceptance and exposes controlled faults. Both use the same owner, worker, capture, and pure
observation validator. The ordinary client adds normalized `Live` results; replay returns `Replay`.
Candidate reports cannot convert into supported live results.

Production build/feature and protocol boundaries passed. The production consumer currently returns
`QualificationMissing` without allocating an output directory. Once accepted, the production matrix
must verify normal, unavailable, incomplete, cancellation, caller-loss, timeout, and external worker-loss
cases through ordinary admission and compare each live result with retained replay. No public request
contains fault controls. This remaining stage covers the authorization, tool pin, and client differences.

## Verification and preservation

- Default workspace: 74 tests passed; test-support: 82; maintainer-tools: 78. Private historical replay: one additional test passed.
- Formatting and Clippy passed for default, test-support, maintainer-tools, and production. Windows maintainer cross-compilation with Clippy passed; this is not Windows live qualification.
- Both Python suites, generated protocol drift, replay dependency boundaries, and production/synthetic/maintainer compile boundaries passed.
- The first ten-control batch also passed. The final batch reran all controls after compiler/profile fingerprinting and final authority tests. Both batches are retained under separate identities.
- Final executed source snapshots match the committed implementation. Acceptance-only edits are excluded from the operation fingerprint and still change the full linked-build identity.

Private bundle `sdk-518-qualification` contains 1,617 files (7,985,045 compressed bytes),
with both matrices, source/executable snapshots, profile comparisons, worker/tool identities, raw and
normalized traces, owner journals, production binaries/refusal, and complete check logs. Archive and
file verification passed; archive and manifest also have a hash-verified second local copy at the
existing preservation location. Remote preservation remains outside this work.

- Archive SHA-256: `4c54af09163219d4e1761fc2e5abf981af33d43d4a60eb6415704e96d2f64cf9`
- Manifest SHA-256: `01cea4724f1341bbd8d845317a4306033048e79a0440a3ed955fee2c65ccb59d`

```sh
python3 tools/evidence.py sdk-518-qualification
# Optional restore to a new directory; no game launch:
python3 tools/evidence.py sdk-518-qualification --restore
```

The [inventory](source-inventory.json) locates the bundle. The machine-readable report and proposed
acceptance are `sdk-518-qualification/qualification-report.json` and `proposed-acceptance.json` within
it. The proposal is evidence for review; it has not been added to runtime authority.

## Maintainer decision

Accept this exact bounded candidate qualification for source promotion, subject to the required
ordinary production controls before PR publication. Approval authorizes adding the proposed record
to `src/qualification/records/accepted.json`; the investigation binary cannot do that. If relevant
implementation behavior changes or production controls fail, revise the report and requalify before
publishing. See the consumer contract (design page removed 2026-09-20; see Git history) for startup and prerequisite details.
