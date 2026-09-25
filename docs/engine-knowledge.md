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
| Static analysis, references, registries | [Discovery methods](native/discovery.md) | Compiler patterns, token paths, scheduler table, owner joins, engine documentation commands, localization tables, on_action and game rule call sites, modifier families from database generators, post-read code and shared helpers, the per-item post-read call, the loaded modifier table |
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
Callers are direct `bl` and `b` only; a string reference is `adr`, or `adrp` then `add` within
64 bytes with no branch between them. The entry is `pdx_native::internals::inspect`, which is
not a consumer API.

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
