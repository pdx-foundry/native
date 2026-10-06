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
mod duration;
mod dynamic_name;
mod engine;
mod execution;
mod field;
mod fixture;
mod game;
mod grammar;
mod numeric;
mod protocol;
mod recorded;
mod scoped_numeric;
mod script;
mod session;
mod work_directory;

pub mod supervisor;

pub use answer::{
    AcceptedCategories, Answer, Basis, BlockFamily, BuildId, Completeness, ContextScopes,
    Declaration, DeclarationKind, DeclaredScopes, DeclaredTags, Define, DefineValueType,
    DerivedName, Disposal, EntryContext, EntryScope, Error, Field, GameRule, Gap, GapKind,
    GapSubject, GeneratedName, GenerationCondition, KeptCategories, LinkData, LoadedContent,
    LoadedModifier, LoadedModifiers, LocalizationCommand, LocalizationContext,
    LocalizationContextId, LocalizationContextReference, LocalizationDeclarations,
    LocalizationLink, LocalizationOutput, MissingName, ModifierCategory, ModifierCategoryKey,
    ModifierDeclaration, ModifierFamily, ModifierNode, ModifierNodeId, ModifierNodeOwner,
    NameLookup, NamePart, OnAction, Operation, OutputScope, ReadScope, Reader, ReaderId,
    ReaderKind, Registry, RuleKind, ScopeDeclaration, ScopeGroup, ScopeId, ScopeInventory,
    ScopeLink, ScopeReference, Source, Support,
};
pub use api::OpenError;
pub use duration::{Duration, DurationCombination, DurationUnit};
pub use dynamic_name::{
    CommandReference, DynamicNameForm, DynamicNameKind, DynamicNamespace, DynamicNamespaceId,
    NamespaceOwner,
};
pub(crate) use engine::operations::registry_items::GameReadiness;
pub use field::{
    EmptyKey, FieldCondition, FieldDomain, FieldMembers, FieldReadAlternative, FieldReadOutcome,
    FieldReference, FieldShape, FieldUse, FieldUseId, KeyMatch, LookupStage, MissingResult,
    ModifierBlock, ModifierEntry, ModifierMembers, ReferenceLookup, ReferenceTarget,
    RepeatBehavior, TriggeredModifierBlock, ValueShape, WeightBlock, WeightOperation,
    WeightOtherKeys,
};
pub use fixture::{
    DiagnosticCoverage, DiagnosticJoin, DiagnosticWindow, FixtureDiagnostic, FixtureFieldOutcome,
    FixtureFieldQuestion, FixtureObservation, FixtureOwnerId, FixtureParsing, FixtureRequest,
    FixtureStorage, FixtureValue, FixtureWindow, ParsedFieldOccurrence, ScopedNumericLiteral,
    ScopedNumericStorage, StoredFieldOccurrence,
};
pub use game::{Game, GameOptions};
pub use grammar::{
    ArgumentPath, ChildOrderCondition, ChildOrderOutcome, ChildOrderRule, ChildScope, CommandForm,
    CommandGrammar, CommandValue, GrammarProperty, TargetArgument, TargetCheckStage,
};
pub use numeric::{
    NumericBound, NumericConversion, NumericLiteralSyntax, NumericRange, NumericRepresentation,
    NumericSignedness,
};
pub use scoped_numeric::{ScopedOperand, ScopedOperandForm, ScopedReferenceKind};
pub use script::{
    ForeignScriptDiagnostic, ScriptCheck, ScriptDiagnostic, ScriptObservation, ScriptStage,
    StoredDuration,
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
    pub use crate::engine::analysis::category_keys::METHOD as MODIFIER_CATEGORY_KEYS_METHOD;
    pub use crate::engine::analysis::defines::METHOD as DEFINES_METHOD;
    pub use crate::engine::analysis::dynamic_names::METHOD as DYNAMIC_NAMES_METHOD;
    pub use crate::engine::analysis::evaluate::trace_causes;
    pub use crate::engine::analysis::grammar::METHOD as COMMAND_GRAMMAR_METHOD;
    pub use crate::engine::analysis::modifier_nodes::METHOD as MODIFIER_NODES_METHOD;
    pub use crate::engine::analysis::names::METHOD as DERIVED_NAMES_METHOD;
    pub use crate::protocol::session::{ObservationControl, ObservationTarget};
    pub use crate::session::{
        check_registry_load, command_grammar_stops, duration_groups, dynamic_name_commands,
        numeric_readers, reference_readers, registry_field_stops, target_getters,
    };
}
