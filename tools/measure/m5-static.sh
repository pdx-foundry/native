#!/bin/bash
# Milestone 5 static measure: build Atlas against one Native source, take a static-mode snapshot,
# run the ledger and compare the result with the accepted result (the gate) and the frozen
# baseline (the cumulative progress). See docs/design/ghidra-front-end.md#measure.
#
#   m5-static.sh run NAME NATIVE   NATIVE is a commit of this repository or a checkout directory
#   m5-static.sh run --new-pin NAME NATIVE
#                                  measure with no comparison, for new shared inputs at a pin move
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
  sed -n '6,11p' "$0" >&2
  exit 64
}

tool_dir=$(cd "$(dirname "$0")" && pwd)
repository=$(git -C "$tool_dir" rev-parse --show-toplevel)
main_checkout=$(dirname "$(git -C "$tool_dir" rev-parse --path-format=absolute --git-common-dir)")
workspace=$(dirname "$main_checkout")

# Prints `$1` as an absolute path: the build changes directory before it uses these paths.
absolute() {
  case $1 in
    /*) echo "$1" ;;
    *) echo "$PWD/$1" ;;
  esac
}

m5=$(absolute "${M5_DIR:-$main_checkout/.local/m5}")
atlas=$(absolute "${ATLAS:-$workspace/atlas}")
config=$(absolute "${CWT_CONFIG:-$workspace/cwtools-stellaris-config/config}")
recording=$(absolute "${M5_RECORDING:-$m5/recording}")

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
# Callers use it in a command substitution, where `set -e` does not apply, so each step returns its
# failure.
revision() {
  local directory=$1 commit changes

  commit=$(git -C "$directory" rev-parse HEAD) || return 1
  changes=$(git -C "$directory" status --porcelain --untracked-files=no) || return 1

  if [ -n "$changes" ]; then
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
    local native_revision

    native_directory=$(cd "$native" && pwd)
    native_revision=$(revision "$native_directory")
    echo "checkout $native_directory $native_revision" > "$run/native.txt"
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

# Refuses a run name that is not one plain directory name: a run's name is its identity in the
# gate records.
check_name() {
  case $1 in
    '' | . | .. | */*) echo "A run name is one directory name, not '$1'" >&2; exit 1 ;;
  esac
}

# Measures the run NAME of NATIVE. With `new_pin` set to `yes`, it compares with no earlier run.
run() {
  local new_pin=$1 name=$2 native=$3
  local run="$m5/runs/$name"

  check_name "$name"

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

  local config_revision digest

  config_revision=$(revision "$config")
  digest=$(recording_digest)
  python3 -I -c 'import json, sys
keys = ("native", "atlas_commit", "config_revision", "recording_sha256")
print(json.dumps(dict(zip(keys, sys.argv[1:])), indent=2))' \
    "$(cat "$run/native.txt")" "$atlas_commit" "$config_revision" "$digest" > "$run/inputs.json"

  echo "Snapshot of $(cat "$run/native.txt")" >&2
  if ! "$binary" snapshot --static "$STELLARIS_PATH" --observations "$recording" "$run/rules.json" \
    > "$run/snapshot.log" 2>&1; then
    echo "The snapshot failed; see $run/snapshot.log" >&2
    exit 1
  fi

  local baseline=() references=()

  # A new pin's inputs differ from every earlier run, so it compares with none.
  if [ "$new_pin" = no ]; then
    if [ -d "$m5/accepted" ]; then
      baseline=(--baseline "$m5/accepted/reports/covered")
      references+=(--gate "$m5/accepted")
    fi

    if [ -d "$m5/baseline" ]; then
      references+=(--baseline "$m5/baseline")
    fi
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

  python3 -I "$tool_dir/m5.py" measure "$run" ${references[@]+"${references[@]}"}
}

point() {
  local link=$1 name=$2

  check_name "$name"
  [ -f "$m5/runs/$name/measure.json" ] || { echo "No measured run $name in $m5/runs" >&2; exit 1; }
  ln -sfn "runs/$name" "$m5/$link"
  echo "$link: $name" >&2
}

accept() {
  local name=$1

  check_name "$name"
  [ -f "$m5/runs/$name/measure.json" ] || { echo "No measured run $name in $m5/runs" >&2; exit 1; }
  python3 -I "$tool_dir/m5.py" accept "$m5/runs/$name" --accepted "$m5/accepted"
  point accepted "$name"
}

case "${1:-}" in
  run)
    if [ "${2:-}" = --new-pin ]; then
      [ $# -eq 4 ] || usage
      run yes "$3" "$4"
    else
      [ $# -eq 3 ] || usage
      run no "$2" "$3"
    fi
    ;;
  freeze) [ $# -eq 2 ] || usage; point baseline "$2"; point accepted "$2" ;;
  accept) [ $# -eq 2 ] || usage; accept "$2" ;;
  *) usage ;;
esac
