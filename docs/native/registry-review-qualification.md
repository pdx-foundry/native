# SDK-518 qualification after PR review

**Status: accepted by the maintainer; renewed ordinary production verification in progress.**

PR review identified two gaps after the [previous qualification](registry-qualification.md) was
accepted and its [ordinary production controls](registry-production.md) passed. The operation
fingerprint now includes the evidence crate's dependency manifest. Ordinary installation admission
now requires the explicit `production` feature, checked by both the caller and independent owner.
These changes alter the qualified implementation. The earlier acceptance does not authorize it.

The consumer contract and capture method are unchanged: `get_registry_items("traditions")` and
`get_registry_items("tradition_categories")` return `registeredItems`, with keys and opaque subject
identities. Normal captures again returned 234 traditions and 33 categories. Native owns the engine
bindings, private content, capture, and supervision. The supported boundary remains the exact
M45-observe ARM64 target and 68 retained installed files. The engine reads private copies of the
registry inputs; items are captured on initial loader return, before later validation.

Field descriptions, gameplay conclusions, mods, DLC additions, later reloads, other registries,
other targets, and Windows live support remain outside this acceptance. The installation-query and
live Game session split is tracked in [SDK-521](https://linear.app/unnamed-system/issue/SDK-521/separate-installation-queries-from-live-game-sessions-in-native).

## Review fixes and their controls

| Fix | Direct verification |
| --- | --- |
| Include `crates/native-evidence/Cargo.toml` in shared implementation identity | The admission boundary check changes only that manifest in a temporary package and confirms that the operation fingerprint changes. |
| Require `production` for ordinary live installation admission | With a matching qualification, tests confirm that both the public client and independent owner refuse with `ProductionFeatureRequired`. No helper or attempt is allocated. |
| Execute the existing binary-extension input-mutation case | The content-integrity test now includes additions with non-text extensions and confirms that the bound snapshot is invalidated. |

All default, `test-support`, `maintainer-tools`, and `production` workspace tests and Clippy checks
passed, together with formatting, Windows cross-Clippy, both Python suites, admission/replay
boundaries, and private historical replay. The final test-only extension-case adjustment also
passed its focused test and formatting. CI passed on macOS, Linux, and Windows. These checks ran
separately from the real-game matrix below.

The production-only consumer was rebuilt for this implementation before acceptance. Both supported
names refuse with `QualificationMissing`; unknown names return `Unsupported`. None allocates an
attempt. Default builds cannot gain ordinary live authority through a matching acceptance record.

## Fresh real-game controls

All 30 controls completed on 2026-09-19 UTC from implementation commit
`f1c9ae43148eb3a47bcd5a664693f0c2206e261d`. Every game was independently reaped, every
durable reservation was resolved, ordinary-profile snapshots matched, and unrelated sentinels
survived. Public replay exactly matched each retained derivation. No operator cleanup was needed.

### `traditions`

| Control | Termination | Activation | Completion | Items/facts | Seconds |
| --- | --- | --- | --- | ---: | ---: |
| normal | Completed | demonstrated | complete | 234 | 38.405 |
| missing-hook | Completed | not-established | unavailable | 0 | 9.515 |
| late-hook | Completed | not-established | unavailable | 0 | 9.132 |
| dropped-record | Completed | demonstrated | incomplete | 233 | 38.419 |
| missing-terminal | Completed | demonstrated | incomplete | 234 | 47.332 |
| access-failure | Completed | demonstrated | unavailable | 0 | 47.493 |
| worker-loss | WorkerLost | demonstrated | worker-lost | 1 | 48.716 |
| cancel | Cancelled | not-established | worker-lost | 0 | 7.188 |
| caller-loss | CallerLost | not-established | worker-lost | 0 | 8.334 |
| timeout | TimedOut | not-established | worker-lost | 0 | 10.187 |

### `tradition_categories`

| Control | Termination | Activation | Completion | Items/facts | Seconds |
| --- | --- | --- | --- | ---: | ---: |
| normal | Completed | demonstrated | complete | 33 | 49.175 |
| missing-hook | Completed | not-established | unavailable | 0 | 11.934 |
| late-hook | Completed | not-established | unavailable | 0 | 10.846 |
| dropped-record | Completed | demonstrated | incomplete | 32 | 49.707 |
| missing-terminal | Completed | demonstrated | incomplete | 33 | 50.287 |
| access-failure | Completed | demonstrated | unavailable | 0 | 49.790 |
| worker-loss | WorkerLost | demonstrated | worker-lost | 1 | 50.662 |
| cancel | Cancelled | not-established | worker-lost | 0 | 7.331 |
| caller-loss | CallerLost | not-established | worker-lost | 0 | 9.246 |
| timeout | TimedOut | not-established | worker-lost | 0 | 9.124 |

### `early-observations`

| Control | Termination | Activation | Completion | Items/facts | Seconds |
| --- | --- | --- | --- | ---: | ---: |
| normal | Completed | demonstrated | complete | 5 | 55.767 |
| missing-hook | Completed | not-established | unavailable | 0 | 12.418 |
| late-hook | Completed | not-established | unavailable | 0 | 12.364 |
| dropped-record | Completed | demonstrated | incomplete | 4 | 53.797 |
| missing-terminal | Completed | not-established | incomplete | 5 | 54.239 |
| access-failure | Completed | not-established | unavailable | 3 | 53.799 |
| worker-loss | WorkerLost | not-established | worker-lost | 3 | 13.716 |
| cancel | Cancelled | not-established | worker-lost | 0 | 7.288 |
| caller-loss | CallerLost | not-established | worker-lost | 0 | 10.517 |
| timeout | TimedOut | not-established | worker-lost | 0 | 9.692 |

Cancellation, caller loss, and timeout retain distinct owner termination reasons, even where
replay describes the abnormal worker exit as `worker-lost`. Legacy observation activation requires
its terminal witness; registry activation can be retained independently of completion.

## Pinned identities

| Identity | Value |
| --- | --- |
| Executable SHA-256 | `3d4c8a7046d87175ce7e3b513b1a2ce589050d654d332744518a49d13ac82216` |
| ARM64 slice SHA-256 | `1e0c9aec45650272fcaecba2eb47f8dce8f17bc08ef2b992be18c99ae098c623` |
| Composition | `30a4f8c87188fc16f756ebe97640373eb8806ed34207ab9c25cf2a335a472ed3` |
| Shared implementation | `8a34146aea615fea371341fa40ab94bbf1dbbc0394c5af128821f3917686dad8` |
| Candidate linked build | `30c38ddc41dc65a1ceea2db22d52dc21152aadf7d30404849aee143db6c2e33b` |
| Compiler | `e794146d99fad54562fdcbd4a7c8b22b0240d2b0312ff6d879d67939b3131850` |
| Debugger | `7c781d24665c41b2e23a404612e317932111a6887f012c5046ce280dc74f5a07` |
| Profile | `release` |
| Production linked build before acceptance | `75b9ea70aeb5ac0f56e1524e27b086f9b89f8afd97adb263af96e2c5bcb1876d` |

The registry method remains `tradition-registry-snapshot/v1`; the historical candidate method is
`registration-category-read-entries/v2`. The strategy is `mac-suspended-child-loader-entry/v3`, with
machine revision `arm64-sdk483-read-entries/v2`. Each attempt retains the exact binding map, machine
mechanisms, worker-package member hashes, content hashes, fixture, tool identities, and raw trace.
All executed source snapshots match the implementation commit above. Shared operation identity
remains distinct from complete linked-build identity and the acceptance record.

## Candidate-to-production differences

| Difference | Evidence before acceptance | Required after acceptance |
| --- | --- | --- |
| Maintainer authority permits private controls; ordinary authority requires the production feature and reviewed acceptance | Direct caller/owner feature-refusal tests, qualification withdrawal/mismatch tests, target/content/helper checks, and production refusals | Admitted normal, unavailable, and incomplete production controls for both names |
| Public client starts and reaps its configured consumer supervisor; candidate harness uses explicit pipes | Client/protocol boundary tests, helper deadlines, finalization-failure disposal preservation, and production consumer build | Ordinary completion, cancellation, caller loss, timeout, and externally induced worker loss |
| Feature selection and acceptance change the complete linked build | Production/candidate operation, composition, compiler, and release-profile identities match | Retain the accepted production build and compare all live results with replay |

Production requests expose no fault controls or qualification bypass. Serialized plans cannot grant
authority. The owner independently checks admission, pinned composition, current inputs, and helper
compatibility. After renewed acceptance, the ordinary production matrix and admission-without-history
check must be repeated before the PR becomes ready. Earlier production evidence is retained as
history; it does not substitute for those checks.

## Retained evidence and earlier failures

The earlier development attempts and isolated one-second process-inventory timeout remain in
`sdk-518-registry-qualification`. That failed attempt preserved disposal and reservation resolution;
its underlying transient cause was not established, and the deadline was not relaxed. The prior
accepted production matrix remains in `sdk-518-registry-production`. Neither archive was modified.
The fresh matrix above passed without changing the implementation or retrying a failed control.

Bundle `sdk-518-registry-review-qualification` contains 5,216 files
(16,298,843 compressed bytes). It includes all 30 attempts, source/binary snapshots,
raw and normalized evidence, public replay, profile manifests, disposed reservation journals,
production refusal binaries, and test logs. Its machine-readable `qualification-report.json` and
`proposed-acceptance.json` are review material, not runtime authority.

- Archive SHA-256: `f19f7954f818e9ba09a18d76cffa12481e624b23ba330c5b8dcfd29d205e1226`
- Manifest SHA-256: `e24f6fd3e46d41de004fa57c64f1497a5e7a01f8e2a4b90fcd0f9d910e1055e4`

Archive and member verification passed. A hash-verified second local copy is retained at
`/Users/jackson/Documents/PDX/evidence/native-2026-09-18`. The [inventory](source-inventory.json)
locates the archive and manifest. Remote preservation remains outside this work.

```sh
python3 tools/evidence.py sdk-518-registry-review-qualification
```

## Maintainer decision

The maintainer explicitly accepted this replacement report in the SDK-518 task before source
promotion. The reviewed record `sdk-518-m45-tradition-registries-v2` is now tracked in
`src/qualification/records/accepted.json`. Renewed ordinary production verification is in progress
and must pass before [PR #6](https://github.com/pdx-foundry/native/pull/6) becomes ready. Relevant implementation changes or failed
controls require renewed evidence and review. No investigation command can promote itself.
