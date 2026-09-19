#!/usr/bin/env python3
"""Fresh paused-session controls. Never installs qualification or substitutes another target."""
import argparse
import importlib.util
import json
import os
import signal
from pathlib import Path
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('lifecycle', ROOT / 'tools/check-candidate-lifecycle.py')
lifecycle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(lifecycle)
NAMES = ('traditions', 'tradition_categories')
COMMON = ('normal', 'reverse', 'cancel', 'caller-loss', 'timeout', 'idle-timeout', 'drop', 'startup-drop', 'runtime-shutdown', 'read-cancel', 'close-cancel', 'final-retention-failure', 'snapshot-retention-failure', 'worker-loss-held', 'game-exit-held')
FAULTS = ('missing-hook', 'late-hook', 'dropped-record', 'missing-terminal', 'access-failure', 'worker-loss')


def intervene(retention, scenario, process):
    until = time.monotonic() + 160
    while time.monotonic() < until:
        attempts = list(retention.glob('game-*'))
        if attempts:
            attempt = attempts[0]
            if scenario == 'snapshot-retention-failure' or (attempt / 'snapshots/traditions/replay.json').exists():
                if scenario in ('worker-loss-held', 'game-exit-held'):
                    identity = json.loads((attempt / 'worker-owned.json').read_text())
                    pid = identity['pid'] if scenario == 'worker-loss-held' else json.loads((attempt / 'worker-request.json').read_text())['game']
                    os.kill(pid, signal.SIGKILL)
                    (attempt / 'external-control.json').write_text(json.dumps(dict(scenario=scenario, pid=pid)))
                    return
                phase = 'snapshots' if scenario == 'snapshot-retention-failure' else 'final' 
                destination = attempt / phase / 'traditions'
                destination.mkdir(parents=True, exist_ok=False)
                (destination / 'descriptor.json').write_text('external evidence-retention failure control')
                return
        assert process.poll() is None, 'consumer exited before retention control'
        time.sleep(.01)
    raise AssertionError('retention control deadline elapsed')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('installation', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--production', action='store_true')
    parser.add_argument('--scenario', choices=COMMON + FAULTS)
    parser.add_argument('--registry', choices=NAMES)
    args = parser.parse_args()
    root = args.output.resolve()
    root.mkdir(mode=0o700, exist_ok=False)
    feature = 'production' if args.production else 'maintainer-tools'
    example = 'live' if args.production else 'session-investigation'
    subprocess.run(['cargo', 'build', '--locked', '--release', '--features', feature, '--example', example], cwd=ROOT, check=True)
    binary = root / example
    shutil.copy2(ROOT / 'target/release/examples' / example, binary)
    for name in ('src', 'crates', 'examples', 'tools'):
        shutil.copytree(ROOT / name, root / 'source' / name, ignore=shutil.ignore_patterns('__pycache__', '.DS_Store'))
    for name in ('Cargo.toml', 'Cargo.lock', 'build.rs'):
        shutil.copyfile(ROOT / name, root / 'source' / name)
    scenarios = [(mode, 'traditions') for mode in COMMON]
    if not args.production:
        scenarios += [(mode, name) for mode in FAULTS for name in NAMES]
    if args.scenario:
        scenarios = [(mode, name) for mode, name in scenarios if mode == args.scenario and (not args.registry or name == args.registry)]
    assert scenarios, 'No matching control'
    ordinary = Path.home() / 'Documents/Paradox Interactive/Stellaris'
    sentinel = subprocess.Popen(['/bin/sleep', '7200'])
    results = []
    try:
        for scenario, selected in scenarios:
            case = f'{scenario}-{selected}'
            retention = root / case
            retention.mkdir()
            before = lifecycle.snapshot(ordinary)
            (root / f'{case}.ordinary-before.json').write_text(json.dumps(before))
            started = time.monotonic()
            try:
                external = scenario.endswith('retention-failure') or scenario.endswith('-held')
                mode = 'hold' if external else scenario
                with (root / f'{case}.stdout').open('w') as stdout, (root / f'{case}.stderr').open('w') as stderr:
                    process = subprocess.Popen([str(binary), str(args.installation), str(retention), mode, selected], stdout=stdout, stderr=stderr)
                    if external:
                        intervene(retention, scenario, process)
                    status = process.wait(timeout=260)
                assert status == 0, (root / f'{case}.stderr').read_text()
                attempts = list(retention.glob('game-*'))
                assert len(attempts) == 1, attempts
                attempt = attempts[0]
                report = lifecycle.wait_report(attempt / 'report.json')
                assert report['origin'] == ('qualified-live' if args.production else 'unqualified-candidate')
                assert report['disposal'] == 'Reaped' and report['reservation_resolved'], report
                if scenario != 'final-retention-failure':
                    assert not report['diagnostics'], report
                replay = {}
                for name in NAMES:
                    if scenario == 'final-retention-failure' and name == 'traditions':
                        assert name not in report['registries'] and report['diagnostics']
                        continue
                    replay[name] = json.loads((attempt / 'final' / name / 'replay.json').read_text())
                    assert replay[name]['disposal'] == 'confirmed'
                    public = subprocess.run(['cargo', 'run', '--quiet', '--example', 'registry-replay', '--', str(attempt / 'final' / name), str(attempt / 'final' / name / 'descriptor.ref.json')], cwd=ROOT, check=True, text=True, capture_output=True)
                    assert json.loads(public.stdout) == replay[name]
                stdout = (root / f'{case}.stdout').read_text()
                live = json.loads(stdout) if stdout.strip() else None
                if live:
                    for name, captured in live['registries'].items():
                        if 'Ok' in captured:
                            assert dict(captured['Ok'], origin='replay') == replay[name]
                # SIGKILL against a Mach-stopped game can remain pending until orderly close.
                # Close must flush debugger exit handling, then independently reap the game.
                expected_outcome = {'cancel':'Cancelled', 'caller-loss':'CallerLost', 'drop':'CallerLost', 'startup-drop':'CallerLost', 'runtime-shutdown':'CallerLost', 'timeout':'TimedOut', 'idle-timeout':'TimedOut', 'worker-loss':'WorkerLost', 'worker-loss-held':'WorkerLost', 'game-exit-held':'Completed'}.get(scenario, 'Completed')
                assert report['outcome'] == expected_outcome, report
                if scenario in FAULTS and scenario != 'worker-loss':
                    other = next(name for name in NAMES if name != selected)
                    assert replay[other]['completion'] == 'complete' and len(replay[other]['registeredItems']) > 0
                    expected = {'missing-hook':'unavailable', 'late-hook':'unavailable', 'dropped-record':'incomplete', 'missing-terminal':'incomplete', 'access-failure':'unavailable'}[scenario]
                    assert replay[selected]['completion'] == expected, replay[selected]
                    expected_readiness = 'PausedDuringRegistryInitialization' if scenario in ('missing-hook', 'late-hook') else 'PausedAfterRegistryInitialization'
                    assert live['readiness'] == expected_readiness, live
                elif scenario in ('normal', 'reverse', 'read-cancel', 'close-cancel', 'final-retention-failure', 'snapshot-retention-failure'):
                    assert all(value['completion'] == 'complete' for value in replay.values())
                    assert live['readiness'] == 'PausedAfterRegistryInitialization'
                if scenario == 'snapshot-retention-failure':
                    assert (attempt / 'snapshot-diagnostics.json').read_text() != '[]'
                    assert 'traditions: Registry unavailable' in (root / f'{case}.stderr').read_text()
                result = dict(case=case, seconds=time.monotonic()-started, outcome=report['outcome'], disposal=report['disposal'], readiness=live['readiness'] if live else None,
                    registries={name:dict(completion=value['completion'], items=len(value['registeredItems'])) for name,value in replay.items()})
                results.append(result)
                (root / 'results.json').write_text(json.dumps(results, indent=2))
                print(json.dumps(result), flush=True)
            finally:
                after = lifecycle.snapshot(ordinary)
                (root / f'{case}.ordinary-after.json').write_text(json.dumps(after))
                assert before == after, 'ordinary profile changed'
                assert sentinel.poll() is None, 'unrelated process terminated'
    finally:
        sentinel.terminate()
        sentinel.wait(timeout=5)


if __name__ == '__main__':
    main()
