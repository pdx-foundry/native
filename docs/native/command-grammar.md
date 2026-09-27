# Nested command grammar

These findings apply only to M45-release and its ARM64 slice, identified in
[targets](targets.md). The method is `command-grammar/v6`; field families use
`registry-fields/v7`, and parser observations use `observe-fixture/v2`. The method includes
[reference lookups](references.md), receiver initializer lookups and out-of-line factories.
The current forms counts are below; the retained child-reader measurements appear in their
original sections.

## Forms and the stage chain

`forms` describes two separate facts. A `Block` entry establishes that the outer reader
reaches the block or member reader. A `Value` entry establishes an alternative only when
its whole read, assignment, initialization and validation chain accepts it. It does not
establish runtime meaning. A known list is exhaustive within the method; a partial list
contains only established alternatives. In particular, handing a scalar to a base block
reader is not an established rejection.

Each value path keeps the command memory from `Read` through `Assign`, deferred references,
`PostInit` and `PostValidate`. Deferred references hold a found-item stand-in for acceptance.
The missing-key run instead supplies the lookup's bound null object and is reported separately.
A diagnostic on any stage makes the path rejecting. Without a diagnostic, every result-bearing
stage must return true in bit zero for acceptance. False, unknown, unfinished and bounded paths
are unresolved. An alternative is listed only if all its paths accept; mixed paths, unknown
reader kinds and more than 64 paths keep `forms` partial with a `value-acceptance` gap.
The diagnostic test is essential: the dispatch and database drivers ignore stage results.

A read watch records loads from command bytes that the same chain has not written. Script
writes, including the operator reader, do not become construction state. The factory walk
keeps only bytes agreed by every returning path, under the existing constructor and unknown-store
rules. It never enters constructor bodies to obtain more state. An unknown receiver branch
whose outcomes differ gives `receiver-state`; facts present on only one side are not listed.
Equal outcomes need no receiver-state gap. Cached results require the same slot functions
and the same watched initial bytes; the stand-in holds the real receiver vtable.

The initializer summary is deliberately narrow. `CAddDistrictEffect::PostInit()` performs an
inline scan and stores the selected item at command `+0xd0`. A proved inline initializer is
summarized only when the reference shape covers its entire body and its condition is `Always`.
An extra store, call or log prevents that summary. A conditional or unestablished lookup gives
`value-acceptance: PostInit: initializer not summarized`. For a called lookup, the method runs
the body and intercepts only the bound getter on the matched database and stored key.
These internal bindings do not change public reference lookup answers.

```sh
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 'CAddDistrictEffect::PostInit()'
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --trigger-grammar always
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --effect-grammar add_district
```

Known forms without `Block` give empty known child families, keys and ordering, and a known
absent numeric child. An inherited member reader contributes no children in that case.
Unknown or partial forms cannot promote child properties. `ReaderSemantics` marks incomplete
extraction only while a property is not known. Target scope checks and recursive block coverage
remain separate work.

### Current value-form results

The M45 population contains 1,074 effects and 1,096 triggers. A failed answer has every
property unresolved; a partial answer retains some established facts. The population audit
checks cache-key agreement, accepting paths for listed alternatives, and diagnostics for
omitted alternatives in a known list. No audit condition fails.

| Kind | Complete | Partial | Failed | `receiver-state` commands | `value-acceptance` commands |
| --- | ---: | ---: | ---: | ---: | ---: |
| Effect | 192 | 872 | 10 | 98 | 272 |
| Trigger | 1 | 1093 | 2 | 249 | 713 |

Counts below are commands per stage and cause; a command can have several causes.

| Stage and cause | Effects | Triggers |
| --- | ---: | ---: |
| `Assign: branch-value` | 1 | 95 |
| `Assign: false without diagnostic` | 1 | 179 |
| `Assign: form-reader-call` | 58 | 43 |
| `Assign: loop-limit` | 0 | 5 |
| `Assign: outside-code` | 0 | 6 |
| `Assign: path-limit` | 7 | 193 |
| `Assign: unfinished path` | 6 | 280 |
| `Assign: unknown result` | 1 | 74 |
| `PostInit: initializer not summarized` | 0 | 10 |
| `PostValidate: branch-value` | 67 | 131 |
| `PostValidate: form-stage-call` | 13 | 10 |
| `PostValidate: loop-limit` | 1 | 1 |
| `PostValidate: path limit` | 14 | 28 |
| `PostValidate: path-limit` | 1 | 2 |
| `PostValidate: unfinished path` | 79 | 59 |
| `PostValidate: unknown result` | 6 | 3 |
| `Read: mixed paths or unknown reader kind` | 70 | 242 |

False-result paths with no diagnostic anywhere in the chain: effects have 6 at `Assign` and
3,167 at `PostValidate`; triggers have 36,985 at `PostValidate`. `Read` and `PostInit` are void,
so they have no false-result count. Counts include each named command's paths, even when its
receiver shares a cached analysis. The full run takes about 84 seconds on the development host.

