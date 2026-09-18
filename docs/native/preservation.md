# Evidence preservation and migration limits

The canonical authored Native knowledge is in this repository's tracked `docs/`. Private identified bundles and full file manifests are in `.local/evidence/bundles/`. A retained second local copy is `/Users/jackson/Documents/PDX/evidence/native-2026-09-18/`, outside scratch/worktree/temp paths. Archive and manifest hashes were compared after copying; a representative owner capsule was restored from that second copy and its written files verified.

The accepted SDK-479 policy calls for manifests and bulk release assets in a private evidence repository plus a second local copy. The connected GitHub account can see `pdx-foundry/native` and `pdx-foundry/atlas`, both public. No accessible `pdx-foundry/pdx-evidence` destination was established; organization listing returned only those two repositories. This migration creates no remote and publishes no mixed evidence. **Private remote preservation remains outstanding.** SDK-486 stays open; local success does not satisfy its remote completion gate.

## Source inventory

[source-inventory.json](source-inventory.json) gives each bundle's identity, full archive hash, original root, portable archive root, Git HEAD, omissions, size and file-manifest location. Per-file manifests preserve original absolute paths, relative paths, sizes/hashes and links. Original license notices were retained; copied game/native observations and source archives do not acquire a blanket SDK license.

| Bundle | Imported source |
| --- | --- |
| sdk-testing | `pdx-sdk/packages/sdk-testing/prototype` and `.scratch/sdk-testing`, including failed launch/input/bridge/resource/time attempts |
| typed-extraction | Complete `typed-pdxscript-prototype/spikes/config-information-extraction`, including reference and early-observation sources/runs/baseline |
| atlas-discovery | Engine-registry worktree's `packages/sdk/prototype`, including council-agenda, command documentation and sibling native helpers |
| atlas-ownership | Registry-ownership worktree's prototype tree, preserving the accepted 262-file capsule and its dependencies |
| atlas-command-grammar | Command-grammar worktree's prototype tree and sibling dependencies |
| atlas-numeric-grammar | Numeric-grammar worktree's prototype tree and sibling dependencies |
| external-research | Standalone September 15 research, original `8b6b` worktree research, Atlas planning/prototype/tradition research docs and temporary handoff |
| apple-silicon-baseline | Original SDK-447 source/raw archive from the `7d2d` SDK worktree |
| source-git | Selected SDK/Typed branch bundles plus exact named Windows/shared-source/specification snapshots |
| linear-records | Native project metadata/issue listing, Atlas map, accepted native decisions, testing maps/documents/comments, downloaded review and native archive assets |
| linear-supplement | Additional lifecycle, recursive event, locator, scripts/stockpiles/shared-suite/harness resolutions and archives; duplicate assets point to linear-records |
| sdk-515-loader-entry | Initial Native debugger-worker candidate: two retained four-control batches and source/tool identities |
| sdk-517-observations | Rust-owned candidate observations: five retained batches, generated worker protocol, failure controls and replay artifacts; see [result](candidate-observations.md) |
| sdk-515-loader-entry-review | Final candidate rerun after PR review: strengthened joins and raw preservation hashes; see [result](loader-entry-worker.md) |

No `sdk-atlas` directory exists in the supplied Developer directory; the verified source is `pdx-atlas`. Its local planning/glossary and consumer conclusions stay Atlas-owned. Full mixed historical capsules are privately retained here to keep native provenance and replay intact, not promoted into a Native rule database.

## Dependencies and recoverability

- Sibling prototype helper trees are included in each Atlas capsule import. Offline dispatch/scheduler/shared-reader replay passes after relocation. Native capture still needs exact host/game/toolchain/debugger prerequisites and original-path adjustments.
- M45-observe's 85,044,680-byte ARM64 thin executable is actually retained privately at the spike's `reference-observation-prototype/evidence/stellaris-arm64.local` and discovery helpers. Its hash matches the recorded slice. This is not a complete runnable game installation. The current installed universal executable was read and verified as M45-observe during migration; it was not copied or launched.
- M45-old, W45 and W446 complete installations/executables are not established as retained by this import. Windows raw archives contain game-produced source fixtures/settings, native sources/binaries/vendor materials and successful/failed captures, but no `stellaris.exe`. Runtime/DLC/library inputs remain external. Exact content manifests do not themselves preserve those content bytes.
- Older Mac probes reference Mythos, ordinary template profiles, source saves, installed SDK distributions and helper paths. Captured private profiles/source snapshots preserve parts of those inputs; the import does not certify closure for every historical fresh run. Use each original script/report's dependency list before attempting capture. Original archives intentionally exclude some saves/account/cache/full dependency material.
- Two initial signed-asset requests returned HTTP 401. Native event-selection raw bytes are retained through the local `native-bridge-probe` archive even though that old remote locator was unavailable. The shared harness asset was recovered with a fresh SDK-446 attachment URL in `linear-supplement`; its earlier unsuccessful retrieval remains recorded. No gap is silently converted into game uncertainty.
- The temporary spike handoff exists and is retained as `external-research/handoff.md`. Standalone/worktree native research is retained locally. Historical Git bundles contain complete histories for selected refs; untracked working evidence is separately archived.

Bundle creation excludes `.git` directory contents, `node_modules`, `__pycache__` and `.DS_Store`. Selected Git histories are preserved in `source-git`. External Node dependencies must be reinstalled or supplied from recorded lock/source inputs where needed. Raw capture/manifests and experiment sources are preserved; derived cache binaries elsewhere can remain in historical archives. Restoration skips symlinks while retaining their original targets in the manifest.

## Canonical location and cleanup

Native-only discovery pointers in Atlas and source-root forwarding documents now direct future agents here. Historical experiment sources, capsules, replay paths and Git evidence remain unchanged. Consumer-specific conclusions stay in Atlas. Frozen source/report files embedded in capsules remain historical authorities for the original run; changing them would break their hashes.

No original raw evidence was deleted. Cleanup of scratch/worktree/temp sources supporting retained claims must wait for verified private remote preservation and dependency review. Keep superseded and failed evidence for retained qualifications/corrections. A future published capability must cite an admitted adapter/test and new qualification evidence, rather than treating migration or a candidate address as qualification.
