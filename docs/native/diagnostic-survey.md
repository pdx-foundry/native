# Information in engine diagnostics (M45-release)

What engine messages state about a rule, from the generated-fixture and config-test spikes on the
exact M45-release build. Atlas owns the fixtures, matrices and rule conclusions in
`/Users/jackson/Developer/pdx-foundry/atlas/docs/prototypes/generated-fixture-spike/` and
`config-test-spike/` (ignored by Atlas Git); checked copies are
`.local/evidence/generated-fixture-spike-2026-09-28.tar.gz` and
`.local/evidence/config-test-spike-2026-09-28/`. No acceptance is inferred from silence alone.

| Message family | What it states | Route |
| --- | --- | --- |
| Wrong current scope | The current scope and the **supported scopes**; three tested effects matched their declarations exactly | `CEffect::ReadMember` passes the mask from vtable slot `+0x80` to `NEventScope::GetScopeDesc` (`Supported Scopes:`); `CPdxLogFileAndLine` during parsing |
| Boolean value | `%s effect at %s accepts only yes or no as values` (`CBoolEffect::Assign`), then returns true, so a parser return is not validity | `CPdxLogFileAndLine` |
| Malformed numeric text | Only `Malformed token` at the bound reader hook; no type | Reader hook; the ordinary log adds the token |
| Unknown child key | Only `Unexpected token`; no key set | Reader hook; the ordinary log adds the key |
| Wrong target type | `has a target of not fleet type!` for `auto_follow_fleet` with `target = owner`, staged `engine-validation-log` | `CAutoFollowFleetEffect::PostValidate` bypasses the log for type 0 or `0x40`; the message does not give the actual type |
| Unknown weight key | `unknown command '<key>' for MTTH/script value in file <file> line : <n>` (M451-hotfix) | `CLogger::Log` in `CMeanTimeToHappen::ReadMember`; the fixture source join reads `file: ` and ` line: `, so this message reaches only `error.log` |
| Entry outside a container's categories | `Modifier has entry not allowed by category:  file: <file> line: <n>` (M451-hotfix); no key and no category; the entry stays stored; a plain container logs it only for an entry whose own mask is 0 ([container masks](modifier-masks.md#container-masks-sdk-709)) | `CPdxLogFileAndLine` in `CPdxModifier<…>::TryReadMember`; reaches fixture diagnostics as `engine-parser-log`, joined to the entry line |
| Missing deferred reference | `Failed to deferred read key reference <key> from database  file: <file> line: <n>`; the argument after `from database` is a source description, not the database name | `CTechAndLevel::ReadDeferred` → `ResolveReference`, `CPdxLogFileAndLine` |

The reader hook captures messages before final formatting; this is not string truncation. The
worker's 4,096-byte C-string bound refuses a read of 4,095 characters or more.

## Pitfalls

- **Match the command identity, not only stage and line.** `set_planet_class` is not the
  class-changing effect on this build (`change_pc` is): it gives a blank-name Boolean message
  while parsing and an invalid-scripted-effect message in validation, so a wrong-scope control
  built on it passed for an unrelated error.
- **Source filtering hides useful messages.** A missing technology also logs
  `Invalid technology being referenced: "<key>"` from `technology_level.cpp:41` with no source
  file or line, so Native's source filter drops it. Name-only messages (traditions without a
  category, trigger-only wrappers listing `custom_tooltip`, `custom_tooltip_with_modifiers`,
  `modifier`, `triggered_modifier`, `on_enabled`, `unlocks_agenda`) also never reach the
  source-correlated list. Isolating traditions replaces normal definitions, so background category
  errors appear; the absence of a source-correlated error is never global log cleanliness.
- **A quiet target check may not be reached.** `target = this` and `target = root` were quiet
  through validation; only `owner` reached the not-fleet message. A target answer needs a positive
  control on the same route.
- **A positive control must exercise tokenization**, not just a string reader that accepted the
  last text (see the whitespace pitfall in [engine calls](engine-calls.md#pitfalls)).
- **The old `pop` bit is still accepted** by `has_citizenship_rights`, `member_of_faction` and
  `is_on_galaxy_map`; their static declarations agree. Accepting a bit does not mean that a current
  authoring context can supply that scope.
- **Validation can reject a surrounding object first.** `create_ambient_object` without an object
  type reports an invalid ambient object even when the tested key is accepted.
- **A CWT `bool` declaration does not prove a Boolean route.** `has_building_construction` reports
  nonexistent buildings for malformed scalars, and `set_disable_at_health = yes` rejects the value
  in its valid scopes. `add_tradition` gave no unknown-item rejection.
- **Repeated probes can say nothing about cardinality**: parse and validation do not report
  overwrite, requiredness or execution.
- The `PrintScriptingDocumentation` dumps agree with Native on effect and trigger names, available
  scopes, and all 45,578 loaded modifier names and tags; they omit Native-known scope links and
  localization entries, and `scopes.log` is not the scope-type inventory.
