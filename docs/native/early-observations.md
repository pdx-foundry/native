# Injection and observations before registration/parsing

The registration-entry and category-read public route was retired on 2026-10-02; its code and live
controls are in Git at `d8f9d8a`. The category control now requests field outcomes with parsing
for `tree_template` and `traditions`. The original SDK-483 experiment is commit `4188faf` on branch
`prototype/sdk-483-early-observations`, retained in the `typed-extraction` bundle under
`early-observation-prototype/`.

## Loader-entry mechanism (M45-observe)

The parent creates an ARM64 suspended direct child. LLDB attaches at `_dyld_start`; before resume,
every requested hook resolves once, is enabled and has zero hits, and the worker refuses resume
if entry, architecture or hooks fail verification. Suspension is a prerequisite; the observed
entry and the correlated trace establish ordering. The four controls (normal, missing hook,
dropped record, worker loss) gave the expected outcomes, and every game was reaped.

- Carry **activation**, **observation completion** and **confirmed disposal** as separate facts.
  Empty output with an unresolved hook is unavailable; record loss prevents completion; worker
  loss does not erase observations already collected.
- Worker and owner monotonic timestamps use different clocks. Order by trace sequence.
- These are read-entry observations, not successful registration returns, stored values,
  complete registries, validation results or gameplay.
- Failed approaches: a launch-inserted dylib recorded registration but never reached parsing; a
  presentation guard crashed in `objc_retain` because an ARC function used an object signature
  for a `BOOL`.
- The prototype's `replay.py --scenario all` **builds a guard and launches games**; [retrieval](retrieval.md)
  describes the offline checks. Atlas's accepted consumer clarification is the "Early observation
  review" section of `/Users/jackson/Developer/pdx-foundry/atlas/docs/planning/extraction-architecture.md`.

## Registry items (Rust supervisor)

`internals::check_registry_load` keeps one bounded live observation of one registry, with an
optional fixture, for the SDK-552 loader-rule controls; `engine/operations/registry_items.rs`
states its witness chain. The public item query is in Git at `d8f9d8a`.

- **Where to read the items.** The worker reads item keys from the engine objects when the
  initial collection loader returns: the collection is full, and later validation has not run.
  A first attempt waited for the later post-read phase, which the game did not reach in 180
  seconds.
