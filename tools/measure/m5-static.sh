#!/bin/bash
# Milestone 5 static measure: build Atlas against one Native source, take a static-mode snapshot,
# run the ledger and compare the result with the accepted result (the gate) and the frozen
# baseline (the cumulative progress). See docs/design/ghidra-front-end.md#measure.
#
#   m5-static.sh run NAME NATIVE   NATIVE is a commit of this repository or a checkout directory
#   m5-static.sh freeze NAME       the run becomes the frozen baseline and the accepted result
#   m5-static.sh accept NAME       the run becomes the accepted result; it must have passed its
#                                  gate against the current accepted result
#
# Inputs (environment, with defaults beside the main checkout of this repository):
#   STELLARIS_PATH  the installed game (required)
#   ATLAS           an Atlas checkout, read only           (../atlas)
#   ATLAS_COMMIT    the Atlas commit to build              (HEAD of $ATLAS)
#   CWT_CONFIG      the cwtools config directory           (../cwtools-stellaris-config/config)
#   M5_DIR          the results directory                  (.local/m5)
#   M5_RECORDING    the live recording of the same build   ($M5_DIR/recording)
set -euo pipefail

usage() {
  sed -n '6,9p' "$0" >&2
  exit 64
}

tool_dir=$(cd "$(dirname "$0")" && pwd)
repository=$(git -C "$tool_dir" rev-parse --show-toplevel)
main_checkout=$(dirname "$(git -C "$tool_dir" rev-parse --path-format=absolute --git-common-dir)")
workspace=$(dirname "$main_checkout")

m5=${M5_DIR:-$main_checkout/.local/m5}
atlas=${ATLAS:-$workspace/atlas}
config=${CWT_CONFIG:-$workspace/cwtools-stellaris-config/config}
recording=${M5_RECORDING:-$m5/recording}

native_remote='https://github.com/pdx-foundry/native.git'
binary="$m5/build/target/release/pdx-atlas"

# Replaces `directory` with the tree of `commit` from `repository`. `tar -m` gives every file the
# current time, so Cargo rebuilds an older commit after a newer one.
extract() {
  local repository=$1 commit=$2 directory=$3

  rm -rf "$directory"
  mkdir -p "$directory"
  git -C "$repository" archive "$commit" | tar -x -m -C "$directory"
}

# Prints the commit of the checkout `$1`, marked when tracked files have uncommitted changes.
revision() {
  local directory=$1 commit

  commit=$(git -C "$directory" rev-parse HEAD)

  if [ -n "$(git -C "$directory" status --porcelain --untracked-files=no)" ]; then
    commit="$commit+uncommitted"
  fi

  echo "$commit"
}

# Sets `native_directory` to the Native source for `native` and writes its identity to
# `$run/native.txt`. The build steps run outside a command substitution: Bash 3.2 does not apply
# `set -e` inside one, so a failed step there would let a cached binary run.
native_source() {
  local native=$1 run=$2

  if [ -d "$native" ]; then
    native_directory=$(cd "$native" && pwd)
    echo "checkout $native_directory $(revision "$native_directory")" > "$run/native.txt"
    return
  fi

  local commit

  commit=$(git -C "$repository" rev-parse --verify "$native^{commit}")
  extract "$repository" "$commit" "$m5/build/native"
  native_directory="$m5/build/native"
  echo "commit $commit" > "$run/native.txt"
}

# Builds Atlas at `$atlas_commit` against the Native source `$1` into `$binary`.
build_atlas() {
  local native_directory=$1
  local atlas_directory="$m5/build/atlas"

  if [ "$(cat "$atlas_directory/.commit" 2>/dev/null)" != "$atlas_commit" ]; then
    extract "$atlas" "$atlas_commit" "$atlas_directory"
    echo "$atlas_commit" > "$atlas_directory/.commit"
  fi

  (
    cd "$atlas_directory"
    cargo build --release --quiet \
      --target-dir "$m5/build/target" \
      --config "patch.'$native_remote'.pdx-native.path='$native_directory'"
  )
}

