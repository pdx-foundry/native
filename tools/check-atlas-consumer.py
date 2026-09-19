#!/usr/bin/env python3
"""Verify the frozen Atlas caller, its public Native imports, and its dependency features."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

def verify(caller):
    frozen = json.loads((caller / 'freeze.json').read_text())
    record = json.loads((Path(__file__).resolve().parents[1] / 'docs/native/atlas-consumer-verification.json').read_text())
    assert frozen['caller_revision'] == record['callerRevision'], 'Caller differs from the verified SDK-519 revision'
    assert frozen['native_revision'] == record['nativeRevision'], 'Native differs from the verified SDK-519 revision'
    files = sorted(path for path in caller.rglob('*') if path.is_file()
                   and 'target' not in path.relative_to(caller).parts
                   and path.name != 'freeze.json')
    hashes = {str(path.relative_to(caller)).replace('\\', '/'): hashlib.sha256(path.read_bytes()).hexdigest()
              for path in files}
    assert hashes == frozen['files'], 'Frozen Atlas files changed; review and freeze a new revision'
    revision = hashlib.sha256(json.dumps(hashes, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
    assert revision == frozen['caller_revision'], 'Caller revision does not match its files'
    allowed = {
        'Activation', 'CaptureOrigin', 'Completion', 'Disposal', 'Engine', 'GameError',
        'GameOptions', 'GameReadiness', 'GameReport', 'Native', 'OpenRequest',
        'RegistryAvailability', 'RegistryDescription', 'RegistryError', 'RegistryResult',
        'ReplayError', 'ReplayRequest', 'ResultOrigin', 'supervisor',
    }
    for path in files:
        if path.suffix != '.rs':
            continue
        source = path.read_text()
        for group in re.findall(r'use pdx_native::\{([^}]+)\};', source):
            assert set(re.findall(r'\w+', group)) <= allowed, (path, group)
        for symbol in re.findall(r'pdx_native::(\w+)', source):
            assert symbol in allowed, (path, symbol)
        for symbol in re.findall(r'supervisor::(\w+)', source):
            assert symbol == 'serve', (path, symbol)
        assert not re.search(r'use pdx_native::\*|extern crate|test_support|maintainer_tools|target_os|target_arch', source), path
    metadata = json.loads(subprocess.check_output([
        'cargo', 'metadata', '--locked', '--format-version', '1',
        '--manifest-path', str(caller / 'Cargo.toml'),
    ]))
    consumer = next(package for package in metadata['packages'] if package['name'] == 'atlas-native-consumer')
    dependencies = consumer['dependencies']
    assert {item['name'] for item in dependencies if item['kind'] is None} == {'pdx-native', 'serde', 'serde_json', 'tokio'}
    assert {item['name'] for item in dependencies if item['kind'] == 'dev'} == {'tempfile'}
    assert all(item['kind'] in (None, 'dev') and item['rename'] is None for item in dependencies)
    dependency = next(item for item in dependencies if item['name'] == 'pdx-native')
    assert dependency['source'] == 'git+https://github.com/pdx-foundry/native.git?rev=' + frozen['native_revision']
    assert dependency['features'] == ['production'] and dependency['uses_default_features']
    native = next(package for package in metadata['packages'] if package['name'] == 'pdx-native')
    features = next(node['features'] for node in metadata['resolve']['nodes'] if node['id'] == native['id'])
    assert set(features) == {'default', 'production'}, features
    print(f'Atlas caller {revision}; Native {frozen["native_revision"]}; public imports and production features verified')
    return hashes


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('caller', type=Path, help='Atlas-owned standalone caller directory')
    args = parser.parse_args()
    verify(args.caller)


if __name__ == '__main__':
    main()
