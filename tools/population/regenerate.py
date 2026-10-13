#!/usr/bin/env python3
"""Merge origin/main and regenerate every tracked population baseline.

Run from the repository root with STELLARIS_PATH set. A merge conflict in a generated baseline is
resolved by the regeneration; any other conflict stops the script before it generates anything.
The script stages the baselines and commits nothing, so the changed answers can be reviewed first.
"""
import os
from pathlib import Path, PurePosixPath
import subprocess
import sys
import time

BASELINES = PurePosixPath("tests/population/m452")
GENERATED = ("registry-field-sweep", "registry-sweep", "command-population")


def is_generated(path):
    """Whether a repository path is part of a generated baseline, in the split or the single-file
    layout. A branch from before the split still changes the single file."""
    path = PurePosixPath(path)
    return any(path == BASELINES / f"{name}.json" or BASELINES / name in path.parents
               for name in GENERATED)


def git(*arguments):
    return subprocess.run(["git", *arguments], check=True, capture_output=True, text=True).stdout


def merge_main():
    git("fetch", "origin", "main")
    if subprocess.run(["git", "merge", "--no-commit", "origin/main"]).returncode == 0:
        return
    unmerged = git("diff", "--name-only", "--diff-filter=U").split()
    if not unmerged:
        sys.exit("git merge failed without conflicts; nothing was regenerated")
    blocking = [path for path in unmerged if not is_generated(path)]
    if blocking:
        sys.exit("resolve these conflicts, then run this script again:\n  " + "\n  ".join(blocking))


def example(name, *arguments):
    subprocess.run(["cargo", "run", "--release", "--example", name, "--", *arguments], check=True)


def regenerate(installation):
    for name in GENERATED:
        git("rm", "-q", "-f", "--ignore-unmatch", str(BASELINES / f"{name}.json"))
    example("registry-field-sweep", "--baseline", str(BASELINES / "registry-field-sweep"), installation)
    example("command-population", "--baseline", str(BASELINES / "command-population"))
    answers = Path(".local/population") / f"sweep-answers-{time.strftime('%Y%m%d-%H%M%S')}"
    answers.parent.mkdir(parents=True, exist_ok=True)
    example("registry-sweep", "--record", str(answers), installation)
    example("registry-sweep", "--baseline", str(BASELINES / "registry-sweep"), str(answers))
    git("add", "-A", *(str(BASELINES / name) for name in GENERATED))


def main():
    installation = os.environ.get("STELLARIS_PATH")
    if not installation:
        sys.exit("set STELLARIS_PATH to the installed game")
    merge_main()
    regenerate(installation)
    merging = subprocess.run(["git", "rev-parse", "-q", "--verify", "MERGE_HEAD"],
                             capture_output=True).returncode == 0
    print("Baselines regenerated and staged. Review the changed answers and the counts on the "
          "knowledge pages, then " + ("run `git commit` to finish the merge." if merging else "commit."))


if __name__ == "__main__":
    main()
