# Nested command grammar

`Native::command_grammar(kind, name)` gives independent `GrammarProperty` values for a registered
command's forms, targets, child families, fixed keys (with [reference lookups](references.md)),
numeric keys, ordering and duration groups. `Partial` keeps established values without claiming the
property is exhaustive; a missing registered command gives `UnknownCommand`. The method runs on
M45-release and M451-hotfix ([targets](targets.md)); addresses and counts below are from
M45-release unless a section names another build. `grammar.rs` has only a one-line
module comment, so this page also describes the method.

## Forms and the stage chain

`forms` describes two facts. A `Block` entry establishes that the outer reader reaches the block or
member reader. A `Value` entry establishes an alternative only when its whole read, assignment,
initialization and validation chain accepts it; it establishes no runtime meaning. Handing a scalar
to a base block reader is not an established rejection.

- Each value path keeps the command memory from `Read` through `Assign`, deferred references,
  `PostInit` and `PostValidate`. Deferred references hold a found-item stand-in for acceptance; the
  missing-key run supplies the lookup's bound null object and is reported separately.
- **The diagnostic test is essential:** the dispatch and database drivers ignore stage results
  (F1, F7). A diagnostic on any stage makes a path rejecting; without one, every result-bearing stage
  must return true in bit zero. False, unknown, unfinished and bounded paths are unresolved. An
  alternative is listed only if all its paths accept; mixed paths, unknown reader kinds and more than
  64 paths keep `forms` partial with a `value-acceptance` gap.
- A read watch records loads from command bytes that the chain has not written; script writes do
  not become construction state. The factory walk keeps only bytes agreed by every returning path
  and never enters constructor bodies for more state. An unknown receiver branch whose outcomes
  differ under the same script-input decisions gives `receiver-state`, withholding only the differing
  facts. The Boolean probe pairs the two token runs under the same operator decision.
- Cached results require the same selected slots, every entered or classified callee and the same
  watched initial bytes; each command computes its own full key, and the population audit compares
  those keys.
- The initializer summary is narrow: a proved inline initializer (such as
  `CAddDistrictEffect::PostInit()`, which stores the selected item at command `+0xd0`) is summarized
  only when the reference shape covers its entire body and its condition is `Always`; anything else
  gives `value-acceptance: PostInit: initializer not summarized`.
- Known forms without `Block` give empty known child families, keys and ordering and a known absent
  numeric child. Unknown or partial forms cannot promote child properties.

## Member ledgers and recursive coverage

Each member node keeps its token intervals, local stops and child links. Every interval is rejected,
a field, a family dispatch, a numeric child, a delegate, a dynamic key or a gap; conditional paths
can overlap, but the union must tile the full token range and every overlapping disposition must be
established. A covered node has no gap, dynamic key, table gap or stop; every named key has a known
reader kind after grouping its reader paths (disagreement gives `unknown-key-reader`). Coverage groups
by token identity, not spelling (`ambiguous-key-token`). Missing member vtables, cycles, cut bodies
(slot bodies keep up to 65,536 bytes) and the eight-level nesting bound leave coverage unresolved.

- The factory contributes only allocation bytes that agree on every returning path; eight agreed
  bytes at a member destination supply its vtable.
- A member slot whose function is one `b` to another function is an alias, read through its target
  (`declarations::alias_target`, at most four aliases). `TFleetSettings::ReadMember` `0x101034738` is
  an alias of `ReadBackwardsCompatible` `0x10103473c`, which reads the fleet `settings` keys.
- `create_species.flags` and `variables` are read with `CReader::ReadUniform<CString>`, a list of
  strings with no named fields, so they publish `Fields: []` with a `reader-routing` gap.
- One internal coverage function governs fixed keys, child families and ordering; the normalizer does
  not infer completeness from public fields. A known numeric child needs every property of its
  grammar known.
- A gap below two or more named keys uses `GapSubject::KeyPath` (outer to inner); numeric-child gaps
  propagate to the parent answer.
- Compound shapes (a temporary event target moved into the owner, an emplaced `CString` array
  element, an optional `CString` set from token text) need a clean caller return with no later store
  into the destination and no call given an owner-derived address; a stack-derived call argument is
  unresolved while any tracked frame slot holds an owner-derived value. An overwrite gives
  `compound-reader-overwrite`. M45 protects 0x190 target bytes, 0x30 optional-string bytes and the
  0x18-byte array header. Ordinary reader joins end at the first reader call and prove nothing about
  later caller instructions.

