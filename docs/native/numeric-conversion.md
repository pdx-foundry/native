# Shared numeric conversion

## Simplification, 2026-10-02

The public numeric clamp property was removed on 2026-10-02. All complete reader shapes established no explicit clamp; the preserved findings below distinguish that result from overflow behavior. The remaining numeric conversion properties stay in the API. The property, and the versions of its evaluator (`src/engine/analysis/numeric.rs`) and expected output (`tests/expected/numeric-m45/readers.json`) that produce it, are in Git at `d8f9d8a`.

SDK-644 adds conversion facts to `Reader.numeric` in registry fields, conditional read
alternatives, command values, fixed keys and ordering-selected readers. The shared identity is
unchanged. `Known(None)` means an established nonnumeric reader; `Unresolved` means no numeric
answer; `Partial(Some(...))` keeps independent known properties. A recording without the property
is refused with `Error::Recorded` (2026-10-02). No separate public operation is added.

## Current result on M451-hotfix

At `main` `6f643a1`, the numeric population covers all 164 registries with no failed question.
Numeric root fields: 184 in 63 registries, **0 complete, 184 partial, 0 failed**; 174 have a
known faithful-storage range. The other 10 are seven short and three float fields. Every root
field keeps `numeric-overflow`, `numeric-lexical-boundary`, `numeric-trailing-text` and
`numeric-external-library-conversion`; the 88 fixed-point fields also keep
`numeric-raw-value-mode`, and the three float fields keep `numeric-float-bound-representation`.
Narrow integer signedness is unresolved. In the command inventory, 1,221 of 1,225 numeric reader
positions have known facts. The other four are the command-level readers of
`set_ai_armor_ratio`, `set_ai_shields_ratio`, `set_ai_starbase_armor_ratio` and
`set_ai_starbase_shields_ratio`: their numeric property is unresolved, but each value form joins
the shared fixed-point reader with complete facts. Reproduce with
`cargo run --release --example numeric-population`.

## Method and limits

The exact executable is M45-release in [targets](targets.md).
`binding/binary/numeric.rs` binds reader and token bodies, their imports, and format strings.
`engine/analysis/numeric.rs` matches complete canonical instruction sequences. Register allocation
and address relocation can vary; changed calls, stores, branches or unsupported shapes produce
an unresolved result. The shared fact table is cached per installation; access still verifies the
executable. No field or registry name selects a result.

The scanner-format match binds `%i`, `%d`, `%u`, `%lli`, `%lld`, `%llu` and `%f` to their storage and
representable literal forms. This match does not establish libc overflow, locale, tokenization
or full-string consumption. Complete matched paths prove there is no explicit reader clamp;
`clamp: Known(None)` does not promise that imported conversion or an instruction never saturates.
The supported faithful-storage bounds are recorded below; width alone establishes none. Signed/unsigned/rational
bounds are exact serialized values; they never pass through a floating-point number.

| Reader shape | Established storage | Partial literal forms |
| --- | --- | --- |
| Direct signed/unsigned byte | 8-bit integer; sign interpretation unresolved | Decimal integer |
| Direct signed/unsigned short | 16-bit integer; sign interpretation unresolved | Decimal integer |
| Direct int | Signed 32-bit integer, scale 1 | Decimal and radix-prefixed integer |
| Direct unsigned int | Unsigned 32-bit integer, scale 1 | Decimal integer |
| Direct signed/unsigned long long | 64-bit integer with established sign, scale 1 | Decimal integer |
| Direct `CFixedPoint` | Signed 64-bit integer, scale 100000 | Decimal integer/fraction and radix-prefixed integer |
| Fixed-point template | Signed 64-bit integer, scale 32768 | Decimal integer/fraction |
| Direct float | Signed IEEE binary32, no integer scale | Decimal integer/fraction and exponent |

The two fixed readers also have raw-value paths. The method checks these paths and reports
`numeric-raw-value-mode`; a scale describes the stored representation, not a promise that every
input is multiplied by that scale. The template's `%lld%lf` path parses whole and fractional
components separately, so an exponent on the fraction is not a whole-value exponent rule.

