# Migration verification

Verified locally on 2026-09-18 UTC. No game was launched and no new engine capability was qualified.

## Import integrity and restore

- All 11 identified bundle archives match their full SHA-256 identities. Their manifests account for 16,222 regular-file/link entries, preserving original locations and portable archive paths. Archive payloads total 1,328,957,822 bytes. Retained tar hardlinks share the verified regular payload and matching manifest size/hash; symlink targets are checked and skipped during restore.
- All bundle archives and file manifests have hash-verified second local copies at `/Users/jackson/Documents/PDX/evidence/native-2026-09-18/`. The registry-owner bundle was restored from that copy and all 1,072 imported files checked against its manifest.
- Seven primary experiment/Linear/Git bundles and the Linear supplement were restored locally with written-file hashes checked. Original source/capsule files remain unchanged; replay runs use restored copies.
- W45/W446 original ZIPs pass all CRC checks and all **32,957 / 19,663** original manifest file hashes. Selected adapter source, DLL/vendor inputs, fixture/settings and independent verifier results restore locally. No Windows game executable is present in those archives.
- Selected original Mac archive manifests and selected source/fixture restores are checked in the private verification records. Native recursion, shared-suite, Mac baseline and locator originals verify **1,969 / 4,405 / 2,571 / 4,102** file hashes.
- Both selected Git bundles pass `git bundle verify` and contain complete histories for seven SDK and two Typed prototype refs. Exact standalone snapshots retain Windows adapter `d53cb4e…`, draft specification `cb8da78…`, shared freeze `b822716…` and Mac baseline `59fd8ba…`.
- Documentation links resolve locally. `.local/` is ignored by Git; normal staging excludes raw evidence. The initial Cargo/src scaffold was preserved.

## Offline findings checks

| Check on restored evidence | Result |
| --- | --- |
| Registry ownership | 41/41 checks: 40 controls plus original 262-file capsule integrity; exact scheduler/snapshot/owner replay |
| Registry discovery | Exact development/held-out/omission partitions; unsupported branch/root controls preserve gaps |
| Council agenda | Ten fields / 21 token paths replay; consumer rejects unqualified completeness and omission/missing-token controls pass |
| Engine command documentation | Exact 47,847-record combined output; omission detected, missing scope remains unknown |
| Command argument grammar | Training/held-out dispatch, missing-body/instruction and record-loss controls; 317-file capsule hashes pass |
| Numeric grammar | 20 native controls, 62 retained cases, 41 retained evaluations, exact snapshot and capsule checks |
| Reference observations | Development/held-out artifact hashes verified; 27 original passing control records retained, not rerun as a fresh native analysis |
| Early observations | Four retained outcomes, exact source/content-manifest hashes, normal ordering/fields/end count, deliberate sequence loss and independent reaping records checked |

The registry consumer demo returns unknown for an absent family and refuses complete snapshot generation. Its symbol-rename example remains illustrative. Offline replay proves recoverable retained bytes and unchanged derivation/control behavior; it does not prove that hooks work on today's running target.

Full private logs and metadata are in `.local/evidence/verification/`: `bundle-verification-final.jsonl`, `offline-replays.json`, `second-copy.json`, `windows-original-manifests.json`, `mac-original-manifests.json`, nested restore records, and `current-target.json`. The installed M45-observe universal image hash was freshly read and matched; no installation was copied or modified. Archive/manifests themselves remain immutable read-only files; new evidence requires a new bundle identity.

## Public bounded replay foundation (SDK-513)

The Rust public-interface suite verifies authored synthetic normal, missing-hook, dropped-record,
and worker-loss cases plus corrupt/missing input and invalid ordering/owner/disposal controls.
Five compile-fail controls reject live Native imports from the evidence package. Dependency and
semantic Clippy negative controls reject a transitive path back to Native and aliased process creation.

Configured public-interface replay of all four accepted SDK-483 private attempts reproduces their
bounded activation/completion and separate confirmed disposal. Every descriptor/core/supporting
artifact matches its retained identity. Three completed runs have archived post-run settings that
differ from their pre-launch manifest hash; results preserve the unavailable original settings bytes
as a provenance gap. Immutable sources, fixture bytes, producer-content manifest, trace, and ownership
journal verify. This establishes historical derivation, not profile restoration or fresh qualification.

No game was launched. Restored originals and sealed bundles were not modified. The separate prepared
working root is `.local/evidence/replay-sdk-513/`; tracked private descriptors contain references only.

## Remaining gaps

Private remote preservation is not established. Historical complete installations/content and several original-path/runtime prerequisites remain external. Initial failed signed-URL requests are recorded with recovered alternate copies where available. SDK-485 portability/maintenance, SDK-509 registry coverage and SDK-510 mounted-file/duplicate questions remain open. [Preservation](preservation.md) gives the exact limits; the migration did not change their ticket status or declare production support.
