# Engine knowledge index

The Rust code and its `//!` comments describe the present methods. These pages hold what the
code cannot: engine facts, failed approaches and pitfalls, and prototype findings that no Rust
method uses.

## Tracked knowledge pages

Each finding applies to one exact executable. Find the build in [targets](native/targets.md)
before you reuse a finding. A version label or a symbol name is not sufficient.

| Subject | Page | What it holds |
| --- | --- | --- |
| Write a method | [Method authoring](native/method-authoring.md) | The inspector and its limits, stop diagnostics, authored tests, parity, population runs and baselines |
| Operations and their code | [Discovery methods](native/discovery.md) | Each operation's source stamp, owning modules and knowledge page |
| Registry fields | [Registry fields](native/registry-fields.md) | What complete means, the field sweep, compiler jump tables and bit fields, reader identities and kinds, read conditions, registry names, the scheduler table and owner joins, block entry contexts |
| Nested command grammar | [Command grammar](native/command-grammar.md) | Forms and stage chains, member ledgers, target arguments, parser observations, population results and the consumer boundary |
| Engine commands and scopes | [Engine commands](native/engine-commands.md) | Declarations, target getters, modifiers, categories, scopes and links, localization tables, on_action and game rule call sites, defines |
| References and dynamic names | [References](native/references.md) | Reference readers and lookup shapes, owner initializers, identifier grammar, flag stores and namespaces |
| Numeric conversion | [Numeric conversion](native/numeric-conversion.md) | Reader shapes, scanner formats, faithful-storage ranges, live boundary samples and the current numeric result |
| Scoped numeric operands | [Scoped numeric](native/scoped-numeric.md) | Whole-body operand proofs, subtype joins, routing forms, owner derivation, the lexer string investigation and retired world evaluation results |
| Duration keys | [Duration keys](native/durations.md) | Unit factors, combination rules, omitted counts, the flag-store countdown and the modifier and trait consumers |
| Modifier blocks | [Modifier blocks](native/modifier-blocks.md) | Shared fixed keys, entry forms, reader gaps and prototype findings on modifier fields |
| Generated modifiers | [Modifier families](native/modifier-families.md) | Generation calls and roots, item keys, the per-item post-read call and the loaded modifier table |
| Information in engine errors | [Diagnostic survey](native/diagnostic-survey.md) | Exact-build scope, Boolean, key, target and reference messages; ordinary-log and source-filter gaps |
| Observe the game before it parses content | [Early observations](native/early-observations.md) | Loader-entry attachment, registry items at loader return, fixture parsing and storage, loader and destination pitfalls |
| Call engine functions in a paused game | [Engine calls and memory](native/engine-calls.md) | Script-check calls and capture, calling conventions, M45-old time, resources, events and object lifetimes |
| Start, isolate, close and supervise a game | [Lifecycle](native/lifecycle.md) | Private profiles, launch, disposal on macOS and Windows, debugger shutdown and approval, the worker handshake and fault controls, the run summary |
| Load a world and observe prepared effects (retired route) | [Ready-world observations](native/ready-world.md) | The retired world route and how to restore it from `d8f9d8a`: world pins, daily updates, flag expiry and variable reads |
| Builds and adaptation between them | [Targets](native/targets.md) | Exact executable identities, ports between builds and Windows adaptation results |
| Static analysis cost | [Performance](native/performance.md) | Dev-profile settings, hashing cost and the static-query invariant |
| Private prototype bundles | [Preservation](native/preservation.md), [Retrieval](native/retrieval.md), [inventory](native/source-inventory.json) | What each bundle holds, how to verify and restore it, and each bundle's identity |

A finding is **demonstrated** on its original build only. A **candidate** lacks a required join or a
behavior check. An unsupported or untested build gets no inferred result. A failed method stays
on its page, with the correction that replaced it.

## Private prototype bundles

The prototype sources, raw captures, logs, disassembly and exported Linear records are not in
Git. They are in `.local/evidence/bundles/`, which Git ignores. A second copy is in
`/Users/jackson/Documents/PDX/evidence/native-2026-09-18/`. No private remote copy exists.

- [source-inventory.json](native/source-inventory.json) lists each bundle, its archive hash and its origin.
- [Preservation](native/preservation.md) says what each bundle holds and what was not kept.
- [Retrieval](native/retrieval.md) says how to verify and restore a bundle, and which prototype commands are safe to run without a game.

Keep the bundles. The Windows adapters, the M45-old engine-call experiments and the raw
captures exist only there. The [development policy](development-policy.md) has the rule on
preserving this knowledge.

## Ownership

Native owns the platform and build methods. Atlas owns authoring-rule conclusions, extraction
fixtures and coverage; its pages are in `/Users/jackson/Developer/pdx-foundry/atlas/docs/`.
Linear is provenance only: the `linear-records` bundle holds the exported tickets, comments and
attachments. For offsets and calling signatures, read the original adapter source in the bundles;
do not keep a second offset table here.
