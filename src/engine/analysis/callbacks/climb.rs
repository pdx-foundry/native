//! The caller climb, which the block method and the callback method share.
//!
//! A call that a method reads, such as an evaluation of a block or a call that fires an
//! on_action, can receive a scope that its function received as a parameter. That function is a
//! *wrapper*, and the climb runs the same test at each direct call to it, up to [`CALLER_DEPTH`]
//! callers. A call that does not pass a parameter on is an *entry*: the context pass runs the
//! entry's function to the entry call and then through it, with each wrapper on the way.
//!
//! Each method gives the climb a *payload* for each wrapper: what the wrapper reaches, such as the
//! blocks that it evaluates or the call sites that it holds. An entry carries the payload of the
//! wrappers that it calls, and the climb charges each payload whose callers it could not follow.
use std::collections::{BTreeMap, BTreeSet};

use super::contexts::{Read, Selected};
use super::names::{self, Fact, State, StringFunctions};
use crate::engine::analysis::declarations::number;
use crate::engine::analysis::decode::Instruction;
use crate::engine::analysis::evaluate::Code;

/// How many callers up the climb follows a scope that wrappers pass on.
pub const CALLER_DEPTH: usize = 2;

/// A direct call to a function.
#[derive(Debug, Clone, Copy)]
pub struct CallSite {
    /// The call instruction.
    pub address: u64,
    /// The start of the function that holds it.
    pub function: u64,
}

/// The decoded code that the climb and the runs from its entries read.
pub(super) struct Decoded<'a> {
    /// Each function that holds a read call, and its direct callers up to [`CALLER_DEPTH`] calls
    /// away.
    pub functions: &'a BTreeMap<u64, Vec<Instruction>>,
    /// The direct calls to each function that holds a read call, and to its callers short of
    /// [`CALLER_DEPTH`].
    pub callers: &'a BTreeMap<u64, Vec<CallSite>>,
    /// The scope functions that the context pass runs.
    pub scope_code: &'a [Instruction],
    pub strings: &'a StringFunctions,
    /// The offset `k` of each function whose whole body is `add x0, x0, #k; ret`, by its address.
    pub offset_getters: &'a BTreeMap<u64, i64>,
}

/// A function that passes its scope parameter `parameter` on to a read call.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Wrapper {
    pub function: u64,
    pub parameter: usize,
}

/// A call from which the context pass reads the calls that it reaches.
pub(super) struct Entry<T> {
    /// The start of the function that holds the call.
    pub function: u64,
    /// The call.
    pub site: u64,
    pub selected: Selected,
    /// The payload that the entry reaches, which an unresolved run is charged to.
    pub reaches: BTreeSet<T>,
}

impl<T: Ord> Entry<T> {
    /// The calls of `calls` that read a part of the payload that this entry reaches, with their
    /// reads. A run from the entry reads only these, so a call that the run passes on its way adds
    /// nothing to a payload that another entry carries.
    pub fn reads(&self, calls: &BTreeMap<u64, (T, Read)>) -> BTreeMap<u64, Read> {
        calls
            .iter()
            .filter(|(_, (reached, _))| self.reaches.contains(reached))
            .map(|(&address, (_, read))| (address, *read))
            .collect()
    }
}

