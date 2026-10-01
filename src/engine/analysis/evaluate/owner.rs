//! Which values may point into a freshly allocated owner, and which writes may reach it.
//!
//! A pointer that existed before the owner's allocation, or that was loaded from memory that no
//! owner-derived value can have reached, cannot point into the owner. A store through it is
//! disjoint from the owner. A value derived from the owner through registers, vectors, memory,
//! calls or path joins may point anywhere into it, and so may a value that a call returns when
//! the owner was within that call's reach.
//!
//! A byte's taint is cleared only by a store of an underived value to its known address. A store
//! to an unknown address and a forgotten byte keep their taint, since the byte may still hold its
//! earlier value.
use std::cell::Cell;
use std::collections::BTreeSet;

use super::{Call, Machine, STACK_TOP};

/// The argument registers `x0` to `x8`, and the registers that a call preserves, `x19` to `x29`.
const REACHABLE_REGISTERS: u32 = 0x1ff | 0x7ff << 19;

/// The argument vectors `v0` to `v7`, and `v8` to `v15`, whose low halves a call preserves.
const REACHABLE_VECTORS: u32 = 0xffff;

/// The vectors whose low halves a call preserves.
const PRESERVED_VECTORS: u32 = 0xff00;

/// Whether the registers that a returning call did not preserve may point into the owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReturnTaint {
    /// `x0` may point into the owner.
    pub returned: bool,
    /// Each other register that the call did not preserve may point into the owner.
    pub clobbered: bool,
}

impl ReturnTaint {
    /// A call that could reach the owner: every register it did not preserve may point into it.
    pub const REACHING: Self = Self {
        returned: true,
        clobbered: true,
    };

    /// A call that cannot reach the owner.
    pub const DISJOINT: Self = Self {
        returned: false,
        clobbered: false,
    };
}

/// The owner-derived values of one tracked machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OwnerTaint {
    start: u64,
    end: u64,
    /// The end of this machine's private stack frame. The stack above it belongs to its callers.
    frame_end: u64,
    registers: u32,
    vectors: u32,
    memory: BTreeSet<u64>,
    /// An owner-derived value went where code that this machine does not run can find it.
    escaped: bool,
    /// This machine wrote memory outside the owner and its private stack frame.
    clobbers_outside: bool,
    /// The stack pointer moved by an unknown amount, so the private frame is not established.
    stack_lost: bool,
    /// Whether the present instruction read an owner-derived input.
    pub(super) inputs: Cell<bool>,
    /// What the call being handled leaves in the registers it does not preserve.
    returning: Option<ReturnTaint>,
}

impl OwnerTaint {
    fn read_register(&self, index: usize) {
        if self.registers & 1 << index != 0 {
            self.inputs.set(true);
        }
    }

    fn read_vector(&self, index: usize) {
        if self.vectors & 1 << index != 0 {
            self.inputs.set(true);
        }
    }

    fn set_register(&mut self, index: usize, derived: bool) {
        set_bit(&mut self.registers, index, derived);
    }

    fn set_vector(&mut self, index: usize, derived: bool) {
        set_bit(&mut self.vectors, index, derived);
    }

    fn contains(&self, address: u64, limit: u64) -> bool {
        address >= self.start && limit <= self.end
    }

    fn tainted_outside(&self) -> bool {
        self.memory.range(..self.start).next().is_some()
            || self.memory.range(self.end..).next().is_some()
    }

    /// A store of `width` bytes to the known `address`, with an owner-derived value or not.
    fn store(&mut self, address: u64, width: u64, derived: bool, stack_pointer: u64) {
        let limit = address.saturating_add(width);
        let private = address >= stack_pointer
            && limit <= self.frame_end
            && (limit <= self.start || address >= self.end);
        if !self.contains(address, limit) && !private {
            self.clobbers_outside = true;
            self.escaped |= derived;
        }

        for at in address..limit {
            if derived {
                self.memory.insert(at);
            } else {
                self.memory.remove(&at);
            }
        }
    }

