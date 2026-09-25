use crate::AnalysisError;
use crate::engine::analysis::discovery::{SchedulerLayout, StaticInput, VtableWitness};
use std::collections::BTreeMap;

use super::fixups::{self, Fixups};
use super::inventory::{self, Inventory};
use super::u64_at;

/// The image inventory, its required fixups, the scheduler window of the verified layout and the
/// vtable witnesses.
pub(in crate::binding) fn read(
    bytes: &[u8],
    layout: &SchedulerLayout,
) -> Result<StaticInput, AnalysisError> {
    let inventory = inventory::read(bytes).map_err(|_| AnalysisError::InvalidRange)?;
    let fixups = fixups::read(&inventory).map_err(|_| AnalysisError::InvalidRange)?;
    let code = scheduler_window(&inventory, layout)?;
    let vtables = vtables(&inventory, &fixups)?;

    Ok(StaticInput {
        symbols: inventory.symbols,
        code,
        layout: layout.clone(),
        pointers: fixups.pointers,
        global_bindings: fixups.bindings,
        bound_slots: fixups.bound,
        strings: inventory.strings,
        vtables,
    })
}

fn scheduler_window(
    inventory: &Inventory<'_>,
    layout: &SchedulerLayout,
) -> Result<Vec<u8>, AnalysisError> {
    let length = layout
        .end
        .checked_sub(layout.start)
        .ok_or(AnalysisError::InvalidRange)?;
    if length > 65536 {
        return Err(AnalysisError::InvalidRange);
    }

    let mut code = Vec::new();
    for offset in (0..length).step_by(4096) {
        code.extend(inventory.code_range(layout.start + offset, (length - offset).min(4096))?);
    }

    Ok(code)
}

/// Each vtable address point whose offset-to-top is plausible and whose type-info and member
/// slots are fixed up.
fn vtables(
    inventory: &Inventory<'_>,
    fixups: &Fixups,
) -> Result<BTreeMap<u64, VtableWitness>, AnalysisError> {
    let symbols = &inventory.symbols;
    let mut vtables = BTreeMap::new();

    for (i, symbol) in symbols
        .iter()
        .enumerate()
        .filter(|(_, s)| s.name.starts_with("vtable for "))
    {
        let end = symbols[i + 1..]
            .iter()
            .find(|s| s.address > symbol.address)
            .map(|s| s.address)
            .unwrap_or(symbol.address);

        for address in (symbol.address..end.min(symbol.address + 65536)).step_by(8) {
            let Ok(raw) = inventory.data_at(address, 16) else {
                continue;
            };
            let offset = u64_at(raw, 0).ok_or(AnalysisError::InvalidRange)? as i64;
            if !(-4096..=0).contains(&offset) || !fixups.pointers.contains_key(&(address + 8)) {
                continue;
            }

            if let Some(member) = fixups.pointers.get(&(address + 56)) {
                vtables.insert(
                    address + 16,
                    VtableWitness {
                        owner: symbol
                            .name
                            .strip_prefix("vtable for ")
                            .expect("selected vtable")
                            .into(),
                        offset_to_top: offset,
                        member: *member,
                    },
                );
            }
        }
    }

    Ok(vtables)
}
