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
use super::families::{point_class, register};
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
        .flat_map(|rows| uses(rows, image.pointers).virtual_calls)
        .collect();
    if slots.is_empty() {
        return Ok(Instances::default());
    }

    let stores: Vec<_> = possible_writers(text, image.symbols, &slots)
        .into_iter()
        .filter_map(|start| {
            let (address, code) = text.function(start).ok()?;
            let rows = decode_arm64(code, address).ok()?;
            let stores: BTreeSet<(u64, u64)> = uses(&rows, image.pointers)
                .vtable_stores
                .into_iter()
                .filter(|(slot, _)| slots.contains(slot))
                .collect();
            (!stores.is_empty()).then_some((rows, stores))
        })
        .collect();

    let data = super::language::constant_data(image.bytes, image.pointers, bound_slots)?;
    let points: BTreeSet<u64> = stores
        .iter()
        .flat_map(|(_, stores)| stores.iter().map(|&(_, stored)| stored))
        .filter(|&stored| point_class(image.symbols, &data, stored).is_some())
        .collect();

    let mut writers: BTreeMap<u64, Vec<Vec<Instruction>>> = BTreeMap::new();
    for (rows, stores) in stores {
        let written: BTreeSet<u64> = stores
            .into_iter()
            .filter(|(_, stored)| points.contains(stored))
            .map(|(slot, _)| image.pointers[&slot])
            .collect();
        for pointer in written {
            writers.entry(pointer).or_default().push(rows.clone());
        }
    }
    let vtables = instance_vtables(&writers, &points, &data);

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

/// What a register holds on the way from a pointer slot to a virtual call or a vtable store.
#[derive(Debug, Clone, Copy)]
enum Held {
    /// An address that `adrp` and `add` form.
    Address(u64),
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
            Self::Address(address) => pointers
                .contains_key(&(address + offset))
                .then_some(Self::Slot(address + offset)),
            Self::Slot(slot) if offset == 0 => Some(Self::Object(slot)),
            Self::Object(slot) if offset == 0 => Some(Self::Vtable(slot)),
            Self::Vtable(slot) => Some(Self::Target(slot)),
            _ => None,
        }
    }
}

/// What one function does through pointer slots.
#[derive(Debug, Default, PartialEq, Eq)]
struct Uses {
    /// The slots through which a virtual call loads its receiver: a load through the slot gives
    /// the object, a load through the object its vtable, and a load from the vtable the target
    /// of a `blr`.
    virtual_calls: BTreeSet<u64>,
    /// Each slot through which an address that the function forms is stored at word 0 of the
    /// object, with that address.
    vtable_stores: BTreeSet<(u64, u64)>,
}

/// Read `rows` in address order. A register keeps its value through other writes, so a use may
/// be wrong; the proof decides.
fn uses(rows: &[Instruction], pointers: &BTreeMap<u64, u64>) -> Uses {
    let mut held: BTreeMap<usize, Held> = BTreeMap::new();
    let mut found = Uses::default();
    for row in rows {
        let operands = row.operands.as_str();
        let next = match row.operation.as_str() {
            "adrp" => page(operands).map(|(destination, page)| (destination, Held::Address(page))),
            "add" => added(operands).and_then(|(destination, source, addend)| {
                match held.get(&source)? {
                    Held::Address(address) => Some((destination, Held::Address(address + addend))),
                    _ => None,
                }
            }),
            "ldr" => memory(operands).and_then(|(destination, base, offset)| {
                Some((destination, held.get(&base)?.loaded(offset, pointers)?))
            }),
            "str" => {
                if let Some((source, base, 0)) = memory(operands)
                    && let (Some(Held::Address(stored)), Some(Held::Object(slot))) =
                        (held.get(&source), held.get(&base))
                {
                    found.vtable_stores.insert((*slot, *stored));
                }
                None
            }
            "blr" => {
                if let Some(Held::Target(slot)) = register(operands).and_then(|r| held.get(&r)) {
                    found.virtual_calls.insert(*slot);
                }
                None
            }
            _ => None,
        };
        if let Some((destination, value)) = next {
            held.insert(destination, value);
        }
    }
    found
}

/// The starts of the functions that may write a vtable through one of `slots`: each loads the
/// slot with `adrp` and a following `ldr`, and has an `adrp` of a page that holds a vtable.
fn possible_writers(text: &Text, symbols: &[Symbol], slots: &BTreeSet<u64>) -> BTreeSet<u64> {
    let vtable_pages = vtable_pages(symbols);
    let words: Vec<u32> = text
        .code
        .as_chunks::<4>()
        .0
        .iter()
        .map(|word| u32::from_le_bytes(*word))
        .collect();

    let mut loading = BTreeSet::new();
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
        let loads_a_slot = words[index + 1..]
            .iter()
            .take(LOAD_WINDOW)
            .filter_map(|&word| ldr_immediate(word))
            .any(|(_, base, offset)| base == page_register && slots.contains(&(page + offset)));
        if loads_a_slot {
            loading.insert(function);
        }
    }

    loading.intersection(&forming).copied().collect()
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

/// The destination, source and addend of `add` operands with an unshifted immediate, such as
/// `x8,x8,#0xcc0`.
fn added(operands: &str) -> Option<(usize, usize, u64)> {
    let mut parts = operands.split(',');
    let destination = register(parts.next()?)?;
    let source = register(parts.next()?)?;
    let addend = number(parts.next()?)?;
    parts
        .next()
        .is_none()
        .then_some((destination, source, addend))
}

/// The data register, base and offset of `ldr` or `str` operands with an immediate offset and
/// no write-back, such as `x8,[x8,#0x248]` or `x20,[x8]`.
fn memory(operands: &str) -> Option<(usize, usize, u64)> {
    let (data, memory) = operands.split_once(',')?;
    let inner = memory.strip_prefix('[')?.strip_suffix(']')?;
    let (base, offset) = match inner.split_once(',') {
        Some((base, offset)) => (base, number(offset)?),
        None => (inner, 0),
    };
    Some((register(data)?, register(base)?, offset))
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::{Uses, uses};
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
            uses(&unlocks, &pointers()).virtual_calls,
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

        assert_eq!(uses(&call, &pointers()), Uses::default());
    }

    /// `TPdxNullObject<CTraditionSwap>::Initialize()` on M452.
    #[test]
    fn an_initializer_stores_a_formed_address_through_the_slot() {
        let initializes = rows(&[
            (0x1000, "adrp", "x19,#0x7000"),
            (0x1004, "ldr", "x19,[x19,#0x248]"),
            (0x1008, "ldr", "x19,[x19]"),
            (0x100c, "mov", "x0,x19"),
            (0x1010, "bl", "#0x9900"),
            (0x1014, "adrp", "x8,#0x6000"),
            (0x1018, "add", "x8,x8,#0xcc0"),
            (0x101c, "add", "x8,x8,#0x10"),
            (0x1020, "str", "x8,[x19]"),
            (0x1024, "ret", ""),
        ]);

        assert_eq!(
            uses(&initializes, &pointers()).vtable_stores,
            BTreeSet::from([(0x7248, 0x6cd0)])
        );
    }
}
