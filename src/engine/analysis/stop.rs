//! Where and why a walk through code stopped.
//!
//! A static method that cannot follow code to its end says so with the instruction it stopped
//! at, where it last entered code, and the obstacle: a value it did not know, a bound it spent,
//! or code it does not run. These are developer diagnostics. Addresses never reach a public
//! answer: public gap text quotes only the reason.
//!
//! While cause tracing is on, an obstruction at an unknown value also carries a [`Trace`]: the
//! places on its path where that value stopped being known. See
//! [`trace_causes`](super::evaluate::trace_causes).
use std::cmp::Ordering;
use std::fmt;

use serde::Serialize;

/// The most causes that one [`Trace`] keeps.
pub const CAUSE_LIMIT: usize = 4;

/// A walk through code, or a method's reading of its result, could not be followed to its end.
///
/// Equality and order compare `reason` and `stop` only. A trace explains an obstruction but is
/// not part of it, so tracing never changes a comparison, a set or an answer.
#[derive(Debug, Clone, Serialize)]
pub struct Unresolved {
    /// A short name for the obstruction, such as `branch-value`. Public gap text may quote it.
    pub reason: &'static str,
    /// Where the walk stopped. `None` when no walk was under way, such as when a method finds
    /// that a walk's result does not hold what it needs.
    pub stop: Option<Stop>,
    /// Why the value that the method needed was unknown, when cause tracing was on and the
    /// obstruction was an unknown value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<Box<Trace>>,
}

impl Unresolved {
    /// An obstruction that no walk located.
    pub const fn new(reason: &'static str) -> Self {
        Self {
            reason,
            stop: None,
            trace: None,
        }
    }

    /// An obstruction that a walk met at `instruction`, where it last entered code at `entry`.
    pub const fn at(
        reason: &'static str,
        instruction: u64,
        entry: u64,
        obstacle: Obstacle,
    ) -> Self {
        Self {
            reason,
            stop: Some(Stop {
                instruction,
                entry,
                obstacle,
            }),
            trace: None,
        }
    }

    /// This obstruction with `trace`, the causes of the unknown value that it needed.
    pub fn traced(self, trace: Option<Trace>) -> Self {
        Self {
            trace: trace.map(Box::new),
            ..self
        }
    }

    /// Add the causes of `other`, an equal obstruction met on another path, so that removing
    /// `other` as a duplicate keeps every cause.
    fn merge_trace(&mut self, other: &Self) {
        match (&mut self.trace, &other.trace) {
            (Some(trace), Some(other)) => trace.merge(other),
            (None, Some(other)) => self.trace = Some(other.clone()),
            (_, None) => {}
        }
    }
}

/// Sort `stops` and keep each obstruction once, with the causes of all its copies.
pub(crate) fn sort_and_dedup(stops: &mut Vec<Unresolved>) {
    stops.sort();
    stops.dedup_by(|duplicate, kept| {
        let equal = duplicate == kept;
        if equal {
            kept.merge_trace(duplicate);
        }
        equal
    });
}

impl PartialEq for Unresolved {
    fn eq(&self, other: &Self) -> bool {
        (self.reason, self.stop) == (other.reason, other.stop)
    }
}

impl Eq for Unresolved {}

impl PartialOrd for Unresolved {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Unresolved {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.reason, self.stop).cmp(&(other.reason, other.stop))
    }
}

/// Where a walk through code stopped, and what stopped it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Stop {
    /// The instruction that could not run or could not be followed, or where a bound ran out.
    pub instruction: u64,
    /// Where the walk last entered code: its own entry, or the target of the innermost call that
    /// it entered. A jump into other code without a call does not change it, so the function that
    /// holds `instruction` comes from the executable's symbols.
    pub entry: u64,
    /// What stopped the walk.
    pub obstacle: Obstacle,
}

/// What stopped a walk at its instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Obstacle {
    /// The instruction needs a value that the walk does not know.
    Unknown(Unknown),
    /// The walk used all of one bound.
    Bound(Bound),
    /// The method does not run this instruction or one of its operand forms.
    Unsupported,
    /// The code goes to an address outside the code that the walk read.
    OutsideCode,
    /// The walk arrived again at an instruction that its own path already ran.
    Cycle,
    /// The walk's caller ended the walk at this call, or the call could not be followed.
    Call,
}

/// A value that a walk did not know.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Unknown {
    /// The general register `x0` to `x30`, or `sp` as 31.
    Register(u8),
    /// The condition flags.
    Flags,
}

/// A bound of a walk, with its size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Bound {
    /// Instructions that one path may run.
    Steps(usize),
    /// Paths that one run may follow.
    Paths(usize),
    /// Arrivals of one path at one loop head.
    LoopArrivals(u32),
    /// Times that the joined facts at one loop head may become fewer.
    Joins(u32),
    /// Walk states that one search may visit.
    States(usize),
    /// Visits that one search may make to the instructions of a function, together.
    Visits(usize),
    /// Entries that one jump table may select.
    TableEntries(usize),
}

/// Why a value that a walk needed was unknown: where, on the walk's own path, it stopped being
/// known. A trace with more than one cause names no single cause.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Trace {
    causes: [Option<Cause>; CAUSE_LIMIT],
    /// The value combined more than [`CAUSE_LIMIT`] causes, and the rest are not kept.
    pub truncated: bool,
    /// Part of the value was never known on the path, so no cause is recorded for it: it was
    /// unknown when the walk began, in memory that the path never wrote, or in a vector
    /// register, whose causes are not traced.
    pub unrecorded: bool,
}

impl Trace {
    /// A value with no recorded cause.
    pub(crate) const UNRECORDED: Self = Self {
        causes: [None; CAUSE_LIMIT],
        truncated: false,
        unrecorded: true,
    };