| Sample | Established result or named obstruction |
| --- | --- |
| `set_owner` | One known `Target` value; empty known children; complete within this chunk's method. Target scopes are not part of this answer yet. |
| `always` | Partial, no listed value: `Assign` has false-without-diagnostic and bounded paths, with unestablished receiver state. No target alternative is listed. |
| `add_district` | Partial with `Block`; the value branch stops at `Assign: form-reader-call`. The token text is copied through `strlen` and `__assign_external`, which is not the established tail-copy shape. |
| `has_tradition` | Its deferred `Reference` read joins the lookup, but `PostValidate: branch-value` prevents listing it. The missing-key chain remains unresolved too. |
| `set_country_flag` | The dynamic-name read is recognized as `String`; validation has unfinished paths and unestablished receiver state. Forms and inherited children remain partial. |

`tests/expected/m45/command-grammars.json` retains the first-release sample answers and their
named gaps. The complete unclassified `Read` and `Assign` function census, with per-shape
command counts, is retained in `.local/sdk-548/forms-stage-chain/form-reader-shapes.md`;
`population-summary.json` retains the stage/cause counts. Raw-string assignment, initializer
execution and receiver-state proof remain shared-method limits, not exceptions keyed by
command name. The property gate never removes children from these partial value answers.

## Engine facts on M45-release

Executable SHA-256 `07988b4f1b865623becd7a61af1cae92e111be6515d341754af70f02107822cd`;
ARM64 slice SHA-256 `a4cb49ad17a84ef6bf438019a50d3a66362c80731f8359888ddbce47c0d0aab9`.
These are current engine facts for `command-grammar/v6`, with `dynamic-names/v2` and
`registry-fields/v7`. They do not establish complete value grammars or target-scope answers.

### F1. The family dispatch always calls `Read`

`CEffect::ReadMember(CReader&, int, EScopeType)` finds the factory, calls its create method,
stores the token at command `+0x20` and the file location at `+0x28`, checks the command's own
scope (slot `+0xa0`, message "Wrong scope for effect"), and then calls the command's virtual
`Read` (slot `+0x10`) with the command, the reader and the scope. It never calls `Assign`.
**It does not read the result of `Read`**: the call is followed by the function's return. A
`Read` that tail-calls `Assign` returns the result of `Assign`, so the dispatch ignores that
result too.

Reproduce:

```sh
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 'CEffect::ReadMember(CReader&, int, EScopeType)'
```

### F2. Outer `Read` shapes (`--lookup-census '::Read(CReader&, EScopeType)'`: 244 bodies, 64 groups)

Count of joined commands for each outer `Read`, from `examples/command-population`:

| Outer `Read` | Effects (1,064 joined) | Triggers (1,094 joined) | What the body does |
| --- | ---: | ---: | --- |
| `CEffect::Read` / `CTrigger::Read` | 523 | 273 | Block. If the reader's value kind (`[reader+0x278]`) is not 3 and the command's byte at `+0x78` (effects) or `+0x60` (triggers) is 0, it logs `Expected "<name> = {", but got …` through `CLogger::Log` and `CLogStream`, then continues. On every path, the loop calls `CReader::ReadSimpleStatement()`, loads the key token from `[reader+0x38]`, and calls the virtual `ReadMember` (effects `+0x18`, triggers `+0x38`). Token `0x438` (inline script) makes a new reader, sets the byte to 1, calls `Read` again and restores the byte. |
| `CSimpleAssignEffect::Read` / `CSimpleAssignTrigger::Read` | 332 | 355 | Value. Tail call to the virtual `Assign` (effects `+0x20`, triggers `+0x28`) with `x1 = reader + 0x278`. The trigger form first calls `CAssignOperator::Read(CReader&)` into command `+0x64`. It does not test the value kind. |
| `CCompareTrigger::Read` | — | 242 | Value with a comparison operator: `CCompareOperator::Read(CReader&)`, then the virtual `Assign`. |
| `CDatabaseObjectEffect<D>::Read` / `…Trigger<D>::Read` | 61 | 120 | Reference. Tail call to `NParserUtil::ReadKeyReferenceDeferred<D>`. |
| `CEventTargetEffect::Read` | 39 | — | Target value (F4), stored at command `+0xa8`. |
| `CComplexIntEffect::Read`, `CComplexIntTrigger::Read`, `CComplexValue…::Read` | 55 | 66 | Tail call to the base block reader. |
| Both forms: `CAddDistrictEffect::Read` and 15 more bodies of one shape | 16 bodies | — | `[reader+0x278] == 3`: tail call to `CEffect::Read`. Otherwise: tail call to the virtual `Assign`. |
| Other bodies | 38 | 38 | One to eight commands each. |

Not joined: effects 7 `command-vtable`, 2 `factory-terminal`, 1 `instruction`; triggers 2
`command-vtable`.

Reproduce:

```sh
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --lookup-census '::Read(CReader&, EScopeType)'
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 'CEffect::Read(CReader&, EScopeType)'
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 'CTrigger::Read(CReader&, EScopeType)'
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 'CSimpleAssignTrigger::Read(CReader&, EScopeType)'
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 'CSimpleAssignEffect::Read(CReader&, EScopeType)'
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 'CCompareTrigger::Read(CReader&, EScopeType)'
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 'CAddDistrictEffect::Read(CReader&, EScopeType)'
```

### F3. `Assign`

- Signature `Assign(CToken const&, EScopeType)`. `x1` is the reader's value token at
  `reader + 0x278`. The token's id is at token `+0`; its text is at `reader + 0x288`.
  It returns a Boolean in `w0`. `CEffect::Assign` and `CTrigger::Assign` return 0.
