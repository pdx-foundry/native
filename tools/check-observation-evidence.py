#!/usr/bin/env python3
"""Check retained reference and early-observation evidence; never launch a game."""
import hashlib
import json
from pathlib import Path
import sys

ROOT = Path(sys.argv[1]).resolve()
def read(path):
    return json.loads(path.read_text())
def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

reference = ROOT / 'reference-observation-prototype'
for name in ['development-manifest.json', 'heldout-manifest.json']:
    manifest = read(reference / 'evidence' / name)
    for name, row in manifest['artifacts'].items():
        assert sha(reference / 'evidence' / name) == row['sha256'], name
controls = read(reference / 'evidence/controls.json')
assert len(controls) == 27 and all(row['passed'] for row in controls)
print(json.dumps({'referenceArtifactHashes': 'verified', 'recordedControls': len(controls), 'qualification': 'retained controls, not rerun'}))

early = ROOT / 'early-observation-prototype'
matrix = read(early / 'evidence/matrix.json')
checks = []
for row in matrix:
    run = early / 'runs' / row['run']
    manifest, result = read(run / 'manifest.json'), read(run / 'result.json')
    assert result['status'] == row['outcome']
    assert result['disposal']['confirmed']
    assert result['targetUnchanged'] and result['producerContentUnchanged'] and result['protectedUnchanged']
    assert sha(run / 'producer-content.json') == manifest['producerContentManifestSha256']
    sources = {name: digest for name, digest in manifest['probeHashes'].items() if not name.endswith('.dylib')}
    for name, digest in sources.items():
        assert sha(run / 'source' / name) == digest, (row['run'], name)
    owner = read(run / 'owner.json')
    disposal = [event for event in owner if event['kind'] == 'disposal-checked'][-1]
    assert disposal['confirmed'] and disposal['reapedPid'] and disposal['remainingIdentity'] is None
    trace = [json.loads(line) for line in (run / 'trace.jsonl').read_text().splitlines() if line.strip()]
    contiguous = [event['seq'] for event in trace] == list(range(1, len(trace) + 1))
    assert contiguous == result['sequenceContiguous']
    if row['outcome'] == 'complete':
        by_kind = {event['kind']: event for event in trace}
        assert by_kind['launch-stopped']['frames'][0]['function'] == '_dyld_start'
        assert by_kind['launch-stopped']['seq'] < by_kind['hooks-active-before-resume']['seq']
        assert all(hook['resolved'] == 1 and hook['hits'] == 0 for hook in by_kind['hooks-active-before-resume']['hooks'].values())
        fields = [event for event in trace if event['kind'] == 'field-observed']
        assert [(event['field'], event['line']) for event in fields] == [('tree_template', 2), ('traditions', 3)]
        assert len({event['owner'] for event in fields}) == 1
        assert by_kind['stream-end']['producerFieldCount'] == len(fields)
        assert by_kind['phase-complete']['seq'] < by_kind['stream-end']['seq']
    checks.append({'run': row['run'], 'outcome': row['outcome'], 'sourceHashes': len(sources), 'reaped': True})
print(json.dumps({'earlyObservationRetainedMatrix': checks}, indent=2))
