//! The registry field method's own result, for Native's developers: every token path, where each
//! one stopped, and every gap before normalization, with the public answer derived from it.
//! Addresses appear here and in no public answer. Not a consumer API.
//!
//! ```no_run
//! use pdx_native::Native;
//! use pdx_native::internals::registry_field_stops;
//!
//! let native = Native::open("/path/to/Stellaris")?;
//! let run = registry_field_stops::run(&native, "common/megastructures")?;
//! println!("{:?}", run.answer.completeness);
//! for gap in run.result.gaps.iter().filter(|gap| gap.stop.is_some()) {
//!     println!("{:?} {}: {}", gap.kind, gap.reason, gap.stop.unwrap());
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
use super::Native;
use crate::{Answer, Error, Field, Operation};

pub use crate::engine::analysis::fields::{
    Condition, FieldGap, FieldGapKind, PathOutcome, ReaderJoin, RegistryFieldResult, RootField,
    TokenPath, Value,
};
pub use crate::engine::analysis::stop::{Bound, Obstacle, Stop, Unknown, Unresolved};

/// One run of the registry field method for one registry.
#[derive(Debug, Clone)]
pub struct Run {
    /// The public answer, as `Native::registry_fields` gives it on this installation.
    pub answer: Answer<Vec<Field>>,
    /// The method's own result, from which `answer` is derived.
    pub result: RegistryFieldResult,
}

/// Run the registry field method once for `registry` on an opened installation. A `Native` over
/// recorded answers has no method result: the error is `Error::Unsupported`. The answer is not
/// written to a recorder.
pub fn run(native: &Native, registry: &str) -> Result<Run, Error> {
    native.method_result(Operation::RegistryFields, || {
        let result = native.registry_field_result(registry)?;
        Ok(Run {
            answer: native.registry_field_answer(registry, &result),
            result,
        })
    })
}
