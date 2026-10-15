//! Path joins: a path that arrives at a join point in the state of an earlier path ends there.
//!
//! A *join point* is an instruction with more than one direct predecessor. Two paths that arrive
//! at one in states that differ only in dead registers and in the stack below the stack pointer
//! run the same instructions from there, with the same calls and the same ends, so the run follows
//! only the first. Paths split on a profiler flag or a string's length and meet again after the
//! test; without the join each test doubles the paths that follow.
//!
//! A register is *dead* at an instruction when every path from it writes the register before it
//! reads it. The liveness relies on the calling convention, as callers of the run already do: a
//! call reads `x0` to `x8` and the callee-saved registers and leaves `x0` to `x18` and the flags
//! unknown to its caller, and a return gives `x0` to `x7` and the callee-saved registers. A
//! callback of the run reads no caller-saved register other than these at a call. Compiled code
//! for this target keeps nothing below the stack pointer, since it has no red zone.
//!
//! [`Machine::run_paths_joining`] does not join paths this way: a path there goes on from the facts
//! kept at a loop head, which later arrivals make fewer, so two paths in one state at different
//! times need not run the same way.
//!
//! A path that comes round a loop without a change that it may read joins its own earlier state:
//! the run already follows every way out of the loop from there. When every path of a run joins
//! and none ends, the run reports one path stopped at the loop bound, so a caller never reads an
//! empty run as an answer. A run that tracks an owner, watches receiver reads or records decisions
//! keeps every path, since its answer depends on each path's history. A path's step count and
//! trace are not compared, so a joined path's later trace is not recorded.
use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::ops::Deref;

use super::{Branch, Machine, Name, Operand, Operation, Register};
use crate::engine::analysis::decode::{destination_count, is_atomic};

/// The loop counts of each path that arrived at a join point, by the join point and the digest of
/// its state.
pub(super) type Arrivals = HashMap<(u64, u64), Vec<BTreeMap<u64, u32>>>;

/// A set of general registers, bit `n` for `x<n>`, with the flags at bit 31.
type Registers = u32;

const FLAGS: Registers = 1 << 31;
const EVERYTHING: Registers = Registers::MAX;
/// `x0` to `x7`.
const ARGUMENTS: Registers = 0xff;
/// `x8`, the address of a returned object.
const RESULT_ADDRESS: Registers = 1 << 8;
/// `x19` to `x30`: the callee-saved registers, the frame pointer and the link register.
const PRESERVED: Registers = 0x7ff8_0000;
/// The link register.
const LINK: Registers = 1 << 30;
/// What a call reads: its arguments and every register that its callee preserves.
const CALL_READS: Registers = ARGUMENTS | RESULT_ADDRESS | PRESERVED & !LINK;
/// `x0` to `x18` and the link register, which a call writes, and the flags.
const CALL_WRITES: Registers = 0x7_ffff | LINK | FLAGS;
/// What a return gives its caller.
const RETURN_READS: Registers = ARGUMENTS | PRESERVED;

/// The general registers and flags that some path may read at each join point of `rows`, by
/// address.
pub(super) fn live_at_joins(rows: &BTreeMap<u64, Operation>) -> BTreeMap<u64, Registers> {
    let position: BTreeMap<u64, usize> = rows
        .keys()
        .enumerate()
        .map(|(index, &address)| (address, index))
        .collect();
    let effects: Vec<_> = rows
        .iter()
        .map(|(&address, operation)| effect(address, operation, &position))
        .collect();

    let mut predecessors = vec![0_u32; rows.len()];
    for effect in &effects {
        for &successor in effect.successors.iter().flatten() {
            predecessors[successor] += 1;
        }
    }

    let mut live = vec![0; rows.len()];
    let mut changed = true;
    while changed {
        changed = false;
        for (index, effect) in effects.iter().enumerate().rev() {
            let after = effect.successors.iter().fold(0, |after, successor| {
                after | successor.map_or(EVERYTHING, |successor| live[successor])
            });
            let before = after & !effect.writes | effect.reads;
            if before != live[index] {
                live[index] = before;
                changed = true;
            }
        }
    }

    rows.keys()
        .enumerate()
        .filter(|&(index, _)| predecessors[index] > 1)
        .map(|(index, &address)| (address, live[index]))
        .collect()
}

/// What one instruction reads and writes, and the positions of the instructions that can run
/// next: `None` for one outside the rows, which may read anything.
struct Effect {
    reads: Registers,
    writes: Registers,
    successors: Vec<Option<usize>>,
}

