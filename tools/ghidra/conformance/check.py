"""Check an export's readers against the conformance expectations of its build.

Usage: python3 check.py EXPECTATIONS.json READERS.json

Each expectation names a reader by its exact name and the flags that the export must give it.
Prints each mismatch and exits with status 1 when there is one. Run it on a new export before the
export is committed (tools/ghidra/README.md).
"""
import json
import sys


def mismatches(expectations, document):
    """Each expectation that the document's readers do not meet, as a sentence."""
    by_name = {}
    for entry, reader in document['readers'].items():
        if reader['name']:
            by_name.setdefault(reader['name'], []).append((entry, reader))
    found = []
    for expectation in expectations['readers']:
        name = expectation['reader']
        readers = by_name.get(name, [])
        if len(readers) != 1:
            found.append(f'{name}: {len(readers)} walked readers have this name, expected 1')
            continue
        entry, reader = readers[0]
        found.extend(f'{name} ({entry}): {problem}' for problem in reader_mismatches(expectation['expect'], reader))
    return found


def reader_mismatches(expect, reader):
    dispatch = reader['dispatch'] or {}
    actual = {
        'complete': reader['complete'],
        'form': dispatch.get('form'),
        'default': (dispatch.get('default') or {}).get('kind'),
        'loops': dispatch.get('loops'),
    }
    problems = [f'{flag} is {actual[flag]!r}, expected {expect[flag]!r}'
                for flag in ('complete', 'form', 'default', 'loops') if flag in expect and actual[flag] != expect[flag]]
    failed = set(reader['incomplete_because'])
    for condition in expect.get('incomplete_because', []):
        if condition not in failed:
            problems.append(f'incomplete_because lacks {condition} (has {sorted(failed)})')
    for condition in expect.get('not_incomplete_because', []):
        if condition in failed:
            problems.append(f'incomplete_because has {condition}')
    by_key = {row['key']: row for row in reader['rows']}
    for key, row_expect in expect.get('rows', {}).items():
        row = by_key.get(None if key == '*' else key)
        if row is None:
            problems.append(f'no row {key!r}')
            continue
        if 'kind' in row_expect and row['kind'] != row_expect['kind']:
            problems.append(f'row {key!r} kind is {row["kind"]!r}, expected {row_expect["kind"]!r}')
        nested = (row['nested'] or {}).get('name')
        if 'nested' in row_expect and nested != row_expect['nested']:
            problems.append(f'row {key!r} nested reader is {nested!r}, expected {row_expect["nested"]!r}')
    return problems


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    expectations = json.load(open(sys.argv[1]))
    document = json.load(open(sys.argv[2]))
    found = mismatches(expectations, document)
    for mismatch in found:
        print(mismatch)
    if found:
        sys.exit(1)
    print(f'{len(expectations["readers"])} conformance expectations met')


if __name__ == '__main__':
    main()
