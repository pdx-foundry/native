# SDK-516 candidate lifecycle controls

The final `batch-04` controls ran the exact M45-observe target through the consumer-hosted Rust
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

## Preservation

Private bundle `sdk-516-lifecycle` contains all four batches, the final source and harness binary,
and reservation snapshots. It has 285 files and 2,159,557 compressed bytes.

- Archive SHA-256: `c8aac6f3dab589776622bbe7331b890e57911b04c2de0aaa4d7697a7909ceb1f`
- Manifest SHA-256: `34af2495e9fa8692ca430cab67acb45391e49a05964e1883c323410971537044`
- Final linked Native build identity: `7bc577542a63ba8010b99854a42bbce95e64510ab3d12299302fc96b65b55145`

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