### Current value-form results

The tracked baseline and its counts are in
[`tests/population/m45-release/README.md`](../../tests/population/m45-release/README.md). A failed
answer has every property unresolved: the failed receiver joins are `command-vtable` (seven effects
and the triggers `switch` and `inverted_switch`), `factory-terminal` (two effects) and `instruction`
(`set_location`). Gap shapes outside `OutsideMethod`:

| Shape | Meaning |
| --- | --- |
| `target-arguments` | The target list cannot be known, mostly because another property is not |
| `value-acceptance` | A value chain is unresolved; the detail names the stage and cause |
| `reader-routing` | A member dispatch that the walker cannot route |
| `unknown-key-reader` | A named key whose reader kind is unknown |
| `target-scope-check` | A target check that is not established |
| `nested-member-vtable` | A nested member without an established vtable |
| `form-reader-call`, `form-path-limit`, `form-reader-kind` | Outer `Read` shapes that stop the forms run |
| `form-command-call`, `form-token-coverage` | An unentered helper that receives the command; a token that the yes/no probes do not cover |
| `loop-limit`, `path-limit` | Evaluator bounds |
| `instruction` | An instruction that the evaluator does not model |
| `receiver-state` | Results that depend on unestablished construction state |

Raw-string assignment, initializer execution and receiver-state proof are shared-method limits,
not exceptions keyed by command name (SDK-641). The unclassified `Read` and `Assign` census is in
`.local/sdk-548/forms-stage-chain/revision-1/`.

## Pitfalls found in review

Each of these let an unproved path support a `Known` or `Complete` answer; each is now a stop or a
gap with an authored test.

- **An unentered helper can act on the command.** A call in `Read` or `Assign` that receives the
  command and that no rule classifies is the stop `form-command-call`; later stages stop with
  `form-stage-call`.
- **Three probes do not stand for every token.** The yes/no and marker probes are complete only
  when every outcome of the unknown-token run also occurs in one of them. Compare outcomes as sets,
  not counts: the evaluator keeps no negative constraints, so the unknown-token run repeats outcomes
  on infeasible forks. Otherwise the stop is `form-token-coverage`.
- **An accepted value can carry an unestablished lookup.** A listed `Reference` alternative whose
  directory, lookup shape or key match is not established gives a `value reference` gap.
- **Nested initializers count** at the member's key path.
- **A tail call does not return.** A compound helper reached through `b` ends the path with
  `compound-reader-return`.
- **A one-instruction member is not a reader.** Walking a `ReadMember` that is only `b` gives
  `reader-routing` and `Fields: []`; follow the alias. A thunk that also changes an argument
  (`mov x1, x3; b …`) is not an alias.
- **One key, one construction.** A second, different construction of a fixed key's child is
  `ambiguous-constructed-child`.
- **Execution sees the parsed command.** The execution probe installs only the vtable; factory bytes
  can be overwritten by readers the method does not list, so a branch on any receiver byte keeps
  both sides.
- **Stores through unknown addresses end initial reads**; a later load of an unprotected watched
  byte adds no cache-key byte.
- **Scalar widths come from the reader** (1, 2, 4 or 8 bytes); an unlisted scalar keeps 8.

## Live fixtures of complete grammars

`cargo live fixture_argument` checks complete grammars with the sample fixed in
`tests/population/m45-release/command-fixture-sample.json`, validated in `common/traditions`
(`potential` for triggers, `on_enabled` for effects).

- Only a rejection that the method established on every path is a rejected sample. A value given
  to a block reader, or an alternative with an unresolved chain, is never a sample.
- A rejected key gives a reader report that can upset the definitions after it, so each needs its
  own session; Boolean rejections are logs and can share one.
- **Few triggers have an established rejection.** Most complete triggers are blocks whose unlisted
  keys go to the family dispatch, which is outside the method, so an unknown key there is not an
  established rejection (trigger `if` is complete but not usable).
