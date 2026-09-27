# M45-release population baselines

These compact answer baselines cover executable SHA-256
`07988b4f1b865623becd7a61af1cae92e111be6515d341754af70f02107822cd`.
Generate and review updates with the commands in
[method authoring](../../../docs/native/method-authoring.md#run-over-the-whole-population).
Update the affected baseline in the same PR that changes method answers. Each subject occupies
one JSON line. Only comparison inputs are tracked: answers, errors, status and command inventory
uncertainty. Generate full diagnostic reports under the ignored `.local/population/` directory.
Two runs of each baseline produced identical bytes and zero changed answers.

| Report | Population | Complete | Partial | Failed |
| --- | ---: | ---: | ---: | ---: |
| Registry fields | 164 registries | 10 | 154 | 0 |
| Effect grammars | 1,074 named commands | 0 | 1,074 | 0 |
| Trigger grammars | 1,096 named commands | 0 | 1,096 | 0 |

## Historical comparison control

Reports regenerated at `f184f08` and `ab9cc7f` establish 20 changed registries after default-member
normalization, not the 12 proposed in SDK-637/SDK-638. Twelve have reader/shape/gap changes; eight
more have only non-default reference lookups: `common/anomalies`, `common/archaeological_site_types`,
`common/astral_rifts`, `common/council_agendas`, `common/event_chains`, `common/planet_modifiers`,
`common/situations`, and `common/starbase_levels`. Dropping *all* reference members produces 12,
but incorrectly hides those established lookups. The comparison preserves them.

The default summary names `value[].reference` and `value[].members.Fields[].reference` once each.
The historical method stamp also changes from `registry-fields/v6` to `registry-fields/v7` for
all 164 answers. Comparisons ignore method and Native version stamps, but retain build identity,
basis, values, completeness, gaps and errors. The stored baselines keep the full source stamps.
