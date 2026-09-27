//! Dynamic names: names that script both defines and reads, such as country flags, grouped by
//! the store that holds them.
use crate::{DeclarationKind, ScopeReference};
use serde::{Deserialize, Serialize};

/// One store of dynamic names, with the commands that define, remove and read names in it.
///
/// Two commands share a namespace only when both reach the same store. A name defined by one
/// command of a namespace is visible to the readers of that namespace. A command can belong to
/// several namespaces, one for each scope type whose store it reaches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DynamicNamespace {
    /// The namespace's identity within this build. Compare it; do not parse it.
    pub id: DynamicNamespaceId,
    /// What kind of name the namespace holds.
    pub kind: DynamicNameKind,
    /// Whose store holds the names.
    pub owner: NamespaceOwner,
    /// The commands that add a name to the store, sorted by kind and name.
    pub defined_by: Vec<CommandReference>,
    /// The commands that remove a name from the store, sorted by kind and name.
    pub removed_by: Vec<CommandReference>,
    /// The commands that test whether the store holds a name, sorted by kind and name.
    pub read_by: Vec<CommandReference>,
    /// Whether the namespace's commands accept a name in another scope's store.
    pub dynamic_form: DynamicNameForm,
}

/// Opaque identity of a dynamic-name namespace within one build. It can change between builds.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DynamicNamespaceId(pub(crate) String);

/// The kind of name that a namespace holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DynamicNameKind {
    /// A flag name. Every flag kind interns its names in one engine table, so equal names are
    /// the same flag wherever they are stored.
    IntegerFlag,
}

/// Whose store holds a namespace's names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NamespaceOwner {
    /// Each scope object of this type has its own store; a command uses the store of the scope
    /// that it runs in.
    Scope(ScopeReference),
    /// One store, whatever scope the command runs in.
    Global,
}

/// A registered command, as `Native::declarations` names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandReference {
    /// The command's family.
    pub kind: DeclarationKind,
    /// The registered name.
    pub name: String,
}

/// Whether a namespace's commands accept `name@target`. The engine keeps that value's name and
/// target, and forms the flag at run time from the name and the scope that `target` names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DynamicNameForm {
    /// Every command of the namespace accepts `name@target`.
    TargetSuffix,
    /// No command of the namespace accepts `name@target`; the whole value is the name.
    NotAccepted,
    /// The form was not established for every command of the namespace.
    Unresolved,
}
