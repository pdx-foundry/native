//! Effects shared by bounded constructor-summary consumers, and the rules of the runs that enter
//! constructor bodies.
use super::evaluate::{Call, Code, Exit, Machine, ReadOnlyData, ReturnTaint};
use super::stop::{CauseKind, Trace, Unresolved};
use std::collections::{BTreeMap, BTreeSet};

/// The image that a constructor route reads: its read-only sections with the targets of its
/// constant pointer slots, and the image targets of its writable slots.
pub(crate) struct ConstructorImage {
    constant: ReadOnlyData,
    writable: BTreeMap<u64, u64>,
}

impl ConstructorImage {
    /// The image of `sections` with the rebased `pointers`, of which `writable_slots` lie outside
    /// the constant sections.
    pub fn new(
        sections: &ReadOnlyData,
        pointers: &BTreeMap<u64, u64>,
        writable_slots: &BTreeSet<u64>,
    ) -> Self {
        let (writable, constant) = pointers
            .iter()
            .map(|(&slot, &target)| (slot, target))
            .partition(|(slot, _)| writable_slots.contains(slot));

        Self {
            constant: sections.with_words(&constant),
            writable,
        }
    }

    /// A machine for a run that enters constructor bodies. Such a run relies on stores being
    /// disjoint from the owner, so it does not take a writable slot's image target as its
    /// value: earlier code may have replaced it, and the slot reads as unknown.
    pub fn entered<'a>(&'a self, code: &'a Code) -> Machine<'a> {
        Machine::new(code, &self.constant)
    }

    /// A machine for a summary-only registry run. It assumes that each writable slot holds its
    /// image target, in memory that a store to an unknown address may change.
    pub fn baseline<'a>(&'a self, code: &'a Code) -> Machine<'a> {
        let mut machine = Machine::new(code, &self.constant);
        for (&slot, &target) in &self.writable {
            machine.write(slot, 8, target);
        }

        machine
    }
}

/// Accept a returned path of a run that enters constructor bodies. A path whose stack pointer
/// moved by an unknown amount adds no facts, since no store is known to stay in its private frame.
pub(super) fn accept_entered_path(machine: &Machine<'_>) -> Result<(), Unresolved> {
    if machine.owner_stack_lost() {
        return Err(Unresolved::new("constructor-stack"));
    }

    Ok(())
}

/// Replace the receiver's remaining span with only the constructor's proven vtable points.
/// The caller establishes ownership and the allocation end; summaries do not retain embedded state.
/// While tracing causes, the forgotten span has the cause `kind`.
pub(super) fn install_vtables(
    machine: &mut Machine<'_>,
    receiver: u64,
    end: u64,
    points: &BTreeMap<u64, u64>,
    kind: CauseKind,
) -> Option<()> {
    let span = end.checked_sub(receiver)?;
    for &offset in points.keys() {
        if offset.checked_add(8)? > span {
            return None;
        }
    }
    machine.forget_for(receiver, span, kind);
    for (&offset, &point) in points {
        machine.write(receiver + offset, 8, point);
    }
    Some(())
}

/// Join the traces of the bytes that two paths do not agree on. Each side gives the bytes it
/// knows and the traces of those it does not, by offset. A byte that neither knows merges both
/// traces; a byte that one side knew, or that both knew differently, has `disagreement` first.
/// Empty when untraced, as `disagreement` is then `None`.
pub(super) fn join_byte_traces(
    (bytes, traces): (&BTreeMap<u64, u8>, &BTreeMap<u64, Trace>),
    (other_bytes, other_traces): (&BTreeMap<u64, u8>, &BTreeMap<u64, Trace>),
    disagreement: Option<Trace>,
) -> BTreeMap<u64, Trace> {
    let Some(disagreement) = disagreement else {
        return BTreeMap::new();
    };
    let offsets: BTreeSet<u64> = bytes
        .keys()
        .chain(other_bytes.keys())
        .chain(traces.keys())
        .chain(other_traces.keys())
        .copied()
        .collect();
    let agreed = |offset: &u64| match (bytes.get(offset), other_bytes.get(offset)) {
        (Some(byte), Some(other)) => byte == other,
        _ => false,
    };

    offsets
        .into_iter()
        .filter(|offset| !agreed(offset))
        .map(|offset| {
            let unknown = [traces.get(&offset), other_traces.get(&offset)];
            let mut trace = match unknown {
                [Some(_), Some(_)] => Trace::default(),
                _ => disagreement,
            };
            for side in unknown.into_iter().flatten() {
                trace.merge(side);
            }
            (offset, trace)
        })
        .collect()
}

