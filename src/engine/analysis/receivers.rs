//! Effects shared by bounded constructor-summary consumers.
use super::evaluate::{Call, Code, Exit, Machine, ReadOnlyData, ReturnTaint};
use std::collections::{BTreeMap, BTreeSet};

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

/// The owner state that an entered constructor leaves on every path that returns.
struct InitialState {
    /// Bytes of the whole owner, relative to its start, that every returning path agrees on.
    bytes: BTreeMap<u64, u8>,
    /// Owner offsets whose bytes may hold an owner-derived value on some returning path.
    tainted: BTreeSet<u64>,
    /// The value agreed for x0 at every normal return.
    returned: Option<u64>,
    /// Whether x0 may point into the owner on some returning path.
    returned_owner_derived: bool,
    /// Whether some path left an owner-derived value where other code can find it.
    escaped: bool,
    /// Whether some path wrote memory outside the owner and its private stack frame.
    clobbers_outside: bool,
}

impl InitialState {
    fn of(machine: &Machine<'_>, owner: u64, end: u64) -> Self {
        Self {
            bytes: machine.known_bytes(owner, end - owner),
            tainted: machine.owner_derived_bytes(),
            returned: machine.register(0),
            returned_owner_derived: machine.owner_derived(0),
            escaped: machine.owner_escaped(),
            clobbers_outside: machine.clobbers_outside_owner(),
        }
    }

    /// The state that both this path and `other` leave: agreed bytes and every possible effect.
    fn join(mut self, other: Self) -> Self {
        self.bytes
            .retain(|offset, byte| other.bytes.get(offset) == Some(byte));
        self.tainted.extend(other.tainted);
        if self.returned != other.returned {
            self.returned = None;
        }
        self.returned_owner_derived |= other.returned_owner_derived;
        self.escaped |= other.escaped;
        self.clobbers_outside |= other.clobbers_outside;
        self
    }
}

/// Bound constructors of one fresh owner at `owner..end`, whose entered bodies are in `code`.
///
/// An entered body may make an owner byte unknown through a call it does not run, or through a
/// store that may point into the owner; a later store to a known address establishes the byte
/// again. A walk that cannot be followed, such as one with a path that does not return, is a call
/// that this machine does not run. Consumers keep their summary-only evaluation as the baseline,
/// which never depends on an entered body.
pub(super) struct Constructors<'a> {
    pub code: &'a Code,
    pub data: &'a ReadOnlyData,
    /// Each bound constructor's compiler vtable points, by offset from its receiver.
    pub summaries: &'a BTreeMap<u64, BTreeMap<u64, u64>>,
    pub owner: u64,
    pub end: u64,
}

