#!/usr/bin/env python3
"""Check the replay dependency graph and prove the semantic no-process lint rejects an alias."""
import copy
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
EVIDENCE = ROOT / "crates/native-evidence"
# Reviewed serialization/hashing dependencies, including their build/proc-macro dependencies.
ALLOWED = {
    "pdx-native-evidence", "serde", "serde_core", "serde_derive", "serde_json", "sha2",
    "proc-macro2", "quote", "syn", "unicode-ident", "itoa", "memchr", "zmij", "digest",
    "block-buffer", "crypto-common", "generic-array", "typenum", "version_check", "cfg-if",
    "cpufeatures", "libc",
}


def check_graph(metadata):
    packages = {row["id"]: row for row in metadata["packages"]}
    nodes = {row["id"]: row for row in metadata["resolve"]["nodes"]}
    evidence = next(row for row in packages.values() if row["name"] == "pdx-native-evidence")
    if {row["name"] for row in evidence["dependencies"]} != {"serde", "serde_json", "sha2"}:
        raise ValueError("evidence direct dependencies changed; review the replay boundary")
    if evidence["features"] or any(target["kind"] != ["lib"] for target in evidence["targets"]):
        raise ValueError("evidence must remain a recorded-data library without features or build/launch targets")
    pending = [evidence["id"]]
    seen = set()
    while pending:
        package_id = pending.pop()
        if package_id in seen:
            continue
        seen.add(package_id)
        name = packages[package_id]["name"]
        if name not in ALLOWED:
            raise ValueError(f"unreviewed/live-capable dependency reachable from evidence: {name}")
        pending.extend(nodes[package_id]["dependencies"])
    return {packages[package_id]["name"] for package_id in seen}


def main():
    metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--locked", "--format-version", "1"], cwd=ROOT))
    reviewed = check_graph(metadata)
    # A transitive path back to Native must fail even if the direct dependency list is unchanged.
    control = copy.deepcopy(metadata)
    native_id = next(row["id"] for row in control["packages"] if row["name"] == "pdx-native")
    serde_node = next(row for row in control["resolve"]["nodes"] if row["id"].split("#")[-1].startswith("serde@"))
    serde_node["dependencies"].append(native_id)
    try:
        check_graph(control)
    except ValueError:
        pass
    else:
        raise SystemExit("dependency negative control failed to reject a transitive Native import")
    subprocess.run(["cargo", "clippy", "--locked", "--manifest-path", str(EVIDENCE / "Cargo.toml"), "--", "-D", "warnings", "-D", "clippy::disallowed_methods"], cwd=EVIDENCE, check=True)
    with tempfile.TemporaryDirectory(prefix="native-replay-boundary-") as temporary:
        package = Path(temporary) / "evidence"
        shutil.copytree(EVIDENCE, package)
        shutil.copyfile(ROOT / "Cargo.lock", package / "Cargo.lock")
        with (package / "src/lib.rs").open("a") as source:
            source.write('\n#[allow(dead_code)]\nfn boundary_control() {\n use std::process::Command as Launch;\n let _ = Launch::new("must-never-launch");\n}\n')
        result = subprocess.run(["cargo", "clippy", "--offline", "--manifest-path", str(package / "Cargo.toml"), "--target-dir", str(ROOT / "target/replay-boundary"), "--", "-D", "clippy::disallowed_methods"], cwd=package, text=True, capture_output=True)
        if result.returncode == 0 or "disallowed method" not in result.stderr or "Command::new" not in result.stderr:
            raise SystemExit(f"semantic no-launch negative control failed:\n{result.stderr}")
    print(f"Replay boundary verified: {len(reviewed)} reviewed packages; transitive Native import and aliased process creation rejected")


if __name__ == "__main__":
    main()
