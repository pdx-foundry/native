# Population baselines

These compact answer baselines cover the catalogued build ([targets](../../../docs/native/targets.md)).
Each file stores the exact build it covers. The counts of each method are on its
[knowledge page](../../../docs/engine-knowledge.md).
Generate and review updates with the commands in
[method authoring](../../../docs/native/method-authoring.md#run-over-the-whole-population).
Update the affected baseline in the same PR that changes method answers. Each subject occupies
one JSON line. Only comparison inputs are tracked: answers, errors, status and command inventory
uncertainty. Generate full diagnostic reports under the ignored `.local/population/` directory.
`registry-sweep.json` holds statuses, not answers: each registry's status and field counts for
the eight sweep relationships, from a recording of the three registry questions
([sweep report](../../../docs/native/method-authoring.md#report-the-registry-sweep)).
Two runs of each baseline produced identical bytes and zero changed answers. The command grammar
baseline takes about three minutes.

`command-fixture-sample.json` is the SDK-548 live fixture sample, fixed before the final
population run: the ordering rule, the eligible commands of each kind, and the 20 chosen. It was
selected on M45-release and keeps that build stamp; `cargo live fixture_argument` checks the same
20 commands on the catalogued build.
