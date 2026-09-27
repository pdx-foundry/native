//! Receiver-byte origins and unknown-input decisions retained by watched command runs.
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use super::Machine;

/// One side of a fork on unknown input. Known token tests do not create decisions.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Decision {
    /// Function containing the decision.
    pub entry: u64,
    /// Branch or conditional instruction address.
    pub instruction: u64,
    /// Zero-based visit to this decision within the chain.
    pub occurrence: usize,
    /// Branch index, preserved across equivalent runs.
    pub side: usize,
    /// Unknown initial receiver bytes that supplied the decision; empty for other input.
    pub receiver: BTreeSet<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Provenance {
    pub registers: [BTreeSet<u64>; 31],
    pub vectors: [BTreeSet<u64>; 32],
    pub flags: BTreeSet<u64>,
    pub memory: BTreeMap<u64, BTreeSet<u64>>,
    pub inputs: RefCell<BTreeSet<u64>>,
    pub decisions: Vec<Decision>,
}

impl Machine<'_> {
    /// Unknown-input decisions of this path, including the receiver bytes that caused them.
    pub fn decisions(&self) -> Vec<Decision> {
        self.provenance
            .as_ref()
            .map(|provenance| provenance.decisions.clone())
            .unwrap_or_default()
    }

    pub(super) fn receiver_sources(&self, address: u64, width: u64) -> BTreeSet<u64> {
        let mut sources = BTreeSet::new();
        let Some(provenance) = &self.provenance else {
            return sources;
        };
        for at in address..address.saturating_add(width) {
            if self.byte(at).is_some() {
                continue;
            }
            if let Some(stored) = provenance.memory.get(&at) {
                sources.extend(stored);
            }
            if let Some(watch) = &self.read_watch
                && (watch.start + 8..watch.end).contains(&at)
                && !watch.written.contains(&at)
            {
                sources.insert(at - watch.start);
            }
        }
        sources
    }

    pub(super) fn record_decision(&mut self, instruction: u64, side: usize, flags: bool) {
        let entry = self.entered();
        let Some(provenance) = &mut self.provenance else {
            return;
        };
        let receiver = if flags {
            provenance.flags.clone()
        } else {
            provenance.inputs.borrow().clone()
        };
        let occurrence = provenance
            .decisions
            .iter()
            .filter(|d| d.entry == entry && d.instruction == instruction)
            .count();
        provenance.decisions.push(Decision {
            entry,
            instruction,
            occurrence,
            side,
            receiver,
        });
    }
}
