//! Where and why a walk through code stopped.
//!
//! A static method that cannot follow code to its end says so with the instruction it stopped
//! at, where it last entered code, and the obstacle: a value it did not know, a bound it spent,
//! or code it does not run. These are developer diagnostics. Addresses never reach a public
//! answer: public gap text quotes only the reason.
use std::fmt;

use serde::Serialize;

/// A walk through code, or a method's reading of its result, could not be followed to its end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Unresolved {
    /// A short name for the obstruction, such as `branch-value`. Public gap text may quote it.
    pub reason: &'static str,
    /// Where the walk stopped. `None` when no walk was under way, such as when a method finds
    /// that a walk's result does not hold what it needs.
    pub stop: Option<Stop>,
}

impl Unresolved {
    /// An obstruction that no walk located.
    pub const fn new(reason: &'static str) -> Self {
        Self { reason, stop: None }
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
        }
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
        }
    }
}
