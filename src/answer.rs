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

impl Completeness {
    /// Complete exactly when every gap is a declared limit outside the method.
    pub(crate) fn from_gaps(gaps: &[Gap]) -> Self {
        if gaps.iter().all(|gap| gap.kind == GapKind::OutsideMethod) {
            Self::Complete
        } else {
            Self::Partial
        }
    }
}

/// One missing part of an answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gap {
    /// Machine-readable reason.
    pub kind: GapKind,
    /// The public subject that the gap concerns, when one can be identified.
    pub subject: Option<GapSubject>,
    /// Human-readable detail. It holds no address, symbol, or other native detail.
    pub detail: String,
}

/// What the name of a gap identifies. Context and scope identities remain stable when names collide.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GapSubject {
    /// A content directory.
    Registry {
        /// Content directory.
        name: String,
    },
    /// A field of the registry in the question.
    Field {
        /// Field name.
        name: String,
    },
    /// A named item of the question's answer.
    AnswerItem {
        /// Item name in this answer.
        name: String,
    },
    /// A localization context, identified independently of its display name.
    LocalizationContext {
        /// Stable context identity.
        id: LocalizationContextId,
        /// Display name, which may be empty when unreadable.
        name: String,
    },
    /// A localization link.
    LocalizationLink {
        /// Link name.
        name: String,
    },
    /// A scope type, identified independently of its display name.
    ScopeType {
        /// Stable scope identity.
        id: ScopeId,
        /// Scope name.
        name: String,
    },
    /// A fixture file in the current observation.
    FixtureFile {
        /// Fixture file path.
        name: String,
    },
}

impl GapSubject {
    /// Human-readable name of the subject.
    pub fn name(&self) -> &str {
        match self {
            Self::Registry { name }
            | Self::Field { name }
            | Self::AnswerItem { name }
            | Self::LocalizationContext { name, .. }
            | Self::LocalizationLink { name }
            | Self::ScopeType { name, .. }
            | Self::FixtureFile { name } => name,
        }
    }

    pub(crate) fn registry(name: impl Into<String>) -> Self {
        Self::Registry { name: name.into() }
    }

    pub(crate) fn field(name: impl Into<String>) -> Self {
        Self::Field { name: name.into() }
    }

    pub(crate) fn answer_item(name: impl Into<String>) -> Self {
        Self::AnswerItem { name: name.into() }
    }

    pub(crate) fn fixture_file(name: impl Into<String>) -> Self {
        Self::FixtureFile { name: name.into() }
    }
}

#[cfg(test)]
mod gap_subject_tests {
    use super::*;

    #[test]
    fn recorded_gap_requires_a_subject_kind() {
        let old = r#"{"kind":"UnresolvedPath","subject":"Planet","detail":"example"}"#;
        assert!(serde_json::from_str::<Gap>(old).is_err());

        let gap = Gap {
            kind: GapKind::UnresolvedPath,
            subject: Some(GapSubject::LocalizationLink {
                name: "Planet".into(),
            }),
            detail: "example".into(),
        };
        let encoded = serde_json::to_value(&gap).unwrap();
        assert_eq!(encoded["subject"]["kind"], "localization_link");
        assert_eq!(serde_json::from_value::<Gap>(encoded).unwrap(), gap);
    }
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
    /// A storage shape or nested reader is not established.
    UnresolvedStorage,
    /// A loader or use-time condition is not fully expressed.
    UnresolvedCondition,
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
///
/// [`Operation::name`] gives each operation's stable snake_case name, which `Display` also writes.
/// [`Operation::ALL`] lists every operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Operation {
    /// Whether this build can return define names and engine read types.
    Defines,
    /// `Native::registries`
    Registries,
    /// `Native::registry_fields`
    RegistryFields,
    /// `Native::declarations`
    Declarations,
    /// `Native::command_grammar`
    CommandGrammar,
    /// `Native::modifiers`
    Modifiers,
    /// `Native::modifier_categories`
    ModifierCategories,
    /// `Native::modifier_families`
    ModifierFamilies,
    /// `Native::scopes`
    Scopes,
    /// `Native::scope_links`
    ScopeLinks,
    /// `Native::localization_declarations`
    LocalizationDeclarations,
    /// `Native::on_actions`
    OnActions,
    /// `Native::game_rules`
    GameRules,
    /// `Game::registry_items`
    RegistryItems,
    /// `Game::observe_fixture`
    ObserveFixture,
    /// `Game::loaded_modifiers`
    LoadedModifiers,
}