/// The owner state that an entered constructor leaves on every path that returns.
struct InitialState {
    /// Bytes of the whole owner, relative to its start, that every returning path agrees on.
    bytes: BTreeMap<u64, u8>,
    /// Why each other owner byte is unknown, by offset, while tracing causes.
    traces: BTreeMap<u64, Trace>,
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
            traces: machine.unknown_byte_traces(owner, end - owner),
            tainted: machine.owner_derived_bytes(),
            returned: machine.register(0),
            returned_owner_derived: machine.owner_derived(0),
            escaped: machine.owner_escaped(),
            clobbers_outside: machine.clobbers_outside_owner(),
        }
    }

    /// The state that both this path and `other` leave: agreed bytes and every possible effect.
    /// A byte that the paths disagree on has the cause `disagreement` while tracing.
    fn join(mut self, other: Self, disagreement: Option<Trace>) -> Self {
        self.traces = join_byte_traces(
            (&self.bytes, &self.traces),
            (&other.bytes, &other.traces),
            disagreement,
        );
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
    pub image: &'a ConstructorImage,
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

        let kind = if body_available {
            CauseKind::Unfollowed
        } else {
            CauseKind::Invalidated
        };
        let call = machine.opaque_call_for(kind);
        install_vtables(
            machine,
            receiver,
            self.end,
            self.summaries.get(&target)?,
            kind,
        )?;

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

        let mut machine = caller.fresh_callee(self.code, &self.image.constant);
        if machine.owner_range() != Some((self.owner, self.end)) {
            return None;
        }
        machine.intercept_tail_calls(self.summaries.keys().copied().collect());
        let paths = machine.run_paths(target, &mut |target, machine| {
            let nested = target.filter(|target| self.summaries.contains_key(target));
            let member = machine
                .register(0)
                .is_some_and(|nested| (receiver..self.end).contains(&nested));
            let Some(nested) = nested.filter(|_| member) else {
                return Ok(machine.opaque_call());
            };

            Ok(match self.initial_state(nested, machine, depth + 1) {
                Some(state) => self.install(machine, &state),
                None => machine.opaque_call_for(CauseKind::Unfollowed),
            })
        });

        let mut agreed: Option<InitialState> = None;
        for path in paths {
            if !matches!(path.end, Ok(Exit::Returned)) {
                return None;
            }
            accept_entered_path(&path.machine).ok()?;
            let state = InitialState::of(&path.machine, self.owner, self.end);
            agreed = Some(match agreed {
                Some(agreed) => agreed.join(state, path.machine.join_cause()),
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

    /// Replace the caller's owner state with the constructor's, with the causes of its unknown
    /// bytes, apply the constructor's other effects, and return from the call.
    fn install(&self, machine: &mut Machine<'_>, state: &InitialState) -> Call {
        machine.forget_known_bytes(self.owner, self.end - self.owner);
        for (&offset, &byte) in &state.bytes {
            machine.write(self.owner + offset, 1, byte.into());
        }
        machine.set_byte_traces(self.owner, &state.traces);
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
    use crate::engine::analysis::evaluate::trace_causes;

    /// An address that no section maps, so a load from it is unknown and underived.
    const GLOBAL: u64 = 0x80000;

    /// A function that is not a bound constructor, so a call to it is not followed.
    const UNBOUND: u64 = 0x9000;

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
        caller.track_private_owner(owner, end);
        for index in 0..9 {
            if caller
                .register(index)
                .is_some_and(|value| (owner..end).contains(&value))
            {
                caller.derive_from_owner(index);
            }
        }
        let image = ConstructorImage {
            constant: data.clone(),
            writable: BTreeMap::new(),
        };
        Constructors {
            code,
            image: &image,
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
                &BTreeMap::from([(0, 4), (32, 5)]),
                CauseKind::Invalidated
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
                    &BTreeMap::from([(offset, 6)]),
                    CauseKind::Invalidated
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

    /// Walk the constructor at 0x1000 on a 64-byte owner in x0, with `bodies` decoded and each
    /// of `constructors` bound, untraced and then traced. Both walks must leave the same state,
    /// and only the traced one has causes.
    fn traced_walk(bodies: Vec<Arm64>, constructors: &[u64]) -> InitialState {
        let starts: Vec<u64> = bodies.iter().map(Arm64::start).collect();
        let bytes: Vec<Vec<u8>> = bodies.into_iter().map(Arm64::bytes).collect();
        let segments: Vec<(u64, &[u8])> = starts
            .iter()
            .copied()
            .zip(bytes.iter().map(Vec::as_slice))
            .collect();
        let code = Code::decode(&segments).unwrap();
        let data = ReadOnlyData::default();
        let summaries = constructors
            .iter()
            .map(|&constructor| (constructor, BTreeMap::new()))
            .collect();
        let run = || {
            let mut caller = Machine::new(&code, &data);
            let owner = caller.reserve(64);
            caller.set_register(0, owner);
            walk(&code, &data, &summaries, &caller, owner, owner + 64).unwrap()
        };

        let untraced = run();
        let traced = trace_causes(run);
        assert_eq!(traced.bytes, untraced.bytes);
        assert_eq!(traced.tainted, untraced.tainted);
        assert_eq!(traced.returned, untraced.returned);
        assert_eq!(traced.escaped, untraced.escaped);
        assert_eq!(traced.clobbers_outside, untraced.clobbers_outside);
        assert!(untraced.traces.is_empty());

        traced
    }

    /// The constructor at 0x1000: it stores 7 at owner +16, with x1 the owner and x2 the address
    /// `GLOBAL`, then runs `body` and returns the owner.
    fn parent_constructor(body: impl FnOnce(&mut Arm64)) -> Arm64 {
        let mut parent = Arm64::at(0x1000);
        parent.prologue();
        arm64!(parent; mov x19, x0; mov x1, x0; mov w8, #7; str w8, [x19, #16]);
        parent.address(2, GLOBAL);
        body(&mut parent);
        arm64!(parent; mov x0, x19);
        parent.epilogue();
        arm64!(parent; ret);
        parent
    }

    /// Call the member constructor `target` for owner +32, and return the call's address.
    fn call_member(body: &mut Arm64, target: u64) -> u64 {
        arm64!(body; add x0, x19, #32);
        let call = body.here();
        body.call(target);
        call
    }

    /// Call `UNBOUND`, and return the call's address.
    fn call_unbound(body: &mut Arm64) -> u64 {
        let call = body.here();
        body.call(UNBOUND);
        call
    }

    /// A function at `start` that runs `body` in a frame and returns.
    fn member(start: u64, body: impl FnOnce(&mut Arm64)) -> Arm64 {
        let mut member = Arm64::at(start);
        member.prologue();
        body(&mut member);
        member.epilogue();
        arm64!(member; ret);
        member
    }

    /// The member at 0x2000 runs `first` when the word at `GLOBAL` is zero, or `second` at
    /// 0x2100, or the other way round when `swapped`.
    fn branching_member(
        swapped: bool,
        first: impl FnOnce(&mut Arm64),
        second: impl FnOnce(&mut Arm64),
    ) -> [Arm64; 2] {
        let mut member = Arm64::at(0x2000);
        member.prologue();
        arm64!(member; ldr x10, [x2]);
        if swapped {
            arm64!(member; cbz x10, extern 0x2100);
        } else {
            arm64!(member; cbnz x10, extern 0x2100);
        }
        first(&mut member);
        member.epilogue();
        arm64!(member; ret);
        let mut other = Arm64::at(0x2100);
        second(&mut other);
        other.epilogue();
        arm64!(other; ret);
        [member, other]
    }

    /// Each recorded cause of owner byte `offset`: its kind, instruction and entry.
    fn losses(state: &InitialState, offset: u64) -> Vec<(CauseKind, u64, u64)> {
        state.traces[&offset]
            .causes()
            .map(|cause| (cause.kind, cause.instruction, cause.entry))
            .collect()
    }

    #[test]
    fn a_byte_that_only_one_side_records_joins_in_either_order() {
        let join = Trace::of(crate::engine::analysis::stop::Cause {
            kind: CauseKind::Join,
            instruction: 0x2010,
            entry: 0x2000,
        });
        let known = BTreeMap::from([(40, 7)]);
        let none = BTreeMap::new();
        let untraced = BTreeMap::new();

        for (first, second) in [(&known, &none), (&none, &known)] {
            let traces = join_byte_traces((first, &untraced), (second, &untraced), Some(join));
            assert_eq!(traces, BTreeMap::from([(40, join)]));
        }
    }

    #[test]
    fn a_nested_opaque_call_is_the_cause_of_a_forgotten_owner_byte() {
        let mut forgotten = 0;
        let grandchild = member(0x3000, |body| forgotten = call_unbound(body));
        let child = member(0x2000, |body| {
            arm64!(body; add x0, x0, #8);
            body.call(0x3000);
        });
        let parent = parent_constructor(|body| {
            call_member(body, 0x2000);
        });

        let state = traced_walk(vec![parent, child, grandchild], &[0x1000, 0x2000, 0x3000]);

        assert_eq!(word(&state.bytes, 16), None);
        assert_eq!(
            losses(&state, 16),
            [(CauseKind::Invalidated, forgotten, 0x3000)]
        );
    }

    #[test]
    fn a_tainted_unknown_address_store_is_the_cause() {
        let mut store = 0;
        let child = member(0x2000, |body| {
            arm64!(body; ldr x10, [x2]; add x10, x1, x10); // the owner at an unknown offset
            store = body.here();
            arm64!(body; str wzr, [x10]);
        });
        let parent = parent_constructor(|body| {
            call_member(body, 0x2000);
        });

        let state = traced_walk(vec![parent, child], &[0x1000, 0x2000]);

        assert_eq!(
            losses(&state, 16),
            [(CauseKind::UnknownStore, store, 0x2000)]
        );
    }

    #[test]
    fn a_later_definite_store_leaves_no_cause() {
        let child = member(0x2000, |body| {
            call_unbound(body);
        });
        let parent = parent_constructor(|body| {
            call_member(body, 0x2000);
            arm64!(body; mov w8, #5; str w8, [x19, #16]);
        });

        let state = traced_walk(vec![parent, child], &[0x1000, 0x2000]);

        assert_eq!(word(&state.bytes, 16), Some(5));
        assert!(!state.traces.contains_key(&16));
    }

    #[test]
    fn a_byte_that_a_member_does_not_touch_keeps_the_callers_cause() {
        let mut forgotten = 0;
        let child = member(0x2000, |_| {});
        let parent = parent_constructor(|body| {
            forgotten = call_unbound(body);
            call_member(body, 0x2000);
        });

        let state = traced_walk(vec![parent, child], &[0x1000, 0x2000]);

        assert_eq!(
            losses(&state, 16),
            [(CauseKind::Invalidated, forgotten, 0x1000)]
        );
    }

    #[test]
    fn a_member_that_establishes_a_byte_again_replaces_the_callers_cause() {
        let mut forgotten = 0;
        let child = member(0x2000, |body| {
            arm64!(body; mov w8, #5; str w8, [x1, #16]);
            forgotten = call_unbound(body);
        });
        let parent = parent_constructor(|body| {
            call_unbound(body);
            arm64!(body; mov x1, x19);
            call_member(body, 0x2000);
        });

        let state = traced_walk(vec![parent, child], &[0x1000, 0x2000]);

        assert_eq!(
            losses(&state, 16),
            [(CauseKind::Invalidated, forgotten, 0x2000)]
        );
    }

    #[test]
    fn each_loss_in_a_member_is_kept_in_order() {
        let (mut first, mut second) = (0, 0);
        let child = member(0x2000, |body| {
            first = call_unbound(body);
            second = call_unbound(body);
        });
        let parent = parent_constructor(|body| {
            call_member(body, 0x2000);
        });

        let state = traced_walk(vec![parent, child], &[0x1000, 0x2000]);

        assert_eq!(
            losses(&state, 16),
            [
                (CauseKind::Invalidated, first, 0x2000),
                (CauseKind::Invalidated, second, 0x2000)
            ]
        );
    }

    #[test]
    fn a_callee_store_adds_to_the_causes_that_it_inherited() {
        let mut inherited = 0;
        let mut overwritten = 0;
        let child = member(0x2000, |body| {
            arm64!(body; ldr x10, [x2]; add x10, x1, x10); // the owner at an unknown offset
            overwritten = body.here();
            arm64!(body; str wzr, [x10]);
        });
        let parent = parent_constructor(|body| {
            arm64!(body; ldr x10, [x2]; add x10, x19, x10); // the owner at an unknown offset
            inherited = body.here();
            arm64!(body; str wzr, [x10]);
            call_member(body, 0x2000);
        });

        let state = traced_walk(vec![parent, child], &[0x1000, 0x2000]);

        assert_eq!(
            losses(&state, 16),
            [
                (CauseKind::UnknownStore, inherited, 0x1000),
                (CauseKind::UnknownStore, overwritten, 0x2000)
            ]
        );

        let child = member(0x2000, |body| {
            arm64!(body; ldr x10, [x2]; str wzr, [x10]); // a pointer from before the owner
        });
        let parent = parent_constructor(|body| {
            arm64!(body; ldr x10, [x2]; add x10, x19, x10); // the owner at an unknown offset
            inherited = body.here();
            arm64!(body; str wzr, [x10]);
            call_member(body, 0x2000);
        });

        let state = traced_walk(vec![parent, child], &[0x1000, 0x2000]);

        assert_eq!(
            losses(&state, 16),
            [(CauseKind::UnknownStore, inherited, 0x1000)]
        );
    }

    #[test]
    fn returning_paths_that_lost_a_byte_differently_keep_both_causes() {
        for swapped in [false, true] {
            let (mut first, mut second) = (0, 0);
            let [child, other] = branching_member(
                swapped,
                |body| first = call_unbound(body),
                |body| second = call_unbound(body),
            );
            let parent = parent_constructor(|body| {
                call_member(body, 0x2000);
            });

            let state = traced_walk(vec![parent, child, other], &[0x1000, 0x2000]);

            let mut causes = losses(&state, 16);
            causes.sort();
            assert_eq!(
                causes,
                [
                    (CauseKind::Invalidated, first, 0x2000),
                    (CauseKind::Invalidated, second, 0x2000)
                ]
            );
        }
    }

    #[test]
    fn returning_paths_that_disagree_on_a_byte_join_first() {
        for swapped in [false, true] {
            let mut forgotten = 0;
            let [child, other] =
                branching_member(swapped, |_| {}, |body| forgotten = call_unbound(body));
            let parent = parent_constructor(|body| {
                call_member(body, 0x2000);
            });

            let state = traced_walk(vec![parent, child, other], &[0x1000, 0x2000]);

            let causes = losses(&state, 16);
            assert_eq!(causes.len(), 2);
            assert_eq!((causes[0].0, causes[0].2), (CauseKind::Join, 0x2000));
            assert_eq!(causes[1], (CauseKind::Invalidated, forgotten, 0x2000));
            assert!(!state.traces[&16].unrecorded);

            let [child, other] = branching_member(
                swapped,
                |body| arm64!(body; mov w8, #5; str w8, [x1, #16]),
                |body| arm64!(body; mov w8, #6; str w8, [x1, #16]),
            );
            let parent = parent_constructor(|body| {
                call_member(body, 0x2000);
            });

            let state = traced_walk(vec![parent, child, other], &[0x1000, 0x2000]);

            let causes = losses(&state, 16);
            assert_eq!(causes.len(), 1);
            assert_eq!((causes[0].0, causes[0].2), (CauseKind::Join, 0x2000));
        }
    }

    #[test]
    fn a_member_walk_that_cannot_be_followed_is_the_cause() {
        let mut child = Arm64::at(0x2000);
        arm64!(child; ldr x10, [x2]; cbz x10, extern 0x2100; ret);
        let mut other = Arm64::at(0x2100);
        arm64!(other; b extern 0x2100); // a path that does not return
        let mut call = 0;
        let parent = parent_constructor(|body| call = call_member(body, 0x2000));

        let state = traced_walk(vec![parent, child, other], &[0x1000, 0x2000]);

        assert_eq!(word(&state.bytes, 16), None);
        assert_eq!(losses(&state, 16), [(CauseKind::Unfollowed, call, 0x1000)]);
    }

    #[test]
    fn a_constructor_call_that_is_not_followed_records_one_cause() {
        let mut caller = Arm64::at(0x1000);
        caller.prologue();
        let call = caller.here();
        caller.call(0x2000);
        caller.epilogue();
        arm64!(caller; ret);
        let caller = caller.bytes();
        let looping = arm64!(at 0x2000; b extern 0x2000);
        let code = Code::decode(&[(0x1000, &caller), (0x2000, &looping)]).unwrap();
        let data = ReadOnlyData::default();
        let image = ConstructorImage {
            constant: data.clone(),
            writable: BTreeMap::new(),
        };
        let summaries = BTreeMap::from([(0x2000, BTreeMap::new())]);

        for (body_available, kind) in [
            (true, CauseKind::Unfollowed),
            (false, CauseKind::Invalidated),
        ] {
            let paths = trace_causes(|| {
                let mut machine = Machine::new(&code, &data);
                let owner = machine.reserve(64);
                machine.write(owner + 16, 4, 7);
                machine.set_register(0, owner);
                machine.track_private_owner(owner, owner + 64);
                machine.derive_from_owner(0);
                let constructors = Constructors {
                    code: &code,
                    image: &image,
                    summaries: &summaries,
                    owner,
                    end: owner + 64,
                };
                let paths = machine.run_paths(0x1000, &mut |target, machine| {
                    Ok(constructors
                        .call(machine, target.unwrap(), owner, body_available)
                        .unwrap())
                });
                paths
                    .into_iter()
                    .map(|path| path.machine.memory_trace(owner + 16, 1))
                    .collect::<Vec<_>>()
            });

            let [Some(trace)] = paths.as_slice() else {
                panic!("one traced path");
            };
            let causes: Vec<_> = trace
                .causes()
                .map(|cause| (cause.kind, cause.instruction, cause.entry))
                .collect();
            assert_eq!(causes, [(kind, call, 0x1000)]);
        }
    }
}