- `--lookup-census '::Assign(CToken const&, EScopeType)'`: 999 bodies (lambdas included) in 103
  groups.
- `CSimpleAssignTrigger::Assign` makes an event target from each token (F4) at command `+0x68`,
  walks the target chain and returns true when the token is a scope keyword.
- `CBoolTrigger::Assign` calls that base first. If it returns false, it compares the token id
  with two literal tokens (`0x3fef`, `0x2cac`), stores the value at `+0x1f8` and sets `+0x1f9`.
  For another token it returns false with no log. It loads the operator token that `Read`
  stored at `+0x64`; `0x427` inverts the value.
- `CBoolTrigger::PostValidate() const` logs "A boolean trigger at %s has been assigned an invalid
  value. Expected: yes/no." when `+0x1f9` is not 1, and returns `+0x1f9 == 1`. So the parser
  takes a target for a Boolean trigger, and validation rejects it with a diagnostic.
- `CIntEffect::Assign` calls `CVariableValue::Assign(CToken const&, EScopeType, CString const&)`
  on command `+0xa8`.

Reproduce:

```sh
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --lookup-census '::Assign(CToken const&, EScopeType)'
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 'CBoolTrigger::Assign(CToken const&, EScopeType)'
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 'CBoolTrigger::PostValidate() const'
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 'CSimpleAssignTrigger::Assign(CToken const&, EScopeType)'
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 'CIntEffect::Assign(CToken const&, EScopeType)'
```

### F4. Event targets

- A reader stores a target with one idiom: `CToken::CToken(CToken const&)` from
  `reader + 0x278` to a stack temporary, then
  `CEventTarget::CEventTarget(CToken, EScopeType, CString const&)` to a second temporary, then
  `CEventTarget::operator=(CEventTarget&&)` with `x0 = owner + D`. The constructor calls
  `CEventTarget::ValidateScope`, which checks the chain of links, not the command's expectation.
- The same idiom reads a key: `create_starbase` `owner` stores at `+0x130`.
- **Explicit checks of the target's scope type are rare.** `CEventTarget::GetScopeType()` has 32
  direct callers. Those in command code: `CActivateGateway::PostValidate`,
  `CAutoFollowFleetEffect::PostValidate`, `CHasHyperlaneToTrigger::Assign`, and about ten
  `ExecuteActual` or `ActualEvaluate` bodies.
- **Most commands use a typed getter at execution.** `CSetOwnerEffect::ExecuteActual` starts
  with `add x0, x0, #0xa8` and `CEventTarget::AccessTargetCountryWithErrorLogging`. The typed
  getters are `CEventTarget::GetScope<Type>`, `GetTarget<Type>WithErrorLogging` and
  `AccessTarget<Type>WithErrorLogging`. Each calls
  `CEventTarget::GetScope(CEventScope&, char const*)`, which returns a scope object through
  `x8`, and then a typed accessor such as `CScopeObjectReference::GetCountry()`.
- Most typed accessors compare the scope's type field (`+0x8`) with one constant
  (`GetCountry`: 4, `GetShip`: 8) and return `TPdxNullObject<T>::_pInstance` for another type.
  `GetGrowthStage()` instead compares with `0x8000000000` and returns literal zero on mismatch.
  `GetGalacticCommunity()` and `GetObject<CGalacticCommunity>()` return the global state's
  community pointer without a type check or a null-object rejection.
- `CEffect::CheckScopeSupport*` and `CTrigger::CheckScopeSupport*` test the command's own scope
  (the getter at effects `+0x80`). They do not test a target argument.

The census of all 45 `CScopeObjectReference::Get*` bodies contains 41 typed accessors:
38 null-object accessors, one literal-zero accessor (`GetGrowthStage`), and two global-state
accessors (the Galactic Community pair). The four other bodies are `GetLocalPointer`,
`GetColonyCarrierRef`, `GetOpenerID`, and `GetObjectName`; they are not typed target accessors.
`GetDesign` uses `TPdxNullObject<CShipDesign>` and `GetDlcRecommendation` uses
`TPdxNullObject<SDlcRecommendationScriptData>`, so deriving these types from the method suffix
alone would give the wrong null object.

Pitfall: the literal-zero accessor has no bound rejection null object. The two global-state
accessors also have none and do not check the type. All three are explicitly bound as
`NoNullObject`; the target getter analysis must keep them unresolved. Treating any other return
value as acceptance would falsely accept every scope bit. **Literal-zero accessor rejection**
is a candidate follow-up, not a supported rejection rule.

The full census and its summary are retained in `.local/sdk-548/foundations/`. To reproduce every
body, run `--symbols 'CScopeObjectReference::Get'`, then run `--function` on each returned name.
The retained `accessor-census.py` performs that loop; it reads only inspector output.

Reproduce:

```sh
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 'CEventTargetEffect::Read(CReader&, EScopeType)'
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 'CCreateStarbaseEffect::ReadMember(CReader&, int, EScopeType)'
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --callers 'CEventTarget::GetScopeType() const'
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 'CSetOwnerEffect::ExecuteActual(CEventScope&) const'
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 'CEventTarget::GetScopeCountry(CEventScope&) const'
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 'CScopeObjectReference::GetCountry() const'
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 'CScopeObjectReference::GetShip() const'
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --symbols 'CScopeObjectReference::Get'
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 'CScopeObjectReference::GetGrowthStage() const'
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 'CScopeObjectReference::GetGalacticCommunity() const'
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 'CGalacticCommunity const* CScopeObjectReference::GetObject<CGalacticCommunity>() const'
```

