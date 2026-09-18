"""Game-free rejection controls for the private worker handshake."""
import copy
import unittest

from protocol import VERSION, validate_hello
from run import replace_once
from verify import FIXTURE, check_preservation, check_trace


class HandshakeTests(unittest.TestCase):
    def test_each_identity_mismatch_is_rejected(self):
        expected = {"artifacts": {"worker.py": "pin"}, "target": "target",
                    "python": "python", "lldb": "lldb", "module": "module"}
        hello = dict(expected, protocol=VERSION, run="run", gamePid=42, workerPid=43)
        validate_hello(hello, expected, "run", 42, 43)
        for key in hello:
            with self.subTest(key=key):
                changed = copy.deepcopy(hello)
                changed[key] = "wrong"
                with self.assertRaises(ValueError):
                    validate_hello(changed, expected, "run", 42, 43)
        with self.assertRaises(ValueError):
            validate_hello(dict(hello, backend="override"), expected, "run", 42, 43)

    def test_source_patch_refuses_absent_or_ambiguous_anchor(self):
        for source in ("missing", "anchor anchor"):
            with self.assertRaises(ValueError):
                replace_once(source, "anchor", "replacement")


class TraceTests(unittest.TestCase):
    def normal_trace(self):
        rows = [
            {"kind": "launch-stopped", "thread": 9, "triple": "arm64-apple-macosx", "frames": [{"function": "_dyld_start"}]},
            {"kind": "hooks-active-before-resume", "hooks": {
                name: {"enabled": True, "resolved": 1, "hits": 0}
                for name in ("registration", "load-file", "field")}},
            {"kind": "resume"},
            *[{"kind": "registration-observed", "thread": 9} for _ in range(3)],
            {"kind": "phase-reached", "phase": "fixture-file-parse", "file": FIXTURE, "thread": 9},
            *[{"kind": "field-observed", "field": name, "line": line,
               "file": FIXTURE, "thread": 9, "owner": "owner"}
              for name, line in (("tree_template", 2), ("traditions", 3))],
            {"kind": "phase-complete", "file": FIXTURE, "thread": 9, "producerFieldCount": 2},
            {"kind": "stream-end", "producerFieldCount": 2, "registrations": 3, "producerLastSequence": 11},
        ]
        return [dict(row, seq=index) for index, row in enumerate(rows, 1)]

    def test_lost_record_cannot_complete(self):
        trace = self.normal_trace()
        check_trace(trace, "none")
        del trace[7]
        check_trace(trace, "incomplete")
        with self.assertRaises(AssertionError):
            check_trace(trace, "none")

    def test_wrong_thread_owner_count_or_disabled_hook_cannot_complete(self):
        for row, key, value in ((0, "thread", 10), (9, "thread", 10), (8, "owner", "other"),
                                (10, "producerFieldCount", 1)):
            trace = self.normal_trace()
            trace[row][key] = value
            with self.assertRaises(AssertionError):
                check_trace(trace, "none")
        trace = self.normal_trace()
        trace[1]["hooks"]["field"]["enabled"] = False
        with self.assertRaises(AssertionError):
            check_trace(trace, "none")

    def test_consistent_but_unrelated_file_cannot_complete(self):
        trace = self.normal_trace()
        for row in trace:
            if "file" in row:
                row["file"] = "common/tradition_categories/unrelated.txt"
        with self.assertRaises(AssertionError):
            check_trace(trace, "none")

    def test_observations_before_resume_cannot_complete(self):
        trace = self.normal_trace()
        resume = trace.pop(2)
        trace.insert(9, resume)
        trace = [dict(row, seq=index) for index, row in enumerate(trace, 1)]
        with self.assertRaises(AssertionError):
            check_trace(trace, "none")

    def test_missing_hook_requires_specific_failure_and_omitted_hook(self):
        trace = [self.normal_trace()[0],
                 {"kind": "hooks-requested", "hooks": {"registration": {}, "load-file": {}}},
                 {"kind": "capability-unavailable", "capability": "tradition-category-field-reads",
                  "reason": "required hook missing before resume"}]
        trace = [dict(row, seq=index) for index, row in enumerate(trace, 1)]
        check_trace(trace, "missing")
        for key in ("capability", "reason"):
            changed = copy.deepcopy(trace)
            changed[-1][key] = "unrelated"
            with self.assertRaises(AssertionError):
                check_trace(changed, "missing")
        trace[1]["hooks"]["field"] = {}
        with self.assertRaises(AssertionError):
            check_trace(trace, "missing")

    def test_preservation_checks_underlying_before_and_after_hashes(self):
        expected = {"target": "target", "content": {"file": "hash"},
                    "protected": {"settings": "settings-hash", "absent": None}}
        record = {"before": expected, "after": expected}
        manifest = {"protectedBefore": expected["protected"]}
        check_preservation(record, expected, manifest)
        for side in ("before", "after"):
            for key in expected:
                changed = copy.deepcopy(record)
                changed[side] = dict(changed[side], **{key: "changed"})
                with self.assertRaises(AssertionError):
                    check_preservation(changed, expected, manifest)


if __name__ == "__main__":
    unittest.main()
