#!/usr/bin/env python3
"""Real-game production consumer controls. Requires reviewed admission; never installs an acceptance."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('lifecycle', ROOT / 'tools/check-candidate-lifecycle.py')
lifecycle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(lifecycle)


def intervene(output, scenario, process):
    """Faults belong to this maintainer harness, never to the production request."""
    deadline = time.monotonic() + 120
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise AssertionError('consumer ended before external control')
        trace = output / 'raw-trace.jsonl'
        worker = output / 'worker-owned.json'
        if trace.exists() and worker.exists():
            rows = trace.read_bytes().splitlines(keepends=True)
            if any(b'"registry-load-start"' in row for row in rows):
                identity = json.loads(worker.read_text())
                pid = identity['pid']
                # Native owns and retains this unreaped worker identity until cleanup.
                if scenario == 'worker-loss':
                    os.kill(pid, signal.SIGKILL)
                else:
                    os.kill(pid, signal.SIGSTOP)
                    try:
                        stop_deadline = time.monotonic() + 2
                        while True:
                            state = subprocess.check_output(['/bin/ps', '-o', 'stat=', '-p', str(pid)], text=True).strip()
                            if state.startswith('T'):
                                break
                            assert time.monotonic() < stop_deadline, 'worker did not stop for corruption control'
                            time.sleep(0.01)
                        rows = trace.read_bytes().splitlines(keepends=True)
                        assert not any(b'"registry-end"' in row for row in rows), 'control arrived too late'
                        index = next(i for i, row in enumerate(rows) if b'"hooks-requested"' in row)
                        removed = rows.pop(index)
                        (output / 'external-dropped-record.json').write_bytes(removed)
                        trace.write_bytes(b''.join(rows))
                    finally:
                        os.kill(pid, signal.SIGCONT)
                (output / 'external-control.json').write_text(json.dumps({'scenario': scenario, 'worker': identity}))
                return
        time.sleep(0.01)
    raise AssertionError('external control deadline elapsed')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('installation', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--registry', choices=['traditions', 'tradition_categories'], required=True)
    args = parser.parse_args()
    root = args.output.resolve()
    root.mkdir(mode=0o700, exist_ok=False)
    subprocess.run(['cargo', 'build', '--locked', '--release', '--features', 'production', '--example', 'live'], cwd=ROOT, check=True)
    binary = root / 'live'
    shutil.copyfile(ROOT / 'target/release/examples/live', binary)
    for name in ('src', 'crates', 'examples'):
        shutil.copytree(ROOT / name, root / 'source' / name, ignore=shutil.ignore_patterns('__pycache__', '.DS_Store'))
    for name in ('Cargo.toml', 'Cargo.lock', 'build.rs'):
        shutil.copyfile(ROOT / name, root / 'source' / name)
    ordinary = Path.home() / 'Documents/Paradox Interactive/Stellaris'
    sentinel = subprocess.Popen(['/bin/sleep', '3600'])
    try:
        for scenario in ('normal', 'unsupported', 'incomplete', 'cancel', 'caller-loss', 'timeout', 'worker-loss'):
            before = lifecycle.snapshot(ordinary)
            (root / f'{scenario}.ordinary-before.json').write_text(json.dumps(before))
            retention = root / scenario
            retention.mkdir()
            output = None
            mode = scenario if scenario in ('cancel', 'caller-loss', 'timeout') else 'normal'
            registry = 'technology' if scenario == 'unsupported' else args.registry
            started = time.monotonic()
            try:
                with (root / f'{scenario}.stdout').open('w') as stdout, (root / f'{scenario}.stderr').open('w') as stderr:
                    process = subprocess.Popen([str(binary), str(args.installation), str(retention), registry, mode], stdout=stdout, stderr=stderr)
                    if scenario != 'unsupported':
                        deadline = time.monotonic() + 30
                        while output is None:
                            attempts = list(retention.glob('registry-*'))
                            if attempts:
                                assert len(attempts) == 1
                                output = attempts[0]
                                break
                            assert process.poll() is None and time.monotonic() < deadline, 'consumer refused or allocation timed out'
                            time.sleep(0.01)
                    if scenario in ('incomplete', 'worker-loss'):
                        intervene(output, scenario, process)
                    status = process.wait(timeout=240)
                if scenario == 'unsupported':
                    assert status != 0 and not list(retention.iterdir())
                    assert 'Unsupported' in (root / f'{scenario}.stderr').read_text()
                    print('unsupported: unknown registry refused before allocation', flush=True)
                    continue
                assert status == 0, (root / f'{scenario}.stderr').read_text()
                owner = lifecycle.wait_report(output / 'report.json')
                assert owner['origin'] == 'qualified-live'
                assert owner['disposal'] == 'Reaped' and owner['reservation_resolved'] and not owner['diagnostics'], owner
                reference = output / 'evidence/descriptor.ref.json'
                replay_process = subprocess.run(['cargo', 'run', '--quiet', '--example', 'registry-replay', '--', str(output / 'evidence'), str(reference)], cwd=ROOT, check=True, text=True, capture_output=True)
                replay = json.loads(replay_process.stdout)
                (root / f'{scenario}.replay.json').write_text(replay_process.stdout)
                assert replay['origin'] == 'replay' and replay['disposal'] == 'confirmed', replay
                if scenario != 'caller-loss':
                    live = json.loads((root / f'{scenario}.stdout').read_text())
                    assert live['result']['Ok']['origin'] == 'live', live
                    normalized = dict(live['result']['Ok'], origin='replay')
                    assert normalized == replay, 'live/replay observation contract differs'
                if scenario == 'normal':
                    assert replay['activation'] == 'demonstrated' and replay['completion'] == 'complete' and len(replay['registeredItems']) > 0
                if scenario == 'incomplete':
                    assert replay['completion'] == 'incomplete', replay
                if scenario == 'worker-loss':
                    assert replay['completion'] == 'worker-lost' and owner['outcome'] == 'WorkerLost', (replay, owner)
                if scenario in ('cancel', 'caller-loss', 'timeout'):
                    assert owner['outcome'] == {'cancel': 'Cancelled', 'caller-loss': 'CallerLost', 'timeout': 'TimedOut'}[scenario]
                print(f'{scenario}: {replay["completion"]}, independent disposal confirmed', flush=True)
            finally:
                after = lifecycle.snapshot(ordinary)
                (root / f'{scenario}.ordinary-after.json').write_text(json.dumps(after))
                (root / f'{scenario}.timing.json').write_text(json.dumps({'wallSeconds': time.monotonic() - started}))
                assert before == after and sentinel.poll() is None, 'ordinary profile or unrelated process changed'
    finally:
        sentinel.terminate()
        sentinel.wait(timeout=5)


if __name__ == '__main__':
    main()
