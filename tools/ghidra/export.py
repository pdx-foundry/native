"""Export the member readers of one build from its Ghidra project into a raw export.

Usage:
    python3 export.py --project PROJECT.gpr --program PROGRAM --tables tables.json \
        --roots roots.json --conformance conformance/m452.json --out OUTDIR

`examples/ghidra-inputs` writes `tables.json` and `roots.json`. The export walks each registry's
root readers, then in rounds every reader that a walked reader reaches: a row's nested reader, a
delegate, and the base reader of the default path. A class without its own `ReadMember` is found
through the vtables that its constructor stores (slot 5). The conformance file's readers are
walked as roots. OUTDIR receives the Ghidra query lists, their answers and `export.json`, the raw
export that `normalize.py` turns into `readers.json`. OUTDIR is a local cache: never commit it.

Environment: GHIDRA_INSTALL_DIR (default ~/Applications/ghidra_12.1.4_PUBLIC), JAVA_HOME
(default Homebrew's openjdk@25) and GHIDRA_HEADLESS_MAXMEM (default 8G).
"""
import argparse
import json
import os
import subprocess
import sys
import time

import rows

HERE = os.path.dirname(os.path.abspath(__file__))
# The lexer kind of a value token that opens a block, `{` (docs/native/modifier-blocks.md).
BLOCK_VALUE_TOKEN = 3
QUERIES_PER_RUN = 300
# Rounds after which the export stops following nested readers; deeper readers stay unwalked.
MAX_ROUNDS = 12


class Ghidra:
    """Runs Export.java over a query list against a read-only project."""

    def __init__(self, project, program, tables, outdir):
        self.project_dir, self.project = os.path.split(os.path.abspath(project))
        self.project = self.project.removesuffix('.gpr')
        self.program = program
        self.reader = tables['reader']
        self.outdir = outdir
        self.runs = 0

    def ask(self, queries, label):
        answers = {}
        queries = sorted(set(queries))
        for start in range(0, len(queries), QUERIES_PER_RUN):
            part = os.path.join(self.outdir, f'{label}-{start // QUERIES_PER_RUN}')
            with open(part + '.list', 'w') as listing:
                listing.write('\n'.join(queries[start:start + QUERIES_PER_RUN]) + '\n')
            self.run(part)
            for line in open(part + '.out'):
                answer = json.loads(line)
                answers[answer['query']] = answer
        return answers

    def run(self, part):
        ghidra = os.environ.get('GHIDRA_INSTALL_DIR', os.path.expanduser('~/Applications/ghidra_12.1.4_PUBLIC'))
        environment = dict(os.environ)
        environment.setdefault('JAVA_HOME', '/opt/homebrew/opt/openjdk@25/libexec/openjdk.jdk/Contents/Home')
        environment.setdefault('GHIDRA_HEADLESS_MAXMEM', '8G')
        command = [os.path.join(ghidra, 'support', 'analyzeHeadless'), self.project_dir, self.project,
                   '-process', self.program, '-noanalysis', '-readOnly', '-scriptPath', HERE,
                   '-postScript', 'Export.java', f'out={part}.out', f'list={part}.list',
                   f'key_text={self.reader["key_text"]}', f'value_token={self.reader["value_token"]}',
                   f'block_token={BLOCK_VALUE_TOKEN}']
        with open(part + '.log', 'w') as log:
            subprocess.run(command, check=True, stdout=log, stderr=subprocess.STDOUT, env=environment)
        if 'SCRIPT ERROR' in open(part + '.log', errors='replace').read():
            sys.exit(f'Export.java failed; see {part}.log')
        self.runs += 1


def constructor_map(constructors):
    """The nested reader spec at each owner offset that a class's constructors give: a member
    constructor's class, else the first vtable stored there."""
    mapping = {}
    for constructor in constructors:
        for store in constructor['vtables']:
            mapping.setdefault(store['offset'], 'vt:' + store['address'])
        for member in constructor['members']:
            if member['offset'] != 0:
                mapping[member['offset']] = 'class:' + member['class']
    return mapping


