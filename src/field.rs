//! Field facts keep parser behavior apart from selection of a stored value at use time.
use crate::Reader;
use serde::{Deserialize, Serialize};

/// The representation stored by a field reader, not an allowed occurrence count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldShape {
    /// The form of one input value.
    pub value: ValueShape,
    /// How successive successfully read occurrences affect storage.
    pub repeat: RepeatBehavior,
}

/// Input form established by a reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ValueShape {
    /// One scalar value.
    Scalar,
    /// A block of child entries.
    Block,
    /// The accepted input form is not established.
    Unknown,
}

/// Storage behavior when a field occurs again. This never gives a minimum or maximum count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum RepeatBehavior {
    /// A successful read replaces the previous stored value.
    Replace,
    /// Each occurrence adds an entry to a collection.
    Accumulate,
    /// Repeat behavior is not established; block readers may mix replacement and retention.
    Unknown,
}

/// A condition and its outcome from one path through the field's loader.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldReadAlternative {
    /// The state required by this alternative.
    pub condition: FieldCondition,
    /// What this path establishes.
    pub outcome: FieldReadOutcome,
}

/// A condition on stored field values. Paths are relative to this registry's definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum FieldCondition {
    /// All paths in the stated analysis reach this outcome without a state restriction.
    Always,
    /// Every term must hold. Terms can include unresolved context.
    All(Vec<FieldCondition>),
    /// The stored scalar is zero or nonzero. For a Boolean, zero means false.
    FieldZero {
        /// Field path, including enclosing block fields.
        path: Vec<String>,
        /// Whether this alternative requires zero.
        zero: bool,
    },
    /// The required state could not be expressed as a field condition.
    Unresolved,
}

/// A loader outcome; an unresolved path is never a successful unconditional read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum FieldReadOutcome {
    /// A proven reader call with its value and repeat behavior.
    Read {
        /// The reader on this path, independently of other alternatives.
        reader: Reader,
        /// Facts established on this path.
        shape: FieldShape,
    },
    /// The engine rejects this field on this path.
    Rejected,
    /// The analysis could not establish the outcome on this path.
    Unresolved,
}

/// A local selection of stored field data outside its member loader.
///
/// This is a possible use under the stated unresolved context, not a complete runtime rule.
/// The method identity lets a later naming or behavior analysis join the same engine method.
/// An empty list does not establish that the stored field is unused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldUse {
    /// Opaque identity of the engine method within this build. Independent selections
    /// in the same method share this ID; equality does not identify one selection.
    pub id: FieldUseId,
    /// Context in which the stored field participates in this use. Unresolved terms are retained.
    pub condition: FieldCondition,
}

/// Opaque identity of the engine method containing a use, within one build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldUseId(pub(crate) String);

/// Nested fields reached by an established block reader.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum FieldMembers {
    /// The reader takes a scalar value.
    None,
    /// Named child fields, with any remaining limitations in the answer's gaps.
    Fields(Vec<crate::Field>),
    /// The child fields have not been established.
    Unresolved,
}

/// A scalar default established independently of input acceptance and occurrence rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum FieldDefault {
    /// The omitted-field result is not established.
    Unknown,
}

/// An exhaustive set of accepted spellings, independently of fallback or recovery behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum FieldDomain {
    /// The accepted domain is not established.
    Unknown,
}
