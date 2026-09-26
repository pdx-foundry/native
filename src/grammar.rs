//! Structured facts about a registered command's block reader.
use crate::{BlockFamily, Field, Reader};
use serde::{Deserialize, Serialize};

/// An extracted property, with missing evidence kept distinct from an empty result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GrammarProperty<T> {
    /// No value was established.
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
    /// Command families that the block can dispatch to.
    pub child_families: GrammarProperty<Vec<BlockFamily>>,
    /// Named child keys and their conditional read alternatives.
    pub fixed_keys: GrammarProperty<Vec<Field>>,
    /// Child grammar selected by dynamic integer keys; a known `None` means none are accepted.
    pub numeric_keys: GrammarProperty<Option<Box<CommandGrammar>>>,
    /// Established reader selections that depend on preceding children.
    pub ordering: GrammarProperty<Vec<ChildOrderRule>>,
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChildOrderOutcome {
    /// The key is handled by this field reader.
    Read(Reader),
    /// The key is delegated to this command collection.
    Dispatch(BlockFamily),
}
