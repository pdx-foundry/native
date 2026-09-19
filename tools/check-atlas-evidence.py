#!/usr/bin/env python3
"""Replay SDK-521's retained controls through the frozen Atlas executable, without launching games."""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=Path)
    parser.add_argument('evidence', type=Path)
    args = parser.parse_args()
    cases = ['normal-traditions', 'missing-hook-traditions', 'missing-hook-tradition_categories',
             'access-failure-traditions', 'access-failure-tradition_categories',
             'dropped-record-traditions', 'dropped-record-tradition_categories',
             'worker-loss-traditions', 'worker-loss-tradition_categories']
    for case in cases:
        attempts = list((args.evidence / case).glob('game-*'))
        assert len(attempts) == 1, (case, attempts)
        attempt = attempts[0]
        plan = {'startup': {}, 'final_snapshots': {}}
        expected = {}
        for phase, folder in [('startup', 'snapshots'), ('final_snapshots', 'final')]:
            for name in ['traditions', 'tradition_categories']:
                root = attempt / folder / name
                if not root.exists():
                    assert phase == 'startup' and case.startswith('worker-loss'), root
                    continue
                plan[phase][name] = {'artifact_root': str(root.resolve()),
                                     'descriptor': json.loads((root / 'descriptor.ref.json').read_text())}
                expected[(phase, name)] = json.loads((root / 'replay.json').read_text())
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'plan.json'
            path.write_text(json.dumps(plan))
            process = subprocess.run([str(args.binary.resolve()), 'replay', str(path)],
                                     text=True, capture_output=True, check=True)
        result = json.loads(process.stdout)
        for (phase, name), native in expected.items():
            assert result[phase][name]['Ok']['native'] == native, (case, phase, name)
            assert result[phase][name]['Ok']['gaps'], (case, phase, name)
        print(json.dumps({'case': case, 'verified_snapshots': len(expected)}))


if __name__ == '__main__':
    main()
