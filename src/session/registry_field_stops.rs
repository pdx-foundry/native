//! The registry field method's own result, for Native's developers: every token path, where each
//! one stopped, and every gap before normalization. Addresses appear here and in no public answer.
//! Not a consumer API.
//!
//! ```no_run
//! use pdx_native::Native;
//! use pdx_native::internals::registry_field_stops;
//!
//! let native = Native::open("/path/to/Stellaris")?;
//! let result = registry_field_stops::run(&native, "common/megastructures")?;
//! for gap in result.gaps.iter().filter(|gap| gap.stop.is_some()) {
//!     println!("{:?} {}: {}", gap.kind, gap.reason, gap.stop.unwrap());
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
use super::Native;
use crate::Error;

pub use crate::engine::analysis::fields::{
    Condition, FieldGap, FieldGapKind, PathOutcome, ReaderJoin, RegistryFieldResult, RootField,
    TokenPath, Value,
};
pub use crate::engine::analysis::stop::{Bound, Obstacle, Stop, Unknown, Unresolved};

/// Run the registry field method for `registry` on an opened installation. A `Native` over
/// recorded answers has no such result: the error is `Error::Unsupported`.
pub fn run(native: &Native, registry: &str) -> Result<RegistryFieldResult, Error> {
    native.registry_field_result(registry)
}
