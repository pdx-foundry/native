#!/usr/bin/env python3
"""Verify SDK-515 trial records offline; never start processes."""
import json
from pathlib import Path
import sys

from protocol import sha, validate_hello, write

OUTCOMES = {"none": "complete", "missing": "unavailable", "incomplete": "incomplete", "worker-loss": "worker-lost"}


def read(path):
    return json.loads(path.read_text())


def check_trace(trace, fault):
    kinds = [row["kind"] for row in trace]
    start = next(row for row in trace if row["kind"] == "launch-stopped")
    assert start["frames"][0]["function"] == "_dyld_start"
    assert start["triple"].startswith("arm64-")
    assert not set(kinds) & {"callback-error", "native-exception", "early-activation-unavailable"}
    sequences = [row["seq"] for row in trace]
    assert sequences == sorted(set(sequences))
    contiguous = sequences == list(range(1, len(trace) + 1))
    if fault == "missing":
        assert "capability-unavailable" in kinds
        assert not set(kinds) & {"resume", "field-observed", "stream-end", "hooks-active-before-resume"}
        assert contiguous
        return
    active = next(row for row in trace if row["kind"] == "hooks-active-before-resume")
    resume = next(row for row in trace if row["kind"] == "resume")
    assert start["seq"] < active["seq"] < resume["seq"]
    assert set(active["hooks"]) == {"registration", "load-file", "field"}
    assert all(hook["enabled"] and hook["resolved"] == 1 and hook["hits"] == 0 for hook in active["hooks"].values())
    registrations = [row for row in trace if row["kind"] == "registration-observed"]
    assert len(registrations) == 3
    assert all(row["seq"] > active["seq"] and row["thread"] for row in registrations)
    if fault == "worker-loss":
        assert "worker-loss-ready" in kinds and "stream-end" not in kinds
        assert contiguous
        return
    entry = next(row for row in trace if row["kind"] == "phase-reached" and row.get("phase") == "fixture-file-parse")
    returned = next(row for row in trace if row["kind"] == "phase-complete")
    end = next(row for row in trace if row["kind"] == "stream-end")
    fields = [row for row in trace if row["kind"] == "field-observed"]
    assert all(row["seq"] < entry["seq"] for row in registrations)
    assert entry["seq"] < returned["seq"] < end["seq"]
    assert entry["file"] == returned["file"] and entry["thread"] == returned["thread"]
    assert all(entry["seq"] < row["seq"] < returned["seq"] and
               row["file"] == entry["file"] and row["thread"] == entry["thread"] for row in fields)
    assert end["producerLastSequence"] == end["seq"]
    assert end["producerFieldCount"] == returned["producerFieldCount"] == 2
    assert end["registrations"] == 3
    if fault == "incomplete":
        assert not contiguous and len(fields) == 1
        assert [(row["field"], row["line"]) for row in fields] == [("traditions", 3)]
    else:
        assert contiguous and len(fields) == 2
        assert [(row["field"], row["line"]) for row in fields] == [("tree_template", 2), ("traditions", 3)]
        assert len({row["owner"] for row in fields}) == 1


def verify(root):
    expected = read(root / "expected.json")
    preflight = read(root / "preflight.json")
    assert sha(root / "expected.json") == preflight["expectedSha256"]
    for name, digest in preflight["artifacts"].items():
        assert sha(root / name) == digest, name
    for name, digest in preflight["trialSources"].items():
        assert sha(root / "trial-source" / name) == digest, name
    results = []
    for run in sorted((root / "runs").iterdir()):
        manifest, result = read(run / "manifest.json"), read(run / "result.json")
        fault = manifest["fault"]
        assert result["status"] == OUTCOMES[fault]
        assert result["targetUnchanged"] and result["producerContentUnchanged"] and result["protectedUnchanged"]
        assert result["disposal"]["confirmed"]
        assert read(run / "producer-content.json") == preflight["content"]
        assert sha(run / "producer-content.json") == manifest["producerContentManifestSha256"]
        fixture = "mod/atlas_early/common/tradition_categories/atlas_early_fixture.txt"
        assert sha(run / "profile" / fixture) == manifest["fixtureHashes"][fixture]
        for name, digest in manifest["probeHashes"].items():
            assert expected["artifacts"][name] == digest
            assert sha(root / name if name.endswith(".dylib") else run / "source" / name) == digest
        owner = read(run / "owner.json")
        owned = next(row for row in owner if row["kind"] == "game-owned-suspended")
        worker = next(row for row in owner if row["kind"] == "worker-started")
        assert any(row["kind"] == "handshake-accepted" for row in owner)
        validate_hello(read(run / "hello.json"), expected, run.name, owned["pid"], worker["pid"])
        disposal = owner[-1]
        assert disposal["kind"] == "disposal-checked"
        assert disposal["confirmed"] and disposal["reapedPid"] == owned["pid"] and disposal["remainingIdentity"] is None
        if fault == "worker-loss":
            assert any(row["kind"] == "worker-loss-injected" for row in owner)
            assert next(row for row in owner if row["kind"] == "worker-exited")["returncode"] == -9
            assert any(row["kind"] == "owner-dispose-requested" for row in owner)
        trace = [json.loads(line) for line in (run / "trace.jsonl").read_text().splitlines()]
        assert all(row["run"] == run.name for row in trace)
        assert next(row for row in trace if row["kind"] == "launch-stopped")["pid"] == owned["pid"]
        check_trace(trace, fault)
        results.append({"run": run.name, "fault": fault, "outcome": result["status"],
                        "records": len(trace), "reaped": True,
                        "ownerSeconds": (disposal["monotonicNs"] - owned["monotonicNs"]) / 1e9})
    assert sorted(row["fault"] for row in results) == sorted(OUTCOMES)
    return {"qualification": "candidate; no production support", "runs": results}


if __name__ == "__main__":
    root = Path(sys.argv[1]).resolve()
    summary = verify(root)
    write(root / "summary.json", summary)
    print(json.dumps(summary, indent=2))
