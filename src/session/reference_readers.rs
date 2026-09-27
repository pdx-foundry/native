//! The reference method's facts for every reference reader in the executable, for Native's
//! developers: each reader's database, the database's content directory, and its lookup or why
//! none was established. Database names appear here and in no public answer. Not a consumer API.
//!
//! ```no_run
//! use pdx_native::Native;
//! use pdx_native::internals::reference_readers;
//!
//! let native = Native::open("/path/to/Stellaris")?;
//! for (callee, fact) in &reference_readers::run(&native)?.readers {
//!     println!("{callee}: {:?} {:?}", fact.directory, fact.lookup);
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
use super::Native;
use crate::{Error, Operation};

pub use crate::engine::analysis::references::{
    KeyMatch, Lookup, ReaderForm, ReaderLookup, ReferenceFacts, Stage, reader,
};

/// The facts of every reference reader on an opened installation. A `Native` over recorded
/// answers has no method result: the error is `Error::Unsupported`.
pub fn run(native: &Native) -> Result<ReferenceFacts, Error> {
    native.method_result(Operation::RegistryFields, || {
        native.reference_facts(Operation::RegistryFields).cloned()
    })
}
