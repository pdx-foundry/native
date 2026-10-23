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
    /// Either one scalar value or a block.
    Any,
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
    /// A repeat updates some stored parts and keeps the others, whether or not it is diagnosed. A
    /// repeated weight block replaces `base` and keeps earlier entries.
    Merges,
    /// Repeat behavior is not established.
    Unknown,
}

/// Whether a definition must contain a root field, or a member of a root statement such as an
/// event kind: what the engine reports when a definition omits it while the game loads. This gives
/// no occurrence count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum FieldRequirement {
    /// A definition without the field logs a report of its absence while the game loads.
    Required,
    /// A definition without the field logs no report of its absence while the game loads. A
    /// check that runs only when another field has a value can still report it.
    Optional,
    /// The requirement depends on the definition's other fields. Each case holds where its
    /// condition holds; a definition that meets no case has no established requirement.
    Cases(Vec<RequirementCase>),
    /// Not established: the check depends on another field's value that no case states, the
    /// registry was not observed, or the field is below a definition's fields.
    Unknown,
}

/// A condition on a definition's other stored fields, and the requirement where it holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequirementCase {
    /// The stored state of the definition's other fields that this case covers.
    pub condition: FieldCondition,
    /// Whether a definition that meets the condition must contain the field.
    pub required: bool,
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
    /// The stored scalar is zero or nonzero. For a Boolean, zero means false. A string field is
    /// zero when its text is empty.
    FieldZero {
        /// Field path, including enclosing block fields.
        path: Vec<String>,
        /// Whether this alternative requires zero.
        zero: bool,
    },
    /// The stored keyword field holds, or does not hold, one keyword. The condition is on the
    /// stored value, so a definition without the field meets it by the field's default.
    FieldKeyword {
        /// Field path, including enclosing block fields.
        path: Vec<String>,
        /// The keyword as a definition writes it.
        keyword: String,
        /// Whether the condition requires that keyword; `false` requires any other value.
        equal: bool,
    },
    /// The field's own value, as written, is a block (`block: true`) or any other value
    /// (`block: false`). A field whose reader tests the value's token has one alternative for each
    /// form, and its `members` describe the block's.
    ValueForm {
        /// Whether this alternative requires a block value.
        block: bool,
    },
    /// The required state could not be expressed as a field condition.
    Unresolved,
}

/// A loader outcome; an unresolved path is never a successful unconditional read.
// Keep the public Reader inline so recorded answers and field alternatives share its shape.
#[allow(clippy::large_enum_variant)]
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
    /// Fixed keys and entry forms of a shared modifier-block reader.
    ModifierBlock(ModifierBlock),
    /// Keys, arithmetic operations and conditions of a shared weight or script-value reader.
    WeightBlock(Box<WeightBlock>),
    /// Keys of a modifier block applied while a trigger holds, such as `triggered_modifier`.
    TriggeredModifier(Box<TriggeredModifierBlock>),
    /// Any key, each read the same way, such as `description_parameters = { NAME = 7 }` or the
    /// cultures of a name list's `character_names`. No list limits the keys.
    AnyKey {
        /// What the engine reads under each key: the value's reader, shape, repeat behavior and
        /// members, as for a named field. Its `name` is empty, and a condition path names it
        /// with an empty segment. A repeated key is read again under the entry's repeat behavior.
        entry: Box<crate::Field>,
        /// The lookups that the engine makes with each key. `NotEstablished` when none is
        /// established; the key then only names its entry.
        key_reference: FieldReference,
        /// The written forms of a scalar value that the engine keeps; `None` when the entry's
        /// value is a block. The engine reports any other value, such as a block or another
        /// keyword, as malformed and keeps the entry with an empty value. A scripted variable or
        /// inline math value is replaced before this check; this list does not cover it.
        value_forms: Option<crate::GrammarProperty<Vec<ScalarForm>>>,
    },
    /// A block of bare values, such as `tags = { a b }`, each read by `reader`. For a list of
    /// keys, each key is looked up as the field's `reference` states.
    Items {
        /// The reader of one value of the list.
        reader: Reader,
    },
    /// The child fields have not been established.
    Unresolved,
}

