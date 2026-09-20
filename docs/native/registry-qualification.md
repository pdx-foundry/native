# SDK-518 registry qualification report

**Status: superseded after PR review; retained historical qualification.**
The reviewed implementation below was accepted and passed production controls. Subsequent review
fixed the missing evidence-manifest fingerprint and required the production feature for ordinary
admission. The tracked acceptance has been removed. The [replacement report](registry-review-qualification.md)
presents fresh evidence for renewed maintainer review. This report replaces the
[earlier observation proposal](live-observation-qualification.md). The implementation at commit
`02f849a60261787400ebeffe74279f008ae0925d` changes the consumer question and the native method.
The earlier proposal does not qualify this implementation. No investigation command can add an
acceptance record.

## Consumer result and scope

`get_registry_items("traditions")` asks for engine collection keys. The first real normal capture returned
234 tradition entries; `get_registry_items("tradition_categories")` returned 33 category entries. Consumers
supply no extraction fixture or observation plan. Native owns registry names, binding selection,
private content preparation, engine capture, and supervision. The consumer configures its supervisor
executable once. Unknown names, including the singular `tradition`, return `Unsupported`.

The public result exposes `registered_items` in Rust and `registeredItems` in JSON, with a key and
opaque subject identity for each item. `get_registry(name)` is reserved for a follow-up that describes
the registry and its reader fields; it is not implemented by this ticket.

These captures used the maintainer authorization entry point with the same shared execution method.
The maintainer accepted this report before its source record was added. The public replay example
can show the retained answers without a game installation; see the
consumer guide (design page removed 2026-09-20; see Git history).

The supported question is bounded to the exact M45-observe ARM64 macOS target and the retained
68-file installed input boundary: 67 registry files and launcher identity. Native mounts private
copies of the two registry directories, and the engine parses them. The worker reads keys from
actual engine objects on return from the initial collection loader, before later validation.
`Complete` means every collection slot at that boundary was witnessed. It does not mean later
validation passed or that all traditions are available to an empire.

This acceptance covers `traditions` and `tradition_categories`, with an optional 1–180 second deadline.
It does not establish fields or schemas, category relationships, gameplay rules, user mods, DLC
additions, later reloads, other registries or targets, Windows support, or portability. Atlas
integration remains SDK-519; clean pinned-build reproduction remains SDK-520.

## Final control matrix

All 30 final controls completed on 2026-09-19 UTC. Every attempt confirmed independent game reaping,
resolved its durable reservation, and retained replayable evidence. Ordinary-profile inventories
matched before and after each run, and each unrelated sentinel process survived. No reservation
clearance or other operator cleanup was needed. Public replay matched each captured derivation.

### `traditions`

| Control | Owner termination | Activation | Completion | Entries/facts | Wall seconds |
| --- | --- | --- | --- | ---: | ---: |
| normal | Completed | demonstrated | complete | 234 | 40.669 |
| missing-hook | Completed | not-established | unavailable | 0 | 9.944 |
| late-hook | Completed | not-established | unavailable | 0 | 10.841 |
| dropped-record | Completed | demonstrated | incomplete | 233 | 56.381 |
| missing-terminal | Completed | demonstrated | incomplete | 234 | 60.332 |
| access-failure | Completed | demonstrated | unavailable | 0 | 58.423 |
| worker-loss | WorkerLost | demonstrated | worker-lost | 1 | 57.628 |
| cancel | Cancelled | not-established | worker-lost | 0 | 8.644 |
| caller-loss | CallerLost | not-established | worker-lost | 0 | 8.812 |
| timeout | TimedOut | not-established | worker-lost | 0 | 10.102 |

### `tradition_categories`

| Control | Owner termination | Activation | Completion | Entries/facts | Wall seconds |
| --- | --- | --- | --- | ---: | ---: |
| normal | Completed | demonstrated | complete | 33 | 53.182 |
| missing-hook | Completed | not-established | unavailable | 0 | 13.275 |
| late-hook | Completed | not-established | unavailable | 0 | 13.139 |
| dropped-record | Completed | demonstrated | incomplete | 32 | 55.026 |
| missing-terminal | Completed | demonstrated | incomplete | 33 | 58.140 |
| access-failure | Completed | demonstrated | unavailable | 0 | 55.508 |
| worker-loss | WorkerLost | demonstrated | worker-lost | 1 | 53.641 |
| cancel | Cancelled | not-established | worker-lost | 0 | 7.862 |
| caller-loss | CallerLost | not-established | worker-lost | 0 | 8.787 |
| timeout | TimedOut | not-established | worker-lost | 0 | 9.689 |