impl Constructors<'_> {
    /// Handle the call of the bound constructor `target` at `receiver` in a machine that tracks
    /// the owner. Enters its body when `body_available`; otherwise, or when the walk cannot be
    /// followed, treats the call as one that the machine does not run and installs the compiler
    /// summary. `None` when a summary point lies outside the owner.
    pub fn call(
        &self,
        machine: &mut Machine<'_>,
        target: u64,
        receiver: u64,
        body_available: bool,
    ) -> Option<Call> {
        let walk = body_available
            .then(|| self.initial_state(target, machine, 0))
            .flatten();
        if let Some(state) = walk {
            return Some(self.install(machine, &state));
        }

        let call = machine.opaque_call();
        install_vtables(machine, receiver, self.end, self.summaries.get(&target)?)?;

        Some(call)
    }

    /// Evaluate the constructor `target` with the call's arguments and the caller's state.
    fn initial_state(
        &self,
        target: u64,
        caller: &Machine<'_>,
        depth: usize,
    ) -> Option<InitialState> {
        let receiver = caller.register(0)?;
        if depth >= 8 || !(self.owner..self.end).contains(&receiver) {
            return None;
        }

        let mut machine = caller.fresh_callee(self.code, self.data);
        if machine.owner_range() != Some((self.owner, self.end)) {
            return None;
        }
        machine.intercept_tail_calls(self.summaries.keys().copied().collect());
        let paths = machine.run_paths(target, &mut |target, machine| {
            let nested = target.filter(|target| self.summaries.contains_key(target));
            let member = machine
                .register(0)
                .is_some_and(|nested| (receiver..self.end).contains(&nested));
            let state = nested
                .filter(|_| member)
                .and_then(|nested| self.initial_state(nested, machine, depth + 1));

            Ok(match state {
                Some(state) => self.install(machine, &state),
                None => machine.opaque_call(),
            })
        });

        let mut agreed: Option<InitialState> = None;
        for path in paths {
            if !matches!(path.end, Ok(Exit::Returned)) || path.machine.owner_stack_lost() {
                return None;
            }
            let state = InitialState::of(&path.machine, self.owner, self.end);
            agreed = Some(match agreed {
                Some(agreed) => agreed.join(state),
                None => state,
            });
        }
        let mut state = agreed?;
        self.complete_summary(target, receiver - self.owner, depth, &mut state.bytes)?;

        Some(state)
    }

    /// Check the constructor's compiler vtable points, at `base` in the owner, against the bytes
    /// it wrote. The outer receiver keeps its existing compiler vtable proof; new embedded points
    /// require constructor-written words, and a partial word is never completed from metadata.
    fn complete_summary(
        &self,
        target: u64,
        base: u64,
        depth: usize,
        bytes: &mut BTreeMap<u64, u8>,
    ) -> Option<()> {
        for (&offset, &point) in self.summaries.get(&target)? {
            let at = base.checked_add(offset)?;
            if at.checked_add(8)? > self.end - self.owner {
                return None;
            }
            let point_bytes = point.to_le_bytes();
            let disagrees = (0..8).any(|index| {
                bytes
                    .get(&(at + index))
                    .is_some_and(|byte| *byte != point_bytes[index as usize])
            });
            if disagrees {
                return None;
            }
            if depth == 0 && (0..8).all(|index| !bytes.contains_key(&(at + index))) {
                for (index, byte) in (at..).zip(point_bytes) {
                    bytes.insert(index, byte);
                }
            }
        }

        Some(())
    }

    /// Replace the caller's owner state with the constructor's, apply the constructor's other
    /// effects, and return from the call.
    fn install(&self, machine: &mut Machine<'_>, state: &InitialState) -> Call {
        machine.forget_known_bytes(self.owner, self.end - self.owner);
        for (&offset, &byte) in &state.bytes {
            machine.write(self.owner + offset, 1, byte.into());
        }
        machine.set_owner_derived_bytes(&state.tainted);
        if state.clobbers_outside {
            machine.forget_memory_outside_owner();
        }
        if state.escaped {
            machine.escape_owner();
        }

        machine.return_with_taint(
            state.returned,
            ReturnTaint {
                returned: state.returned_owner_derived,
                clobbered: true,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::assembler::{Arm64, arm64};
    use crate::engine::analysis::durations::word;

    /// An address that no section maps, so a load from it is unknown and underived.
    const GLOBAL: u64 = 0x80000;

    /// Walk the constructor at 0x1000 from `caller`, which tracks the owner at `owner..end`.
    /// Each argument register that points into the owner is owner-derived.
    fn walk(
        code: &Code,
        data: &ReadOnlyData,
        summaries: &BTreeMap<u64, BTreeMap<u64, u64>>,
        caller: &Machine<'_>,
        owner: u64,
        end: u64,
    ) -> Option<InitialState> {
        let mut caller = caller.clone();
        caller.track_owner(owner, end);
        for index in 0..9 {
            if caller
                .register(index)
                .is_some_and(|value| (owner..end).contains(&value))
            {
                caller.derive_from_owner(index);
            }
        }
        Constructors {
            code,
            data,
            summaries,
            owner,
            end,
        }
        .initial_state(0x1000, &caller, 0)
    }

    /// Walk `parent` at 0x1000, which calls the member constructor `child` at 0x2000, on a
    /// 64-byte owner.
    fn walk_parent(parent: Arm64, child: Arm64) -> InitialState {
        let parent = parent.bytes();
        let child = child.bytes();
        let code = Code::decode(&[(0x1000, &parent), (0x2000, &child)]).unwrap();
        let data = ReadOnlyData::default();
        let mut caller = Machine::new(&code, &data);
        let owner = caller.reserve(64);
        caller.set_register(0, owner);
        let summaries = BTreeMap::from([(0x1000, BTreeMap::new()), (0x2000, BTreeMap::new())]);

        walk(&code, &data, &summaries, &caller, owner, owner + 64).unwrap()
    }

    #[test]
    fn callee_frame_does_not_alias_a_caller_stack_argument() {
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
        let state = walk(&code, &data, &constructors, &caller, owner, owner + 64).unwrap();
        assert!(!state.bytes.contains_key(&32));

        caller.write(caller.stack_pointer(), 4, 7);
        let state = walk(&code, &data, &constructors, &caller, owner, owner + 64).unwrap();
        assert_eq!(word(&state.bytes, 32), Some(7));
    }

    #[test]
    fn external_stack_pointer_cannot_preserve_a_stale_caller_load() {
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
        let state = walk(&code, &data, &constructors, &caller, owner, owner + 128).unwrap();
        assert!(state.escaped);
        assert_eq!(word(&state.bytes, 32), None);
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

    #[test]
    fn a_member_summary_is_checked_and_completed_at_the_member_offset() {
        let child = arm64!(at 0x1000;
            mov w9, #9;
            str w9, [x1, #16]; // an earlier member through the owner
            ret
        );
        let code = Code::decode(&[(0x1000, child.as_slice())]).unwrap();
        let data = ReadOnlyData::default();
        let mut caller = Machine::new(&code, &data);
        let owner = caller.reserve(64);
        caller.write(owner, 8, 0x30000);
        caller.set_register(0, owner + 32);
        caller.set_register(1, owner);
        let summaries = BTreeMap::from([(0x1000, BTreeMap::from([(0, 0x35000), (8, 0x36000)]))]);
        let state = walk(&code, &data, &summaries, &caller, owner, owner + 64).unwrap();
        let point = |offset: u64| {
            (0..8).try_fold(0_u64, |point, index| {
                Some(point | u64::from(*state.bytes.get(&(offset + index))?) << (index * 8))
            })
        };
        assert_eq!(point(0), Some(0x30000));
        assert_eq!(point(32), Some(0x35000));
        assert_eq!(point(40), Some(0x36000));
        assert_eq!(word(&state.bytes, 16), Some(9));
    }

    #[test]
    fn a_member_write_to_a_caller_stack_argument_leaves_no_stale_caller_value() {
        let mut parent = Arm64::at(0x1000);
        parent.prologue();
        arm64!(parent;
            sub sp, sp, #16;
            mov w8, #7;
            str w8, [sp];
            mov x19, x0;
            add x0, x19, #32;
            mov x1, sp;
            mov w2, #9
        );
        parent.call(0x2000);
        arm64!(parent;
            ldr w8, [sp];
            str w8, [x19, #40];
            add sp, sp, #16;
            mov x0, x19
        );
        parent.epilogue();
        arm64!(parent; ret);
        let mut child = Arm64::at(0x2000);
        arm64!(child; str w2, [x1]; ret);
        let state = walk_parent(parent, child);
        assert!(state.clobbers_outside);
        assert_eq!(word(&state.bytes, 40), None);
    }

    #[test]
    fn a_member_walk_joins_the_effects_of_each_returning_path() {
        let registering = [
            arm64!(at 0x2000; cbz x10, extern 0x2008; str x0, [x2]; ret),
            arm64!(at 0x2000; cbnz x10, extern 0x2008; str x0, [x2]; ret),
        ];
        for child in registering {
            let mut parent = Arm64::at(0x1000);
            parent.prologue();
            arm64!(parent; mov x19, x0; add x0, x19, #32);
            parent.address(2, GLOBAL);
            parent.call(0x2000);
            arm64!(parent; mov w8, #5; str w8, [x19, #16]);
            parent.load(3, GLOBAL); // the owner if the member registered it there
            arm64!(parent; str wzr, [x3]; mov x0, x19);
            parent.epilogue();
            arm64!(parent; ret);
            let parent = parent.bytes();
            let code = Code::decode(&[(0x1000, &parent), (0x2000, &child)]).unwrap();
            let data = ReadOnlyData::default();
            let mut caller = Machine::new(&code, &data);
            let owner = caller.reserve(64);
            caller.set_register(0, owner);
            let summaries = BTreeMap::from([(0x1000, BTreeMap::new()), (0x2000, BTreeMap::new())]);
            let state = walk(&code, &data, &summaries, &caller, owner, owner + 64).unwrap();
            assert!(state.escaped);
            assert_eq!(word(&state.bytes, 16), None);
        }
    }

    #[test]
    fn a_member_that_returns_an_unknown_owner_address_derives_its_return() {
        let mut parent = Arm64::at(0x1000);
        parent.prologue();
        arm64!(parent; mov x19, x0; mov x1, x0; add x0, x19, #32);
        parent.address(2, GLOBAL);
        parent.call(0x2000);
        arm64!(parent;
            mov w8, #5;
            str w8, [x19, #16];
            str wzr, [x0];
            mov x0, x19
        );
        parent.epilogue();
        arm64!(parent; ret);
        let mut child = Arm64::at(0x2000);
        arm64!(child;
            ldr x10, [x2];
            add x0, x1, x10; // the owner at an unknown offset
            ret
        );
        let state = walk_parent(parent, child);
        assert_eq!(word(&state.bytes, 16), None);
    }

    #[test]
    fn an_unknown_owner_address_stored_by_a_member_stays_derived() {
        let mut parent = Arm64::at(0x1000);
        parent.prologue();
        arm64!(parent; mov x19, x0; mov x1, x0; add x0, x19, #32);
        parent.address(2, GLOBAL);
        parent.call(0x2000);
        arm64!(parent;
            mov w8, #5;
            str w8, [x19, #16];
            ldr x3, [x19, #32];
            str wzr, [x3];
            mov x0, x19
        );
        parent.epilogue();
        arm64!(parent; ret);
        let mut child = Arm64::at(0x2000);
        arm64!(child;
            ldr x10, [x2];
            add x10, x1, x10; // the owner at an unknown offset
            str x10, [x0];
            ret
        );
        let state = walk_parent(parent, child);
        assert_eq!(word(&state.bytes, 16), None);
    }
}