# A digest of every file in the recording, so a comparison can refuse a different recording.
recording_digest() {
  (cd "$recording" && find . -type f ! -name .DS_Store -print0 | LC_ALL=C sort -z | xargs -0 shasum -a 256) |
    shasum -a 256 | cut -d' ' -f1
}

run() {
  local name=$1 native=$2
  local run="$m5/runs/$name"

  : "${STELLARIS_PATH:?STELLARIS_PATH names the installed game}"
  [ -d "$recording" ] || { echo "No recording at $recording; set M5_RECORDING" >&2; exit 1; }
  [ -d "$config" ] || { echo "No cwtools config at $config; set CWT_CONFIG" >&2; exit 1; }
  [ ! -e "$run" ] || { echo "$run exists; choose another name" >&2; exit 1; }

  # One run at a time: runs share the build directories, and a static snapshot is memory-heavy.
  mkdir -p "$m5/build"
  mkdir "$m5/build/lock" 2>/dev/null || { echo "Another run holds $m5/build/lock" >&2; exit 1; }
  trap 'rmdir "$m5/build/lock"' EXIT
  mkdir -p "$run"

  atlas_commit=${ATLAS_COMMIT:-$(git -C "$atlas" rev-parse HEAD)}
  atlas_commit=$(git -C "$atlas" rev-parse --verify "$atlas_commit^{commit}")

  native_source "$native" "$run"
  rm -f "$binary"
  build_atlas "$native_directory"

  cat > "$run/inputs.json" <<EOF
{
  "native": "$(cat "$run/native.txt")",
  "atlas_commit": "$atlas_commit",
  "config_revision": "$(revision "$config")",
  "recording_sha256": "$(recording_digest)"
}
EOF

  echo "Snapshot of $(cat "$run/native.txt")" >&2
  if ! "$binary" snapshot --static "$STELLARIS_PATH" --observations "$recording" "$run/rules.json" \
    > "$run/snapshot.log" 2>&1; then
    echo "The snapshot failed; see $run/snapshot.log" >&2
    exit 1
  fi

  local baseline=()

  if [ -d "$m5/accepted" ]; then
    baseline=(--baseline "$m5/accepted/reports/covered")
  fi

  echo "Ledger" >&2
  local status=0

  (cd "$m5/build/atlas" && "$binary" ledger --config "$config" --snapshot "$run/rules.json" \
    --game-content "$STELLARIS_PATH" --output "$run/reports" --dashboard "$run/dashboard" \
    ${baseline[@]+"${baseline[@]}"}) > "$run/ledger.log" 2>&1 || status=$?

  # Exit 2 writes the reports and names the config's source diagnostics.
  if [ "$status" -ne 0 ] && [ "$status" -ne 2 ]; then
    echo "The ledger failed; see $run/ledger.log" >&2
    exit 1
  fi

  local references=()

  [ -d "$m5/accepted" ] && references+=(--gate "$m5/accepted")
  [ -d "$m5/baseline" ] && references+=(--baseline "$m5/baseline")
  python3 -I "$tool_dir/m5.py" measure "$run" ${references[@]+"${references[@]}"}
}

point() {
  local link=$1 name=$2

  [ -f "$m5/runs/$name/measure.json" ] || { echo "No measured run $name in $m5/runs" >&2; exit 1; }
  ln -sfn "runs/$name" "$m5/$link"
  echo "$link: $name" >&2
}

accept() {
  local name=$1

  [ -f "$m5/runs/$name/measure.json" ] || { echo "No measured run $name in $m5/runs" >&2; exit 1; }
  python3 -I "$tool_dir/m5.py" accept "$m5/runs/$name" --accepted "$m5/accepted"
  point accepted "$name"
}

case "${1:-}" in
  run) [ $# -eq 3 ] || usage; run "$2" "$3" ;;
  freeze) [ $# -eq 2 ] || usage; point baseline "$2"; point accepted "$2" ;;
  accept) [ $# -eq 2 ] || usage; accept "$2" ;;
  *) usage ;;
esac