/// A written form of one scalar value, as the text lexer cuts it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ScalarForm {
    /// A bare word that starts with a digit or `-`, or that has only digits, `-` and `.`.
    Number,
    /// A quoted string, or a bare word that names no engine keyword. Keywords match without case.
    Text,
    /// This engine keyword, written as a bare word.
    Keyword(String),
}

/// Parser acceptance of a shared modifier block. This does not establish runtime effects,
/// occurrence limits, duplicate rules, or the existence of localisation keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModifierBlock {
    /// Named keys, with a partial result when some member paths remain unresolved.
    pub fixed_keys: crate::GrammarProperty<Vec<crate::Field>>,
    /// Forms of entries whose keys are not fixed keys.
    pub entries: crate::GrammarProperty<Vec<ModifierEntry>>,
}

/// Parser acceptance of a triggered modifier clause, such as `triggered_planet_modifier`. This
/// does not establish when the condition is evaluated, how a multiplier scales the modifier, or
/// where the modifier takes effect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggeredModifierBlock {
    /// Keys that the clause reads itself, such as `potential` and `mult`. A `modifier` key
    /// carries its `FieldMembers::ModifierBlock`.
    pub fixed_keys: crate::GrammarProperty<Vec<crate::Field>>,
    /// The modifier block that reads every key that `fixed_keys` does not name, so modifier
    /// entries can be written directly in the clause. `Unresolved` means that the disposition of
    /// such keys is not established, not that the engine rejects them.
    pub other_keys: crate::GrammarProperty<ModifierMembers>,
}

/// Keys read as members of a shared modifier block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModifierMembers {
    /// The modifier-block reader; its identity matches every field that the same reader reads.
    pub reader: Reader,
    /// The modifier block's grammar.
    pub block: ModifierBlock,
}

/// An entry selected by a name in the modifier table or a reference registry.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ModifierEntry {
    /// The key is a modifier name, as reported by `Native::modifiers`,
    /// `Native::modifier_families`, or `Game::loaded_modifiers`. Scripted modifiers use this
    /// form: their registry's modifier-family answer makes each item key a modifier name.
    Numeric {
        /// The shared fixed-point reader and its conversion properties.
        value: Reader,
    },
    /// The key is an item key of the target registry.
    Reference {
        /// Registry containing the referenced items, or an unresolved registry join.
        target: crate::ReferenceTarget,
        /// Broad value form; no numeric conversion limits are stated here.
        value: crate::ReaderKind,
    },
}

/// Parser acceptance of a shared weight block, such as `ai_weight`, `weight_modifier` or a
/// script value, and of the `modifier` entries inside it. This does not establish how the engine
/// evaluates the weight.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeightBlock {
    /// The reader of a bare value written instead of a block, such as `ai_weight = 7`, or `None`
    /// when the block reader accepts no bare value.
    pub scalar: crate::GrammarProperty<Option<Reader>>,
    /// Named keys. A nested block key carries its own members and read scope.
    pub fixed_keys: crate::GrammarProperty<Vec<crate::Field>>,
    /// Keys that select an arithmetic operation.
    pub operations: crate::GrammarProperty<Vec<WeightOperation>>,
    /// How a further operation key affects the block. `Accumulate`: each operation adds an
    /// entry. `Replace`: the block holds one operation, and a later operation key replaces it;
    /// the engine reports the replacement in its log.
    pub operation_repeat: RepeatBehavior,
    /// Keys that are neither fixed keys nor operations.
    pub other_keys: WeightOtherKeys,
}