/// One define whose name and value type the executable reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Define {
    /// The engine namespace, such as `NGameplay`.
    pub namespace: String,
    /// The name within the namespace.
    pub name: String,
    /// The form requested by the engine reader.
    pub value_type: DefineValueType,
}

/// Broad value form requested by a define read site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DefineValueType {
    /// A boolean value.
    Boolean,
    /// A signed or unsigned integer value.
    Integer,
    /// A fixed-point value.
    FixedPoint,
    /// A floating-point value.
    Float,
    /// A string value.
    String,
    /// A fixed-size vector.
    Vector,
    /// A variable-length array.
    List,
    /// A color value.
    Color,
    /// A game date.
    Date,
}

impl Operation {
    /// Every operation once, in declaration order.
    pub const ALL: &'static [Operation] = &[
        Self::Defines,
        Self::Registries,
        Self::RegistryFields,
        Self::Declarations,
        Self::CommandGrammar,
        Self::Modifiers,
        Self::ModifierCategories,
        Self::ModifierFamilies,
        Self::Scopes,
        Self::ScopeLinks,
        Self::LocalizationDeclarations,
        Self::OnActions,
        Self::GameRules,
        Self::RegistryItems,
        Self::ObserveFixture,
        Self::LoadedModifiers,
    ];

    /// The operation's stable snake_case name, such as `registry_fields`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Defines => "defines",
            Self::Registries => "registries",
            Self::RegistryFields => "registry_fields",
            Self::Declarations => "declarations",
            Self::CommandGrammar => "command_grammar",
            Self::Modifiers => "modifiers",
            Self::ModifierCategories => "modifier_categories",
            Self::ModifierFamilies => "modifier_families",
            Self::Scopes => "scopes",
            Self::ScopeLinks => "scope_links",
            Self::LocalizationDeclarations => "localization_declarations",
            Self::OnActions => "on_actions",
            Self::GameRules => "game_rules",
            Self::RegistryItems => "registry_items",
            Self::ObserveFixture => "observe_fixture",
            Self::LoadedModifiers => "loaded_modifiers",
        }
    }

    /// Whether the operation reads engine declarations, which need a declaration recipe.
    pub(crate) fn is_declaration(self) -> bool {
        matches!(
            self,
            Self::Defines
                | Self::Declarations
                | Self::CommandGrammar
                | Self::Modifiers
                | Self::ModifierCategories
                | Self::ModifierFamilies
                | Self::Scopes
                | Self::ScopeLinks
                | Self::LocalizationDeclarations
                | Self::OnActions
                | Self::GameRules
        )
    }
}

impl std::fmt::Display for Operation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod operation_tests {
    use super::*;
    use std::collections::HashSet;

    /// The number of variants. A new variant fails to compile here until it is counted.
    fn variant_count(operation: Operation) -> usize {
        match operation {
            Operation::Defines
            | Operation::Registries
            | Operation::RegistryFields
            | Operation::Declarations
            | Operation::CommandGrammar
            | Operation::Modifiers
            | Operation::ModifierCategories
            | Operation::ModifierFamilies
            | Operation::Scopes
            | Operation::ScopeLinks
            | Operation::LocalizationDeclarations
            | Operation::OnActions
            | Operation::GameRules
            | Operation::RegistryItems
            | Operation::ObserveFixture
            | Operation::LoadedModifiers => 16,
        }
    }

    #[test]
    fn all_lists_every_operation_once_with_a_unique_name() {
        assert_eq!(Operation::ALL.len(), variant_count(Operation::Defines));
        // Distinct names also mean distinct operations, so the list holds each variant once.
        let names: HashSet<&str> = Operation::ALL.iter().map(|op| op.name()).collect();
        assert_eq!(names.len(), Operation::ALL.len());
        assert_eq!(Operation::RegistryFields.to_string(), "registry_fields");
    }

