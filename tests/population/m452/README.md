# M452 population baselines

These compact answer baselines cover executable SHA-256
`c621723d9c8e0c1cd153319208d30a9dfbb9e63675be86f9d0ae7debeaa7fe1b`.
Generate and review updates with the commands in
[method authoring](../../../docs/native/method-authoring.md#run-over-the-whole-population).
Update the affected baseline in the same PR that changes method answers. Each subject occupies
one JSON line. Only comparison inputs are tracked: answers, errors, status and command inventory
uncertainty. Generate full diagnostic reports under the ignored `.local/population/` directory.
Two runs of each baseline produced identical bytes and zero changed answers. The command
grammar baseline takes about three minutes.

`command-fixture-sample.json` is the SDK-548 live fixture sample, fixed before the final
population run: the ordering rule, the eligible commands of each kind, and the 20 chosen. It was
selected on M45-release and keeps that build stamp; `cargo live fixture_argument` checks the same
20 commands on M452.

| Report | Population | Complete | Partial | Failed |
| --- | ---: | ---: | ---: | ---: |
| Registry fields | 164 registries | 12 | 152 | 0 |
| Effect grammars | 1,080 named commands | 163 | 915 | 2 |
| Trigger grammars | 1,098 named commands | 122 | 976 | 0 |

Compared with M451-hotfix, after the `accepted_categories` member that SDK-708 added: eight new
commands; civics lost `multiply_by_habitability_effect_modifier` and edicts gained
`relay_network_modifier`; six `set_ai_*` armor, shield and weapon-preference effects have new
reader identities; and `is_species_class` keeps the same value but now stops at value acceptance
in `PostValidate` instead of the form reader call and reference lookup. Each is a 4.5.2 change.

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
