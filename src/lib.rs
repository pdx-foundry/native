//! A standard API to ask Stellaris questions, the same on each platform and game build.
//!
//! [`Native`] pins an installation and answers static questions from the executable.
//! [`Game`] is a supervised game session that answers live questions. Every answer is an
//! [`Answer`] with a completeness statement, typed gaps and a source stamp.
#![deny(unsafe_code)]
#![warn(missing_docs)]

mod answer;
mod api;
mod binding;
mod engine;
mod execution;
mod field;
mod fixture;
mod game;
mod grammar;
mod protocol;
mod recorded;
mod session;
mod work_directory;

pub mod supervisor;

pub use answer::{
    Answer, Basis, BlockFamily, BuildId, Completeness, ContextScopes, Declaration, DeclarationKind,
    DeclaredScopes, DeclaredTags, Define, DefineValueType, Disposal, EntryContext, EntryScope,
    Error, Field, GameRule, Gap, GapKind, GapSubject, GeneratedName, GenerationCondition, LinkData,
    LoadedContent, LoadedModifier, LoadedModifiers, LocalizationCommand, LocalizationContext,
    LocalizationContextId, LocalizationContextReference, LocalizationDeclarations,
    LocalizationLink, LocalizationOutput, ModifierCategory, ModifierDeclaration, ModifierFamily,
    NamePart, OnAction, Operation, OutputScope, Reader, ReaderId, ReaderKind, Registry, RuleKind,
    ScopeDeclaration, ScopeGroup, ScopeId, ScopeInventory, ScopeLink, ScopeReference, Source,
    Support,
};
pub use api::OpenError;
pub use engine::operations::registry_items::GameReadiness;
pub use field::{
    FieldCondition, FieldDefault, FieldDomain, FieldMembers, FieldReadAlternative,
    FieldReadOutcome, FieldShape, FieldUse, FieldUseId, RepeatBehavior, ValueShape,
};
pub use fixture::{
    DiagnosticCoverage, DiagnosticJoin, DiagnosticWindow, FieldRead, FixtureDiagnostic,
    FixtureFieldOutcome, FixtureFieldQuestion, FixtureObservation, FixtureObservationKind,
    FixtureOwnerId, FixtureParsing, FixtureRequest, FixtureRuntime, FixtureStorage, FixtureWindow,
    ParsedFieldOccurrence, ProcessingStage, RegistrationEntry, StoredStringOccurrence,
};
pub use game::{Game, GameOptions};
pub use grammar::{
    ChildOrderCondition, ChildOrderOutcome, ChildOrderRule, CommandGrammar, GrammarProperty,
};
pub use session::Native;

pub(crate) use api::UnavailableReason;
pub(crate) use engine::analysis::AnalysisError;

/// Live fault controls for Native's own integration tests, and the executable inspector, the
/// registry field and command grammar stops, and cause tracing for Native's developers. Not a
/// consumer API.
#[doc(hidden)]
pub mod internals {
    pub use crate::binding::inspect;
    pub use crate::engine::analysis::evaluate::trace_causes;
    pub use crate::protocol::session::{ObservationControl, ObservationTarget};
    pub use crate::session::{command_grammar_stops, registry_field_stops};
}
