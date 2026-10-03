//! The storage route of a flag accessor for one scope type.
//!
//! An accessor gets the flags of the scope that a command runs in. The route is found by running
//! the accessor with a stand-in scope object whose type field holds the scope type and whose
//! other contents are unknown, so a route that every path agrees on holds for every scope object
//! of that type.
//!
//! - A call that receives the scope object gives a terminal stand-in. A route that returns it,
//!   plus a constant, is owned by the scope through that terminal. The terminal's own route is
//!   then resolved in turn: a forwarding wrapper such as `mov x0, x1; b F` normalizes to `F`'s
//!   route. When the terminal's route is not established, the terminal itself is the route; the
//!   same function with the same scope type gives the same store.
//! - Each 64-bit value that the code loads from a fixed address holds a stand-in pointer. A
//!   route that returns it, plus a constant, is that global store, whatever the calling scope.
//!   A pointer slot keeps the address that it holds, so a global reached through the global
//!   offset table is named by its own address.
//!
//! Byte loads stay unknown, so a guarded path, such as a logging check, is followed on both sides
//! and must agree. Any other returned value, or an unresolved path, gives no route.
use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::{Caller, stand_in_command};
use crate::engine::analysis::declarations::{Function, number};
use crate::engine::analysis::decode::{Instruction, decode_arm64};
use crate::engine::analysis::evaluate::{Call, Code, Exit, Machine, ReadOnlyData};
use crate::engine::analysis::stop::Unresolved;

/// How many terminals deep a route is normalized.
const DEPTH_LIMIT: usize = 4;

/// First stand-in value of a terminal's result.
const TERMINAL_BASE: u64 = 0x6000_0000_0000;

/// First stand-in value of a global.
const GLOBAL_BASE: u64 = 0x6800_0000_0000;

/// The distance between two stand-in values. The low bits of a returned value are its offset.
pub(super) const STAND_IN_STRIDE: u64 = 1 << 32;

/// Bytes of the stand-in scope object that the method tracks.
const SCOPE_SPAN: u64 = 0x1000;

/// Where an accessor's flags are stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Route {
    /// In the object that a function returns for the scope object.
    Scope {
        /// The function that receives the scope object.
        terminal: u64,
        /// The store's offset in the returned object.
        offset: u64,
    },
    /// In the object that a global points to, for every scope.
    Global {
        /// The global's address.
        address: u64,
        /// The store's offset in the object that the global points to.
        offset: u64,
    },
}

impl Route {
    /// This route moved `offset` further into its object.
    pub(super) fn plus(self, extra: u64) -> Self {
        match self {
            Self::Scope { terminal, offset } => Self::Scope {
                terminal,
                offset: offset + extra,
            },
            Self::Global { address, offset } => Self::Global {
                address,
                offset: offset + extra,
            },
        }
    }
}

/// The code and layout that route runs read.
pub(super) struct RouteInput<'a> {
    pub functions: &'a BTreeMap<u64, Function>,
    pub pointers: &'a BTreeMap<u64, u64>,
    pub data: &'a ReadOnlyData,
    pub scope_type_offset: u64,
}

/// Route runs, kept for every function, caller and scope type that a run reached. A kept run is
/// the function's own route, before its terminal is followed, so it holds at every depth.
pub(super) struct Routes<'a> {
    input: RouteInput<'a>,
    runs: HashMap<(u64, Caller, usize), Result<Route, Unresolved>>,
}

impl<'a> Routes<'a> {
    pub(super) fn new(input: RouteInput<'a>) -> Self {
        Self {
            input,
            runs: HashMap::new(),
        }
    }

    /// The route of `function` when `caller` calls it in a scope of the type with mask bit
    /// `scope`.
    pub(super) fn route(
        &mut self,
        function: u64,
        caller: Caller,
        scope: usize,
    ) -> Result<Route, Unresolved> {
        self.resolve(function, caller, scope, 0)
    }

    fn resolve(
        &mut self,
        function: u64,
        caller: Caller,
        scope: usize,
        depth: usize,
    ) -> Result<Route, Unresolved> {
        let route = self
            .runs
            .entry((function, caller, scope))
            .or_insert_with(|| run(&self.input, function, caller, scope))
            .clone()?;

        Ok(match route {
            Route::Scope { terminal, offset } if depth + 1 < DEPTH_LIMIT => self
                .resolve(terminal, Caller::Scope, scope, depth + 1)
                .map_or(route, |inner| inner.plus(offset)),
            route => route,
        })
    }
}

