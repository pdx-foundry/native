//! Whole-function numeric conversion proofs. Shapes keep every branch, call and destination
//! store, so a constant alone cannot establish a scale. Bound scanner contracts establish
//! partial literal forms. The scanner's own conversion is a boundary of the method; what the engine
//! does around it is in the matched shapes. The token text that every reader converts comes from
//! the text lexer, whose boundary `lexer` establishes once.
use std::collections::BTreeMap;
pub(crate) mod lexer;
pub(crate) use lexer::LexerInput;
pub(crate) mod modifier;
pub(crate) use modifier::ModifierInput;
pub use modifier::ModifierNumericEntry;

/// Exact binary32 extrema do not fit the public bound variants.
pub(crate) const FLOAT_BOUND_REPRESENTATION_GAP: &str = "numeric-float-bound-representation";

/// The fixed-point raw path, which only a binary lexer takes: save game and network input.
pub(crate) const BINARY_INPUT_BOUNDARY: &str = "numeric-binary-input";

/// The imported scanner's own conversion of the token text, which the method does not read.
const SCANNER_BOUNDARY: [&str; 3] = [
    "numeric-overflow",
    "numeric-trailing-text",
    "numeric-external-library-conversion",
];

/// The token kind that sends the fixed-point template to its unscaled `atoll` path: the static
/// token `long_float` and the kind of a token built from a `CFixedPoint`. A numeric word has kind
/// `0xc`, so it never takes this path.
const TEMPLATE_RAW_TOKEN_KIND: u64 = 0x167;

use super::decode::Instruction;
use super::references::shapes::{Bindings, Shape, canonical};
use super::stop::Unresolved;
use crate::{
    GrammarProperty, NumericBound, NumericConversion, NumericLiteralSyntax, NumericRange,
    NumericRepresentation, NumericSignedness,
};

/// Numeric facts keyed by the same callee that establishes the public reader identity.
#[derive(Debug, Clone, serde::Serialize)]
pub struct NumericFacts {
    /// Numeric entry insertion boundary; independent of modifier block grammar.
    pub modifier_entry: Result<ModifierNumericEntry, Unresolved>,
    /// Every bound numeric reader, including unsuccessful analyses.
    pub readers: BTreeMap<String, NumericReader>,
    /// Token conversions used directly by constructed scoped numeric subtypes.
    pub token_readers: BTreeMap<String, NumericReader>,
}

/// Independent facts and the remaining analysis obstructions for one shared reader.
#[derive(Debug, Clone, serde::Serialize)]
pub struct NumericReader {
    /// Established properties, without promoting partial facts to a complete conversion.
    pub conversion: GrammarProperty<Option<NumericConversion>>,
    /// Limitations that make an answer using this reader partial.
    pub gaps: Vec<Unresolved>,
    /// Behavior outside the method: the platform scanner and the binary input path.
    pub boundary: Vec<Unresolved>,
}

/// Executable code and semantic helper identities supplied by the binding.
pub(crate) struct NumericInput {
    pub modifier: ModifierInput,
    pub readers: BTreeMap<String, ReaderInput>,
    pub token_readers: BTreeMap<String, TokenInput>,
    /// Token methods that return the converted value of their text, such as `CToken::GetInt`.
    pub token_values: BTreeMap<String, TokenInput>,
    /// The engine's own fixed-point conversion of a text, which a token value method may call.
    pub fixed_point_text: Vec<Instruction>,
    pub lexer: LexerInput,
}

pub(crate) struct TokenInput {
    pub body: Vec<Instruction>,
    pub names: BTreeMap<u64, String>,
    pub token_text_offset: u64,
}

pub(crate) struct ReaderInput {
    pub wrapper: Vec<Instruction>,
    pub token: Vec<Instruction>,
    pub raw_token: Vec<Instruction>,
    pub names: BTreeMap<u64, String>,
    pub reader_token_offset: u64,
    pub token_text_offset: u64,
}

pub(crate) fn analyze(input: &NumericInput) -> NumericFacts {
    let lexer = lexer::token_boundary(&input.lexer);
    let selector = lexer::text_selector(&input.lexer);
    let mut readers: BTreeMap<_, _> = input
        .readers
        .iter()
        .map(|(name, input)| (name.clone(), analyze_reader(input, &lexer, &selector)))
        .collect();
    readers.extend(input.token_values.iter().map(|(name, value)| {
        let conversion = value_conversion(value, &input.fixed_point_text);
        (name.clone(), reader_from_conversion(conversion, &lexer))
    }));
    NumericFacts {
        modifier_entry: modifier::analyze(&input.modifier, &readers),
        readers,
        token_readers: input
            .token_readers
            .iter()
            .map(|(name, token)| (name.clone(), analyze_token(token, &lexer)))
            .collect(),
    }
}

