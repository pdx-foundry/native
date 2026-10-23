"""Check a build directory's `readers.json`, `tables.json` and `provenance.json` against their schemas.

Usage: python3 schema.py BUILD_DIRECTORY

Prints each violation with its JSON path and exits with status 1 when there is one. The checks
are the shapes that `normalize.py` and `examples/ghidra-inputs` write, and the references between
the files: every token of a row is in the token table, and every reader that a registry or a row
names is in `readers.json`.
"""
import json
import os
import sys

SCHEMA = 1
FORMS = {'token-switch', 'compare-chain', 'entry', 'key-text', 'value-token'}
DEFAULT_KINDS = {'persistent', 'base-reader', 'log-and-skip', 'return', 'miss', 'helper', 'entry'}
CONDITIONS = {'dispatch-form', 'domain-not-enumerated', 'default-base-unfollowed', 'default-base-incomplete',
              'default-unresolved', 'budget', 'loop-cut', 'delegate-unfollowed', 'delegate-incomplete',
              'key-text-read', 'nested-unresolved', 'not-walked'}
VALUES = {None, 'Scalar', 'Block', 'Any'}
REPEATS = {'Unknown', 'Replace', 'Accumulate'}
OPTIONAL_STRING = (str, type(None))
OPTIONAL_INT = (int, type(None))


class Checker:
    def __init__(self):
        self.errors = []

    def fail(self, path, message):
        self.errors.append(f'{path}: {message}')

    def keys(self, path, value, expected):
        if not isinstance(value, dict):
            self.fail(path, 'not an object')
            return False
        if set(value) != set(expected):
            self.fail(path, f'fields {sorted(set(value) ^ set(expected))} differ from the schema')
            return False
        return True

    def type(self, path, value, types):
        if not isinstance(value, types) or (isinstance(value, bool) and bool not in _tuple(types)):
            self.fail(path, f'{value!r} is not {types}')

    def member(self, path, value, allowed):
        if value not in allowed:
            self.fail(path, f'{value!r} is not one of {sorted(map(str, allowed))}')


def _tuple(types):
    return types if isinstance(types, tuple) else (types,)


def check_tables(check, tables):
    if not check.keys('tables', tables, ['schema', 'tool', 'build', 'tokens', 'scope_types', 'reader',
                                         'manual_exceptions']):
        return
    check.member('tables.schema', tables['schema'], {SCHEMA})
    check.member('tables.tool', tables['tool'], {'native-tables'})
    if check.keys('tables.build', tables['build'], ['executable', 'version']):
        check.type('tables.build.executable', tables['build']['executable'], str)
        check.type('tables.build.version', tables['build']['version'], str)
    for token, name in tables['tokens'].items():
        if not token.isdigit():
            check.fail(f'tables.tokens.{token}', 'not a token identity')
        check.type(f'tables.tokens.{token}', name, str)
    for index, scope in enumerate(tables['scope_types']):
        path = f'tables.scope_types[{index}]'
        if check.keys(path, scope, ['bit', 'name', 'id']):
            check.type(path + '.bit', scope['bit'], int)
            check.type(path + '.name', scope['name'], str)
            check.type(path + '.id', scope['id'], str)
    if check.keys('tables.reader', tables['reader'], ['key_text', 'value_token']):
        check.type('tables.reader.key_text', tables['reader']['key_text'], int)
        check.type('tables.reader.value_token', tables['reader']['value_token'], int)
    exceptions = tables['manual_exceptions']
    if check.keys('tables.manual_exceptions', exceptions, ['zero_mask_read_scope', 'block_value_token']):
        block = exceptions['block_value_token']
        if check.keys('tables.manual_exceptions.block_value_token', block, ['value', 'source', 'written_by']):
            check.type('tables.manual_exceptions.block_value_token.value', block['value'], int)
            check.member('tables.manual_exceptions.block_value_token.written_by', block['written_by'], {'hand'})
        for index, exception in enumerate(exceptions['zero_mask_read_scope']):
            path = f'tables.manual_exceptions.zero_mask_read_scope[{index}]'
            if check.keys(path, exception, ['reader', 'block', 'keys', 'read_scope', 'decision', 'written_by']):
                check.member(path + '.written_by', exception['written_by'], {'hand'})


def check_readers(check, document, tokens):
    if not check.keys('readers', document, ['schema', 'tool', 'build', 'registries', 'readers']):
        return
    check.member('readers.schema', document['schema'], {SCHEMA})
    check.member('readers.tool', document['tool'], {'ghidra-export'})
    readers = document['readers']
    for name, registry in document['registries'].items():
        path = f'registries[{name}]'
        if not check.keys(path, registry, ['roots', 'error']):
            continue
        check.type(path + '.error', registry['error'], OPTIONAL_STRING)
        for index, root in enumerate(registry['roots']):
            root_path = f'{path}.roots[{index}]'
            if check.keys(root_path, root, ['key', 'class', 'reader', 'error']):
                check.type(root_path + '.key', root['key'], str)
                if root['reader'] is not None and root['reader'] not in readers:
                    check.fail(root_path + '.reader', f'{root["reader"]} is not a reader')
                if (root['reader'] is None) == (root['error'] is None):
                    check.fail(root_path, 'exactly one of reader and error')
    for entry, reader in readers.items():
        check_reader(check, f'readers[{entry}]', reader, readers, tokens)


