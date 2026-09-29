//! Structured facts about a registered command's block reader.
use crate::{BlockFamily, DeclaredScopes, Field, FieldReference, Reader};
use serde::{Deserialize, Serialize};

/// An extracted property, with missing evidence kept distinct from an empty result.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GrammarProperty<T> {
    /// No value was established.
    #[default]
    Unresolved,
    /// These values were established, but more may exist.
    Partial(T),
    /// The property was established completely within the method.
    Known(T),
}

/// Static child grammar of one registered command. This does not establish runtime meaning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandGrammar {
    /// Concrete shared parser identity, including its member reader.
    pub reader: Reader,
    /// Each way to write the command's value. `Known` lists every accepted form.
    #[serde(default)]
    pub forms: GrammarProperty<Vec<CommandForm>>,
    /// Each argument that the reader stores as an event target. A known empty list means that
    /// the command takes no target argument. A partial or unresolved list proves no absence.
    #[serde(default)]
    pub targets: GrammarProperty<Vec<TargetArgument>>,
    /// Command families that the block can dispatch to.
    pub child_families: GrammarProperty<Vec<BlockFamily>>,
    /// Named child keys and their conditional read alternatives.
    pub fixed_keys: GrammarProperty<Vec<Field>>,
    /// Child grammar selected by dynamic integer keys; a known `None` means none are accepted.
    pub numeric_keys: GrammarProperty<Option<Box<CommandGrammar>>>,
    /// Established reader selections that depend on preceding children.
    pub ordering: GrammarProperty<Vec<ChildOrderRule>>,
    /// Groups of child keys that set one duration count. A partial list proves no absence.
    #[serde(default)]
    pub durations: GrammarProperty<Vec<crate::Duration>>,
}

/// An accepted value alternative or a block whose children have their own properties.
// Keep the public command value inline with the existing normalized answer structure.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum CommandForm {
    /// `command = value`, with one entry for each accepted value alternative.
    Value(CommandValue),
    /// `command = { … }`. The child properties describe the block.
    Block,
}

/// The reader and reference lookup of an accepted command value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandValue {
    /// Shared reader identity and the broad kind of its value.
    pub reader: Reader,
    /// Lookup performed for a reference value; acceptance assumes a found key.
    pub reference: FieldReference,
}

/// A stored event-target argument and the scope check that constrains it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetArgument {
    /// Location of the argument in the command's grammar.
    pub argument: ArgumentPath,
    /// Scope types accepted by the established check.
    pub scopes: DeclaredScopes,
    /// Earliest check that decides the accepted set without a stricter later check.
    pub stage: TargetCheckStage,
}

/// When the engine checks a stored target's scope type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum TargetCheckStage {
    /// During parsing or assignment.
    WhileReading,
    /// After reading and before execution: in the command's initialization or validation.
    Validation,
    /// When the command executes or evaluates.
    Execution,
    /// No single complete check was established.
    Unresolved,
}

/// Location of an event-target argument relative to its command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ArgumentPath {
    /// The command's own value.
    Value,
    /// Named keys from the outermost key to the argument.
    Key(Vec<String>),
}

/// A reader selection that depends on the sequence of already parsed children.
/// This describes routing, not a restriction on parser acceptance or runtime execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildOrderRule {
    /// Child key to which the rule applies.
    pub child: String,
    /// Conditions that must all hold for this routing choice.
    pub conditions: Vec<ChildOrderCondition>,
    /// Reader or command family selected on this path.
    pub outcome: ChildOrderOutcome,
}

/// A condition on children already present when the next key is read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChildOrderCondition {
    /// Whether the child collection is empty at this point.
    First(bool),
    /// Whether the immediately preceding stored child's key matches one of these keys.
    Previous {
        /// Keys recovered from the executable's token inventory.
        keys: Vec<String>,
        /// True for a matching predecessor; false for any other predecessor.
        matches: bool,
    },
}

/// A proven destination of a conditional child-routing path.
// Keep the public Reader inline with other normalized grammar fields.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChildOrderOutcome {
    /// The key is handled by this field reader.
    Read(Reader),
    /// The key is delegated to this command collection.
    Dispatch(BlockFamily),
}

#[cfg(test)]
mod tests {
    use super::GrammarProperty;

    #[test]
    fn default_property_is_unresolved_without_a_value_default() {
        struct NoDefault;
        assert!(matches!(
            GrammarProperty::<NoDefault>::default(),
            GrammarProperty::Unresolved
        ));
    }
}
