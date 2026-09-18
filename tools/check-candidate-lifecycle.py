#!/usr/bin/env python3
"""Explicit real-game SDK-516 controls. Requires the exact installation and provisioned host store."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import select
import shutil
import time

ROOT = Path(__file__).resolve().parents[1]


def snapshot(root):
    result = {}
    if not root.exists():
        return result
    for path in sorted(root.rglob('*')):
        if path.is_symlink():
            result[str(path.relative_to(root))] = {'link': str(path.readlink())}
        elif path.is_file():
            result[str(path.relative_to(root))] = hashlib.sha256(path.read_bytes()).hexdigest()
    return result


def wait_report(path):
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        if path.exists():
            try:
                return json.loads(path.read_text())
            except json.JSONDecodeError:
                pass
        time.sleep(.05)
    raise AssertionError(f'No retained owner report: {path}')


def run_control(harness, installation, output, scenario):
    command = [str(harness), str(installation), str(output / scenario), scenario]
    if scenario != 'timeout':
        return subprocess.run(command, text=True, capture_output=True, timeout=50)
    owner_controller = subprocess.Popen(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        assert select.select([owner_controller.stderr], [], [], 15)[0], 'Owner startup timed out'
        first_line = owner_controller.stderr.readline()
        assert first_line.startswith('attempt='), first_line
        checkout = output / 'other-checkout'
        checkout.mkdir()
        executable = checkout / 'investigate'
        shutil.copyfile(harness, executable)
        executable.chmod(0o700)
        competitor = subprocess.run([str(executable), str(installation), str(output / 'competing'), 'normal'], text=True, capture_output=True, timeout=15)
        (output / 'competing.stdout').write_text(competitor.stdout)
        (output / 'competing.stderr').write_text(competitor.stderr)
        assert competitor.returncode != 0 and 'reservation busy' in competitor.stderr, competitor
        stdout, stderr = owner_controller.communicate(timeout=40)
        return subprocess.CompletedProcess(command, owner_controller.returncode, stdout, first_line + stderr)
    finally:
        if owner_controller.poll() is None:
            owner_controller.terminate()
            owner_controller.wait(timeout=5)


def ordinary_conflict(harness, installation, output):
    fixture = output / ('long-installation-path-' * 8)
    fixture.mkdir()
    executable = fixture / 'stellaris'
    source = fixture / 'fixture.c'
    source.write_text('#include <unistd.h>\nint main(void) { sleep(60); return 0; }\n')
    subprocess.run(['/usr/bin/cc', str(source), '-o', str(executable)], check=True)
    ordinary = subprocess.Popen([str(executable)])
    time.sleep(.1)
    assert ordinary.poll() is None, 'Conflict fixture did not survive startup'
    try:
        identity_command = ['/bin/ps', '-ww', '-p', str(ordinary.pid), '-o', 'pid=,lstart=,comm=']
        before = subprocess.check_output(identity_command, text=True)
        run = subprocess.run([str(harness), str(installation), str(output / 'ordinary-conflict'), 'normal'], text=True, capture_output=True, timeout=15)
        (output / 'ordinary-conflict.stdout').write_text(run.stdout)
        (output / 'ordinary-conflict.stderr').write_text(run.stderr)
        assert run.returncode == 0, run.stderr
        report = json.loads(run.stdout)
        assert report['disposal'] == 'NotLaunched' and 'Conflicting ordinary game' in report['outcome']['Failed'], report
        assert ordinary.poll() is None
        after = subprocess.check_output(identity_command, text=True)
        assert before == after
        (output / 'ordinary-conflict-control.json').write_text(json.dumps({'fixture': 'Compiled harmless sleeping process named stellaris; not a game qualification', 'before': before, 'after': after, 'report': report}, indent=2))
    finally:
        ordinary.terminate()
        ordinary.wait(timeout=5)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('installation', type=Path)
    parser.add_argument('output', type=Path, help='New private batch directory')
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(mode=0o700, parents=False, exist_ok=False)
    subprocess.run(['cargo', 'build', '--locked', '--release', '--features', 'maintainer-tools', '--example', 'investigate'], cwd=ROOT, check=True)
    harness = ROOT / 'target/release/examples/investigate'
    ordinary = Path.home() / 'Documents/Paradox Interactive/Stellaris'
    before = snapshot(ordinary)
    (output / 'ordinary-before.json').write_text(json.dumps(before, indent=2))
    sentinel = subprocess.Popen(['/bin/sleep', '180'])
    outcomes = {'normal': 'Completed', 'cancel': 'Cancelled', 'worker-loss': 'WorkerLost', 'caller-loss': 'CallerLost', 'timeout': 'TimedOut'}
    results = []
    try:
        for scenario, expected in outcomes.items():
            attempt = output / scenario
            started = time.monotonic()
            run = run_control(harness, args.installation, output, scenario)
            (output / f'{scenario}.stdout').write_text(run.stdout)
            (output / f'{scenario}.stderr').write_text(run.stderr)
            assert run.returncode == 0, (scenario, run.stderr)
            report = wait_report(attempt / 'report.json')
            owner = json.loads((attempt / 'owner.json').read_text())
            assert report['origin'] == 'unqualified-candidate'
            assert report['outcome'] == expected, report
            assert report['disposal'] == 'Reaped' and report['reservation_resolved'], report
            assert not report['diagnostics'], report
            assert owner['state'] == 'Disposed' and owner['attempt'] == report['attempt']
            pid = owner['game']['pid']
            # Absence supplements the direct owner's reaping fact; it cannot replace it.
            assert subprocess.run(['/bin/ps', '-p', str(pid)], capture_output=True).returncode != 0
            assert sentinel.poll() is None, 'Unrelated sentinel process changed'
            after = snapshot(ordinary)
            assert after == before, 'Ordinary profile changed'
            results.append({'scenario': scenario, 'attempt': report['attempt'], 'seconds': time.monotonic() - started, 'report': report})
            (output / 'results.json').write_text(json.dumps(results, indent=2))
        ordinary_conflict(harness, args.installation, output)
        assert snapshot(ordinary) == before
        (output / 'ordinary-after.json').write_text(json.dumps(snapshot(ordinary), indent=2))
    finally:
        sentinel.terminate()
        sentinel.wait(timeout=5)
    print(json.dumps({'verified': str(output), 'attempts': len(results)}))


if __name__ == '__main__':
    main()
