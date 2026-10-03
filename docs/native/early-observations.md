# Injection and observations before registration/parsing

The registration-entry and category-read public route was retired by the 2026-10-02
[simplification review](../design/simplification.md). The implementation and its live
controls remain in Git at `d8f9d8ab337d10c9e920caeb02fc651f53b78042`. The findings below remain
engine knowledge. The current category control requests field outcomes with parsing for
`tree_template` and `traditions`, checking source lines, shared owner, entries and returns.
The generic field-outcome method replaces the fixed category tokens and locality exception.


SDK-483 was accepted on 2026-09-17 for M45-observe. The experiment is `4188faf564b8609fde747da09bd4b8db8045b315`, branch `prototype/sdk-483-early-observations`. The retained source and evidence are in `typed-extraction/typed-extraction/early-observation-prototype/`.

## Qualified sequence

The final parent creates an ARM64 suspended direct child. LLDB attaches and observes `_dyld_start`. Before resume, all three requested registration/loader/field breakpoints resolve once, are enabled and have zero hits. The worker refuses resume if entry, architecture or hooks fail verification. Suspension is a prerequisite; the observed entry and correlated trace establish the ordering result.

The normal trace records three registration call entries during startup initializers, then the fixture's category `LoadFile` entry before reader construction. It joins `tree_template` at line 2 and `traditions` at line 3 to one owner and file, observes the matching loader return on the same thread, and emits an explicit terminal sequence/count record. Owner evidence then separately confirms exit and reaping.

The fixture is a category with a template string and empty traditions list. No save/world is loaded. These are **read-entry observations**, not stored values, successful registration returns, validation or gameplay. The three observed registration tokens do not establish a complete command registry.