### F5. Member constructors and receiver stops

- `add_resource`: `CEffectEntry<CAddResourceEffect>::Create()` stores the vtable, then calls
  `CFixedResourceTable::CFixedResourceTable()` with `x0 = object + 0xa8`. That class has no
  vtable group. Its empty constructor summary forgets only the member suffix, preserving the
  primary vtable and joining the concrete receiver.
- `exists`: the create method calls `CEventTarget::CreateFromToken(int)` with `x8 = object +
  0x68`. That function is `mov x1, x0; mov x0, x8; b CEventTarget::CEventTarget(int)`: a wrapper
  that tail-calls a constructor with the result address as receiver. The walk enters this
  register-move wrapper and applies the member constructor summary, preserving the primary vtable.

Reproduce:

```sh
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --effect-grammar add_resource --trace
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 'CEffectEntry<CAddResourceEffect>::Create() const'
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --trigger-grammar exists --trace
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 'CEventTarget::CreateFromToken(int)'
```

The repair applies to 127 effects and 76 triggers. No previously joined command loses its
receiver. A constructor without a vtable summary is accepted only at a nonzero offset inside
the allocation. At offset zero, outside it, or with an unknown receiver, the call keeps the
unknown-call fallback. This matters for `add_zone` and `remove_zone`: their stack temporary is
constructed before the command's final vtable store. A wrapper that calls another function
first is not entered. Unknown stores retain the evaluator's existing invalidation rule.

### F6. Present state of the acceptance samples

| Command | Outer `Read` | Member reader | Established keys | Stops |
| --- | --- | --- | --- | --- |
| `create_starbase` | `CEffect::Read` | its own | `size` String, `effect` Block; `owner`, `design`, `module`, `building` Unknown | `reader-routing` |
| `add_district` | both forms | its own | `district_type` String, `ignore_cap` and `type_conversion` Boolean | none |
| `set_timed_country_flag` | `CComplexIntEffect::Read` | its own | `flag`, `days`, `months`, `years` Unknown | `reader-routing` |
| `set_country_flag`, `remove_country_flag`, `add_tradition` | simple assign | family dispatch (inherited) | none | none |
| `always`, `has_country_flag` | simple assign | `CTrigger::ReadMember` | none | none |
| `has_tradition` | database object | `CTrigger::ReadMember` | none | none |
| `add_resource`, `join_war_on_side`, `exists` | not joined | — | — | `command-vtable` |

`create_starbase` key shapes behind `reader-routing`: the target idiom (`owner`); an array
element made by `CPdxArray<CString, int>::SetSizeAndEmplace` and read by
`CReader::Read(CString&, bool)` (`module`, `building`); `CPdxOptional<CString>::SetEmplace`
from the token text (`design`).

Reproduce:

```sh
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --effect-grammar create_starbase
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --effect-grammar add_district
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --effect-grammar set_timed_country_flag
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --effect-grammar set_country_flag
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --effect-grammar remove_country_flag
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --effect-grammar add_tradition
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --trigger-grammar always
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --trigger-grammar has_country_flag
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --trigger-grammar has_tradition
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --effect-grammar add_resource
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --effect-grammar join_war_on_side
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --trigger-grammar exists
```

### F7. Stages, their results, and construction

- Slots from the address point. Effects: `PostInit` `+0x90`, `PostValidate` `+0x98`,
  `ExecuteActual` `+0x50`. Triggers: `PostValidate` `+0x68`, `PostInit` `+0x70`,
  `ActualEvaluate` `+0x20`. `CEffect::PostInit` and `CTrigger::PostInit` are `ret`.
  `CEffect::PostValidate` and `CTrigger::PostValidate` are `mov w0, #1; ret`.
- Each command constructor adds the command to its database. The `CEffect::CEffect()` body
  at `0x1004571b0` inserts directly through `CPdxArray<CEffect*, int>::InsertAtEmplace`;
  its other entry at `0x10045741c` tail-calls that body. It does not call
  `CEffectDatabase::AddEffect`. `CTrigger::CTrigger()` calls `CTriggerDatabase::AddTrigger`.
  `CEffectDatabase::PostInit()` calls slot `+0x90` of every command in the database;
  `CEffectDatabase::PostValidate()` calls slot `+0x98` of every command;
  `CTriggerDatabase::PostValidate()` calls slot `+0x68` of every command. So the engine runs
  both stages for every command that it constructs.
- **Both validation drivers ignore the result.** After the `blr` to the slot, neither driver
  reads `w0`; each goes to the next command and leaves the command in the database. The
  inspector finds direct calls only, so a caller of the slot through a register, other than the
  two drivers, is not excluded.
- **Consequence.** No inspected engine path rejects a value because a stage returned false.
  The engine's observable rejection is the diagnostic. A false result with no diagnostic does
  not establish acceptance and does not establish rejection.
- `PostValidate` returns a Boolean. In the three bodies that were inspected
  (`CBoolTrigger`, `CIfEffect`, `CMultipleTargetEffect`), each path that returns false also
  logs through `CPdxLogFileAndLine`. This is not established for the 1,796 `PostValidate`
  symbols of the build.
