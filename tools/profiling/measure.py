#!/usr/bin/env python3
"""Measure static Native analysis; optionally include the live SDK-559 workloads."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import time


def run(label, command, output, measurements):
    """Keep complete output and a wall-clock result, including failed commands."""
    print(f"Starting {label}", flush=True)
    phases = output / f"{label}-phases"
    phases.mkdir()
    environment = dict(os.environ, NATIVE_PROFILE_DIR=str(phases))
    started = time.perf_counter()
    with (output / f"{label}.log").open("x") as log:
        result = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, env=environment)
    measurement = {
        "label": label,
        "command": [str(part) for part in command],
        "seconds": time.perf_counter() - started,
        "exit_code": result.returncode,
    }
    measurements.append(measurement)
    (output / "timings.json").write_text(json.dumps(measurements, indent=2) + "\n")
    print(json.dumps(measurement), flush=True)
    result.check_returncode()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="New directory for logs and timing results")
    parser.add_argument("--repeats", type=int, default=2)
    parser.add_argument("--suite-repeats", type=int, default=1)
    parser.add_argument("--release", action="store_true", help="Use optimized binaries")
    parser.add_argument("--live", action="store_true", help="Also launch the live workloads")
    args = parser.parse_args()
    if args.repeats < 1 or args.suite_repeats < 0:
        parser.error("repeats must be positive and suite-repeats must be nonnegative")
    installation = os.environ["STELLARIS_PATH"]
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    measurements = []
    metadata = {
        "revision": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
        "installation": installation,
        "date": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "os": subprocess.check_output(["sw_vers"], text=True),
        "hardware": subprocess.check_output(
            ["sysctl", "machdep.cpu.brand_string", "hw.memsize", "hw.ncpu"], text=True
        ),
        "rustc": subprocess.check_output(["rustc", "-Vv"], text=True),
    }
    (output / "machine.json").write_text(json.dumps(metadata, indent=2) + "\n")
    profile = ["--release"] if args.release else []
    run(
        "build",
        ["cargo", "build", *profile, "--tests", "--examples", "--message-format=json"],
        output,
        measurements,
    )
    executables = {}
    for line in (output / "build.log").read_text().splitlines():
        if not line.startswith("{"):
            continue
        event = json.loads(line)
        if event.get("reason") == "compiler-artifact" and event.get("executable"):
            executables[event["target"]["name"]] = event["executable"]
    parity = "binding::analysis::tests::every_m45_named_candidate_has_one_initial_loader_entry"
    jobs = [
        ("registries", [executables["registries"], installation]),
        ("loader-parity", [executables["pdx_native"], "--ignored", "--exact", parity, "--nocapture"]),
    ]
    if args.live:
        jobs.extend([
            ("outside-common", [executables["live"], "--ignored", "outside_common"]),
            ("registry-report", [executables["registry-items-report"], installation]),
        ])
    for label, command in jobs:
        for repetition in range(1, args.repeats + 1):
            run(f"{label}-{repetition}", command, output, measurements)
    if args.live:
        for repetition in range(1, args.suite_repeats + 1):
            run(
                f"live-suite-{repetition}",
                [executables["live"], "--ignored"],
                output,
                measurements,
            )


if __name__ == "__main__":
    main()
