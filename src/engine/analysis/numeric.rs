//! Whole-function numeric conversion proofs. Shapes keep every branch, call and destination
//! store, so a constant alone cannot establish a scale. Bound scanner contracts establish
//! partial literal forms; library edge cases and lexer acceptance remain unresolved.
use std::collections::BTreeMap;
pub(crate) mod modifier;
pub(crate) use modifier::ModifierInput;
pub use modifier::ModifierNumericEntry;

/// Exact binary32 extrema do not fit the public bound variants.
pub(crate) const FLOAT_BOUND_REPRESENTATION_GAP: &str = "numeric-float-bound-representation";

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
    /// Unqualified paths or external conversion behavior.
    pub gaps: Vec<Unresolved>,
}

/// Executable code and semantic helper identities supplied by the binding.
pub(crate) struct NumericInput {
    pub modifier: ModifierInput,
    pub readers: BTreeMap<String, ReaderInput>,
    pub token_readers: BTreeMap<String, TokenInput>,
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
    let readers = input
        .readers
        .iter()
        .map(|(name, input)| (name.clone(), analyze_reader(input)))
        .collect();
    NumericFacts {
        modifier_entry: modifier::analyze(&input.modifier, &readers),
        readers,
        token_readers: input
            .token_readers
            .iter()
            .map(|(name, token)| (name.clone(), analyze_token(token)))
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

fn analyze_token(input: &TokenInput) -> NumericReader {
    let Some(conversion) = token_conversion(&input.body, &input.names, input.token_text_offset)
    else {
        return unresolved("numeric-token-shape");
    };

    let gaps = conversion_gaps(&conversion);
    NumericReader {
        conversion: GrammarProperty::Partial(Some(conversion)),
        gaps,
    }
}

fn analyze_reader(input: &ReaderInput) -> NumericReader {
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
    let mut gaps = conversion_gaps(&conversion);
    if let Some(reason) = missing_path {
        gaps.push(Unresolved::new(reason));
    }
    if fixed.is_some() {
        gaps.push(Unresolved::new("numeric-raw-value-mode"));
    }
    NumericReader {
        conversion: GrammarProperty::Partial(Some(conversion)),
        gaps,
    }
}

fn conversion_gaps(conversion: &NumericConversion) -> Vec<Unresolved> {
    let mut gaps = vec![
        Unresolved::new("numeric-overflow"),
        Unresolved::new("numeric-lexical-boundary"),
        Unresolved::new("numeric-trailing-text"),
        Unresolved::new("numeric-external-library-conversion"),
    ];
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

enum Form {
    Scan,
    Narrow(u8),
    Decimal,
    Binary,
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
                let low = number(binding, "scale_low")?;
                let high = number(binding, "scale_high")?;
                if low > u16::MAX.into() || high > u16::MAX.into() {
                    return None;
                }
                let factor = low | (high << 16);
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
            Self::Binary => {
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
            // value. Scanner overflow and fcvtzs behavior are separate unresolved properties.
        })
    }
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
