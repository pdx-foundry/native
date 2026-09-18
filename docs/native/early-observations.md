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

Carry **activation**, **observation completion**, and **confirmed disposal** as separate facts. Empty output with an unresolved hook is unavailable; record loss prevents completion; worker loss does not erase already collected observations. Native owns target-specific addresses, argument conventions, source/owner joins and cleanup. Atlas supplies observation requests, fixtures and a deadline.

Fresh capture requires the exact M45-observe installation/content, ARM64 host, Xcode/LLDB and debugger access. The original `replay.py --scenario all` **builds a guard and launches games**; its name does not mean offline replay. This consolidation did not run it. The offline migration check reads manifests, source hashes, traces and final ownership records, described in [retrieval](retrieval.md).

No production adapter, Windows timing, database-constructor order, late/hot reload, arbitrary parser stage, owner-loss recovery or low maintenance cost is established. Atlas's accepted consumer clarification is retained at `/Users/jackson/Developer/pdx-atlas/docs/prototypes/early-observation.md`. Local Linear acceptance is `linear-records/linear/SDK-483-comments.json`; the review summary's earlier pending label remains historical.
