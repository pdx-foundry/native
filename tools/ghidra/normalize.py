"""Turn a raw export (export.py) into a build directory's `readers.json` and `provenance.json`.

Usage: python3 normalize.py EXPORT.json TABLES.json BUILD_DIRECTORY

`readers.json` holds, for each walked reader, its dispatch facts, its completeness under the six
conditions of `docs/design/ghidra-front-end.md#completeness`, and its rows; and for each registry
the readers of its root keys. It holds typed rows, addresses and names only. The script prints the
count of registries whose whole reader tree is complete.
"""
import datetime
import hashlib
import json
import os
import sys

import rows

SCHEMA = 1
HERE = os.path.dirname(os.path.abspath(__file__))
EXPORT_FILES = ('Export.java', 'export.py', 'rows.py', 'normalize.py')
RECOGNIZED_FORMS = ('token-switch', 'compare-chain', 'entry')
# Default kinds that reject a key the reader does not enumerate: Atlas may call such a key no field.
REJECTING_DEFAULTS = ('log-and-skip', 'persistent')
# Row kinds that read the value through a nested reader; without one the row's reader is unresolved.
NESTING_KINDS = ('persistent', 'serializer', 'object', 'object-list', 'entry', 'delegates')
ROW_FIELDS = ('token', 'key', 'kind', 'value', 'reader_kind', 'repeat', 'conditional', 'family', 'items',
              'branches', 'nested', 'scope_mask', 'enum', 'lookup', 'paths', 'store')


def normalize(export, tables):
    """The `readers.json` document of a raw export."""
    tokens = tables['tokens']
    locate = Locator(export)
    readers = {entry: reader_record(reader, locate, tokens, export['enums'])
               for entry, reader in export['readers'].items()}
    for entry, failure in sorted(locate.failed_entries().items()):
        readers.setdefault(entry, failed_record(entry, failure))
    mark_completeness(readers)

    return {
        'schema': SCHEMA,
        'tool': 'ghidra-export',
        'build': tables['build']['executable'],
        'registries': registries(export['registries'], locate),
        'readers': dict(sorted(readers.items())),
    }


class Locator:
    """Where a reader spec (an address, a name, or `class:` and a class name) was walked."""

    def __init__(self, export):
        self.walked = export['walked']
        self.resolved = export['resolved']
        self.failures = export['failures']
        self.readers = export['readers']

    def entry(self, spec):
        target = self.resolved.get(spec) if spec.startswith('class:') else spec
        entry = self.walked.get(target) if target else None
        return entry if entry in self.readers else None

    def failure(self, spec):
        target = self.resolved.get(spec) if spec.startswith('class:') else spec
        return self.failures.get(spec) or self.failures.get(target)

    def failed_entries(self):
        """The addresses that the export tried to walk and could not."""
        return {target: failure for target, failure in self.failures.items()
                if target.startswith('0x') and self.walked.get(target) is None}

    def nested(self, spec):
        if spec is None:
            return None
        entry = self.entry(spec)
        name = self.readers[entry]['name'] if entry else spec
        return {'reader': entry, 'name': name, 'walked': entry is not None}


def failed_record(entry, failure):
    return {'name': None, 'class': None, 'walked': False, 'error': failure, 'dispatch': None,
            'complete': False, 'incomplete_because': ['not-walked'], 'rows': [], 'default_routed': []}


def reader_record(reader, locate, tokens, enums):
    routed = sorted(row['token'] for row in reader['rows'] if row['kind'] == 'default-routed')
    table = [row_record(row, locate, tokens, enums) for row in reader['rows'] if row['kind'] != 'default-routed']
    table.sort(key=lambda row: (row['token'] is None, row['token'] or 0))
    base = reader['default']['reader']
    default = {'kind': reader['default']['kind'], 'reader': locate.entry(base) if base else None,
               'followed': bool(base) and locate.entry(base) is not None}
    if base and not default['followed']:
        default['name'] = base
    delegates = [locate.nested(name) for name in reader['delegates']]
    dispatch = {
        'form': reader['form'],
        'domain_enumerated': all(switch['labels_known'] for switch in reader['switches'])
        and all(str(token) in tokens for token in reader['compared']),
        'default': default,
        'delegates': delegates,
        'key_text_read': reader['key_text_read'],
        'loops': reader['loops'],
        'truncated': reader['truncated'],
    }
    return {'name': reader['name'], 'class': rows.owner_class(reader['name']), 'walked': True, 'error': None,
            'dispatch': dispatch, 'complete': None, 'incomplete_because': [], 'rows': table,
            'default_routed': routed}


