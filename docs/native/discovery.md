# Static analysis methods

Each static method records its engine facts, current result, gaps and pitfalls on the page for
its subject. Add a page when a method starts a new subject. The code and its module comments
describe the methods; these pages hold what the code cannot.

| Page | Subjects |
| --- | --- |
| [Registry fields](registry-fields.md) | The current field sweep and its stops; compiler jump tables and bit fields in root readers; members and shared readers; registry scheduling and owner joins |
| [Engine commands](engine-commands.md) | Effect and trigger declarations and their scopes; target getters; modifier, category, scope and link declarations; localization contexts; on_actions and game rules |
| [Modifier families](modifier-families.md) | Modifier families from database generators, post-read code and shared helpers; the per-item post-read call; item keys; the loaded modifier table |

This page also holds the retired reference seam and the define read helpers.

## Define read helpers

On M45-release, `Native::defines()` (`defines/v1`) finds 2,385 compiled `NDefines` and
`NUncheckedDefines` `ReadDefine` helpers, and follows 2,305 of them to a literal namespace, a
literal name and a typed engine reader, in about four seconds. Resolved types are 1,091
fixed-point, 672 integer, 326 string, 172 float, 23 list, 13 vector and 8 boolean. The other 80
named helpers use a table-search loop that exceeds the path search; they are `UnresolvedReader`
gaps, including `NGraphics.ORBIT_HSV`. There are no failed or unnamed helpers.

The method classifies the target of each direct `GetValue`, `GetArrayValue` or
`ReadDefinesValue` call. `GetValue` takes namespace and name arguments; `GetArrayValue` reads a
named value from a namespace table. Shipped define entries, defaults, comments, bounds and uses
are not established here. SDK-610 owns the broader extraction question, and Atlas owns the
comparison with shipped content and config.

## Reusable reference seam (retired)

The SDK-482 prototype was accepted on 2026-09-17 at experiment
`c2258d2ef5bdcb195f6d2a3a88d7a45e2f80cc57`, branch `prototype/sdk-482-reference-observations`,
on M45-observe. Source: `typed-extraction/typed-extraction/reference-observation-prototype/` (see
[retrieval](retrieval.md)). Its Rust port left the product build; [reference method
retirement](reference-method-retirement.md) keeps its expected cases.

The provider selects the Mach-O slice, binds symbols, fixups and stubs, parses ARM64
instructions, relocates local branches and checks the shape of a complete function. Four compiler
templates, each checked by hand, keep register widths, aliases, branch destinations, comparisons
and calls. Only declared input and output locations, global bindings and typed callees are
parameters; any other code change gives unknown. It is not a general decompiler.

- Ship's reader destination joins a typed map call with empty and nonempty alternatives and
  same-type null substitution.
- District's reader joins a collection scan that compares length and bytes, with first-match and
  empty or no-match outcomes. A fresh relic trial reused this scan method unchanged.
- Planet class establishes a getter wrapper and its null substitution, but not the custom reader
  or the table internals.
- Army stays unknown: conditional event-target traversal and indirect jump tables had no
  reusable method.

Typed database and null names are candidate reference classes. They do not prove the collection's
element class, content-loader ownership, registration, validation or gameplay. The reader
traversal tracks concrete tokens and owner provenance and stops on unknown calls or state.

All 27 controls pass: wrong owners or registers, clobbers, changed comparisons or getters,
inverted branches or null selection, changed string layout or type, pointer truncation, reader
provenance, unknown calls and an unavailable target or owner. The controls change disassembly in
memory, not the executable. The result establishes one pattern on one build, not a success rate.
`run.py` does not launch the game, but needs the pinned installation and Xcode tools, and
rewrites its outputs; use a restored working copy.
