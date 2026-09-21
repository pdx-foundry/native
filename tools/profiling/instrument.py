#!/usr/bin/env python3
"""Copy tracked Native sources and add temporary SDK-559 timing probes to the copy."""

import argparse
from pathlib import Path
import shutil
import subprocess


RUST_SCOPES = {
    'src/binding.rs': ['open', 'registry_bindings'],
    'src/binding/installation.rs': ['open', 'executable_bytes', 'content_snapshot'],
    'src/binding/analysis.rs': [
        'executable', 'verified', 'registry_fields', 'field_input', 'build_catalog',
    ],
    'src/binding/binary.rs': ['hash', 'identify'],
    'src/binding/binary/discovery.rs': ['read'],
    'src/binding/binary/constructors.rs': ['read', 'initializers'],
    'src/binding/binary/fields.rs': ['read'],
    'src/engine/analysis/directories.rs': ['globals'],
    'src/session/questions.rs': ['registries_from_executable', 'registry_fields_from_executable'],
}


def instrument_functions(source, relative, functions):
    """Add whole-function spans at the expected source boundaries."""
    for name in functions:
        prefix = 'pub(crate) ' if relative == 'src/binding.rs' else ''
        token = f'{prefix}fn {name}('
        if source.count(token) != 1:
            raise ValueError(f'Expected one function: {relative}:{name}')
        start = source.index(token)
        body = source.index('{', start) + 1
        label = f'{relative}:{name}'
        timer = f'\n    let _sdk559_span = crate::sdk559_profile::Span::new("{label}");'
        source = source[:body] + timer + source[body:]
    return source


def instrument_discovery(source):
    """Split inventory construction into non-overlapping measured phases."""
    phases = [
        ("symbols", "    let mut raw_symbols = BTreeMap::new();", "    let fixups = fixups(slice, &file, &raw_symbols)?;"),
        ("fixups", "    let fixups = fixups(slice, &file, &raw_symbols)?;", "    let mut strings = BTreeMap::new();"),
        ("strings", "    let mut strings = BTreeMap::new();", "    let mut code = Vec::new();"),
        ("code-and-vtables", "    let mut code = Vec::new();", "    Ok(StaticInput {"),
    ]
    for label, start, end in phases:
        if source.count(start) != 1 or source.count(end) != 1:
            raise ValueError(f'Expected one discovery phase: {label}')
        timer = label.replace('-', '_') + '_span'
        begin = f'    let {timer} = crate::sdk559_profile::Span::new("discovery:{label}");\n'
        source = source.replace(start, begin + start)
        source = source.replace(end, f'    drop({timer});\n' + end)
    return source


def instrument(destination):
    """Write the probes only in the copied source tree."""
    for relative, functions in RUST_SCOPES.items():
        path = destination / relative
        path.write_text(instrument_functions(path.read_text(), relative, functions))
    discovery = destination / 'src/binding/binary/discovery.rs'
    discovery.write_text(instrument_discovery(discovery.read_text()))
    with (destination / 'src/lib.rs').open('a') as output:
        output.write("\n" + Path(__file__).with_name("span.rs").read_text())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('destination', type=Path, help='New source directory; original files are never edited')
    args = parser.parse_args()
    destination = args.destination.resolve()
    destination.mkdir(parents=True, exist_ok=False)
    paths = subprocess.check_output(['git', 'ls-files', '-z']).decode().split('\0')
    for relative in filter(None, paths):
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(relative, target)
    instrument(destination)
    print(destination)


if __name__ == '__main__':
    main()
