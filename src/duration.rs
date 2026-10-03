//! Static duration keys of a command: how unit keys set one count.
use serde::{Deserialize, Serialize};

use crate::GrammarProperty;

/// Sibling keys of one command that set one duration count, such as `days`, `months` and `years`.
///
/// Keys are grouped by the engine code that reads them, never by name. A key's factor multiplies
/// the value written for that key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Duration {
    /// Each key of the group and the factor it applies.
    pub units: Vec<DurationUnit>,
    /// How a later key combines with an earlier one.
    pub combination: GrammarProperty<DurationCombination>,
    /// The count when no key of the group is written.
    pub omitted_count: GrammarProperty<i64>,
}

/// One key of a duration group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurationUnit {
    /// The child key.
    pub key: String,
    /// `Some(k)`: the key applies factor `k`. `None`: the key keeps the current shared factor.
    /// `Partial` when the key's own code is established but the group's combination is not.
    pub factor: GrammarProperty<Option<i64>>,
}

/// How later duration keys combine with earlier ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DurationCombination {
    /// Each key multiplies its value by its factor while reading, into one count. The last key
    /// read replaces the count.
    ScaledAtRead,
    /// Every key writes one scoped operand, which keeps the operand's selection rules instead of
    /// replacing it whole. A key with a factor replaces one shared factor; a key without one
    /// keeps it. At execution the count is the selected operand value times the factor, as a
    /// wrapping signed 32-bit product.
    SharedFactor {
        /// The factor before any key is read.
        initial_factor: i64,
    },
}
