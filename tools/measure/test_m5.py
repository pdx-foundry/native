import unittest

from m5 import acceptance_refusal, compare, measure, mismatched_inputs


def claim(identity, file, owner="engine_fact"):
    return {"id": identity, "file": file, "owner": owner, "subject": ["type"], "property": "value_form"}


def record(identity, covered, reason="Atlas has an answer"):
    return {"claim": identity, "covered": covered, "reason": reason}


class MeasureTests(unittest.TestCase):
    def test_measure_counts_atlas_owned_claims_under_common_and_events_only(self):
        ledger = {"claims": [
            claim("a", "common/buildings.cwt"),
            claim("b", "events/events.cwt", owner="content_derived"),
            claim("c", "common/buildings.cwt"),
            claim("d", "effects.cwt"),
            claim("e", "common/buildings.cwt", owner="consumer_policy"),
        ]}
        coverage = {"claims": [
            record("a", True),
            record("b", False, "No qualified answer"),
            record("c", False, "Native gap: unresolved"),
            record("d", True),
            record("e", False),
        ]}

        covered, open_claims = measure(ledger, coverage)

        self.assertEqual(covered, {"a"})
        self.assertEqual({key: reason for key, (_, reason) in open_claims.items()},
                         {"b": "no answer", "c": "Native gap"})

    def test_compare_reports_lost_and_gained_claims(self):
        self.assertEqual(compare({"a", "b"}, {"b", "c"}), {"gained": ["c"], "lost": ["a"]})

    def test_runs_with_another_recording_do_not_compare(self):
        reference = {"native": "one", "atlas_commit": "x", "recording_sha256": "r1"}
        current = {"native": "two", "atlas_commit": "x", "recording_sha256": "r2"}

        self.assertEqual(mismatched_inputs(reference, current), ["recording_sha256"])

    def test_runs_of_other_installed_content_do_not_compare(self):
        reference = {"content_sha256": "c1"}
        current = {"content_sha256": "c2"}

        self.assertIn("content_sha256", mismatched_inputs(reference, current))


class AcceptanceTests(unittest.TestCase):
    def test_only_a_run_that_passed_against_the_current_accepted_run_is_accepted(self):
        passed = {"gate": {"run": "main", "passed": True, "lost": 0}}

        self.assertIsNone(acceptance_refusal(passed, "main"))
        self.assertIsNotNone(acceptance_refusal(passed, "newer"))
        self.assertIsNotNone(acceptance_refusal({}, "main"))
        self.assertIsNotNone(acceptance_refusal(
            {"gate": {"run": "main", "passed": False, "lost": 3}}, "main"))


if __name__ == "__main__":
    unittest.main()