- Order in `CGameApplication::InitGame()`: `NNullObjAndDatabaseInitUtil::SetupDatabases` (content
  is read; `CGlobalDeferredDatabaseObjectResolver::Run()` is called inside it), then
  `CTriggerDatabase::PostInit()`, `CTriggerDatabase::PostValidate()`,
  `CEffectDatabase::PostInit()`, `CEffectDatabase::PostValidate()`, then
  `CPostInitVariableValueDatabase::ProcessVariableValues()`.
  **Stage order for one command: read, deferred references resolved, `PostInit`, `PostValidate`.**
- The base constructors write the log-suppression byte of F2 as zero: `CEffect::CEffect()` has
  `strb wzr, [x0, #0x78]`; `CTrigger::CTrigger()` has `strb wzr, [x0, #0x60]`. Each then registers
  the object in its database, as described above. The factory walk uses constructor summaries and
  forgets these bytes, so the walk of today does not establish them.

Reproduce:

```sh
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 0x1004571b0
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 0x10045741c
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 0x100d06974
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 'CEffectDatabase::PostInit()'
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 'CEffectDatabase::PostValidate() const'
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 'CTriggerDatabase::PostInit()'
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 'CTriggerDatabase::PostValidate() const'
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 'CEffect::PostInit()'
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 'CTrigger::PostInit()'
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 'CEffect::PostValidate() const'
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 'CTrigger::PostValidate() const'
cargo run --release --example inspect -- --image "$STELLARIS_PATH" --function 'CGameApplication::InitGame()'
```

## Parser observation

Field parser entry and return are separate from storage decoding. The live
`fixture_block_parsing` case observed two `potential` occurrences, with source lines,
the same definition owner, and paired returns. It passed in 28 seconds. Block storage
remains unavailable; a parser return is not a claim about stored values or runtime meaning.

The trigger collection's wrong-scope branch logs through `CPdxLogFileAndLine`, bypassing
the existing malformed and unexpected reader reports. A live country trigger containing
`is_planet_class` produced a source-located wrong-scope error on line 3. The first trial
hooked the shared `CFileLogger::Log` method. It also intercepted setup logging: the retained
valid-case log held 11,147 setup lines and 2,854 error lines before the session ended near
its deadline. All three validation cases lacked a complete terminal. None established
acceptance or complete diagnostic coverage.

The replacement recipe observes the formatted `CPdxLogFileAndLine` dispatch and its
formatting-failure branch, plus the formatted string sent to `CLogStream` by
`CScriptedTrigger::PostValidate`. The latter is a separate route for deferred unknown
triggers. The candidate terminal is entry to `CModifier::LogDefinitions`, after trigger
and effect post-init. Exact hook locations and argument registers live in the binding
recipe. The narrowed deferred unknown-trigger case passed in 87 seconds with a complete
diagnostic window and a line-3 diagnostic. The full regression run then passed all three
narrowed cases: valid (89 seconds), wrong scope (88 seconds), and deferred unknown trigger
(89 seconds).

The trigger control matrix passed all twelve accepted/rejected cases for `and`, `or`, `not`,
`if`, `else_if`, and `else` (83–91 seconds each). Accepted cases witnessed the outer field parse
and complete diagnostic coverage. Rejected cases produced source-located wrong-scope diagnostics.
These are parser observations, not runtime truth-value assertions.

Inspection also found a separate effect compilation-error path through `CScriptedEffect::OnError`.
Its message and the receiver's source CString have an exact-build binding. The worker joins them
without decoding block storage; this hook is required for complete validation coverage. Authored
worker checks cover its source join and unrelated-file filtering. The live valid, wrong-scope, and deferred unknown-effect cases passed in 88, 86, and 91 seconds.
All twelve effect control cases subsequently passed (87–95 seconds each): `if`, `else_if`,
`else`, `hidden_effect`, `random_list`, and `every_owned_planet`, each with a valid sample and
a source-located deferred unknown-child rejection. Together with the trigger matrix, all twelve
target controls now have accepted/rejected parser samples.

The expanded window uses the session's existing pause owner. Missing hooks, missing
terminals, lost records, invalid owners, unpaired reader invocations, and invalid diagnostic
source joins prevent complete observations. Source lines join to a block occurrence only
when one witnessed interval matches. Ambiguous intervals retain only the source location.

## Static extraction

`Native::command_grammar(kind, name)` has independent properties for child families, fixed
keys, numeric keys, and ordering. `GrammarProperty::Partial` keeps established values without
claiming that the property is exhaustive. Recorded answers preserve those distinctions and
`Error::UnknownCommand`. Declaration answers remain declaration-only.

Registration analysis retains each factory. A bounded factory walk requires every returned
allocation to agree on its constructor-installed primary vtable, then resolves both `Read` and
`ReadMember`. Constructor summaries use compiler vtable-group metadata, as the existing nested
field method does. Calls must have a receiver within the allocated object. A constructor replaces
facts from that receiver onward with its own base-subobject vtable points; embedded member
constructors therefore preserve earlier primary vtables. Unknown calls invalidate the allocation.
Missing, out-of-range, overwritten, or conflicting vtable evidence remains unresolved. The walk
uses the existing 64-path/20,000-instruction evaluator bounds.

