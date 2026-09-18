//! Fixed synthetic admission scenarios. These cannot bind installations or execute live strategies.

/// A tracked scenario; callers cannot supply qualification records or native inputs.
#[derive(Debug, Clone, Copy)]
pub enum SyntheticCase {
    /// Matching acceptance, inputs, and in-memory prerequisites; evidence bytes are absent.
    Accepted,
    /// A recipe exists, but no qualification has been accepted.
    RecipeOnly,
    /// The matching acceptance was withdrawn.
    Withdrawn,
    /// The acceptance names a different exact composition revision.
    RevisionMismatch,
    /// The acceptance requires different content bytes.
    ContentMismatch,
    /// The content changed after the context was bound.
    ContentChanged,
    /// The executable changed after the context was bound.
    TargetChanged,
    /// Inputs can no longer be checked.
    InputUnavailable,
    /// Initial content identification failed, so content cannot be compared to the acceptance.
    ContentUnavailable,
    /// Qualification matches, but a current prerequisite is missing.
    MissingPrerequisite,
    /// Acceptance covers fewer registration entries than the recipe's declared limit.
    NarrowQualification,
    /// Separate acceptances each cover one field; they do not qualify their union.
    SplitQualifications,
    /// A withdrawn old acceptance coexists with a new applicable acceptance.
    ReplacementAcceptance,
    /// The compiled host resolves the real candidate strategy without executing it.
    RealStrategy,
}

/// Construct a context with fixed synthetic inputs, then use its ordinary capability interface.
/// No installation path, executable bytes, qualification files, or live execution plan is accepted.
pub fn engine(case: SyntheticCase) -> crate::EngineContext {
    crate::session::EngineContext::from_binding(crate::binding::synthetic(case))
}