impl Default for NumericFacts {
    fn default() -> Self {
        Self {
            readers: BTreeMap::new(),
            token_readers: BTreeMap::new(),
            modifier_entry: Err(Unresolved::new("modifier-numeric-not-analyzed")),
        }
    }
}

fn analyze_token(input: &TokenInput, lexer: &Result<(), Unresolved>) -> NumericReader {
    let conversion = token_conversion(&input.body, &input.names, input.token_text_offset);
    reader_from_conversion(conversion, lexer)
}

/// The fact of a token conversion that has no reader wrapper and no raw path.
fn reader_from_conversion(
    conversion: Option<NumericConversion>,
    lexer: &Result<(), Unresolved>,
) -> NumericReader {
    let Some(conversion) = conversion else {
        return unresolved("numeric-token-shape");
    };

    NumericReader {
        gaps: conversion_gaps(&conversion, lexer),
        boundary: scanner_boundary(),
        conversion: GrammarProperty::Partial(Some(conversion)),
    }
}

/// `selector` is the text lexer's raw-path selector: `Ok` when a reader on a text lexer never takes
/// the fixed-point raw path.
fn analyze_reader(
    input: &ReaderInput,
    lexer: &Result<(), Unresolved>,
    selector: &Result<(), Unresolved>,
) -> NumericReader {
    let wrapper = canonical(&input.wrapper, &input.names);
    let scalar = Shape::parse(include_str!("numeric/shapes/scalar.txt")).matches(&wrapper);
    let fixed = Shape::parse(include_str!("numeric/shapes/fixed_wrapper.txt")).matches(&wrapper);
    let Some(binding) = scalar.as_ref().or(fixed.as_ref()) else {
        return unresolved("numeric-wrapper-shape");
    };
    if number(binding, "reader_token") != Some(input.reader_token_offset) {
        return unresolved("numeric-token-destination");
    }
    let ordinary = token_conversion(&input.token, &input.names, input.token_text_offset);
    let (conversion, missing_path) = if fixed.is_some() {
        let raw = token_conversion(&input.raw_token, &input.names, input.token_text_offset);
        fixed_paths(ordinary, raw)
    } else {
        (ordinary, None)
    };
    let Some(conversion) = conversion else {
        return unresolved("numeric-token-shape");
    };
    let mut gaps = conversion_gaps(&conversion, lexer);
    let mut boundary = scanner_boundary();
    gaps.extend(missing_path.map(Unresolved::new));
    if fixed.is_some() {
        match selector {
            Ok(()) => boundary.push(Unresolved::new(BINARY_INPUT_BOUNDARY)),
            Err(obstacle) => gaps.push(obstacle.clone()),
        }
    }

    NumericReader {
        conversion: GrammarProperty::Partial(Some(conversion)),
        gaps,
        boundary,
    }
}

fn scanner_boundary() -> Vec<Unresolved> {
    SCANNER_BOUNDARY.map(Unresolved::new).to_vec()
}

/// The limitations of a matched conversion. `lexer` is the token boundary's result; its obstacle,
/// if any, is the gap on the token text that every scanner receives.
fn conversion_gaps(
    conversion: &NumericConversion,
    lexer: &Result<(), Unresolved>,
) -> Vec<Unresolved> {
    let mut gaps: Vec<_> = lexer.clone().err().into_iter().collect();
    if conversion.representation == GrammarProperty::Known(NumericRepresentation::BinaryFloat)
        && conversion.width_bits == GrammarProperty::Known(32)
    {
        gaps.push(Unresolved::new(FLOAT_BOUND_REPRESENTATION_GAP));
    }
    gaps
}

/// Raw mode transfers an already-scaled integer. Its scanner syntax contributes forms, but its
/// unit scale must never replace the ordinary fixed-point path's storage scale.
fn fixed_paths(
    ordinary: Option<NumericConversion>,
    raw: Option<NumericConversion>,
) -> (Option<NumericConversion>, Option<&'static str>) {
    let Some(mut ordinary) = ordinary else {
        let raw = raw.map(|mut raw| {
            raw.scale = GrammarProperty::Unresolved;
            incomplete_path(raw)
        });
        return (raw, Some("numeric-token-shape"));
    };
    let Some(raw) = raw else {
        return (
            Some(incomplete_path(ordinary)),
            Some("numeric-raw-conversion"),
        );
    };
    if raw.representation != GrammarProperty::Known(NumericRepresentation::Integer)
        || raw.width_bits != GrammarProperty::Known(64)
        || raw.signedness != GrammarProperty::Known(NumericSignedness::Signed)
    {
        return (Some(incomplete_path(ordinary)), Some("numeric-raw-storage"));
    }
    if let (GrammarProperty::Partial(forms), GrammarProperty::Partial(raw_forms)) =
        (&mut ordinary.literal_syntax, &raw.literal_syntax)
    {
        for form in raw_forms {
            if !forms.contains(form) {
                forms.push(*form);
            }
        }
    }
    (Some(ordinary), None)
}

