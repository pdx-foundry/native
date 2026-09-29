//! Static operand rules for scoped numeric destinations.
use serde::{Deserialize, Serialize};

use crate::GrammarProperty;

/// Operand forms and selection rules established for one scoped numeric destination.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopedOperand {
    /// Recognized routing forms, not a promise that a lookup succeeds. Partial forms leave
    /// qualified scope and parameter grammar unresolved.
    pub forms: GrammarProperty<Vec<ScopedOperandForm>>,
    /// A successful literal conversion retains prior source location and stored references.
    /// Malformed literals and later reference writes are outside this property.
    pub literal_assignment_preserves_reference_state: GrammarProperty<bool>,
    /// Selection among stored representations; this says nothing about evaluation success.
    pub selection: GrammarProperty<ScopedOperandSelection>,
}

/// One recognized operand form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ScopedOperandForm {
    /// A value read through the destination's numeric token conversion.
    Literal,
    /// A reference routed by an engine prefix; lookup success is separate.
    Prefixed {
        /// Reference kind stored by this prefix.
        kind: ScopedReferenceKind,
        /// Exact prefix, including its separator.
        prefix: String,
    },
    /// An unprefixed variable name.
    Variable,
}

/// The stored representation chosen by the engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopedOperandSelection {
    /// Condition for selecting the literal storage.
    pub literal_condition: ScopedLiteralCondition,
    /// Priority among stored references on the reachable dynamic path.
    pub reference_priority: Vec<ScopedReferenceKind>,
}

/// Condition under which the literal representation is selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ScopedLiteralCondition {
    /// The source-location string has zero length.
    EmptySourceLocation,
}

/// Stored reference slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ScopedReferenceKind {
    /// Trigger or scripted trigger reference.
    Trigger,
    /// Script value reference.
    ScriptValue,
    /// Modifier reference.
    Modifier,
    /// Variable reference.
    Variable,
}