    /// Whether a call may write the owner: it may read every argument, every preserved
    /// register, every stack byte, and every place where an owner-derived value escaped.
    fn reaches_call(&self, stack_pointer: u64) -> bool {
        self.escaped
            || self.registers & REACHABLE_REGISTERS != 0
            || self.vectors & REACHABLE_VECTORS != 0
            || self.memory.range(stack_pointer..STACK_TOP).next().is_some()
    }

    fn returned(&mut self, taint: ReturnTaint) {
        for index in 0..=18 {
            self.set_register(index, taint.clobbered);
        }
        self.set_register(0, taint.returned);
        let volatile = !PRESERVED_VECTORS;
        if taint.clobbered {
            self.vectors = u32::MAX;
        } else {
            self.vectors &= !volatile;
        }
    }

    /// Join the facts that a loop head kept with this path's, and return whether they did not
    /// already cover this path.
    fn widen(&mut self, kept: &Self) -> bool {
        let covered = self.registers & !kept.registers == 0
            && self.vectors & !kept.vectors == 0
            && self.memory.is_subset(&kept.memory)
            && (!self.escaped || kept.escaped)
            && (!self.clobbers_outside || kept.clobbers_outside)
            && (!self.stack_lost || kept.stack_lost);
        self.registers |= kept.registers;
        self.vectors |= kept.vectors;
        self.memory.extend(&kept.memory);
        self.escaped |= kept.escaped;
        self.clobbers_outside |= kept.clobbers_outside;
        self.stack_lost |= kept.stack_lost;

        !covered
    }
}

fn set_bit(bits: &mut u32, index: usize, set: bool) {
    if set {
        *bits |= 1 << index;
    } else {
        *bits &= !(1 << index);
    }
}

impl<'a> Machine<'a> {
    /// Track which values may point into the fresh owner at `start..end`, which no value of
    /// this machine derives from yet. The whole stack is this machine's private frame.
    pub fn track_owner(&mut self, start: u64, end: u64) {
        self.owner = Some(Box::new(OwnerTaint {
            start,
            end,
            frame_end: STACK_TOP,
            registers: 0,
            vectors: 0,
            memory: BTreeSet::new(),
            escaped: false,
            clobbers_outside: false,
            stack_lost: false,
            inputs: Cell::new(false),
            returning: None,
        }));
    }

    /// The tracked owner's `(start, end)`.
    pub fn owner_range(&self) -> Option<(u64, u64)> {
        self.owner.as_ref().map(|owner| (owner.start, owner.end))
    }

    /// Mark general register `index` as holding an owner-derived value.
    #[cfg(test)]
    pub fn derive_from_owner(&mut self, index: usize) {
        if let Some(owner) = &mut self.owner {
            owner.set_register(index, true);
        }
    }

    /// Mark every general and vector register as possibly holding an owner-derived value, as at
    /// the entry of code whose caller is not analysed.
    pub fn derive_every_register_from_owner(&mut self) {
        if let Some(owner) = &mut self.owner {
            owner.registers = u32::MAX;
            owner.vectors = u32::MAX;
        }
    }

    /// Whether general register `index` may hold an owner-derived value.
    pub fn owner_derived(&self, index: usize) -> bool {
        self.owner
            .as_ref()
            .is_some_and(|owner| owner.registers & 1 << index != 0)
    }

    /// The offsets in the owner whose bytes may hold an owner-derived value.
    pub fn owner_derived_bytes(&self) -> BTreeSet<u64> {
        let Some(owner) = &self.owner else {
            return BTreeSet::new();
        };
        owner
            .memory
            .range(owner.start..owner.end)
            .map(|at| at - owner.start)
            .collect()
    }

    /// Replace which owner bytes may hold an owner-derived value with `offsets`.
    pub fn set_owner_derived_bytes(&mut self, offsets: &BTreeSet<u64>) {
        let Some(owner) = &mut self.owner else {
            return;
        };
        let (start, end) = (owner.start, owner.end);
        owner.memory.retain(|at| !(start..end).contains(at));
        owner
            .memory
            .extend(offsets.iter().map(|offset| start + offset));
    }