def row_record(row, locate, tokens, enums):
    record = {field: row.get(field) for field in ROW_FIELDS}
    record['key'] = tokens.get(str(row['token'])) if row['token'] is not None else None
    record['nested'] = locate.nested(row['nested'])
    record['enum'] = enum_record(row, tokens, enums) if row['kind'] == 'enum' else None
    record['lookup'] = None
    if row['branches'] is not None:
        record['branches'] = [dict(branch, nested=locate.nested(branch['nested'])) for branch in row['branches']]
    return record


def enum_record(row, tokens, enums):
    converter = rows.callee(row['call'])['name'] if row.get('call') else None
    table = enums.get(converter) if converter else None
    if not table:
        return None
    return {'converter': converter, 'tokens': table['tokens'],
            'keys': [tokens.get(str(token)) for token in table['tokens']],
            'complete': table['labels_known'], 'unknown': 'logs' if table['logs'] else 'silent'}


def local_conditions(reader):
    """The completeness conditions that a reader fails on its own walk."""
    dispatch = reader['dispatch']
    failed = []
    if dispatch['form'] not in RECOGNIZED_FORMS:
        failed.append('dispatch-form')
    if not dispatch['domain_enumerated']:
        failed.append('domain-not-enumerated')
    default = dispatch['default']
    entry_default = default['kind'] == 'entry' and dispatch['form'] == 'entry'
    if default['kind'] == 'base-reader' and not default['followed']:
        failed.append('default-base-unfollowed')
    elif default['kind'] not in REJECTING_DEFAULTS + ('base-reader',) and not entry_default:
        failed.append('default-unresolved')
    if dispatch['truncated'] or any(row['paths']['truncated'] for row in reader['rows']):
        failed.append('budget')
    if dispatch['loops']:
        failed.append('loop-cut')
    for delegate in delegate_specs(reader):
        if not (delegate and delegate['walked']):
            failed.append('delegate-unfollowed')
    if dispatch['key_text_read'] and dispatch['form'] != 'entry':
        failed.append('key-text-read')
    if any(unresolved_nested(row) for row in reader['rows']):
        failed.append('nested-unresolved')
    return sorted(set(failed))


def unresolved_nested(row):
    """Whether the row, or one of its value branches, reads into an object without a walked reader."""
    return any((outcome['nested'] is not None and not outcome['nested']['walked'])
               or (outcome['kind'] in NESTING_KINDS and outcome['nested'] is None)
               for outcome in [row] + (row['branches'] or []))


def delegate_specs(reader):
    """The member readers that a reader delegates to: the ones its default path calls besides its
    base reader, and the ones a token path calls."""
    rows_delegates = [row['nested'] for row in reader['rows'] if row['kind'] == 'delegates']
    return reader['dispatch']['delegates'] + rows_delegates


def dependencies(reader):
    """The readers whose completeness a reader's own depends on: its default's base reader and its
    delegates."""
    dispatch = reader['dispatch']
    found = []
    if dispatch['default']['kind'] == 'base-reader' and dispatch['default']['reader']:
        found.append(('default-base-incomplete', dispatch['default']['reader']))
    for delegate in delegate_specs(reader):
        if delegate and delegate['walked']:
            found.append(('delegate-incomplete', delegate['reader']))
    return found


def mark_completeness(readers):
    """Set `complete` and `incomplete_because` on every reader. A reader is complete when it passes
    its own conditions and every reader it depends on is complete; readers that depend on each
    other in a cycle are complete together when each passes its own conditions."""
    local = {entry: local_conditions(reader) if reader['walked'] else ['not-walked']
             for entry, reader in readers.items()}
    complete = {entry: not failed for entry, failed in local.items()}
    changed = True
    while changed:
        changed = False
        for entry, reader in readers.items():
            if complete[entry] and reader['walked'] and \
                    any(not complete.get(dependency, False) for _, dependency in dependencies(reader)):
                complete[entry] = False
                changed = True
    for entry, reader in readers.items():
        failed = set(local[entry])
        if reader['walked']:
            failed.update(condition for condition, dependency in dependencies(reader)
                          if not complete.get(dependency, False))
        reader['complete'] = complete[entry]
        reader['incomplete_because'] = sorted(failed)