fn effect(address: u64, operation: &Operation, position: &BTreeMap<u64, usize>) -> Effect {
    let at = |address: u64| position.get(&address).copied();
    let Operation::Parsed { mnemonic, operands } = operation else {
        return Effect {
            reads: EVERYTHING,
            writes: 0,
            successors: vec![at(address + 4)],
        };
    };

    let (operand_reads, operand_writes) = operand_registers(mnemonic, operands);
    let (reads, writes) = match Branch::of(mnemonic) {
        Some(Branch::Call | Branch::RegisterCall) => (CALL_READS | operand_reads, CALL_WRITES),
        Some(Branch::Return) => (RETURN_READS, 0),
        Some(Branch::Register) => (EVERYTHING, 0),
        Some(Branch::Trap) => (0, 0),
        Some(Branch::OnFlags(_)) => (FLAGS, 0),
        Some(Branch::Always | Branch::OnZero { .. } | Branch::OnBit { .. }) | None => {
            let reads_flags = operation.condition().is_some() || reads_flags(mnemonic);
            let writes_flags = writes_flags(mnemonic);
            (
                operand_reads | if reads_flags { FLAGS } else { 0 },
                operand_writes | if writes_flags { FLAGS } else { 0 },
            )
        }
    };
    let successors = operation
        .successors(address)
        .map(|targets| targets.into_iter().map(at).collect())
        // A branch through a register may go anywhere.
        .unwrap_or_else(|| vec![None]);

    Effect {
        reads,
        writes,
        successors,
    }
}

/// The general registers that an instruction's operands read and write, by the rules of
/// [`destination_count`]: its leading destinations are written, and an insertion, a
/// compare-and-swap or an atomic operation also reads them; every other register is read, and a
/// pre- or post-index access writes its base.
fn operand_registers(mnemonic: &str, operands: &[Operand]) -> (Registers, Registers) {
    let atomic = is_atomic(mnemonic);
    let reads_destinations = matches!(mnemonic, "movk" | "bfi" | "bfxil" | "bfm")
        || mnemonic.starts_with("cas")
        || atomic;
    let destinations = if Branch::of(mnemonic).is_some() {
        0..0
    } else if atomic {
        1..2
    } else {
        0..destination_count(mnemonic)
    };
    let bit = |register: &Register| match register.name {
        Name::General(index) => 1 << index,
        _ => 0,
    };

    let (mut reads, mut writes) = (0, 0);
    for (index, operand) in operands.iter().enumerate() {
        match operand {
            Operand::Register(register) if destinations.contains(&index) => {
                writes |= bit(register);
                if reads_destinations {
                    reads |= bit(register);
                }
            }
            Operand::Register(register) => reads |= bit(register),
            Operand::Memory(memory) => {
                reads |= bit(&memory.base);
                if let Some((register, _)) = &memory.index {
                    reads |= bit(register);
                }
                let post_index = index + 1 < operands.len();
                if memory.write_back || post_index {
                    writes |= bit(&memory.base);
                }
            }
            _ => {}
        }
    }
    (reads, writes)
}

/// Instructions without a condition operand that read the flags.
fn reads_flags(mnemonic: &str) -> bool {
    ["adc", "sbc", "ngc", "mrs"]
        .iter()
        .any(|prefix| mnemonic.starts_with(prefix))
}

/// Instructions that set every flag.
fn writes_flags(mnemonic: &str) -> bool {
    matches!(
        mnemonic,
        "cmp"
            | "cmn"
            | "tst"
            | "adds"
            | "subs"
            | "ands"
            | "bics"
            | "negs"
            | "ccmp"
            | "ccmn"
            | "fcmp"
            | "fcmpe"
            | "fccmp"
            | "fccmpe"
    )
}

