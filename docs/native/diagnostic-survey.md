# Diagnostic-channel survey, 2026-09-28

The bounded generated-fixture spike completed on the exact M45-release build in
[targets](targets.md). Findings do not transfer to other builds without checks. Native method
`observe-fixture/v4` provided all observations. No worker, hook, supervisor, public operation or
decoder was added.

Atlas owns the fixtures, question matrix, inference scores and rule conclusions. The retained
experiment is `/Users/jackson/Developer/pdx-foundry/atlas/docs/prototypes/generated-fixture-spike/`:
`REPORT.md`, `observations.md`, `classifications.json`, `scores.json`, `cases.json`, `fixtures/`,
`results/`, `static/`, the Rust caller under `runner/`, and the selection/extraction scripts.
Atlas ignores that prototype directory. A second complete local copy is
`.local/evidence/generated-fixture-spike-2026-09-28.tar.gz`, with a checked inventory beside it.
Preserve a verified usable copy before cleanup; neither copy is a remote archive.

## Measured reach

The original prioritized survey used ten game launches and 735.683 seconds. After the user
allowed promising extensions, the finished study used **24 launches, 1,932.249 live seconds,
and 98 field questions**. Every launch confirmed disposal. Every requested bound diagnostic
window completed and every outer-field question observed paired parser entry/return with
complete parsing. These are finite live observations, not static-population coverage.
No runtime world was loaded. Unavailable storage stayed separate from parsing and diagnostics.

The first effect specimen was invalid and remains in the denominator; two replacement sessions
used the verified effect. The remaining launches tested three additional scope subjects, three
Boolean subjects, isolated message repetition, key/form errors, target validation and a deferred
reference. The recorded intervals include start-game work and cleanup and conservatively exceed
just process-alive time. Build/setup commands and offline analysis are separate from live time.

## Wrong current-scope messages

`CEffect::ReadMember` builds its final placeholder from virtual slot `+0x80`, passes the returned
mask to `NEventScope::GetScopeDesc`, and trims the resulting string. `GetScopeDesc` labels the
list `Supported Scopes:` and enumerates mask bits. The fresh disassembly matches the retained
SDK-548 inspection on this exact build. This is static evidence, separate from rendered messages.

The live `change_pc` message contains country as the current scope and planet, ship, colony as
supported scopes. The trigger `is_planet_class` message lists planet, ship, dlc_recommendation,
colony. The full multiline messages arrive through `CPdxLogFileAndLine` during parsing and match
the ordinary game-log body. Wrapped controls and corrected cases retain complete parsing and
bound diagnostic collection without that rejection.

The preselected follow-up effects `set_planet_flag`, `set_star_flag`, and `set_fleet_flag` each
produce a list exactly matching the independent declaration answer. Corrected controls and an
isolated planet-flag repetition agree. This measures the diagnostic family's accuracy against
known answers; it is not a new declaration discovery or a proof about runtime object availability.

### Invalid effect control found

`set_planet_class` is not the registered class-changing effect on this build. It produces a
blank-name Boolean message during parsing, then an invalid-scripted-effect message during
validation, even inside the proposed valid scope. `change_pc` is the verified registered effect.
The live harness previously checked only diagnostic stage and location, so its supposed
wrong-scope effect control could pass on the unrelated Boolean error. The specimen in
`tests/live.rs` now uses `change_pc`; the new paired live observations support the replacement,
and the test target compiles. Earlier test passes cannot substantiate the old scope claim.

## Boolean and generic reader errors

`CBoolEffect::Assign` logs `%s effect at %s accepts only yes or no as values` through
`CPdxLogFileAndLine`, then returns true. An outer parser return alone therefore cannot establish
validity. Named messages reject integer, fractional and text samples for `stop_crisis_sound`;
its yes/no controls and corrected cases have no matching error. Three preselected additional
commands and an isolated repetition give the same domain, agreeing with their complete static
Boolean grammars. Empty-block input to the calibration command also gives this same message.
That one block probe does not establish behavior for arbitrary scalar/block readers.

The invalid effect above reaches a blank-name instance of this message family. An extractor
must require the requested command identity and source join; matching a phrase alone would
turn a wrong command into a false Boolean-type conclusion. The experiment's extractor checks
those boundaries; offline verification tests a mismatched source and the blank-name case.

Direct integer and fixed-point malformed readers both report only `Malformed token` at the
bound reader hook. The normalized answer supplies file, line, field and occurrence separately.
The ordinary game log includes the offending token and its later source formatting. An unknown
`leave_alliance` child key similarly gives `Unexpected token` at the hook; the ordinary log adds
the key. Corrections remove the intended errors, but these messages enumerate no type or key set.
This omission is capture before final formatting, not string truncation. The worker's existing
4,096-byte C-string bound refuses a >=4,095-character read; none of these messages hit that bound.

## Target validation differs from input-scope validation

The initial `auto_follow_fleet` experiment uses `every_owned_fleet` and changes only `target`
between `this` and `root`. Both are quiet through the bound validation window. The target's
concrete type is not established by those fixtures; they remain unresolved for the intended
wrong-country-target question.

