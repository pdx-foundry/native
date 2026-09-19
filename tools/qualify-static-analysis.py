#!/usr/bin/env python3
"""Verify the retained decode control and optionally promote its exact static composition.

No game is launched. Run the full workspace checks before --promote; host CI validates
portable instruction controls separately. Raw bytes remain under the private output root.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def reference(path, name):
    raw = path.read_bytes()
    return dict(path=name, bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable', type=Path)
    parser.add_argument('output', type=Path, help='New private directory under .local/evidence')
    parser.add_argument('--promote', action='store_true')
    args = parser.parse_args()
    output = args.output.resolve()
    output.relative_to(ROOT / '.local/evidence')
    assert not output.exists(), 'Use a new private evidence directory'
    output.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(['python3', 'tools/evidence.py', 'typed-extraction'], cwd=ROOT, check=True)
    env = dict(os.environ, PDX_NATIVE_ANALYSIS_EXECUTABLE=str(args.executable.resolve()), PDX_NATIVE_ANALYSIS_OUTPUT=str(output))
    subprocess.run(['cargo', 'test', '--locked', '--release', '--lib', 'qualify_retained_decode', '--', '--ignored'], cwd=ROOT, env=env, check=True)
    candidate = json.loads((output / 'qualification.json').read_text())
    if not args.promote:
        print(json.dumps(candidate, indent=2))
        return
    report = dict(candidate,
        status='qualified-static-control',
        basis='Exact M45 control compared with retained disassembly; public replay reproduces instructions. Portable authored controls run in the macOS/Linux/Windows CI matrix.',
        limits=['One function only; no inferred reader or registry semantics.', 'No live session requalification or game launch.', 'Cross-host controls use authored bytes; the full private executable is verified locally.'],
        candidate=reference(output / 'qualification.json', str((output / 'qualification.json').relative_to(ROOT / '.local/evidence'))),
    )
    report_path = ROOT / 'docs/native/static-analysis-qualification.json'
    previous_report = report_path.read_bytes() if report_path.exists() else None
    report_path.write_text(json.dumps(report, indent=2) + '\n')
    promoted_report = output / 'qualified.json'
    promoted_report.write_bytes(report_path.read_bytes())
    authority_path = ROOT / 'src/qualification/records/analysis.json'
    previous_authority = authority_path.read_bytes()
    authority = json.loads(previous_authority)
    record = dict(id='sdk-527-m45-static-decode-v1', composition=candidate['composition'], evidence=[reference(promoted_report, str(promoted_report.relative_to(ROOT / '.local/evidence')))])
    # Retain previous records if a later implementation is requalified.
    authority['accepted'] = [item for item in authority['accepted'] if item['composition'] != record['composition']]
    if any(item['id'] == record['id'] for item in authority['accepted']):
        record['id'] += '-' + candidate['composition'][:12]
    authority['accepted'].append(record)
    authority_path.write_text(json.dumps(authority, indent=2) + '\n')
    try:
        subprocess.run(['cargo', 'test', '--locked', '--release', '--test', 'private_analysis', '--', '--ignored'], cwd=ROOT, env=env, check=True)
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
