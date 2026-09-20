# SDK-515 loader-entry worker candidate

On 2026-09-18, Native repeated the accepted SDK-483 mechanism with an explicit worker
handshake and transport/package identities. The decision (design page removed 2026-09-20; see Git history)
selects an LLDB subprocess with embedded Python for the loader-entry strategy. This result
is **candidate evidence**, not production support or acceptance of a Rust port.

The retained bundle and its four historical controls were verified first. The installed
M45-observe universal executable, ARM64 slice and all 68 files in the declared producer
content boundary matched. The fixture remained the retained category containing
`tree_template` at line 2 and `traditions` at line 3. The trial ran on ARM64 macOS 26.6.2
(25G83), Xcode LLDB 2100.0.17.203 with embedded Python 3.9.6; the parent used Python 3.13.5.

## Final matrix

Final evidence is `sdk-515-loader-entry-review/trial-03/` inside the bundle below. All attempts
accepted the exact handshake, observed `_dyld_start`, and ended with independent parent
confirmation that the owned game was reaped and absent.

| Attempt | Result from raw records | Records | Parent ownership to disposal |
| --- | --- | --- | --- |
| `20260918-140905-none-3cee0f` | Complete: three registration call entries, two fields with one owner/file/thread, matching loader return, continuous sequence and terminal count | 16 | 30.844 s |
| `20260918-140937-missing-432fd9` | Unavailable: omitted field hook; no resume or observations | 3 | 2.360 s |
| `20260918-140939-incomplete-e4f5c6` | Incomplete: record 11 dropped; only the second field arrived while terminal count remained two | 15 | 31.445 s |
| `20260918-141011-worker-loss-ee2224` | Worker lost: SIGKILL after three registrations, no terminal; parent killed and reaped game | 10 | 2.966 s |

The full final batch, including preflight and guard compilation, took 70.963 seconds. These
are measured wall intervals, not active labor or a maintenance-cost estimate. Worker and
parent clock domains remain separate. Raw before/after hashes establish that the executable, recorded producer filename set/bytes
and four protected ordinary-profile files were unchanged after each final attempt.

Three batches (12 game launches) produced the expected four outcomes. All games were reaped.
The first two batches remain in the original `sdk-515-loader-entry` bundle (68.172 and
64.330 seconds). PR review found missing verifier checks for the fixture path, startup thread,
resume ordering, missing-hook cause and raw preservation hashes. The final `trial-03` batch
adds the required raw witnesses and passes all strengthened checks. The earlier captures are
not rewritten or presented as proof of the new checks.

There were zero new debugger-access approvals and zero manual cleanup actions. The earlier
SDK-483 debugger permission intervention remains a prerequisite inherited from this host.
Review findings and corrections are linked from the new bundle's `review.json`.

## Identities and preservation

Private bundle ID: `sdk-515-loader-entry-review` (159 files, 375,683 compressed bytes).

- Archive: `sdk-515-loader-entry-review-3734647f1f3820bf.tar.gz`
- Archive SHA-256: `3734647f1f3820bfad6cf584565f3f8bfba632fd64caf39adaef153733d5bf71`
- Manifest SHA-256: `cdda4163e80d060f0831b7ce509bb7410384621cc202f5305811af106549fc1c`

[source-inventory.json](source-inventory.json) pins both identities. The bundle and file
manifest are in `.local/evidence/bundles/`. A verified second local copy is in
`/Users/jackson/Documents/PDX/evidence/native-2026-09-18/`. Private remote preservation
remains outstanding under the existing preservation policy.

The new bundle retains the final batch, private profiles, raw worker/parent logs, handshakes,
traces, before/after preservation hashes, source and guard artifacts, preflight/version reports
and the review correction record. `trial-03/trial-source/` retains the actual trial tools. Each run's `source/` holds its
executed producer scripts; `preflight.json` and `expected.json` pin the package and tool
identities. No raw game records or copied native callback sources are published in Git.

Offline verification after restoring into a new directory:

```sh
python3 tools/evidence.py sdk-515-loader-entry-review --restore
python3 tools/loader-entry-trial/verify.py \
  .local/evidence/restored/sdk-515-loader-entry-review/sdk-515-loader-entry-review/trial-03
```

This restore and offline verification passed. The normal/default and test-support Rust
suites, restored private replay, formatting, both Clippy configurations, replay/admission
boundary checks and eight game-free Python rejection tests also passed. Tests reject altered
handshake identities, absent/ambiguous source edits, dropped records, changed thread/owner
joins, invalid terminal counts, disabled hooks, unrelated files/failure causes, pre-resume observations
and changed preservation hashes.

## Limits carried forward

These observations remain call/read entries, not successful registration returns, stored
values, complete registries, validation results or gameplay. Content identity is bounded to
the declared files; the full installation and DLC closure are not qualified. The selected
worker is concrete, but the production Rust parent, generated wire bindings, package
distribution, concurrency lock, controller/owner-loss recovery and broader target/tool
qualification remain work for later implementation. The public library still launches no
game and admits no live operation.
