# Evidence preservation

The [development policy](../development-policy.md) sets the rule: preserve unique prototype and
probe knowledge, and treat routine captures and build outputs as disposable.

The canonical authored Native knowledge is in this repository's tracked `docs/`. Private identified bundles and full file manifests are in `.local/evidence/bundles/`. A retained second local copy is `/Users/jackson/Documents/PDX/evidence/native-2026-09-18/`, outside scratch/worktree/temp paths. Archive and manifest hashes were compared after copying; a representative owner capsule was restored from that second copy and its written files verified.

There is no private remote copy; SDK-486 owns remote preservation.

## Bundles

[source-inventory.json](source-inventory.json) gives each bundle's identity, archive hash, original
root, portable root, Git HEAD, omissions, size and manifest location. Per-file manifests keep the
original absolute paths, sizes, hashes and links. [Retrieval](retrieval.md) says how to verify and
restore a bundle.

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
| sdk-517-observations | Rust-owned candidate observations: five retained batches, generated worker protocol, failure controls and replay artifacts |
| sdk-515-loader-entry-review | Final debugger-worker trial with raw preservation hashes; see [retrieval](retrieval.md#debugger-worker-trial-bundle) |

Frozen source and report files inside capsules are the authority for their original run;
changing them breaks their hashes. Consumer-specific conclusions stay in Atlas.

## What a bundle does not hold

- No complete game installation. The M45-observe ARM64 slice (85,044,680 bytes) is the only
  retained executable, also at `.local/executables/stellaris-m45-observe-arm64`; its hash is the
  slice identity in [targets](targets.md). M45-old, W45 and W446 executables, DLC and runtime
  libraries are external.
- Older Mac probes reference Mythos, template profiles, source saves, SDK distributions and helper
  paths that are only partly captured. Use each original script's dependency list before a fresh
  capture.
- Bundles exclude `.git` contents, `node_modules`, `__pycache__` and `.DS_Store`; `source-git`
  holds the selected Git histories. Restoration skips symlinks and keeps their targets in the
  manifest.

## Other retained copies

Every bundle passed `tools/knowledge_bundles.py`, and each archive and manifest has a byte-identical copy
in `~/Documents/PDX/evidence/native-2026-09-18/`.

`.local/preserved-development/` holds source, notes and small observations that are absent from
the bundle manifests, under their former `.local` paths. These 1,780 files have a verified
second copy in `native-2026-09-18/simplification-development-notes.tar.gz`.

The Atlas caller before migration, including its old freeze and synthetic files, is preserved in
`native-2026-09-18/atlas-native-consumer-before-simplification.tar.gz`.

The config-test spike experiment is in Atlas at
`/Users/jackson/Developer/pdx-foundry/atlas/docs/prototypes/config-test-spike/` (ignored by Atlas
Git). A checked copy is `.local/evidence/config-test-spike-2026-09-28/`: the experiment archive,
its inventory and verification, the config inputs and `native-source.bundle`. Atlas
`preservation.json` records their paths and hashes. Absolute paths in the private runner need
adjustment after relocation.

Before you clean a source tree, verify that a usable copy keeps its unique knowledge and
dependencies.