/// Keep facts about the proved path without claiming that every conversion path agrees.
fn incomplete_path(mut conversion: NumericConversion) -> NumericConversion {
    fn partial<T>(property: GrammarProperty<T>) -> GrammarProperty<T> {
        match property {
            GrammarProperty::Known(value) => GrammarProperty::Partial(value),
            other => other,
        }
    }
    conversion.representation = partial(conversion.representation);
    conversion.width_bits = partial(conversion.width_bits);
    conversion.signedness = partial(conversion.signedness);
    conversion.scale = partial(conversion.scale);
    conversion.accepted_range = GrammarProperty::Unresolved;
    conversion
}

fn unresolved(reason: &'static str) -> NumericReader {
    NumericReader {
        conversion: GrammarProperty::Unresolved,
        gaps: vec![Unresolved::new(reason)],
        boundary: Vec::new(),
    }
}

fn token_conversion(
    rows: &[Instruction],
    names: &BTreeMap<u64, String>,
    text_offset: u64,
) -> Option<NumericConversion> {
    let lines = canonical(rows, names);
    for (shape, form) in [
        (include_str!("numeric/shapes/scan.txt"), Form::Scan),
        (
            include_str!("numeric/shapes/narrow_byte.txt"),
            Form::Narrow(8),
        ),
        (
            include_str!("numeric/shapes/narrow_half.txt"),
            Form::Narrow(16),
        ),
        (include_str!("numeric/shapes/decimal.txt"), Form::Decimal),
        (include_str!("numeric/shapes/binary.txt"), Form::Binary),
    ] {
        let Some(binding) = Shape::parse(shape).matches(&lines) else {
            continue;
        };
        if number(&binding, "token_text") != Some(text_offset) {
            continue;
        }
        return form.conversion(&binding);
    }
    None
}

/// A token value method loads the token text and tail-calls one conversion: the imported `atoi`,
/// or the engine's fixed-point conversion, whose whole body must match its own shape.
fn value_conversion(
    input: &TokenInput,
    fixed_point_text: &[Instruction],
) -> Option<NumericConversion> {
    let lines = canonical(&input.body, &input.names);
    let binding = Shape::parse(include_str!("numeric/shapes/token_value.txt")).matches(&lines)?;
    if number(&binding, "token_text") != Some(input.token_text_offset) {
        return None;
    }
    match binding.get("conversion")?.as_str() {
        "decimal_int" => Form::Int.conversion(&binding),
        "fixed_point_text" => {
            let lines = canonical(fixed_point_text, &input.names);
            let binding = Shape::parse(include_str!("numeric/shapes/fixed_point_text.txt"))
                .matches(&lines)?;
            Form::FixedPointText.conversion(&binding)
        }
        _ => None,
    }
}

enum Form {
    Scan,
    Narrow(u8),
    Decimal,
    Binary,
    /// `atoi`: the scanner's `int`.
    Int,
    /// `StringToFixedPoint`: `atoll` for the whole part, then every character after the first dot.
    FixedPointText,
}