    /// Whether an owner-derived value went where code that this machine does not run can find
    /// it.
    pub fn owner_escaped(&self) -> bool {
        self.owner.as_ref().is_some_and(|owner| owner.escaped)
    }

    /// Record that an owner-derived value escaped, such as through a separately evaluated call.
    pub fn escape_owner(&mut self) {
        if let Some(owner) = &mut self.owner {
            owner.escaped = true;
        }
    }

    /// Whether this machine wrote memory outside the owner and its private stack frame.
    pub fn clobbers_outside_owner(&self) -> bool {
        self.owner
            .as_ref()
            .is_some_and(|owner| owner.clobbers_outside)
    }

    /// Whether the stack pointer moved by an unknown amount, so that no store is known to stay
    /// in the private frame.
    pub fn owner_stack_lost(&self) -> bool {
        self.owner.as_ref().is_some_and(|owner| owner.stack_lost)
    }

    /// Make every byte outside the owner unknown, as after a write to an unknown place that is
    /// not in the owner. Taint stays, since a byte may keep its value.
    pub fn forget_memory_outside_owner(&mut self) {
        let Some(owner) = &mut self.owner else {
            return self.forget_memory();
        };
        owner.clobbers_outside = true;
        let (start, end) = (owner.start, owner.end);
        for (_, byte) in self
            .memory
            .iter_mut()
            .filter(|(address, _)| !(start..end).contains(*address))
        {
            *byte = None;
        }
    }

    /// Make every known owner byte unknown.
    fn forget_owner(&mut self) {
        if let Some((start, end)) = self.owner_range() {
            self.forget_known_bytes(start, end - start);
        }
    }

    /// Return `value` from the call being handled, with `taint` for the registers that the call
    /// does not preserve.
    pub fn return_with_taint(&mut self, value: Option<u64>, taint: ReturnTaint) -> Call {
        if let Some(owner) = &mut self.owner {
            owner.returning = Some(taint);
        }
        Call::Return(value)
    }

    /// Return from a call whose code this machine does not run; see
    /// [`Machine::opaque_call_effects`].
    pub fn opaque_call(&mut self) -> Call {
        self.opaque_call_effects();

        Call::Return(None)
    }

    /// Apply the effects of the call being handled when this machine does not run its code. It
    /// may write any memory outside the owner. When the owner is within its reach, it may also
    /// write any owner byte, keep the owner where later code finds it, and return owner-derived
    /// values. A handler that knows the call's result, such as an allocation, returns it with
    /// [`Machine::return_with_taint`] afterwards.
    pub fn opaque_call_effects(&mut self) {
        let reaches = self
            .owner
            .as_ref()
            .is_none_or(|owner| owner.reaches_call(self.stack_pointer));
        self.forget_memory_outside_owner();
        if !reaches {
            self.return_with_taint(None, ReturnTaint::DISJOINT);
            return;
        }

        self.forget_owner();
        self.escape_owner();
        self.return_with_taint(None, ReturnTaint::REACHING);
    }

    /// A call returned. Without a taint from the call's handler, every register that the call
    /// did not preserve may point into the owner. A tracked machine keeps no vector value
    /// across a call, since a call preserves only the low halves of `v8` to `v15`.
    pub(super) fn owner_call_returned(&mut self) {
        let Some(owner) = &mut self.owner else {
            return;
        };
        let taint = owner.returning.take().unwrap_or(ReturnTaint::REACHING);
        owner.returned(taint);
        self.vectors = [None; 32];
    }

    /// Give `callee` every owner-derived value that it can read. The values of preserved
    /// registers are unknown to the callee, but their taint stays, because the callee may read or
    /// spill them.
    pub(super) fn copy_owner_taint_to_callee(&self, callee: &mut Self) {
        let Some(owner) = &self.owner else {
            return;
        };
        callee.owner = Some(Box::new(OwnerTaint {
            frame_end: self.stack_pointer,
            registers: owner.registers & REACHABLE_REGISTERS,
            vectors: owner.vectors & REACHABLE_VECTORS,
            clobbers_outside: false,
            stack_lost: false,
            inputs: Cell::new(false),
            returning: None,
            ..(**owner).clone()
        }));
    }

