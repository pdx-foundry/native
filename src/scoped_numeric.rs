//! Static operand rules for scoped numeric destinations.
use serde::{Deserialize, Serialize};

use crate::GrammarProperty;

/// Operand forms established for one scoped numeric destination.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopedOperand {
    /// Recognized routing forms, not a promise that a lookup succeeds. Partial forms leave
    /// qualified scope and parameter grammar unresolved.
    pub forms: GrammarProperty<Vec<ScopedOperandForm>>,
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