### `early-observations`

| Control | Owner termination | Activation | Completion | Entries/facts | Wall seconds |
| --- | --- | --- | --- | ---: | ---: |
| normal | Completed | demonstrated | complete | 5 | 59.274 |
| missing-hook | Completed | not-established | unavailable | 0 | 15.916 |
| late-hook | Completed | not-established | unavailable | 0 | 15.136 |
| dropped-record | Completed | demonstrated | incomplete | 4 | 57.138 |
| missing-terminal | Completed | not-established | incomplete | 5 | 57.349 |
| access-failure | Completed | not-established | unavailable | 3 | 57.201 |
| worker-loss | WorkerLost | not-established | worker-lost | 3 | 13.341 |
| cancel | Cancelled | not-established | worker-lost | 0 | 8.847 |
| caller-loss | CallerLost | not-established | worker-lost | 0 | 9.290 |
| timeout | TimedOut | not-established | worker-lost | 0 | 10.585 |

Wall time includes preparation, startup, capture, and replay. The owner retains cancellation,
caller loss, and timeout as distinct termination reasons, even where replay records the worker
exit as `worker-lost`. The legacy observation validator requires its terminal to establish
activation; the registry validator can retain activation independently of collection completion.

## Exact implementation and authority

| Identity | Value |
| --- | --- |
| Executable SHA-256 | `3d4c8a7046d87175ce7e3b513b1a2ce589050d654d332744518a49d13ac82216` |
| ARM64 slice SHA-256 | `1e0c9aec45650272fcaecba2eb47f8dce8f17bc08ef2b992be18c99ae098c623` |
| Composition | `1b3345f519da2ac7411c9dcf73c09cb50b9bed842df9325d3d44b45b6b9c3376` |
| Shared implementation | `9098659e5758617b33173ef8a8314e0e8b4c2bb5ae11d51f2da0e100bd4047be` |
| Rust compiler identity | `e794146d99fad54562fdcbd4a7c8b22b0240d2b0312ff6d879d67939b3131850` |
| Candidate linked build | `1250f80d45b1c9ee2a42bbf485bf6b368a9f5329630d5caa4e15d542a7eede1e` |
| Production linked build before acceptance | `cac1ec0452f6c52a363ea69067b4889f2619f738e3282b1e13c714167c437248` |
| Debugger identity | `7c781d24665c41b2e23a404612e317932111a6887f012c5046ce280dc74f5a07` |
| Registry method | `tradition-registry-snapshot/v1` |
| Historical candidate method | `registration-category-read-entries/v2` |
| Strategy | `mac-suspended-child-loader-entry/v3` |
| Machine revision | `arm64-sdk483-read-entries/v2` |
| Profile | `release` |

The selected operation carries machine mechanisms, registry and legacy bindings, prerequisites,
strategy, package, and source identities into execution. Session, supervisor, and capture consume
that selection. Synthetic variation tests change the binding, content prerequisite, and package
identity and verify that they reach the shared executor. These tests establish structure only.

The complete binding map, machine registers, worker-package member hashes, copied-content hashes,
legacy two-field fixture, debugger identity, and raw traces are retained with each attempt. Shared
implementation identity is distinct from the full linked-build identity and from acceptance.
Changing only acceptance leaves the operation fingerprint unchanged and changes the linked build.

### Candidate-to-production differences

| Difference | Evidence before acceptance | Required check after acceptance |
| --- | --- | --- |
| Maintainer entry bypasses acceptance and permits private controls; ordinary entry repeats admission and pins the accepted helper | Qualification withdrawal/mismatch, changed target/content, missing prerequisites, invalid deadline/fixture, incompatible helpers, and maintainer-protocol refusal tests; production consumer refuses before allocation | Ordinary admitted normal, unavailable, and incomplete captures for both names |
| Public client starts the configured supervisor command, bounds protocol reads, normalizes results, and reaps the supervisor; candidate harness uses explicit pipes | Client boundary tests, silent-helper deadline, candidate-origin refusal, finalization failure preserving disposal, and production-only consumer build | Ordinary completion, cancellation, caller loss, timeout, and actual external worker loss |
| Feature modes change the full linked build | Same production/candidate composition and shared implementation; retained production build metadata; compile boundaries | Retain the accepted production build and compare its live results with public replay |

