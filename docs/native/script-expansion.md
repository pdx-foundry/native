# Script expansion

`Native::script_expansions` (`script-expansions/v1`) tells how script reuses text. It covers
inline scripts, scripted effects, scripted triggers, script values and scripted variables. For
each one it gives:

- where script can use it, and where its names are defined;
- when the engine expands a use;
- its call forms and parameter forms, and what an absent parameter yields;
- the load-time checks.

The method is `src/engine/analysis/expansions.rs`. The binding is
`src/binding/binary/expansions.rs`, which holds the engine function names, the caller roles and
the stated forms. The session is `src/session/expansions.rs`.

Scripted modifiers are not an expansion. They are modifier names, and
[modifier families](modifier-families.md) gives them.

## Engine facts (M451-hotfix)

| Mechanism | Definitions | Expanding function | Stage evidence |
| --- | --- | --- | --- |
| Inline script | `CInlineScriptDatabase::Init` enumerates `common/inline_scripts`; the name is the path without the extension | `CreateInlineScriptReader(CReader&)`: reads `CInlineScriptParameters` (`script`, and every other key as a parameter), replaces `$KEY$` in a copy of the file text for each given parameter, and reads the copy with a new `CReader` | Called by the readers themselves |
| Scripted effect, trigger, script value | `CMetaScriptTemplate` in the template databases of `common/scripted_effects`, `common/scripted_triggers` and `common/script_values` | A use is a placeholder: `CScriptedEffect(CString)` in `CEffect::ReadMember`, `CScriptedTrigger(CString)` in `CTriggerCollectionBase::ReadMember` and `CTriggerDatabase::CreateTriggerOrScriptedPlaceholder`, and `CScriptableLookupValue` in `CVariableValue::ReadTriggerModifierOrScriptValue` | `GenerateSource` is called only by each `BuildFromSource` and by the error path `CScriptedEffect::OnError`. Each `BuildFromSource` is reached only from its `PostInit`, plus the script-value console command |
| Scripted variable | `CReader::RegisterVariableInList`: the reader's own list from `ReadSimpleStatement`, and the global list from `ReadGlobalVariables`. `InitGame` passes `common/scripted_variables` to `CGlobalScriptedVariablesDatabase::InitializeWithDirectory` | `ParseAdvancedStatementWithVariables`, called only by `CReader` functions; it reads both the reader's list and `CReader::s_GlobalVariables` | Lexing: only the shared reader class calls it |

**Names of expanded sources.** Diagnostics name these sources, and the fixture route joins them
to the line of the call:

- An inline script's reader is named `<file>:<line>(inline_script) <script path>`. `<line>` is
  where the call's block ends, because the reader has read the parameters by then.
- A generated instance is read by a reader named
  `scripted effect <name> at file: <file> line: <line>`. Its own line numbers count lines of the
  generated source.
- A deferred validation message from an instance names the line in the definition, as
  `common/scripted_effects/00_scripted_effects.txt:8853 @ scripted effect <name> at file: …`.

**Checks.** Each message literal is the `x1` argument of the next call, to
`CPdxLogFileAndLine::operator()`, `CLogStream::operator<<`, `PdxStrFmt<512>`, or a `CString`
builder:

| Check | Message |
| --- | --- |
| Unknown inline script | `Unknown inline_script "` |
| Unknown scripted effect | `Script Error: Invalid scripted effect: ` |
| Unknown scripted trigger | `Error in scripted trigger, cannot find` |
| Unknown script value | `Script Error: Invalid script value` |
| Depth limit | `CRITICAL: Max effects/triggers/script values post init recursive depth` |
| Missing parameter | `Compiling source for <name> failed for missing args: …` (`ValidateArguments`) |
| Invalid variable name | `Invalid variable name [%s]` |
| Duplicate variable name | `Variable name %s is already taken` |

## Result

All five mechanisms are answered. The answer is `Partial` because of two gaps:

- **Unjoined inline-script readers.** The inline reader has 173 direct callers. 164 are template
  registry roots, joined to every named registry. 3 are shared readers: the effect block, the
  trigger block and the persistent object block. 6 are custom readers that have no named content
  directory: events, message types, static modifiers, two scripted-action readers and traits.
- **Lookup order of scripted variables.** The static answer does not establish it.

Placeholder sites:

| Placeholder | Joined | Excluded |
| --- | --- | --- |
| Scripted effect | 1 | 0 |
| Scripted trigger | 4: through one helper pair, 2 trigger-reference keys and the scoped operand | 0 |
| Script value | 2 | 1, the console command |

Every placeholder site constructs its object on a new allocation. Every caller of the variable
expression parser is a `CReader` function.

## Fixture checks

`tests/live.rs` runs `script_expansion_templates` and `script_expansion_variables`. Each case
first reads the static answer. Every row below joins to the line of its call. A row that cannot
tell its form from its absence would remove that form from the stated rule.

**Inline scripts:**

