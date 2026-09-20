#!/usr/bin/env python3
"""Verify SDK-487 comparison evidence and copy it after Rust discovery has been frozen."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
BUNDLE = 'atlas-discovery'
PREFIX = 'prototype/council-agenda-reconstruction/evidence/dispatch/'


def reference(path, name):
    raw = path.read_bytes()
    return dict(path=name, bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest())


def prepare(output):
    output = output.resolve()
    output.relative_to(ROOT / '.local/evidence')
    assert (output / 'qualification.json').is_file(), 'Freeze Rust discovery before comparison'
    subprocess.run(['python3', 'tools/evidence.py', BUNDLE], cwd=ROOT, check=True)
    manifest = json.loads((ROOT / '.local/evidence/bundles' / (BUNDLE + '.manifest.json')).read_text())
    files = {row['path']: row for row in manifest['files']}
    retained = ROOT / '.local/evidence/restored' / BUNDLE
    destination = output / 'comparison'
    destination.mkdir(exist_ok=False)

    def copy(name, relative=None):
        relative = relative or PREFIX + name
        row = files[relative]
        raw = (retained / relative).read_bytes()
        assert len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256'], relative
        (destination / name).write_bytes(raw)
        return reference(destination / name, name)

    references = {name: copy(name) for name in ['inventory.json', 'manifest.json', 'tokens.json']}
    references['registry-contract.json'] = copy('registry-contract.json', 'prototype/council-agenda-reconstruction/registry.json')
    captured = json.loads((destination / 'manifest.json').read_text())
    for function in captured['functions'].values():
        name = function['artifact']
        references[name] = copy(name)
        assert references[name]['sha256'] == function['sha256']
    (destination / 'references.json').write_text(json.dumps(references, indent=2) + '\n')
    return references


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    print(json.dumps(prepare(parser.parse_args().output), indent=2))
