//! Read the input of the instance-pointer proof (`engine::analysis::callbacks::instances`) for
//! the virtual calls in decoded functions, and the words that a context run needs to follow such
//! a call.
use std::collections::{BTreeMap, BTreeSet};

use crate::AnalysisError;
use crate::engine::analysis::callbacks::instances::instance_vtables;
use crate::engine::analysis::declarations::number;
use crate::engine::analysis::decode::{Instruction, adrp, decode_arm64, ldr_immediate};
use crate::engine::analysis::discovery::Symbol;

use super::declarations::Text;
use super::families::{point_class, register, written_registers};
use super::references::Image;

/// How many words after an `adrp` a load of its page is looked for.
const LOAD_WINDOW: usize = 4;

/// The proven instance pointers that virtual calls in some decoded functions go through.
#[derive(Debug, Default)]
pub(super) struct Instances {
    /// The vtable address point of the object that each proven instance pointer holds.
    pub vtables: BTreeMap<u64, u64>,
    /// The loaded words that such a virtual call reads: the pointer slot that names the
    /// instance pointer, and the slots of the object's vtable group.
    pub words: BTreeMap<u64, u64>,
}

/// Prove the instance pointers that virtual calls in `functions` go through, and give the words
/// that a context run reads to follow those calls.
pub(super) fn instances(
    text: &Text,
    image: &Image<'_>,
    bound_slots: &BTreeSet<u64>,
    functions: &BTreeMap<u64, Vec<Instruction>>,
) -> Result<Instances, AnalysisError> {
    let slots: BTreeSet<u64> = functions
        .values()
        .flat_map(|rows| virtual_call_slots(rows, image.pointers))
        .collect();
    if slots.is_empty() {
        return Ok(Instances::default());
    }

    let mut writers: BTreeMap<u64, Vec<Vec<Instruction>>> = BTreeMap::new();
    let mut undecoded = BTreeSet::new();
    for (start, loaded) in possible_writers(text, image.symbols, &slots) {
        let decoded = text
            .function(start)
            .ok()
            .and_then(|(address, code)| decode_arm64(code, address).ok());
        let Some(rows) = decoded else {
            undecoded.extend(loaded.iter().map(|slot| image.pointers[slot]));
            continue;
        };
        for slot in written_slots(&rows, image.pointers).intersection(&loaded) {
            writers
                .entry(image.pointers[slot])
                .or_default()
                .push(rows.clone());
        }
    }

    let data = super::language::constant_data(image.bytes, image.pointers, bound_slots)?;
    let vtables: BTreeMap<u64, u64> = instance_vtables(&writers, &data)
        .into_iter()
        .filter(|(pointer, point)| {
            !undecoded.contains(pointer) && point_class(image.symbols, &data, *point).is_some()
        })
        .collect();

    let mut words = BTreeMap::new();
    for slot in slots {
        let pointer = image.pointers[&slot];
        if let Some(&point) = vtables.get(&pointer) {
            words.insert(slot, pointer);
            words.extend(vtable_slots(image, point));
        }
    }

    Ok(Instances { vtables, words })
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
fn virtual_call_slots(rows: &[Instruction], pointers: &BTreeMap<u64, u64>) -> BTreeSet<u64> {
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
                if let Some(Held::Target(slot)) = register(operands).and_then(|r| held.get(&r)) {
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
            let destination = parts.next().and_then(register);
            let source = parts.next().and_then(register);
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
        .filter_map(register)
}

/// The functions that may write a vtable through one of `slots`, each with the slots that it
/// loads: each loads a slot with `adrp` and a following `ldr`, and has an `adrp` of a page that
/// holds a vtable.
fn possible_writers(
    text: &Text,
    symbols: &[Symbol],
    slots: &BTreeSet<u64>,
) -> BTreeMap<u64, BTreeSet<u64>> {
    let vtable_pages = vtable_pages(symbols);
    let words: Vec<u32> = text
        .code
        .as_chunks::<4>()
        .0
        .iter()
        .map(|word| u32::from_le_bytes(*word))
        .collect();

    let mut loading: BTreeMap<u64, BTreeSet<u64>> = BTreeMap::new();
    let mut forming = BTreeSet::new();
    for (index, &word) in words.iter().enumerate() {
        let at = text.address + index as u64 * 4;
        let Some((page_register, page)) = adrp(word, at) else {
            continue;
        };
        let Some(&function) = text.starts.range(..=at).next_back() else {
            continue;
        };
        if vtable_pages.contains(&page) {
            forming.insert(function);
        }
        let loaded = words[index + 1..]
            .iter()
            .take(LOAD_WINDOW)
            .filter_map(|&word| ldr_immediate(word))
            .filter(|&(_, base, _)| base == page_register)
            .map(|(_, _, offset)| page + offset)
            .filter(|slot| slots.contains(slot));
        loading.entry(function).or_default().extend(loaded);
    }

    loading.retain(|function, loaded| !loaded.is_empty() && forming.contains(function));
    loading
}

/// The pages that the vtable groups cover, each from its `vtable for` symbol to the next symbol.
fn vtable_pages(symbols: &[Symbol]) -> BTreeSet<u64> {
    let mut starts: Vec<(u64, bool)> = symbols
        .iter()
        .map(|symbol| (symbol.address, symbol.name.starts_with("vtable for ")))
        .collect();
    starts.sort_unstable();

    starts
        .windows(2)
        .filter(|pair| pair[0].1 && pair[1].0 > pair[0].0)
        .flat_map(|pair| (pair[0].0 & !0xfff..pair[1].0).step_by(0x1000))
        .collect()
}

/// The rebased pointers in the vtable group that holds `point`, from the `vtable for` symbol at
/// or below it to the next symbol.
fn vtable_slots(image: &Image<'_>, point: u64) -> BTreeMap<u64, u64> {
    let start = image
        .symbols
        .iter()
        .filter(|symbol| symbol.address <= point && symbol.name.starts_with("vtable for "))
        .map(|symbol| symbol.address)
        .max();
    let end = image
        .symbols
        .iter()
        .map(|symbol| symbol.address)
        .filter(|&address| address > point)
        .min()
        .unwrap_or(u64::MAX);
    let Some(start) = start else {
        return BTreeMap::new();
    };

    image
        .pointers
        .range(start..end)
        .map(|(&slot, &target)| (slot, target))
        .collect()
}

/// The destination and page of `adrp` operands such as `x8,#0x102ffd000`.
fn page(operands: &str) -> Option<(usize, u64)> {
    let (destination, page) = operands.split_once(',')?;
    Some((register(destination)?, number(page)?))
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
    Some((register(destination)?, register(base)?, offset))
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::{virtual_call_slots, written_slots};
    use crate::engine::analysis::decode::Instruction;

    fn rows(lines: &[(u64, &str, &str)]) -> Vec<Instruction> {
        lines
            .iter()
            .map(|(address, operation, operands)| Instruction {
                address: *address,
                bytes: [0; 4],
                operation: (*operation).into(),
                operands: (*operands).into(),
            })
            .collect()
    }

    /// The slot at 0x7248 names the instance pointer at 0x9d50.
    fn pointers() -> BTreeMap<u64, u64> {
        BTreeMap::from([(0x7248, 0x9d50)])
    }

    /// `CTraditionType::GetUnlocksAgenda` on M452: the null swap's virtual call after the loop,
    /// with the loop's `mov x20,x22` between the receiver's load and the call.
    #[test]
    fn a_virtual_call_through_an_instance_pointer_names_its_slot() {
        let unlocks = rows(&[
            (0x1000, "adrp", "x8,#0x7000"),
            (0x1004, "ldr", "x8,[x8,#0x248]"),
            (0x1008, "ldr", "x20,[x8]"),
            (0x100c, "ldr", "w8,[x0,#0x5dc]"),
            (0x1010, "b.lt", "#0x1020"),
            (0x1014, "mov", "x20,x22"), // a swap that the loop selected
            (0x1018, "b", "#0x1010"),
            (0x101c, "nop", ""),
            (0x1020, "ldr", "x8,[x20]"),
            (0x1024, "ldr", "x8,[x8,#0x40]"),
            (0x1028, "mov", "x0,x20"),
            (0x102c, "blr", "x8"),
            (0x1030, "ret", ""),
        ]);

        assert_eq!(
            virtual_call_slots(&unlocks, &pointers()),
            BTreeSet::from([0x7248])
        );
    }

    #[test]
    fn a_virtual_call_through_an_argument_names_no_slot() {
        let call = rows(&[
            (0x1000, "ldr", "x8,[x0]"),
            (0x1004, "ldr", "x8,[x8,#0x40]"),
            (0x1008, "blr", "x8"),
            (0x100c, "ret", ""),
        ]);

        assert!(virtual_call_slots(&call, &pointers()).is_empty());
    }

    /// Loads the instance pointer's object into `x20`, runs `middle` from 0x100c, and stores
    /// `x9` at the address in `x20`.
    fn stores_through_x20(middle: &[(&'static str, &'static str)]) -> Vec<Instruction> {
        let mut lines = vec![
            (0, "adrp", "x8,#0x7000"),
            (0, "ldr", "x8,[x8,#0x248]"),
            (0, "ldr", "x20,[x8]"),
        ];
        lines.extend(
            middle
                .iter()
                .map(|(operation, operands)| (0, *operation, *operands)),
        );
        lines.extend([(0, "str", "x9,[x20]"), (0, "ret", "")]);
        for (index, line) in lines.iter_mut().enumerate() {
            line.0 = 0x1000 + 4 * index as u64;
        }
        rows(&lines)
    }

    #[test]
    fn a_register_written_on_every_path_no_longer_holds_the_object() {
        let reused = stores_through_x20(&[("ldr", "w20,[x22,#0x5dc]")]);

        assert!(written_slots(&reused, &pointers()).is_empty());
    }

    #[test]
    fn a_register_written_on_one_path_may_still_hold_the_object() {
        let around = stores_through_x20(&[
            ("cbz", "x1,#0x1014"),
            ("mov", "x20,x22"), // only when x1 is not zero
        ]);

        assert_eq!(
            written_slots(&around, &pointers()),
            BTreeSet::from([0x7248])
        );
    }

    #[test]
    fn a_store_to_the_pointer_and_a_call_that_receives_it_write_through_the_slot() {
        let clears = rows(&[
            (0x1000, "adrp", "x8,#0x7000"),
            (0x1004, "ldr", "x8,[x8,#0x248]"),
            (0x1008, "str", "xzr,[x8]"),
            (0x100c, "ret", ""),
        ]);
        let passes = rows(&[
            (0x1000, "adrp", "x0,#0x7000"),
            (0x1004, "ldr", "x0,[x0,#0x248]"),
            (0x1008, "bl", "#0x9900"),
            (0x100c, "ret", ""),
        ]);

        assert_eq!(
            written_slots(&clears, &pointers()),
            BTreeSet::from([0x7248])
        );
        assert_eq!(
            written_slots(&passes, &pointers()),
            BTreeSet::from([0x7248])
        );
    }
}