    #[test]
    fn serde_form_is_the_variant_name() {
        assert_eq!(
            serde_json::to_value(Operation::RegistryFields).unwrap(),
            "RegistryFields"
        );
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
    /// No registration with this name was found in the selected command inventory.
    UnknownCommand {
        /// The inventory asked for.
        kind: DeclarationKind,
        /// The command name asked for.
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
            Self::UnknownCommand { kind, name } => write!(f, "no {kind:?} command is named {name}"),
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
    /// Value form and repeat behavior agreed by all established read alternatives.
    pub shape: crate::FieldShape,
    /// Loader conditions paired with read, rejected, or unresolved outcomes. Never empty.
    pub read: Vec<crate::FieldReadAlternative>,
    /// Child fields, or an explicit unresolved block boundary.
    pub members: crate::FieldMembers,
    /// Exhaustive accepted spellings, or an explicit unknown.
    pub domain: crate::FieldDomain,
    /// Behavior on omission, established separately from parser reads.
    pub default: crate::FieldDefault,
    /// Use-time selections that the method reached. An empty list does not prove no conditions.
    pub uses: Vec<crate::FieldUse>,
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
}

/// A declared scope set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeclaredScopes {
    /// Every scope is supported.
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

/// Modifiers that the engine generates for each item of one registry, such as one build-speed
/// modifier for each building.
///
/// # Example
///
/// ```
/// # use pdx_native::{DeclaredTags, GenerationCondition, ModifierFamily, NamePart};
/// let family = ModifierFamily {
///     name: vec![
///         NamePart::Literal("planet_".into()),
///         NamePart::ItemKey,
///         NamePart::Literal("_build_speed_mult".into()),
///     ],
///     category_tags: DeclaredTags::Listed(vec!["Colony".into()]),
///     condition: GenerationCondition::Always,
///     name_limit: None,
/// };
/// assert_eq!(
///     family.name_for("building_foundry").as_deref(),
///     Some("planet_building_foundry_build_speed_mult"),
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModifierFamily {
    /// The parts of each generated name, in order. At least one part is the item key.
    pub name: Vec<NamePart>,
    /// The intended-use category tags of each generated modifier, by the rule of
    /// `Native::modifiers`. They are the tags of this registration; a later registration of the
    /// same name can change the loaded tags.
    pub category_tags: DeclaredTags,
    /// Whether every item of the registry generates the family.
    pub condition: GenerationCondition,
    /// The longest name in bytes that the engine keeps, when a fixed-size buffer builds the name.
    /// The name for a key that would be longer is not established.
    pub name_limit: Option<usize>,
}

impl ModifierFamily {
    /// The modifier name that the family gives the item named `key`, or `None` when the name
    /// would be longer than `name_limit`.
    pub fn name_for(&self, key: &str) -> Option<String> {
        let name: String = self
            .name
            .iter()
            .map(|part| match part {
                NamePart::Literal(text) => text.as_str(),
                NamePart::ItemKey => key,
            })
            .collect();
        match self.name_limit {
            Some(limit) if name.len() > limit => None,
            _ => Some(name),
        }
    }
}

/// One part of a generated modifier name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum NamePart {
    /// This text.
    Literal(String),
    /// The key of the item, such as `building_foundry`.
    ItemKey,
}

/// Whether every item of a registry generates a modifier family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum GenerationCondition {
    /// Every item generates it.
    Always,
    /// Some items may not generate it, such as when the engine checks an item field first, or the
    /// method could not follow every path. A gap names the family.
    Unresolved,
}

/// The modifiers that the engine holds after all content has loaded, with the content that the
/// game loaded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadedModifiers {
    /// Every loaded modifier, in the engine's table order.
    pub modifiers: Vec<LoadedModifier>,
    /// The item keys of each registry whose families were applied, as the engine holds them at
    /// the same point, by content directory. A registry whose keys could not be read is absent
    /// and has a gap.
    pub registry_items: std::collections::BTreeMap<String, Vec<String>>,
    /// The content that the game loaded before the engine documented its modifiers.
    pub content: LoadedContent,
}