    pub(super) fn read_owner_register(&self, index: usize) {
        if let Some(owner) = &self.owner {
            owner.read_register(index);
        }
    }

    pub(super) fn read_owner_vector(&self, index: usize) {
        if let Some(owner) = &self.owner {
            owner.read_vector(index);
        }
    }

    /// General register `index` now holds a value computed from the present instruction's
    /// inputs.
    pub(super) fn assign_owner_register(&mut self, index: usize) {
        if let Some(owner) = &mut self.owner {
            let derived = owner.inputs.get();
            owner.set_register(index, derived);
        }
    }

    pub(super) fn set_owner_vector(&mut self, index: usize, derived: bool) {
        if let Some(owner) = &mut self.owner {
            owner.set_vector(index, derived);
        }
    }

    pub(super) fn lose_owner_stack(&mut self) {
        if let Some(owner) = &mut self.owner {
            owner.stack_lost = true;
        }
    }

    /// A definite store of `width` bytes at `address`.
    pub(super) fn store_owner_bytes(&mut self, address: u64, width: u64, derived: bool) {
        let stack_pointer = self.stack_pointer;
        if let Some(owner) = &mut self.owner {
            owner.store(address, width, derived, stack_pointer);
        }
    }

    /// A store to an unknown address, of an owner-derived value or not. The present
    /// instruction's inputs are those of the address. Returns the owner's range when the store
    /// cannot reach it.
    pub(super) fn store_owner_unknown(&mut self, derived: bool) -> Option<(u64, u64)> {
        let owner = self.owner.as_mut()?;
        owner.clobbers_outside = true;
        owner.escaped |= derived;

        (!owner.inputs.get()).then_some((owner.start, owner.end))
    }

    /// Whether a load of `width` bytes may read an owner-derived value. An unknown address may
    /// select any byte that its own inputs reach.
    pub(super) fn loaded_owner(
        &self,
        address_derived: bool,
        address: Option<u64>,
        width: u64,
    ) -> bool {
        let Some(owner) = &self.owner else {
            return false;
        };
        match address {
            None => address_derived || owner.escaped || owner.tainted_outside(),
            Some(address) => (address..address.saturating_add(width))
                .any(|at| owner.memory.contains(&at) || (owner.escaped && self.byte(at).is_none())),
        }
    }

    /// Join the owner taint of a loop head with this path's, and return whether it did not
    /// already cover this path.
    pub(super) fn widen_owner(&mut self, kept: Option<&OwnerTaint>) -> bool {
        match (&mut self.owner, kept) {
            (Some(owner), Some(kept)) => owner.widen(kept),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Code, Exit, ReadOnlyData};
    use super::*;
    use crate::engine::analysis::assembler::arm64;

    /// An address that no section maps, so a load from it is unknown and underived.
    const GLOBAL: u64 = 0x80000;

    /// A machine at 0x100 that tracks a 64-byte owner in x0 and holds `GLOBAL` in x9.
    fn tracked<'a>(code: &'a Code, data: &'a ReadOnlyData) -> (Machine<'a>, u64) {
        let mut machine = Machine::new(code, data);
        let owner = machine.reserve(64);
        machine.track_owner(owner, owner + 64);
        machine.set_register(0, owner);
        machine.derive_from_owner(0);
        machine.set_register(9, GLOBAL);

        (machine, owner)
    }

    /// Run `bytes` at 0x100, with every call opaque, and return the owner's word at +8.
    fn owner_word_after(bytes: &[u8]) -> Option<u64> {
        let code = Code::decode(&[(0x100, bytes)]).unwrap();
        let data = ReadOnlyData::default();
        let (mut machine, owner) = tracked(&code, &data);
        let exit = machine.run(0x100, &mut |_, machine| Ok(machine.opaque_call()));
        assert_eq!(exit, Ok(Exit::Returned));

        machine.read(owner + 8, 4)
    }