    /// A value that stopped being known because of `cause`.
    pub(crate) const fn of(cause: Cause) -> Self {
        let mut causes = [None; CAUSE_LIMIT];
        causes[0] = Some(cause);
        Self {
            causes,
            truncated: false,
            unrecorded: false,
        }
    }

    /// The recorded causes, each once, in the order that the trace recorded them. A join comes
    /// before the earlier causes that it combines.
    pub fn causes(&self) -> impl Iterator<Item = Cause> + '_ {
        self.causes.iter().flatten().copied()
    }

    /// Whether the trace records nothing: no cause and no unrecorded part.
    pub(crate) fn is_empty(&self) -> bool {
        self.causes[0].is_none() && !self.unrecorded
    }

    /// Add the causes and markers of `other`, a value that this one combines. Causes past
    /// [`CAUSE_LIMIT`] are dropped and mark the trace as truncated.
    pub(crate) fn merge(&mut self, other: &Self) {
        self.truncated |= other.truncated;
        self.unrecorded |= other.unrecorded;
        for cause in other.causes() {
            if self.causes().any(|kept| kept == cause) {
                continue;
            }
            match self.causes.iter_mut().find(|slot| slot.is_none()) {
                Some(slot) => *slot = Some(cause),
                None => self.truncated = true,
            }
        }
    }
}

/// One place where a value stopped being known.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Cause {
    /// How the value stopped being known.
    pub kind: CauseKind,
    /// The instruction at which the value stopped being known.
    pub instruction: u64,
    /// Where the walk last entered code then, as in [`Stop::entry`].
    pub entry: u64,
}

/// How a value stopped being known.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum CauseKind {
    /// A call returned no known value in `x0`, or left a caller-saved register or the flags
    /// unknown.
    Call,
    /// The method made memory unknown, such as an object that an unrecognized call may change.
    Invalidated,
    /// A store to an unknown address may have written the value.
    UnknownStore,
    /// Paths joined at this loop head, and other paths may have known or lost the value
    /// differently.
    Join,
}

impl fmt::Display for Cause {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} at {:#x}, entered at {:#x}",
            self.kind, self.instruction, self.entry
        )
    }
}

impl fmt::Display for CauseKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Call => "call",
            Self::Invalidated => "invalidated by the method",
            Self::UnknownStore => "store to an unknown address",
            Self::Join => "paths joined",
        })
    }
}

impl fmt::Display for Stop {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} at {:#x}, entered at {:#x}",
            self.obstacle, self.instruction, self.entry
        )
    }
}

impl fmt::Display for Obstacle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown(unknown) => write!(formatter, "{unknown} unknown"),
            Self::Bound(bound) => write!(formatter, "{bound} spent"),
            Self::Unsupported => formatter.write_str("unsupported instruction"),
            Self::OutsideCode => formatter.write_str("outside the read code"),
            Self::Cycle => formatter.write_str("cycle"),
            Self::Call => formatter.write_str("call not followed"),
        }
    }
}

impl fmt::Display for Unknown {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Register(31) => formatter.write_str("sp"),
            Self::Register(index) => write!(formatter, "x{index}"),
            Self::Flags => formatter.write_str("flags"),
        }
    }
}

impl fmt::Display for Bound {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Steps(size) => write!(formatter, "step bound {size}"),
            Self::Paths(size) => write!(formatter, "path bound {size}"),
            Self::LoopArrivals(size) => write!(formatter, "loop arrival bound {size}"),
            Self::Joins(size) => write!(formatter, "join bound {size}"),
            Self::States(size) => write!(formatter, "state bound {size}"),
            Self::Visits(size) => write!(formatter, "visit bound {size}"),
            Self::TableEntries(size) => write!(formatter, "table entry bound {size}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call_at(instruction: u64) -> Trace {
        Trace::of(Cause {
            kind: CauseKind::Call,
            instruction,
            entry: 0x100,
        })
    }

    fn branch_value() -> Unresolved {
        Unresolved::at(
            "branch-value",
            0x108,
            0x100,
            Obstacle::Unknown(Unknown::Register(1)),
        )
    }

    #[test]
    fn a_trace_is_not_part_of_an_obstructions_identity() {
        let traced = branch_value().traced(Some(call_at(0x104)));

        assert_eq!(traced, branch_value());
        assert_eq!(traced.cmp(&branch_value()), Ordering::Equal);
        assert_ne!(traced, Unresolved::new("branch-value"));
    }

    #[test]
    fn removing_duplicate_stops_keeps_the_causes_of_every_copy() {
        let mut unrecorded = Trace::UNRECORDED;
        unrecorded.merge(&call_at(0x104));
        let mut stops = vec![
            branch_value().traced(Some(unrecorded)),
            Unresolved::new("factory-return"),
            branch_value().traced(Some(call_at(0x10c))),
            branch_value(),
        ];

        sort_and_dedup(&mut stops);

        assert_eq!(stops, [branch_value(), Unresolved::new("factory-return")]);
        let trace = stops[0].trace.as_deref().unwrap();
        let causes: Vec<_> = trace.causes().map(|cause| cause.instruction).collect();
        assert_eq!(causes, [0x104, 0x10c]);
        assert!(trace.unrecorded);
    }

    #[test]
    fn a_trace_drops_causes_past_its_limit_and_says_so() {
        let mut trace = Trace::default();
        for instruction in (0..=CAUSE_LIMIT as u64).map(|call| 0x200 + call * 4) {
            trace.merge(&call_at(instruction));
        }
        trace.merge(&call_at(0x200));

        assert_eq!(trace.causes().count(), CAUSE_LIMIT);
        assert!(trace.truncated);
        assert!(!trace.unrecorded);
    }
}
