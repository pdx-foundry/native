//! Cause tracing: where, on its own path, each unknown value of a [`Machine`] stopped being
//! known.
//!
//! Tracing is a developer diagnostic that [`trace_causes`] turns on. It never changes a value, a
//! path or an end. A traced machine computes what an untraced one computes, and keeps a [`Trace`]
//! beside each unknown register, the flags and each unknown byte. A value's trace is read only
//! while the value is unknown, and every write of an unknown value sets it. Memory that may have
//! been overwritten while it was already unknown keeps its first recorded cause, since that is
//! where it stopped being known.
//!
//! An instruction's unknown register reads collect in the machine's inputs, and an unknown value
//! that the instruction computes takes them. A memory instruction divides its inputs, so that a
//! stored value, a loaded value and a written-back base each take only their own.
use std::cell::Cell;
use std::collections::BTreeMap;
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

    /// `cause` may have overwritten the byte at `address`, which was `known` or not. A byte
    /// with a recorded cause keeps it, since it stopped being known there first. Any other byte
    /// gains `cause`; a byte that was never known keeps its unrecorded part.
    fn overwrite(&mut self, address: u64, known: bool, cause: Trace) {
        let mut trace = if known {
            Trace::default()
        } else {
            self.byte(address)
        };
        if trace.causes().next().is_none() {
            trace.merge(&cause);
            self.memory.insert(address, trace);
        }
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
    pub(super) fn take_inputs(&self) -> Trace {
        self.traces
            .as_ref()
            .map_or_else(Trace::default, |traces| traces.inputs.take())
    }

    pub(super) fn restore_inputs(&self, inputs: Trace) {
        if let Some(traces) = &self.traces {
            traces.inputs.set(inputs);
        }
    }

    /// The inputs of a value loaded from `width` bytes at `address`, when `address_inputs` gave
    /// the address. An unknown address is the whole cause; otherwise the unknown bytes are.
    pub(super) fn loaded_inputs(
        &self,
        address_inputs: Trace,
        address: Option<u64>,
        width: u64,
    ) -> Trace {
        match address {
            None => address_inputs,
            Some(address) => self.memory_trace(address, width).unwrap_or_default(),
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
    pub(super) fn trace_stored(&mut self, address: u64, width: u64, known: bool) {
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

    /// The method is about to make `width` bytes at `address` unknown. Before any walk, no
    /// instruction is the cause.
    pub(super) fn trace_forgetting(&mut self, address: u64, width: u64) {
        if self.traces.is_none() {
            return;
        }
        let cause = match self.entry {
            0 => Trace::UNRECORDED,
            _ => self.cause(CauseKind::Invalidated),
        };
        let bytes: Vec<_> = (address..address + width)
            .map(|address| (address, self.byte(address).is_some()))
            .collect();
        if let Some(traces) = &mut self.traces {
            for (address, known) in bytes {
                traces.overwrite(address, known, cause);
            }
        }
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
