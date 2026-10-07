//! Instance pointers: global words that each hold the address of one object, such as a null
//! object, and the vtable that the code which loads them gives that object.
//!
//! The engine sets such a word at run time, so the executable does not hold the object. The pass
//! proves the object's vtable from the code that writes it instead. Each function that loads the
//! word and forms a vtable address point runs with the word holding a scratch object. Every
//! returned path must leave the word holding that object, or null, and either leave the object's
//! first word unwritten or store one address point there. A search that stops only at its bounds
//! with no such contradiction stands.
//!
//! The pass assumes that only the code that loads an instance pointer and forms an address point
//! gives its object a vtable, that the object keeps that vtable once set, and that a store
//! through an unknown address writes neither the pointer nor the vtable word. Calls in that code
//! are not followed.
use std::collections::{BTreeMap, BTreeSet};

use crate::engine::analysis::decode::Instruction;
use crate::engine::analysis::evaluate::{
    Call, Code, DATA_OBJECT_BASE, Exit, Machine, ReadOnlyData,
};

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
    /// The pointer still holds the object, whose vtable word holds this address point.
    Establishes(u64),
    /// Anything else, such as a replaced pointer or another value in the vtable word.
    Invalid,
}

/// The vtable address point of each instance pointer that its `writers` prove: the code of the
/// functions that load the pointer and form an address point, each with its rows in address
/// order. `points` holds the address points that the writers may store, and `data` the bytes
/// that the writers read, with the pointers that the loader rebases.
pub fn instance_vtables(
    writers: &BTreeMap<u64, Vec<Vec<Instruction>>>,
    points: &BTreeSet<u64>,
    data: &ReadOnlyData,
) -> BTreeMap<u64, u64> {
    writers
        .iter()
        .filter_map(|(&pointer, functions)| {
            let outcomes: Vec<Outcome> = functions
                .iter()
                .flat_map(|rows| outcomes(pointer, rows, points, data))
                .collect();

            proven_point(&outcomes).map(|point| (pointer, point))
        })
        .collect()
}

/// The address point that `outcomes` agree on: none is invalid, at least one establishes a
/// point, and every one that establishes gives the same point.
fn proven_point(outcomes: &[Outcome]) -> Option<u64> {
    let mut proven = None;
    for outcome in outcomes {
        match (*outcome, proven) {
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
/// outcome of each path. A path that stops before it returns is invalid, unless it stopped at a
/// bound of the search; a trap ends a path that the engine never completes.
fn outcomes(
    pointer: u64,
    rows: &[Instruction],
    points: &BTreeSet<u64>,
    data: &ReadOnlyData,
) -> Vec<Outcome> {
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
        .run_paths(entry, &mut |_, _| Ok(Call::Return(None)))
        .iter()
        .filter_map(|path| match &path.end {
            Ok(Exit::Returned) => Some(outcome(&path.machine, pointer, object, points)),
            Ok(Exit::Trapped) => None,
            Err(unresolved) if unresolved.is_bound() => None,
            _ => Some(Outcome::Invalid),
        })
        .collect()
}

/// A path that clears the pointer ends the object's life, as a null object's destructor does;
/// no virtual call runs through a null pointer.
fn outcome(machine: &Machine<'_>, pointer: u64, object: u64, points: &BTreeSet<u64>) -> Outcome {
    match machine.read(pointer, 8) {
        Some(0) => return Outcome::NoVtable,
        Some(held) if held == object => {}
        _ => return Outcome::Invalid,
    }
    if !machine.has_written(object, 8) {
        return Outcome::NoVtable;
    }

    match machine.read(object, 8) {
        Some(point) if points.contains(&point) => Outcome::Establishes(point),
        _ => Outcome::Invalid,
    }
}

#[cfg(test)]
mod tests;