The modifier boundary proof follows the declaration-table numeric path into the same fixed-point
reader and through both insertion capacity paths to a 64-bit entry store. Its shared identity is
`a9818fec780f8313`. This join is in the developer report; it does not add parser properties to
modifier declarations or claim that modifier blocks themselves store a scalar. The
[modifier grammar](modifier-blocks.md) reuses this entry identity and the same normalization pass;
its block reader keeps `numeric: Known(None)`. Modifier grammar,
category applicability, duplicates and application remain outside this method.

## Live observations

The original observation subset covers 20 input sequences on four field contexts: megastructure integer,
megastructure direct fixed point, army direct fixed point, and nested project template fixed
point. Each malformed sequence contains two occurrences, so 80 cases contain 84 stored values.
Every case requires complete owner/source/parser-return/storage/diagnostic coverage. Static
representation, signedness, width and scale are checked against typed member-return storage.
Final stored values and diagnostics are checked separately. No static/live storage conflict was
found in these cases. `tests/live/numeric.rs` retains this subset within the larger SDK-655 matrix below.

Selected observations (raw fixed-point integers):

| Input | Direct integer | Direct fixed, scale 100000 | Template, scale 32768 |
| --- | ---: | ---: | ---: |
| `+7` | 7 | 700000 | 229376 |
| `12tail` | 12 | 1200000 | 393216 |
| `0x10` | 16 | 1600000 | 0 |
| `010` | 8 | 800000 | 327680 |
| `1e2` | 1 | 100000 | 32768 |
| `-1.23456789` | -1 | -123456 | -40454 |
| `2147483648` | -2147483648 | 214748364800000 | 70368744177664 |

`92233720368547.75808` stores `i64::MIN` in the direct fixed reader. The template input
`281474976710656.0` also stores `i64::MIN`. These are finite observations, not universal wrap rules.
After `7`, `not_a_number` retains 7 (or raw 700000) and emits `Malformed token` for the direct
readers. The template stores zero without a diagnostic in this observed window; that is not
proof of valid syntax. Megastructure `build_time` changes nonpositive values to raw 100000 after
the member return. Army `war_exhaustion` retains the same negative fixed-point value. That later
field behavior must not become a shared-reader clamp.

Expected results are in `tests/expected/numeric-m45/`. The live file includes the exact build and
only inputs, stored values, final values and diagnostics. Raw first-run observations and the
population report are retained in `.local/sdk-644/`; the initial live run's four debugger traces
remain in its retained temporary directories named in that run's output. The M451-hotfix float
and short fixture coverage is described below; no live proof is claimed for the six widths
without a bound field in the exposed population.

## First-release numeric forms (SDK-544)

