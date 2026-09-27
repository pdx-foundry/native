//! The command grammar method's own result, for Native's developers: the receiver join's
//! obstruction, or every child token path and stop, with the public answer derived from it.
//! Addresses appear here and in no public answer. Not a consumer API.
//!
//! ```no_run
//! use pdx_native::{DeclarationKind, Native};
//! use pdx_native::internals::command_grammar_stops;
//!
//! let native = Native::open("/path/to/Stellaris")?;
//! let run = command_grammar_stops::run(&native, DeclarationKind::Effect, "hidden_effect")?;
//! if let Err(unresolved) = &run.result {
//!     println!("{}: {:?}", unresolved.reason, unresolved.stop);
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
use crate::engine::analysis::references::ReferenceFacts;
use crate::engine::analysis::{
    declarations::{self, Site},
    grammar,
};
use std::collections::BTreeSet;

use super::{Native, grammar::normalize};
use crate::{Answer, CommandGrammar, DeclarationKind, Error, Operation};

pub use crate::engine::analysis::grammar::{ChildFields, GrammarResult};
pub use crate::engine::analysis::stop::{Cause, CauseKind, Trace, Unresolved};

/// One run of the command grammar method for one registered command.
pub struct Run {
    /// The public answer, as `Native::command_grammar` gives it on this installation.
    pub answer: Answer<CommandGrammar>,
    /// The method's own result, from which `answer` is derived: the grammar, or the obstruction
    /// that stopped the command's receiver join.
    pub result: Result<GrammarResult, Unresolved>,
    /// Registration, factory and receiver joins retained even if a later stage fails.
    pub chain: Chain,
}

/// Run the command grammar method once for the command `name` of `kind` on an opened
/// installation. A `Native` over recorded answers has no method result: the error is
/// `Error::Unsupported`. The answer is not written to a recorder.
pub fn run(native: &Native, kind: DeclarationKind, name: &str) -> Result<Run, Error> {
    native.method_result(Operation::CommandGrammar, || {
        let (input, inventory) = native
            .declaration_analysis(Operation::CommandGrammar)?
            .grammar_input(kind)
            .map_err(|failure| super::questions::error(Operation::CommandGrammar, failure))?;
        if matches!(
            super::grammar::registered_factory(inventory, name),
            Ok(None)
        ) {
            return Err(Error::UnknownCommand {
                kind,
                name: name.into(),
            });
        }
        let references = native.reference_facts(Operation::CommandGrammar)?;
        Ok(inspect_command(
            input,
            inventory,
            name,
            native.build(),
            references,
        ))
    })
}

/// One registration contributing to a named command. Addresses are developer diagnostics only.
#[derive(Debug, serde::Serialize)]
pub struct Registration {
    /// Registration instruction address.
    pub instruction: u64,
    /// Registered factory, when readable.
    pub factory: Option<u64>,
}

/// Available joins preceding grammar analysis. Missing stages remain absent, never inferred.
#[derive(Debug, Default, serde::Serialize)]
pub struct Chain {
    /// The analysis stage that failed, or none when grammar analysis returned a result.
    pub stopped_at: Option<&'static str>,
    /// All registrations with this command name.
    pub registrations: Vec<Registration>,
    /// Unique factory selected by the same lookup as the public operation.
    pub factory: Option<u64>,
    /// Factory create method, if its slot is readable.
    pub create: Option<u64>,
    /// Concrete receiver vtable established by factory evaluation.
    pub receiver: Option<u64>,
    /// Virtual read method, if its slot is readable.
    pub read: Option<u64>,
    /// Virtual member method, if its slot is readable.
    pub member: Option<u64>,
}

/// An unresolved registration observation; it need not represent one distinct command.
#[derive(Debug, serde::Serialize)]
pub struct UnknownRegistration {
    /// Registration instruction; repeated addresses can have distinct caller observations.
    pub instruction: u64,
    /// Why no name was established.
    pub reason: &'static str,
}

/// Inventory uncertainty retained after visiting every unique known command name.
pub struct Population {
    /// Observations without a name. Their distinct command count is unknown.
    pub unknown_registrations: Vec<UnknownRegistration>,
    /// Input-wide inventory gaps; a nonempty list precludes a full denominator.
    pub inventory_gaps: Vec<&'static str>,
}

