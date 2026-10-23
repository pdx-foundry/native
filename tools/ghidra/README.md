# Ghidra export

These tools make the facts directory of one build, `builds/<slice prefix>/`, from a Ghidra analysis
of its executable. The [Ghidra front-end design](../../docs/design/ghidra-front-end.md) describes the
schema, the completeness conditions and the whole-block contract. Ghidra is never a dependency of a
library call: `cargo test` reads only the committed files.

| File | What it does |
| --- | --- |
| `Export.java` | The Ghidra script. It walks each member reader over the decompiler's basic blocks and writes typed facts: calls, stores, switch labels, loops. It never writes decompiled text. |
| `export.py` | The driver. It runs `Export.java` in rounds, from each registry's root readers to every reader that they reach, and writes the raw export. |
| `rows.py` | The classification of a walk's events into field rows and dispatch facts. |
| `normalize.py` | It turns the raw export into `readers.json` and `provenance.json`, with the completeness of each reader, and prints the count of registries whose reader tree is complete. |
| `schema.py` | The schema check of a build directory. |
| `conformance/` | The expectations that an export of each build must meet (`m452.json`) and their check (`check.py`). |
| `examples/ghidra-inputs` (Rust) | Writes `tables.json` (tokens, scope types, reader offsets, manual exceptions) and the root readers of each registry, from Native's own analysis. |

## Setup

- Ghidra 12.1.4 from the official release, with the macOS ARM64 decompiler built by
  `support/gradle/gradlew buildNatives`. `export.py` finds it in `GHIDRA_INSTALL_DIR`, by default
  `~/Applications/ghidra_12.1.4_PUBLIC`.
- OpenJDK 25 (Homebrew `openjdk@25`), in `JAVA_HOME`.
- An 8 GB heap (`GHIDRA_HEADLESS_MAXMEM=8G`, the default of `export.py`).
- A private Ghidra workspace outside every repository, which holds the analysed project, the
  executable's slice and the raw exports. Ghidra refuses a project path with a folder that starts
  with `.`, so the workspace cannot be under `.local/`. Never commit or copy its contents: the
  committed files hold typed rows, addresses and names only.

## Procedure for a build

Analysis is once for each build. It takes about 18 minutes and makes a 1.4 GB project. The local
project's cache key is the slice's SHA-256 and the Ghidra version: analyse again when either
changes.

```sh
WORKSPACE=<private Ghidra workspace>
BUILD=<first 8 hex digits of the slice SHA-256>

# 1. Once for each build: the ARM64 slice, and a project with Ghidra's default analysis.
lipo -thin arm64 "$STELLARIS_PATH/stellaris.app/Contents/MacOS/stellaris" -output "$WORKSPACE/bin/<program>"
"$GHIDRA_INSTALL_DIR/support/analyzeHeadless" "$WORKSPACE/projects" <project> -import "$WORKSPACE/bin/<program>"

# 2. Native's inputs: tables.json and the root readers.
cargo run --release --example ghidra-inputs -- "$STELLARIS_PATH" "$WORKSPACE/exports/$BUILD/tables.json" \
    "$WORKSPACE/exports/$BUILD/roots.json"

# 3. The export: about 100 seconds for M452. The project opens read-only.
python3 tools/ghidra/export.py --project "$WORKSPACE/projects/<project>.gpr" --program <program> \
    --tables "$WORKSPACE/exports/$BUILD/tables.json" --roots "$WORKSPACE/exports/$BUILD/roots.json" \
    --conformance tools/ghidra/conformance/<build>.json --out "$WORKSPACE/exports/$BUILD/raw"

# 4. Normalize into the build directory, then check it.
mkdir -p builds/$BUILD && cp "$WORKSPACE/exports/$BUILD/tables.json" builds/$BUILD/
python3 tools/ghidra/normalize.py "$WORKSPACE/exports/$BUILD/raw/export.json" builds/$BUILD/tables.json builds/$BUILD
python3 tools/ghidra/schema.py builds/$BUILD
python3 tools/ghidra/conformance/check.py tools/ghidra/conformance/<build>.json builds/$BUILD/readers.json
```

A new build also needs a conformance file. Copy the expectations of the previous build, and
check by hand each reader that does not meet them before you change an expectation.

A new export for the same build replaces `readers.json` and `provenance.json` in one change. Its
review artifacts are the conformance check and the static measure
(`tools/measure/m5-static.sh`), not the file diff. Export again when a script of the export
changes: `provenance.json` records the SHA-256 of `Export.java`, `export.py`, `rows.py` and
`normalize.py`.

## Tests

```sh
cd tools/ghidra && python3 -m unittest test_normalize
cd tools/ghidra/conformance && python3 -m unittest test_check
```
