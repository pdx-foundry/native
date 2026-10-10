# Shared numeric conversion

`Reader.numeric` carries conversion facts in registry fields, read alternatives, command values,
fixed keys and ordering-selected readers. `Known(None)` is an established nonnumeric reader;
`Unresolved` is no numeric answer; `Partial(Some(...))` keeps independent known properties. The
retired clamp property is in Git at `d8f9d8a`: no complete reader shape has an explicit clamp,
which is a different fact from overflow behavior.

## Result on M451-hotfix

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

## Method

`binding/binary/numeric.rs` binds reader and token bodies, their imports and format strings;
`engine/analysis/numeric.rs` matches complete canonical instruction sequences. Changed calls,
stores, branches or unsupported shapes give an unresolved result. The scanner-format match binds
`%i`, `%d`, `%u`, `%lli`, `%lld`, `%llu` and `%f` to their storage and literal forms; it does not
establish libc overflow, locale or full-string consumption. Which bytes form the token text is the
[text lexer](text-lexer.md)'s fact, which the numeric method checks once for every reader.

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

The modifier-entry proof follows the declaration-table numeric path into the direct fixed-point
reader (`a9818fec780f8313`) and through both insertion capacity paths to a 64-bit entry store;
`m452_numeric_reader_static_parity` checks it against `tests/expected/numeric-m452/modifier-entry.json`.
[Modifier blocks](modifier-blocks.md) reuse this entry identity. Storage of an authored entry is not
observed: the fixture route does not decode modifier entries. `add_resource` resource amounts have
no static reader: `CAddResourceEffect::ReadMember` gives every other key to
`CFixedResourceTable::CSerializer::ReadMember`, which the command grammar does not route.

## Faithful storage and endpoints

`accepted_range` means faithful storage: inputs stored without overflow or narrowing beyond the
reader's established rounding or truncation rule; an outside value may still parse. A `Known`
endpoint needs a verified contract of the exact imported platform scanner, checked by a
reproducible test, and agreeing live inward, endpoint and outward cases. Width alone is not
enough, and a known range does not close `numeric-overflow`.

| Reader | `accepted_range` | Fractional rule |
| --- | --- | --- |
| `int` | `Known`: `Signed(-2147483648)` through `Signed(2147483647)` | Ordinary decimal fractions keep their integer prefix, truncating toward zero. Not a floating-point parser: `1e2` gives 1, radix prefixes follow `%i`. |
| `CFixedPoint` | `Known`: `Rational(-9223372036854775808, 100000)` through `Rational(9223372036854775807, 100000)` | Canonical decimal fractions keep five characters, truncating toward zero; the integer part follows `%lli`. |
| Fixed-point template | `Known`: `Rational(-9223372036854775808, 32768)` through `Rational(9223372036854775807, 32768)` | The fractional magnitude is scanned to binary64, multiplied by 32768, adjusted by +0.5 and truncated by `fcvtzs`; the leading sign selects add or subtract. Its raw paths do not round. |
| `signed char`, `unsigned char`, `short`, `unsigned short` | `Unresolved` | Narrow paths keep the low byte or halfword of a 32-bit scan; discarding high bits is narrowing. Sign interpretation is unresolved. |
| `unsigned int`, `long long`, `unsigned long long` | `Unresolved` | No exposed live field, so no live boundary set. |
| `float` | `Unresolved` | No exact binary32 variant in `NumericBound`; the public `NumericConversion` gap says so. |