    #[test]
    fn a_store_through_a_preexisting_pointer_keeps_the_owner() {
        let bytes = arm64!(at 0x100;
            mov w8, #7;
            str w8, [x0, #8];
            ldr x3, [x9];
            str xzr, [x3];
            ret
        );
        assert_eq!(owner_word_after(&bytes), Some(7));
    }

    #[test]
    fn a_store_at_an_unknown_offset_from_the_owner_forgets_it() {
        let bytes = arm64!(at 0x100;
            mov w8, #7;
            str w8, [x0, #8];
            ldr x22, [x9];
            add x20, x0, #0x20;
            strb wzr, [x20, x22];
            ret
        );
        assert_eq!(owner_word_after(&bytes), None);
    }

    #[test]
    fn a_call_reaches_the_owner_through_arguments_preserved_registers_and_the_stack() {
        let unreachable = arm64!(at 0x100;
            mov w8, #7;
            str w8, [x0, #8];
            mov x0, #0;
            bl extern 0x200;
            ret
        );
        assert_eq!(owner_word_after(&unreachable), Some(7));

        let argument = arm64!(at 0x100;
            mov w8, #7;
            str w8, [x0, #8];
            bl extern 0x200;
            ret
        );
        let preserved = arm64!(at 0x100;
            mov x19, x0;
            mov w8, #7;
            str w8, [x19, #8];
            mov x0, #0;
            bl extern 0x200;
            ret
        );
        let stacked = arm64!(at 0x100;
            mov w8, #7;
            str w8, [x0, #8];
            str x0, [sp, #-16]!; // the owner's address in a stacked argument
            mov x0, #0;
            bl extern 0x200;
            ret
        );
        let slot = arm64!(at 0x100;
            mov w8, #7;
            str w8, [x0, #8];
            str x0, [sp, #-16]!;
            mov x0, #0;
            mov x2, sp; // a pointer to the slot that holds the owner's address
            bl extern 0x200;
            ret
        );
        let escaped = arm64!(at 0x100;
            mov w8, #7;
            str w8, [x0, #8];
            ldr x5, [x9];
            str x0, [x5]; // registers the owner where any later call can find it
            mov x0, #0;
            bl extern 0x200;
            ret
        );
        for bytes in [argument, preserved, stacked, slot, escaped] {
            assert_eq!(owner_word_after(&bytes), None);
        }
    }

    #[test]
    fn a_store_after_a_reaching_call_establishes_its_bytes_again() {
        let bytes = arm64!(at 0x100;
            mov x19, x0;
            bl extern 0x200;
            mov w8, #7;
            str w8, [x19, #8];
            ret
        );
        assert_eq!(owner_word_after(&bytes), Some(7));
    }

    #[test]
    fn a_stored_owner_address_stays_derived_after_it_becomes_unknown() {
        let general = arm64!(at 0x100;
            mov w8, #7;
            str w8, [x0, #8];
            str x0, [sp, #-16]!;
            ldr x5, [x9];
            str xzr, [x5]; // may overwrite the slot, which may still hold the owner's address
            ldr x3, [sp];
            str wzr, [x3];
            ret
        );
        let vector = arm64!(at 0x100;
            mov w8, #7;
            str w8, [x0, #8];
            fmov d1, x0;
            str d1, [sp, #-16]!;
            ldr x5, [x9];
            str xzr, [x5];
            ldr d2, [sp];
            fmov x3, d2;
            str wzr, [x3];
            ret
        );
        let selected = arm64!(at 0x100;
            mov w8, #7;
            str w8, [x0, #8];
            str x0, [sp, #-16]!;
            ldr x10, [x9];
            add x11, sp, x10; // an unknown stack address that may select the owner's slot
            ldr x12, [x11];
            str wzr, [x12];
            ret
        );
        for bytes in [general, vector, selected] {
            assert_eq!(owner_word_after(&bytes), None);
        }
    }

    #[test]
    fn a_load_after_an_escape_may_read_the_owner_address() {
        let known = arm64!(at 0x100;
            ldr x5, [x9];
            str x0, [x5]; // registers the owner at an unknown place
            mov w8, #7;
            str w8, [x0, #8];
            ldr x6, [x9];
            str wzr, [x6];
            ret
        );
        let unknown = arm64!(at 0x100;
            ldr x5, [x9];
            str x0, [x5];
            mov w8, #7;
            str w8, [x0, #8];
            ldr x6, [x5];
            str wzr, [x6];
            ret
        );
        for bytes in [known, unknown] {
            assert_eq!(owner_word_after(&bytes), None);
        }
    }

    #[test]
    fn a_call_keeps_the_derivation_of_preserved_vector_halves() {
        let bytes = arm64!(at 0x100;
            mov x19, x0;
            fmov d8, x0;
            mov x0, #0;
            bl extern 0x200;
            mov w8, #7;
            str w8, [x19, #8];
            fmov x3, d8; // unknown after the call, but still the owner's address
            str wzr, [x3];
            ret
        );
        assert_eq!(owner_word_after(&bytes), None);
    }

    #[test]
    fn a_tracked_call_changes_every_vector_value() {
        let bytes = arm64!(at 0x100;
            mov x0, #0;
            fmov d0, x9;
            bl extern 0x200;
            fmov x3, d0;
            ret
        );
        let code = Code::decode(&[(0x100, bytes.as_slice())]).unwrap();
        let data = ReadOnlyData::default();
        let (mut machine, _) = tracked(&code, &data);
        machine
            .run(0x100, &mut |_, machine| Ok(machine.opaque_call()))
            .unwrap();
        assert_eq!(machine.register(3), None);
    }

    #[test]
    fn a_callee_receives_the_derivation_of_vector_arguments_and_preserved_registers() {
        let caller = arm64!(at 0x100;
            mov x19, x0;
            fmov d0, x0;
            mov w8, #7;
            str w8, [x0, #8];
            ret
        );
        let vector = arm64!(at 0x200;
            fmov x10, d0;
            ldr x11, [x9];
            str wzr, [x10, x11];
            ret
        );
        let spilled = arm64!(at 0x200;
            str x19, [sp, #-16]!;
            ldr x5, [x9];
            str xzr, [x5];
            ldr x10, [sp], #16;
            str wzr, [x10];
            ret
        );
        for callee in [vector, spilled] {
            let code =
                Code::decode(&[(0x100, caller.as_slice()), (0x200, callee.as_slice())]).unwrap();
            let data = ReadOnlyData::default();
            let (mut machine, owner) = tracked(&code, &data);
            machine.run(0x100, &mut |_, _| unreachable!()).unwrap();
            machine.set_register(9, GLOBAL);
            let mut callee = machine.fresh_callee(&code, &data);
            callee.run(0x200, &mut |_, _| unreachable!()).unwrap();
            assert_eq!(callee.read(owner + 8, 4), None);
        }
    }

    #[test]
    fn a_loop_head_join_keeps_a_path_that_only_escaped() {
        // The loop leaves at its head, so only a later arrival there can leave after the escape.
        let bytes = arm64!(at 0x100;
            mov w8, #7;
            str w8, [x0, #8];
            ldr x6, [x9];
            str xzr, [x6]; // every arrival has written outside the owner
            ldr x5, [x9]; // the loop head
            str xzr, [x5];
            cbz x10, extern 0x124;
            str x0, [x5]; // lets the owner escape
            b extern 0x110;
            ldr x3, [x9];
            str wzr, [x3];
            ret
        );
        let code = Code::decode(&[(0x100, bytes.as_slice())]).unwrap();
        let data = ReadOnlyData::default();
        let (machine, owner) = tracked(&code, &data);
        let paths = machine.run_paths_joining(0x100, &mut |_, machine| Ok(machine.opaque_call()));
        let returned: Vec<_> = paths
            .iter()
            .filter(|path| path.end == Ok(Exit::Returned))
            .collect();
        assert!(!returned.is_empty());
        assert!(
            returned
                .iter()
                .any(|path| path.machine.read(owner + 8, 4).is_none())
        );
    }
}