def check_reader(check, path, reader, readers, tokens):
    if not check.keys(path, reader, ['name', 'class', 'walked', 'error', 'dispatch', 'complete',
                                     'incomplete_because', 'rows', 'default_routed']):
        return
    check.type(path + '.complete', reader['complete'], bool)
    for condition in reader['incomplete_because']:
        check.member(path + '.incomplete_because', condition, CONDITIONS)
    if reader['complete'] != (not reader['incomplete_because']):
        check.fail(path, 'complete exactly when no condition fails')
    if not reader['walked']:
        check.type(path + '.error', reader['error'], str)
        return
    dispatch = reader['dispatch']
    if check.keys(path + '.dispatch', dispatch, ['form', 'domain_enumerated', 'default', 'delegates', 'key_text_read',
                                                 'loops', 'truncated']):
        check.member(path + '.dispatch.form', dispatch['form'], FORMS)
        check.type(path + '.dispatch.loops', dispatch['loops'], int)
        default = dispatch['default']
        check.member(path + '.dispatch.default.kind', default.get('kind'), DEFAULT_KINDS)
        if default.get('reader') is not None and default['reader'] not in readers:
            check.fail(path + '.dispatch.default.reader', f'{default["reader"]} is not a reader')
        for index, delegate in enumerate(dispatch['delegates']):
            check_nested(check, f'{path}.dispatch.delegates[{index}]', delegate, readers)
    for token in reader['default_routed']:
        check.type(path + '.default_routed', token, int)
    for index, row in enumerate(reader['rows']):
        check_row(check, f'{path}.rows[{index}]', row, readers, tokens)


def check_row(check, path, row, readers, tokens):
    if not check.keys(path, row, ['token', 'key', 'kind', 'value', 'reader_kind', 'repeat', 'conditional', 'family',
                                  'items', 'branches', 'nested', 'scope_mask', 'enum', 'lookup', 'paths', 'store']):
        return
    check.type(path + '.token', row['token'], OPTIONAL_INT)
    if row['token'] is not None and tokens.get(str(row['token'])) != row['key']:
        check.fail(path + '.key', f'{row["key"]!r} is not the token table\'s name of {row["token"]}')
    check.type(path + '.kind', row['kind'], str)
    check.member(path + '.value', row['value'], VALUES)
    check.member(path + '.repeat', row['repeat'], REPEATS)
    check.type(path + '.conditional', row['conditional'], bool)
    check.type(path + '.scope_mask', row['scope_mask'], OPTIONAL_INT)
    check.member(path + '.lookup', row['lookup'], {None})
    check_nested(check, path + '.nested', row['nested'], readers)
    if check.keys(path + '.paths', row['paths'], ['leaves', 'unknown', 'truncated']):
        check.type(path + '.paths.truncated', row['paths']['truncated'], bool)
    if row['store'] is not None and check.keys(path + '.store', row['store'], ['offset', 'size']):
        check.type(path + '.store.offset', row['store']['offset'], int)
        check.type(path + '.store.size', row['store']['size'], OPTIONAL_INT)
    if row['enum'] is not None and check.keys(path + '.enum', row['enum'],
                                              ['converter', 'tokens', 'keys', 'complete', 'unknown']):
        check.member(path + '.enum.unknown', row['enum']['unknown'], {'logs', 'silent'})
    for index, branch in enumerate(row['branches'] or []):
        branch_path = f'{path}.branches[{index}]'
        if check.keys(branch_path, branch, ['value_token', 'kind', 'value', 'reader_kind', 'repeat', 'family',
                                            'items', 'nested', 'proved', 'paths']):
            check.member(branch_path + '.value_token', branch['value_token'], {'block', 'other'})
            check.member(branch_path + '.value', branch['value'], VALUES)
            check.member(branch_path + '.repeat', branch['repeat'], REPEATS)
            check.type(branch_path + '.proved', branch['proved'], bool)
            check_nested(check, branch_path + '.nested', branch['nested'], readers)


def check_nested(check, path, nested, readers):
    if nested is None or not check.keys(path, nested, ['reader', 'name', 'walked']):
        return
    if nested['walked'] != (nested['reader'] is not None) or (nested['reader'] and nested['reader'] not in readers):
        check.fail(path, 'a walked nested reader names a reader of the file, and only then')


def check_provenance(check, provenance):
    if not check.keys('provenance', provenance, ['schema', 'build', 'schemas', 'ghidra', 'export_scripts', 'date',
                                                 'written_by']):
        return
    if check.keys('provenance.build', provenance['build'], ['executable', 'slice', 'version', 'architecture']):
        for field in ('executable', 'slice'):
            value = provenance['build'][field]
            if not (isinstance(value, str) and len(value) == 64):
                check.fail(f'provenance.build.{field}', 'not a SHA-256')
    check.member('provenance.schemas', provenance['schemas'],
                 [{'readers.json': SCHEMA, 'tables.json': SCHEMA, 'provenance.json': SCHEMA}])
    for name, digest in provenance['export_scripts'].items():
        if not (isinstance(digest, str) and len(digest) == 64):
            check.fail(f'provenance.export_scripts.{name}', 'not a SHA-256')


def check_directory(directory):
    check = Checker()
    load = lambda name: json.load(open(os.path.join(directory, name)))
    tables = load('tables.json')
    check_tables(check, tables)
    check_readers(check, load('readers.json'), tables.get('tokens', {}))
    provenance = load('provenance.json')
    check_provenance(check, provenance)
    if provenance.get('build', {}).get('executable') != tables.get('build', {}).get('executable'):
        check.fail('provenance.build.executable', 'differs from tables.build.executable')
    return check.errors


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    errors = check_directory(sys.argv[1])
    for error in errors:
        print(error)
    if errors:
        sys.exit(1)
    print('schema check passed')


if __name__ == '__main__':
    main()
