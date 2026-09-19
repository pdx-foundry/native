#!/usr/bin/env python3
"""Run Atlas's production flow and externally fail one snapshot's retention. Launches games."""
import argparse
import json
from pathlib import Path
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=Path)
    parser.add_argument('installation', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    root = args.output.resolve()
    root.mkdir(exist_ok=False, parents=True)
    retention = root / 'captures'
    retention.mkdir()
    with (root / 'result.json').open('w') as stdout, (root / 'stderr').open('w') as stderr:
        process = subprocess.Popen([str(args.binary.resolve()), 'live', str(args.installation), str(retention)],
                                   stdout=stdout, stderr=stderr)
        # Native owns the format and this fault control. Atlas never reads supervisor internals.
        deadline = time.monotonic() + 180
        injected = False
        try:
            while process.poll() is None and time.monotonic() < deadline:
                attempts = list(retention.glob('game-*'))
                if attempts:
                    destination = attempts[0] / 'snapshots/traditions'
                    destination.mkdir(parents=True, exist_ok=False)
                    (destination / 'descriptor.json').write_text('authored retention failure control')
                    injected = True
                    break
                time.sleep(.01)
            assert injected, 'No attempt available for retention control'
            status = process.wait(timeout=260)
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=60)
    assert status == 0, (root / 'stderr').read_text()
    output = json.loads((root / 'result.json').read_text())
    assert 'ObservationUnavailable' in output['queries']['traditions']['Err']
    assert output['queries']['tradition_categories']['Ok']['native']['completion'] == 'complete'
    assert output['startup']['availability']['traditions']['Unavailable']['diagnostics']
    assert output['termination']['Ok']['disposal'] == 'Reaped'
    assert output['termination']['Ok']['reservation_resolved']
    assert set(output['replay']['startup']) == {'tradition_categories'}
    assert set(output['replay']['final_snapshots']) == {'traditions', 'tradition_categories'}
    plan = root / 'replay-plan.json'
    plan.write_text(json.dumps(output['replay']))
    replay = subprocess.run([str(args.binary.resolve()), 'replay', str(plan)], text=True, capture_output=True, check=True)
    (root / 'replay.json').write_text(replay.stdout)
    replayed = json.loads(replay.stdout)
    for phase, original in [('startup', output['queries']), ('final_snapshots', output['final_observations'])]:
        for name, result in replayed[phase].items():
            assert result['Ok']['native'] == dict(original[name]['Ok']['native'], origin='replay')
    print('Per-registry retention failure: other query, close, final recovery, and replay passed')


if __name__ == '__main__':
    main()
