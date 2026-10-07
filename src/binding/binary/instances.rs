//! Read the input of the instance-pointer proof (`engine::analysis::callbacks::instances`) for
//! the virtual calls in decoded functions, and the words that a context run needs to follow such
//! a call.
use std::collections::{BTreeMap, BTreeSet};

use crate::AnalysisError;
use crate::engine::analysis::callbacks::instances::{Loader, instance_vtables, virtual_call_slots};
use crate::engine::analysis::decode::{Instruction, adrp, decode_arm64, ldr_immediate};
use crate::engine::analysis::discovery::Symbol;

use super::declarations::Text;
use super::families::point_class;
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

    let loaders: Vec<Loader> = possible_writers(text, image.symbols, &slots)
        .into_iter()
        .map(|(start, slots)| Loader {
            rows: text
                .function(start)
                .ok()
                .and_then(|(address, code)| decode_arm64(code, address).ok()),
            slots,
        })
        .collect();
    let data = super::language::constant_data(image.bytes, image.pointers, bound_slots)?;
    let vtables: BTreeMap<u64, u64> = instance_vtables(&loaders, image.pointers, &data)
        .into_iter()
        .filter(|(_, point)| point_class(image.symbols, &data, *point).is_some())
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
