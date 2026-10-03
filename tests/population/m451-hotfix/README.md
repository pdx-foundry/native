# M451-hotfix population baselines

These compact answer baselines cover executable SHA-256
`29fa877366040a528098da39ec7e70b7baac76782a2a6bd161616d691f86fa38`.
Generate and review updates with the commands in
[method authoring](../../../docs/native/method-authoring.md#run-over-the-whole-population).
Update the affected baseline in the same PR that changes method answers. Each subject occupies
one JSON line. Only comparison inputs are tracked: answers, errors, status and command inventory
uncertainty. Generate full diagnostic reports under the ignored `.local/population/` directory.
Two runs of each baseline produced identical bytes and zero changed answers. The command
grammar baseline takes about 80 seconds.

`command-fixture-sample.json` is the SDK-548 live fixture sample, fixed before the final
population run: the ordering rule, the eligible commands of each kind, and the 20 chosen. It was
selected on M45-release and keeps that build stamp; `cargo live fixture_argument` checks the same
20 commands on M451-hotfix.

| Report | Population | Complete | Partial | Failed |
| --- | ---: | ---: | ---: | ---: |
| Registry fields | 164 registries | 8 | 156 | 0 |
| Effect grammars | 1,074 named commands | 248 | 819 | 7 |
| Trigger grammars | 1,096 named commands | 119 | 975 | 2 |

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
