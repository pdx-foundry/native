# Retired reference initializer method

The SDK-482 Rust port was removed from the product build at milestone 2 because no
supported Native operation called it. Its exact M45-observe results are retained as
[small expected cases](reference-method-cases.json); the executable inputs and original
27 mutation controls remain in the preserved `typed-extraction` prototype bundle and
Git history: experiment `c2258d2ef5bdcb195f6d2a3a88d7a45e2f80cc57`, branch
`prototype/sdk-482-reference-observations`, source
`typed-extraction/typed-extraction/reference-observation-prototype/` (see
[retrieval](retrieval.md)).

The provider selects the Mach-O slice, binds symbols, fixups and stubs, parses ARM64
instructions, relocates local branches and checks the shape of a complete function. Four compiler
templates, each checked by hand, keep register widths, aliases, branch destinations, comparisons
and calls. Only declared input and output locations, global bindings and typed callees are
parameters; any other code change gives unknown. It is not a general decompiler. The reader
traversal tracks concrete tokens and owner provenance and stops on unknown calls or state.

| Input shape | Established result | Boundary |
| --- | --- | --- |
| Ship `PostInit`: typed map call, same-type null, and joined `random_existing_design` reader | Candidate `CShipSize`, with empty, missing, and found alternatives | The typed map callee's internals were not proved. |
| District `PostInit`: length-and-byte collection scan and joined `district_type` reader | Candidate `CDistrictType`, first match or typed null | Collection element type and loader were not proved. |
| Planet-class `PostInit`: getter wrapper around a typed call | Candidate `CPlanetClass`, null or call result | Hash-table callee internals and authored field join were not proved. |
| Army `PostInit`: conditional event-target traversal and indirect jump table | Unknown, with an unresolved conditional | The four qualified compiler shapes did not match. |
| Relic `PostInit`: unchanged district scan shape | Candidate `CRelic`, with no authored field join | This was successful method reuse, not proof of a full resolver. |

The two unresolved callee shapes above are separate failures. Neither says that the
working caller selection, typed-null check, reader join, or linear scan must be
rewritten. A future public reference operation should start from these cases and
qualify the missing callee and ownership relationships before claiming more.

Typed database and null names are candidate reference classes. They do not prove the collection's
element class, content-loader ownership, registration, validation or gameplay. The 27 controls
change disassembly in memory, not the executable: wrong owners or registers, clobbers, changed
comparisons or getters, inverted branches or null selection, changed string layout or type,
pointer truncation, reader provenance, unknown calls and an unavailable target or owner. `run.py`
does not launch the game, but needs the pinned installation and Xcode tools, and rewrites its
outputs; use a restored working copy.
