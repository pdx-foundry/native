#!/usr/bin/env python3
"""Compare two scoped-numeric, duration, or command population reports offline."""
import argparse
from collections import Counter
import json
from pathlib import Path
import sys


ABSENT = object()

# These schema members are collections of independent facts. Other lists, including
# reference priority and key paths, retain their order and length.
UNORDERED_MEMBERS = {
    "forms", "targets", "child_families", "fixed_keys", "ordering", "durations",
    "units", "literal_syntax", "read", "uses", "All", "Fields", "Lookups", "Listed",
}


def match_members(edges):
    """Match each old member to a distinct compatible candidate, when possible."""
    candidate_owners = {}

    def assign(old_index, visited):
        for candidate in edges[old_index]:
            if candidate in visited:
                continue
            visited.add(candidate)
            previous = candidate_owners.get(candidate)
            if previous is None or assign(previous, visited):
                candidate_owners[candidate] = old_index
                return True
        return False

    # Reassignment matters when an unknown alternative can match several candidates
    # but a more specific alternative can match only one of them.
    for old_index in range(len(edges)):
        assign(old_index, set())
    return {old_index: candidate for candidate, old_index in candidate_owners.items()}


def property_tag(value):
    if value == "Unresolved":
        return "Unresolved"
    if isinstance(value, dict) and len(value) == 1:
        return next((tag for tag in ("Known", "Partial") if tag in value), None)
    return None


def difference(path, before, after):
    return {"path": path, "before": before,
            "after": after if after is not ABSENT else {"absent": True}}


def regressions(before, after, path):
    """Preserve established values while allowing new facts and stronger answers."""
    if before == after or before == "Unresolved":
        return []
    if after is ABSENT:
        return [difference(path, before, after)]
    old_tag, new_tag = property_tag(before), property_tag(after)
    if old_tag in ("Known", "Partial"):
        if new_tag not in ("Known", "Partial") or (old_tag == "Known" and new_tag == "Partial"):
            return [difference(path, before, after)]
        return regressions(before[old_tag], after[new_tag], path + "/" + old_tag)
    if isinstance(before, dict) and isinstance(after, dict):
        losses = []
        for key, value in before.items():
            candidate = after.get(key, ABSENT)
            member_path = path + "/" + key
            losses.extend(regressions(value, candidate, member_path))
        return losses
    if isinstance(before, list) and isinstance(after, list):
        # Named fields and units can move when a method discovers another member.
        for key in ("name", "key"):
            if before and all(isinstance(row, dict) and key in row for row in before + after):
                old_rows = index_rows(before, lambda row: row[key])
                new_rows = index_rows(after, lambda row: row[key])
                losses = []
                for name, row in old_rows.items():
                    candidate = new_rows.get(name, ABSENT)
                    member_path = path + "/" + name
                    losses.extend(regressions(row, candidate, member_path))
                return losses
        members = path.split("/")
        while members and members[-1] in ("Known", "Partial"):
            members.pop()
        member = members[-1] if members else ""
        if member not in UNORDERED_MEMBERS:
            if len(before) != len(after):
                return [difference(path, before, after)]
            losses = []
            for index, (old, new) in enumerate(zip(before, after)):
                losses.extend(regressions(old, new, path + "/" + str(index)))
            return losses
        edges = []
        for index, old in enumerate(before):
            compatible = []
            for candidate, new in enumerate(after):
                if not regressions(old, new, path + "/" + str(index)):
                    compatible.append(candidate)
            edges.append(compatible)
        matches = match_members(edges)
        return [difference(path + "/" + str(index), old, after)
                for index, old in enumerate(before) if index not in matches]
    # These unwrapped markers explicitly mean no fact was established. Known(null)
    # is handled above and remains an established absence, not an unknown value.
    if isinstance(before, str) and before in ("Unknown", "NotEstablished"):
        return []
    if before is None and path.endswith("/id"):
        return []
    return [difference(path, before, after)]


def index_rows(rows, identity):
    indexed = {}
    for row in rows:
        key = identity(row)
        if key in indexed:
            raise ValueError(f"duplicate population identity: {key}")
        indexed[key] = row
    return indexed


def encode_key(*parts):
    return json.dumps(parts, separators=(",", ":"))


def duration_key(row):
    return tuple(sorted(unit["key"] for unit in row["units"]))


def registry_property(value):
    """Interpret the Result debug strings emitted by duration-population."""
    if value.startswith("Err(") and value.endswith(")"):
        return "Unresolved"
    if value.startswith("Ok(") and value.endswith(")"):
        return {"Known": value[3:-1]}
    raise ValueError(f"invalid registry duration Result: {value}")


def registry_duration(row):
    return {**row, "combination": registry_property(row["combination"]),
            "units": [{**unit, "factor": registry_property(unit["factor"])}
                      for unit in row["units"]]}


def align_duration_groups(before, after):
    """Retain identities when a group gains units within the same owner and path."""
    old_keys = sorted(before.keys() - after.keys())
    new_keys = sorted(after.keys() - before.keys())
    edges = []
    for old_key in old_keys:
        old_identity = json.loads(old_key)
        compatible = []
        for candidate, new_key in enumerate(new_keys):
            new_identity = json.loads(new_key)
            if (old_identity[:-1] == new_identity[:-1]
                    and set(old_identity[-1]) <= set(new_identity[-1])):
                compatible.append(candidate)
        edges.append(compatible)
    aligned = dict(after)
    for old_index, candidate in match_members(edges).items():
        aligned[old_keys[old_index]] = aligned.pop(new_keys[candidate])
    return aligned


