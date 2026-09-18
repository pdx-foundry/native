#!/usr/bin/env python3
"""Explicit real-game candidate observation controls. Never substitutes a synthetic target."""
import argparse
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('lifecycle_controls', ROOT / 'tools/check-candidate-lifecycle.py')
lifecycle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(lifecycle)
SCENARIOS = {
    'normal': 'complete', 'missing-hook': 'unavailable', 'late-hook': 'unavailable',
    'dropped-record': 'incomplete', 'missing-terminal': 'incomplete',
    'access-failure': 'unavailable', 'worker-loss': 'worker-lost',
    'cancel': None, 'caller-loss': None, 'timeout': None,
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('installation', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--scenario', choices=SCENARIOS)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(mode=0o700, exist_ok=False)
    subprocess.run(['cargo', 'build', '--locked', '--release', '--features', 'maintainer-tools', '--example', 'observe'], cwd=ROOT, check=True)
    source = output / 'source'
    source.mkdir()
    for name in ('src', 'crates', 'examples'):
        shutil.copytree(ROOT / name, source / name)
    for name in ('Cargo.toml', 'Cargo.lock', 'build.rs'):
        shutil.copyfile(ROOT / name, source / name)
    shutil.copyfile(ROOT / 'target/release/examples/observe', output / 'observe')
    fixture = ROOT / 'tests/fixtures/candidate/category.txt'
    ordinary = Path.home() / 'Documents/Paradox Interactive/Stellaris'
    sentinel = subprocess.Popen(['/bin/sleep', '3600'])
    try:
        for scenario in ([args.scenario] if args.scenario else SCENARIOS):
            before = lifecycle.snapshot(ordinary)
            (output / f'{scenario}.ordinary-before.json').write_text(json.dumps(before, indent=2))
            started = time.monotonic()
            try:
                run = subprocess.run([str(ROOT / 'target/release/examples/observe'), str(args.installation), str(output / scenario), str(fixture), scenario], text=True, capture_output=True, timeout=240)
                (output / f'{scenario}.stdout').write_text(run.stdout)
                (output / f'{scenario}.stderr').write_text(run.stderr)
                if run.returncode:
                    raise AssertionError(run.stderr)
                report = lifecycle.wait_report(output / scenario / 'report.json')
                assert report['disposal'] == 'Reaped' and report['reservation_resolved'], report
                assert not report['diagnostics'], report
                assert report['replay'], report
                evidence = output / scenario / 'evidence'
                replay = json.loads((evidence / 'replay.json').read_text())
                assert replay['disposal'] == 'confirmed', replay
                assert replay['capture_origin'] == 'captured' and replay['origin'] == 'replay'
                expected = SCENARIOS[scenario]
                if expected:
                    assert replay['completion'] == expected, replay
                if scenario == 'normal':
                    assert replay['activation'] == 'demonstrated' and len(replay['observations']) == 5, replay
                if scenario in ('missing-hook', 'late-hook'):
                    rows = [json.loads(line) for line in (evidence / 'raw-trace.jsonl').read_text().splitlines()]
                    assert all(row['kind'] != 'resume' for row in rows), rows
                if scenario in ('cancel', 'caller-loss', 'timeout'):
                    assert report['outcome'] == {'cancel':'Cancelled', 'caller-loss':'CallerLost', 'timeout':'TimedOut'}[scenario], report
                public = subprocess.run(['cargo', 'run', '--quiet', '--example', 'replay', '--', str(evidence), str(evidence / 'descriptor.ref.json')], cwd=ROOT, capture_output=True, text=True, check=True)
                (output / f'{scenario}.public-replay.json').write_text(public.stdout)
                print(f'{scenario}: {replay["completion"]}, independent disposal confirmed', flush=True)
            finally:
                after = lifecycle.snapshot(ordinary)
                (output / f'{scenario}.ordinary-after.json').write_text(json.dumps(after, indent=2))
                (output / f'{scenario}.timing.json').write_text(json.dumps({'wallSeconds': time.monotonic()-started}))
                assert before == after, 'ordinary profile changed'
                assert sentinel.poll() is None, 'unrelated process was terminated'
    finally:
        sentinel.terminate()
        sentinel.wait(timeout=5)


if __name__ == '__main__':
    main()
