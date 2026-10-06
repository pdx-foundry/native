//! How script reuses text: inline scripts, scripted effects and triggers, script values and
//! scripted variables.
use crate::{BlockFamily, GrammarProperty};
use serde::{Deserialize, Serialize};

/// One way that script reuses text, with where it is accepted and when the engine expands it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptExpansion {
    /// The mechanism.
    pub mechanism: ExpansionMechanism,
    /// Where the names that a use refers to are defined, in lookup order when `Known`.
    pub definitions: GrammarProperty<Vec<ExpansionDefinitions>>,
    /// Where script can use the mechanism. A partial list proves no absence.
    pub hosts: GrammarProperty<Vec<ExpansionHost>>,
    /// How script writes a use.
    pub call_forms: GrammarProperty<Vec<CallForm>>,
    /// When the engine replaces a use with the text that it stands for. A tool checks the
    /// written text before this stage and the expanded text after it.
    pub stage: GrammarProperty<ExpansionStage>,
    /// How a definition refers to the parameters of a use. A known empty list means that the
    /// mechanism takes no parameters.
    pub parameter_forms: GrammarProperty<Vec<ParameterForm>>,
    /// What a use yields when it omits a parameter that the definition writes as a plain `$KEY$`
    /// outside the conditional blocks that the use drops. Omitting a parameter that the definition
    /// writes only as `$KEY|default$` or inside `[[KEY] … ]` is valid. A known `None` means that
    /// the mechanism takes no parameters.
    pub missing_parameter: GrammarProperty<Option<MissingParameter>>,
    /// Diagnostics that the engine logs for a use while it loads content. Other messages can
    /// exist, so a list is never `Known`.
    pub checks: GrammarProperty<Vec<ExpansionCheck>>,
}

/// A mechanism that reuses script text.
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
pub enum ExpansionMechanism {
    /// `inline_script = { script = path KEY = value }`: a file under the inline-script directory,
    /// read in place.
    InlineScript,
    /// A `common/scripted_effects` definition called by name as an effect.
    ScriptedEffect,
    /// A `common/scripted_triggers` definition called by name as a trigger.
    ScriptedTrigger,
    /// A `common/script_values` definition called as `value:name|KEY|value|`.
    ScriptValue,
    /// `@name = value`, used as `@name` or inside `@[ expression ]`.
    ScriptedVariable,
}

/// Where the names of a mechanism are defined.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ExpansionDefinitions {
    /// The files of a content directory, which every file can use.
    Directory {
        /// Content directory, such as `common/scripted_effects`.
        directory: String,
    },
    /// The earlier statements of the file that uses the name.
    SameFile,
}

/// Where script can use a mechanism.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ExpansionHost {
    /// The top level of a registry's definition files.
    RegistryRoot {
        /// Content directory of the registry, such as `common/traditions`.
        registry: String,
    },
    /// Every block of commands of this family, wherever a field or command reads one.
    Commands(BlockFamily),
    /// Every block that the shared object reader reads, such as the members of a definition.
    ObjectBlock,
    /// Every key whose reference lookup targets the trigger commands
    /// ([`crate::ReferenceTarget::Triggers`]).
    TriggerReference,
    /// Every operand of a [`crate::ReaderKind::ScopedNumeric`] reader, as `value:name` or
    /// `trigger:name`.
    ScopedOperand,
    /// Every statement that the shared reader reads.
    Statement,
}

/// How script writes a use.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[non_exhaustive]
pub enum CallForm {
    /// `name = value`, with no parameters.
    Value,
    /// `name = { KEY = value … }`.
    Block {
        /// The key that names the definition when the use's own key does not, such as `script`.
        name_key: Option<String>,
    },
    /// `prefix:name|KEY|value|`.
    Pipe,
    /// `@name`.
    Variable,
    /// `@[ expression ]`, which can name variables without `@`.
    Arithmetic,
}

/// When the engine replaces a use with the text that it stands for.
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
pub enum ExpansionStage {
    /// While the shared reader reads the statement, before a field reader receives the value.
    Lex,
    /// Where the use is read: the substituted text is read in its place.
    Read,
    /// After all content loads: the use is read as a placeholder, and the engine then generates
    /// and reads the definition's text with the use's parameters.
    Compile,
}

/// How a definition refers to the parameters of a use.
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
pub enum ParameterForm {
    /// `$KEY$`, replaced with the value.
    Substitution,
    /// `$KEY|default$`, replaced with the value, or with `default` when the use gives none.
    Default,
    /// `[[KEY] text ]`, kept only when the use gives the parameter.
    Conditional,
    /// `[[!KEY] text ]`, kept only when the use does not give the parameter.
    NegatedConditional,
}

/// What a use yields when it omits a parameter that its definition requires.
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
pub enum MissingParameter {
    /// The engine logs a diagnostic that names the missing parameters.
    Diagnostic,
    /// The text keeps `$KEY$`, and the engine reads it as written.
    KeptAsText,
}

/// A diagnostic that the engine logs for a use while it loads content.
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
pub enum ExpansionCheck {
    /// No definition has the name.
    UnknownName,
    /// Uses nest deeper than the engine's limit, as recursive definitions do.
    DepthLimit,
    /// A variable name does not start with a letter or holds other characters than letters,
    /// digits and underscores.
    InvalidVariableName,
    /// A definition uses a variable name that is already taken.
    DuplicateVariable,
}
