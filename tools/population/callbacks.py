#!/usr/bin/env python3
"""Count the entry contexts of on_action and game rule answers offline.

Reads the compact parity files that `expected --out` writes, such as
tests/expected/m452/on-actions.json and game-rules.json, and prints the counts that
docs/native/engine-commands.md records.
"""
import argparse
from collections import Counter
import json
from pathlib import Path
import re
import sys


ENTRY = re.compile(r"this=(\S+) root=(\S+) from=\[([^\]]*)\] prev=\[([^\]]*)\]")
LINK_SLOTS = {"SelfLink", "NotSet", "Unresolved"}


def slots(entry):
    """The this, root, from and prev slots of one compact entry context."""
    match = ENTRY.fullmatch(entry)
    if match is None:
        raise ValueError(f"not a compact entry context: {entry}")
    this, root, from_chain, prev_chain = match.groups()
    return {"this": this, "root": root,
            "from": from_chain.split(","), "prev": prev_chain.split(",")}


def is_known(entry):
    """Whether every scope of the entry context is established."""
    scopes = slots(entry)
    return "Unresolved" not in [scopes["this"], scopes["root"], *scopes["from"], *scopes["prev"]]


def has_typed_prev(entry):
    return slots(entry)["prev"][0] not in LINK_SLOTS


def count(answer, with_kinds):
    """Counts by name, and the gap details of the names that have no known context.

    Each name of `answer` maps to its compact entry contexts, or with `with_kinds` (game rules)
    to `[kind, entry contexts]`.
    """
    names = answer["names"]
    gaps_by_name = {}
    subjectless = Counter()
    for kind, subject, detail in answer["gaps"]:
        if subject is None:
            subjectless[detail] += 1
        else:
            gaps_by_name.setdefault(subject["name"], []).append(detail)

    counts = Counter()
    kinds = Counter()
    without_context = Counter()
    only_unresolved = Counter()
    for name, value in names.items():
        kind, entries = value if with_kinds else (None, value)
        known = [entry for entry in entries if is_known(entry)]
        gaps = gaps_by_name.get(name, [])

        kinds[kind] += 1
        counts["names"] += 1
        counts["with_context"] += bool(entries)
        counts["with_known_context"] += bool(known)
        counts["several_known_contexts"] += len(known) > 1
        counts["typed_prev"] += any(has_typed_prev(entry) for entry in known)
        if not entries:
            counts["without_context"] += 1
            without_context.update(gaps)
        elif not known:
            counts["only_unresolved"] += 1
            only_unresolved.update(gaps)

    report = {**counts}
    if with_kinds:
        report["kinds"] = dict(sorted(kinds.items()))
    report["without_context_gaps"] = dict(without_context.most_common())
    report["only_unresolved_gaps"] = dict(only_unresolved.most_common())
    report["answer_gaps"] = dict(subjectless.most_common())
    return report


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("on_actions", type=Path)
    parser.add_argument("game_rules", type=Path)
    arguments = parser.parse_args(argv)
    report = {
        "on_actions": count(json.loads(arguments.on_actions.read_text()), with_kinds=False),
        "game_rules": count(json.loads(arguments.game_rules.read_text()), with_kinds=True),
    }
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main(sys.argv[1:])