The installed twelve-control probe now establishes all twelve concrete readers. Trigger `or`
and `not` share both reader methods; the outer reader alone is shared more widely. Trigger `and`
reaches the trigger collection and the fixed `id` key. All three trigger conditional names share
the same concrete reader, reaching `id`, `limit`, and trigger children. All three effect conditional
names likewise share a concrete reader. They reach `limit` and effect children. The member path
for `else` selects an embedded effect reader when the stored child collection is empty, or when
its last child is neither `if` nor `else_if`. Otherwise it delegates to the effect collection.
The method follows the signed count load, previous-child pointer, token comparisons, and
conditional comparison. The public ordering rules describe these reader selections, not a
restriction on accepted syntax. Conditional family alternatives remain in the field ledger.
`hidden_effect` reaches the effect collection; `every_owned_planet` reaches `limit` and effect
children.

`random_list` now joins integer-key decoding to an allocated entry's concrete virtual reader.
The numeric token comes from the dispatch path, and its origin must be the original reader's
bound token storage. The numeric value stays unknown; weight arithmetic is not interpreted.
All candidate paths must reach the same concrete child reader. The public numeric-key property
carries that child's nested grammar. Its member reader admits effect children and delegates two
fixed token cases to the shared mean-time reader; detailed modifier paths remain unresolved.

The earlier factory trial established only `and`, `or`, and `not`. Walking constructor bodies
lost primary vtables at registration calls and saved registers at unresolved database writes.
The constructor-summary method replaces that failed approach without changing the evaluator's
unknown-write handling.

A second-opinion review proposed fresh-allocation escape tracking in the evaluator. Inspection
found that its assumed registration paths were incomplete: `CTriggerDatabase::AddTrigger` hashes
the pointer and can enter a Robin Hood hash-table insertion; effect insertion includes array
reallocation, copies, an indirect virtual call on the array, and element-moving loops. Existing
database paths also retain unknown counts. The proposed escape rule must additionally account
for an unknown address derived directly from a fresh pointer before publication; such a write can
affect the allocation even when it has not escaped. No relaxation of unknown writes has been
implemented. The inspected instructions are retained in `.local/sdk-542/receiver-registration-paths.txt`.

The member walk reuses field token dispatch, follows inherited delegates to depth eight, and
keeps missing routing and cycles unresolved. Authored cases cover inherited fixed keys, child
families, receiver offsets, missing reader arguments, recursive delegates, numeric receiver joins,
conditional comparisons, and ordering with missing or ambiguous evidence. Persistent-member
families use the destination join described below.

## Persistent field families

Generic `CReader::Read(CPersistent&)` calls retain their owner-relative destination. A bounded
constructor walk joins that destination to a vtable address point and the bound `Read` and
`ReadMember` slots. Constructor aliases must preserve the owner receiver. Inline vtable stores
also count when the walk proves the destination and installed pointer. All returning constructor
paths must agree; unknown calls erase affected facts. A shared `CPersistent::Read` callee alone
never establishes a concrete grammar or a distinct reader identity.

The M45 join establishes the `modifier` family in traditions (destination `0x210`, custom
modifier member reader) and council agendas (`0x40`, graphical modifier member reader).
Their AI-weight destinations also have concrete reader identities but retain an unknown family.
Authored cases cover constructor aliases, inline installation, missing summaries, wrong
destinations, conflicting constructors, later invalidation, and different member-reader identities.

## Edge-case observations

The expanded live matrix passed empty, missing, repeated, and late `limit` for trigger and effect
`if`; effect `else` first, after an ordinary effect, and after `if`; and numeric weighted entries
including zero. A nonnumeric weighted key was rejected by the unexpected-reader hook. These
observations do not establish runtime ordering semantics or weight evaluation.

The malformed `if = { limit = yes set_country_flag = native_fixture_flag }` sample completed
parsing and the validation window, then produced three source-located deferred diagnostics.
It did not invoke the immediate malformed-reader hook. The test expectation now names the
observed deferred engine-log source; its isolated rerun passed in 88 seconds.

## Population measurement

After review repairs, the method ran on 2026-09-26 over both full declaration inventories in 265 seconds,
reusing one executable-derived input per inventory. No command or field name selects production
behavior. The six target commands in each inventory are reported separately from all other
commands. Unresolved entries remain in every denominator; neither inventory had unnamed entries.

| Inventory slice | Commands | Concrete reader | Child family facts | Numeric child grammar | Ordering facts |
| --- | ---: | ---: | ---: | ---: | ---: |
| Trigger target controls | 6 | 6 | 6 | 0 | 0 |
| Other triggers | 1,090 | 856 | 118 | 0 | 0 |
| Effect target controls | 6 | 6 | 5 | 1 | 3 |
| Other effects | 1,068 | 931 | 743 | 1 | 0 |

All 2,170 grammar answers are partial; none failed as an operation and none claims a complete
property. Established fixed keys give a partial list; no established keys leaves that property unresolved. Zero in this
table means unresolved, not an established absence. `random_list` has no established outer child
family: its numeric entry grammar dispatches effect children. The same numeric method transfers
to `locked_random_list`. Other commands remain outside the control-sample acceptance matrix;
shared dispatch facts do not establish their full argument grammar or live acceptance.

