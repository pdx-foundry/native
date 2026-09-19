# Native knowledge and retained evidence

PDX Native is the canonical home for reusable engine integration knowledge. This import consolidates retained SDK testing, Atlas, Typed PDXScript, and Linear material as of 2026-09-18 UTC. It preserves planning and experiments; it introduces no runtime implementation or new engine qualification.

## Start here

1. Choose a capability below. Read its prerequisites and limits before selecting a source.
2. Match the **exact executable, architecture, content boundary, and method revision** in [targets](native/targets.md). Version labels and symbol names are insufficient.
3. Read [retrieval and replay](native/retrieval.md). Verify and restore the cited private bundle. Offline replay and fresh capture are separate operations.
4. Carry missing joins, unsupported targets, failed attempts, and missing evidence into the new work. Record new evidence under a new identity.

| Capability | Local findings | Established boundary |
| --- | --- | --- |
| Open, isolate, close, supervise | [Lifecycle](native/lifecycle.md) | Private profiles, bounded background launch and process disposal; graceful in-game exit remains unproved |
| Inject and observe before parsing | [Early observations](native/early-observations.md) | ARM64 loader-entry activation, bounded registration/field reads, independent parent disposal |
| Call engine functions and locate live objects | [Engine calls and memory](native/engine-calls.md) | Exact-build main-thread calls, native predicates/effects/time/resource reads, qualified country/planet lifetimes |
| Static analysis, references, registries | [Discovery methods](native/discovery.md) | Bounded compiler patterns, token paths, scheduler and owner joins; complete registry coverage remains unproved |
| Adapt and qualify across targets | [Targets and qualification](native/targets.md) | Historical ready-world Mac/Windows scenario transfer; Atlas portability SDK-485 is open |
| Locate imports, second copy, omissions | [Preservation](native/preservation.md), [inventory](native/source-inventory.json) | Private local bundles and verified local restore; remote preservation outstanding |

## Ownership and status

The [bounded replay implementation](design/replay.md) adds a game-free Rust consumer flow for the
retained SDK-483 window. Its tests govern retained derivation behavior, with explicit synthetic and
historical origins. Public live admission uses matching tracked qualification records. The maintainer-only
[candidate capture path](design/candidate-observations.md) can now produce fresh recorded evidence
for the same replay validator; capture success does not grant qualification. SDK-518 established bounded registry questions over shared execution. SDK-521 replaces the live consumer surface with [installation queries and paused Game sessions](design/live-observations.md); its changed implementation requires fresh verification and a matching record.
The [Game session qualification status](native/game-session-qualification.md) retains the current
checks, a failed game-exit control, its verified shutdown correction, and completed development recovery.
All 27 candidate controls and 15 [ordinary production controls](native/game-session-production.md)
passed under the [development policy](development-policy.md).
The [earlier observation qualification report](native/live-observation-qualification.md) is superseded
for promotion. The [fresh registry qualification report](native/registry-qualification.md) retains
30 passing candidate controls and maintainer acceptance before source promotion. The
[ordinary production verification](native/registry-production.md) covers the earlier implementation.
PR review subsequently added evidence-manifest identity and a required production feature. The
[replacement qualification report](native/registry-review-qualification.md) retains 30 fresh passing
controls and renewed maintainer acceptance. The [renewed ordinary production verification](native/registry-review-production.md)
covers both supported registry names on that accepted implementation. The capability pages below
retain the historical experiments and their wider limits.

Atlas retains authoring-rule conclusions and extraction fixtures/coverage. Its consumer pages remain at `/Users/jackson/Developer/pdx-atlas/docs/prototypes/`; the historical capsules copied here retain those fixtures to keep their native evidence replayable. This does not make Native the Atlas rule database.

A **demonstrated** finding has the stated retained controls on its original target. An **accepted bounded result** records human acceptance of that experiment, not production support. A **candidate** lacks required joins or behavioral qualification. **Unknown**, **unavailable**, **incomplete**, and **worker lost** are distinct outcomes. An obsolete or failed method remains historical evidence, with its later correction identified. Unsupported or untested targets receive no inferred qualification.

Linear is provenance, not a required reading service: descriptions, acceptance comments, relevant documents, and retrieved attachment bytes are in the `linear-records` bundle. Source IDs, retrieval metadata, and asset hashes survive locally. Frozen reviews can still say “pending”; the later accepted resolution supplies status. No Linear history or unrelated ticket status was changed.

Raw captures, saves, logs, disassembly, binaries, copied content observations, and original mixed archives stay in gitignored `.local/evidence/`. Keep selected authored findings tracked. Refer to original adapter code for offsets and calling signatures instead of maintaining another offset table here.