- Only the hooked log routes count: the formatted `CPdxLogFileAndLine` log, `CReader::ReportUnexpected`
  and the `CLogger::Log` stream at `CScriptedTrigger::PostValidate`.
- Open follow-ups with no tickets: prove the trigger `Assign` and `PostValidate` rejection routes;
  resolve the Boolean trigger chain (`always` returns false with no diagnostic); read the accepted
  operators; prove construction state for `receiver-state` commands.

## Engine facts on M45-release

### F1. The family dispatch always calls `Read`

`CEffect::ReadMember(CReader&, int, EScopeType)` finds the factory, calls its create method, stores
the token at command `+0x20` and the file location at `+0x28`, checks the command's own scope (slot
`+0xa0`, "Wrong scope for effect"), and calls the virtual `Read` (slot `+0x10`). It never calls
`Assign`, and **it does not read the result of `Read`**; a `Read` that tail-calls `Assign` returns
`Assign`'s result, which is ignored too.

### F2. Outer `Read` shapes

`--lookup-census '::Read(CReader&, EScopeType)'` gives 244 bodies in 64 groups:

| Outer `Read` | What the body does |
| --- | --- |
| `CEffect::Read` / `CTrigger::Read` | Block. If the reader's value kind (`[reader+0x278]`) is not 3 and the command's byte at `+0x78` (effects) or `+0x60` (triggers) is 0, it logs `Expected "<name> = {", but got …` through `CLogger::Log` and `CLogStream`, then continues. The loop calls `CReader::ReadSimpleStatement()`, loads the key token from `[reader+0x38]`, and calls the virtual `ReadMember` (effects `+0x18`, triggers `+0x38`). Token `0x438` (inline script) makes a new reader, sets the byte to 1, calls `Read` again and restores the byte. |
| `CSimpleAssignEffect::Read` / `CSimpleAssignTrigger::Read` | Value. Tail call to the virtual `Assign` (effects `+0x20`, triggers `+0x28`) with `x1 = reader + 0x278`; the trigger form first calls `CAssignOperator::Read(CReader&)` into command `+0x64`. No value-kind test. |
| `CCompareTrigger::Read` | Value with `CCompareOperator::Read(CReader&)`, then the virtual `Assign`. |
| `CDatabaseObjectEffect<D>::Read` / `…Trigger<D>::Read` | Reference: tail call to `NParserUtil::ReadKeyReferenceDeferred<D>`. |
| `CEventTargetEffect::Read` | Target value (F4), stored at command `+0xa8`. |
| `CComplexIntEffect::Read`, `CComplexIntTrigger::Read`, `CComplexValue…::Read` | Tail call to the base block reader. |
| Both forms: `CAddDistrictEffect::Read` and 15 more of one shape | `[reader+0x278] == 3`: tail call to `CEffect::Read`; otherwise the virtual `Assign`. |

### F3. `Assign`

- Signature `Assign(CToken const&, EScopeType)`. `x1` is the reader's value token at `reader +
  0x278`; its id is at token `+0` and its text at `reader + 0x288`. It returns a Boolean in `w0`;
  `CEffect::Assign` and `CTrigger::Assign` return 0. The census gives 999 bodies in 103 groups.
- `CSimpleAssignTrigger::Assign` makes an event target from each token (F4) at command `+0x68`, walks
  the chain and returns true when the token is a scope keyword.
- `CBoolTrigger::Assign` calls that base first. If it returns false, it compares the token id with
  `0x3fef` and `0x2cac`, stores the value at `+0x1f8` and sets `+0x1f9`; another token returns false
  with no log. Operator token `0x427` (stored by `Read` at `+0x64`) inverts the value.
  `CBoolTrigger::PostValidate() const` logs "A boolean trigger at %s has been assigned an invalid
  value. Expected: yes/no." when `+0x1f9` is not 1. So the parser takes a target for a Boolean
  trigger, and validation rejects it with a diagnostic.
- `CIntEffect::Assign` calls `CVariableValue::Assign(CToken const&, EScopeType, CString const&)` on
  command `+0xa8`.

### F4. Event targets

- A reader stores a target with one idiom: `CToken::CToken(CToken const&)` from `reader + 0x278` to a
  stack temporary, `CEventTarget::CEventTarget(CToken, EScopeType, CString const&)` to a second
  temporary, then `CEventTarget::operator=(CEventTarget&&)` with `x0 = owner + D`. The constructor
  calls `CEventTarget::ValidateScope`, which checks the chain of links, not the command's
  expectation. The same idiom reads a key (`create_starbase` `owner` at `+0x130`).
- **Explicit checks of the target's scope type are rare.** `CEventTarget::GetScopeType()` has 32
  direct callers; in command code: `CActivateGateway::PostValidate`,
  `CAutoFollowFleetEffect::PostValidate`, `CHasHyperlaneToTrigger::Assign`, and about ten
  `ExecuteActual` or `ActualEvaluate` bodies.
- **Most commands use a typed getter at execution** (`CSetOwnerEffect::ExecuteActual`:
  `add x0, x0, #0xa8`, then `CEventTarget::AccessTargetCountryWithErrorLogging`). The getters
  (`GetScope<Type>`, `GetTarget<Type>WithErrorLogging`, `AccessTarget<Type>WithErrorLogging`) call
  `CEventTarget::GetScope(CEventScope&, char const*)`, which returns a scope through `x8`, then
  `CEventScope::GetTarget<Type>`. These include conversion routes: the country getter can read a
  megastructure or leader and then its owner. **A getter's result type does not bound its accepted
  input scopes.**