Production requests cannot set fault controls or bypass qualification. Serialized requests carry
intent; the independent owner recomposes the operation and checks current inputs and authority.
Faults in ordinary production verification must come from the external harness. No historical
bundle is needed for admission; replay explicitly requires and verifies the referenced artifacts.

## Development findings

The first attempted registry witness was entry to the later post-read phase. It was not reached
within 180 seconds. The retained attempt timed out, independently reaped the game, and resolved its
reservation. The final method instead witnesses return from the initial loader, where the engine
collection is populated. Static disassembly and the failed attempt remain in the evidence.

The next development attempt returned 234 tradition keys. A later category attempt returned 33
keys after adding private copied inputs. These development captures are not the final qualification;
their executed sources are retained separately. The final matrix uses the frozen shared implementation.

A complete 30-control batch also passed before the consumer names changed to `get_registry_items`
and `registeredItems`. It is preserved in `sdk-518-registry-pre-rename` as superseded, unaccepted
evidence. The final matrix above repeats all controls on the final public contract.

The first post-rename normal attempt failed because the one-second process-inventory deadline
expired after activation. It retained no registered items, reported failure instead of completion,
independently reaped the game, and resolved its reservation. The ordinary profile and unrelated
sentinel were preserved. Twenty subsequent runs of the exact inventory command completed within
0.03 seconds; the cause of the isolated delay was not established. The deadline was not relaxed.
The failed attempt is retained as `failed-inventory`, and the complete final matrix was restarted
under a new attempt root. This is an explicit availability limitation for maintainer review.

## Verification and preservation

Formatting, Clippy, and workspace tests passed for default, `test-support`, `maintainer-tools`, and
`production`. Both Python suites passed, as did generated protocol checks, admission/replay dependency
boundaries, and retained private historical replay. Windows cross-compilation with Clippy passed;
it does not establish Windows live support. Process tests ran separately from real-game controls.

Registry derivation tests cover a witnessed empty collection, missing evidence, wrong owner/thread,
duplicate keys, count/index/terminal mismatches, and dropped records. Artifact tests reject absent,
changed, or incorrectly joined content even when the surrounding manifest is rehashed. Historical
replay still accepts its original artifacts and refuses registry events mixed into its contract.
Replaying a pre-rename real registry capture produces an identical result after changing only the
collection field from `entries` to `registeredItems`, including identical item identities and provenance.

Production-only checks confirm both known names refuse with `QualificationMissing`, unknown names
return `Unsupported`, and these refusals allocate no attempt directory. Those pre-promotion checks remain retained. The accepted record then enabled ordinary admission;
the [ordinary production controls](registry-production.md) passed before the initial PR publication.
That record has since been removed because review fixes changed the implementation.

Private bundle `sdk-518-registry-qualification` contains 6,057 files (20,425,092
compressed bytes). It retains the final matrices, executed source and binaries, failed development
attempt, static disassembly, exact profile and content manifests, tool/package identities, owner
journals, public replay outputs, production refusal binaries, and full check logs. Final executed
source snapshots match implementation commit `02f849a60261787400ebeffe74279f008ae0925d`.

- Archive SHA-256: `5a6e23bd36d1df5409c8c30d13751986e667ae4d5b366a2d58264b1c73475d80`
- Manifest SHA-256: `714abba923efc7575c486665e81343dfd170139edb46116ad46dd90e71d76c5d`

Archive and member verification passed. The archive and manifest also have a hash-verified second
local copy at `/Users/jackson/Documents/PDX/evidence/native-2026-09-18`. Remote preservation is outside
this work. The [inventory](source-inventory.json) locates the private bundle.

```sh
python3 tools/evidence.py sdk-518-registry-qualification
# Optional restore to a new directory; no game launch:
python3 tools/evidence.py sdk-518-registry-qualification --restore
```

The machine-readable report and proposed source record are `sdk-518-registry-qualification/qualification-report.json`
and `proposed-acceptance.json` within the archive. The archived proposal remains immutable review material; the tracked record supplies runtime
authority. The reviewed proposal was tracked in the acceptance registry and has since been removed for renewal.

## Maintainer decision

The maintainer explicitly accepted this report in the SDK-518 task before source promotion.
The reviewed record was then added to `src/qualification/records/accepted.json`. The [ordinary production controls](registry-production.md) subsequently passed. The deferred installation-context/live-session API
redesign is tracked in [SDK-521](https://linear.app/unnamed-system/issue/SDK-521/separate-installation-queries-from-live-game-sessions-in-native).
Relevant implementation changes or failed controls require renewed evidence and review.
