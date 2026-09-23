//! The one result type, its source stamp, its typed gaps, and the one error type.
use serde::{Deserialize, Serialize};

/// An answer to one engine question.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Answer<T> {
    /// What was established. A partial answer keeps every established part.
    pub value: T,
    /// Whether the stated search or window completed. Never complete game knowledge.
    pub completeness: Completeness,
    /// Missing answers within the method and declared limits outside it. A complete search may
    /// still name limits outside its boundary.
    pub gaps: Vec<Gap>,
    /// Which build and method gave this answer.
    pub source: Source,
}

/// Whether the method's stated search completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Completeness {
    /// The stated search or window completed. An empty value then means that nothing was found
    /// within that boundary.
    Complete,
    /// Part of the search did not complete; `gaps` says which part.
    Partial,
}

/// One missing part of an answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gap {
    /// Machine-readable reason.
    pub kind: GapKind,
    /// The registry, field, or other public name that the gap concerns, when one exists.
    pub subject: Option<String>,
    /// Human-readable detail. It holds no address, symbol, or other native detail.
    pub detail: String,
}

/// Why a part of an answer is missing. A gap never means that the game forbids a construct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GapKind {
    /// Registries were found whose content directory is not established.
    UnnamedRegistries,
    /// The method does not cover some loaders, readers, or paths.
    OutsideMethod,
    /// The method could not read a required part of the executable.
    UnreadableInput,
    /// A field exists on a path whose name could not be recovered.
    UnnamedField,
    /// A registration site's name is composed at run time and absent from the executable's
    /// literal token table.
    UnnamedDeclaration,
    /// A path through the reader could not be followed to its end.
    UnresolvedPath,
    /// A field's reader could not be established.
    UnresolvedReader,
    /// The reader is identified, but its accepted values and behavior are not established.
    ReaderSemantics,
    /// A live observation window did not complete; the established part is kept.
    IncompleteObservation,
}

/// How an answer was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Basis {
    /// The engine declares it. Behavior is not demonstrated.
    Declared,
    /// Traced in the executable without a game.
    StaticAnalysis,
    /// Observed in a supervised game.
    LiveObservation,
    /// Read from recorded answers. Never evidence of the present installation.
    Recorded,
}

/// Opaque identity of the exact game build. Keep it and compare it; do not parse it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BuildId(pub(crate) String);

/// The stamp on every answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    /// Exact game build.
    pub build: BuildId,
    /// Native release that ran the method.
    pub native_version: String,
    /// Method name and revision.
    pub method: String,
    /// How the answer was obtained.
    pub basis: Basis,
}

impl Source {
    pub(crate) fn new(build: BuildId, method: &str, basis: Basis) -> Self {
        Self {
            build,
            native_version: env!("CARGO_PKG_VERSION").into(),
            method: method.into(),
            basis,
        }
    }
}

/// A question that Native can be asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Operation {
    /// `Native::registries`
    Registries,
    /// `Native::registry_fields`
    RegistryFields,
    /// `Native::declarations`
    Declarations,
    /// `Native::modifiers`
    Modifiers,
    /// `Native::modifier_categories`
    ModifierCategories,
    /// `Native::scopes`
    Scopes,
    /// `Native::scope_links`
    ScopeLinks,
    /// `Native::localization_declarations`
    LocalizationDeclarations,
    /// `Game::registry_items`
    RegistryItems,
    /// `Game::observe_fixture`
    ObserveFixture,
}

impl Operation {
    /// Whether the operation reads engine declarations, which need a declaration recipe.
    pub(crate) fn is_declaration(self) -> bool {
        matches!(
            self,
            Self::Declarations
                | Self::Modifiers
                | Self::ModifierCategories
                | Self::Scopes
                | Self::ScopeLinks
                | Self::LocalizationDeclarations
        )
    }
}