- Of 45 `CScopeObjectReference::Get*` bodies, 41 are typed accessors: 38 compare the scope type
  (`+0x8`) with one constant (`GetCountry` 4, `GetShip` 8) and return `TPdxNullObject<T>::_pInstance`
  on mismatch; `GetGrowthStage()` compares with `0x8000000000` and returns literal zero;
  `GetGalacticCommunity()` and `GetObject<CGalacticCommunity>()` return the global community with no
  type check. Pitfall: these three have no rejection null object and are bound `NoNullObject`, so
  the getter analysis keeps them unresolved; treating any other return as acceptance would accept
  every scope bit. `GetDesign` uses `TPdxNullObject<CShipDesign>` and `GetDlcRecommendation`
  `TPdxNullObject<SDlcRecommendationScriptData>`: the method suffix does not name the null type.
- `CEffect::CheckScopeSupport*` and `CTrigger::CheckScopeSupport*` test the command's own scope (the
  getter at effects `+0x80`), not a target argument.

### F5. Member constructors and receiver stops

- `add_resource`: `CEffectEntry<CAddResourceEffect>::Create()` stores the vtable, then calls
  `CFixedResourceTable::CFixedResourceTable()` with `x0 = object + 0xa8`; that class has no vtable
  group, so its empty constructor summary forgets only the member suffix.
- `exists`: the create method calls `CEventTarget::CreateFromToken(int)` with `x8 = object + 0x68`,
  which is `mov x1, x0; mov x0, x8; b CEventTarget::CEventTarget(int)`. The walk enters such
  register-move wrappers and applies the member constructor summary.
- A constructor without a vtable summary is accepted only at a nonzero offset inside the
  allocation; at offset zero, outside it, or with an unknown receiver, the call keeps the unknown-call
  fallback (`add_zone` and `remove_zone` construct a stack temporary before the final vtable store).
  A wrapper that calls another function first is not entered.

### F7. Stages, their results, and construction

- Slots from the address point. Effects: `PostInit` `+0x90`, `PostValidate` `+0x98`, `ExecuteActual`
  `+0x50`. Triggers: `PostValidate` `+0x68`, `PostInit` `+0x70`, `ActualEvaluate` `+0x20`.
  `CEffect::PostInit` and `CTrigger::PostInit` are `ret`; `CEffect::PostValidate` and
  `CTrigger::PostValidate` are `mov w0, #1; ret`.
- Each command constructor adds the command to its database: `CEffect::CEffect()` (`0x1004571b0`,
  also reached from `0x10045741c`) through `CPdxArray<CEffect*, int>::InsertAtEmplace`, not
  `AddEffect`; `CTrigger::CTrigger()` through `CTriggerDatabase::AddTrigger`. The database
  `PostInit` and `PostValidate` drivers call their slot for every command.
