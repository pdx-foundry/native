#!/usr/bin/env python3
"""Qualify static registry discovery and retained SDK-489 ownership replay; never launch a game."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('prepare_discovery', ROOT / 'tools/prepare-registry-discovery.py')
prepare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prepare)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--promote', action='store_true')
    args = parser.parse_args()
    output = args.output.resolve()
    output.relative_to(ROOT / '.local/evidence')
    assert not output.exists(), 'Use a new private output directory'
    env = dict(os.environ, PDX_NATIVE_ANALYSIS_EXECUTABLE=str(args.executable.resolve()), PDX_NATIVE_DISCOVERY_OUTPUT=str(output), PYTHONDONTWRITEBYTECODE='1')
    subprocess.run(['python3', 'tools/evidence.py', 'atlas-ownership'], cwd=ROOT, check=True)
    subprocess.run(['cargo', 'test', '--locked', '--release', '--lib', 'qualify_registry_discovery', '--', '--ignored'], cwd=ROOT, env=env, check=True)
    prepare.prepare(output)
    subprocess.run(['cargo', 'test', '--locked', '--release', '--test', 'private_discovery', 'retained_discovery_parity_and_41_controls', '--', '--ignored'], cwd=ROOT, env=env, check=True)
    subprocess.run(['cargo', 'test', '--locked', '--release', '--test', 'private_discovery', 'captured_replay_rejects_cross_run_artifact_substitution', '--', '--ignored'], cwd=ROOT, env=env, check=True)
    report = json.loads((output / 'qualification.json').read_text())
    report.update(status='verified-static-discovery-and-historical-replay',
        controls=prepare.reference(output / 'rust-controls.json', str((output / 'rust-controls.json').relative_to(ROOT / '.local/evidence'))),
        historical_descriptor=prepare.reference(output / 'historical-descriptor.json', str((output / 'historical-descriptor.json').relative_to(ROOT / '.local/evidence'))),
        historical_result=prepare.reference(output / 'historical-result.json', str((output / 'historical-result.json').relative_to(ROOT / '.local/evidence'))),
        basis='Exact executable-derived candidates and scheduler slots match SDK-489; 41 retained controls and historical root-owner joins pass in Rust.',
        limits=['164 candidates and 198 scheduling rows are not a complete registry inventory.', '162 observed candidates and six economic-plan roots are historical startup evidence, not fresh live ownership.', 'No game launch, new custom/nested/late discovery, item enumeration or field schemas.', 'Portable synthetic tests run on all three CI hosts; the private M45 executable is checked locally.'])
    (output / 'verified.json').write_text(json.dumps(report, indent=2) + '\n')
    if not args.promote:
        print(json.dumps(report, indent=2))
        return
    authority_path = ROOT / 'src/qualification/records/analysis.json'
    previous_authority = authority_path.read_bytes()
    report_path = ROOT / 'docs/native/registry-discovery-qualification.json'
    previous_report = report_path.read_bytes() if report_path.exists() else None
    authority = json.loads(previous_authority)
    record = dict(id='sdk-528-m45-registry-discovery-' + report['composition'][:12], composition=report['composition'], evidence=[prepare.reference(output / 'verified.json', str((output / 'verified.json').relative_to(ROOT / '.local/evidence')))])
    authority['accepted'] = [item for item in authority['accepted'] if item['composition'] != record['composition']] + [record]
    authority_path.write_text(json.dumps(authority, indent=2) + '\n')
    report_path.write_text(json.dumps(report, indent=2) + '\n')
    try:
        subprocess.run(['cargo', 'test', '--locked', '--release', '--test', 'private_discovery', 'public_discovery_admission_and_replay', '--', '--ignored'], cwd=ROOT, env=env, check=True)
    except BaseException:
        authority_path.write_bytes(previous_authority)
        if previous_report is None:
            report_path.unlink()
        else:
            report_path.write_bytes(previous_report)
        raise
    print(json.dumps(dict(qualified=record['id'], output=str(output)), indent=2))


if __name__ == '__main__':
    main()
