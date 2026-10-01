//! Cause tracing: where, on its own path, each unknown value of a [`Machine`] stopped being
//! known.
//!
//! Tracing is a developer diagnostic that [`trace_causes`] turns on. It never changes a value, a
//! path or an end. A traced machine computes what an untraced one computes, and keeps a [`Trace`]
//! beside each unknown register, the flags and each unknown byte. A value's trace is read only
//! while the value is unknown, and every write of an unknown value sets it. An unknown byte keeps,
//! in order, the places that may have overwritten it since its last definite store: the first is
//! where it stopped being known, and the last is its latest loss. A bounded trace keeps its first
//! losses and its latest one.
//!
//! A call whose body a constructor walk enters runs on a separate machine. That machine starts
//! with its caller's causes, and its owner bytes' causes replace the caller's when it returns.
//!
//! An instruction's unknown register reads collect in the machine's inputs, and an unknown value
//! that the instruction computes takes them. A memory instruction divides its inputs, so that a
//! stored value, a loaded value and a written-back base each take only their own. This input
//! bookkeeping also carries receiver-byte origins independently of diagnostic tracing.
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::RangeInclusive;

use super::{HeadState, Machine};
use crate::engine::analysis::stop::{Cause, CauseKind, Obstacle, Trace, Unknown, Unresolved};

thread_local! {
    /// Whether the machines that this thread creates trace causes. Static analysis runs on its
    /// caller's thread, so this switch reaches every method without an option in each input.
    static TRACING: Cell<bool> = const { Cell::new(false) };
}

/// Run `method` with cause tracing on for every machine that it creates on this thread, and
/// return its result. An obstruction at an unknown value then carries the [`Trace`] of that value.
///
/// For Native's developers. Tracing changes no answer, stop or comparison. Analysis that an
/// earlier call cached is not run again, so trace a question on a newly opened `Native`.
///
/// ```no_run
/// use pdx_native::Native;
/// use pdx_native::internals::{registry_field_stops, trace_causes};
///
/// let native = Native::open("/path/to/Stellaris")?;
/// let run = trace_causes(|| registry_field_stops::run(&native, "common/megastructures"))?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn trace_causes<T>(method: impl FnOnce() -> T) -> T {
    struct Restore(bool);

    impl Drop for Restore {
        fn drop(&mut self) {
            TRACING.set(self.0);
        }
    }

    let _restore = Restore(TRACING.replace(true));
    method()
}

/// The traces of one machine's unknown values.
#[derive(Debug, Clone)]
pub(super) struct Traces {
    registers: [Trace; 31],
    flags: Trace,
    /// Unknown bytes with a recorded cause. An unknown byte without an entry is unrecorded.
    memory: BTreeMap<u64, Trace>,
    /// The traces of the unknown registers that the present instruction read.
    inputs: Cell<Trace>,
}

impl Traces {
    /// Traces for a new machine, when this thread traces causes.
    pub(super) fn when_tracing() -> Option<Box<Self>> {
        TRACING.get().then(|| {
            Box::new(Self {
                registers: [Trace::UNRECORDED; 31],
                flags: Trace::UNRECORDED,
                memory: BTreeMap::new(),
                inputs: Cell::default(),
            })
        })
    }

    fn byte(&self, address: u64) -> Trace {
        self.memory
            .get(&address)
            .copied()
            .unwrap_or(Trace::UNRECORDED)
    }

    /// `cause` may have overwritten the byte at `address`, which was `known` or not. A known
    /// byte starts a new trace. An unknown byte records `cause` as its latest loss, and a byte
    /// that was never known keeps its unrecorded part.
    fn overwrite(&mut self, address: u64, known: bool, cause: Trace) {
        let mut trace = if known {
            Trace::default()
        } else {
            self.byte(address)
        };
        trace.record_loss(&cause);
        self.memory.insert(address, trace);
    }

    /// The traces that a call's separately evaluated body starts with: the caller's memory and
    /// argument registers.
    pub(super) fn for_callee(&self) -> Box<Self> {
        let mut registers = [Trace::UNRECORDED; 31];
        registers[..9].copy_from_slice(&self.registers[..9]);
        Box::new(Self {
            registers,
            flags: Trace::UNRECORDED,
            memory: self.memory.clone(),
            inputs: Cell::default(),
        })
    }

    fn read(&self, trace: &Trace) {
        let mut inputs = self.inputs.get();
        inputs.merge(trace);
        self.inputs.set(inputs);
    }

