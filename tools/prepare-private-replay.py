#!/usr/bin/env python3
"""Copy pinned SDK-483 replay inputs from a restored private bundle into a new working root."""
import argparse
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DESCRIPTORS = ROOT / "tests/fixtures/private"


def verified_bytes(path, reference):
    if not path.is_file() or path.is_symlink():
        raise SystemExit(f"evidence-unavailable: restore the private typed-extraction bundle; missing {path}")
    data = path.read_bytes()
    if len(data) != reference["bytes"] or hashlib.sha256(data).hexdigest() != reference["sha256"]:
        raise SystemExit(f"retained artifact identity mismatch: {path}")
    return data


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("restored_root", type=Path, help="Root created by tools/evidence.py typed-extraction --restore")
    parser.add_argument("destination", type=Path, help="New working root; must not exist")
    args = parser.parse_args()
    if args.destination.exists():
        raise SystemExit(f"destination already exists: {args.destination}")
    pending = {}
    for case in ["normal", "missing-hook", "incomplete-stream", "worker-loss"]:
        reference = json.loads((DESCRIPTORS / f"{case}.ref.json").read_text())
        data = verified_bytes(DESCRIPTORS / reference["path"], reference)
        descriptor = json.loads(data)
        pending[reference["path"]] = data
        artifacts = [descriptor[role] for role in ["manifest", "request", "trace", "owner"]]
        artifacts.extend(descriptor["supporting"])
        for artifact in artifacts:
            path = artifact["path"]
            source = ROOT / ".local/evidence/bundles/typed-extraction.manifest.json" if path == "bundle-manifest.json" else args.restored_root / path
            pending[path] = verified_bytes(source, artifact)
    # Read and verify all prerequisites before creating a destination. Originals remain untouched.
    args.destination.mkdir(parents=True)
    for name, data in pending.items():
        output = args.destination / name
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_bytes(data)
    print(f"Prepared {len(pending)} verified recorded-data artifacts at {args.destination.resolve()}")
    print(f"PDX_NATIVE_PRIVATE_EVIDENCE={args.destination.resolve()}")


if __name__ == "__main__":
    main()