The four controls (normal, missing hook, dropped record, worker loss) gave the same outcomes as
the later [worker trial](loader-entry-worker.md#final-matrix), and every game was reaped.

Worker and owner monotonic timestamps have different clock domains. Use trace sequence for producer order; do not compare those timestamps as one clock.

## Native joins and retained failures

`pdx_native.py` owns launch, target pin, profile/content hashes, timeout, worker and final ownership. `debugger_attempt.py` owns LLDB breakpoints, ARM64 argument/source joins, loader return and trace sequencing. `evidence/` retains reader, lexer, filename accessor, category loader/reader and token disassembly. `runs/*/source/` preserves the source actually executed, so the final result need not depend on current prototype code.

The accepted final mechanism is the debugger observer. A fallback launch-inserted dylib recorded registration but failed to reach parsing. Its private in-process hooks/trampoline are not qualified by the final result, and missing constructor witnesses cannot prove database construction order.

Initial `task_for_pid` failed until debugger access was approved; see
[debugger authorization](lifecycle.md#findings-from-the-rust-supervisor-m45-observe-macos). A presentation guard crashed in `objc_retain` because an ARC function used an object signature for a BOOL; a no-argument selector also needed the correct signature. Both were corrected. Failed raw runs and `evidence/development-failures.json` remain. The old debugger-owned disposal failure remains explicit; final ownership is the independent direct parent.

## Reuse and limits

SDK-515 repeated the four controls with an LLDB subprocess and embedded Python, with a pinned
handshake. See the [loader-entry worker trial](loader-entry-worker.md). The Rust supervisor uses the
same worker.

Carry **activation**, **observation completion**, and **confirmed disposal** as separate facts. Empty output with an unresolved hook is unavailable; record loss prevents completion; worker loss does not erase already collected observations. Native owns target-specific addresses, argument conventions, source/owner joins and cleanup. Atlas supplies observation requests, fixtures and a deadline.

Fresh capture requires the exact M45-observe installation/content, ARM64 host, Xcode/LLDB and debugger access. The original `replay.py --scenario all` **builds a guard and launches games**; its name does not mean offline replay. This consolidation did not run it. The offline migration check reads manifests, source hashes, traces and final ownership records, described in [retrieval](retrieval.md).

No production adapter, Windows timing, database-constructor order, late/hot reload, arbitrary parser stage, owner-loss recovery or low maintenance cost is established. Atlas's accepted consumer clarification is the "Early observation review" section of `/Users/jackson/Developer/pdx-foundry/atlas/docs/planning/extraction-architecture.md`. Local Linear acceptance is `linear-records/linear/SDK-483-comments.json`; the review summary's earlier pending label remains historical.

## Registry items (Rust supervisor, M45-observe)

The public item query was retired on 2026-10-02 and is in Git at `d8f9d8a`.
`internals::check_registry_load` keeps one bounded live observation of one registry, with an
optional fixture, for the SDK-552 loader-rule controls; `engine/operations/registry_items.rs`
states its witness chain.

- **Where to read the items.** The worker reads item keys from the engine objects when the initial collection loader returns. The collection is full at that point, and later validation has not run. A first attempt waited for entry to the later post-read phase; the game did not reach it in 180 seconds.
- **Inputs.** The engine reads private copies of the registry directories. On M45-observe the result is 234 traditions and 33 tradition categories, from one paused process, in about 35 to 45 seconds.
- **Every registry.** Static discovery supplies the initial loader entry of each registry. The
  same return-boundary witness gave 49 ascension perks, 17 ethics, 171 edicts, 358 civics from a
  nested directory, and 10 galaxy definitions from `map/galaxy`. A full M45-observe report over
  164 registries gave 161 complete answers. `common/bypass` and `common/map_modes` were
  unsupported because their key layout was not yet established (see
  [item keys](modifier-families.md#item-keys)), and `common/game_scenarios` because its initial
  loader did not run before the startup deadline.
- **Pause at the deadline.** Without a returned loader, a registry session pauses only at the
  worker's deadline, 170 seconds of the 180-second startup budget, and the registry is
  `NotLoaded`. One M45-release session paused at the deadline before any of the six generator
  registries loaded; why it did not reach them is not known.
- **Launch flag.** The launch uses `-debug_mode`, as the prototype did. One early batch omitted the flag and still reached the fixture, so the flag is not known to be necessary.
- **Missing debugger.** To test a missing debugger without a change to the host, set `DEVELOPER_DIR` to a directory that does not exist.

## Prepared fixture sessions (SDK-532, M45-observe)

The public session now combines the retained registration/category hooks with the registry hooks.
The fixture's loader-return callback ends its observation window without stopping the session;
the existing registry callbacks then establish the final pause. The exact M45-observe installation
passed all 27 cases in `tests/live.rs`, including the existing registry controls, on 2026-09-20.

A private category directory containing the two-field fixture gives three registration entries,
`tree_template` at line 2 and `traditions` at line 3, joined to one owner. The same session returns
one category and the 234 pinned traditions. Either observation kind can also be requested alone.
Missing/late fixture hooks give an observation error. Dropped records, missing terminals and the
injected access failure give partial answers. Worker loss before the pause gives a startup error
with confirmed disposal. The tests check ordinary-profile contents and an unrelated process after
every case, and compare saved answers with game-free reads.

The first integration attempt failed before attachment: a numeric-key binding map generated
`patternProperties`, which the worker codec deliberately refuses. A typed list of field bindings
uses the supported schema subset; the Python request-codec test covers this boundary.

## Parser field outcomes (SDK-533, M45-observe)

The tradition file method joins the exact `CTraditionType` constructor, root member reader,
String-reader return, malformed-report routine, and the return from `LoadFromReader` while its
reader is still alive. The statically established String fields are `custom_tooltip`,
`custom_tooltip_with_modifiers`, and `unlocks_agenda`; they share reader identity
`325efaa17499c32d`. The constructor establishes an omitted definition without parsing fixture text.
Each joined field return reads actual owner storage, and the file terminal reads it again.

Diagnostics are intercepted at `CReader::ReportMalformed(CString const&)` and
`CReader::ReportUnexpected(CString const&)`, the overloads that create their own reader error
entries, so the no-argument forwarding overloads do not duplicate them.
The multiline quoted-string control reports the engine text `Malformed token` on line 3, joins it
to the requested field occurrence, and stores the independently observed value `Unreadable String`.
The coverage terminal means only that parser diagnostic collection completed for this file load.
No M45 mechanism in this method reaches post-read validation, a world, or gameplay runtime;
requested runtime is an explicit unavailable outcome and `OutsideMethod` gap.

### Milestone 2 string-reader transfer (2026-09-21)

Timebox: one working day on 2026-09-21 for implementation and verification together; active
hours were not logged.

The frozen `registry-fields/v2` sweep found one string reader shared by 170 fields. The
bounded experiment selected `CReader::Read(CString&, bool)` and the `CTraditionType`
owner. The static root-dispatch trace derives the field token from `x8` and the
owner-relative destination from `x1`; it reproduces the three previously handwritten
tradition offsets. No named-field token or storage offset remains in the live binding.
The loader candidate establishes the registry's exact `LoadFile` entry and owner class.
The loader's unique direct call to its specialized `LoadFromReader` establishes the
return instruction where the reader and owner are still live. The known tradition
fixture passed after this change. Only then did the same method run on
`common/ascension_perks`: two known field names in an unfamiliar registry that shares the
`CTraditionType` owner class returned the expected strings through `Game::observe_fixture`,
joined to one owner, with complete observation and confirmed disposal
(`fixture_transfer_string_reader`). No ascension-specific native field constant was added.
The next transfer used `common/relics#portrait`, a name absent from the handwritten table
and an owner class (`CRelic`) different from `CTraditionType`. The exact-build analysis
selected that owner's constructor, member reader, loader boundary, token, and owner-relative
storage. The live `fixture_transfer_relic_portrait` case returned the fixture string with a
joined owner, complete observation, and confirmed disposal; no relic-specific field constant
was added.

An attempted `common/federation_perks#icon` case joined the definition owner but reported
`No exact-build storage binding for this field`. Its root trace does not meet the single,
nonconditional owner-destination rule, so it was not counted as a transfer success.

For transferred registries, exact-build analysis selects the owner constructor and member entry. The binding
still supplies parser diagnostic entry points, `CString` representation, reader/lexer
source layout, and the launch-thread observation window. Those are manual engine
relationships on M45-observe. The retained exception claims only initial-load string storage for fields
whose trace gives one nonconditional owner destination and token, when the loader,
constructor, and member-reader boundaries are unique. Its obstacle is deriving these
object and source layouts on other builds. A future binding must replace or reverify
them; the method reports unsupported fields and registries as unavailable.

### Manual category read-entry exception (historical)

This section describes the state before 2026-10-02. `InitialCategoryLoad` and `CategoryFieldReads` were public, category-specific names.
The M45 binding retains the `tree_template` and `traditions` token values for
`common/tradition_categories`. This exception claims only that those two reader entries
occur in the initial category-load window; it says nothing about storage or validation.
In the separate initial file-load outcome window, a malformed category value produces a
source-located `Malformed token` diagnostic even while its storage result is unavailable.
The root-field analysis does not yet derive the read-entry hook and token selection as
one general operation. Replace these names and token constants when that operation can
select a field from the exact-build binding and pass an unfamiliar-category transfer.

This was the only entry in the locality gate's exception list (`tests/locality.rs`); the list is empty since 2026-10-02. It covers
the registry check in `src/fixture.rs`. Since SDK-569, the fixture reducer takes the category
field names and their count from the binding's `FixtureBinding.fields`, not from its own
constants. The reducer's three registration entries are part of the same window and are
removed with this exception.

## Direct numeric storage (SDK-643, M45-release)

The attempt began on 2026-09-28 within the accepted one-working-day bound, including verification.
The executable and slice match the M45-release identities in [targets](targets.md). The existing
owner, source, member-return and file-terminal joins now carry a typed storage value. A direct
`CReader::Read(int&)` writes a signed 32-bit integer. `CReader::Read(CFixedPoint&)` writes a signed
64-bit integer at scale 100,000. The reader and `CToken::ReadValue` bodies establish these
representations on this build; the binding selects them by the joined callee, never by field name.
Conditional paths, multiple reader alternatives, an unproven token or destination, and other
numeric reader signatures remain unavailable. The public answer makes no conversion or range
rule claim.

`FixtureStorage::Observed` replaces `FixtureStorage::String`; each `StoredFieldOccurrence`
replaces `StoredStringOccurrence` and holds a `FixtureValue`. Its variants are `String`, `Integer`
and `FixedPoint { raw, scale }`. The final value has the same type. This changes the serialized
fixture answer, so older recorded string answers need recapture. The method stamp is
`observe-fixture/v3`. Storage and parser diagnostics still complete independently.

### Exact-build observations

The fixture cases are `fixture_numeric_megastructures` and `fixture_numeric_armies` in
`tests/live.rs`. Each supplies boundary, fractional and malformed inputs. The malformed definition
first stores `7`, then reads `not_a_number`; both occurrences remain source-correlated.

The observed values are in `tests/expected/numeric-m45/live.json`; [numeric
conversion](numeric-conversion.md#live-observations) interprets them.

Both numeric live cases passed, along with all 12 existing field-outcome cases and both string
transfer cases (16 live cases total). The full default Rust suite and all 42 worker/codec tests
also passed. Both fixed-point fields use scale 100,000. The army case transfers the method to a second owner
class and registry without a new decoder. These observations concern these inputs and this load
window, not general rounding, overflow, validity or gameplay semantics.

**A member return and a file terminal can differ.** The first megastructure assertion assumed
that `build_time = -1.234567` would remain unchanged. The actual member-return value was raw
`-123456`, while the file-terminal value was raw `100000` (one whole unit), with no parser
error. The observation was complete; the test expectation was wrong. The test now checks both
values separately. The original fixture, complete trace and run summary are retained in
`.local/sdk-643/fractional-terminal/`. The observer does not infer a general clamp rule from this.

### Population and remaining template gap

The ignored `binding::analysis::tests::numeric_fixture_storage_population` test scanned every one
of the 164 discovered registries. It found 181 broadly numeric fields in 62 registries. Static
storage coverage is separate from live observation completeness:

| Reader kind | Decoder and verified loader boundary | Decoder, no verified loader boundary | No proven direct decoder/destination |
| --- | ---: | ---: | ---: |
| Integer | 37 | 8 | 48 |
| Fixed-point (`CFixedPoint`) | 28 | 9 | 51 |

All 164 analysis queries completed; none failed. The 65 fields in the first column are candidates
for live observation, not 65 completed live answers. The unavailable group does not pass the
single unconditional reader, matching token and owner-destination rule, or uses a different
numeric signature. Missing loader/constructor/member joins prevent a live session even when
storage is statically known. The full report is `.local/sdk-643/numeric-population.json`.

The template form `CReader::Read(fpml::fixed_point<long long, (unsigned char)48,
(unsigned char)15>&)` has an authored decoder control but **no completed live fixture**. Its token
reader stores 64 bits and shifts the whole part by 15, with fractional conversion at scale 32,768.
The direct-call trace finds 11 calls in `SCameraParams`, `CCountry`, `CFleetIntel`,
`SProjectRequirements`, `CFleet` and `CShipGrowthStage::CSerializer`. None is a numeric root field
in the 164-registry scan. `SProjectRequirements` is a nested serializer; `common/special_projects`
is not a discovered registry. The original root-only fixture route cannot mount and join that
candidate. The nested fixture route below supplies the required storage window.

The trace and token-reader disassembly are retained in `.local/sdk-643/template-callers.txt` and
`.local/sdk-643/template-token-reader.txt`, including both build hashes. Static decoding alone
does not satisfy the template live criterion. SDK-643 supplies the direct `int&` and `CFixedPoint&`
observations. SDK-648 supplies the nested template-reader fixture loader, owner/source joins and
boundary, fractional and malformed cases described below. [Numeric conversion](numeric-conversion.md)
records the current conversion observations and remaining conversion limits. A world-object reader
is outside this initial-load method; [duration keys](durations.md) owns duration expiry and
[scoped numeric](scoped-numeric.md) owns scoped operand storage and world evaluation.

## Nested template numeric storage (SDK-648, M45-release)

SDK-648 closes the template-reader gap above on the same verified M45-release executable.
`FixtureFieldQuestion::with_parent_field` selects one embedded parent. The method stamp is
`observe-fixture/v4`. The first supported inline loader is `common/special_projects`; it remains
outside the template registry inventory. Its exact-build recipe binds the file reader's
construction and destruction and the root object's virtual read call. Field names, tokens,
embedded offsets and storage destinations come from static analysis of the executable.

The root `CSpecialProjectType` constructor establishes an embedded `SProjectRequirements`.
Its `fleet_power` reader reaches the template fixed-point decoder after reading the comparison
operator. The storage proof permits that comparison call with the same reader, then requires
one unconditional tail call with a proven owner-relative destination. It does not interpret
comparison semantics. Disassembly is retained under `.local/sdk-648/`.

The live `fixture_numeric_nested_projects` case places each key after the requirements block.
The worker buffers events until the root read returns and establishes the key, preserving source
lines and occurrence order. Parent and leaf reads must use the same file reader, thread and
source file, and the leaf receiver must equal the root plus its proven embedded offset. Their
return hooks also check the entry stack pointer: the first live attempt showed that recursive
`CPersistent` reads share a return address, so an address-only hook fired before the parent
returned. File-final values are read again at reader destruction, before directory postprocessing.

The values, at scale 32,768, are in `tests/expected/numeric-m45/live.json`. The live run completed
with all owner, source, parser-return and file-terminal joins intact. Diagnostic coverage completed
with no diagnostics for these inputs, including `not_a_number`. This differs from the direct
`CFixedPoint` observation; neither result establishes a general validity or conversion rule.
Recorded answers round-trip these typed values.

Only initial file-load field outcomes are supported for the inline loader. Validation, world
state and runtime evaluation are outside this method. Unproven parents, receivers, destinations
or readers remain unavailable. A nested read through another source reader, such as an inline
script, cannot be reported as a complete omitted field. No leaf occurrence means no observed
write; when storage is proven, the separate file-final read can still report its initialized value.

The SDK-648 inventory rerun completed all 164 discovered registry queries with no failed query.
It found the same 181 numeric root fields in 62 registries as SDK-643: 65 with both a direct
storage proof and loader boundary, 17 with storage but no loader boundary, and 99 without a
proven direct destination or decoder. The failure shapes and per-reader counts in the earlier
table are unchanged. The new inline nested fixture is outside that inventory and passed its
separate binding and live checks. The report is `.local/sdk-648/numeric-population.txt`; the
complete template live answer is `.local/sdk-648/template-live.txt`. The full default Rust suite,
Clippy, documentation checks, 52 Python worker/codec tests, and all three numeric live cases pass.
The nested worker-loss live control also passes with confirmed process disposal. Both nested
cases explicitly select the traditions registry while the inline fixture loads independently.

## Float and short storage (SDK-656, M451-hotfix)

The exact executable is M451-hotfix in [targets](targets.md).
The fixture binding joins `CReader::Read(float&)` to binary32 storage and
`CReader::Read(short&)` to 16-bit storage. The worker reads four or two bytes respectively,
without interpreting the short's signedness. Unknown callees, ambiguous token paths, missing
reader receivers and non-owner destinations remain unavailable. Authored Rust controls and
worker memory-read/codec controls cover the two new paths.

### Loader and destination pitfalls

A template loader's boolean specialization is part of its callee identity. Assuming `false`
misses `TSingleObjectGameDatabase<CStarClassDatabase, CStarClass, true>`. The selected loader
symbol establishes its corresponding `LoadFromReader` name; the following `mov x0, sp` and
`CReader::~CReader()` call establish its return boundary. A matching `ReadNewEntry` helper can
hold the owner constructor. The binding follows at most that one direct helper and rejects
missing or ambiguous constructor routes. It does not follow unrelated calls.

For `CStarClass`, the loader is at `0x1006539e8`, its file reader at `0x100653d74`, and the
file-reader return at `0x100653a58`. Its matching `ReadNewEntry` is at `0x100654110` and calls
the key-bearing constructor at `0x100c0a9f0`. The member reader is at `0x100c0aa50`; the float
reader is at `0x1025b8304`. `icon_scale` uses the owner-relative destination `0x138`.

The token proof cannot require a scratch-register value at the callee. `x8` can hold a
jump-table index, an owner address or a reused scratch value. The immediate comparison for
token `0x25c` leaves `w8` holding the earlier comparison constant `0x2cb4`, although the selected
path proves the token and the float destination. Require the path's singleton token
domain and unconditional reader/owner join instead. These compiler shapes also transfer to
storm, astral-action and sector fields without a registry or field branch.

`common/ship_sizes/max_speed` transforms its stored value after the reader. Its final value is
not a shared-reader rule.

### Live isolation obstacle

Do not run the default Rust suite alongside a live fixture: see the ordinary game conflict in
[lifecycle](lifecycle.md#findings-from-the-rust-supervisor-m45-observe-macos). A run that broke
this rule reported `External game invalidated isolation` before the pause, with confirmed
disposal. Static analysis can run alongside a live session.

The reviewed numeric matrix has 110 cases and 124 stored occurrences, including nine float
cases across three fields and 21 short cases across seven fields. All ten fields have verified
loader boundaries. Static representation and width agree with all stored values. The malformed
short sequence stores 7 followed by zero, with `Malformed token`; float retains the binary32
pattern of 7 with the same diagnostic. Final values agree with the last occurrence in these new
cases. These are finite observations, not conversion or acceptance rules.

The retained expected-output mismatch contains a historical build stamp and absent new rows,
with no storage conflict. Its eight sessions complete with all observations joined and its test
failure is `numeric conversion observations differ`. The current reviewed expectation includes
the 30 new rows and M451-hotfix stamp. The 80 existing rows have no value or diagnostic changes.
The candidate, case output and the retained trace directories are indexed by
`.local/sdk-656/numeric-conversion-live.json` and `.local/sdk-656/live-candidate.log`;
the final sector session's trace is under
`/var/folders/kd/4s9l1qz1055d4cq25nz2xddh0000gn/T/pdx-native-52280-1790796504489133000`.

### Current fixture storage population

The M451-hotfix fixture-binding population covers 164 registries and 184 numeric root fields;
all queries return and none fails. Storage and loader proofs are separate:

| Reader kind | Decoder and verified loader | Decoder, no verified loader | No proven decoder/destination |
| --- | ---: | ---: | ---: |
| Integer (including seven short fields) | 79 | 12 | 2 |
| Fixed-point | 79 | 8 | 1 |
| Float | 3 | 0 | 0 |

All seven short fields are in the first column. The three unavailable destinations are
`common/council_agendas` `agenda_cooldown` and `agenda_finish_modifier_duration`, and
`common/ship_sizes` `hull_scale`: they do not supply the required single unconditional scalar
reader/owner-destination proof. A proven decoder without a verified loader is still unavailable
for live fixture observation. Population proof does not claim live observations for those
unselected fields. The full current report is `.local/sdk-656/fixture-storage-population-final.log`.

### Decimal review must not change bit comparisons

A numeric JSON review value for binary32 `0x7f7fffff` does not round-trip through this repository's
`serde_json` parser: binary64 `0x47efffffe0000000` serializes as `3.4028234663852886e+38`, but
parses as `0x47efffffe0000001`. The stored binary32 bits and all other observations agree.
Keep review decimals as test-only text derived from the bits; compare the public integer bit
patterns exactly. Do not add a Cargo feature or a floating-point field to the public value.
The probe and its output are `.local/sdk-656/float-json-roundtrip.rs` and
`.local/sdk-656/float-json-roundtrip.txt`.

The retained `fixture_numeric_conversion_matrix` expectation failure has 110 fully joined cases
and 124 stored values identical to the reviewed values. Only the numeric review field comparison
fails after JSON parsing. Its output is `.local/sdk-656/live-final.log`; its boundary case trace
is under `/var/folders/kd/4s9l1qz1055d4cq25nz2xddh0000gn/T/pdx-native-54654-1790796863497189000`.
All eight sessions have confirmed disposal. The current test and expectation use decimal text
for these review fields, independently of static/live storage checks.