def command_case(case):
    answer = case.get("answer")
    if answer is not None:
        answer = dict(answer)
        answer["source"] = {key: value for key, value in answer["source"].items()
                            if key not in ("method", "native_version")}
    return {"status": case["status"], "answer": answer, "error": case.get("error")}


def project(report):
    """Keep comparison inputs; omit timings and internal analysis diagnostics."""
    if not isinstance(report, dict) or not isinstance(report.get("build"), str):
        raise ValueError("expected a population report with a build string")
    if "arguments" in report and "fields" in report:
        sections = {}
        for kind in ("fields", "arguments"):
            rows = index_rows(report[kind]["destinations"],
                              lambda row: encode_key(row["owner"], row["path"]))
            sections[kind] = {key: row["reader"] for key, row in rows.items()}
        return "scoped", sections, {kind: report[kind]["counts"] for kind in sections}
    if "groups" in report and "registry_groups" in report:
        groups = index_rows(report["groups"],
                            lambda row: encode_key(row["command"], duration_key(row["duration"])))
        registries = index_rows(report["registry_groups"],
                                lambda row: encode_key(row["registry"], row["path"], duration_key(row)))
        return "duration", {"durations": {key: row["duration"] for key, row in groups.items()},
                            "registry_groups": {key: registry_duration(row) for key, row in registries.items()}}, {"durations": report["counts"]}
    if "inventories" in report:
        commands, inventories = {}, {}
        for inventory in report["inventories"]:
            kind = inventory["kind"]
            if kind in inventories:
                raise ValueError(f"duplicate command inventory: {kind}")
            inventories[kind] = {
                "gaps": inventory["inventory_gaps"],
                "unknown_reasons": sorted(row["reason"] for row in inventory["unknown_registrations"]),
                "full_denominator_known": inventory["full_denominator_known"],
            }
            cases = index_rows(inventory["cases"], lambda row: f'{kind}/command/{row["name"]}')
            commands.update({key: command_case(row) for key, row in cases.items()})
    elif "answers" in report:
        commands, inventories = {}, {}
        for subject, case in report["answers"].items():
            if subject.endswith("/inventory"):
                inventories[subject.removesuffix("/inventory")] = case
            elif "/command/" in subject:
                commands[subject] = command_case(case)
            else:
                raise ValueError(f"not a command population subject: {subject}")
    else:
        raise ValueError("unrecognized population report format")
    counts = dict(Counter(case["status"] for case in commands.values()))
    return "command", {"commands": commands, "inventories": inventories}, {"commands": counts}


def command_regressions(before, after, path):
    if after is ABSENT:
        return [difference(path, before, after)]
    old_answer, new_answer = before["answer"], after["answer"]
    if old_answer is None:
        return []
    if new_answer is None:
        return [difference(path + "/answer", old_answer, new_answer)]
    losses = regressions(old_answer["value"], new_answer["value"], path + "/value")
    losses += regressions(old_answer["source"], new_answer["source"], path + "/source")
    if old_answer["completeness"] == "Complete" and new_answer["completeness"] != "Complete":
        losses.append(difference(path + "/completeness", "Complete", new_answer["completeness"]))
    ranks = {"failed": 0, "partial": 1, "complete": 2}
    if ranks[before["status"]] > ranks[after["status"]]:
        losses.append(difference(path + "/status", before["status"], after["status"]))
    return losses


def inventory_regressions(before, after, path):
    if after is ABSENT:
        return [difference(path, before, after)]
    if (before["full_denominator_known"] and not after["full_denominator_known"]
            or Counter(after["gaps"]) - Counter(before["gaps"])
            or Counter(after["unknown_reasons"]) - Counter(before["unknown_reasons"])):
        return [difference(path, before, after)]
    return []


def compare(before, after):
    old_kind, old_sections, old_counts = project(before)
    new_kind, new_sections, new_counts = project(after)
    if old_kind != new_kind:
        raise ValueError("reports must have the same population type")
    if before["build"] != after["build"]:
        raise ValueError("reports must describe the same exact build")
    changes, losses = {}, []
    for section, old_rows in old_sections.items():
        new_rows = new_sections[section]
        if section in ("durations", "registry_groups"):
            new_rows = align_duration_groups(old_rows, new_rows)
        changes[section] = []
        check = {"commands": command_regressions, "inventories": inventory_regressions}.get(section, regressions)
        for key in sorted(old_rows.keys() | new_rows.keys()):
            old, new = old_rows.get(key, ABSENT), new_rows.get(key, ABSENT)
            path = section + "/" + key
            if old != new:
                changes[section].append(difference(path, old if old is not ABSENT else {"absent": True}, new))
            if old is not ABSENT:
                losses.extend(check(old, new, path))
    if old_kind != "command":
        for count in ("commands", "registries"):
            if after[count] < before[count]:
                losses.append(difference(count, before[count], after[count]))
        for failure in after["failed_questions"]:
            if failure not in before["failed_questions"]:
                losses.append(difference("failed_questions", None, failure))
        if old_kind == "scoped" and before["registry_completeness"] == "Complete" and after["registry_completeness"] != "Complete":
            losses.append(difference("registry_completeness", before["registry_completeness"], after["registry_completeness"]))
    return {"regressions": losses, "changes": changes,
            "changed": {kind: len(rows) for kind, rows in changes.items()},
            "counts": {"before": old_counts, "after": new_counts}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("before", type=Path)
    parser.add_argument("after", type=Path)
    args = parser.parse_args()
    try:
        report = compare(json.loads(args.before.read_text()), json.loads(args.after.read_text()))
    except (OSError, ValueError, KeyError, TypeError, AttributeError) as error:
        parser.exit(2, f"invalid population input: {error}\n")
    print(json.dumps(report, indent=2))
    return int(bool(report["regressions"]))


if __name__ == "__main__":
    sys.exit(main())