impl Form {
    fn conversion(self, binding: &Bindings) -> Option<NumericConversion> {
        use GrammarProperty::{Known, Unresolved};
        use NumericLiteralSyntax::{
            DecimalFraction, DecimalInteger, Exponent, RadixPrefixedInteger,
        };
        use NumericRepresentation::{BinaryFloat, Integer};
        use NumericSignedness::{Signed, Unsigned};
        let (representation, width, signedness, scale, literal_syntax) = match self {
            Self::Scan => match binding.get("format")?.as_str() {
                "\"%i\"" => (
                    Integer,
                    32,
                    Known(Signed),
                    Known(Some(1)),
                    vec![DecimalInteger, RadixPrefixedInteger],
                ),
                "\"%d\"" => (
                    Integer,
                    32,
                    Known(Signed),
                    Known(Some(1)),
                    vec![DecimalInteger],
                ),
                "\"%u\"" => (
                    Integer,
                    32,
                    Known(Unsigned),
                    Known(Some(1)),
                    vec![DecimalInteger],
                ),
                "\"%lli\"" => (
                    Integer,
                    64,
                    Known(Signed),
                    Known(Some(1)),
                    vec![DecimalInteger, RadixPrefixedInteger],
                ),
                "\"%lld\"" => (
                    Integer,
                    64,
                    Known(Signed),
                    Known(Some(1)),
                    vec![DecimalInteger],
                ),
                "\"%llu\"" => (
                    Integer,
                    64,
                    Known(Unsigned),
                    Known(Some(1)),
                    vec![DecimalInteger],
                ),
                "\"%f\"" => (
                    BinaryFloat,
                    32,
                    Known(Signed),
                    Known(None),
                    vec![DecimalInteger, DecimalFraction, Exponent],
                ),
                _ => return None,
            },
            // A truncating byte/halfword store alone does not establish signed interpretation.
            Self::Narrow(width) => {
                if width == 16
                    && !matches!(
                        binding.get("format").map(String::as_str),
                        Some("\"%d\"" | "\"%u\"")
                    )
                {
                    return None;
                }
                (
                    Integer,
                    width,
                    Unresolved,
                    Known(Some(1)),
                    vec![DecimalInteger],
                )
            }
            Self::Decimal => {
                let factor = scale_factor(binding)?;
                // The matched fractional path pads exactly five decimal digits. Both paths
                // must use the same unit before a single storage scale is established.
                let scale = if factor == 10u64.pow(5) {
                    Known(Some(factor))
                } else {
                    Unresolved
                };
                (
                    Integer,
                    64,
                    Known(Signed),
                    scale,
                    vec![DecimalInteger, DecimalFraction, RadixPrefixedInteger],
                )
            }
            Self::Int => (
                Integer,
                32,
                Known(Signed),
                Known(Some(1)),
                vec![DecimalInteger],
            ),
            Self::FixedPointText => {
                // The matched body scales a fraction of at most five digits to five; a longer
                // fraction keeps every digit unscaled. Only 10^5 is the unit of both parts.
                let factor = scale_factor(binding)?;
                let scale = if factor == 10u64.pow(5) {
                    Known(Some(factor))
                } else {
                    Unresolved
                };
                (
                    Integer,
                    64,
                    Known(Signed),
                    scale,
                    vec![DecimalInteger, DecimalFraction],
                )
            }
            Self::Binary => {
                if number(binding, "raw_token")? != TEMPLATE_RAW_TOKEN_KIND {
                    return None;
                }
                let shift = number(binding, "scale_shift")?;
                let scale = 1u64.checked_shl(u32::try_from(shift).ok()?);
                let factor = f64::from_bits(number(binding, "fraction_scale")?);
                let scale = match scale {
                    Some(scale) if factor == scale as f64 => Known(Some(scale)),
                    _ => Unresolved,
                };
                // `%lld%lf` reads the whole and fractional components separately. An exponent
                // on the fractional component is not an exponent on the whole input value.
                (
                    Integer,
                    64,
                    Known(Signed),
                    scale,
                    vec![DecimalInteger, DecimalFraction],
                )
            }
        };
        // These complete shapes have agreeing exact-platform scanner and live boundary
        // controls (SDK-655). Other widths and scales must not inherit their ranges.
        let accepted_range = match self {
            Self::Scan if binding.get("format")?.as_str() == "\"%i\"" => {
                Known(Box::new(NumericRange {
                    minimum: Known(NumericBound::Signed(-2147483648)),
                    maximum: Known(NumericBound::Signed(2147483647)),
                }))
            }
            Self::Decimal if scale == Known(Some(100000)) => scaled_range(100000),
            Self::Binary if scale == Known(Some(32768)) => scaled_range(32768),
            _ => Unresolved,
        };
        Some(NumericConversion {
            representation: Known(representation),
            width_bits: Known(width),
            signedness,
            scale,
            literal_syntax: GrammarProperty::Partial(literal_syntax),
            accepted_range,
            // Every matched path has no explicit bound comparison/select on the converted
            // value. Scanner overflow is the scanner boundary; the template's `fcvtzs` and
            // integer arithmetic are in its shape.
        })
    }
}

/// The constant that `mov` and `movk` form from `scale_low` and `scale_high`.
fn scale_factor(binding: &Bindings) -> Option<u64> {
    let low = number(binding, "scale_low")?;
    let high = number(binding, "scale_high")?;
    if low > u16::MAX.into() || high > u16::MAX.into() {
        return None;
    }
    Some(low | (high << 16))
}

fn scaled_range(denominator: u64) -> GrammarProperty<Box<NumericRange>> {
    GrammarProperty::Known(Box::new(NumericRange {
        minimum: GrammarProperty::Known(NumericBound::Rational {
            numerator: i64::MIN,
            denominator,
        }),
        maximum: GrammarProperty::Known(NumericBound::Rational {
            numerator: i64::MAX,
            denominator,
        }),
    }))
}

fn number(binding: &Bindings, key: &str) -> Option<u64> {
    let text = binding.get(key)?;
    match text.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => text.parse().ok(),
    }
}

#[cfg(test)]
mod tests;
