//! The dynamic-name method's result for every registered effect and trigger, for Native's
//! developers: each command's flag roles, stores and stops. Store routes hold addresses and
//! appear in no public answer. Not a consumer API.
//!
//! ```no_run
//! use pdx_native::Native;
//! use pdx_native::internals::dynamic_name_commands;
//!
//! let native = Native::open("/path/to/Stellaris")?;
//! for command in dynamic_name_commands::run(&native)? {
//!     println!("{:?} {}: {:?}", command.kind, command.name, command.outcome);
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
use super::Native;
use crate::{Error, Operation};

pub use crate::engine::analysis::declarations::ScopeType;
pub use crate::engine::analysis::dynamic_names::{
    CommandNames, FlagCommand, NameOutcome, Role, RoleUse, routes::Route,
};

/// The method's result for every registered command on an opened installation. A `Native` over
/// recorded answers has no method result: the error is `Error::Unsupported`.
pub fn run(native: &Native) -> Result<Vec<CommandNames>, Error> {
    native.method_result(Operation::DynamicNames, || native.dynamic_name_commands())
}