A second pair changes the target to `owner`. This produces the source-correlated message
`has a target of not fleet type!` through `CPdxLogFileAndLine`, staged as `engine-validation-log`.
The otherwise identical `this` correction is quiet. This supplies the missing positive control
for that reporting route and a finite target rejection, while retaining the first pair's limit.

Static `CAutoFollowFleetEffect::PostValidate` bypasses the not-fleet log for type zero or `0x40`
(fleet); other resolved types reach it. `CEventTarget::GetScopeType` follows the terminal target
node and dispatches on its token. The live message does not report the actual target type or
explain the unknown-type bypass. No complete accepted target set or runtime object availability
follows. The public whole-chain target answer remains unresolved because execution checks are a
separate obligation. This finding does not justify relabeling that public result as complete.

## Deferred references and source-filter gaps

`CHasTechnologyTrigger::Assign` calls `CTechAndLevel::ReadDeferred`, registering with the global
deferred resolver. `ResolveReference` logs through `CPdxLogFileAndLine`. The live message is:

```text
Failed to deferred read key reference atlas_missing_technology from database  file: common/traditions/atlas_spike_reference_wrong.txt line: 5
```

The argument after `from database` is a source description, not the database's name. The ordinary
log separately reports `Invalid technology being referenced: "atlas_missing_technology"` from
`technology_level.cpp:41`, without a source filename/line. Native's source filtering excludes that
useful message. Changing only the reference to installed `tech_lasers_1` removes both errors.
A controlled unique-token association is available here, but no general cross-log join was added.

The ordinary logs also name fixture traditions without a category, and trigger-only wrappers
without effects. The latter explicitly lists alternatives: `custom_tooltip`,
`custom_tooltip_with_modifiers`, `modifier`, `triggered_modifier`, `on_enabled`, `unlocks_agenda`.
These name-only messages do not reach the public source-correlated diagnostic list. Their
required-content hint is retained but was not independently calibrated across the alternatives
or conditions. Isolating traditions replaces normal definitions, so unrelated background category
errors also occur. Absence of a source-correlated error is never global log cleanliness.

## Recommendation and limits

Both scope and Boolean direct-extraction families passed three-subject transfer trials with no
false, partial or unavailable answer; isolated repeats agreed. Numeric and key messages support
only controlled candidate rejection. Target and reference messages offer useful partial facts
with the limits above. Other missing keys, enums, wrong-family children and scalar-to-block
readers remain untested. No acceptance is inferred from silence alone.

This study does not establish that generation is cheaper. The recorded same-question engine
comparison for `set_fleet_flag` found its factory/receiver and a two-instruction scope-mask body
before reading its live message. Prior familiarity and an interrupted wall interval prevent a
clean active-labor comparison. The Atlas report separates live costs from this measurement limit.
Further work should be a bounded trial of the proven message families, not a runtime harness or
new observation API. Full messages, every specimen, both capture surfaces and all unavailable
results remain in the Atlas matrix.

## Config-test calibration follow-up

The separate config-test spike used the same exact M45-release build, ordinary logs and the
in-process parse/validate route described in [engine calls](engine-calls.md). No world was loaded.
Its six launches include a failed control and five successful sessions, all with confirmed
disposal. Atlas owns the frozen 89-claim matrix and decisions in
`docs/prototypes/config-test-spike/REPORT.md`. Preserve its raw logs and scripts; the checked
second copy is `.local/evidence/config-test-spike-2026-09-28/`.

Route details that affect diagnostic interpretation; the whitespace, scope-width and
sustained-session facts are in [engine calls](engine-calls.md#config-test-spike-pause-and-token-controls):

- A positive control must exercise tokenization, not just a string reader that happened to accept
  the last text.
- The old `pop` bit is still accepted by `has_citizenship_rights`, `member_of_faction`
  and `is_on_galaxy_map`. Their static declarations agree. Acceptance of a bit does not establish
  that a current authoring context can supply that scope.
- Validation can reject a surrounding object before a field property is controlled.
  `create_ambient_object` without an object type reports an invalid ambient object even when
  the candidate key is accepted. Those key rows remain No comment. A CWT `bool` declaration
  also does not prove a Boolean route: `has_building_construction` reports nonexistent buildings
  for malformed scalar inputs, and `set_disable_at_health = yes` rejects the value in its valid
  scopes. `add_tradition` did not supply the required unknown-item rejection. No silence was
  credited on these routes.

Line-distinct snippets provide the ordinary-log join and avoid identical-message suppression.
Repeated cardinality probes remain No comment: parse and validation say nothing about overwrite,
requiredness or execution.

The documentation dumps that `PrintScriptingDocumentation` writes agree with Native on effect
and trigger names, available scopes, and all 45,578 loaded modifier names and tags. The dumps
omit Native-known scope links and localization entries; `scopes.log` is not the scope-type
inventory. See the Atlas
both-direction comparison before treating any missing dump entry as engine absence.