/// Analyze all known command names using one shared input for the requested family.
/// The visitor receives each name and run in name order, including failed joins. Each raw
/// result can be dropped before the next run. Unknown registrations and input-wide gaps
/// are returned separately from named totals.
pub fn population(
    native: &Native,
    kind: DeclarationKind,
    mut visit: impl FnMut(&str, Run),
) -> Result<Population, Error> {
    native.method_result(Operation::CommandGrammar, || {
        let (input, inventory) = native
            .declaration_analysis(Operation::CommandGrammar)?
            .grammar_input(kind)
            .map_err(|failure| super::questions::error(Operation::CommandGrammar, failure))?;
        let (names, unknown_registrations) = inventory_names(inventory);
        let references = native.reference_facts(Operation::CommandGrammar)?;
        for name in names {
            visit(
                &name,
                inspect_command(input, inventory, &name, native.build(), references),
            );
        }
        Ok(Population {
            unknown_registrations,
            inventory_gaps: inventory.table_gaps.clone(),
        })
    })
}

fn inventory_names(
    inventory: &declarations::DeclarationResult,
) -> (BTreeSet<String>, Vec<UnknownRegistration>) {
    let mut names = BTreeSet::new();
    let mut unknown = Vec::new();
    for &(instruction, ref site) in &inventory.sites {
        match site {
            Site::Declared { name, .. }
            | Site::Unreadable {
                name: Some(name), ..
            } => {
                names.insert(name.clone());
            }
            Site::RuntimeToken { obstacle } => unknown.push(UnknownRegistration {
                instruction,
                reason: obstacle,
            }),
            Site::Unreadable { name: None, what } => unknown.push(UnknownRegistration {
                instruction,
                reason: what,
            }),
        }
    }
    (names, unknown)
}

fn inspect_command(
    input: &grammar::GrammarInput,
    inventory: &declarations::DeclarationResult,
    name: &str,
    build: crate::BuildId,
    references: &ReferenceFacts,
) -> Run {
    let mut chain = Chain {
        stopped_at: Some("registration"),
        ..Chain::default()
    };
    for &(instruction, ref site) in &inventory.sites {
        match site {
            Site::Declared {
                name: found,
                factory,
                ..
            } if found == name => chain.registrations.push(Registration {
                instruction,
                factory: Some(*factory),
            }),
            Site::Unreadable {
                name: Some(found), ..
            } if found == name => chain.registrations.push(Registration {
                instruction,
                factory: None,
            }),
            _ => {}
        }
    }
    let result = (|| {
        let factory = super::grammar::registered_factory(inventory, name)?
            .ok_or_else(|| Unresolved::new("command-registration"))?;
        chain.factory = Some(factory);
        chain.create = input
            .declarations
            .pointers
            .get(&(factory + input.declarations.slots.create))
            .copied();
        chain.stopped_at = Some("factory receiver");
        let receiver = declarations::factory_vtable(&input.declarations, factory)?;
        chain.receiver = Some(receiver);
        chain.read = input
            .declarations
            .pointers
            .get(&(receiver + input.declarations.parser_slots.read))
            .copied();
        chain.member = input
            .declarations
            .pointers
            .get(&(receiver + input.declarations.parser_slots.member))
            .copied();
        chain.stopped_at = Some("reader slots and bodies");
        let reader = declarations::reader_at_vtable(&input.declarations, receiver)?;
        chain.stopped_at = Some("grammar");
        let result = grammar::analyze_reader(input, reader, 0)?;
        chain.stopped_at = None;
        Ok(result)
    })();
    Run {
        answer: normalize(result.as_ref(), name, build, references),
        result,
        chain,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_unreadable_entries_remain_and_unknown_observations_are_not_a_denominator() {
        let inventory = declarations::DeclarationResult {
            sites: vec![
                (
                    1,
                    Site::Declared {
                        name: "composed".into(),
                        factory: 9,
                        description: String::new(),
                        usage: String::new(),
                        scopes: declarations::ScopeOutcome::Any,
                    },
                ),
                (
                    2,
                    Site::Unreadable {
                        name: Some("composed".into()),
                        what: "entry",
                    },
                ),
                (
                    3,
                    Site::Unreadable {
                        name: Some("unreadable".into()),
                        what: "entry",
                    },
                ),
                (
                    4,
                    Site::RuntimeToken {
                        obstacle: "caller-bound",
                    },
                ),
                (
                    4,
                    Site::Unreadable {
                        name: None,
                        what: "name",
                    },
                ),
            ],
            table_gaps: vec!["table"],
        };
        let (names, unknown) = inventory_names(&inventory);
        assert_eq!(
            names,
            BTreeSet::from(["composed".into(), "unreadable".into()])
        );
        assert_eq!(unknown.len(), 2);
        assert_eq!(unknown[0].reason, "caller-bound");
        assert_eq!(unknown[1].reason, "name");
        assert_eq!(unknown[0].instruction, unknown[1].instruction);
        assert_eq!(
            super::super::grammar::registered_factory(&inventory, "composed"),
            Err(Unresolved::new("command-registration"))
        );
    }
}