/// The one route that every returning path of `function` gives.
fn run(
    input: &RouteInput<'_>,
    function: u64,
    caller: Caller,
    scope: usize,
) -> Result<Route, Unresolved> {
    let body = input
        .functions
        .get(&function)
        .ok_or(Unresolved::new("accessor-body"))?;
    let rows =
        decode_arm64(&body.code, body.address).map_err(|_| Unresolved::new("accessor-code"))?;
    let loads = global_loads(&rows, input.pointers);
    let code = Code::from_rows(rows);
    let mut machine = Machine::new(&code, input.data);

    let scope_object = machine.reserve(SCOPE_SPAN);
    machine.write(
        scope_object + input.scope_type_offset,
        8,
        1u64.checked_shl(scope as u32).unwrap_or(0),
    );
    match caller {
        Caller::Command { vtable } => {
            let command = stand_in_command(&mut machine, vtable);
            machine.set_register(0, command);
            machine.set_register(1, scope_object);
        }
        Caller::Scope => machine.set_register(0, scope_object),
    }
    let globals = seed_globals(&mut machine, &loads, input.data);

    let mut terminals = Vec::<u64>::new();
    let paths = machine.run_paths(function, &mut |target, machine| match target {
        Some(target) if machine.register(0) == Some(scope_object) => {
            let index = position_or_push(&mut terminals, target);
            Ok(Call::Return(Some(TERMINAL_BASE + index * STAND_IN_STRIDE)))
        }
        _ => Ok(Call::Return(None)),
    });

    let mut routes = BTreeSet::new();
    for path in paths {
        match path.end? {
            Exit::Returned => {}
            Exit::Trapped => continue,
            _ => return Err(Unresolved::new("accessor-end")),
        }
        let value = path
            .machine
            .register(0)
            .ok_or(Unresolved::new("accessor-return"))?;
        let route = stand_in(value, TERMINAL_BASE, &terminals)
            .map(|(terminal, offset)| Route::Scope { terminal, offset })
            .or_else(|| {
                stand_in(value, GLOBAL_BASE, &globals)
                    .map(|(address, offset)| Route::Global { address, offset })
            })
            .ok_or(Unresolved::new("accessor-return"))?;
        routes.insert(route);
    }
    match (routes.pop_first(), routes.is_empty()) {
        (Some(route), true) => Ok(route),
        (None, _) => Err(Unresolved::new("accessor-return")),
        (Some(_), false) => Err(Unresolved::new("accessor-routes")),
    }
}

/// The index of `value` in `values`, added when absent.
pub(super) fn position_or_push(values: &mut Vec<u64>, value: u64) -> u64 {
    let index = values
        .iter()
        .position(|&known| known == value)
        .unwrap_or_else(|| {
            values.push(value);
            values.len() - 1
        });

    index as u64
}

/// The subject and offset of a stand-in value from `base`, when it names one of `subjects`.
pub(super) fn stand_in(value: u64, base: u64, subjects: &[u64]) -> Option<(u64, u64)> {
    let relative = value.checked_sub(base)?;
    let subject = subjects.get(usize::try_from(relative / STAND_IN_STRIDE).ok()?)?;

    Some((*subject, relative % STAND_IN_STRIDE))
}

/// Seed each global load that the data does not hold, such as a pointer slot's target, with its
/// stand-in pointer, and return the global addresses in stand-in order.
fn seed_globals(machine: &mut Machine<'_>, loads: &BTreeSet<u64>, data: &ReadOnlyData) -> Vec<u64> {
    let mut globals = Vec::new();
    for &address in loads {
        if data.read(address, 8).is_none() {
            let index = position_or_push(&mut globals, address);
            machine.write(address, 8, GLOBAL_BASE + index * STAND_IN_STRIDE);
        }
    }

    globals
}

/// The fixed addresses that `rows` load a 64-bit value from: an address that `adrp` with `add`
/// or a load offset forms, or that a loaded pointer slot holds.
fn global_loads(rows: &[Instruction], pointers: &BTreeMap<u64, u64>) -> BTreeSet<u64> {
    let mut addresses = BTreeMap::<String, u64>::new();
    let mut loads = BTreeSet::new();
    for row in rows {
        let operands: Vec<&str> = row.operands.split(',').collect();
        let written = operands
            .first()
            .map(|register| register.replacen('w', "x", 1));
        match (row.operation.as_str(), operands.as_slice()) {
            ("adrp", [destination, page]) => {
                if let Some(page) = number(page) {
                    addresses.insert((*destination).to_owned(), page);
                    continue;
                }
            }
            ("add", [destination, base, offset]) => {
                if let Some(address) = offset_from(&addresses, base, offset) {
                    addresses.insert((*destination).to_owned(), address);
                    continue;
                }
            }
            ("ldr", [destination, base, offset]) => {
                let base = base.trim_start_matches('[');
                let offset = offset.trim_end_matches(']');
                if let Some(address) = offset_from(&addresses, base, offset)
                    && destination.starts_with('x')
                {
                    loads.insert(address);
                    if let Some(&target) = pointers.get(&address) {
                        addresses.insert((*destination).to_owned(), target);
                        continue;
                    }
                }
            }
            ("ldr", [destination, base]) => {
                let base = base.trim_start_matches('[').trim_end_matches(']');
                if let Some(&address) = addresses.get(base)
                    && destination.starts_with('x')
                {
                    loads.insert(address);
                    if let Some(&target) = pointers.get(&address) {
                        addresses.insert((*destination).to_owned(), target);
                        continue;
                    }
                }
            }
            ("bl" | "blr", _) => {
                addresses.retain(|register, _| !caller_saved(register));
                continue;
            }
            _ => {}
        }
        if let Some(register) = written
            && !row.operation.starts_with("st")
        {
            addresses.remove(&register);
        }
    }

    loads
}

/// The address `base` plus the immediate `offset`, when `base` holds a formed address.
fn offset_from(addresses: &BTreeMap<String, u64>, base: &str, offset: &str) -> Option<u64> {
    addresses.get(base)?.checked_add(number(offset)?)
}

fn caller_saved(register: &str) -> bool {
    register
        .strip_prefix('x')
        .and_then(|index| index.parse::<u8>().ok())
        .is_some_and(|index| index <= 18)
}
