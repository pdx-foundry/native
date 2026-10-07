//! Instance pointers: global words that each hold the address of one object, such as a null
//! object, and the vtable that the code which loads them gives that object.
//!
//! The engine sets such a word at run time, so the executable does not hold the object. The pass
//! proves the object's first word from the code that writes it instead. Each function that loads
//! the word and forms an address on a vtable's page runs with the word holding a scratch object.
//! Every path must return, leave the word holding that object or null, and either leave the
//! object's first word unwritten or store one known value there; no call may receive the object
//! or the word after that store. The binding then keeps a value that is a vtable address point.
//!
//! The pass assumes that only the code that loads an instance pointer and forms an address on a
//! vtable's page gives its object a vtable, that the object keeps that vtable once set, and that
//! a store through an unknown address writes neither the pointer nor the vtable word. Calls in
//! that code are not followed.
use std::collections::BTreeMap;

use crate::engine::analysis::decode::Instruction;
use crate::engine::analysis::evaluate::{
    Call, Code, DATA_OBJECT_BASE, Exit, Machine, ReadOnlyData,
};
use crate::engine::analysis::stop::Unresolved;

/// How far apart [`with_objects`] places the objects, so that a load from one at an offset below
/// this never reads another's vtable word.
const OBJECT_SPACING: usize = 0x10_0000;

/// `data` with each instance pointer of `vtables` holding an object, at
/// [`DATA_OBJECT_BASE`] and above, whose first word is the pointer's vtable address point and
/// whose other words are unknown. A context run then reads a virtual call on that object from
/// `data`.
pub fn with_objects(data: &ReadOnlyData, vtables: &BTreeMap<u64, u64>) -> ReadOnlyData {
    let words: BTreeMap<u64, u64> = vtables
        .iter()
        .zip((DATA_OBJECT_BASE..).step_by(OBJECT_SPACING))
        .flat_map(|((&pointer, &point), object)| [(pointer, object), (object, point)])
        .collect();

    data.with_words(&words)
}

/// What one returned path of a writer leaves at the instance pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    /// The path gives the object no vtable: the pointer still holds the object and the path did
    /// not write its vtable word, or the path cleared the pointer.
    NoVtable,
    /// The pointer still holds the object, whose vtable word holds this value.
    Establishes(u64),
    /// Anything else, such as a replaced pointer or another value in the vtable word.
    Invalid,
}

/// The first word that its `writers` prove for the object of each instance pointer: the code of
/// the functions that load the pointer and form an address on a vtable's page, each with its rows
/// in address order. `data` holds the bytes that the writers read, with the pointers that the
/// loader rebases.
pub fn instance_vtables(
    writers: &BTreeMap<u64, Vec<Vec<Instruction>>>,
    data: &ReadOnlyData,
) -> BTreeMap<u64, u64> {
    writers
        .iter()
        .filter_map(|(&pointer, functions)| {
            let found = functions
                .iter()
                .flat_map(|rows| outcomes(pointer, rows, data));

            proven_point(found).map(|point| (pointer, point))
        })
        .collect()
}

/// The value that `outcomes` agree on: none is invalid, at least one establishes a value, and
/// every one that establishes gives the same value. It reads no outcome after the first that
/// rejects, so the writers after it do not run.
fn proven_point(outcomes: impl IntoIterator<Item = Outcome>) -> Option<u64> {
    let mut proven = None;
    for outcome in outcomes {
        match (outcome, proven) {
            (Outcome::Invalid, _) => return None,
            (Outcome::NoVtable, _) => {}
            (Outcome::Establishes(point), None) => proven = Some(point),
            (Outcome::Establishes(point), Some(known)) if point == known => {}
            (Outcome::Establishes(_), Some(_)) => return None,
        }
    }
    proven
}

/// Run one writer from its first row with `pointer` holding a scratch object, and give the
/// outcome of each path. A path that stops before it returns is invalid, also at a bound of the
/// search, which leaves its other paths unread; a trap ends a path that the engine never
/// completes.
fn outcomes(pointer: u64, rows: &[Instruction], data: &ReadOnlyData) -> Vec<Outcome> {
    let Some(entry) = rows.first().map(|row| row.address) else {
        return Vec::new();
    };
    let code = Code::from_rows(rows.iter().cloned());
    let mut machine = Machine::new(&code, data);
    let object = machine.reserve(8);
    machine.write(pointer, 8, object);
    machine.protect(pointer, 8);
    machine.protect(object, 8);

    machine
        .run_paths(entry, &mut |_, machine| {
            let receives = |value| (0..=8).any(|index| machine.register(index) == Some(value));
            if machine.has_written(object, 8) && (receives(object) || receives(pointer)) {
                return Err(Unresolved::new("instance-passed-on"));
            }
            Ok(Call::Return(None))
        })
        .iter()
        .filter_map(|path| match &path.end {
            Ok(Exit::Returned) => Some(outcome(&path.machine, pointer, object)),
            Ok(Exit::Trapped) => None,
            _ => Some(Outcome::Invalid),
        })
        .collect()
}

/// A path that clears the pointer ends the object's life, as a null object's destructor does;
/// no virtual call runs through a null pointer.
fn outcome(machine: &Machine<'_>, pointer: u64, object: u64) -> Outcome {
    match machine.read(pointer, 8) {
        Some(0) => return Outcome::NoVtable,
        Some(held) if held == object => {}
        _ => return Outcome::Invalid,
    }
    if !machine.has_written(object, 8) {
        return Outcome::NoVtable;
    }

    match machine.read(object, 8) {
        Some(value) => Outcome::Establishes(value),
        None => Outcome::Invalid,
    }
}

#[cfg(test)]
mod tests;
