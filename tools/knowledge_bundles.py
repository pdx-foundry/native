#!/usr/bin/env python3
"""Verify private bundles, or restore regular files to a new local directory."""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import shutil
import tarfile

ROOT = Path(__file__).resolve().parents[1]
BUNDLES = ROOT / '.local/evidence/bundles'

def digest(stream):
    result = hashlib.sha256()
    while data := stream.read(1024 * 1024):
        result.update(data)
    return result.hexdigest()

def verify(manifest):
    archive = BUNDLES / manifest['archive']
    with archive.open('rb') as stream:
        assert digest(stream) == manifest['sha256'], archive
    expected = {row['path']: row for row in manifest['files']}
    with tarfile.open(archive) as bundle:
        seen = set()
        for member in bundle:
            assert member.name in expected and member.name not in seen, member.name
            seen.add(member.name)
            row = expected[member.name]
            if 'link' in row:
                assert member.issym() and member.linkname == row['link'], member.name
            else:
                assert member.isfile() or member.islnk(), member.name
                if member.islnk():
                    linked = expected[member.linkname]
                    assert (linked['bytes'], linked['sha256']) == (row['bytes'], row['sha256']), member.name
                    # Its payload is checked at the linked regular entry.
                    continue
                else:
                    assert member.size == row['bytes'], member.name
                with bundle.extractfile(member) as stream:
                    assert digest(stream) == row['sha256'], member.name
        assert seen == set(expected), 'Missing archive entries'
    return archive

def restore(manifest, archive):
    destination = ROOT / '.local/evidence/restored' / manifest['id']
    destination.mkdir(parents=True, exist_ok=False)
    skipped = []
    with tarfile.open(archive) as bundle:
        for member in bundle:
            path = PurePosixPath(member.name)
            assert not path.is_absolute() and '..' not in path.parts, member.name
            if not (member.isfile() or member.islnk()):
                skipped.append(member.name)
                continue
            output = destination.joinpath(*path.parts)
            output.parent.mkdir(parents=True, exist_ok=True)
            with bundle.extractfile(member) as source, output.open('wb') as target:
                shutil.copyfileobj(source, target)
            with output.open('rb') as source:
                row = next(row for row in manifest['files'] if row['path'] == member.name)
                assert digest(source) == row['sha256'], member.name
    print(json.dumps({'restored': str(destination), 'skippedLinks': skipped}))

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('bundle', nargs='?', help='Bundle ID; omit to verify all')
    parser.add_argument('--restore', action='store_true', help='Requires one ID and a new destination')
    args = parser.parse_args()
    assert not args.restore or args.bundle, 'Choose one bundle to restore'
    paths = [BUNDLES / (args.bundle + '.manifest.json')] if args.bundle else sorted(BUNDLES.glob('*.manifest.json'))
    assert paths, 'Private evidence is not present; see docs/native/preservation.md'
    index = {row['id']: row for row in json.loads((ROOT / 'docs/native/source-inventory.json').read_text())['bundles']}
    for path in paths:
        manifest = json.loads(path.read_text())
        if manifest['id'] in index:
            pinned = index[manifest['id']]
            with path.open('rb') as stream:
                assert digest(stream) == pinned['manifestSha256'], path
            assert manifest['sha256'] == pinned['sha256'], 'Archive identity differs from tracked inventory'
        archive = verify(manifest)
        print(json.dumps({'verified': manifest['id'], 'files': len(manifest['files']), 'sha256': manifest['sha256']}), flush=True)
        if args.restore:
            restore(manifest, archive)

if __name__ == '__main__':
    main()
