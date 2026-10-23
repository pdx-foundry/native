#!/usr/bin/env python3
"""Measure one Atlas static run for Milestone 5 and compare it with earlier runs.

Usage: python3 -I m5.py measure RUN [--gate ACCEPTED_RUN] [--baseline FROZEN_RUN]
       python3 -I m5.py accept RUN --accepted ACCEPTED_RUN

RUN holds `inputs.json` and the ledger's `reports/`. The measure counts the Atlas-owned claims
(owners `engine_fact` and `content_derived`) of the config files under `common/` and `events/`.
The gate fails when a claim that the accepted run covers is not covered. `measure` writes
`measure.json`, `covered.txt` (the covered measure claims) and `gaps.md` into RUN and exits 1 when
the gate fails. `accept` exits 1 unless RUN passed its gate against the current accepted run.
"""

import argparse
from collections import Counter, defaultdict
import json
from pathlib import Path
import sys

ATLAS_OWNED = ("engine_fact", "content_derived")
MEASURED_FILES = ("common/", "events/")

# A comparison needs the same Atlas, config, recording and build; only Native may differ.
SHARED_INPUTS = ("atlas_commit", "config_revision", "recording_sha256", "config_sha256", "target",
                 "content_sha256")


def measure(ledger, coverage):
    """The covered and open claims of the measure, with the reason each open claim has."""
    claims = {claim["id"]: claim for claim in ledger["claims"]}
    covered = set()
    open_claims = {}

    for record in coverage["claims"]:
        claim = claims[record["claim"]]

        if claim["owner"] not in ATLAS_OWNED or not claim["file"].startswith(MEASURED_FILES):
            continue

        if record["covered"]:
            covered.add(claim["id"])
            continue

        reason = "no answer" if record["reason"].startswith("No qualified") else "Native gap"
        open_claims[claim["id"]] = (claim, reason)

    return covered, open_claims


def compare(reference, current):
    """The claims `current` covers that `reference` does not (gained), and the reverse (lost)."""
    return {"gained": sorted(current - reference), "lost": sorted(reference - current)}


def mismatched_inputs(reference, current):
    """The shared inputs whose values differ between two runs."""
    return [key for key in SHARED_INPUTS if reference.get(key) != current.get(key)]


def gaps_table(covered, open_claims):
    """The open-gap page of the measure: totals, then open claims by file and type."""
    total = len(covered) + len(open_claims)
    by_reason = Counter(reason for _, reason in open_claims.values())
    by_type = defaultdict(Counter)

    for claim, reason in open_claims.values():
        type_name = (claim["subject"] or ["-"])[0]
        by_type[(claim["file"], type_name)][f"{claim['property']} ({reason})"] += 1

    reasons = ", ".join(f"{count} {reason}" for reason, count in by_reason.most_common())
    lines = [
        "# Milestone 5 open gaps",
        "",
        f"- Coverage: {len(covered)} / {total} ({100 * len(covered) / total:.2f}%)",
        f"- Open gaps: {len(open_claims)} ({reasons})",
        "",
        "| Open | File | Type | Properties |",
        "| ---: | --- | --- | --- |",
    ]

    for (file, type_name), properties in sorted(by_type.items(), key=lambda item: -sum(item[1].values())):
        detail = ", ".join(f"{name} {count}" for name, count in properties.most_common())
        lines.append(f"| {sum(properties.values())} | {file} | {type_name} | {detail} |")

    return "\n".join(lines) + "\n"


def read_run(run):
    """The inputs, ledger and coverage of a run directory."""
    reports = run / "reports"
    inputs = json.loads((run / "inputs.json").read_text())
    ledger = json.loads((reports / "ledger.json").read_text())
    coverage = json.loads((reports / "coverage.json").read_text())

    inputs["config_sha256"] = coverage["config_sha256"]
    inputs["target"] = coverage["target"]
    inputs["content_sha256"] = coverage["content"]["sha256"]

    return inputs, ledger, coverage


def comparison(label, reference_run, inputs, covered):
    """Compares `covered` with the accepted claims of `reference_run`; refuses other inputs."""
    reference = json.loads((reference_run / "measure.json").read_text())
    differing = mismatched_inputs(reference["inputs"], inputs)

    if differing:
        sys.exit(f"The {label} run {reference_run} has other inputs: {', '.join(differing)}")

    reference_covered = set((reference_run / "covered.txt").read_text().split())
    delta = compare(reference_covered, covered)

    return {"run": reference_run.resolve().name, "gained": len(delta["gained"]),
            "lost": len(delta["lost"]), "lost_claims": delta["lost"]}


def acceptance_refusal(measure_result, accepted_run_name):
    """Why a run cannot become the accepted result, or None when it can."""
    gate = measure_result.get("gate")

    if gate is None:
        return "it has no gate"

    if gate["run"] != accepted_run_name:
        return f"its gate compared it with {gate['run']}, and the accepted run is {accepted_run_name}"

    if not gate["passed"]:
        return f"it lost {gate['lost']} claims"

    return None


def accept(arguments):
    """Exits 1 when the run cannot become the accepted result."""
    measure_result = json.loads((arguments.run / "measure.json").read_text())
    refusal = acceptance_refusal(measure_result, arguments.accepted.resolve().name)

    if refusal:
        sys.exit(f"{arguments.run.name} cannot be accepted: {refusal}")


def main():
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command", required=True)
    measuring = commands.add_parser("measure")
    measuring.add_argument("run", type=Path)
    measuring.add_argument("--gate", type=Path, help="the accepted run; lost must be 0")
    measuring.add_argument("--baseline", type=Path, help="the frozen baseline run")
    accepting = commands.add_parser("accept")
    accepting.add_argument("run", type=Path)
    accepting.add_argument("--accepted", type=Path, required=True)
    arguments = parser.parse_args()

    if arguments.command == "accept":
        accept(arguments)
    else:
        report(arguments)


def report(arguments):
    """Measures the run, compares it, and writes its results."""
    inputs, ledger, coverage = read_run(arguments.run)
    covered, open_claims = measure(ledger, coverage)
    atlas_owned = coverage["totals"]["atlas_owned"]
    result = {
        "inputs": inputs,
        "covered": len(covered),
        "total": len(covered) + len(open_claims),
        "atlas_owned": {"covered": atlas_owned["covered"], "total": atlas_owned["total"]},
    }

    print(f"{arguments.run.name}: {inputs['native']}")
    print(f"  measure {result['covered']} / {result['total']}; "
          f"Atlas-owned {atlas_owned['covered']} / {atlas_owned['total']}")

    if arguments.gate:
        gate = comparison("gate", arguments.gate, inputs, covered)
        gate["passed"] = gate["lost"] == 0
        result["gate"] = gate
        print(f"  gate against {gate['run']}: gained {gate['gained']}, lost {gate['lost']}")

    if arguments.baseline:
        cumulative = comparison("baseline", arguments.baseline, inputs, covered)
        result["cumulative"] = cumulative
        print(f"  cumulative against {cumulative['run']}: "
              f"gained {cumulative['gained']}, lost {cumulative['lost']}")

    (arguments.run / "covered.txt").write_text("".join(f"{claim}\n" for claim in sorted(covered)))
    (arguments.run / "gaps.md").write_text(gaps_table(covered, open_claims))
    (arguments.run / "measure.json").write_text(json.dumps(result, indent=2) + "\n")

    if not result.get("gate", {"passed": True})["passed"]:
        print("  the gate failed: lost claims are in measure.json")
        sys.exit(1)


if __name__ == "__main__":
    main()