/// One modifier in the loaded table.
///
/// A modifier that the executable does not declare and that no family explains has
/// `declared == false` and an empty `generated_by`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadedModifier {
    /// The modifier's script name.
    pub name: String,
    /// The intended-use category tags of the loaded modifier, by the rule of
    /// `Native::modifiers`. Content that registers a declared name again can change them.
    pub category_tags: DeclaredTags,
    /// Whether the executable declares this name: it is in `Native::modifiers`.
    pub declared: bool,
    /// Each family and loaded item whose generated name is this name. A family explains a name
    /// only when [`ModifierFamily::name_for`] gives it for a key of the family's registry that
    /// the engine holds; names are never matched by similarity.
    pub generated_by: Vec<GeneratedName>,
}

/// A modifier name that a family of `Native::modifier_families` gives for one loaded item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedName {
    /// The family's registry, such as `common/buildings`.
    pub registry: String,
    /// The key of the item, such as `building_foundry`.
    pub item: String,
    /// The family's name template, as in [`ModifierFamily::name`].
    pub template: Vec<NamePart>,
}

/// The content that a game session loaded.
///
/// Native's sessions enable no user mod and disable no DLC. Their profile mounts one private
/// mod that holds exact copies of the installed files of each selected registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum LoadedContent {
    /// The installation's own content and its installed DLC.
    Installation,
    /// The installation's content, with one registry's directory replaced by the prepared
    /// fixture's files.
    Fixture {
        /// The replaced content directory, such as `common/tradition_categories`.
        registry: String,
        /// The fixture's files, by path below the content root.
        files: Vec<String>,
    },
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

/// An on_action that the engine fires by name, with the scopes that its call sites supply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnAction {
    /// The on_action name.
    pub name: String,
    /// Each distinct context that a followed call site supplies. Empty when no call site of the
    /// name could be followed; a gap with this name as its subject then says why.
    pub entries: Vec<EntryContext>,
}

/// A game rule that the engine evaluates, with the scopes that its call sites supply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameRule {
    /// The rule name.
    pub name: String,
    /// Whether the rule evaluates a trigger or computes a weight.
    pub kind: RuleKind,
    /// Each distinct context that a followed call site supplies. Empty when no call site of the
    /// rule could be followed; a gap with this name as its subject then says why.
    pub entries: Vec<EntryContext>,
}

/// The kind of a game rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleKind {
    /// The rule is a trigger that allows or refuses something.
    Scripted,
    /// The rule computes a weight.
    Weighted,
}

/// The scopes that one or more call sites supply when the engine enters a callback. Each
/// context is one alternative; the engine does not merge them.
///
/// Join each [`ScopeReference`] to `Native::scopes` by `id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryContext {
    /// The scope that `this` is.
    pub this: EntryScope,
    /// The scope that `root` links to.
    pub root: EntryScope,
    /// `from`, `fromfrom` and so on, in order. The chain ends after the first entry that is not
    /// [`EntryScope::Scope`].
    pub from: Vec<EntryScope>,
}

/// One scope that a call site supplies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum EntryScope {
    /// A scope of this type.
    Scope(ScopeReference),
    /// A scope object with no scope type.
    NotSet,
    /// The link points back to the scope that holds it. This is the engine's default link; what
    /// script sees through it is outside this answer.
    SelfLink,
    /// The scope could not be established.
    Unresolved,
}

/// The shared reader behind a field. Two fields with one reader report the same `id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reader {
    /// Opaque reader identity within one build, or `None` when no reader is established.
    pub id: Option<ReaderId>,
    /// Value form that the reader accepts.
    pub kind: ReaderKind,
    /// Command family accepted by a block reader, independently of its full grammar.
    #[serde(default)]
    pub family: BlockFamily,
}

/// The child command family established for a reader.
#[derive(
    Debug,
    Default,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[non_exhaustive]
pub enum BlockFamily {
    /// Trigger commands.
    Trigger,
    /// Effect commands.
    Effect,
    /// Modifier entries; their detailed grammar is not established.
    Modifier,
    /// The family is not established, including conflicting or missing alternatives.
    #[default]
    Unknown,
    /// An established scalar reader has no child command family.
    NotApplicable,
}

/// Opaque identity of a shared reader within one build.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReaderId(pub(crate) String);

/// Broad value form accepted by a shared reader.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
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
