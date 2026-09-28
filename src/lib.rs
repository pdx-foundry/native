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
mod dynamic_name;
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
pub use dynamic_name::{
    CommandReference, DynamicNameForm, DynamicNameKind, DynamicNamespace, DynamicNamespaceId,
    NamespaceOwner,
};
pub use engine::operations::registry_items::GameReadiness;
pub use field::{
    EmptyKey, FieldCondition, FieldDefault, FieldDomain, FieldMembers, FieldReadAlternative,
    FieldReadOutcome, FieldReference, FieldShape, FieldUse, FieldUseId, KeyMatch, LookupStage,
    MissingResult, ReferenceLookup, ReferenceTarget, RepeatBehavior, ValueShape,
};
pub use fixture::{
    DiagnosticCoverage, DiagnosticJoin, DiagnosticWindow, FieldRead, FixtureDiagnostic,
    FixtureFieldOutcome, FixtureFieldQuestion, FixtureObservation, FixtureObservationKind,
    FixtureOwnerId, FixtureParsing, FixtureRequest, FixtureRuntime, FixtureStorage, FixtureWindow,
    ParsedFieldOccurrence, ProcessingStage, RegistrationEntry, StoredStringOccurrence,
};
pub use game::{Game, GameOptions};
pub use grammar::{
    ArgumentPath, ChildOrderCondition, ChildOrderOutcome, ChildOrderRule, CommandForm,
    CommandGrammar, CommandValue, GrammarProperty, TargetArgument, TargetCheckStage,
};
pub use session::Native;

pub(crate) use api::UnavailableReason;
pub(crate) use engine::analysis::AnalysisError;

/// Live fault controls for Native's own integration tests, and the executable inspector, the
/// registry field and command grammar stops, method stamps for parity tests, and cause tracing
/// for Native's developers. Not a consumer API.
#[doc(hidden)]
pub mod internals {
    pub use crate::binding::inspect;
    pub use crate::engine::analysis::defines::METHOD as DEFINES_METHOD;
    pub use crate::engine::analysis::dynamic_names::METHOD as DYNAMIC_NAMES_METHOD;
    pub use crate::engine::analysis::evaluate::trace_causes;
    pub use crate::engine::analysis::grammar::METHOD as COMMAND_GRAMMAR_METHOD;
    pub use crate::protocol::session::{ObservationControl, ObservationTarget};
    pub use crate::session::{
        command_grammar_stops, dynamic_name_commands, reference_readers, registry_field_stops,
        target_getters,
    };
}
