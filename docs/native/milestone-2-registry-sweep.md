# Milestone 2 frozen registry-field sweep

Run on 2026-09-21 against the exact M45-observe executable
`3d4c8a7046d87175ce7e3b513b1a2ce589050d654d332744518a49d13ac82216`.
The method was `registry-fields/v2`; no method or binding change preceded the run.
The full report, with every answer, gap, reader identity and query time, was removed in
SDK-603. Retrieve it with `git show 866e2ea:docs/native/milestone-2-registry-sweep.json`.
The current method gives a new result, not a reproduction of these v2 answers.

| Measure | Result |
| --- | ---: |
| Named registries | 164 |
| Successful / failed queries | 164 / 0 |
| Queries with an unresolved path | 101 |
| Fields found | 873 |
| Unresolved paths | 101 |
| Fields with known / unknown reader identity | 652 / 221 |
| Fields with known / unknown reader kind | 642 / 231 |
| Distinct known reader identities | 19 |
| Total elapsed time | 217.5 seconds |

The largest reader identity, `325efaa17499c32d`, handles 170 fields. It is a string
reader for three fields in `common/traditions` and the same three fields in the
previously unobserved `common/ascension_perks`. That makes it the R1 transfer candidate.
The next reader identities cover 119 block fields and 101 block fields. Every query
finished; no method time or resource limit stopped this run. The method still reports
its own bounded-search gaps. For example, `common/ai_budget` has an unresolved path.

## Comparison with the Milestone 4 baseline

The [Milestone 4 field baseline](milestone-4-field-baseline.md) ran `registry-fields/v3` on
M45-release. Compared with this report, it has the same 164 registries. Outside
`common/megastructures` it has the same fields, reader identities, reader kinds and gaps.
There are two differences:

- **Five megastructure fields.** v3 on M45-release adds `overclock_loc_key`,
  `overclock_cooldown`, `dismantle_possible`, `dismantle_potential` and
  `should_ai_dismantle`, and one broad-form gap for `overclock_cooldown`. On M45-observe the
  compiler dispatched these tokens through a jump table that v2 did not follow. The
  [registry field notes](registry-fields.md#compiler-jump-tables) hold this finding.
- **Completeness.** v2 reported every answer as partial. v3 derives completeness and reports
  28 answers as complete.

Reader IDs come from callee names, so the same 19 IDs appear on both builds.
