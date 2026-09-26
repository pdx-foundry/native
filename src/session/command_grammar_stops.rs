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
}

/// Run the command grammar method once for the command `name` of `kind` on an opened
/// installation. A `Native` over recorded answers has no method result: the error is
/// `Error::Unsupported`. The answer is not written to a recorder.
pub fn run(native: &Native, kind: DeclarationKind, name: &str) -> Result<Run, Error> {
    native.method_result(Operation::CommandGrammar, || {
        let result = native.command_grammar_result(kind, name)?;
        Ok(Run {
            answer: normalize(result.as_ref(), name, native.build()),
            result,
        })
    })
}
