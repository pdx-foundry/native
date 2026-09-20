#!/usr/bin/env python3
"""Qualify SDK-530 root-field discovery and optionally promote its exact static composition."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('prepare_fields', ROOT / 'tools/prepare-registry-fields.py')
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
    assert not output.exists(), 'Use a new private evidence directory'
    env = dict(os.environ, PDX_NATIVE_ANALYSIS_EXECUTABLE=str(args.executable.resolve()), PDX_NATIVE_FIELDS_OUTPUT=str(output), PYTHONDONTWRITEBYTECODE='1')
    subprocess.run(['cargo', 'test', '--locked', '--release', '--lib', 'qualify_registry_fields', '--', '--ignored'], cwd=ROOT, env=env, check=True)
    comparison = prepare.prepare(output)
    for test in ['retained_agenda_paths_names_and_reader_arguments_match', 'captured_omission_clobber_and_unknown_shape_controls']:
        subprocess.run(['cargo', 'test', '--locked', '--release', '--test', 'private_fields', test, '--', '--ignored'], cwd=ROOT, env=env, check=True)
    report = json.loads((output / 'qualification.json').read_text())
    report.update(status='verified-root-field-discovery', method='registry-fields/v1',
        basis='Executable-only Rust derivation; exact SDK-487 agenda names, tokens, paths, conditions, reader arguments and raw-function parity. Tradition root fields retain explicit missing joins.',
        comparison=comparison,
        controls=[prepare.reference(output / name, str((output / name).relative_to(ROOT / '.local/evidence'))) for name in ['parity.json', 'controls.json']],
        limits=['Complete registry remains false; five SDK-487 agenda shared-reader contracts remain unresolved.', 'Template ownership is static evidence; helper closure, full semantics and other builds remain unqualified.', 'No game launch or live-session qualification.'])
    (output / 'verified.json').write_text(json.dumps(report, indent=2) + '\n')
    if not args.promote:
        print(json.dumps(report, indent=2))
        return
    authority_path = ROOT / 'src/qualification/records/analysis.json'
    report_path = ROOT / 'docs/native/registry-fields-qualification.json'
    previous_authority = authority_path.read_bytes()
    previous_report = report_path.read_bytes() if report_path.exists() else None
    authority = json.loads(previous_authority)
    record = dict(id='sdk-530-m45-registry-fields-' + report['composition'][:12], composition=report['composition'], evidence=[prepare.reference(output / 'verified.json', str((output / 'verified.json').relative_to(ROOT / '.local/evidence')))])
    authority['accepted'] = [item for item in authority['accepted'] if item['composition'] != record['composition']] + [record]
    authority_path.write_text(json.dumps(authority, indent=2) + '\n')
    report_path.write_text(json.dumps(report, indent=2) + '\n')
    try:
        subprocess.run(['cargo', 'test', '--locked', '--release', '--test', 'private_fields', 'public_fields_need_only_an_executable_and_reject_foreign_subjects', '--', '--ignored'], cwd=ROOT, env=env, check=True)
        report['public_controls'] = prepare.reference(output / 'public-controls.json', str((output / 'public-controls.json').relative_to(ROOT / '.local/evidence')))
        report_path.write_text(json.dumps(report, indent=2) + '\n')
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
