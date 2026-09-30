//! Effects shared by bounded constructor-summary consumers.
use super::evaluate::Machine;
use std::collections::BTreeMap;

/// Replace the receiver's remaining span with only the constructor's proven vtable points.
/// The caller establishes ownership and the allocation end; summaries do not retain embedded state.
pub(super) fn install_vtables(
    machine: &mut Machine<'_>,
    receiver: u64,
    end: u64,
    points: &BTreeMap<u64, u64>,
) -> Option<()> {
    let span = end.checked_sub(receiver)?;
    for &offset in points.keys() {
        if offset.checked_add(8)? > span {
            return None;
        }
    }
    machine.forget(receiver, span);
    for (&offset, &point) in points {
        machine.write(receiver + offset, 8, point);
    }
    Some(())
}

/// Constructor-agreed bytes, return register and caller-memory invalidation.
pub(super) struct InitialState {
    /// Initialized bytes relative to the constructor receiver.
    pub bytes: Option<BTreeMap<u64, u8>>,
    /// Invalidate tracked caller memory before installing bytes proved after an escaped child write.
    pub invalidates_caller_memory: bool,
    /// The value agreed for x0 at every normal return.
    pub returned: Option<u64>,
}

/// Evaluate a constructor with the call's arguments and established caller bytes.
/// Only bound constructors are entered. Unknown calls and incomplete nested walks invalidate
/// tracked caller memory on the entered path. Every normal return must agree. A write outside
/// the receiver or private stack frame invalidates the whole owner proof. Consumers withhold
/// all entered-body contributions on invalidation and retain their independent summary-only
/// evaluation; baseline points and bytes never depend on this walk.
pub(super) fn initial_state(
    code: &super::evaluate::Code,
    data: &super::evaluate::ReadOnlyData,
    constructors: &BTreeMap<u64, BTreeMap<u64, u64>>,
    target: u64,
    caller: &Machine<'_>,
    owner: u64,
    end: u64,
) -> Option<InitialState> {
    ConstructorWalk {
        code,
        data,
        constructors,
        owner,
        end,
    }
    .run(target, caller, 0)
}

struct ConstructorWalk<'a> {
    code: &'a super::evaluate::Code,
    data: &'a super::evaluate::ReadOnlyData,
    constructors: &'a BTreeMap<u64, BTreeMap<u64, u64>>,
    owner: u64,
    end: u64,
}

impl ConstructorWalk<'_> {
    fn run(&self, target: u64, caller: &Machine<'_>, depth: usize) -> Option<InitialState> {
        use super::evaluate::{Call, Exit};

        let receiver = caller.register(0)?;
        if depth >= 8 || !(self.owner..self.end).contains(&receiver) {
            return None;
        }
        let mut machine = caller.fresh_callee(self.code, self.data);
        machine.watch_store_span(self.owner, receiver, self.end);
        for index in 0..9 {
            if let Some(value) = caller.register(index) {
                machine.set_register(index, value);
            }
        }
        machine.intercept_tail_calls(self.constructors.keys().copied().collect());
        let mut invalidates_caller_memory = false;
        let paths = machine.run_paths(target, &mut |target, machine| {
            let Some(target) = target.filter(|target| self.constructors.contains_key(target))
            else {
                machine.forget_memory();
                invalidates_caller_memory = true;
                return Ok(Call::Return(None));
            };
            let Some(nested) = machine
                .register(0)
                .filter(|nested| (receiver..self.end).contains(nested))
            else {
                machine.forget_memory();
                invalidates_caller_memory = true;
                return Ok(Call::Return(None));
            };
            let Some(state) = self.run(target, machine, depth + 1) else {
                machine.forget_memory();
                invalidates_caller_memory = true;
                return Ok(Call::Return(None));
            };
            if state.invalidates_caller_memory {
                machine.forget_memory();
                invalidates_caller_memory = true;
            }
            if let Some(bytes) = state.bytes {
                install_initial_state(machine, nested, self.end, &bytes);
            } else {
                forget_initial_bytes(machine, nested, self.end);
            }
            Ok(Call::Return(state.returned))
        });
        let escaped_store = paths.iter().any(|path| path.machine.escaped_store());
        let unresolved = || {
            (escaped_store || invalidates_caller_memory).then_some(InitialState {
                bytes: None,
                returned: None,
                invalidates_caller_memory: true,
            })
        };
        let mut bytes: Option<BTreeMap<u64, u8>> = None;
        let mut returned = None;
        for path in paths {
            if !matches!(path.end, Ok(Exit::Returned)) {
                return unresolved();
            }
            let current = path.machine.known_bytes(receiver, self.end - receiver);
            if let Some(agreed) = &mut bytes {
                agreed.retain(|offset, byte| current.get(offset) == Some(byte));
            } else {
                returned = Some(path.machine.register(0));
                bytes = Some(current);
            }
            if returned != Some(path.machine.register(0)) {
                returned = Some(None);
            }
        }
        if escaped_store {
            return Some(InitialState {
                bytes: None,
                returned: returned.flatten(),
                invalidates_caller_memory: true,
            });
        }
        let Some(mut bytes) = bytes else {
            return unresolved();
        };
        // The outer receiver keeps its existing compiler vtable proof. New embedded points
        // require constructor-written words; a partial word is never completed from metadata.
        for (&offset, &point) in self.constructors.get(&target)? {
            if offset
                .checked_add(8)
                .is_none_or(|end| end > self.end - receiver)
            {
                return unresolved();
            }
            if (0..8).any(|index| {
                bytes
                    .get(&(offset + index))
                    .is_some_and(|byte| *byte != (point >> (index * 8)) as u8)
            }) {
                return unresolved();
            }
            if depth == 0 && (0..8).all(|index| !bytes.contains_key(&(offset + index))) {
                for index in 0..8 {
                    bytes.insert(offset + index, (point >> (index * 8)) as u8);
                }
            }
        }
        Some(InitialState {
            bytes: Some(bytes),
            returned: returned.flatten(),
            invalidates_caller_memory,
        })
    }
}

