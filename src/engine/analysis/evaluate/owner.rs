//! Which values may point into a freshly allocated owner, and which writes may reach it.
//!
//! A pointer that existed before the owner's allocation, or that was loaded from a byte that no
//! owner-derived value was stored to, cannot point into the owner. A store through it is disjoint
//! from the owner. A value derived from the owner through registers, vectors, tainted memory,
//! calls or path joins may point anywhere into it.
//!
//! A byte's taint is cleared only by a store of an underived value to its known address. A store
//! to an unknown address and a forgotten byte keep their taint, since the byte may still hold its
//! earlier value.
//!
//! The method assumes that code changes only the object it is given. Members are built in address
//! order, so a later call or store leaves the members before its object intact:
//!
//! - A call that this machine does not run may write the owner only from the lowest owner address
//!   it is given in `x0` to `x8`, or the whole owner when `x0` is derived with an unknown value.
//!   A call that is given no owner address writes no owner byte and returns no derived value.
//! - A store to an unknown address whose base register holds a known owner address writes only
//!   from that base on.
//! - An owner address that went to memory outside this machine's tracking, such as a registration
//!   array, is not found again by later loads. Only tainted bytes yield derived values.
//!
//! Tracking starts with every held value derived from the owner. The one narrowing is an
//! allocator's return, before which no value can point into the new owner.
use std::cell::Cell;
use std::collections::BTreeSet;