Reader identity and value kind are measured separately. Of all 1,096 triggers, 137 have an
established block kind and 959 retain an unknown kind; for 1,074 effects the counts are 463 and
611. All twelve target controls have an established block kind and their trigger/effect family.
Fixed-key facts occur in 184 trigger and 498 effect answers, including four controls in each
inventory. Remaining fixed-key properties are unresolved, not proven empty.

The 234 unresolved trigger receivers split into 206 `factory-return` and 28 `command-vtable`
stops. The 137 unresolved effect receivers split into 134 `command-vtable`, two `factory-terminal`,
and one unsupported `instruction` stop. After a receiver join, unresolved child paths remain in 67 trigger
answers and 179 effect answers. Other recurring diagnostics are `reader-routing` (64 trigger,
170 effect answers) and unsupported `instruction` (12 trigger, 48 effect answers), plus conditional
child families, branch values/conditions, and flags. These counts are distinct command answers per
reason; one answer may have several reasons. Numeric-child gaps belong to their outer command.

`inspect --trigger-grammar NAME --trace` (or `--effect-grammar`) names where a receiver lost the
value that its check needed. Tracing leaves every answer unchanged. On M45-release:

- `pop_change_ethic` stops at `command-vtable`. `CEffectEntry<CAddEthicEffect<false>>::Create()+0x18`
  calls `CAddEthicEffect<false>::CAddEthicEffect()`, which is not a known constructor, so the
  method invalidates the allocation.
- `exists` loses its vtable in the same way, at a call to `CEventTarget::CreateFromToken(int)`.
- `branch_office_value` stops at `command-vtable` inside its out-of-line factory
  `NTrigger::Create<CBranchOfficeValueTrigger>`, at a call to `CEventTarget::CEventTarget()`
  after the last vtable store. The factory walk runs a create method's tail-called factory in
  place of the tail call (SDK-543); before that, 206 triggers such as `has_country_flag` stopped
  at `factory-return`.

The same revision ran `registry-field-sweep` over all 164 discovered registries in 148 seconds:
10 complete field inventories, 154 partial, zero failed. This measures field discovery, not
complete grammar. It retained 1,564 root fields: 915 with reader identity, 649 without; 974 with a
known broad kind, 590 unknown. There are 40 distinct established root reader identities.

| Field level | Trigger | Effect | Modifier | Not applicable | Unknown | Total |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Root | 132 | 105 | 28 | 565 | 734 | 1,564 |
| Nested | 3 | 4 | 0 | 23 | 11 | 41 |

Of the 734 unknown root families, 144 have a block reader and 590 have an unknown broad kind.
The most common internal stops are unjoined direct reader calls (512 `bl`, 84 `b`), 151 unresolved
token paths, and 60 indirect calls. These are path counts, not command or field counts. The shared
conditional-dispatch changes also expose `upgrade_desc` in megastructures; its reader and
conditions remain unresolved. No existing field was removed from the four parity samples.

All block fields in traditions and tradition categories are accounted for. Traditions has
`potential`/`possible` trigger families, `on_enabled`/`on_disabled` effect families, a modifier
family, and explicit unknown families for `ai_weight` and `tradition_swap`. The latter preserves
its SDK-541 nested fields and use conditions. Categories has a trigger `potential` and unknown
`ai_weight`. Council agendas has trigger `potential`/`allow`, effect `effect`/`init_effect`, a
modifier family, and unknown `ai_weight`. Unknown weight grammar belongs to SDK-545.

Reproduce the measurements on the verified installation:

```sh
export STELLARIS_PATH='/path/to/Stellaris'
NATIVE_GRAMMAR_REPORT=/tmp/command-grammar.json cargo test --release --lib m45_command_grammar_population -- --ignored --nocapture
cargo run --release --example registry-field-sweep -- "$STELLARIS_PATH" > /tmp/field-families.json
cargo parity
cargo live fixture_control
```

The development reports are retained under `.local/sdk-542/`; they are measurements, not replay
inputs. Authored tests cover missing/ambiguous static evidence and observation-integrity faults.
The live matrix has one accepted and one source-located rejected sample per target command, all
14 limit/order/weight/malformed edge cases, trigger and effect diagnostic probes, and repeated
block-entry/return observations. No parser sample claims stored block values, weight arithmetic,
scope propagation, or runtime meaning.

A validation window lasts until all content has loaded, about 74 seconds on M45-release, whatever
the fixture holds. So the well-formed samples of one block field share one session
(`fixture_control_triggers`, `fixture_control_effects`). Each sample is its own five-line
definition, and it is checked alone. An accepted sample must have no diagnostic on its lines. A
rejected sample needs a diagnostic of its stage on its child's line. A diagnostic that no sample
owns fails the case. The nonnumeric weight and the malformed `limit` each keep their own session:
a reader report or a malformed block can upset the parsing of the definitions after it. A fixture
holds at most 32 questions, which bounds a batch.

## Consumer boundary

SDK-597 and SDK-625 own Atlas snapshot integration and fixture conclusions. Native exposes
parsing, storage, diagnostics, and runtime as separate dimensions. Callers must not treat an
unavailable storage decoder as a parse rejection or treat the absence of diagnostics from an
incomplete window as acceptance. SDK-600 must retain unresolved entries and missing live
observations in its completion gate. Full argument grammar, scope propagation, weight
evaluation, and detailed modifier grammar remain outside SDK-542.

