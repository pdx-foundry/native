# Local retrieval of the prototype bundles

Run these commands from `/Users/jackson/Developer/pdx-native`. Python 3 standard library is enough for bundle verification and the selected Atlas replays. No game launch is needed.

```sh
python3 tools/evidence.py
python3 tools/evidence.py atlas-ownership --restore
cd .local/evidence/restored/atlas-ownership/prototype/registry-ownership
PYTHONDONTWRITEBYTECODE=1 python3 replay.py
PYTHONDONTWRITEBYTECODE=1 python3 atlas_demo.py
```

Each `--restore` requires a new destination and verifies compressed archive identity, every regular-file hash/size and every retained symlink before restoring. Restores hash the written regular files again. Links remain recorded in the manifest but are skipped during restoration, so old profile links cannot redirect writes to the user's machine. Sealed archives are never used as working directories. If replay changes outputs, retain them outside the bundle and restore a new working copy for the next comparison.

## Search and locate

Bundle IDs are the local evidence IDs. `.local/evidence/bundles/<id>.manifest.json` maps every original absolute path to a portable archive-relative path, SHA-256, size or symlink target. [source-inventory.json](source-inventory.json) is the tracked compact inventory; original manifests/capsules are inside their bundles. Search restored material and exported Linear text with `rg`:

```sh
rg -n 'disposal|unknown|CEventScope|LoadFile' docs .local/evidence/restored
rg -n 'accepted|Decisions so far|maintenance' .local/evidence/restored/linear-records/linear
```

For Linear material, restore `linear-records`. `linear/SDK-483-comments.json`, for example, retains accepted resolution text with source comment IDs and timestamps. `linear/doc-<slug>.json` retains document ID/content/URL and retrieval time. API envelopes for attachments include unsuccessful attempts; actual downloaded bytes and their normalized source URLs/hashes/status are indexed in `assets/retrieval.json`. Signed URLs in private historical responses are not durable locators.

## Offline command map

| Bundle | Directory below its restore root | Command |
| --- | --- | --- |
| atlas-ownership | `prototype/registry-ownership` | `python3 replay.py`, `python3 atlas_demo.py` |
| atlas-discovery | `prototype/engine-registry-discovery` | `python3 replay.py` |
| atlas-discovery | `prototype/council-agenda-reconstruction` | `python3 replay.py`, `python3 atlas_demo.py` |
| atlas-discovery | `prototype/engine-command-discovery` | `python3 replay.py` |
| atlas-command-grammar | `prototype/command-grammars` | `python3 verify.py`, `python3 consumer_demo.py` |
| atlas-numeric-grammar | `prototype/numeric-grammar` | `python3 verify.py`, `python3 consumer_demo.py` |

These replay retained instructions, traces, joins, normalizations and synthetic missing-evidence controls. They do not reproduce game execution. Sibling native helper directories are included to preserve original relative imports.

`typed-extraction` contains the full mixed extraction spike for historical reference. Reference `run.py` needs the exact game installation/Xcode even though it does not launch the game. Early-observation **`replay.py` launches games**. Do not run it for an offline check; read the retained manifests, traces and parent reaping records in the restored spike.

`apple-silicon-baseline` includes the original `apple-silicon-native-evidence.tar.gz`. Linear asset files retain UUID filenames; inspect format with `file`, then list/extract in a new private working directory. W45 asset is `3abce4f4-ee3d-4a66-bb4f-5ef058a2fb66`; W446 asset is `c7ff3152-650d-4148-bb86-a2b7ac72e306`. Their source documents give archive hashes and original layouts.

`source-git/git/sdk.bundle` and `typed.bundle` preserve selected original branch histories without rewriting source repositories. `git bundle verify <path>` works in an empty Git repository; use `git clone <bundle> <new-directory>` for a separate historical source checkout. Standalone `.tar` snapshots preserve the named Windows adapter and draft specification commits even where no retained branch points at them. Working-tree/untracked evidence is preserved in the other bundles, not assumed present in Git.

## Fresh capture prerequisites

SDK-515's `sdk-515-loader-entry-review` bundle retains the final candidate debugger-worker
trial. Its initial batches remain in `sdk-515-loader-entry`.
Use its [result and restore instructions](loader-entry-worker.md) for offline verification.
The separate `tools/loader-entry-trial/run.py` command **launches games** and requires the
exact retained installation.

Fresh captures require the **particular** target/architecture, compatible save or parser fixture, declared installed content and DLC, OS/toolchain/debugger access, and no conflicting live game. Native capture scripts retain original absolute installation/profile/helper paths. Retarget working copies and record the changes; preserve hash gates and qualification controls. Do not silently substitute another binary or mock source. Missing installations, old source saves, Mythos/dependency content and Windows host access limit fresh reproduction even when offline replay succeeds.