/// A key that selects an arithmetic operation of a weight block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeightOperation {
    /// The key, such as `add` or `factor`.
    pub key: String,
    /// The reader of the operand, or `None` when the engine reads no value for this key.
    #[serde(deserialize_with = "crate::answer::required_option")]
    pub operand: Option<Reader>,
}

/// What a weight block does with a key that is neither a fixed key nor an operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum WeightOtherKeys {
    /// The engine reports the key in its log and stores nothing.
    Rejected,
    /// The key is read as a trigger condition, in these scopes.
    Triggers(crate::GrammarProperty<Vec<crate::ReadScope>>),
    /// The disposition is not established.
    Unresolved,
}

/// An exhaustive set of accepted spellings, independently of fallback or recovery behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum FieldDomain {
    /// The accepted domain is not established.
    Unknown,
    /// The accepted spellings, sorted by name.
    Listed(Vec<String>),
}

/// The lookups that the engine makes with a field's value.
///
/// A lookup names the registry that holds the item a key selects. It says nothing about which
/// keys a mod or the base game defines.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum FieldReference {
    /// No lookup of the value is established. This is not proof that the engine makes none; a
    /// gap says why when the field's reader holds a key.
    #[default]
    NotEstablished,
    /// Each established lookup, under the read condition it belongs to.
    Lookups(Vec<ReferenceLookup>),
}

/// One lookup of a field's value as a key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceLookup {
    /// The read alternative that performs this lookup.
    pub condition: FieldCondition,
    /// The registry whose items the key selects.
    pub target: ReferenceTarget,
    /// When the lookup runs.
    pub stage: LookupStage,
    /// Which item a key selects.
    pub key_match: KeyMatch,
    /// Whether an empty key is looked up.
    pub empty_key: EmptyKey,
    /// What a key that selects no item yields.
    pub on_missing: MissingResult,
}

/// The collection a lookup searches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ReferenceTarget {
    /// A registry, named by its full content directory, such as `common/ship_sizes`. It can be a
    /// registry that `Native::registries` does not list, when its loader is outside that method.
    Registry {
        /// The content directory.
        name: String,
    },
    /// The trigger commands, as `declarations(DeclarationKind::Trigger)` lists them. The
    /// lookup's other facts say what a name that is no trigger command yields.
    Triggers,
    /// The searched collection is not joined to a content directory; a gap says why.
    Unresolved,
}

/// When the engine looks a key up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum LookupStage {
    /// While the field is read, so the target registry must already be loaded.
    WhileReading,
    /// Later, when the engine resolves the keys that its readers deferred. Keys may name items
    /// that load after the field.
    Deferred,
    /// When the engine initializes the object that holds the field, after reading it. The field
    /// stores the key text; the initialization looks it up.
    OwnerInitialization,
    /// Each time the engine uses the item, such as when it shows the item's name. A name that is
    /// missing does not stop the item from loading.
    WhenUsed,
    /// The stage is not established.
    Unresolved,
}

/// Which item a key selects. Keys are compared byte for byte, without case folding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum KeyMatch {
    /// The item whose key equals the value.
    Equal,
    /// The first item, in the collection's order, whose key equals the value.
    FirstEqual,
    /// The comparison is not established.
    Unresolved,
}

/// Whether an empty value is looked up like any other key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum EmptyKey {
    /// An empty value is looked up; `on_missing` applies when no item has an empty key.
    LookedUp,
    /// An empty value is not looked up.
    NotLookedUp,
    /// Not established.
    Unresolved,
}

/// What a key that selects no item yields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum MissingResult {
    /// A typed placeholder object, not a null pointer or the previous value. Its type is not
    /// established to be the target registry's item type.
    NullObject,
    /// A name that no trigger command has is accepted as a scripted trigger. The lookup does not
    /// check that a scripted trigger has the name.
    ScriptedTriggerPlaceholder,
    /// Nothing: a list reader logs the key and adds no item to the list.
    Omitted,
    /// Not established.
    Unresolved,
}