/// What the climb found.
pub(super) struct Climb<T> {
    /// Each call that passes a scope that is not a parameter to a wrapper.
    pub entries: Vec<Entry<T>>,
    /// Each part of a payload whose callers the climb could not follow, with the reason.
    pub charges: Vec<(T, &'static str)>,
    /// The function of every wrapper found, at every depth.
    pub wrappers: BTreeSet<u64>,
}

impl<T: Clone> Climb<T> {
    fn charge(&mut self, reaches: &BTreeSet<T>, reason: &'static str) {
        self.charges
            .extend(reaches.iter().map(|reached| (reached.clone(), reason)));
    }
}

/// Follow each wrapper's scope parameter up its direct callers, to [`CALLER_DEPTH`] callers. Each
/// call that does not pass a parameter on becomes an entry with the payload of the wrapper that it
/// calls. A wrapper with no caller charges `no-caller`, a caller that is not decoded charges
/// `caller-not-decoded`, and a wrapper past the depth charges `caller-depth`.
pub(super) fn climb<T: Ord + Clone>(
    decoded: &Decoded<'_>,
    wrappers: BTreeMap<Wrapper, BTreeSet<T>>,
) -> Climb<T> {
    let mut climb = Climb {
        entries: Vec::new(),
        charges: Vec::new(),
        wrappers: BTreeSet::new(),
    };
    let mut level = wrappers;
    for depth in 1..=CALLER_DEPTH + 1 {
        climb
            .wrappers
            .extend(level.keys().map(|wrapper| wrapper.function));
        if depth > CALLER_DEPTH {
            for reaches in level.values() {
                climb.charge(reaches, "caller-depth");
            }
            break;
        }

        let callers =
            |wrapper: &Wrapper| decoded.callers.get(&wrapper.function).into_iter().flatten();
        for (wrapper, reaches) in &level {
            if callers(wrapper).next().is_none() {
                climb.charge(reaches, "no-caller");
            }
        }
        let calls: Vec<(&Wrapper, &BTreeSet<T>, CallSite)> = level
            .iter()
            .flat_map(|(wrapper, reaches)| {
                callers(wrapper).map(move |call| (wrapper, reaches, *call))
            })
            .collect();
        let states = states_at(
            decoded.functions,
            decoded.strings,
            decoded.offset_getters,
            calls
                .iter()
                .map(|(_, _, call)| (call.function, call.address)),
        );

        let mut next: BTreeMap<Wrapper, BTreeSet<T>> = BTreeMap::new();
        for (wrapper, reaches, call) in calls {
            let Some(state) = states.get(&call.address) else {
                climb.charge(reaches, "caller-not-decoded");
                continue;
            };
            match parameter(state, wrapper.parameter) {
                Some(parameter) => {
                    let caller = Wrapper {
                        function: call.function,
                        parameter,
                    };
                    next.entry(caller)
                        .or_default()
                        .extend(reaches.iter().cloned());
                }
                None => climb.entries.push(Entry {
                    function: call.function,
                    site: call.address,
                    selected: Selected::Wrapper(wrapper.function),
                    reaches: reaches.clone(),
                }),
            }
        }
        level = next;
    }
    climb
}

/// The name-pass state before each `(function, address)`, for the functions that are decoded.
pub(super) fn states_at(
    functions: &BTreeMap<u64, Vec<Instruction>>,
    strings: &StringFunctions,
    offset_getters: &BTreeMap<u64, i64>,
    sites: impl Iterator<Item = (u64, u64)>,
) -> BTreeMap<u64, State> {
    let mut by_function: BTreeMap<u64, BTreeSet<u64>> = BTreeMap::new();
    for (function, address) in sites {
        by_function.entry(function).or_default().insert(address);
    }

    let mut states = BTreeMap::new();
    for (function, addresses) in by_function {
        let Some(rows) = functions.get(&function) else {
            continue;
        };
        names::each_state(rows, strings, offset_getters, |row, state| {
            if addresses.contains(&row.address) {
                states.insert(row.address, state.clone());
            }
        });
    }
    states
}

/// The parameter that register `register` holds on every path, unchanged.
pub(super) fn parameter(state: &State, register: usize) -> Option<usize> {
    match names::sole_fact(state.register(register)) {
        Some(Fact::Argument(parameter, 0)) => Some(parameter),
        _ => None,
    }
}

/// The code of `function`, with every function in `entered` that it reaches by direct calls and
/// the scope functions, or `None` when `function` is not decoded.
pub(super) fn entry_code(
    decoded: &Decoded<'_>,
    entered: &BTreeSet<u64>,
    function: u64,
) -> Option<Code> {
    let mut included = BTreeSet::from([function]);
    let mut pending = vec![function];
    while let Some(next) = pending.pop() {
        let Some(rows) = decoded.functions.get(&next) else {
            if next == function {
                return None;
            }
            continue;
        };
        let called = rows
            .iter()
            .filter(|row| matches!(row.operation.as_str(), "bl" | "b"))
            .filter_map(|row| number(&row.operands))
            .filter(|target| entered.contains(target));
        for callee in called {
            if included.insert(callee) {
                pending.push(callee);
            }
        }
    }

    let rows = included
        .iter()
        .filter_map(|function| decoded.functions.get(function))
        .flatten()
        .chain(decoded.scope_code)
        .cloned();
    Some(Code::from_rows(rows))
}