- **M45-observe result.** 234 traditions and 33 tradition categories from one paused process. Over
  all 164 registries, 161 gave complete answers (including 49 ascension perks, 17 ethics, 171
  edicts, 358 civics from a nested directory, and 10 galaxy definitions from `map/galaxy`).
  `common/bypass` and `common/map_modes` lacked an established key layout (see
  [item keys](modifier-families.md#item-keys)); the initial loader of `common/game_scenarios` did
  not run before the startup deadline.
- **Pause at the deadline.** Without a returned loader, a registry session pauses only at the
  worker's deadline (170 seconds of the 180-second budget), and the registry is `NotLoaded`. One
  M45-release session paused there before any of the six generator registries loaded; why is not
  known.
- **Launch flag.** The launch uses `-debug_mode`. One early batch omitted it and still reached the
  fixture, so it is not known to be necessary.
- **Missing debugger.** To test a missing debugger without a host change, set `DEVELOPER_DIR` to a
  directory that does not exist.

## Fixture sessions and parser field outcomes

The fixture's loader-return callback ends its observation window without stopping the session;
the registry callbacks then establish the pause. Missing or late fixture hooks give an
observation error; dropped records, missing terminals and an access failure give partial
answers; worker loss before the pause gives a startup error with confirmed disposal.

The tradition method joins the exact `CTraditionType` constructor, root member reader,
String-reader return, malformed-report routine, and the return from `LoadFromReader` while its
reader is still alive. The constructor establishes an omitted definition without parsing fixture
text. Each joined field return reads owner storage, and the file terminal reads it again.
Diagnostics are intercepted at `CReader::ReportMalformed(CString const&)` and
`CReader::ReportUnexpected(CString const&)`, the overloads that create their own reader errors, so
the forwarding overloads do not duplicate them. A multiline quoted string reports `Malformed token`
and stores `Unreadable String`. The coverage terminal means only that diagnostic collection
completed for this file load.

Pitfalls:

- **A numeric-key map breaks the worker codec.** A binding map keyed by number generated
  `patternProperties`, which the codec refuses. Use a typed list.
- **One owner destination per field.** The string-reader transfer derives the token from `x8` and
  the owner-relative destination from `x1` on the root-dispatch trace, with no field constant. It
  transferred to `common/ascension_perks` and `common/relics#portrait`. `common/federation_perks#icon`
  joined its owner but has no single nonconditional owner destination, so it stays unavailable.
- **Expanded text joins its call.** A reader of an inline script or of a generated scripted-effect
  instance names the fixture call in its source. The worker joins its reports and member reads to
  the call's line ([script expansion](script-expansion.md)).
- **Manual relationships remain per build.** The binding still supplies the parser diagnostic
  entry points, the `CString` representation, the reader and lexer source layout and the
  launch-thread window. A new build must reverify them.

## Numeric fixture storage

The binding selects a decoder by the joined callee, never by field name: `CReader::Read(int&)`
stores a signed 32-bit integer, `CReader::Read(CFixedPoint&)` a signed 64-bit integer at scale
100,000, the template reader `fpml::fixed_point<long long, 48, 15>` a signed 64-bit integer at scale
32,768, `CReader::Read(float&)` binary32 bits and `CReader::Read(short&)` 16 bits with no sign
interpretation. Conditional paths, several reader alternatives, an unproven token or destination
and other signatures stay unavailable. The observed values are in
`tests/expected/numeric-m452/live.json`; [numeric conversion](numeric-conversion.md#live-observations)
interprets them.

The template reader has no root field in the registry population. Its live case uses the inline
loader of `common/special_projects` (not a discovered registry): `FixtureFieldQuestion::with_parent_field`
selects the embedded `SProjectRequirements`, whose `fleet_power` reader reads a comparison operator
and then tail-calls the decoder. Only initial file-load outcomes are supported there.

M451-hotfix fixture-binding population, 184 numeric root fields:

| Reader kind | Decoder and verified loader | Decoder, no verified loader | No proven decoder/destination |
| --- | ---: | ---: | ---: |
| Integer (including seven short fields) | 79 | 12 | 2 |
| Fixed-point | 79 | 8 | 1 |
| Float | 3 | 0 | 0 |

The three unavailable destinations are `common/council_agendas` `agenda_cooldown` and
`agenda_finish_modifier_duration`, and `common/ship_sizes` `hull_scale`. A decoder without a verified
loader is still unavailable for a live fixture.

### Pitfalls

- **A member return and a file terminal can differ.** `build_time = -1.234567` stored raw `-123456`
  at member return and raw `100000` at the file terminal, with no parser error. Check both values;
  do not infer a clamp rule.
- **Recursive reads share a return address.** Recursive `CPersistent` reads return to one
  address, so an address-only hook fired before the parent returned. Return hooks check the entry
  stack pointer. Parent and leaf reads must share file reader, thread and source file, and the
  leaf receiver must equal the root plus its proven embedded offset.
- **No leaf occurrence means no observed write.** A nested read through another source reader,
  such as an inline script, cannot be reported as a complete omitted field.
- **A loader's boolean specialization is part of its identity.** Assuming `false` misses
  `TSingleObjectGameDatabase<CStarClassDatabase, CStarClass, true>`. The loader symbol gives its
  `LoadFromReader`; the following `mov x0, sp` and `CReader::~CReader()` call give its return
  boundary. A matching `ReadNewEntry` helper can hold the owner constructor; the binding follows at
  most that one helper. For `CStarClass` on M451-hotfix: loader `0x1006539e8`, file reader
  `0x100653d74`, file-reader return `0x100653a58`, `ReadNewEntry` `0x100654110`, key constructor
  `0x100c0a9f0`, member reader `0x100c0aa50`, float reader `0x1025b8304`; `icon_scale` is at owner
  `+0x138`.
- **`x8` is not a token proof.** It can hold a jump-table index, an owner address or a reused
  scratch value; at token `0x25c`, `w8` still held the earlier comparison constant `0x2cb4`. Require
  the path's singleton token domain and an unconditional reader/owner join.
- **A field can transform its value after the reader.** `common/ship_sizes/max_speed` does; its
  final value is not a shared-reader rule.
- **Do not run the default Rust suite alongside a live fixture** (see the ordinary game conflict in
  [lifecycle](lifecycle.md#supervisor-and-worker-pitfalls-macos)). A run that did reported
  `External game invalidated isolation` before the pause.
- **Decimal review values must not replace bit comparisons.** Binary32 `0x7f7fffff` as binary64
  `0x47efffffe0000000` serializes as `3.4028234663852886e+38` but parses back as
  `0x47efffffe0000001` in `serde_json`. Compare the integer bit patterns; keep review decimals as
  test-only text. Do not add a floating-point field to the public value.