def primary_vtables(constructors):
    return sorted({store['address'] for constructor in constructors for store in constructor['vtables']})


def walk_reader(walk, constructors, yes_token):
    """The rows, branches and dispatch facts of one reader walk."""
    walks = walk['walks']
    every = [event for token_walk in walks.values() for event in token_walk['events']]
    form = form_of(walk, every)
    entry_form = form == 'entry'
    reference = [text for _, text in walks['default']['events']] if form != 'value-token' and not entry_form else []

    token_rows = [token_row(int(label), token_walk, walks, reference, constructors, yes_token)
                  for label, token_walk in walks.items() if label != 'default' and '@' not in label]
    if entry_form:
        token_rows.append(token_row(None, walks['default'], walks, [], constructors, yes_token))
    kind, base = ('entry', None) if entry_form else rows.default_kind(walks['default']['events'])
    delegates = [name for name in rows.member_calls(walks['default']['events']) if name != base]
    token_delegates = sorted({name for label, token_walk in walks.items() if label != 'default'
                              for name in rows.member_calls(token_walk['events'])
                              if name not in (base, walk['name'])} - set(delegates))
    loops = rows.loop_bodies(every)
    return {
        'name': walk['name'], 'entry': walk['entry'], 'source': walk['source'], 'form': form,
        'switches': walk['switches'], 'compared': walk['compared'],
        'default': {'kind': kind, 'reader': base},
        'delegates': delegates + token_delegates,
        'token_delegates': token_delegates,
        'key_text_read': any(text == 'KEYTEXT' for _, text in every),
        'loops': sum(1 for body in loops if not rows.recognized_loop(body)),
        'truncated': any(token_walk['truncated'] for token_walk in walks.values()),
        'rows': token_rows,
    }


def form_of(walk, events):
    """The reader's dispatch form. A key reader that compares no token and reads the key's text is
    an entry reader when it neither compares nor looks up that text; otherwise its keys are text
    that the export does not enumerate."""
    if walk['source'] == 'value':
        return 'value-token'
    if not walk['compared'] and any(text == 'KEYTEXT' for _, text in events):
        return 'key-text' if rows.compares_text(events) or rows.looks_up_text(events) else 'entry'
    return 'token-switch' if walk['switches'] else 'compare-chain'


def token_row(token, token_walk, walks, reference, constructors, yes_token):
    result = rows.classify(token_walk['events'], reference, constructors, yes_token)
    result.update(token=token, paths={'leaves': token_walk['leaves'], 'unknown': token_walk['unknown'],
                                      'truncated': token_walk['truncated']},
                  store=rows.store_of(token_walk['events'], result), scope_mask=rows.scope_mask_of(result),
                  branches=None)
    if token is not None and f'{token}@block' in walks:
        result['branches'] = [
            branch_outcome(value_token, walks[f'{token}@{value_token}'], reference, constructors, yes_token)
            for value_token in ('block', 'other')]
    return result


def branch_outcome(value_token, branch_walk, reference, constructors, yes_token):
    outcome = rows.classify(branch_walk['events'], reference, constructors, yes_token)
    return {'value_token': value_token, 'kind': outcome['kind'], 'value': outcome['value'],
            'reader_kind': outcome['reader_kind'], 'repeat': outcome['repeat'], 'nested': outcome['nested'],
            'paths': {'leaves': branch_walk['leaves'], 'unknown': branch_walk['unknown'],
                      'truncated': branch_walk['truncated']}}