The first Atlas release names three numeric uses: resource changes, additive and multiplicative
naval capacity, and literal values. Each has a shared reader result and an observation, or a
named gap. The world observations came from the retired world route on M451-hotfix (in Git at
`d8f9d8a`); see [world evaluation](scoped-numeric.md#world-evaluation-on-m451-hotfix-sdk-647).

| Form | Reader result | Observation | Gap and owner |
| --- | --- | --- | --- |
| `add_resource` `mult` and `multiplier` | Scoped operand, signed 64-bit, scale 100000, with the partial operand forms and the selection rule ([scoped numeric](scoped-numeric.md)) | World: with `energy = 10`, literal `2.5` adds 25 through either key; a variable of 2.5 adds 25; `trigger:num_owned_planets` (1) adds 10; `value:tech_weight_likelihood` (1.25) adds 12.5; `modifier:country_edict_fund_add` (15) adds 150 | Literal boundaries, as for every scoped operand |
| `add_resource` resource amounts | None. `CAddResourceEffect::ReadMember` gives every other key to `CFixedResourceTable::CSerializer::ReadMember`, and the command grammar does not route that member (`reader-routing`, `unknown-key-reader`) | World: `energy = 10` adds raw 1000000, `0.5` adds raw 50000, `-3` removes raw 300000 | The static reader of resource-named keys. Owner: the command grammar gaps of SDK-625 and SDK-626 |
| Additive naval capacity, `country_naval_cap_add` | A modifier entry joins the direct fixed-point reader: signed 64-bit, scale 100000 (`tests/expected/numeric-m45/modifier-entry.json`) | The direct fixed-point cases above. World: after `add_modifier` of `fallen_empire_base` (entry value 1000), `modifier:country_naval_cap_add` gives raw 100000000 and integer 1000, from 0 before | Storage of an entry from authored text is not observed: a world loads no mod content, and the fixture route does not decode modifier entries. Grammar: [modifier blocks](modifier-blocks.md). Application and propagation: SDK-547 |
| Multiplicative naval capacity, `country_naval_cap_mult` | The same entry reader | World: after `add_modifier` of `community_champion_counselor` (entry value 0.1), `modifier:country_naval_cap_mult` gives raw 10000 and integer 0, from 0 before | The same |
| Literal values | The table of reader shapes above | The live matrix above; the world literal cases | The conversion gap shapes in the [current result](#current-result-on-m451-hotfix) |

Both naval-capacity results equal `export_modifier_to_variable` for the same modifier. The
integer 0 for 0.1 is the truncation of a fixed-point result in an integer destination.
The added modifiers were not visible in the statement after `add_modifier`; see the pitfall on
the scoped numeric page.

The named gaps in this table (SDK-625 and SDK-626 for resource amounts, the modifier-block method for remaining modifier
grammar and authored entries, SDK-547 for application) are accepted limits at the closure of
SDK-544. The modifier-entry join (shared reader `a9818fec780f8313`, the unchanged 64-bit store on
both insertion capacity paths) is checked on M451-hotfix by `m45_numeric_reader_static_parity`
against `tests/expected/numeric-m45/modifier-entry.json`. It does not resolve the shared reader's
`numeric-overflow` and `numeric-external-library-conversion` limits.

## Verification

```sh
cargo test --lib numeric
cargo test --test recorded_answers --test locality
cargo test --release --lib m45_numeric_reader_static_parity -- --ignored
cargo parity
cargo live fixture_numeric
cargo run --release --example numeric-population > .local/sdk-644/numeric-population.json
```

Exact-build commands use `STELLARIS_PATH`. The population counts and failure shapes are in the
[current result](#current-result-on-m451-hotfix).

## Float and short fixture storage on M451-hotfix (SDK-656)

The exact build is M451-hotfix.
`FixtureValue::Float { bits }` preserves the IEEE binary32 pattern; callers use `f32::from_bits`
to obtain its value. `FixtureValue::Integer16 { bits }` preserves the short reader's stored
16 bits. No signedness is inferred from these bits, and `Eq` remains available on fixture values
and their containing types. Human-readable numeric interpretations occur only in the live test
report. Static signedness, accepted ranges and remaining conversion boundaries are described in the
[boundary evidence](#boundary-evidence-on-m451-hotfix-sdk-655) below.

The three float fields are `common/star_classes` `icon_scale` and `common/storm_types`
`cosmic_storm_galaxy_lightning_time` and `cosmic_storm_galaxy_max_opacity`. The seven short fields
are `common/astral_actions` `unlock_threshold` and `usages`, and `common/sector_types` `max_systems`,
`min_systems`, `min_colonies`, `max_colonies` and `max_jumps`. Each field has a boundary, a fractional
and a malformed sequence in `tests/live/numeric.rs`. Every observation requires the joined owner,
source occurrence, parser return, storage and diagnostic window. The test checks static storage
representation and width, while leaving unresolved short signedness unresolved.

The SDK-656 subset has 110 cases and 124 member-return stored values: the original 80 cases and 84
values, nine float cases and 12 values, and 21 short cases and 28 values. All 80 existing rows
are unchanged on M451-hotfix; only their enclosing build stamp changes. The 30 added rows include
test-only decimal text or signed/unsigned readings. Decimal text avoids a binary64 JSON
round-trip discrepancy in the largest finite float; public values still contain only bits.
No static/live representation or width conflict occurs. All new final values equal their last
member-return value.

| Input sequence | Float bits and decimal reading, all three fields | Short bits, all seven fields |
| --- | --- | --- |
| `3.4028234663852886e38` (float boundary) | `0x7f7fffff`, `3.4028234663852886e38` | Not selected |
| `32767` (short boundary) | Not selected | `0x7fff` (both readings 32767) |
| `1.23456789` | `0x3f9e0652`, `1.2345678806304932` | `0x0001` (both readings 1) |
| `7`, then `not_a_number` | `0x40e00000`, then the same bits (7) | `0x0007`, then `0x0000` |

Every malformed sequence emits one `Malformed token` diagnostic from `reader-malformed-report`.
In these finite observations the float retains 7 while the short stores zero after the malformed
occurrence. The short result differs from the direct int's retained 7; it must not become a
shared integer conversion rule. These cases do not establish accepted ranges, general overflow
or signedness.

The other six shared integer readers have no live cases. Each is unavailable within the bounded
M451-hotfix population: 0 of 1,564 root fields and 0 of 38 exposed nested fields. This applies
separately to `signed char`, `unsigned char`, `unsigned short`, `unsigned int`, `long long` and
`unsigned long long`. The search cannot reach 982 unresolved member descriptions and does not
establish universal absence.

The [SDK-544 AC4 amendment](https://linear.app/unnamed-system/issue/SDK-544) of 2026-10-01 excludes
these six readers from the live fixture requirement on M451-hotfix, on this bounded search. They
keep their static checks and platform scanner samples. A later change that exposes one of them
through a supported fixture route must add its boundary, fractional and malformed cases and the
static/live conflict check. The five required kinds have these live cases:

| Kind | Field | Live case |
| --- | --- | --- |
| `int` | `common/megastructures` `sensor_range` | `fixture_numeric_megastructures`, `fixture_numeric_conversion_matrix` |
| Direct fixed point | `common/megastructures` `build_time`, `common/armies` `war_exhaustion` | `fixture_numeric_megastructures`, `fixture_numeric_armies`, `fixture_numeric_conversion_matrix` |
| Template fixed point | `common/special_projects` nested `fleet_power` | `fixture_numeric_nested_projects`, `fixture_numeric_conversion_matrix` |
| `float` | `common/star_classes` `icon_scale`; two `common/storm_types` fields | `fixture_numeric_conversion_matrix` |
| `short` | `common/astral_actions` `unlock_threshold`, `usages`; five `common/sector_types` fields | `fixture_numeric_conversion_matrix` |

Each kind has boundary, fractional and malformed inputs. Every observation requires the joined
owner, source occurrence, parser return, storage and diagnostic window. `check_storage` in
`tests/live/numeric.rs` fails the run when the static representation, width, scale or signedness
differs from the observed storage.

## Boundary evidence on M451-hotfix (SDK-655)

These findings apply to the exact M451-hotfix build in [targets](targets.md).
The existing whole-function shapes match all 11 shared readers. SDK-655 adds authored controls
for the scanner pointer, scanner return check, narrowing loads/stores, and both fixed wrappers'
raw paths. Removing or changing required instructions loses the proof. Numeric storage facts
and `NumericBound` are unchanged. The float reader additionally reports
`numeric-float-bound-representation`, exposed as a public `GapKind::NumericConversion` with
the detail `Exact binary32 range endpoints cannot be represented by NumericBound.`

### Faithful storage and endpoint requirements

Jackson's SDK-655 decision defines `accepted_range` as faithful storage: inputs stored without
overflow or narrowing beyond the reader's normal, established rounding or truncation rule.
An outside value may still parse successfully. Each `Known` endpoint requires both a verified
contract of the exact imported platform scanner, checked by a reproducible test, and agreeing
live boundary cases on each side of that limit. Otherwise the endpoint stays `Unresolved`; a
range with one established endpoint is `Partial`. Width alone is insufficient. Knowing a range
does not close `numeric-overflow` unless behavior outside the range is also established.

The rounding/truncation statements have their own limits:

- For ordinary base-10 fractional text, the int reader scans the integer prefix, giving truncation
  toward zero. The live positive and negative `1.23456789` samples agree. This is not a general
  floating-point parser: `1e2` yields 1, and radix-prefixed forms follow `%i`.
- Narrow integer paths scan a 32-bit integer prefix and then keep its low byte or halfword.
  Discarding high bits is narrowing, not an allowed rounding rule for faithful storage.
- Direct fixed point's ordinary decimal path keeps at most five fractional characters, giving
  truncation toward zero for canonical decimal fractions. The integer component follows `%lli`;
  radix prefixes and malformed fractional suffixes must not inherit the decimal statement.
- The template scales the parsed binary64 fractional component, applies the matched signed
  half-unit adjustment and `fcvtzs` integer truncation, then combines it with the shifted whole
  component. A universal exact-decimal rounding rule is not established independently of the
  imported scanner and floating-point environment. Its raw paths do not perform this rounding.
- The float samples preserve exact binary32 result bits. They do not establish a universal
  rounding rule for every decimal input and floating-point environment.

The UUID-pinned platform test establishes scanner behavior at the sampled limits and just
outside them. Together with matching live inward/endpoint/outward samples and complete engine
conversion paths, it meets the accepted endpoint bar. Static proof of libc implementation is
not required. In-range integer conversion follows the scanner's defined conversion semantics;
out-of-range results remain finite observations rather than a universal overflow contract.

| Reader | `accepted_range` | Fractional rule and remaining overflow limit |
| --- | --- | --- |
| `int` | `Known`: `Signed(-2147483648)` through `Signed(2147483647)` | Ordinary decimal fractions retain their integer prefix, truncating toward zero. `numeric-overflow` stays: sampled outside-limit wrap is not a general rule. |
| `CFixedPoint` | `Known`: `Rational(-9223372036854775808, 100000)` through `Rational(9223372036854775807, 100000)` | Canonical decimal fractions keep five digits, truncating toward zero. `numeric-overflow` stays: sampled outside-limit wrap is not a general rule. |
| Fixed-point template | `Known`: `Rational(-9223372036854775808, 32768)` through `Rational(9223372036854775807, 32768)` | Canonical decimal fractional magnitude is scanned to binary64, multiplied by 32768, adjusted by +0.5, then truncated toward zero by `fcvtzs`; the original leading sign controls addition/subtraction. This is rounding the scanned magnitude to the nearest raw unit, with half units away from zero, subject to binary64 conversion. `numeric-overflow` stays: sampled outside-limit wrap is not a general rule. |
| `signed char`, `unsigned char`, `short`, `unsigned short` | `Unresolved` | Signed interpretation remains unresolved; only `short` has exposed live boundaries. |
| `unsigned int`, `long long`, `unsigned long long` | `Unresolved` | No exposed live field and therefore no agreeing live boundary set. |
| `float` | `Unresolved` | No exact binary32 bound variant; `numeric-float-bound-representation` remains. |

Both endpoints of all three known ranges have agreeing inward, endpoint and outward live
samples. Bounds serialize as exact integers or numerator/denominator pairs. The ranges describe
ordinary numeric literal conversion; they do not resolve lexer acceptance, malformed suffixes,
raw-value mode selection or later field adjustments. Missing conversion paths or an unproved
scale cannot inherit these ranges. `NumericBound` is unchanged.

Scoped literals inherit these token-reader facts only when their concrete storage is established.
Unresolved constructor destinations have no range; a known shared token conversion cannot supply the
missing destination proof. The [current result](#current-result-on-m451-hotfix) records the range
counts, and [scoped literal ranges](scoped-numeric.md#shared-literal-ranges-sdk-655) records the
corresponding live boundary cases. Outward wrapped values remain observations and do not close the
scoped literal conversion gap.

### Engine boundary, suffixes and narrow storage

Every matched token conversion loads the original text pointer from token offset `0x10`.
The first scanner call uses that pointer; direct fixed point's second call uses its five-character
fractional buffer. The wrappers pass their token at reader offset `0x278`. The scanner receives no token length,
end pointer or `%n` destination. The simple conversions compare the assignment count with zero,
not with the input length and not with a positive-success threshold. The readers do not check
`errno`. These are reader-code facts, not a proof of which source characters the lexer includes
in a token.

The length-taking `CToken::Init` at `0x1025bd0a4` copies the supplied number of bytes and writes
a zero byte immediately after them (`0x1025bd150` and `0x1025bd154`). This inspected constructor
does not establish that every numeric token uses this path or that its supplied length has a
particular lexical meaning. `CTextLexer::GetTok` at `0x1025ae0a4` has unresolved virtual input
calls and indirect dispatch at `0x1025ae1c0`, `0x1025ae4bc` and `0x1025ae53c`. The numeric method
does not prove these paths or their delimiter, quote and escape handling. The
`numeric-lexical-boundary` gap therefore stays, including for scoped literals.

| Reader | Token entry | Engine conversion and destination |
| --- | --- | --- |
| `signed char` | `0x1025bdfd4` | `%d` into a zero-initialized 32-bit temporary; low byte stored |
| `unsigned char` | `0x1025bdf80` | The same instruction shape and `%d`, including the low-byte store |
| `short` | `0x1025be1f4` | `%d` into a 32-bit temporary; low halfword stored |
| `unsigned short` | `0x1025be1a4` | `%u` into a 32-bit temporary; low halfword stored |
| `int` | `0x1025bdf4c` | `%i` into the supplied destination |
| `unsigned int` | `0x1025be170` | `%u` into the supplied destination |
| `long long` | `0x1025be0a0` | `%lld` into the supplied destination |
| `unsigned long long` | `0x1025bdbb8` | `%llu` into the supplied destination |
| `float` | `0x1025bdc68` | `%f` into the supplied destination |
| `CFixedPoint` | `0x1025bdc9c` | `%lli`, an independent `strchr` search for a dot, and up to five fractional characters; integer multiply/add and sign adjustment |
| Fixed-point template | `0x1025bdddc` | `%lld%lf`, scaled fractional conversion, whole-component shift and add/subtract; scanner assignment count is ignored |

Both byte and both halfword paths substitute zero when the scanner returns zero. The halfword
temporary is not initialized before the scanner call. An EOF return is nonzero: do not describe
the reader check as “one assignment succeeded,” or infer a deterministic short value from an
empty input. The library probe initializes its own destinations and does not model that
uninitialized stack slot.

None of the four narrow paths interprets the stored sign bit. A `%d`/`%u` temporary gives the
scanner's 32-bit conversion type, not the signedness of the later 8/16-bit value. The two byte
bodies cannot distinguish signed from unsigned storage at all. Their `signedness: Unresolved`
is retained; caller interpretation or an independently bound type contract is still missing.

No matched reader demands full scanner consumption. This is not a universal “ignore all
suffixes” rule: direct fixed point searches the original token for a dot independently of the
integer scanner's stopping position, and the template asks for a second conversion. The finite
suffix observations below do not close `numeric-trailing-text` for arbitrary text.

### Raw fixed-point paths

The direct wrapper at `0x1025b7460` and template wrapper at `0x1025b81f8` load the object at
reader offset `0x30`, call its virtual slot `0x20`, and choose the raw path for a nonzero result.
That path zero-initializes a temporary, invokes the signed 64-bit token reader (`%lld`), and
copies the resulting integer to the destination without applying the ordinary scale. A zero
conversion result reports `Malformed token`; the destination store belongs to the nonzero
path. Authored controls require both this store and the selector branch.

Inspection identifies the standard lexer implementations of this slot. `CReader(CLexer&)` stores
its supplied lexer at offset `0x30` (`0x1025b1ddc`). The text lexer vtable's slot at `0x10326c5e8`
points to `CTextLexer::IsBinary` (`0x1025af138`, constant zero); the binary lexer slot at
`0x10326c620` points to `CBinLexer::IsBinary` (`0x1025af24c`, constant one). With those receivers,
the wrapper's raw path is the binary-lexer path. Exact-build parity checks these slots and
constant-return bodies. The numeric method does not join each reader's runtime receiver to
these concrete vtables, so this conditional result does not remove its mode gap.

The template token reader has another raw path: token kind `0x167` calls imported `atoll`, stores
its result without scaling and returns success unconditionally. A constructor from a template
fixed-point object (`0x1025bd784`) sets kind `0x167`, loads the object's raw integer and formats
it with `%lld`; this is an internal object-to-token route, not an authored-text spelling.
Its other conversion path also returns
success regardless of the `%lld%lf` assignment count. These paths explain why a storage scale
does not imply that every input is multiplied by that scale. Neither the wrapper's concrete
receiver nor the source syntax that produces token kind `0x167` is joined by the method.
`numeric-raw-value-mode` stays for both fixed readers; a source spelling is not inferred from
the integer token code.

### Exact-platform library observations

`tests/numeric_scanner.rs` compiles `tools/numeric_scanner.c` with the host C compiler and checks
231 finite observations against `tests/expected/numeric-m45/scanner-platform.json`. It also checks
the exact game build, the game's undefined `_sscanf` and `_atoll` imports from `libSystem`, and
the loaded implementation's identity. Both functions resolve to `/usr/lib/system/libsystem_c.dylib`,
Mach-O UUID `fba7b23eaa603a909aa4a7a2e0ad63ee`, on ARM64 macOS build `26A428`. A different image or
OS build fails the comparison rather than inheriting these observations.

The probe uses locale `C`. It records assignment counts, `errno`, exact stored bits, and a separate
call with appended `%n` to measure consumption. The instrumented call must give the same count
and stored bits as the original call. For `%lld%lf`, failure of the second conversion leaves
`%n` unreached (`consumed: -1`); it is not a measurement of zero characters consumed. Probe
destinations start at 7, except the two-component conversion which starts at zero like the
engine. `atoll` has no assignment count. This is the reproducible exact-platform boundary check supporting the three ranges above.
It is not a universal contract for out-of-range C behavior that the language leaves undefined.

| Conversion | Observed at and around the tested limits |
| --- | --- |
| `%i`, `%d` | `2147483647` stores `0x7fffffff`; `2147483648` stores `0x80000000`; `-2147483649` stores `0x7fffffff`. Each returns 1 with `errno` 0. At the signed 64-bit overflow probes, low 32 bits of the saturated 64-bit result are stored and `errno` is 34. |
| `%u` | `4294967295` stores `0xffffffff`; `4294967296` stores zero; `-1` stores `0xffffffff`, all with count 1 and `errno` 0. `18446744073709551616` stores `0xffffffff` with `errno` 34. |
| `%lli`, `%lld` | Endpoints and their inward neighbors are stored exactly. One unit beyond either signed 64-bit endpoint stores the corresponding endpoint, with count 1 and `errno` 34. |
| `%llu` | `0`, `1` and the unsigned maximum and its inward neighbor are stored exactly. `-1` stores the unsigned maximum with `errno` 0; one unit above the maximum stores the maximum with `errno` 34. |
| `%f` | Exact finite extrema and adjacent inward floats retain their expected bits. Positive/negative `2^128` give positive/negative infinity with count 1 and `errno` 34. The minimum normal, largest subnormal, minimum subnormal and a value below the rounding midpoint are tested; subnormal and zero-underflow samples set `errno` 34. |
| `%lld%lf` | The whole component has the tested `%lld` saturation. Separate positive/negative binary64 boundary inputs give finite extrema or infinities in the fractional destination; normal/subnormal/zero-underflow samples retain their exact bits. These are library component results before the engine's shift, rounding and `fcvtzs`. |
| `atoll` | Signed 64-bit endpoints and inward neighbors are exact; one unit outside saturates to the endpoint with `errno` 34. `not_a_number` and empty text give zero with `errno` 22. |

All seven single scanner formats consume only `12` from `12tail` and from `12 34`. The `%i`
and `%lli` samples read `0x10` as 16 and `010` as 8; decimal integer formats read the `0` prefix
of `0x10` and read `010` as 10. `%f` reads `1e2` as 100. The two-component format can consume
`12 34` as two values. Invalid nonnumeric input has zero assignments; empty input has EOF (`-1`).
These observations do not prove the game's locale or its lexer behavior.

The exact-platform test runs with:

```sh
NATIVE_SCANNER_REPORT=.local/sdk-655/scanner-platform.json cargo test --release --test numeric_scanner -- --ignored
```

`numeric-overflow` and `numeric-external-library-conversion` stay for all 11 readers. Finite
platform samples do not prove behavior for all overlong numbers, all lexical forms, other
locales, or fixed-point arithmetic after scanning. The six readers without an exposed live field
are excluded from the live requirement by the AC4 amendment above. The three ranges above combine
these platform checks with matched engine paths and live boundaries; the other eight remain
unresolved. Exact finite
binary32 extrema also exceed every current `NumericBound` representation; the maintainer decision
keeps that type unchanged and requires the specific float gap above.

### Live boundary samples

The M451-hotfix matrix contains **189 cases and 203 member-return stored values**, with complete
owner, source, parser-return, storage and diagnostic joins. All 110 SDK-656 cases are unchanged;
the 79 added cases supply inward neighbors, endpoints and outward neighbors of the tested integer
and fixed-point storage limits, float finite/subnormal boundaries, short bit boundaries, and
suffix/quoted-space samples. Case names such as `fixed_min` and `unsigned_short_max` identify
test inputs around storage limits; the three accepted ranges require the combined evidence
above. These case names do not identify an unsigned short reader.

The four original integer/fixed contexts each have 32 cases. Float boundaries use the verified
`common/star_classes` `icon_scale` field; the two storm fields retain their three storage cases.
Short boundaries use `common/astral_actions` `unlock_threshold`; `usages` and the five sector
fields retain their three storage cases. Each session stays within the fixture API's 32-question
limit. Adding all boundary cases to every float/short field exceeds that limit; it is a request
failure, not an engine conversion result.

| Reader and input | Observed member-return storage |
| --- | --- |
| Direct int, `2147483646`, `2147483647`, `2147483648` | `2147483646`, `2147483647`, `-2147483648` |
| Direct int, `-2147483647`, `-2147483648`, `-2147483649` | `-2147483647`, `-2147483648`, `2147483647` |
| Direct fixed, `92233720368547.75806`, `.75807`, `.75808` with the same whole part | Raw `9223372036854775806`, `9223372036854775807`, `-9223372036854775808` |
| Direct fixed, `-92233720368547.75807`, `.75808`, `.75809` with the same negative whole part | Raw `-9223372036854775807`, `-9223372036854775808`, `9223372036854775807` |
| Template, `281474976710655.99993896484375`, `281474976710655.999969482421875`, `281474976710656.0` | Raw `9223372036854775806`, `9223372036854775807`, `-9223372036854775808` |
| Template, `-281474976710655.999969482421875`, `-281474976710656.0`, `-281474976710656.000030517578125` | Raw `-9223372036854775807`, `-9223372036854775808`, `9223372036854775807` |
| Float, positive finite maximum's inward neighbor, maximum, positive `2^128` | Bits `0x7f7ffffe`, `0x7f7fffff`, `0x7f800000` (infinity) |
| Float, negative finite minimum's inward neighbor, minimum, negative `2^128` | Bits `0xff7ffffe`, `0xff7fffff`, `0xff800000` (negative infinity) |
| Float, minimum normal, largest subnormal, minimum subnormal, below-half-minimum sample | Bits `0x00800000`, `0x007fffff`, `0x00000001`, `0x00000000` |
| Short, `32766`, `32767`, `32768`; `-32767`, `-32768`, `-32769` | Bits `0x7ffe`, `0x7fff`, `0x8000`; `0x8001`, `0x8000`, `0x7fff` |
| Short, `65534`, `65535`, `65536`; scanner inputs `2147483647`, `2147483648` | Bits `0xfffe`, `0xffff`, `0x0000`; `0xffff`, `0x0000` |
| `1.25tail`, direct int / direct fixed / template | `1` / raw `100025` / raw `40960` |
| Quoted `"12 34"`, direct int / direct fixed / template / float / short | `12` / raw `1200000` / raw `1507328` (46) / bits `0x41400000` / bits `0x000c` |

All added samples have no source-located diagnostics. In the direct fixed `1.25tail` sample,
the five-character fractional buffer contains `25tai`; parsing its numeric prefix gives 25 raw
units, not 25000. The quoted-space template sample demonstrates that a successful second scan
can affect the result even when the first scan stops before the token ends. Neither observation
is a universal suffix rule. Megastructure `build_time` still changes nonpositive results after
the member return; its final fallback remains separate from the shared-reader observations.

The six unbound reader types listed under SDK-656 have platform scanner samples but no live
field cases. Raw binary-lexer mode and internally constructed kind-`0x167` tokens have no new
live claim. These limitations are not counted as successful coverage; the AC4 amendment above
accepts the six readers as exclusions, not as covered readers.

Reproduce with `cargo live fixture_numeric` and `cargo live fixture_scoped_numeric`, one at a
time. The reviewed numeric expectations are `tests/expected/numeric-m45/live.json`; mismatches
write `.local/sdk-655/numeric-conversion-live.json`. The shared engine parity commands are:

```sh
cargo test --lib numeric
cargo test --release --lib -- --ignored m45_numeric_reader_static_parity m45_scoped_numeric m451_float_and_short_fixture_storage_bindings m451_numeric_boundary_engine_parity
```
