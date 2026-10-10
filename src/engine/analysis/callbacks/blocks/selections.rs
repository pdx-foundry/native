//! Candidate receiver offsets for sites that strict naming lost. These sets are only a search
//! aid: the context run must confirm the selected offset on each path. An empty set means no
//! supported origin; `None` is absorbing overflow, so a loop cannot revive an unbounded set.
use super::*;
use crate::engine::analysis::decode::{general_register, written_registers};

/// The receiver reservation and the supported positive offset range. Larger objects need a
/// proved extent before this method can reserve enough space for them.
pub(super) const RECEIVER_BYTES: u64 = 0x10_0000;

type Offsets = Option<BTreeSet<i64>>;
type Registers = [Offsets; 31];

pub(super) struct Selection {
    pub site: EvaluationSite,
    pub blocks: BTreeSet<Block>,
    pub scope_parameter: bool,
}

/// Find possible owner-receiver blocks only where strict naming found nothing. The same search
/// feeds binding retention and analysis, so a selected virtual site reaches the context pass.
pub(super) fn discover(
    sites: &[EvaluationSite],
    functions: &BTreeMap<u64, Vec<Instruction>>,
    pointers: &BTreeMap<u64, TypePointers>,
    states: &BTreeMap<u64, State>,
    evaluation: &Evaluation<'_>,
) -> Vec<Selection> {
    let mut offsets = BTreeMap::new();
    let mut selections = Vec::new();
    for site in sites {
        let Some(pointers) = pointers.get(&site.function) else {
            continue;
        };
        let Some(owner) = pointers.method_of.as_ref() else {
            continue;
        };
        if pointers.registers.get(&0) != Some(owner) {
            continue;
        }
        let Some(state) = states.get(&site.address) else {
            continue;
        };
        if !matches!(evaluation.name(site, state, pointers), Named::Nothing) {
            continue;
        }
        let Some(rows) = functions.get(&site.function) else {
            continue;
        };
        let family = match site.call {
            EvaluationCall::Direct(target) => evaluation.evaluators.get(&target).copied(),
            EvaluationCall::Register(register) => {
                names::local_state(rows, site.address).and_then(|state| {
                    let receiver = names::sole_fact(state.register(EVALUATED_BLOCK))?;
                    slot_family(&state, register, receiver, evaluation.slots)
                })
            }
        };
        let Some(family) = family else {
            continue;
        };
        let candidates = offsets
            .entry(site.function)
            .or_insert_with(|| at_sites(rows));
        let Some(Some(candidates)) = candidates.get(&site.address) else {
            continue;
        };
        let blocks: BTreeSet<_> = candidates
            .iter()
            .filter(|&&offset| offset > 0 && offset < RECEIVER_BYTES as i64)
            .map(|&offset| Block {
                owner: owner.clone(),
                offset,
                family,
            })
            .collect();
        if !blocks.is_empty() {
            selections.push(Selection {
                site: site.clone(),
                blocks,
                scope_parameter: climb::parameter(state, EVALUATED_SCOPE).is_some(),
            });
        }
    }
    selections
}

fn join(left: &Offsets, right: &Offsets) -> Offsets {
    let joined: BTreeSet<_> = left.as_ref()?.union(right.as_ref()?).copied().collect();
    (joined.len() <= names::VALUE_LIMIT).then_some(joined)
}

/// Possible offsets of the method receiver in `x0` before each instruction. Loads are unknown:
/// a loaded swap is not the owner, but must not erase an owner alternative at a later join.
fn at_sites(rows: &[Instruction]) -> BTreeMap<u64, Offsets> {
    if rows.is_empty() {
        return BTreeMap::new();
    }
    let blocks = names::Blocks::new(rows);
    let mut initial: Registers = std::array::from_fn(|_| Some(BTreeSet::new()));
    initial[0] = Some(BTreeSet::from([0]));
    let mut entries = BTreeMap::from([(0, initial)]);
    let mut pending = BTreeSet::from([0]);
    while let Some(block) = pending.pop_first() {
        let mut state = entries[&block].clone();
        for row in &rows[blocks.range(block)] {
            step(row, &mut state);
        }
        for successor in blocks.successors(block) {
            if let Some(previous) = entries.get_mut(&successor) {
                let mut changed = false;
                for (old, new) in previous.iter_mut().zip(&state) {
                    let joined = join(old, new);
                    changed |= *old != joined;
                    *old = joined;
                }
                if changed {
                    pending.insert(successor);
                }
            } else {
                entries.insert(successor, state.clone());
                pending.insert(successor);
            }
        }
    }
    let mut found = BTreeMap::new();
    for (block, mut state) in entries {
        for row in &rows[blocks.range(block)] {
            found.insert(row.address, state[0].clone());
            step(row, &mut state);
        }
    }
    found
}

fn step(row: &Instruction, state: &mut Registers) {
    let operands = names::split(&row.operands);
    let register = |operand: &str| {
        operand
            .starts_with('x')
            .then(|| general_register(operand))
            .flatten()
            .filter(|&index| index < 31)
    };
    let value = |operand: &str| {
        register(operand)
            .map(|index| state[index].clone())
            .unwrap_or_else(|| Some(BTreeSet::new()))
    };
    let destination = operands.first().and_then(|operand| register(operand));
    let assigned = match (row.operation.as_str(), operands.as_slice(), destination) {
        ("mov", [_, source], Some(destination)) => Some((destination, value(source))),
        ("add" | "sub", [_, source, amount], Some(destination)) if amount.starts_with('#') => {
            let offsets = value(source).and_then(|offsets| {
                let amount = names::immediate(amount)?;
                let amount = if row.operation == "sub" {
                    amount.checked_neg()?
                } else {
                    amount
                };
                offsets
                    .into_iter()
                    .map(|offset| offset.checked_add(amount))
                    .collect()
            });
            Some((destination, offsets))
        }
        ("csel", [_, left, right, _], Some(destination)) => {
            Some((destination, join(&value(left), &value(right))))
        }
        _ => None,
    };
    for register in written_registers(&row.operation, &row.operands) {
        if register < 31 {
            state[register] = Some(BTreeSet::new());
        }
    }
    if let Some((register, offsets)) = assigned {
        state[register] = offsets;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::assembler::arm64;
    use crate::engine::analysis::decode::decode_arm64;

    #[test]
    fn an_offset_loop_overflows_without_reviving_candidates() {
        let bytes = arm64!(at 0x1000;
            add x0, x0, #8;
            cbnz x1, extern 0x1000;
            ret
        );
        let rows = decode_arm64(&bytes, 0x1000).unwrap();
        assert_eq!(at_sites(&rows)[&0x1008], None);
    }

    #[test]
    fn an_unknown_loaded_choice_does_not_erase_the_receiver_choice() {
        let bytes = arm64!(at 0x1000;
            add x8, x0, #0x40;
            ldr x9, [x0, #0x80];
            csel x0, x8, x9, eq;
            add x0, x0, #8;
            ldr x8, [x0];
            ldr x8, [x8, #0x10];
            blr x8;
            ret
        );
        let rows = decode_arm64(&bytes, 0x1000).unwrap();
        assert_eq!(at_sites(&rows)[&0x1018], Some(BTreeSet::from([0x48])));
        let state = names::local_state(&rows, 0x1018).unwrap();
        let receiver = names::sole_fact(state.register(0)).unwrap();
        let target = names::sole_fact(state.register(8)).unwrap();
        assert_eq!(names::vtable_slot(receiver, target), Some(0x10));
    }
}