def registries(roots, locate):
    found = {}
    for registry in roots:
        if 'error' in registry:
            found[registry['registry']] = {'roots': [], 'error': registry['error']}
            continue
        entries = []
        for root in registry['roots']:
            spec = hex(root['address']) if root['address'] is not None else 'class:' + root['class']
            entry = locate.entry(spec)
            entries.append({'key': root['key'], 'class': root['class'], 'reader': entry,
                            'error': None if entry else locate.failure(spec) or 'not walked'})
        found[registry['registry']] = {'roots': entries, 'error': None}
    return dict(sorted(found.items()))


def complete_tree(readers, entry, seen=None):
    """Whether the whole reader tree under `entry` proves its block's key domain: every reader in
    it is complete, rejects the keys it does not enumerate (directly or through a complete base),
    and is not an entry reader."""
    seen = set() if seen is None else seen
    if entry in seen:
        return True
    seen.add(entry)
    reader = readers.get(entry)
    if reader is None or not reader['complete'] or reader['dispatch']['form'] == 'entry':
        return False
    default = reader['dispatch']['default']
    if default['kind'] == 'base-reader' and not complete_tree(readers, default['reader'], seen):
        return False
    nested = [row['nested'] for row in reader['rows']] + \
        [branch['nested'] for row in reader['rows'] for branch in row['branches'] or []]
    return all(complete_tree(readers, spec['reader'], seen) for spec in nested if spec)


def summary(document):
    readers = document['readers']
    registry_trees = {name: bool(registry['roots']) and all(
        root['reader'] and complete_tree(readers, root['reader']) for root in registry['roots'])
        for name, registry in document['registries'].items()}
    reasons = {}
    for reader in readers.values():
        for reason in reader['incomplete_because']:
            reasons[reason] = reasons.get(reason, 0) + 1
    return {
        'readers': len(readers),
        'complete_readers': sum(1 for reader in readers.values() if reader['complete']),
        'registries': len(registry_trees),
        'complete_registry_trees': sum(registry_trees.values()),
        'incomplete_because': dict(sorted(reasons.items(), key=lambda item: -item[1])),
    }


def provenance(export, tables, files, date):
    program = export['program']
    return {
        'schema': SCHEMA,
        'build': {
            'executable': tables['build']['executable'],
            'slice': program['sha256'],
            'version': tables['build']['version'],
            'architecture': 'aarch64' if program['language'].startswith('AARCH64:') else program['language'],
        },
        'schemas': {'readers.json': SCHEMA, 'tables.json': tables['schema'], 'provenance.json': SCHEMA},
        'ghidra': {'version': program['ghidra'], 'language': program['language']},
        'export_scripts': files,
        'date': date,
        'written_by': {'readers.json': 'ghidra-export', 'tables.json': tables['tool'],
                       'provenance.json': 'ghidra-export'},
    }


def write_json(path, document):
    with open(path, 'w') as file:
        json.dump(document, file, indent=1)
        file.write('\n')


def readers_text(document):
    """`readers.json` with one line for each registry and each row, so that a new export's diff
    shows the rows that changed."""
    compact = lambda value: json.dumps(value, separators=(', ', ': '))
    lines = ['{']
    for field in ('schema', 'tool', 'build'):
        lines.append(f' {compact(field)}: {compact(document[field])},')
    lines.append(' "registries": {')
    registries = list(document['registries'].items())
    for index, (name, registry) in enumerate(registries):
        lines.append(f'  {compact(name)}: {compact(registry)}' + (',' if index + 1 < len(registries) else ''))
    lines.append(' },')
    lines.append(' "readers": {')
    readers = list(document['readers'].items())
    for index, (entry, reader) in enumerate(readers):
        header = {field: value for field, value in reader.items() if field != 'rows'}
        lines.append(f'  {compact(entry)}: {compact(header)[:-1]}, "rows": [')
        for row_index, row in enumerate(reader['rows']):
            lines.append(f'   {compact(row)}' + (',' if row_index + 1 < len(reader['rows']) else ''))
        lines.append('  ]}' + (',' if index + 1 < len(readers) else ''))
    lines.append(' }')
    lines.append('}')
    return '\n'.join(lines) + '\n'


def main():
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    export = json.load(open(sys.argv[1]))
    tables = json.load(open(sys.argv[2]))
    directory = sys.argv[3]
    document = normalize(export, tables)
    files = {name: hashlib.sha256(open(os.path.join(HERE, name), 'rb').read()).hexdigest() for name in EXPORT_FILES}

    with open(os.path.join(directory, 'readers.json'), 'w') as file:
        file.write(readers_text(document))
    today = datetime.date.today().isoformat()
    write_json(os.path.join(directory, 'provenance.json'), provenance(export, tables, files, today))
    print(json.dumps(summary(document), indent=1))


if __name__ == '__main__':
    main()
