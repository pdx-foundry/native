import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from compare import compare, regressions


def scoped(reader):
    return {"build": "build", "commands": 1, "registries": 0,
            "registry_completeness": "Complete", "failed_questions": [],
            "fields": {"destinations": [], "counts": {"complete": 0}},
            "arguments": {"counts": {"partial": 1}, "destinations": [
                {"owner": "Effect/test", "path": ["value"], "reader": reader}]}}


def duration(value):
    return {"build": "build", "commands": 1, "registries": 0,
            "failed_questions": [], "counts": {"partial": 1}, "registry_groups": [],
            "groups": [{"command": "Effect/test", "duration": value}]}


def commands(value):
    return {"build": "build", "inventories": [{
        "kind": "Effect", "inventory_gaps": [], "unknown_registrations": [],
        "full_denominator_known": True, "cases": [{
            "name": "test", "status": "partial", "answer": {
                "value": value, "completeness": "Partial", "gaps": [],
                "source": {"method": "v1", "native_version": "0.1", "build": "build", "basis": "StaticAnalysis"}}}]}]}


class ComparisonTests(unittest.TestCase):
    def test_floor_checks_nested_properties_even_on_partial_to_known_gain(self):
        old = {"Partial": {"width": {"Known": 32}, "scale": "Unresolved"}}
        gain = {"Known": {"width": {"Known": 32}, "scale": {"Known": 1000}}}
        self.assertEqual(regressions(old, gain, "value"), [])
        gain["Known"]["width"] = {"Known": 64}
        self.assertTrue(regressions(old, gain, "value"))
        for new in ["Unresolved", {"Partial": 32}, {"Known": 64}, None]:
            self.assertTrue(regressions({"Known": 32}, new, "value"))
        self.assertTrue(regressions({"Known": None}, {"Known": 0}, "value"))

    def test_named_members_reorder_and_gain_but_cannot_lose_resolved_facts(self):
        old = [{"name": "one", "value": {"Partial": 1}}, {"name": "two", "value": "Unresolved"}]
        new = [old[1], {"name": "new"}, {"name": "one", "value": {"Known": 1}}]
        self.assertEqual(regressions(old, new, "fields"), [])
        self.assertTrue(regressions(old, new[:2], "fields"))
        self.assertTrue(regressions([1, 1], [1], "duplicates"))
        self.assertEqual(regressions([1, 2], [2, 1, 3], "forms"), [])

    def test_scoped_addition_removal_and_change_counts(self):
        before = scoped({"numeric": {"Partial": {"width": "Unresolved"}}})
        after = scoped({"numeric": {"Known": {"width": {"Known": 32}}}})
        report = compare(before, after)
        self.assertFalse(report["regressions"])
        self.assertEqual(report["changed"], {"fields": 0, "arguments": 1})
        self.assertTrue(compare(after, before)["regressions"])
        removed = copy.deepcopy(after)
        removed["arguments"]["destinations"] = []
        self.assertTrue(compare(after, removed)["regressions"])
        self.assertEqual(compare(removed, after)["changed"]["arguments"], 1)
        self.assertFalse(compare(removed, after)["regressions"])

    def test_duration_units_and_registry_groups_are_compared(self):
        before = duration({"units": [{"key": "days", "factor": {"Known": 1}}], "omitted_count": "Unresolved"})
        after = copy.deepcopy(before)
        after["groups"][0]["duration"]["omitted_count"] = {"Known": 0}
        self.assertEqual(compare(before, after)["changed"]["durations"], 1)
        self.assertFalse(compare(before, after)["regressions"])
        after["groups"][0]["duration"]["units"][0]["factor"] = {"Known": 30}
        self.assertTrue(compare(before, after)["regressions"])
        before["registry_groups"] = [{"registry": "common/test", "path": [],
                                      "units": [{"key": "days", "factor": "Ok(Some(1))"}], "combination": "Ok(ScaledAtRead)"}]
        self.assertTrue(compare(before, after)["regressions"])

    def test_command_diagnostics_and_stamps_do_not_hide_value_changes(self):
        before = commands({"fixed_keys": {"Known": [{"name": "value", "numeric": {"Known": 32}}]}})
        after = copy.deepcopy(before)
        case = after["inventories"][0]["cases"][0]
        after["elapsed_ms"] = 20
        case["diagnostics"] = {"address": 123}
        case["answer"]["source"]["method"] = "v2"
        self.assertEqual(compare(before, after)["changed"]["commands"], 0)
        case["answer"]["gaps"] = [{"detail": "new gap"}]
        self.assertEqual(compare(before, after)["changed"]["commands"], 1)
        self.assertFalse(compare(before, after)["regressions"])
        case["answer"]["value"]["fixed_keys"]["Known"][0]["numeric"] = {"Known": 64}
        self.assertTrue(compare(before, after)["regressions"])

    def test_command_error_loss_and_compact_baseline(self):
        before = commands({"numeric": {"Known": 1}})
        inventory = before["inventories"][0]
        baseline = {"build": "build", "answers": {
            "Effect/inventory": {"gaps": [], "unknown_reasons": [], "full_denominator_known": True},
            "Effect/command/test": {key: value for key, value in inventory["cases"][0].items() if key != "name"}}}
        self.assertFalse(compare(before, baseline)["regressions"])
        self.assertEqual(compare(before, baseline)["changed"]["commands"], 0)
        after = copy.deepcopy(before)
        after["inventories"][0]["cases"][0] = {"name": "test", "status": "failed", "error": "broken"}
        self.assertTrue(compare(before, after)["regressions"])
        self.assertFalse(compare(after, before)["regressions"])
        after["inventories"][0]["cases"] = []
        self.assertTrue(compare(before, after)["regressions"])

    def test_inventory_uncertainty_and_metadata_losses(self):
        before = commands({})
        after = copy.deepcopy(before)
        after["inventories"][0]["inventory_gaps"] = ["unknown table"]
        self.assertTrue(compare(before, after)["regressions"])
        self.assertFalse(compare(after, before)["regressions"])
        before = scoped({})
        for key, value in [("commands", 0), ("failed_questions", [{"registry": "test", "error": "broken"}]),
                           ("registry_completeness", "Partial")]:
            after = copy.deepcopy(before)
            after[key] = value
            self.assertTrue(compare(before, after)["regressions"])

    def test_unknown_markers_can_gain_facts_but_known_absence_cannot(self):
        self.assertFalse(regressions({"id": None, "kind": "Unknown", "reference": "NotEstablished"},
                                     {"id": "reader", "kind": "Integer", "reference": {"Lookups": []}}, "reader"))
        self.assertTrue(regressions({"Known": None}, {"Known": "value"}, "numeric"))

    def test_rejects_duplicates_wrong_build_and_wrong_report_type(self):
        before = scoped({})
        after = copy.deepcopy(before)
        after["arguments"]["destinations"] *= 2
        with self.assertRaises(ValueError):
            compare(before, after)
        after = copy.deepcopy(before)
        after["build"] = "another-build"
        with self.assertRaises(ValueError):
            compare(before, after)
        with self.assertRaises(ValueError):
            compare(before, commands({}))
        with self.assertRaises(ValueError):
            compare({}, {})

    def test_semantic_sequences_keep_order_and_length(self):
        for member, values in [("reference_priority", ["Trigger", "Variable"]),
                               ("path", ["outer", "inner"]), ("Key", ["outer", "inner"])]:
            before = commands({member: values})
            for changed in [list(reversed(values)), values + ["extra"]]:
                self.assertTrue(compare(before, commands({member: changed}))["regressions"])

    def test_alternatives_match_strengthened_facts_after_an_insertion(self):
        old_form = {"Value": {"reader": {"kind": "Integer", "numeric": {"Partial": 32}}}}
        new_form = {"Value": {"reader": {"kind": "Integer", "numeric": {"Known": 32}}}}
        boolean = {"Value": {"reader": {"kind": "Boolean", "numeric": {"Known": None}}}}
        before = commands({"forms": {"Known": [old_form]}})
        after = commands({"forms": {"Known": [boolean, new_form]}})
        self.assertFalse(compare(before, after)["regressions"])
        after["inventories"][0]["cases"][0]["answer"]["value"]["forms"]["Known"] = [boolean]
        self.assertTrue(compare(before, after)["regressions"])
        self.assertTrue(regressions([old_form, old_form], [new_form], "forms"))
        # The broad alternative must leave the only numeric candidate for the narrow one.
        self.assertFalse(regressions(["Unresolved", old_form], [new_form, boolean], "forms"))

    def test_duration_group_unit_additions_preserve_identity_and_check_values(self):
        before = duration({"units": [{"key": "days", "factor": {"Known": 1}}],
                           "combination": {"Known": "ScaledAtRead"}})
        after = copy.deepcopy(before)
        after["groups"][0]["duration"]["units"].append({"key": "months", "factor": {"Known": 30}})
        report = compare(before, after)
        self.assertFalse(report["regressions"])
        self.assertEqual(report["changed"]["durations"], 1)
        after["groups"][0]["duration"]["units"][0]["factor"] = {"Known": 2}
        self.assertTrue(compare(before, after)["regressions"])
        before["groups"].append({"command": "Effect/test", "duration": {
            "units": [{"key": "months", "factor": {"Known": 30}}],
            "combination": {"Known": "ScaledAtRead"}}})
        after["groups"][0]["duration"]["units"][0]["factor"] = {"Known": 1}
        self.assertTrue(compare(before, after)["regressions"], "two groups cannot collapse into one")

    def test_registry_duration_results_normalize_errors_and_preserve_successes(self):
        before = duration({"units": []})
        before["registry_groups"] = [{"registry": "common/test", "path": ["outer", "inner"],
            "units": [{"key": "days", "factor": 'Err(Unresolved { reason: "call", stop: None, trace: None })'}],
            "combination": 'Err(Unresolved { reason: "call", stop: None, trace: None })'}]
        after = copy.deepcopy(before)
        group = after["registry_groups"][0]
        group["units"][0]["factor"] = 'Err(Unresolved { reason: "other", stop: None, trace: None })'
        self.assertEqual(compare(before, after)["changed"]["registry_groups"], 0)
        group["units"][0]["factor"] = "Ok(Some(1))"
        group["combination"] = "Ok(ScaledAtRead)"
        group["units"].append({"key": "months", "factor": "Ok(Some(30))"})
        self.assertFalse(compare(before, after)["regressions"])
        self.assertEqual(compare(before, after)["changed"]["registry_groups"], 1)
        self.assertTrue(compare(after, before)["regressions"])
        changed = copy.deepcopy(after)
        changed["registry_groups"][0]["units"][0]["factor"] = "Ok(Some(2))"
        self.assertTrue(compare(after, changed)["regressions"])
        changed["registry_groups"][0]["units"][0]["factor"] = "Ok(None)"
        self.assertTrue(compare(after, changed)["regressions"])

    def test_cli_exit_codes_and_machine_readable_report(self):
        with tempfile.TemporaryDirectory() as directory:
            old_path, new_path = [Path(directory) / name for name in ("before.json", "after.json")]
            old_path.write_text(json.dumps(scoped({"numeric": {"Known": 1}})))
            for candidate, expected in [(scoped({"numeric": {"Known": 1}}), 0),
                                        (scoped({"numeric": "Unresolved"}), 1), ({}, 2)]:
                new_path.write_text(json.dumps(candidate))
                result = subprocess.run([sys.executable, str(Path(__file__).with_name("compare.py")), str(old_path), str(new_path)],
                                        capture_output=True, text=True)
                self.assertEqual(result.returncode, expected, result.stderr)
                if expected != 2:
                    self.assertIn("regressions", json.loads(result.stdout))
            new_path.write_text("{")
            result = subprocess.run([sys.executable, str(Path(__file__).with_name("compare.py")), str(old_path), str(new_path)], capture_output=True)
            self.assertEqual(result.returncode, 2)


if __name__ == "__main__":
    unittest.main()
