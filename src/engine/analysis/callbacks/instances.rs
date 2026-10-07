//! Instance pointers: global words that each hold the address of one object, such as a null
//! object, and the vtable that the code which loads them gives that object.
//!
//! The engine sets such a word at run time, so the executable does not hold the object. The pass
//! proves the object's first word from the code that writes it instead. Of the functions that load
//! the word and form an address on a vtable's page, a forward pass over each one's branches finds
//! those that may store through the word or its object ([`written_slots`]). Each of those runs
//! with the word holding a scratch object. Every path must return, leave the word holding that
//! object or null, and either leave the object's first word unwritten or store one known value
//! there; no call may receive the object or the word after that store. The binding then keeps a
//! value that is a vtable address point.
//!
//! The pass assumes that only the code that loads an instance pointer and forms an address on a
//! vtable's page gives its object a vtable, that the object keeps that vtable once set, and that
//! a store through an unknown address writes neither the pointer nor the vtable word. Calls in
//! that code are not followed.
use std::collections::{BTreeMap, BTreeSet};

use crate::engine::analysis::declarations::number;
use crate::engine::analysis::decode::{Instruction, general_register, written_registers};
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

/// A function that loads an instance pointer's slot and forms an address on a vtable's page.
#[derive(Debug, Clone)]
pub struct Loader {
    /// Its rows in address order, or `None` when it does not decode.
    pub rows: Option<Vec<Instruction>>,
    /// The pointer slots that it loads.
    pub slots: BTreeSet<u64>,
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

/// The first word that the `loaders` of each instance pointer prove for its object. A loader
/// that may write through a slot is a writer of the pointer that the slot names (`pointers`); a
/// loader that does not decode rejects every pointer that it loads. `data` holds the bytes that
/// the writers read, with the pointers that the loader rebases.
pub fn instance_vtables(
    loaders: &[Loader],
    pointers: &BTreeMap<u64, u64>,
    data: &ReadOnlyData,
) -> BTreeMap<u64, u64> {
    let mut writers: BTreeMap<u64, Vec<&[Instruction]>> = BTreeMap::new();
    let mut undecoded = BTreeSet::new();
    for loader in loaders {
        let Some(rows) = &loader.rows else {
            undecoded.extend(loader.slots.iter().map(|slot| pointers[slot]));
            continue;
        };
        for slot in written_slots(rows, pointers).intersection(&loader.slots) {
            writers.entry(pointers[slot]).or_default().push(rows);
        }
    }

    writers
        .into_iter()
        .filter(|(pointer, _)| !undecoded.contains(pointer))
        .filter_map(|(pointer, functions)| {
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

/// What a register holds on the way from a pointer slot to a virtual call.
#[derive(Debug, Clone, Copy)]
enum Held {
    /// An address that `adrp` gives.
    Page(u64),
    /// The address of the instance pointer that this pointer slot names.
    Slot(u64),
    /// The object that the instance pointer named by this slot holds.
    Object(u64),
    /// That object's vtable.
    Vtable(u64),
    /// A slot of that vtable.
    Target(u64),
}

impl Held {
    /// What a load at `offset` from this value holds.
    fn loaded(self, offset: u64, pointers: &BTreeMap<u64, u64>) -> Option<Self> {
        match self {
            Self::Page(page) => pointers
                .contains_key(&(page + offset))
                .then_some(Self::Slot(page + offset)),
            Self::Slot(slot) if offset == 0 => Some(Self::Object(slot)),
            Self::Object(slot) if offset == 0 => Some(Self::Vtable(slot)),
            Self::Vtable(slot) => Some(Self::Target(slot)),
            _ => None,
        }
    }
}

/// The pointer slots through which a virtual call in `rows` loads its receiver: `adrp` and
/// `ldr` load the slot, a load through it gives the object, a load through the object gives its
/// vtable, and a load from the vtable gives the target of a `blr`. The rows are read in address
/// order and a register keeps its value through other writes, so a slot may be wrong; the proof
/// decides.
pub fn virtual_call_slots(rows: &[Instruction], pointers: &BTreeMap<u64, u64>) -> BTreeSet<u64> {
    let mut held: BTreeMap<usize, Held> = BTreeMap::new();
    let mut slots = BTreeSet::new();
    for row in rows {
        let operands = row.operands.as_str();
        let next = match row.operation.as_str() {
            "adrp" => page(operands).map(|(destination, page)| (destination, Held::Page(page))),
            "ldr" => memory(operands).and_then(|(destination, base, offset)| {
                Some((destination, held.get(&base)?.loaded(offset, pointers)?))
            }),
            "blr" => {
                if let Some(Held::Target(slot)) =
                    general_register(operands).and_then(|r| held.get(&r))
                {
                    slots.insert(*slot);
                }
                None
            }
            _ => None,
        };
        if let Some((destination, value)) = next {
            held.insert(destination, value);
        }
    }
    slots
}

/// What a register may hold in [`written_slots`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Taint {
    Page(u64),
    /// The address of the instance pointer that this pointer slot names, or an address formed
    /// from it.
    Slot(u64),
    /// The object that the instance pointer named by this slot holds, or an address formed from
    /// it.
    Object(u64),
}

/// What each register may hold before an instruction.
type Taints = BTreeMap<usize, BTreeSet<Taint>>;

/// The pointer slots through which `rows` may write the instance pointer or its object: a store
/// whose base or stored register may hold the pointer's address or its object, and a call that
/// may receive the pointer's address. A forward pass over the function's branches keeps, before
/// each instruction, every value that some path may leave in a register; a branch through a
/// register may go to any instruction. A value that passes through memory is not followed.
fn written_slots(rows: &[Instruction], pointers: &BTreeMap<u64, u64>) -> BTreeSet<u64> {
    let index: BTreeMap<u64, usize> = rows
        .iter()
        .enumerate()
        .map(|(position, row)| (row.address, position))
        .collect();
    let mut before: Vec<Option<Taints>> = vec![None; rows.len()];
    let mut pending = Vec::new();
    if !rows.is_empty() {
        before[0] = Some(Taints::new());
        pending.push(0);
    }

    let mut written = BTreeSet::new();
    while let Some(position) = pending.pop() {
        let row = &rows[position];
        let mut taints = before[position].clone().unwrap_or_default();
        written.extend(slots_written_by(row, &taints));
        apply(row, &mut taints, pointers);

        for next in successors(row, position, &index, rows.len()) {
            let reached = before[next].is_some();
            let known = before[next].get_or_insert_with(Taints::new);
            let mut grew = false;
            for (register, values) in &taints {
                let held = known.entry(*register).or_default();
                for value in values {
                    grew |= held.insert(*value);
                }
            }
            if grew || !reached {
                pending.push(next);
            }
        }
    }
    written
}

/// The slots that `row` may write through, given what the registers may hold before it.
fn slots_written_by(row: &Instruction, taints: &Taints) -> Vec<u64> {
    let held = |register: usize, slot_only: bool| {
        taints
            .get(&register)
            .into_iter()
            .flatten()
            .filter_map(move |taint| match taint {
                Taint::Slot(slot) => Some(*slot),
                Taint::Object(slot) if !slot_only => Some(*slot),
                _ => None,
            })
    };

    if row.operation.starts_with("st") {
        return stored_registers(&row.operands)
            .flat_map(|register| held(register, false).collect::<Vec<_>>())
            .collect();
    }
    if matches!(row.operation.as_str(), "bl" | "blr") {
        return (0..=8)
            .flat_map(|register| held(register, true).collect::<Vec<_>>())
            .collect();
    }
    Vec::new()
}

/// Update what the registers may hold after `row`.
fn apply(row: &Instruction, taints: &mut Taints, pointers: &BTreeMap<u64, u64>) {
    let operands = row.operands.as_str();
    let produced: Option<(usize, BTreeSet<Taint>)> = match row.operation.as_str() {
        "adrp" => {
            page(operands).map(|(destination, page)| (destination, [Taint::Page(page)].into()))
        }
        "ldr" | "ldur" => memory(operands).map(|(destination, base, offset)| {
            let loaded = taints
                .get(&base)
                .into_iter()
                .flatten()
                .filter_map(|taint| match taint {
                    Taint::Page(page) => pointers
                        .contains_key(&(page + offset))
                        .then_some(Taint::Slot(page + offset)),
                    Taint::Slot(slot) if offset == 0 => Some(Taint::Object(*slot)),
                    _ => None,
                });
            (destination, loaded.collect())
        }),
        "mov" | "add" | "sub" => {
            let mut parts = operands.split(',');
            let destination = parts.next().and_then(general_register);
            let source = parts.next().and_then(general_register);
            destination.zip(source).map(|(destination, source)| {
                let carried = taints.get(&source).into_iter().flatten().copied();
                (
                    destination,
                    carried
                        .filter(|taint| !matches!(taint, Taint::Page(_)))
                        .collect(),
                )
            })
        }
        _ => None,
    };

    for register in written_registers(&row.operation, operands) {
        taints.remove(&register);
    }
    if let Some((destination, values)) = produced
        && !values.is_empty()
    {
        taints.insert(destination, values);
    }
}

/// The positions of the instructions that can run after the one at `position`.
fn successors(
    row: &Instruction,
    position: usize,
    index: &BTreeMap<u64, usize>,
    count: usize,
) -> Vec<usize> {
    let operation = row.operation.as_str();
    let target = row
        .operands
        .rsplit(',')
        .next()
        .and_then(number)
        .and_then(|target| index.get(&target).copied());
    let next = (position + 1 < count).then_some(position + 1);

    match operation {
        "ret" => Vec::new(),
        "br" => (0..count).collect(),
        "b" => target.into_iter().collect(),
        _ if operation.starts_with("b.")
            || matches!(operation, "cbz" | "cbnz" | "tbz" | "tbnz") =>
        {
            target.into_iter().chain(next).collect()
        }
        _ => next.into_iter().collect(),
    }
}

/// The base and the stored registers of a store's operands, such as `x8,x9,[x19,#0x10]!`.
fn stored_registers(operands: &str) -> impl Iterator<Item = usize> + '_ {
    operands
        .split(',')
        .map(|part| part.trim_start_matches('[').trim_end_matches(['!', ']']))
        .filter_map(general_register)
}

/// The destination and page of `adrp` operands such as `x8,#0x102ffd000`.
fn page(operands: &str) -> Option<(usize, u64)> {
    let (destination, page) = operands.split_once(',')?;
    Some((general_register(destination)?, number(page)?))
}

/// The destination, base and offset of `ldr` operands with an immediate offset and no
/// write-back, such as `x8,[x8,#0x248]` or `x20,[x8]`.
fn memory(operands: &str) -> Option<(usize, usize, u64)> {
    let (destination, memory) = operands.split_once(',')?;
    let inner = memory.strip_prefix('[')?.strip_suffix(']')?;
    let (base, offset) = match inner.split_once(',') {
        Some((base, offset)) => (base, number(offset)?),
        None => (inner, 0),
    };
    Some((
        general_register(destination)?,
        general_register(base)?,
        offset,
    ))
}

#[cfg(test)]
mod tests;