/// Whether the game process that a session owned is gone. Only the independent supervisor can
/// confirm it; a lost connection or an exit code never does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Disposal {
    /// The supervisor reaped the game process that it owned.
    Confirmed,
    /// Disposal was not established for this session. A new game is refused while a conflicting
    /// Stellaris process remains visible to the host reservation check.
    Unconfirmed(String),
    /// No game process was created: the start failed early, or the answers are recorded.
    NotApplicable,
}

/// Whether this build and host can answer an operation. Asking never starts a game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Support {
    /// The operation has an implementation here, and its inputs and tools can be read.
    Supported,
    /// The operation cannot run here, for this reason.
    Unsupported(String),
}

/// Why a question has no answer. Never used for a search that completed and found nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Error {
    /// This build or host has no implementation of the operation.
    Unsupported {
        /// The operation asked for.
        operation: Operation,
        /// Why it is not supported here.
        reason: String,
    },
    /// The executable changed after `open`. Open the installation again.
    BuildChanged,
    /// A fixture request cannot be mounted or observed. Refused before game launch.
    FixtureRequest {
        /// The unsupported input or missing setup.
        reason: String,
    },
    /// No registry with this name was found. `registries` lists the known names.
    UnknownRegistry {
        /// The name asked for.
        name: String,
    },
    /// The method failed on an input that it should handle.
    Method(String),
    /// The game session did not establish this observation. Other observations may be available.
    Observation {
        /// The operation asked for.
        operation: Operation,
        /// What was not established.
        reason: String,
    },
    /// The game did not reach a safe pause. The session is over.
    Startup {
        /// Why the start failed.
        reason: String,
        /// Whether the game process, if one was created, is gone.
        disposal: Disposal,
    },
    /// Final session cleanup failed. The work directory is kept for inspection.
    Cleanup {
        /// What failed during cleanup.
        reason: String,
        /// Whether the supervisor established that the game process is gone.
        disposal: Disposal,
    },
    /// The game session is closing or closed.
    Closed,
    /// Recorded answers hold no file for this question. Never an empty answer.
    NotRecorded {
        /// The question and its subject, such as `registry_items/common/traditions`.
        question: String,
    },
    /// A recorded answer could not be read or written.
    Recorded(String),
    /// The connection to the supervisor failed. Disposal of the game is not established.
    Supervisor(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported { operation, reason } => {
                write!(f, "{operation:?} is not supported: {reason}")
            }
            Self::FixtureRequest { reason } => write!(f, "Invalid fixture request: {reason}"),
            Self::BuildChanged => f.write_str("the executable changed after it was opened"),
            Self::UnknownRegistry { name } => write!(f, "no registry is named {name}"),
            Self::Method(reason) => write!(f, "the method failed: {reason}"),
            Self::Observation { operation, reason } => {
                write!(f, "{operation:?} was not observed: {reason}")
            }
            Self::Startup { reason, disposal } => {
                write!(
                    f,
                    "the game did not start: {reason} (disposal: {disposal:?})"
                )
            }
            Self::Cleanup { reason, disposal } => {
                write!(
                    f,
                    "session cleanup failed: {reason} (disposal: {disposal:?})"
                )
            }
            Self::Closed => f.write_str("the game session is closed"),
            Self::NotRecorded { question } => write!(f, "no answer is recorded for {question}"),
            Self::Recorded(reason) => write!(f, "recorded answer failed: {reason}"),
            Self::Supervisor(reason) => write!(f, "the supervisor connection failed: {reason}"),
        }
    }
}
impl std::error::Error for Error {}

/// One engine registry, named by its content directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registry {
    /// Content directory relative to the game root, such as `common/traditions`.
    pub name: String,
}

/// One root field of a registry's definitions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    /// Field key as the engine spells it.
    pub name: String,
    /// The shared reader that handles the field's value.
    pub reader: Reader,
    /// The engine reads this field differently depending on state that the field key does not
    /// determine.
    pub conditional: bool,
}