/// Replace the receiver's remaining span with constructor-agreed bytes.
pub(super) fn install_initial_state(
    machine: &mut Machine<'_>,
    receiver: u64,
    end: u64,
    bytes: &BTreeMap<u64, u8>,
) {
    forget_initial_bytes(machine, receiver, end);
    for (&offset, &byte) in bytes {
        machine.write(receiver + offset, 1, byte.into());
    }
}

// These consumers reserve scratch allocations with no read-only backing. Invalidating known
// bytes suffices; filling the rest of a 64 KiB owner with unknown entries only enlarges path clones.
pub(super) fn forget_initial_bytes(machine: &mut Machine<'_>, receiver: u64, end: u64) {
    for offset in machine.known_bytes(receiver, end - receiver).keys() {
        machine.forget(receiver + offset, 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::evaluate::{Code, ReadOnlyData};

    #[test]
    fn callee_frame_does_not_alias_a_caller_stack_argument() {
        use crate::engine::analysis::assembler::{Arm64, arm64};
        let mut body = Arm64::at(0x1000);
        body.prologue();
        arm64!(body;
            mov w8, #9;
            str w8, [sp];
            ldr w8, [x1];
            str w8, [x0, #32]
        );
        body.epilogue();
        arm64!(body; ret);
        let bytes = body.bytes();
        let code = Code::decode(&[(0x1000, bytes.as_slice())]).unwrap();
        let data = ReadOnlyData::default();
        let mut caller = Machine::new(&code, &data);
        caller.set_stack_pointer(caller.stack_pointer() - 32);
        let owner = caller.reserve(64);
        caller.set_register(0, owner);
        caller.set_register(1, caller.stack_pointer());
        let constructors = BTreeMap::from([(0x1000, BTreeMap::new())]);
        let state = initial_state(
            &code,
            &data,
            &constructors,
            0x1000,
            &caller,
            owner,
            owner + 64,
        )
        .unwrap();
        assert!(!state.bytes.unwrap().contains_key(&32));

        caller.write(caller.stack_pointer(), 4, 7);
        let state = initial_state(
            &code,
            &data,
            &constructors,
            0x1000,
            &caller,
            owner,
            owner + 64,
        )
        .unwrap();
        assert_eq!(
            super::super::durations::word(&state.bytes.unwrap(), 32),
            Some(7)
        );
    }

    #[test]
    fn external_stack_pointer_cannot_preserve_a_stale_caller_load() {
        use crate::engine::analysis::assembler::{Arm64, arm64};
        let mut parent = Arm64::at(0x1000);
        parent.prologue();
        arm64!(parent;
            mov x19, x0;
            mov x20, #0x80000;
            mov x1, x20;
            mov w2, #9;
            add x0, x19, #80;
            bl #0x2000;
            ldr w8, [x20];
            str w8, [x19, #32];
            mov x0, x19
        );
        parent.epilogue();
        arm64!(parent; ret);
        let mut child = Arm64::at(0x2000);
        arm64!(child;
            mov x9, sp;
            mov sp, x1;
            str w2, [x1];
            mov sp, x9;
            ret
        );
        let parent_bytes = parent.bytes();
        let child_bytes = child.bytes();
        let code = Code::decode(&[(0x1000, &parent_bytes), (0x2000, &child_bytes)]).unwrap();
        let data = ReadOnlyData::default();
        let mut caller = Machine::new(&code, &data);
        let owner = caller.reserve(128);
        caller.set_register(0, owner);
        caller.write(0x80000, 4, 7);
        let constructors = BTreeMap::from([(0x1000, BTreeMap::new()), (0x2000, BTreeMap::new())]);
        let state = initial_state(
            &code,
            &data,
            &constructors,
            0x1000,
            &caller,
            owner,
            owner + 128,
        )
        .unwrap();
        assert!(state.invalidates_caller_memory);
        assert_eq!(
            state
                .bytes
                .as_ref()
                .and_then(|bytes| super::super::durations::word(bytes, 32)),
            None
        );
    }

    #[test]
    fn constructor_summary_forgets_embedded_state_and_rejects_overflowing_points() {
        let code = Code::from_rows(vec![]);
        let data = ReadOnlyData::default();
        let mut machine = Machine::new(&code, &data);
        let owner = machine.reserve(64);
        machine.write(owner, 8, 1);
        machine.write(owner + 24, 8, 2);
        machine.write(owner + 56, 8, 3);
        assert_eq!(
            install_vtables(
                &mut machine,
                owner + 16,
                owner + 64,
                &BTreeMap::from([(0, 4), (32, 5)])
            ),
            Some(())
        );
        assert_eq!(machine.read(owner, 8), Some(1));
        assert_eq!(machine.read(owner + 16, 8), Some(4));
        assert_eq!(machine.read(owner + 48, 8), Some(5));
        assert_eq!(machine.read(owner + 24, 8), None);
        assert_eq!(machine.read(owner + 56, 8), None);
        for offset in [41, u64::MAX] {
            assert_eq!(
                install_vtables(
                    &mut machine,
                    owner + 16,
                    owner + 64,
                    &BTreeMap::from([(offset, 6)])
                ),
                None
            );
        }
    }
}
