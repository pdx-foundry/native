# Engine knowledge index

This page says where the knowledge about the game engine is kept. The knowledge comes from
prototypes and probes (SDK testing, Atlas, Typed PDXScript) and from the Rust supervisor.
The Rust code and its `//!` comments describe the present methods. These pages hold what the
code cannot hold: experiments, failed approaches, and findings that are not yet ported.

## Tracked knowledge pages

Each finding applies to one exact executable. Find the build in [targets](native/targets.md)
before you reuse a finding. A version label or a symbol name is not sufficient.

| Subject | Page | What it holds |
| --- | --- | --- |
| Start, isolate, close and supervise a game | [Lifecycle](native/lifecycle.md) | Private profiles, background launch, process disposal on macOS and Windows, debugger shutdown |
| Observe the game before it parses content | [Early observations](native/early-observations.md) | ARM64 loader-entry attachment, registration and field reads, where to read registry items |
| The debugger worker | [Loader-entry worker](native/loader-entry-worker.md) | The LLDB worker trial, its handshake and its four controls |
| Call engine functions and find live objects | [Engine calls and memory](native/engine-calls.md) | Main-thread calls, calling conventions, time, resources, events, country and planet lifetimes |
| Static analysis index | [Discovery methods](native/discovery.md) | The index of the method pages below, define read helpers |
| Registry fields and scheduling | [Registry fields](native/registry-fields.md) | The current field sweep and its stops, compiler jump tables and bit fields, token paths, members and shared readers, scheduler table, owner joins |
| Engine commands and scopes | [Engine commands](native/engine-commands.md) | Engine documentation commands, target getters, modifier, category, scope and link declarations, localization tables, on_action and game rule call sites |
| Generated modifiers | [Modifier families](native/modifier-families.md) | Modifier families from database generators, the loaded modifier table, post-read code and shared helpers, the per-item post-read call |
| Modifier generation and shared readers | [Modifier prototype brief](native/modifier-family-prototype.md) | Five release-build templates, two live content mutations, implementation seams and explicit grammar gaps |
| Builds and adaptation between them | [Targets](native/targets.md) | Exact executable hashes, Mac and Windows adaptation results |

A finding is **demonstrated** on its original build only. A **candidate** lacks a required join or a
behavior check. An unsupported or untested build gets no inferred result. A failed method stays
on its page, with the correction that replaced it.

## Inspecting an executable

`examples/inspect.rs` looks inside any ARM64 executable, catalogued or not, with no game and no
target record. Use it in place of a Python dump before you write a method:

```sh
cargo run --release --example inspect -- --function 'CMegaStructureType::ReadMember'
```

The image is `--image PATH` or `STELLARIS_PATH`. The other commands are `--symbols TEXT`,
`--callers NAME`, `--strings TEXT` and `--slots NAME --count N`. Each run first prints the image
hashes and whether chained fixups were read. Without them, no data slot is resolved and the run
prints why. Function extents come from symbols, so every end is an inferred boundary. Indirect
branches stay unresolved, and jump tables are shown only as the addresses the code forms.
Callers are direct `bl` and `b` only; a string reference is `adr`, or `adrp` then `add` in one
function with no branch or write between them. The inspector reads ARM64 images only. The entry
is `pdx_native::internals::inspect`, which is not a consumer API.

### Where a method stopped

On a catalogued build, `--registry-fields DIRECTORY` runs the registry field method and prints
each token path that stopped: the reason, the obstacle (an unknown register or flags, a spent
bound, an unsupported instruction, code outside the read function, a cycle, or a call that was not
followed), the stop instruction with its symbol and offset, where the walk entered code, and the
path's last instructions. Then it prints every internal gap before normalization.

```sh
cargo run --release --example inspect -- --registry-fields common/megastructures
```

The static methods locate their stops with one diagnostic (`Unresolved` and `Stop` in
`src/engine/analysis/stop.rs`). The internal results of registry fields, scopes, scope links,
localization, modifiers and modifier families keep it. Callbacks and defines keep only the reason word,
since they combine reasons across paths. Declaration scopes are read without a walk, so they have
no stop.
Public answers quote only the reason word; no address reaches them. `pdx_native::internals::registry_field_stops::run` runs the registry field
method once and returns its internal result with the public answer derived from it, as
`Native::registry_fields` derives it; it is not a consumer API.

`examples/registry-field-sweep.rs` runs every registry and groups the internal gaps by stop
instruction kind and obstacle, then by function. `--diff BEFORE AFTER` lists the registries whose
normalized answer changed between two reports; two runs on the same build give an empty diff.

```sh
cargo run --release --example registry-field-sweep -- "$STELLARIS_PATH" > after.json
cargo run --release --example registry-field-sweep -- --diff before.json after.json
```

## Private prototype bundles

The prototype sources, raw captures, logs, disassembly and exported Linear records are not in
Git. They are in `.local/evidence/bundles/`, which Git ignores. A second copy is in
`/Users/jackson/Documents/PDX/evidence/native-2026-09-18/`. No private remote copy exists.

- [source-inventory.json](native/source-inventory.json) lists each bundle, its archive hash and its origin.
- [Preservation](native/preservation.md) says what each bundle holds and what was not kept.
- [Retrieval](native/retrieval.md) says how to verify and restore a bundle, and which prototype commands are safe to run without a game.

Keep the bundles until the Python prototypes are ported. The
[development policy](development-policy.md) has the rule on preserving this knowledge.

## Ownership

Native owns the platform and build methods. Atlas owns authoring-rule conclusions, extraction
fixtures and coverage; its pages are in `/Users/jackson/Developer/pdx-atlas/docs/prototypes/`.
Linear is provenance only: the `linear-records` bundle holds the exported tickets, comments and
attachments. For offsets and calling signatures, read the original adapter source in the bundles;
do not keep a second offset table here.
