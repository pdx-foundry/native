# Direct numeric conversion on M45-release

SDK-644 adds conversion facts to `Reader.numeric` in registry fields, conditional read
alternatives, command values, fixed keys and ordering-selected readers. The shared identity is
unchanged. `Known(None)` means an established nonnumeric reader; `Unresolved` means no numeric
answer; `Partial(Some(...))` keeps independent known properties. Older recordings default to
unresolved. No separate public operation is added.

## Method and limits

The exact executable is M45-release (`07988b4f1b865623becd7a61af1cae92e111be6515d341754af70f02107822cd`),
with ARM64 slice `a4cb49ad17a84ef6bf438019a50d3a66362c80731f8359888ddbce47c0d0aab9`.
`binding/binary/numeric.rs` binds reader and token bodies, their imports, and format strings.
`engine/analysis/numeric.rs` matches complete canonical instruction sequences. Register allocation
and address relocation can vary; changed calls, stores, branches or unsupported shapes produce
an unresolved result. The shared fact table is cached per installation; access still verifies the
executable. No field or registry name selects a result.

The scanner contract binds `%i`, `%d`, `%u`, `%lli`, `%lld`, `%llu` and `%f` to their storage and
representable literal forms. This contract does not establish libc overflow, locale, tokenization
or full-string consumption. Complete matched paths prove there is no explicit reader clamp;
`clamp: Known(None)` does not promise that imported conversion or an instruction never saturates.
Accepted bounds remain unresolved, independently of storage width. Signed/unsigned/rational
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
modifier declarations or claim that modifier blocks themselves store a scalar. Modifier grammar,
category applicability, duplicates and application remain outside this method.

## Live observations

`tests/live/numeric.rs` tests 20 input sequences on four field contexts: megastructure integer,
megastructure direct fixed point, army direct fixed point, and nested project template fixed
point. Each malformed sequence contains two occurrences, so 80 cases contain 84 stored values.
Every case requires complete owner/source/parser-return/storage/diagnostic coverage. Static
representation, signedness, width and scale are checked against typed member-return storage.
Final stored values and diagnostics are checked separately. No static/live storage conflict was
found in these cases.

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
remain in its retained temporary directories named in that run's output. Static fixture binding
still lacks decoders for float and the other integer widths; no live proof is claimed for them.

## First-release numeric forms (SDK-544)

The first Atlas release names three numeric uses: resource changes, additive and multiplicative
naval capacity, and literal values. Each has a shared reader result and an observation, or a
named gap. The world observations are on M451-hotfix; see
[world evaluation](scoped-numeric.md#world-evaluation-on-m451-hotfix-sdk-647).

| Form | Reader result | Observation | Gap and owner |
| --- | --- | --- | --- |
| `add_resource` `mult` and `multiplier` | Scoped operand, signed 64-bit, scale 100000, with the partial operand forms and the selection rule ([scoped numeric](scoped-numeric.md)) | World: with `energy = 10`, literal `2.5` adds 25 through either key; a variable of 2.5 adds 25; `trigger:num_owned_planets` (1) adds 10; `value:tech_weight_likelihood` (1.25) adds 12.5; `modifier:country_edict_fund_add` (15) adds 150 | Literal boundaries, as for every scoped operand |
| `add_resource` resource amounts | None. `CAddResourceEffect::ReadMember` gives every other key to `CFixedResourceTable::CSerializer::ReadMember`, and the command grammar does not route that member (`reader-routing`, `unknown-key-reader`) | World: `energy = 10` adds raw 1000000, `0.5` adds raw 50000, `-3` removes raw 300000 | The static reader of resource-named keys. Owner: the command grammar gaps of SDK-625 and SDK-626 |
| Additive naval capacity, `country_naval_cap_add` | A modifier entry joins the direct fixed-point reader: signed 64-bit, scale 100000 (`tests/expected/numeric-m45/modifier-entry.json`) | The direct fixed-point cases above. World: after `add_modifier` of `fallen_empire_base` (entry value 1000), `modifier:country_naval_cap_add` gives raw 100000000 and integer 1000, from 0 before | Storage of an entry from authored text is not observed: a world loads no mod content, and the fixture route does not decode modifier entries. Grammar: SDK-607. Application and propagation: SDK-547 |
| Multiplicative naval capacity, `country_naval_cap_mult` | The same entry reader | World: after `add_modifier` of `community_champion_counselor` (entry value 0.1), `modifier:country_naval_cap_mult` gives raw 10000 and integer 0, from 0 before | The same |
| Literal values | The table of reader shapes above | The live matrix above; the world literal cases | The five conversion gap shapes in the [discovery index](discovery.md#direct-numeric-conversion-sdk-644) |

Both naval-capacity results equal `export_modifier_to_variable` for the same modifier. The
integer 0 for 0.1 is the truncation of a fixed-point result in an integer destination.
The added modifiers were not visible in the statement after `add_modifier`; see the pitfall on
the scoped numeric page.

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
[discovery index](discovery.md#direct-numeric-conversion-sdk-644).