def nested_specs(reader):
    """The readers that one walked reader leads the export to: the nested readers of its rows and
    branches, the member readers that its token paths call, and the base reader of its default path.
    The export does not follow the other readers that an unresolved default calls."""
    specs = set(reader['token_delegates'])
    for token_row in reader['rows']:
        candidates = [token_row['nested']] + [branch['nested'] for branch in token_row['branches'] or []]
        specs.update(spec for spec in candidates if spec and not spec.startswith('vt:'))
    if reader['default']['reader']:
        specs.add(reader['default']['reader'])
    return {spec for spec in specs if rows.followed(spec)}


def root_specs(roots, extra):
    specs = set(extra)
    for registry in roots:
        for root in registry.get('roots', []):
            specs.add(hex(root['address']) if root['address'] is not None else 'class:' + root['class'])
    return specs


class Rounds:
    """The state of an export: what was walked, resolved and read from the project so far."""

    def __init__(self, ghidra, yes_token):
        self.ghidra = ghidra
        self.yes_token = yes_token
        self.readers = {}       # entry address -> reader
        self.walked = {}        # walked spec -> entry address, or None when the walk failed
        self.resolved = {}      # class spec -> member reader entry, or None
        self.constructors = {}  # class -> its constructors
        self.slots = {}         # vtable address -> (entry, name) of the function in slot 5
        self.failures = {}      # spec -> why it was not walked
        self.timings = []

    def run(self, specs):
        pending = {spec: 0 for spec in specs}
        for round_number in range(MAX_ROUNDS):
            started = time.time()
            self.resolve_classes({spec for spec in pending if spec.startswith('class:')}, round_number)
            batch = self.batch(pending)
            if not batch:
                return
            pending = self.walk(batch, round_number)
            self.timings.append({'round': round_number, 'readers': len(batch),
                                 'seconds': round(time.time() - started, 1)})
            print(f'round {round_number}: {len(batch)} readers in {time.time() - started:.0f}s', flush=True)
        for spec in pending:
            self.failures.setdefault(spec, f'not walked: the export stops after {MAX_ROUNDS} rounds')

    def target(self, spec):
        return self.resolved.get(spec) if spec.startswith('class:') else spec

    def resolve_classes(self, specs, round_number):
        """Find each class's member reader by name, else in slot 5 of a vtable that a constructor stores."""
        specs = {spec for spec in specs if spec not in self.resolved}
        if not specs:
            return
        names = self.ghidra.ask(['names:' + spec[6:] + '::ReadMember' for spec in specs], f'r{round_number}-names')
        unresolved = []
        for spec in specs:
            found = names.get('names:' + spec[6:] + '::ReadMember', {}).get('result') or []
            if found:
                self.resolved[spec] = max(found, key=lambda function: function['size'])['entry']
            else:
                unresolved.append(spec[6:])
        self.need_constructors(unresolved, round_number)
        self.need_slots({vtable for cls in unresolved for vtable in primary_vtables(self.constructors[cls])},
                        round_number)
        for cls in unresolved:
            targets = [self.slots[vtable] for vtable in primary_vtables(self.constructors[cls])]
            found = [entry for entry, name in targets if entry and name and name.endswith('::ReadMember')]
            self.resolved['class:' + cls] = found[0] if found else None

    def batch(self, pending):
        """The walk targets of the pending specs that are not walked yet, each at its least depth."""
        batch = {}
        for spec, depth in pending.items():
            target = self.target(spec)
            if target is None:
                self.failures[spec] = 'no member reader found by name or by vtable slot 5'
            elif target not in self.walked:
                batch[target] = min(depth, batch.get(target, depth))
        return batch

    def walk(self, batch, round_number):
        """Walk a batch of readers and return the specs that their rows lead to."""
        answers = self.ghidra.ask(['paths:' + target for target in batch], f'r{round_number}-paths')
        walks = {}
        for target in batch:
            answer = answers.get('paths:' + target, {})
            if 'result' in answer:
                walks[target] = answer['result']
            else:
                self.walked[target] = None
                self.failures[target] = answer.get('error', 'no answer')
        owners = {rows.owner_class(walk['name']) for walk in walks.values()}
        self.need_constructors(owners, round_number)
        self.need_slots({spec[3:] for cls in owners for spec in constructor_map(self.constructors[cls]).values()
                         if spec.startswith('vt:')}, round_number)

        pending = {}
        for target, walk in walks.items():
            self.walked[target] = walk['entry']
            if walk['entry'] in self.readers:
                continue
            reader = walk_reader(walk, self.member_readers(walk['name']), self.yes_token)
            reader['depth'] = batch[target]
            self.readers[walk['entry']] = reader
            for spec in nested_specs(reader):
                pending.setdefault(spec, batch[target] + 1)
        return {spec: depth for spec, depth in pending.items() if self.target(spec) not in self.walked}

    def member_readers(self, reader_name):
        """The nested reader at each owner offset of a reader's class: a member constructor's
        class, or the member reader in slot 5 of the vtable that a constructor stores there."""
        members = {}
        for offset, spec in constructor_map(self.constructors.get(rows.owner_class(reader_name), [])).items():
            if not spec.startswith('vt:'):
                members[offset] = spec
                continue
            entry, name = self.slots.get(spec[3:], (None, None))
            if entry and name and name.endswith('::ReadMember') and not name.startswith('CPersistent::'):
                members[offset] = entry
        return members

    def need_constructors(self, classes, round_number):
        missing = sorted(cls for cls in classes if cls and cls not in self.constructors)
        answers = self.ghidra.ask(['ctors:' + cls for cls in missing], f'r{round_number}-ctors') if missing else {}
        for cls in missing:
            self.constructors[cls] = answers.get('ctors:' + cls, {}).get('result') or []

    def need_slots(self, vtables, round_number):
        missing = sorted(vtable for vtable in vtables if vtable not in self.slots)
        answers = self.ghidra.ask([f'vtslot:{vtable}:5' for vtable in missing], f'r{round_number}-vt') \
            if missing else {}
        for vtable in missing:
            result = answers.get(f'vtslot:{vtable}:5', {}).get('result') or {}
            self.slots[vtable] = (result.get('target'), result.get('name'))

    def token_converters(self):
        """The token tables of the converters that enum rows call, by converter name."""
        converters = sorted({rows.callee(token_row['call'])['name'] for reader in self.readers.values()
                             for token_row in reader['rows'] if token_row['kind'] == 'enum' and token_row['call']})
        answers = self.ghidra.ask(['enum:' + name for name in converters], 'enums') if converters else {}
        return {name: answers.get('enum:' + name, {}).get('result') for name in converters}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    for option in ('--project', '--program', '--tables', '--roots', '--conformance', '--out'):
        parser.add_argument(option, required=True)
    arguments = parser.parse_args()
    os.makedirs(arguments.out, exist_ok=True)
    started = time.time()

    tables = json.load(open(arguments.tables))
    roots = json.load(open(arguments.roots))
    conformance = json.load(open(arguments.conformance))
    yes_token = next(int(token) for token, name in tables['tokens'].items() if name == 'yes')
    ghidra = Ghidra(arguments.project, arguments.program, tables, arguments.out)
    program = ghidra.ask(['program'], 'program')['program']['result']

    rounds = Rounds(ghidra, yes_token)
    rounds.run(root_specs(roots, [expectation['reader'] for expectation in conformance['readers']]))
    export = {
        'program': program, 'registries': roots, 'resolved': rounds.resolved, 'walked': rounds.walked,
        'failures': rounds.failures, 'readers': rounds.readers, 'enums': rounds.token_converters(),
        'timings': rounds.timings, 'seconds': round(time.time() - started, 1), 'ghidra_runs': ghidra.runs,
    }
    with open(os.path.join(arguments.out, 'export.json'), 'w') as file:
        json.dump(export, file, indent=1)
    print(f'{len(rounds.readers)} readers, {len(rounds.failures)} not walked, {time.time() - started:.0f}s')


if __name__ == '__main__':
    main()