impl Machine<'_> {
    /// Whether this path, now at `pc`, joins a path of the run that arrived there earlier in the
    /// same state. Records the arrival when it does not.
    pub(super) fn joins_earlier_path(&self, pc: u64, arrivals: &mut Arrivals) -> bool {
        // A run that tracks an owner, watches receiver reads or records decisions keeps every
        // path: its answer depends on each path's history. A tail call that the caller intercepts
        // receives registers that the liveness, which follows the branch, does not count.
        if self.owner.is_some()
            || self.read_watch.is_some()
            || self.provenance.is_some()
            || !self.tail_entries.is_empty()
        {
            return false;
        }
        let Some(&live) = self.code.live_at_joins().get(&pc) else {
            return false;
        };

        // A path that has come round each loop at least as often as an earlier one in its state
        // reaches no loop bound later than that path does. Steps are not compared: when the
        // earlier path stops at the step limit, the run reports that bound.
        let earlier = arrivals.entry((pc, self.live_state(live))).or_default();
        let covered = earlier.iter().any(|visits| {
            visits
                .iter()
                .all(|(head, count)| self.loop_visits.get(head).is_some_and(|own| own >= count))
        });
        if !covered {
            earlier.push(self.loop_visits.clone());
        }
        covered
    }

    /// A digest of every fact that the rest of a path may read, given the registers and flags
    /// that it may read. Two paths that are equal in these facts have the same digest; a 64-bit
    /// collision between two different states at one join point is taken not to happen.
    fn live_state(&self, live: Registers) -> u64 {
        let mut hasher = Mixer::default();
        for (index, register) in self.registers.iter().enumerate() {
            (live & 1 << index != 0)
                .then_some(register)
                .hash(&mut hasher);
        }
        self.vectors.hash(&mut hasher);
        if live & FLAGS != 0 {
            self.flags.hash(&mut hasher);
            self.possible_flags.hash(&mut hasher);
        }
        self.stack_pointer.hash(&mut hasher);
        let dead_stack = self.memory.range(self.next_object..self.stack_pointer);
        let dead = dead_stack.fold(0, |sum: u64, (&address, &byte)| {
            sum.wrapping_add(byte_digest(address, byte))
        });
        self.memory.digest.wrapping_sub(dead).hash(&mut hasher);
        self.next_object.hash(&mut hasher);
        self.protected.hash(&mut hasher);
        self.register_saves.hash(&mut hasher);
        self.returned_values.hash(&mut hasher);
        self.tail_aliases.hash(&mut hasher);
        self.labels.hash(&mut hasher);
        self.unknown_stores.hash(&mut hasher);
        self.frames.hash(&mut hasher);
        self.callees.hash(&mut hasher);
        self.frame_tops.hash(&mut hasher);
        self.entry.hash(&mut hasher);
        hasher.finish()
    }
}

/// The memory of a machine: the byte at each address that a path wrote, or `None` for one that
/// it made unknown, with a digest of every entry that changes as the entries do. Reads go through
/// the map; writes go through these methods, which keep the digest.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Bytes {
    map: BTreeMap<u64, Option<u8>>,
    /// The wrapping sum of [`byte_digest`] over the entries.
    digest: u64,
}

impl Bytes {
    pub(super) fn insert(&mut self, address: u64, byte: Option<u8>) {
        if let Some(before) = self.map.insert(address, byte) {
            self.digest = self.digest.wrapping_sub(byte_digest(address, before));
        }
        self.digest = self.digest.wrapping_add(byte_digest(address, byte));
    }

    /// Make unknown every byte at an address for which `forget` holds, and give `forgotten` each
    /// such address with whether its byte was known.
    pub(super) fn forget_where(
        &mut self,
        forget: impl Fn(u64) -> bool,
        mut forgotten: impl FnMut(u64, bool),
    ) {
        for (&address, byte) in &mut self.map {
            if !forget(address) {
                continue;
            }

            forgotten(address, byte.is_some());
            self.digest = self
                .digest
                .wrapping_sub(byte_digest(address, *byte))
                .wrapping_add(byte_digest(address, None));
            *byte = None;
        }
    }
}

impl Deref for Bytes {
    type Target = BTreeMap<u64, Option<u8>>;

    fn deref(&self) -> &Self::Target {
        &self.map
    }
}

impl FromIterator<(u64, Option<u8>)> for Bytes {
    fn from_iter<I: IntoIterator<Item = (u64, Option<u8>)>>(entries: I) -> Self {
        let mut bytes = Self::default();
        for (address, byte) in entries {
            bytes.insert(address, byte);
        }
        bytes
    }
}

/// A well-mixed digest of one memory entry.
fn byte_digest(address: u64, byte: Option<u8>) -> u64 {
    let byte = byte.map_or(0x100, u64::from);
    mix(address.wrapping_mul(0x200).wrapping_add(byte))
}

/// The `splitmix64` finalizer: a bijection whose every output bit depends on every input bit.
fn mix(mut value: u64) -> u64 {
    value = (value ^ value >> 30).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ value >> 27).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ value >> 31
}

/// A hasher that mixes each word into its state, faster than the standard library's for the
/// many small words of a machine state.
#[derive(Default)]
struct Mixer(u64);

impl Hasher for Mixer {
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut word = [0; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            self.write_u64(u64::from_le_bytes(word));
        }
    }

    fn write_u8(&mut self, word: u8) {
        self.write_u64(word.into());
    }

    fn write_u16(&mut self, word: u16) {
        self.write_u64(word.into());
    }

    fn write_u32(&mut self, word: u32) {
        self.write_u64(word.into());
    }

    fn write_u64(&mut self, word: u64) {
        self.0 = mix(self.0.rotate_left(23) ^ word);
    }

    fn write_u128(&mut self, word: u128) {
        self.write_u64(word as u64);
        self.write_u64((word >> 64) as u64);
    }

    fn write_usize(&mut self, word: usize) {
        self.write_u64(word as u64);
    }

    fn write_isize(&mut self, word: isize) {
        self.write_u64(word as u64);
    }

    fn finish(&self) -> u64 {
        self.0
    }
}