/// A command kind whose declarations can be read from the executable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DeclarationKind {
    /// An effect command.
    Effect,
    /// A trigger command.
    Trigger,
}

impl DeclarationKind {
    pub(crate) fn subject(self) -> &'static str {
        match self {
            Self::Effect => "effect",
            Self::Trigger => "trigger",
        }
    }
}

/// The engine's documented command and declared applicability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Declaration {
    /// The name used for registration.
    pub name: String,
    /// The first line of the engine's documentation string.
    pub description: String,
    /// Remaining documentation lines, or empty when there are none.
    pub usage: String,
    /// Scopes the command declares that it supports.
    pub scopes: DeclaredScopes,
    /// Targets the command declares that it supports.
    pub targets: DeclaredScopes,
}

/// A declared scope or target set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeclaredScopes {
    /// Every scope or target is supported.
    Any,
    /// These scope types, including an empty set.
    Listed(Vec<ScopeReference>),
    /// The declaration could not be followed to its scope set.
    Unresolved,
}

/// A built-in modifier that the engine declares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModifierDeclaration {
    /// The modifier's script name.
    pub name: String,
    /// The intended-use category tags that the engine declares for the modifier. They are not
    /// the objects or scopes where the modifier takes effect.
    pub category_tags: DeclaredTags,
}

/// A declared set of category tags.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeclaredTags {
    /// The named tags, in the engine's order.
    Listed(Vec<String>),
    /// The declaration could not be followed to its tags.
    Unresolved,
}

/// A modifier category name that the engine declares. A category is an intended-use tag, not an
/// application context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModifierCategory {
    /// The category name as the engine spells it, such as `Countries`.
    pub name: String,
}

/// Opaque identity of a scope type within one build. Two types can share a display name (two
/// types are named `country`); they never share an identity. Keep it and compare it; do not
/// parse it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ScopeId(pub(crate) String);

/// A reference to one scope type in the answer of `Native::scopes`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeReference {
    /// The scope type. Join references to declarations by this identity.
    pub id: ScopeId,
    /// The type's display name, for reading only; it does not identify the type.
    pub name: String,
}

/// The scope types that the engine declares, and the keywords that match several of them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeInventory {
    /// Each scope type, sorted by name.
    pub types: Vec<ScopeDeclaration>,
    /// Keywords that match any one of several scope types, sorted by keyword.
    pub groups: Vec<ScopeGroup>,
}

/// A keyword that matches any one of several scope types, such as `carrier` (a planet or a ship).
/// It is not a name of each type: the types stay distinct scopes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeGroup {
    /// The script keyword.
    pub keyword: String,
    /// The scope types that the keyword matches, in the engine's order.
    pub scopes: Vec<ScopeReference>,
}

/// A scope type that the engine declares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeDeclaration {
    /// The scope type's identity. Scope references in other answers carry the same identity.
    pub id: ScopeId,
    /// The engine's name for the scope type, as its documentation prints it. It can contain a
    /// space (`pop job`), and two types can share a name (two types are named `country`).
    pub name: String,
    /// Script keywords that the engine resolves to this scope type, sorted. Two keywords in one
    /// list are the same scope to the engine. Empty when no literal keyword resolves to it.
    pub keywords: Vec<String>,
}

/// A scope link: a keyword that changes the current scope, such as `owner`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeLink {
    /// The link keyword. For a link that takes data, the prefix without its colon.
    pub name: String,
    /// The scopes that the link declares it can be used from.
    pub input_scopes: DeclaredScopes,
    /// The scope that the link declares it changes to.
    pub output_scope: OutputScope,
    /// Whether the link takes data after its name.
    pub data: LinkData,
}

/// The declared output scope of a link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputScope {
    /// One of these scope types. Most links list one; `carrier` lists two.
    Listed(Vec<ScopeReference>),
    /// The engine declares that the output depends on the context, such as for `prev`.
    Various,
    /// The declaration could not be followed to its output.
    Unresolved,
}

