# SDK-515 loader-entry worker candidate

On 2026-09-18, Native repeated the accepted SDK-483 mechanism with an explicit worker
handshake and transport/package identities. The [decision](../design/debugger-worker.md)
selects an LLDB subprocess with embedded Python for the loader-entry strategy. This result
is **candidate evidence**, not production support or acceptance of a Rust port.

The retained bundle and its four historical controls were verified first. The installed
M45-observe universal executable, ARM64 slice and all 68 files in the declared producer
content boundary matched. The fixture remained the retained category containing
`tree_template` at line 2 and `traditions` at line 3. The trial ran on ARM64 macOS 26.6.2
(25G83), Xcode LLDB 2100.0.17.203 with embedded Python 3.9.6; the parent used Python 3.13.5.

## Final matrix

Final evidence is `sdk-515-loader-entry/trial-02/` inside the bundle below. All attempts
accepted the exact handshake, observed `_dyld_start`, and ended with independent parent
confirmation that the owned game was reaped and absent.

| Attempt | Result from raw records | Records | Parent ownership to disposal |
| --- | --- | --- | --- |
| `20260918-135523-none-b3f3ea` | Complete: three registration call entries, two fields with one owner/file/thread, matching loader return, continuous sequence and terminal count | 16 | 28.490 s |
| `20260918-135552-missing-0a4fa9` | Unavailable: omitted field hook; no resume or observations | 3 | 2.128 s |
| `20260918-135554-incomplete-29ee29` | Incomplete: record 11 dropped; only the second field arrived while terminal count remained two | 15 | 28.326 s |
| `20260918-135623-worker-loss-35bc84` | Worker lost: SIGKILL after three registrations, no terminal; parent killed and reaped game | 10 | 2.804 s |

The full final batch, including preflight and guard compilation, took 64.330 seconds. These
are measured wall intervals, not active labor or a maintenance-cost estimate. Worker and
parent clock domains remain separate. The executable, recorded producer files and four
protected ordinary-profile files were unchanged after each attempt.

Two batches (eight game launches) were run. Both produced the expected four outcomes.
`trial-01/` preserves the preliminary batch (68.172 seconds). Between batches, source
verification was tightened against the sealed manifest, tool-source snapshots and fixture
hash checks were added, and synthetic rejection tests were added. There were zero new
debugger-access approvals and zero manual cleanup actions. The earlier SDK-483 debugger
permission intervention is still a prerequisite inherited from this host, not a fresh result.

## Identities and preservation

Private bundle ID: `sdk-515-loader-entry` (304 files, 738,447 compressed bytes).

- Archive: `sdk-515-loader-entry-16fd79a9602733b9.tar.gz`
- Archive SHA-256: `16fd79a9602733b97c2d52468efc5a14768e05473442b6e61434276675cc8ad0`
- Manifest SHA-256: `e92815b0ab279c3c907f3c33bc8baf4181cfbb3225b5fa7e08ef61d1e687328a`

[source-inventory.json](source-inventory.json) pins both identities. The bundle and file
manifest are in `.local/evidence/bundles/`. A verified second local copy is in
`/Users/jackson/Documents/PDX/evidence/native-2026-09-18/`. Private remote preservation
remains outstanding under the existing preservation policy.

The bundle retains both batches, private profiles, raw worker/parent logs, handshakes,
traces, source and guard artifacts, preflight/version reports and a source-review record.
`trial-02/trial-source/` retains the actual trial tools. Each run's `source/` holds its
executed producer scripts; `preflight.json` and `expected.json` pin the package and tool
identities. No raw game records or copied native callback sources are published in Git.

Offline verification after restoring into a new directory:

```sh
python3 tools/evidence.py sdk-515-loader-entry --restore
python3 tools/loader-entry-trial/verify.py \
  .local/evidence/restored/sdk-515-loader-entry/sdk-515-loader-entry/trial-02
```

This restore and offline verification passed. The normal/default and test-support Rust
suites, restored private replay, formatting, both Clippy configurations, replay/admission
boundary checks and four game-free Python rejection tests also passed. Tests reject altered
handshake identities, absent/ambiguous source edits, dropped records, changed thread/owner
joins, invalid terminal counts and disabled hooks.

## Limits carried forward

These observations remain call/read entries, not successful registration returns, stored
values, complete registries, validation results or gameplay. Content identity is bounded to
the declared files; the full installation and DLC closure are not qualified. The selected
worker is concrete, but the production Rust parent, generated wire bindings, package
distribution, concurrency lock, controller/owner-loss recovery and broader target/tool
qualification remain work for later implementation. The public library still launches no
game and admits no live operation.
