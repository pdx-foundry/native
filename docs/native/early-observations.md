# Injection and observations before registration/parsing

SDK-483 was accepted on 2026-09-17 for M45-observe. The experiment is `4188faf564b8609fde747da09bd4b8db8045b315`, branch `prototype/sdk-483-early-observations`. The retained source and evidence are in `typed-extraction/typed-extraction/early-observation-prototype/`.

## Qualified sequence

The final parent creates an ARM64 suspended direct child. LLDB attaches and observes `_dyld_start`. Before resume, all three requested registration/loader/field breakpoints resolve once, are enabled and have zero hits. The worker refuses resume if entry, architecture or hooks fail verification. Suspension is a prerequisite; the observed entry and correlated trace establish the ordering result.

The normal trace records three registration call entries during startup initializers, then the fixture's category `LoadFile` entry before reader construction. It joins `tree_template` at line 2 and `traditions` at line 3 to one owner and file, observes the matching loader return on the same thread, and emits an explicit terminal sequence/count record. Owner evidence then separately confirms exit and reaping.

The fixture is a category with a template string and empty traditions list. No save/world is loaded. These are **read-entry observations**, not stored values, successful registration returns, validation or gameplay. The three observed registration tokens do not establish a complete command registry.

| Retained final run | Observation outcome | Independent disposal |
| --- | --- | --- |
| `20260917-002215-none-d8a910` | Complete, ordering and continuous sequence, two fields, terminal count | Confirmed and reaped |
| `20260917-002242-missing-66363a` | Missing field hook, explicit unavailable capability | Confirmed and reaped |
| `20260917-002307-incomplete-ef5140` | Dropped record 11; sequence/count mismatch, incomplete | Confirmed and reaped |
| `20260917-002334-worker-loss-e2cab9` | Worker SIGKILL while callback stopped the game; no terminal completion | Parent killed and reaped game |

Worker and owner monotonic timestamps have different clock domains. Use trace sequence for producer order; do not compare those timestamps as one clock.

## Native joins and retained failures

`pdx_native.py` owns launch, target pin, profile/content hashes, timeout, worker and final ownership. `debugger_attempt.py` owns LLDB breakpoints, ARM64 argument/source joins, loader return and trace sequencing. `evidence/` retains reader, lexer, filename accessor, category loader/reader and token disassembly. `runs/*/source/` preserves the source actually executed, so the final result need not depend on current prototype code.

The accepted final mechanism is the debugger observer. A fallback launch-inserted dylib recorded registration but failed to reach parsing. Its private in-process hooks/trampoline are not qualified by the final result, and missing constructor witnesses cannot prove database construction order.

Initial `task_for_pid` failed. One human intervention enabled debugger access. A presentation guard crashed in `objc_retain` because an ARC function used an object signature for a BOOL; a no-argument selector also needed the correct signature. Both were corrected. Failed raw runs and `evidence/development-failures.json` remain. The old debugger-owned disposal failure remains explicit; final ownership is the independent direct parent.

## Reuse and limits

SDK-515 repeated the four controls with an LLDB subprocess and embedded Python, with a pinned
handshake. See the [loader-entry worker trial](loader-entry-worker.md). The Rust supervisor uses the
same worker.

Carry **activation**, **observation completion**, and **confirmed disposal** as separate facts. Empty output with an unresolved hook is unavailable; record loss prevents completion; worker loss does not erase already collected observations. Native owns target-specific addresses, argument conventions, source/owner joins and cleanup. Atlas supplies observation requests, fixtures and a deadline.

Fresh capture requires the exact M45-observe installation/content, ARM64 host, Xcode/LLDB and debugger access. The original `replay.py --scenario all` **builds a guard and launches games**; its name does not mean offline replay. This consolidation did not run it. The offline migration check reads manifests, source hashes, traces and final ownership records, described in [retrieval](retrieval.md).

No production adapter, Windows timing, database-constructor order, late/hot reload, arbitrary parser stage, owner-loss recovery or low maintenance cost is established. Atlas's accepted consumer clarification is retained at `/Users/jackson/Developer/pdx-atlas/docs/prototypes/early-observation.md`. Local Linear acceptance is `linear-records/linear/SDK-483-comments.json`; the review summary's earlier pending label remains historical.

## Registry items (Rust supervisor, M45-observe)

- **Where to read the items.** The worker reads item keys from the engine objects when the initial collection loader returns. The collection is full at that point, and later validation has not run. A first attempt waited for entry to the later post-read phase; the game did not reach it in 180 seconds.
- **Inputs.** The engine reads private copies of the registry directories. On M45-observe the result is 234 traditions and 33 tradition categories, from one paused process, in about 35 to 45 seconds.
- **SDK-529 extension.** Static discovery now supplies initial loader entries for selected
  registry directories. The same return-boundary witness gave 49 ascension perks, 17 ethics,
  171 edicts, 358 civics from a nested directory, and 10 galaxy definitions from `map/galaxy`
  in an M45 live probe. See [registry items](registry-items.md) for report status and limits.
- **Launch flag.** The launch uses `-debug_mode`, as the prototype did. One early batch omitted the flag and still reached the fixture, so the flag is not known to be necessary.
- **Missing debugger.** To test a missing debugger without a change to the host, set `DEVELOPER_DIR` to a directory that does not exist.

## Prepared fixture sessions (SDK-532, M45-observe)

The public session now combines the retained registration/category hooks with the registry hooks.
The fixture's loader-return callback ends its observation window without stopping the session;
the existing registry callbacks then establish the final pause. The exact M45-observe installation
passed all 27 cases in `tests/live.rs`, including the existing registry controls, on 2026-09-20.

A private category directory containing the two-field fixture gives three registration entries,
`tree_template` at line 2 and `traditions` at line 3, joined to one owner. The same session returns
one category and the 234 pinned traditions. Either observation kind can also be requested alone.
Missing/late fixture hooks give an observation error. Dropped records, missing terminals and the
injected access failure give partial answers. Worker loss before the pause gives a startup error
with confirmed disposal. The tests check ordinary-profile contents and an unrelated process after
every case, and compare saved answers with game-free reads.

The first integration attempt failed before attachment: a numeric-key binding map generated
`patternProperties`, which the worker codec deliberately refuses. A typed list of field bindings
uses the supported schema subset; the Python request-codec test covers this boundary.

## Parser field outcomes (SDK-533, M45-observe)

The tradition file method joins the exact `CTraditionType` constructor, root member reader,
String-reader return, malformed-report routine, and the return from `LoadFromReader` while its
reader is still alive. The statically established String fields are `custom_tooltip`,
`custom_tooltip_with_modifiers`, and `unlocks_agenda`; they share reader identity
`325efaa17499c32d`. The constructor establishes an omitted definition without parsing fixture text.
Each joined field return reads actual owner storage, and the file terminal reads it again.

Diagnostics are intercepted at `CReader::ReportMalformed(CString const&)` and
`CReader::ReportUnexpected(CString const&)`, the overloads that create their own reader error
entries, so the no-argument forwarding overloads do not duplicate them.
The multiline quoted-string control reports the engine text `Malformed token` on line 3, joins it
to the requested field occurrence, and stores the independently observed value `Unreadable String`.
The coverage terminal means only that parser diagnostic collection completed for this file load.
No M45 mechanism in this method reaches post-read validation, a world, or gameplay runtime;
requested runtime is an explicit unavailable outcome and `OutsideMethod` gap.
