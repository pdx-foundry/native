//! Numeric conversion properties of a shared reader, separate from caller post-processing.
use crate::GrammarProperty;
use serde::{Deserialize, Serialize};

/// Independently established properties of a numeric reader. Storage limits do not establish
/// accepted literals or parser bounds. Unresolved properties must not be inferred from others.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NumericConversion {
    /// Representation written to the destination.
    pub representation: GrammarProperty<NumericRepresentation>,
    /// Width of the destination in bits.
    pub width_bits: GrammarProperty<u8>,
    /// Interpretation of the stored number's sign, when the conversion proves it.
    pub signedness: GrammarProperty<NumericSignedness>,
    /// Stored integer units per whole value. Floating-point storage has no integer scale.
    pub scale: GrammarProperty<Option<u64>>,
    /// Forms supported on established conversion paths for representable inputs. A partial
    /// list proves no exclusions or availability in every token/reader mode. Lexer boundaries,
    /// trailing text and overflow remain separate from these conversion forms.
    pub literal_syntax: GrammarProperty<Vec<NumericLiteralSyntax>>,
    /// Bounds for faithful storage: no overflow or narrowing beyond the representation's
    /// established rounding or truncation rule. Values outside may still parse successfully.
    /// For example, ordinary base-10 fractional text read as an int truncates toward zero.
    /// Storage width alone does not establish these bounds.
    /// Unknown out-of-range behavior remains a conversion gap even when these bounds are known.
    pub accepted_range: GrammarProperty<Box<NumericRange>>,
}

/// Destination representation established from conversion and stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NumericRepresentation {
    /// An integer, including scaled fixed-point storage.
    Integer,
    /// An IEEE 754 binary floating-point value.
    BinaryFloat,
}

/// Sign interpretation established by the conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NumericSignedness {
    /// Negative and nonnegative values.
    Signed,
    /// Nonnegative values.
    Unsigned,
}

/// A literal form, independent of range, overflow and rounding rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NumericLiteralSyntax {
    /// Decimal integer notation.
    DecimalInteger,
    /// Integer notation with an inferred radix prefix.
    RadixPrefixedInteger,
    /// Decimal fractional notation.
    DecimalFraction,
    /// Floating-point exponent notation.
    Exponent,
}

/// Exact bounds; missing endpoints are unknown, not unbounded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NumericRange {
    /// Inclusive lower endpoint.
    pub minimum: GrammarProperty<NumericBound>,
    /// Inclusive upper endpoint.
    pub maximum: GrammarProperty<NumericBound>,
}

/// An exact endpoint, serialized without conversion through floating point.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NumericBound {
    /// A signed whole number.
    Signed(i64),
    /// An unsigned whole number.
    Unsigned(u64),
    /// A signed numerator divided by a nonzero denominator.
    Rational {
        /// Signed numerator.
        numerator: i64,
        /// Nonzero denominator.
        denominator: u64,
    },
}