SDK-597 consumes `Reader.family` on both the field summary and its paired read alternatives.
`Unknown` and `NotApplicable` are different facts; a known conditional alternative must not make
an unresolved sibling unconditional. Preserve nested SDK-541 paths and `All(Unresolved, ...)`
conditions. Reader identities are opaque within a build and may refine when a concrete receiver
is established; they are not command names or permanent schema identifiers.

SDK-625 consumes each `GrammarProperty` independently. An established key or numeric child gives
no credit to unresolved siblings. `ChildOrderRule` describes parser routing using the stored child
collection, not a runtime ordering requirement. Parser acceptance requires a witnessed, complete
`FixtureParsing` result and complete relevant diagnostic window without a source-located error;
rejection requires a source-located diagnostic. A recorded answer supports reproduction but gives
no new live credit. The bounded post-read window is explicitly `FixtureFileLoadAndValidation`.

SDK-600 can assert the council agenda families above, but agenda cost, entry scopes, reference
targets, and weight grammar remain with SDK-544, SDK-549, SDK-543, and SDK-545. The council answer
is still partial. Neither this method's static facts nor its parser samples satisfy the entire
Milestone 4 gate or the later Atlas composition and live coverage run.

## Verification

Formatting, Clippy across all targets with warnings denied, rustdoc with warnings denied,
`cargo test --workspace --locked` (415 unit tests plus integration, example and documentation
tests), 36 Python worker tests, and all 21 installed M45 parity tests passed on the final revision. The required LARP style review found two duplicated responsibilities: log-source
interpretation mixed with emission, and concrete read/member identity construction in three
normalizers. Both were separated without changing the observed facts or serialized identities.
The final regression suite, Python tests and parity passed after the PR and architecture repairs.
All six trigger/effect validation probes passed again, followed by the repeated block parser case.
An earlier overlapping unit/live run invalidated two live sessions; the final run serialized
these checks and passed. The architecture review and follow-up style finding were verified before
repair; see the [finding dispositions](command-grammar-review.md).

## PR review repairs

Review found seven edge cases. Command recording now encodes non-plain names in a separate
namespace, preserving exact lookup names without trailing-slash collisions. An unnamed registration
keeps even an otherwise matching factory unresolved. Numeric child grammars merge identical gaps
only once, and population failure counts count each command once per reason.

Known constant equality comparisons now take only the feasible branch, including the followed
conditional-comparison form. Registry field dispatch keeps member delegates unresolved; only the
command walk that follows those delegates treats them as delegation boundaries. Generic persistent
fields have no reader ID until their concrete destination joins, preserving the broad block kind.

Source-correlated engine-log diagnostics may arrive on another nonzero game thread in the bounded
validation window. They retain sequence, file, stage and terminal checks. Owner reads, parser
entries/returns and completion markers still require the activation thread. Authored controls cover
both allowed log stages and wrong source, stage, occurrence, thread and terminal evidence.

## Reusable population reporter (SDK-631)

`examples/command-population.rs` replaces the test-only reporting procedure for subsequent
measurements. See [method authoring](method-authoring.md#command-inspection-and-population-reports)
for commands, denominator rules and diff behavior. The original SDK-542 test remains a historical
measurement check; this tool adds no extraction claims or parser validation.

Two unfiltered runs on M45-release (`07988b4f1b865623becd7a61af1cae92e111be6515d341754af70f02107822cd`)
produced identical normalized reports. The second run took 287 seconds. Counts reconcile with
SDK-542's delivered population above:

| Family | Named operation answers | Complete | Partial | Failed operations | Failed receiver joins |
| --- | ---: | ---: | ---: | ---: | ---: |
| Effects | 1,074 | 0 | 1,074 | 0 | 137 |
| Triggers | 1,096 | 0 | 1,096 | 0 | 234 |

Both inventories had zero unknown registration observations and zero input-wide gaps. Failed
receiver joins remain partial operation answers and stay in the named totals. All 12 target
controls retain their available joins; these measurements do not extend their parser checks.

Distinct command counts per internal failure reason (nested numeric failures count against the
outer command; groups overlap):

| Failure shape | Effects | Triggers |
| --- | ---: | ---: |
| `factory-return` | 0 | 206 |
| `command-vtable` | 134 | 28 |
| `factory-terminal` | 2 | 0 |
| `reader-routing` | 170 | 64 |
| `instruction` | 48 | 12 |
| `branch-value` | 2 | 1 |
| `branch-condition` | 2 | 0 |
| `flags` | 1 | 0 |
| `conditional-child-family` | 3 | 0 |
| Non-singleton, missing/ambiguous token name, or unestablished member path | 81 | 5 |
| Hidden default in the `CMeanTimeToHappen::ReadMember` jump table | 2 | 0 |

The SDK-543 receiver repair changed only the trigger column. Of the 206 `factory-return`
triggers, 156 now join their receiver (120 with fixed keys) and 50 stop at `command-vtable`:
78 failed trigger receiver joins remain, all at `command-vtable`. `reader-routing` rises to 177
trigger answers because more receivers are walked. The population run takes about one minute;
see [dynamic names](references.md#dynamic-names).

The last two rows retain child-field gaps that the old population report only summarized in its
public unresolved-path gap. The reporter also groups each shape by instruction kind, obstacle
and function, preserving stops where present and saying when no instruction was located.
Named inspection was checked for a successful `if` receiver chain and the failed
`has_country_flag` factory return, including its retained cause trace.