use super::{Call, Machine, STACK_TOP};
use crate::engine::analysis::stop::CauseKind;

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

    /// No value that the machine holds now derives from the owner.
    fn underive_held_values(&mut self) {
        self.registers = 0;
        self.vectors = 0;
        self.memory.clear();
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
        }

        for at in address..limit {
            if derived {
                self.memory.insert(at);
            } else {
                self.memory.remove(&at);
            }
        }
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
            && (!self.clobbers_outside || kept.clobbers_outside)
            && (!self.stack_lost || kept.stack_lost);
        self.registers |= kept.registers;
        self.vectors |= kept.vectors;
        self.memory.extend(&kept.memory);
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
    /// Track which values may point into the owner at `start..end`. Nothing is known about where
    /// its address went: every register, vector and byte that this machine holds may point into
    /// it. The whole stack is this machine's private frame.
    pub fn track_owner(&mut self, start: u64, end: u64) {
        self.owner = Some(Box::new(OwnerTaint {
            start,
            end,
            frame_end: STACK_TOP,
            registers: u32::MAX,
            vectors: u32::MAX,
            memory: self.memory.keys().copied().collect(),
            clobbers_outside: false,
            stack_lost: false,
            inputs: Cell::new(false),
            returning: None,
        }));
    }

    /// Track the owner at `start..end` that the allocator call being handled returns, and return
    /// it. Every value that this machine holds was computed before the allocation, so none points
    /// into the owner. The allocator may leave it in any register that the call does not preserve.
    pub fn return_allocated_owner(&mut self, start: u64, end: u64) -> Call {
        self.track_owner(start, end);
        if let Some(owner) = &mut self.owner {
            owner.underive_held_values();
        }

        self.return_with_taint(Some(start), ReturnTaint::REACHING)
    }

    /// Track an owner at `start..end` from which no value derives, as an authored test arranges.
    #[cfg(test)]
    pub fn track_private_owner(&mut self, start: u64, end: u64) {
        self.track_owner(start, end);
        if let Some(owner) = &mut self.owner {
            owner.underive_held_values();
        }
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

    /// Whether general register `index` may hold an owner-derived value.
    pub fn owner_derived(&self, index: usize) -> bool {
        self.owner
            .as_ref()
            .is_some_and(|owner| owner.registers & 1 << index != 0)
    }

    /// Every byte, by address, that may hold an owner-derived value, inside the owner or not.
    pub fn owner_derived_memory(&self) -> BTreeSet<u64> {
        self.owner
            .as_ref()
            .map(|owner| owner.memory.clone())
            .unwrap_or_default()
    }

    /// Install the owner-derived bytes that a constructor left, as by
    /// [`Machine::owner_derived_memory`]. They replace this machine's taint in the owner, whose
    /// state the constructor reports in full. Outside the owner they add to it.
    pub fn install_owner_derived_memory(&mut self, addresses: &BTreeSet<u64>) {
        let Some(owner) = &mut self.owner else {
            return;
        };
        let (start, end) = (owner.start, owner.end);
        owner.memory.retain(|at| !(start..end).contains(at));
        owner.memory.extend(addresses);
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

    /// The lowest owner address that the call being handled is given: a known owner address
    /// in `x0` to `x8`, or the owner's start when `x0` is owner-derived with an unknown value.
    /// `None` when the call is given no owner address, so it cannot change the owner.
    ///
    /// Other derived registers are ignored: their values may be left over from earlier code, and
    /// the callee's arity is not known.
    pub fn given_owner_address(&self) -> Option<u64> {
        let owner = self.owner.as_ref()?;
        let unknown_receiver = owner.registers & 1 != 0 && self.register(0).is_none();
        let known = (0..=8)
            .filter_map(|index| self.register(index))
            .filter(|value| (owner.start..owner.end).contains(value));

        known.chain(unknown_receiver.then_some(owner.start)).min()
    }

    /// The taint of the registers that the call being handled does not preserve: a call that is
    /// given an owner address may return one in any of them.
    pub fn given_call_taint(&self) -> ReturnTaint {
        match self.given_owner_address() {
            Some(_) => ReturnTaint::REACHING,
            None => ReturnTaint::DISJOINT,
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
        self.opaque_call_for(CauseKind::Invalidated)
    }

    /// [`Machine::opaque_call`], whose owner writes have the cause `kind` while tracing causes,
    /// such as [`CauseKind::Unfollowed`] for a bound constructor whose walk was rejected.
    pub(crate) fn opaque_call_for(&mut self, kind: CauseKind) -> Call {
        self.opaque_call_effects_for(kind);

        Call::Return(None)
    }

    /// Apply the effects of the call being handled when this machine does not run its code. It
    /// may write any memory outside the owner, and the owner from the lowest owner address that it
    /// is given (see [`Machine::given_owner_address`]). A call given an owner address may return
    /// owner-derived values and leave them in any byte that it may write. A handler that knows the call's result, such as an allocation,
    /// returns it with [`Machine::return_with_taint`] afterwards.
    pub fn opaque_call_effects(&mut self) {
        self.opaque_call_effects_for(CauseKind::Invalidated);
    }

    fn opaque_call_effects_for(&mut self, kind: CauseKind) {
        let given = self.given_owner_address();
        self.forget_memory_outside_owner();
        let (Some(from), Some((_, end))) = (given, self.owner_range()) else {
            self.return_with_taint(None, ReturnTaint::DISJOINT);
            return;
        };

        self.forget_known_bytes_for(from, end - from, kind);
        self.derive_bytes_a_call_may_write(from);
        self.return_with_taint(None, ReturnTaint::REACHING);
    }

    /// A call given the owner address `from` may store an owner address in the objects that it is
    /// given: the owner from `from` on, and each object outside the owner whose address is in `x0`
    /// to `x8`, such as a stack out-parameter. An object outside the owner has an unknown size. On
    /// the stack it may reach the end of the frame that holds it. Elsewhere it is taken as a
    /// pointer-sized slot and any run of held bytes that continues it; memory that this machine
    /// does not hold falls under the method's assumption about escaped owner addresses.
    fn derive_bytes_a_call_may_write(&mut self, from: u64) {
        let Some(owner) = &self.owner else {
            return;
        };
        let (start, end, frame_end) = (owner.start, owner.end, owner.frame_end);
        // A moved stack pointer gives no frame to bound a stack object.
        let stack = (!owner.stack_lost).then_some(self.stack_pointer..STACK_TOP);
        let given_outside: Vec<u64> = (0..=8)
            .filter_map(|index| self.register(index))
            .filter(|address| !(start..end).contains(address))
            .collect();
        let mut written: Vec<u64> = (from..end).collect();
        for address in given_outside {
            let object_end = match &stack {
                Some(stack) if stack.contains(&address) && address < frame_end => frame_end,
                Some(stack) if stack.contains(&address) => STACK_TOP,
                _ => self.held_object_end(address),
            };
            written.extend(address..object_end);
        }
        if let Some(owner) = &mut self.owner {
            owner.memory.extend(written);
        }
    }

    /// The end of a pointer-sized slot at `address` and the run of held bytes that continues it.
    fn held_object_end(&self, address: u64) -> u64 {
        let mut object_end = address.saturating_add(8);
        for &held in self.memory.range(object_end..).map(|(at, _)| at) {
            if held != object_end {
                break;
            }
            object_end = held.saturating_add(1);
        }

        object_end
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
    /// spill them. A callee of a machine whose stack pointer moved by an unknown amount has no
    /// established frame either.
    pub(super) fn copy_owner_taint_to_callee(&self, callee: &mut Self) {
        let Some(owner) = &self.owner else {
            return;
        };
        callee.owner = Some(Box::new(OwnerTaint {
            frame_end: self.stack_pointer,
            registers: owner.registers & REACHABLE_REGISTERS,
            vectors: owner.vectors & REACHABLE_VECTORS,
            clobbers_outside: false,
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

    /// A store to an unknown address whose base register holds `base`. The present
    /// instruction's inputs are those of the address. Returns the part of the owner that the
    /// store cannot reach: the whole owner when the address is underived, and the bytes before
    /// `base` when `base` is a known owner address.
    pub(super) fn store_owner_unknown(&mut self, base: Option<u64>) -> Option<(u64, u64)> {
        let owner = self.owner.as_mut()?;
        owner.clobbers_outside = true;
        if !owner.inputs.get() {
            return Some((owner.start, owner.end));
        }

        base.filter(|base| (owner.start..owner.end).contains(base))
            .map(|base| (owner.start, base))
    }

    /// Whether a load of `width` bytes may read an owner-derived value: a tainted byte, or any
    /// byte that an unknown address's own inputs reach.
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
            None => address_derived || owner.tainted_outside(),
            Some(address) => {
                (address..address.saturating_add(width)).any(|at| owner.memory.contains(&at))
            }
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
        machine.track_private_owner(owner, owner + 64);
        machine.set_register(0, owner);
        machine.derive_from_owner(0);
        machine.set_register(9, GLOBAL);

        (machine, owner)
    }

    /// Run `bytes` at 0x100, with every call opaque, and return the owner's words at `offsets`.
    fn owner_words_after(bytes: &[u8], offsets: &[u64]) -> Vec<Option<u64>> {
        let code = Code::decode(&[(0x100, bytes)]).unwrap();
        let data = ReadOnlyData::default();
        let (mut machine, owner) = tracked(&code, &data);
        let exit = machine.run(0x100, &mut |_, machine| Ok(machine.opaque_call()));
        assert_eq!(exit, Ok(Exit::Returned));

        offsets
            .iter()
            .map(|offset| machine.read(owner + offset, 4))
            .collect()
    }

    #[test]
    fn tracking_starts_with_every_value_owner_derived() {
        let bytes = arm64!(at 0x100;
            fmov x20, d8;
            ldr x21, [sp]; // a known byte written before tracking
            mov w8, #7;
            str w8, [x0, #8];
            strb wzr, [x10]; // x10 is unknown and was held before tracking
            ret
        );
        let code = Code::decode(&[(0x100, bytes.as_slice())]).unwrap();
        let data = ReadOnlyData::default();
        let mut machine = Machine::new(&code, &data);
        let owner = machine.reserve(64);
        machine.set_register(0, owner);
        machine.write(machine.stack_pointer(), 8, GLOBAL);
        machine.track_owner(owner, owner + 64);
        assert!((0..=30).all(|index| machine.owner_derived(index)));

        let exit = machine.run(0x100, &mut |_, machine| Ok(machine.opaque_call()));
        assert_eq!(exit, Ok(Exit::Returned));
        assert!(machine.owner_derived(20));
        assert!(machine.owner_derived(21));
        assert_eq!(machine.read(owner + 8, 4), None);
    }

    /// Run `bytes` at 0x100, whose first call allocates a 64-byte owner, with a known stack word
    /// written before the call. Returns the machine and the owner.
    fn allocated<'a>(code: &'a Code, data: &'a ReadOnlyData) -> (Machine<'a>, u64) {
        let mut machine = Machine::new(code, data);
        machine.write(machine.stack_pointer(), 8, GLOBAL);
        let exit = machine.run(0x100, &mut |_, machine| {
            let owner = machine.reserve(64);
            Ok(machine.return_allocated_owner(owner, owner + 64))
        });
        assert_eq!(exit, Ok(Exit::Returned));
        let (owner, _) = machine.owner_range().unwrap();

        (machine, owner)
    }

    #[test]
    fn an_allocated_owner_derives_only_what_its_allocator_may_return() {
        let bytes = arm64!(at 0x100;
            bl extern 0x200;
            fmov x20, d8;
            ldr x21, [sp]; // a known byte written before the allocation
            ret
        );
        let code = Code::decode(&[(0x100, bytes.as_slice())]).unwrap();
        let data = ReadOnlyData::default();
        let (machine, _) = allocated(&code, &data);
        for index in 0..=30 {
            assert_eq!(
                machine.owner_derived(index),
                index <= 18 || index == 20,
                "x{index}"
            );
        }

        let preexisting = arm64!(at 0x100;
            bl extern 0x200;
            mov w8, #7;
            str w8, [x0, #8];
            strb wzr, [x19]; // x19 is unknown and was held before the allocation
            ret
        );
        let clobbered = arm64!(at 0x100;
            bl extern 0x200;
            mov w8, #7;
            str w8, [x0, #8];
            strb wzr, [x9];
            ret
        );
        for (bytes, kept) in [(preexisting, true), (clobbered, false)] {
            let code = Code::decode(&[(0x100, bytes.as_slice())]).unwrap();
            let (machine, owner) = allocated(&code, &data);
            assert_eq!(machine.read(owner + 8, 4), kept.then_some(7));
        }
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
        assert_eq!(owner_words_after(&bytes, &[8]), [Some(7)]);
    }

    #[test]
    fn a_store_at_an_unknown_offset_writes_the_owner_from_its_base() {
        let known_base = arm64!(at 0x100;
            mov w8, #7;
            str w8, [x0, #8];
            str w8, [x0, #0x20];
            ldr x22, [x9];
            add x20, x0, #0x20;
            strb wzr, [x20, x22];
            ret
        );
        assert_eq!(owner_words_after(&known_base, &[8, 0x20]), [Some(7), None]);

        let unknown_base = arm64!(at 0x100;
            mov w8, #7;
            str w8, [x0, #8];
            ldr x22, [x9];
            add x20, x0, x22; // derived from the owner, with an unknown value
            strb wzr, [x20];
            ret
        );
        assert_eq!(owner_words_after(&unknown_base, &[8]), [None]);
    }

    #[test]
    fn a_call_writes_the_owner_from_the_lowest_address_it_is_given() {
        let receiver = arm64!(at 0x100;
            mov w8, #7;
            str w8, [x0, #8];
            str w8, [x0, #0x20];
            add x0, x0, #0x20;
            bl extern 0x200;
            ret
        );
        let argument = arm64!(at 0x100;
            mov w8, #7;
            str w8, [x0, #8];
            str w8, [x0, #0x20];
            add x3, x0, #0x20;
            mov x0, #0;
            bl extern 0x200;
            ret
        );
        for bytes in [receiver, argument] {
            assert_eq!(owner_words_after(&bytes, &[8, 0x20]), [Some(7), None]);
        }

        let lowest = arm64!(at 0x100;
            mov w8, #7;
            str w8, [x0, #8];
            str w8, [x0, #0x20];
            add x3, x0, #8;
            add x0, x0, #0x20;
            bl extern 0x200;
            ret
        );
        let unknown_receiver = arm64!(at 0x100;
            mov w8, #7;
            str w8, [x0, #8];
            str w8, [x0, #0x20];
            ldr x22, [x9];
            add x0, x0, x22;
            bl extern 0x200;
            ret
        );
        for bytes in [lowest, unknown_receiver] {
            assert_eq!(owner_words_after(&bytes, &[8, 0x20]), [None, None]);
        }
    }

    #[test]
    fn a_call_given_no_owner_address_keeps_the_owner() {
        let unreachable = arm64!(at 0x100;
            mov w8, #7;
            str w8, [x0, #8];
            mov x0, #0;
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
            mov x2, sp; // a pointer to the slot that holds the owner's address
            bl extern 0x200;
            ret
        );
        let stale_arguments = arm64!(at 0x100;
            mov w8, #7;
            str w8, [x0, #8];
            ldr x22, [x9];
            add x1, x0, x22; // derived, with an unknown value left from earlier code
            mov x0, #0;
            bl extern 0x200;
            ret
        );
        let derived_elsewhere = arm64!(at 0x100;
            mov x19, x0;
            mov w8, #7;
            str w8, [x19, #8];
            sub x0, x19, x19; // derived from the owner, but a known address outside it
            bl extern 0x200;
            ret
        );
        let registered = arm64!(at 0x100;
            mov w8, #7;
            str w8, [x0, #8];
            ldr x5, [x9];
            str x0, [x5]; // registers the owner where a later call can find it
            mov x0, #0;
            bl extern 0x200;
            ret
        );
        for bytes in [
            unreachable,
            preserved,
            stacked,
            stale_arguments,
            derived_elsewhere,
            registered,
        ] {
            assert_eq!(owner_words_after(&bytes, &[8]), [Some(7)]);
        }
    }

    #[test]
    fn a_call_given_an_owner_address_may_leave_one_in_what_it_writes() {
        let member = arm64!(at 0x100;
            mov x19, x0;
            add x0, x19, #32;
            bl extern 0x200;
            mov w8, #7;
            str w8, [x19, #8];
            ldr x3, [x19, #32]; // the call may have stored an owner address here
            str wzr, [x3];
            ret
        );
        let out_parameter = arm64!(at 0x100;
            mov x19, x0;
            sub sp, sp, #16;
            add x0, x19, #32;
            mov x1, sp; // a fresh stack slot that the call may write
            bl extern 0x200;
            mov w8, #7;
            str w8, [x19, #8];
            ldr x3, [sp];
            str wzr, [x3];
            ret
        );
        let later_field = arm64!(at 0x100;
            mov x19, x0;
            sub sp, sp, #16;
            add x0, x19, #32;
            mov x1, sp; // a fresh 16-byte object that the call may write
            bl extern 0x200;
            mov w8, #7;
            str w8, [x19, #8];
            ldr x3, [sp, #8];
            str wzr, [x3];
            ret
        );
        let held_object = arm64!(at 0x100;
            mov x19, x0;
            mov x1, x9; // a known object outside the stack
            str xzr, [x1];
            str xzr, [x1, #8];
            add x0, x19, #32;
            bl extern 0x200;
            mov w8, #7;
            str w8, [x19, #8];
            ldr x3, [x9, #8];
            str wzr, [x3];
            ret
        );
        for bytes in [member, out_parameter, later_field, held_object] {
            assert_eq!(owner_words_after(&bytes, &[8]), [None]);
        }

        let given_nothing = arm64!(at 0x100;
            mov x19, x0;
            str xzr, [sp, #-16]!;
            mov x0, #0;
            mov x1, sp;
            bl extern 0x200;
            mov w8, #7;
            str w8, [x19, #8];
            ldr x3, [sp];
            str wzr, [x3];
            ret
        );
        assert_eq!(owner_words_after(&given_nothing, &[8]), [Some(7)]);
    }

    #[test]
    fn a_moved_stack_pointer_bounds_no_stack_object() {
        let bytes = arm64!(at 0x100;
            mov x19, x0;
            mov w8, #7;
            str w8, [x19, #8];
            mov x1, #0x80000;
            mov sp, x1; // far below the frame
            add x0, x19, #32;
            bl extern 0x200;
            ret
        );
        let code = Code::decode(&[(0x100, bytes.as_slice())]).unwrap();
        let data = ReadOnlyData::default();
        let (mut machine, owner) = tracked(&code, &data);
        let exit = machine.run(0x100, &mut |_, machine| Ok(machine.opaque_call()));
        assert_eq!(exit, Ok(Exit::Returned));
        assert!(machine.owner_stack_lost());
        assert_eq!(machine.read(owner + 8, 4), Some(7));

        let callee = machine.fresh_callee(&code, &data);
        assert!(callee.owner_stack_lost());
    }

    #[test]
    fn only_a_call_given_an_owner_address_returns_one() {
        let given_nothing = arm64!(at 0x100;
            mov w8, #7;
            str w8, [x0, #8];
            mov x0, #0;
            bl extern 0x200;
            str wzr, [x0];
            str wzr, [x1];
            ret
        );
        assert_eq!(owner_words_after(&given_nothing, &[8]), [Some(7)]);

        let given_a_member = arm64!(at 0x100;
            mov w8, #7;
            str w8, [x0, #8];
            add x0, x0, #0x20;
            bl extern 0x200;
            str wzr, [x1];
            ret
        );
        assert_eq!(owner_words_after(&given_a_member, &[8]), [None]);
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
        assert_eq!(owner_words_after(&bytes, &[8]), [Some(7)]);
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
        let global = arm64!(at 0x100;
            mov w8, #7;
            str w8, [x0, #8];
            str x0, [x9]; // the owner's address at a known global
            ldr x5, [x10];
            str xzr, [x5];
            ldr x3, [x9];
            str wzr, [x3];
            ret
        );
        for bytes in [general, vector, selected, global] {
            assert_eq!(owner_words_after(&bytes, &[8]), [None]);
        }
    }

    #[test]
    fn a_load_of_untainted_memory_is_not_the_owner_address() {
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
            assert_eq!(owner_words_after(&bytes, &[8]), [Some(7)]);
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
        assert_eq!(owner_words_after(&bytes, &[8]), [None]);
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
    fn a_loop_head_join_keeps_a_path_that_only_stored_the_owner() {
        // The loop leaves after its head, so only a later arrival there can leave after the store.
        let bytes = arm64!(at 0x100;
            mov w8, #7;
            str w8, [x0, #8];
            sub x11, sp, #16;
            ldr x6, [x9];
            str xzr, [x6]; // every arrival has written outside the owner
            ldr x5, [x9]; // the loop head
            str xzr, [x5]; // forgets the slot's value but not its derivation
            cbz x10, extern 0x128;
            str x0, [x11];
            b extern 0x114;
            ldr x3, [x11];
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