- **Both validation drivers ignore the result:** after the `blr`, neither reads `w0`. No inspected
  path rejects a value because a stage returned false; the observable rejection is the diagnostic,
  and a false result without one establishes neither acceptance nor rejection. In the three
  inspected `PostValidate` bodies (`CBoolTrigger`, `CIfEffect`, `CMultipleTargetEffect`), every
  false path also logs; this is not established for all 1,796. Calls through a register other than
  the two drivers are not excluded.
- `CGameApplication::InitGame()` order: `SetupDatabases` (content read;
  `CGlobalDeferredDatabaseObjectResolver::Run()` inside it), `CTriggerDatabase::PostInit()`,
  `PostValidate()`, `CEffectDatabase::PostInit()`, `PostValidate()`, then
  `CPostInitVariableValueDatabase::ProcessVariableValues()`. **For one command: read, deferred
  references, `PostInit`, `PostValidate`.**
- The base constructors zero the log-suppression byte of F2 (`strb wzr, [x0, #0x78]` and
  `[x0, #0x60]`); the factory walk's constructor summaries forget these bytes.

## Parser observation

- The trigger collection's wrong-scope branch logs through `CPdxLogFileAndLine`, bypassing the
  malformed and unexpected reader reports.
- **Do not hook the shared `CFileLogger::Log`.** It also intercepts setup logging (over 11,000 lines
  in one valid case), and the session ran into its deadline. Native observes the formatted
  `CPdxLogFileAndLine` dispatch and its formatting-failure branch, the string that
  `CScriptedTrigger::PostValidate` sends to `CLogStream` (deferred unknown triggers), and
  `CScriptedEffect::OnError` (effect compilation errors, joined to the receiver's source string);
  the terminal is entry to `CModifier::LogDefinitions`. Hook locations live in the binding recipe.
- Source-correlated engine-log diagnostics may arrive on another nonzero game thread in the bounded
  validation window; owner reads, parser entries and returns, and completion markers still require
  the activation thread.
- Source lines join to a block occurrence only when one witnessed interval matches; ambiguous
  intervals keep only the source location.
- A nonnumeric weighted key is rejected by the unexpected-reader hook. The malformed
  `if = { limit = yes set_country_flag = native_fixture_flag }` gives three source-located deferred
  diagnostics and does not invoke the immediate malformed-reader hook.
- `set_planet_class` is not the class effect on this build; see the
  [diagnostic survey](diagnostic-survey.md#pitfalls).

## Static extraction

- Registration analysis keeps each factory. A bounded factory walk needs every returned allocation
  to agree on its constructor-installed primary vtable, then resolves `Read` and `ReadMember`.
  Constructor summaries use compiler vtable-group metadata; a call needs a receiver inside the
  allocation; a constructor replaces facts from its receiver onward with its own base-subobject
  vtable points. Unknown calls invalidate the allocation. The walk uses the 64-path /
  20,000-instruction evaluator bounds. A failed approach walked constructor bodies and lost primary
  vtables at registration calls.
- Trigger `or` and `not` share both reader methods; `and` reaches the trigger collection and the
  fixed `id` key. The three trigger conditionals share one concrete reader (`id`, `limit`, trigger
  children); the three effect conditionals share one (`limit`, effect children).
- `else` selects an embedded effect reader when the stored child collection is empty or its last
  child is neither `if` nor `else_if`; otherwise it delegates to the effect collection. The ordering
  rules describe these reader selections, not a syntax restriction.
- `random_list` joins integer-key decoding to an allocated entry's concrete virtual reader; the
  numeric token's origin must be the original reader's bound token storage, and weight arithmetic
  is not interpreted. The same method transfers to `locked_random_list`.
- The member walk reuses field token dispatch, follows inherited delegates to depth eight, and keeps
  missing routing and cycles unresolved.
- `CTriggerDatabase::AddTrigger` can enter a Robin Hood hash insertion, and effect insertion
  reallocates, copies and calls through the array; fresh-owner tracking for these paths is
  [owner derivation](scoped-numeric.md#owner-derivation).

## Persistent field families

Generic `CReader::Read(CPersistent&)` calls keep their owner-relative destination. A bounded
constructor walk joins it to a vtable address point and the bound `Read` and `ReadMember` slots;
constructor aliases must preserve the owner receiver, inline vtable stores count when the walk
proves the destination and pointer, and all returning paths must agree. A shared
`CPersistent::Read` callee alone never establishes a concrete grammar or reader identity. On M45 the
join gives the `modifier` family in traditions (destination `0x210`, custom modifier member reader)
and council agendas (`0x40`, graphical modifier member reader); their AI-weight destinations have
concrete reader identities but an unknown family.

## Consumer boundary

Native exposes parsing, storage, diagnostics and runtime as separate dimensions. Callers must not
treat an unavailable storage decoder as a parse rejection, or the absence of diagnostics from an
incomplete window as acceptance.

- `Reader.family` is on both the field summary and its read alternatives. `Unknown` and
  `NotApplicable` are different facts; a known conditional alternative must not make an unresolved
  sibling unconditional. Keep nested paths and `All(Unresolved, ...)` conditions. Reader identities
  are opaque within a build and may refine when a concrete receiver is established.
- Never combine the condition of one read alternative with the reader or shape of another. A partial
  list keeps its established items, and a missing item proves no absence. `limit` is a child key,
  not a registered command. Only a `Complete` grammar can drive a validation rule.
- Each `GrammarProperty` is independent: an established key or numeric child gives no credit to
  unresolved siblings. `ChildOrderRule` describes parser routing, not a runtime ordering requirement.
- Parser acceptance needs a witnessed, complete `FixtureParsing` result and a complete relevant
  diagnostic window without a source-located error; rejection needs a source-located diagnostic. The
  bounded post-read window is `FixtureFileLoadAndValidation`. A recorded answer gives no live credit.
- The council agenda answer is still partial (SDK-600); entry scopes, reference targets and weight
  grammar belong to SDK-549, SDK-543 and SDK-545.

### Target arguments and their checks

`targets` lists each target argument (the command's own value or a named key path) with its accepted
scope types and `TargetCheckStage`; a known empty list means the command takes no target.

- The getter table runs each typed getter and scope accessor for each input type bit. The scope
  reference has its type at `+0x8` and an established local object pointer at `+0x1c`; the resolver
  writes the scope to its indirect result address. Acceptance needs a proved object; rejection needs
  every returning path to return the bound null object. Unknown returns, unclassified conversions,
  unresolved indirect calls, stops and bounds keep the getter unresolved.
- A target can be checked while reading, in `PostInit` or `PostValidate` (both `Validation`), or at
  execution. Every touched stage must be established, and the earliest stage supplies the answer
  only when each later set contains its accepted set; otherwise scopes and stage stay unresolved
  with `target-scope-check`. A false validation result without a diagnostic prevents an
  execution-stage answer. No read or a zero mask never establishes `Any`.
- `targets` is known only when forms, fixed keys and numeric keys are known, the reachable member
  tree is covered and every target check is established. A bound getter at the exact start of
  another collected, disjoint target is unrelated to the current probe. Target probes stop at
  unclassified calls, including owner pointers saved in a stack frame; scope-result loads and calls
  must be classified too.
- On M45 none of the 27 typed target getters has a complete input-bit table: 17 stop at an
  unclassified getter call (conversion or scope cleanup) and ten at an unresolved indirect call. Of
  41 scope accessors, 37 have established tables. No argument has an established public stage.
  `.local/sdk-548/targets-cross-check/` holds the per-bit tables.

Pitfalls:

- **A getter name is not the accepted input set.** `set_owner` reaches the country getter's
  conversion routes and reports its target with unresolved scopes, plus `target-scope-check:
  unclassified getter call`. The non-logging `GetScope` wrappers also call scope cleanup, and
  `CEventScope::~CEventScope()` traverses parameters through indirect calls, so an unclassified
  getter call can be cleanup or conversion.
- **Stages need compatible sets.** `CAutoFollowFleetEffect` checks its target at `+0xa8` with
  `GetScopeType` in `PostValidate`, then resolves it through `GetFleet` during execution.
- **Separate getters do not set the probed argument's input set.** `would_join_war` has three target
  keys (`side`, `attacker`, `defender`); each reaches its own getter and stays unresolved.
- **Slot overrides are not target lists.** None of the 16
  [target-getter overrides](engine-commands.md#target-getters-are-not-target-sets) agrees with an
  established target list; a disagreement establishes no different accepted set.