    /// The trace of an unknown value that the present instruction computes from its inputs.
    fn computed(&self) -> Trace {
        match self.inputs.get() {
            inputs if inputs.is_empty() => Trace::UNRECORDED,
            inputs => inputs,
        }
    }
}

/// A path's values as it arrives at a loop head, before the join changes them.
pub(super) struct Arrival {
    registers: [Option<u64>; 31],
    flags_known: bool,
    memory: BTreeMap<u64, Option<u8>>,
    traces: Box<Traces>,
}

/// Instruction inputs keep diagnostic causes separate from semantic receiver origins.
#[derive(Clone)]
pub(super) struct Inputs {
    trace: Trace,
    pub(super) receiver: BTreeSet<u64>,
    /// Whether an input may be derived from a tracked owner.
    pub(super) owner: bool,
}

impl Machine<'_> {
    /// Why general register `index` is unknown on this path. `None` when it is known or when
    /// this machine does not trace causes.
    pub fn register_trace(&self, index: usize) -> Option<Trace> {
        let traces = self.traces.as_ref()?;
        self.registers[index]
            .is_none()
            .then(|| traces.registers[index])
    }

    /// Why any of `width` bytes at `address` is unknown on this path, with the causes of every
    /// unknown byte. `None` when all are known or when this machine does not trace causes.
    pub fn memory_trace(&self, address: u64, width: u64) -> Option<Trace> {
        let traces = self.traces.as_ref()?;
        let mut trace = Trace::default();
        for address in address..address.saturating_add(width) {
            if self.byte(address).is_none() {
                trace.merge(&traces.byte(address));
            }
        }
        (!trace.is_empty()).then_some(trace)
    }

    /// A trace whose one cause of `kind` is at the present instruction.
    fn cause(&self, kind: CauseKind) -> Trace {
        Trace::of(Cause {
            kind,
            instruction: self.pc,
            entry: self.entered(),
        })
    }

    /// `unresolved`, a stop at `obstacle`, with the trace of the unknown value that stopped it.
    pub(super) fn with_stop_trace(&self, unresolved: Unresolved, obstacle: Obstacle) -> Unresolved {
        let trace = match obstacle {
            Obstacle::Unknown(Unknown::Register(index)) => self.register_trace(index.into()),
            Obstacle::Unknown(Unknown::Flags) => self
                .traces
                .as_ref()
                .filter(|_| self.flags.is_none())
                .map(|traces| traces.flags),
            _ => None,
        };
        unresolved.traced(trace)
    }

    pub(super) fn clear_inputs(&self) {
        self.take_inputs();
    }

    pub(super) fn read_unknown_register(&self, index: usize) {
        if let Some(provenance) = &self.provenance {
            provenance
                .inputs
                .borrow_mut()
                .extend(&provenance.registers[index]);
        }
        if let Some(traces) = &self.traces {
            traces.read(&traces.registers[index]);
        }
    }

    /// The present instruction read a vector register, which is unknown. Vector registers keep
    /// no causes.
    pub(super) fn read_unknown_vector(&self) {
        if let Some(traces) = &self.traces {
            traces.read(&Trace::UNRECORDED);
        }
    }

    /// The inputs that the present instruction has read so far, which it no longer holds.
    pub(super) fn take_inputs(&self) -> Inputs {
        Inputs {
            trace: self
                .traces
                .as_ref()
                .map_or_else(Trace::default, |traces| traces.inputs.take()),
            receiver: self
                .provenance
                .as_ref()
                .map(|provenance| provenance.inputs.take())
                .unwrap_or_default(),
            owner: self.owner.as_ref().is_some_and(|owner| owner.inputs.take()),
        }
    }

    pub(super) fn restore_inputs(&self, inputs: Inputs) {
        if let Some(traces) = &self.traces {
            traces.inputs.set(inputs.trace);
        }
        if let Some(provenance) = &self.provenance {
            provenance.inputs.replace(inputs.receiver);
        }
        if let Some(owner) = &self.owner {
            owner.inputs.set(inputs.owner);
        }
    }

    /// An unknown address supplies the origins; a known address supplies its unknown bytes.
    pub(super) fn loaded_inputs(
        &self,
        address_inputs: Inputs,
        address: Option<u64>,
        width: u64,
    ) -> Inputs {
        let owner = self.loaded_owner(address_inputs.owner, address, width);
        match address {
            None => Inputs {
                owner,
                ..address_inputs
            },
            Some(address) => Inputs {
                trace: self.memory_trace(address, width).unwrap_or_default(),
                receiver: self.receiver_sources(address, width),
                owner,
            },
        }
    }

    /// General register `index` became unknown through the present instruction.
    pub(super) fn trace_register(&mut self, index: usize) {
        if let Some(traces) = &mut self.traces {
            traces.registers[index] = traces.computed();
        }
    }

    /// The flags became unknown through the present instruction.
    pub(super) fn trace_flags(&mut self) {
        if let Some(traces) = &mut self.traces {
            traces.flags = traces.computed();
        }
    }

    /// `width` bytes at `address` were stored through the present instruction.
    pub(super) fn record_stored_inputs(&mut self, address: u64, width: u64, known: bool) {
        if let Some(provenance) = &mut self.provenance {
            let sources = provenance.inputs.borrow().clone();
            for at in address..address + width {
                if known {
                    provenance.memory.remove(&at);
                } else {
                    provenance.memory.insert(at, sources.clone());
                }
            }
        }
        let Some(traces) = &mut self.traces else {
            return;
        };
        let trace = traces.computed();
        for address in address..address + width {
            if known {
                traces.memory.remove(&address);
            } else {
                traces.memory.insert(address, trace);
            }
        }
    }

    /// The method is about to make `width` bytes at `address` unknown, for the reason `kind`.
    pub(super) fn trace_forgetting(&mut self, address: u64, width: u64, kind: CauseKind) {
        if self.traces.is_none() {
            return;
        }
        let cause = self.forgetting_cause(kind);
        let bytes: Vec<_> = (address..address + width)
            .map(|address| (address, self.byte(address).is_some()))
            .collect();
        if let Some(traces) = &mut self.traces {
            for (address, known) in bytes {
                traces.overwrite(address, known, cause);
            }
        }
    }

    /// The method is about to make the known bytes of `length` bytes at `address` unknown, for
    /// the reason `kind`. Each byte there that is already unknown and was written or inherited
    /// gains the cause too, since the method may overwrite it again.
    pub(super) fn trace_forgetting_unknown(&mut self, address: u64, length: u64, kind: CauseKind) {
        if self.traces.is_none() {
            return;
        }
        let range = address..address.saturating_add(length);
        let mut unknown: Vec<u64> = self
            .memory
            .range(range.clone())
            .filter(|(_, byte)| byte.is_none())
            .map(|(&address, _)| address)
            .collect();
        unknown.extend(self.inherited_unknown_bytes(|address| range.contains(&address)));
        let cause = self.forgetting_cause(kind);
        if let Some(traces) = &mut self.traces {
            for address in unknown {
                traces.overwrite(address, false, cause);
            }
        }
    }

    /// Before any walk, no instruction is the cause.
    fn forgetting_cause(&self, kind: CauseKind) -> Trace {
        match self.entry {
            0 => Trace::UNRECORDED,
            _ => self.cause(kind),
        }
    }

    /// The unknown bytes that `keep` selects and that only a trace records: a call's copy of
    /// its caller's unknown memory, which [`Machine::fresh_callee`] leaves out of memory.
    pub(super) fn inherited_unknown_bytes(&self, keep: impl Fn(u64) -> bool) -> Vec<u64> {
        let Some(traces) = &self.traces else {
            return Vec::new();
        };
        traces
            .memory
            .keys()
            .copied()
            .filter(|address| !self.memory.contains_key(address))
            .filter(|&address| keep(address) && self.byte(address).is_none())
            .collect()
    }

    /// The present instruction stores to an unknown address, which may have overwritten each
    /// of `bytes`: an address and whether its byte was known.
    pub(super) fn trace_unknown_store(&mut self, bytes: &[(u64, bool)]) {
        let cause = self.cause(CauseKind::UnknownStore);
        if let Some(traces) = &mut self.traces {
            for &(address, known) in bytes {
                traces.overwrite(address, known, cause);
            }
        }
    }

    /// The trace of each unknown byte of `length` bytes at `address`, by offset from `address`.
    /// Empty when this machine does not trace causes.
    pub(crate) fn unknown_byte_traces(&self, address: u64, length: u64) -> BTreeMap<u64, Trace> {
        let Some(traces) = &self.traces else {
            return BTreeMap::new();
        };
        (0..length)
            .filter(|offset| self.byte(address + offset).is_none())
            .map(|offset| (offset, traces.byte(address + offset)))
            .collect()
    }

    /// Replace the trace of each unknown byte that `traces` lists by its offset from `address`.
    pub(crate) fn set_byte_traces(&mut self, address: u64, traces: &BTreeMap<u64, Trace>) {
        let unknown: Vec<_> = traces
            .iter()
            .filter(|(offset, _)| self.byte(address + *offset).is_none())
            .map(|(offset, trace)| (address + offset, *trace))
            .collect();
        if let Some(machine_traces) = &mut self.traces {
            machine_traces.memory.extend(unknown);
        }
    }

    /// A trace whose one cause is that paths joining after the present instruction disagreed.
    /// `None` when this machine does not trace causes.
    pub(crate) fn join_cause(&self) -> Option<Trace> {
        self.traces.as_ref().map(|_| self.cause(CauseKind::Join))
    }

    /// The call at the present instruction returned and left `registers` and the flags unknown.
    pub(super) fn trace_call(&mut self, registers: RangeInclusive<usize>) {
        let trace = self.cause(CauseKind::Call);
        if let Some(traces) = &mut self.traces {
            traces.registers[registers].fill(trace);
            traces.flags = trace;
        }
    }

    /// This path's values before a join at a loop head, when this machine traces causes.
    pub(super) fn arrival(&self) -> Option<Arrival> {
        let traces = self.traces.clone()?;
        Some(Arrival {
            registers: self.registers,
            flags_known: self.flags.is_some(),
            memory: self.memory.clone(),
            traces,
        })
    }

    /// Give each value that the join at the present instruction left unknown the causes of each
    /// side that did not know it: `kept`, the facts of earlier arrivals, and `arrival`, this path
    /// as it came. When a side knew the value, the paths disagreed, and the join is a cause too.
    pub(super) fn trace_join(&mut self, kept: &HeadState, arrival: &Arrival) {
        let join = self.cause(CauseKind::Join);
        let data = self.data;
        let (Some(traces), Some(kept_traces)) = (&mut self.traces, kept.traces.as_deref()) else {
            return;
        };
        let joined = |kept: Option<Trace>, arrival: Option<Trace>| match (kept, arrival) {
            (Some(mut kept), Some(arrival)) => {
                kept.merge(&arrival);
                kept
            }
            (kept, arrival) => {
                let mut trace = join;
                for side in [kept, arrival].into_iter().flatten() {
                    trace.merge(&side);
                }
                trace
            }
        };

        for index in 0..self.registers.len() {
            if self.registers[index].is_none() {
                traces.registers[index] = joined(
                    kept.registers[index]
                        .is_none()
                        .then(|| kept_traces.registers[index]),
                    arrival.registers[index]
                        .is_none()
                        .then(|| arrival.traces.registers[index]),
                );
            }
        }

        if self.flags.is_none() {
            traces.flags = joined(
                kept.flags.is_none().then_some(kept_traces.flags),
                (!arrival.flags_known).then_some(arrival.traces.flags),
            );
        }

        let byte_in = |memory: &BTreeMap<u64, Option<u8>>, address: u64| {
            memory
                .get(&address)
                .copied()
                .unwrap_or_else(|| data.byte(address))
        };
        for (&address, byte) in &self.memory {
            if byte.is_none() {
                let trace = joined(
                    byte_in(&kept.memory, address)
                        .is_none()
                        .then(|| kept_traces.byte(address)),
                    byte_in(&arrival.memory, address)
                        .is_none()
                        .then(|| arrival.traces.byte(address)),
                );
                traces.memory.insert(address, trace);
            }
        }
    }

    /// Keep in `kept` the causes of `arrival`, a path whose state the kept facts cover, so that
    /// a path that later widens those facts keeps them.
    pub(super) fn trace_covered(&self, kept: &mut HeadState, arrival: &Arrival) {
        let Some(kept_traces) = kept.traces.as_deref_mut() else {
            return;
        };
        for index in 0..arrival.registers.len() {
            if arrival.registers[index].is_none() {
                kept_traces.registers[index].merge(&arrival.traces.registers[index]);
            }
        }
        if !arrival.flags_known {
            kept_traces.flags.merge(&arrival.traces.flags);
        }
        for (&address, byte) in &arrival.memory {
            if byte.is_none() {
                let mut trace = kept_traces.byte(address);
                trace.merge(&arrival.traces.byte(address));
                kept_traces.memory.insert(address, trace);
            }
        }
    }
}

#[cfg(test)]
mod tests;
