#!/usr/bin/env python3
"""Prepare replay inputs from a new static capture and the verified SDK-489 capsule."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
RETAINED = ROOT / '.local/evidence/restored/atlas-ownership/prototype/registry-ownership'
RUNS = ['runs/20260917-185056', 'runs/20260917-185330']


def reference(path, name):
    data = path.read_bytes()
    return dict(path=name, bytes=len(data), sha256=hashlib.sha256(data).hexdigest())


def prepare(output):
    subprocess.run(['python3', 'tools/evidence.py', 'atlas-ownership'], cwd=ROOT, check=True)
    capsule = json.loads((RETAINED / 'capsule.json').read_text())
    for name, expected in capsule['files'].items():
        assert hashlib.sha256((RETAINED / name).read_bytes()).hexdigest() == expected, name
    subprocess.run(['python3', str(RETAINED / 'replay.py')], cwd=ROOT, check=True)
    descriptor = json.loads((output / 'descriptor.json').read_text())
    descriptor['runs'] = []
    shutil.copyfile(RETAINED / 'capsule.json', output / 'sdk-489-capsule.json')
    for run in RUNS:
        record = dict(identity=run, capsule=reference(output / 'sdk-489-capsule.json', 'sdk-489-capsule.json'))
        for field, filename in [('trace', 'trace.jsonl'), ('table', 'startup-table.json'), ('result', 'result.json'), ('manifest', 'manifest.json')]:
            name = f'{run}/{filename}'
            destination = output / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(RETAINED / name, destination)
            record[field] = reference(destination, name)
        descriptor['runs'].append(record)
    path = output / 'historical-descriptor.json'
    path.write_text(json.dumps(descriptor, indent=2) + '\n')
    (output / 'historical.ref.json').write_text(json.dumps(reference(path, path.name), indent=2) + '\n')
    return descriptor


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    prepare(args.output.resolve())
