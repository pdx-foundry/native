# SDK-516 candidate lifecycle controls

The initial `batch-04` controls ran the exact M45-observe target through the consumer-hosted Rust
supervisor on 2026-09-18. Each game remained suspended and was independently reaped. This proves
bounded candidate lifecycle behavior, not observation activation or public live qualification.

| Control | Attempt | Controller wall seconds | Disposal |
| --- | --- | ---: | --- |
| normal | `17676-1789758252080003000` | 3.922 | Reaped |
| cancel | `17698-1789758255868413000` | 3.494 | Reaped |
| worker-loss | `17705-1789758259371615000` | 3.506 | Reaped |
| caller-loss | `17711-1789758262876746000` | 3.528 | Reaped |
| timeout | `17717-1789758266410356000` | 32.967 | Reaped |

Wall times include controller preparation, target integrity checks, and report handling; they are
not owner cleanup durations. Raw owner snapshots, reports, captures, and per-scenario stdout/stderr
are retained. Entire ordinary-profile file inventories/hashes matched before and after each run;
an unrelated sentinel process remained alive. The game was never resumed and loaded no world.

During the timeout control, a second copy of the consumer executable in another checkout directory
was refused by the same host lock. A compiled harmless process named `stellaris` at a long path
caused a conflict refusal with `NotLaunched`; its process identity and liveness remained unchanged.
The live worker-loss control sends the consumer's failure notification. The game-free unit control
also kills an actual harmless worker before notifying the owner; there is no debugger in this slice.

Game-free process tests cover partial launch, failed journal writes, unreadable/unknown/truncated
records, and owner death. The owner-death test kills the independent owner after reservation but
before spawning any game; a free OS lock still cannot admit another attempt. No orphaned real game
was created to test this rule, and automatic recovery remains unimplemented.

The retained earlier batches are not rewritten. Batch 01 failed before game allocation because
controller preparation exceeded the debug handshake budget; preparation now precedes startup.
Batches 02–03 supplied successful lifecycle records, but their copied Apple system-binary conflict
fixtures were killed by macOS before inspection. The final batch compiles its own stable fixture.
The inventory also uses untruncated process paths, covered by a long-path regression test.

## Initial preservation

Private bundle `sdk-516-lifecycle` contains all four batches, the final source and harness binary,
and reservation snapshots. It has 285 files and 2,159,557 compressed bytes.

- Archive SHA-256: `c8aac6f3dab589776622bbe7331b890e57911b04c2de0aaa4d7697a7909ceb1f`
- Manifest SHA-256: `34af2495e9fa8692ca430cab67acb45391e49a05964e1883c323410971537044`
- Captured linked Native build identity: `7bc577542a63ba8010b99854a42bbce95e64510ab3d12299302fc96b65b55145`

The tracked [inventory](source-inventory.json) pins both bundle identities. Archive and per-file
verification passed; the archive and manifest have a hash-verified second local copy at the existing
preservation location. Private remote preservation remains outstanding under the existing policy.

```sh
python3 tools/evidence.py sdk-516-lifecycle
# Optional restore to a new directory; this does not launch a game:
python3 tools/evidence.py sdk-516-lifecycle --restore
```

See [consumer-hosted lifecycle](../design/lifecycle.md) for integration, provisioning, and the
explicit real-game control command. Default, synthetic, and maintainer suites, Clippy, architecture
boundary checks, private replay, and a Windows maintainer cross-compilation check passed locally.
Windows/Linux runtime support is not implied by those checks.

## Review corrections and fresh repeat

After [PR #4 review](https://github.com/pdx-foundry/native/pull/4), the implementation preserves
pre-start worker-loss causes, restores reserved state after a failed disposal commit, writes the
report after capture diagnostics are known, retains conflict attempts before returning, and hashes
the linked evidence manifest into build identity. Setup and suspended hold now have separate
30-second bounds, so a valid 29,999 ms hold gets its full window. Regression tests cover these
policies and unsupported-host refusal before resource allocation.

A fresh seven-control batch passed on the corrected implementation. Six games were reaped; the
pre-start worker-loss attempt correctly launched none. Contention and the ordinary-instance
control passed again, with the conflict report and capture now retained. Ordinary-profile hashes
and the unrelated sentinel remained unchanged throughout.

| Control | Attempt | Controller wall seconds | Disposal |
| --- | --- | ---: | --- |
| normal | `20542-1789759660309380000` | 3.890 | Reaped |
| long-hold | `20549-1789759664058353000` | 33.563 | Reaped |
| worker-loss-before-launch | `20764-1789759697590258000` | 3.079 | NotLaunched |
| cancel | `20773-1789759700673976000` | 3.452 | Reaped |
| worker-loss | `20784-1789759704136239000` | 3.482 | Reaped |
| caller-loss | `20790-1789759707634971000` | 3.501 | Reaped |
| timeout | `20800-1789759711143179000` | 33.547 | Reaped |

Private bundle `sdk-516-lifecycle-review` retains this fresh batch, captured source, executable, review
findings, and reservation snapshots (194 files; 1,052,021 compressed bytes).
Both files were verified and copied to the existing second local preservation location.

- Archive SHA-256: `8858f19a016a45211f1a8616f8630882d9d042ec73a173ec5e45edd46bbba0c3`
- Manifest SHA-256: `1285c9d0dc0645d65fb2244d3bef89cd9b520a7eafd8ce4162163466880b1abb`
- Captured linked Native build identity: `1b4ecd7ffae6d39c60eb6eca46c4eb3a1e5fc6e160fb73a6d64e78bdad89b47e`

```sh
python3 tools/evidence.py sdk-516-lifecycle-review
```

The earlier bundle remains intact. These fixes and captures do not promote a public live operation
or qualify any debugger/observation behavior.

A subsequent CI-only change moved the shared test module below runtime items for Clippy on non-Mac hosts. No lifecycle runtime logic changed; the capture identities above still identify the executed source exactly.