/// What a link takes after its name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum LinkData {
    /// The link is a complete keyword.
    None,
    /// The link is this prefix followed by a value, such as `event_target:my_target`.
    Prefix(String),
}

/// The localization ("localisation") language that the engine declares: the contexts of bracket
/// commands such as `[Root.GetName]`, the commands and links of each context, and the scope types
/// that select each context.
///
/// Commands and links name their contexts by [`LocalizationContextReference`]. Join a reference
/// to [`LocalizationDeclarations::contexts`] by `id`, and a context's scopes to
/// `Native::scopes` by [`ScopeReference::id`]; never join by name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalizationDeclarations {
    /// Each context, sorted by name.
    pub contexts: Vec<LocalizationContext>,
    /// Each command name, sorted, with the contexts that declare it.
    pub commands: Vec<LocalizationCommand>,
    /// Each link, sorted by name. A name with different outputs has one row per output.
    pub links: Vec<LocalizationLink>,
}

/// Opaque identity of a localization context within one build. Keep it and compare it; do not
/// parse it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct LocalizationContextId(pub(crate) String);

/// A reference to one context in [`LocalizationDeclarations::contexts`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalizationContextReference {
    /// The context. Join references to contexts by this identity.
    pub id: LocalizationContextId,
    /// The context's display name, for reading only.
    pub name: String,
}

/// The kind of object that a localization statement points at, such as a country or a dead
/// fleet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalizationContext {
    /// The context's identity.
    pub id: LocalizationContextId,
    /// The engine's name for the context, such as `Ship (and Starbase)` or `Base Scope`. Empty
    /// when the name could not be read; a gap then names the context's identity.
    pub name: String,
    /// The scope types that select this context.
    pub scopes: ContextScopes,
}

/// Which scope types select a localization context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContextScopes {
    /// Every scope type was followed; these select the context.
    Joined(Vec<ScopeReference>),
    /// Every scope type was followed; none selects the context, such as for a dead object.
    Missing,
    /// Some scope types could not be followed. These select the context; others may too. The
    /// list can be empty.
    Partial(Vec<ScopeReference>),
}

/// A localization command, such as `GetName`: text that the current context gives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalizationCommand {
    /// The command name.
    pub name: String,
    /// The contexts that declare the command, sorted by name.
    pub contexts: Vec<LocalizationContextReference>,
}

/// A localization link, such as `Owner`: a name that changes the current context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalizationLink {
    /// The link name.
    pub name: String,
    /// The contexts that declare the link with this output, sorted by name.
    pub input_contexts: Vec<LocalizationContextReference>,
    /// The context that the link changes to.
    pub output: LocalizationOutput,
}

/// The context that a localization link changes to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocalizationOutput {
    /// One of these contexts, sorted by name. Most links list one.
    Listed(Vec<LocalizationContextReference>),
    /// The engine selects the context from the object at run time, such as for `Root`.
    Various,
    /// Every path through the link returns without changing the context: the engine declares
    /// the link but does not follow it.
    Unchanged,
    /// The link could not be followed to its output, or it changes the context on some paths and
    /// not on others; a gap names it.
    Unresolved,
}

/// The shared reader behind a field. Two fields with one reader report the same `id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reader {
    /// Opaque reader identity within one build, or `None` when no reader is established.
    pub id: Option<ReaderId>,
    /// Value form that the reader accepts.
    pub kind: ReaderKind,
}

/// Opaque identity of a shared reader within one build.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReaderId(pub(crate) String);

/// Broad value form accepted by a shared reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ReaderKind {
    /// A boolean value.
    Boolean,
    /// A signed or unsigned integer value.
    Integer,
    /// A fixed-point numeric value.
    FixedPoint,
    /// A string value.
    String,
    /// A deferred reference key.
    Reference,
    /// A nested trigger, effect, persistent object, or other script block.
    Block,
    /// The value form is not established.
    Unknown,
}
