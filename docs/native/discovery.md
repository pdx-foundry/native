# Static analysis, reference observations, and registry discovery

Each static method records its findings, failed shapes and sweep counts on the page for its
subject. Add a page when a method starts a new subject.

| Page | Methods |
| --- | --- |
| [Registry fields](registry-fields.md) | The Milestone 4 field baseline (SDK-596) and where its paths stop (SDK-581); jump tables and bit fields in root readers (SDK-563); members and shared readers (SDK-487, SDK-492, SDK-493); registry scheduling and owner joins (SDK-489) |
| [Engine commands](engine-commands.md) | Effect and trigger declarations (SDK-488, SDK-535, SDK-562); target getters (SDK-568); modifiers, categories, scopes and links (SDK-536, SDK-565); localization (SDK-537); on_actions and game rules (SDK-538) |
| [Modifier families](modifier-families.md) | Families from database generators (SDK-540); the loaded modifier inventory (SDK-564); post-read code and shared helpers (SDK-566); item post-read code (SDK-575) |

This page keeps the reusable reference seam (SDK-482), the Rust ports and the define read helpers
(SDK-539).

## Reusable reference seam

SDK-482 was accepted on 2026-09-17 at experiment `c2258d2ef5bdcb195f6d2a3a88d7a45e2f80cc57`, branch `prototype/sdk-482-reference-observations`. Source: `typed-extraction/typed-extraction/reference-observation-prototype/`. Target: M45-observe. The provider selects the Mach-O slice, binds symbols/fixups/stubs, parses ARM64 instructions, relocates local branches and checks complete-function shape. Four manually qualified compiler templates preserve register widths, aliases, branch destinations, comparisons and calls. Only declared input/output locations, global bindings and typed callees are parameters; other code changes return unknown. This is not a general decompiler.

Ship's authored reader destination joins a typed map call with empty/nonempty alternatives and same-type null substitution. District's reader joins a length-and-byte-comparison collection scan, with first-match versus empty/no-match outcomes. Planet-class qualifies a getter wrapper and its null substitution but not the custom reader or table internals. Army remains unknown because conditional event-target traversal and indirect jump tables lack a qualified reusable method. A fresh relic trial reused the frozen district scan method unchanged; no post-selection method or consumer changes were made.

Typed database/null names are candidate reference classes. They do not prove actual collection element class, content-loader ownership, registration, validation or gameplay. The reader traversal tracks concrete tokens and owner provenance and stops on unknown calls/state. Preserve stage, conditional alternatives, incomplete joins, target-local handles and distinct native-method/rule qualification in emitted observations.

All 27 retained controls pass in the original result: wrong owners/registers, clobbers, changed comparisons/getters, inverted branches/null selection, changed string layout/type, pointer truncation, reader provenance, unknown calls and unavailable target/owner. Controls mutate disassembly in memory rather than the executable. The transfer establishes one pattern on one build, not a broad success rate or low maintenance cost. `run.py` does not launch the game but does require the pinned installation and Xcode tools, and rewrites outputs; use a restored working copy. See [offline retrieval](retrieval.md) for retained-evidence checks that require neither.

Original Atlas consumer pointers remain in `/Users/jackson/Developer/pdx-atlas/docs/prototypes/`. Accepted resolutions, including SDK-482/487/488/489/492/493, are available offline in `linear-records/linear/SDK-<number>-comments.json`. Original reviews keep their earlier pending labels and unmodified evidence.

## Rust ports

The Rust code in `src/engine/analysis` ports five of these methods: template registry discovery with
the static scheduler table (SDK-489), registry names from the database constructors, and root fields
with their reader joins (SDK-487), effect and trigger declarations (SDK-535), and modifier,
category, scope and link declarations (SDK-536). SDK-537 adds a method that no prototype had:
localization contexts, commands and links. SDK-538 adds another: on_actions and game rules with the
scopes that their call sites supply. SDK-540 adds modifier families from database generators. The
module comments describe each method. The methods read the executable only and receive no field or
config seeds. The five shared-reader contracts of [members and shared
readers](registry-fields.md#members-and-shared-readers) stay unresolved, so no registry has a
complete field answer.

### Define read helpers (SDK-539)

On the exact M45-release executable, `Native::defines()` found 2,385 compiled
`NDefines` and `NUncheckedDefines` `ReadDefine` helpers. It followed 2,305 to a literal namespace,
literal name and typed engine reader. The result is partial: 80 named helpers use a table-search
loop that exceeds the bounded path search. They remain `UnresolvedReader` gaps, including
`NGraphics.ORBIT_HSV`. There were no failed or unnamed helpers. Resolved types are 1,091
fixed-point, 672 integer, 326 string, 172 float, 23 list, 13 vector and 8 boolean. The static
query took about four seconds in a development test run.

The method reads only executable code and literals. It classifies the target of each direct
`GetValue`, `GetArrayValue` or `ReadDefinesValue` call. `GetValue` uses namespace and name
arguments; `GetArrayValue` reads a named value from a namespace table. The source stamp is
`defines/v1` with `StaticAnalysis`. The tracked parity test checks counts, gap types and small
samples; it needs the exact executable through `STELLARIS_PATH`. Shipped define entries,
defaults, comments, bounds and uses are not established here. SDK-610 owns the broader
extraction question and Atlas owns the comparison with shipped content and config.