| Row | Shows |
| --- | --- |
| `tradition = "native_x inherit_icon = native_bad"` makes a second statement in a tradition member (object block). Its malformed Boolean is reported near line 3 of the inline text, before the loader returns. The `native_ok` control is quiet | Substitution, one parameter holding several statements, the read stage |
| `shroud/add_percentage_to_resource` in `on_enabled` (effect block) with a bad resource reports it. The `energy` control is quiet. Without `RESOURCE`, the error names `$RESOURCE$` | A second host. An absent parameter is kept as text |

**Templates:**

| Row | Shows |
| --- | --- |
| `pop_group_add_ethic_effect = { POP_GROUP = native_bad_target }` fails at definition line 8853, inside `[[POP_GROUP]`. Without `POP_GROUP` it is quiet | Substitution, the conditional |
| `pop_group_transfer_effect` without `POP_GROUP` fails on its `OLD_ETHOS` inside `[[!POP_GROUP]`. With `POP_GROUP = this` it is quiet | The negated conditional |
| `store_galactic_community_leader_backup_data` with a bad `ROOM` reports generated line 11 only. With `FLAG = native_flag` it reports lines 10 and 11 | `$FLAG|no$` gives `no` when the parameter is absent |
| `native_missing_effect = yes` gives `Invalid scripted effect` in validation | The value form and an unknown definition |

**Script values.** `value:skill_scaled_age_increase|SKILL|1|` is quiet. `|SKILL|` gives
`Uneven number of parameters` while the file is read. `value:native_missing_value|K|v|` gives
`Invalid script value` in validation.

**Scripted variables** (special-project `requirements.fleet_power`):

| Use | Stored value | Shows |
| --- | --- | --- |
| A local `@native_local = 7` | 7 | The file's own definition |
| `@discovery_weight` | 3 | A global definition |
| A local redefinition of `@resettlement_unity` (global 10) | 11 | The file's definition is looked up first; there is no duplicate diagnostic |
| `@[ native_local + discovery_weight ]` | 10 | Variables named without `@` inside an expression |

## Gaps and limits

- **A missing parameter is one that the definition writes as a plain `$KEY$` outside the
  conditional blocks that the use drops.** A use that omits a parameter written only as
  `$KEY|default$` or inside `[[KEY] … ]` is valid; the fixture rows above omit such parameters
  with no diagnostic.
- **The missing-parameter message is not joined to a call.** It names the template, not the use.
  `MissingParameter::Diagnostic` rests on the static check alone.
- **The unknown inline-script message is not captured by fixtures.** The inline reader sends it
  piece by piece to a `CLogStream`, and no fixture hook reads that stream. The check rests on the
  static message argument.
- **Duplicate definitions** are an `OutsideMethod` gap on every mechanism with a definition
  directory: which definition a name selects when two files define it is not established
  (SDK-552).
- **Cycles and forward definitions** are `OutsideMethod` gaps. The depth check is established; a
  cycle needs authored definitions, and the lookup's result for a later definition was not
  traced.
- **Outside the method:** comparing a scripted
  trigger's value (`== int`), `optimize_memory`, and escapes such as `\$`.

## Stated forms (recorded manual exception)

- **Claim.**
  - Scripted effects and triggers take `name = value` and `name = { KEY = value }`. Script
    values take `value:name|KEY|value|`.
  - All three use `$KEY$`, `$KEY|default$`, `[[KEY] … ]` and `[[!KEY] … ]`.
  - An inline script takes `inline_script = { script = path KEY = value }` and `$KEY$`, and keeps
    an absent parameter as text.
  - Scripted variables take `@name` and `@[ expression ]`.
- **Conditions.** `CMetaScriptTemplate::ParseForArguments`, `ProcessSourceForMacros`,
  `CInlineScriptParameters::ReadMember` and `CReader::ReadSimpleStatement` are bound. If one is
  missing, the forms are unresolved.
- **Obstacle.** The forms are character and token-kind tests inside one scanner or reader loop.
- **Removal route.** A scanner-shape reader.

The fixture rows above check each stated form on scripted effects. Triggers and script values
share the scanner. The inline `inline_script = path` value form has no discriminating row, so it
is not stated.

## Pitfalls

- **Do not bind the `CLogStream` sink.** Retargeting the stream-log site to the `CLogStream`
  destructor's logger call (`0x1025074ac`, the message in `x4`) does catch every stream message.
  But it slows loading enough that a validation fixture reaches its deadline, as the shared file
  logger does.
- **Read an instance's source after `GenerateSource`.** The sourced-log site is the instruction
  after `GenerateSource` returns in `CScriptedEffect::OnError` (`0x101d2206c`). There, `x19` is
  the placeholder (source at `+0x28`), `x20` is the error, and the generated source is at
  `x29 - 0x50`. The message itself is built piece by piece on a stream.
- **A placeholder constructor's tail calls are not uses.** One `CScriptableLookupValue`
  constructor continues in another through `b`. Count only `bl` calls as uses. Keep tail calls
  in stage chains, where `PostInit` reaches `BuildFromSource` through `b`.
- **A frame-relative temporary.** `InitGame` builds the global-variable directory at
  `x29 - 0x98`. The directory scan follows `sub`, and an `add` or `sub` from a stack-derived
  register.
