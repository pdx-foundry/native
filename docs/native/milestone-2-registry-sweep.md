# Milestone 2 frozen registry-field sweep

Run on 2026-09-21 against the exact M45-observe executable
`3d4c8a7046d87175ce7e3b513b1a2ce589050d654d332744518a49d13ac82216`.
The method was `registry-fields/v2`; no method or binding change preceded the run.
[`milestone-2-registry-sweep.json`](milestone-2-registry-sweep.json) retains every answer,
gap, error, reader identity and query time. Reproduce with
`cargo run --example registry_field_sweep -- "$STELLARIS_PATH"`.

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

The JSON is the frozen baseline for later repairs. Once a case informs a repair, it is
a regression case, not a held-out transfer test.