Scoped literals inherit these ranges only when their concrete storage is established; see
[scoped numeric](scoped-numeric.md#engine-facts-and-method-m45-release-to-m452).

## Engine boundary facts (M451-hotfix)

- Every matched token conversion loads the text pointer from token `+0x10`; the wrappers pass
  their token at reader `+0x278`. The scanner gets no length, end pointer or `%n`. Simple
  conversions compare the assignment count with zero, not with the input length, and no reader
  checks `errno`.
- `CToken::Init` (`0x1025bd0a4`) copies the supplied bytes and writes a zero after them. Which
  bytes form a token is established by the [text lexer](text-lexer.md) method on M452.
- Byte and halfword paths substitute zero when the scanner returns zero; the halfword temporary is
  uninitialized before the call, and EOF is a nonzero return. Do not infer a short value from empty
  input. The two byte bodies cannot distinguish signed from unsigned storage.
- No reader demands full consumption, but this is not an "ignore all suffixes" rule: direct fixed
  point searches the original token for a dot (so `1.25tail` gives raw `100025`: the fraction buffer
  holds `25tai`), and the template asks for a second conversion (quoted `"12 34"` gives 46).

| Reader | Token entry | Conversion |
| --- | --- | --- |
| `signed char` / `unsigned char` | `0x1025bdfd4` / `0x1025bdf80` | `%d` into a zeroed 32-bit temporary; low byte stored |
| `short` / `unsigned short` | `0x1025be1f4` / `0x1025be1a4` | `%d` / `%u` into a 32-bit temporary; low halfword stored |
| `int` / `unsigned int` | `0x1025bdf4c` / `0x1025be170` | `%i` / `%u` into the destination |
| `long long` / `unsigned long long` | `0x1025be0a0` / `0x1025bdbb8` | `%lld` / `%llu` |
| `float` | `0x1025bdc68` | `%f` |
| `CFixedPoint` | `0x1025bdc9c` | `%lli`, a `strchr` search for a dot, up to five fractional characters |
| Fixed-point template | `0x1025bdddc` | `%lld%lf`, scaled fraction, whole-component shift; the assignment count is ignored |

### Raw fixed-point paths

The direct wrapper (`0x1025b7460`) and template wrapper (`0x1025b81f8`) call virtual slot `0x20`
of the lexer at reader `+0x30` and take a raw path for a nonzero result: `%lld` copied without
scale. `CTextLexer::IsBinary` (`0x1025af138`) returns 0 and `CBinLexer::IsBinary` (`0x1025af24c`)
returns 1, so the raw path is the binary-lexer path; the method does not join each reader's
runtime lexer, so `numeric-raw-value-mode` stays. The template token reader also has token kind
`0x167`, which calls `atoll` without scaling; it comes from an internal object-to-token
constructor (`0x1025bd784`), not an authored spelling.

### Exact platform scanner

`tests/numeric_scanner.rs` compiles `tools/numeric_scanner.c` and checks finite observations
against `tests/expected/numeric-m452/scanner-platform.json`, plus the game's undefined `_sscanf` and
`_atoll` imports and the loaded `libsystem_c.dylib` identity (UUID
`fba7b23eaa603a909aa4a7a2e0ad63ee`, macOS build `26A428`). Another image or OS build fails the
test instead of inheriting the observations. The probe uses locale `C`. For `%lld%lf`, a failed
second conversion leaves `%n` unreached (`consumed: -1`), not zero characters consumed.

## Live observations

`tests/live/numeric.rs` (`cargo live fixture_numeric`) holds the boundary, fractional, malformed,
suffix and quoted cases for int, direct and template fixed point, float and short;
`tests/expected/numeric-m452/live.json` holds the values, and `check_storage` fails a run when
static representation, width, scale or signedness differ from the observed storage. Six integer
readers (`signed char`, `unsigned char`, `unsigned short`, `unsigned int`, `long long`,
`unsigned long long`) have no root or exposed nested field in the bounded population (982 member
descriptions stay unreachable); the SDK-544 AC4 amendment excludes them from the live requirement.

Pitfalls:

- **A field can change its value after the member return.** Megastructure `build_time` turns
  nonpositive values into raw 100000; army `war_exhaustion` keeps the same negative value. Do not
  make this a shared-reader clamp.
- **The template stores zero silently** for `not_a_number` after `7`, while the direct readers keep 7
  and report `Malformed token`; short stores zero after the malformed occurrence. None is a shared
  conversion rule.
- **Outside-limit samples wrap**, but they are finite observations, not a wrap rule.
- **The fixture API allows 32 questions per session**; adding every boundary case to every
  float and short field exceeds it, which is a request failure, not an engine result.
- **Decimal readings of float bits are test-only text**; public values keep only bits (see
  [early observations](early-observations.md#pitfalls)).
