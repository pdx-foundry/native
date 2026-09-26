# Nested command grammar

These findings apply only to M45-release and its ARM64 slice, identified in
[targets](targets.md). The method is `command-grammar/v1`; field families use
`registry-fields/v6`, and parser observations use `observe-fixture/v2`.

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
- `has_country_flag` stops at `factory-return`. Its create method tail-calls
  `NTrigger::Create<CHasCountryFlag>`, which the factory walk does not decode, so the returned
  receiver is unknown.

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
block-entry/return observations. Each malformed case uses its own game session. No parser sample
claims stored block values, weight arithmetic, scope propagation, or runtime meaning.

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
