use super::super::{Call, Code, Exit, Machine, Path, ReadOnlyData, STACK_TOP};
use super::trace_causes;
use crate::engine::analysis::assembler::arm64;
use crate::engine::analysis::stop::{Cause, CauseKind, Obstacle, Trace, Unknown, Unresolved};

const ENTRY: u64 = 0x100;

type Outcome = (Result<Exit, Unresolved>, [Option<u64>; 31], Vec<(u64, u64)>);

fn authored(bytes: &[u8]) -> Code {
    Code::decode(&[(ENTRY, bytes)]).unwrap()
}

fn unknown_calls(_: Option<u64>, _: &mut Machine<'_>) -> Result<Call, Unresolved> {
    Ok(Call::Return(None))
}

/// Runs `walk` on a new machine for `code` untraced, then traced, and returns the traced paths.
/// Both runs must end each path alike, with the same known registers and memory.
fn traced<'a>(
    code: &'a Code,
    data: &'a ReadOnlyData,
    walk: impl Fn(Machine<'a>) -> Vec<Path<'a>>,
) -> Vec<Path<'a>> {
    let untraced = walk(Machine::new(code, data));
    let traced = trace_causes(|| walk(Machine::new(code, data)));

    assert_eq!(outcomes(&traced), outcomes(&untraced));
    for path in &untraced {
        assert!(path.machine.traces.is_none());
        if let Err(unresolved) = &path.end {
            assert!(unresolved.trace.is_none());
        }
    }

    traced
}

fn outcomes(paths: &[Path<'_>]) -> Vec<Outcome> {
    paths
        .iter()
        .map(|path| {
            let machine = &path.machine;
            (path.end.clone(), machine.registers, machine.known_words())
        })
        .collect()
}

/// Every path from `ENTRY`, where each call returns an unknown value.
fn every_path<'a>(code: &'a Code, data: &'a ReadOnlyData) -> Vec<Path<'a>> {
    traced(code, data, |machine| {
        machine.run_paths(ENTRY, &mut unknown_calls)
    })
}

fn joining_paths<'a>(code: &'a Code, data: &'a ReadOnlyData) -> Vec<Path<'a>> {
    traced(code, data, |machine| {
        machine.run_paths_joining(ENTRY, &mut unknown_calls)
    })
}

fn stop<'p>(path: &'p Path<'_>) -> &'p Unresolved {
    path.end.as_ref().unwrap_err()
}

fn stop_trace(path: &Path<'_>) -> Trace {
    *stop(path).trace.clone().expect("a traced stop")
}

fn cause(kind: CauseKind, instruction: u64) -> Cause {
    Cause {
        kind,
        instruction,
        entry: ENTRY,
    }
}

fn causes(trace: Trace) -> Vec<Cause> {
    trace.causes().collect()
}

fn branch_value(instruction: u64, register: u8) -> Unresolved {
    let unknown = Obstacle::Unknown(Unknown::Register(register));
    Unresolved::at("branch-value", instruction, ENTRY, unknown)
}

#[test]
fn a_call_is_the_cause_of_the_registers_and_flags_that_it_clobbers() {
    let bytes = arm64!(at 0x100;
        mov x1, #1;
        bl extern 0x900; // leaves x1 unknown
        br x1
    );
    let code = authored(&bytes);
    let data = ReadOnlyData::default();
    let paths = every_path(&code, &data);

    assert_eq!(stop(&paths[0]), &branch_value(0x108, 1));
    let trace = stop_trace(&paths[0]);
    assert_eq!(causes(trace), [cause(CauseKind::Call, 0x104)]);
    assert!(!trace.unrecorded && !trace.truncated);

    let bytes = arm64!(at 0x100;
        mov x0, #1;
        cmp x0, #1;
        bl extern 0x900; // leaves the flags unknown
        b.eq extern 0x100
    );
    let code = authored(&bytes);
    let run = |mut machine: Machine<'_>| machine.run(ENTRY, &mut |_, _| Ok(Call::Return(None)));
    let untraced = run(Machine::new(&code, &data));
    let traced = trace_causes(|| run(Machine::new(&code, &data)));

    assert_eq!(traced, untraced);
    let flags = Obstacle::Unknown(Unknown::Flags);
    assert_eq!(traced, Err(Unresolved::at("flags", 0x10c, ENTRY, flags)));
    let trace = *traced.unwrap_err().trace.unwrap();
    assert_eq!(causes(trace), [cause(CauseKind::Call, 0x108)]);
}

#[test]
fn a_reload_from_overwritten_memory_names_the_store_and_not_an_earlier_loss() {
    let bytes = arm64!(at 0x100;
        bl extern 0x900; // leaves x3 and x9 unknown
        mov x9, #0x300; // x9 is known again
        str x9, [sp];
        str xzr, [x3]; // x3 is unknown, so this may overwrite the saved x9
        ldr x9, [sp];
        br x9
    );
    let code = authored(&bytes);
    let data = ReadOnlyData::default();
    let paths = every_path(&code, &data);

    assert_eq!(stop(&paths[0]), &branch_value(0x114, 9));
    let trace = stop_trace(&paths[0]);
    assert_eq!(causes(trace), [cause(CauseKind::UnknownStore, 0x10c)]);
    assert!(!trace.unrecorded);

    let protected = traced(&code, &data, |mut machine| {
        machine.protect(STACK_TOP, 8);
        machine.run_paths(ENTRY, &mut unknown_calls)
    });
    assert_eq!(protected[0].end, Ok(Exit::Returned));
}

#[test]
fn a_memory_instruction_gives_each_value_only_its_own_causes() {
    let data = ReadOnlyData::default();

    let bytes = arm64!(at 0x100;
        bl extern 0x900; // leaves x0 unknown
        ldp x19, x20, [x0];
        ret
    );
    let code = authored(&bytes);
    let paths = every_path(&code, &data);
    let from_address = [cause(CauseKind::Call, 0x100)];
    for register in [19, 20] {
        let trace = paths[0].machine.register_trace(register).unwrap();
        assert_eq!(causes(trace), from_address);
    }

    let bytes = arm64!(at 0x100;
        bl extern 0x900;
        mov x19, x0;
        bl extern 0x900;
        mov x20, x0;
        str x19, [x20], #8; // the written-back base depends on x20 alone
        ret
    );
    let code = authored(&bytes);
    let paths = every_path(&code, &data);
    let base = paths[0].machine.register_trace(20).unwrap();
    assert_eq!(causes(base), [cause(CauseKind::Call, 0x108)]);

    let bytes = arm64!(at 0x100;
        str xzr, [sp];
        bl extern 0x900;
        strb w2, [sp]; // byte 0 is lost to the first call
        bl extern 0x900;
        strb w3, [sp, #1]; // byte 1 is lost to the second call
        ldrh w4, [sp];
        ldurh w5, [sp, #-1]; // a byte that no instruction wrote, then byte 0
        ret
    );
    let code = authored(&bytes);
    let paths = every_path(&code, &data);
    let both = paths[0].machine.register_trace(4).unwrap();
    assert_eq!(
        causes(both),
        [cause(CauseKind::Call, 0x104), cause(CauseKind::Call, 0x10c)]
    );
    assert!(!both.unrecorded);
    let partly_unrecorded = paths[0].machine.register_trace(5).unwrap();
    assert_eq!(causes(partly_unrecorded), [cause(CauseKind::Call, 0x104)]);
    assert!(partly_unrecorded.unrecorded);
}

#[test]
fn each_path_keeps_the_causes_of_its_own_instructions() {
    let bytes = arm64!(at 0x100;
        cbz x3, extern 0x110; // x3 is unknown, so both sides run
        bl extern 0x900;
        br x1; // x1 is lost to the call
        nop;
        str xzr, [sp];
        str xzr, [x4]; // x4 is unknown
        ldr x1, [sp];
        br x1 // x1 is lost to the store
    );
    let code = authored(&bytes);
    let data = ReadOnlyData::default();
    let mut stops: Vec<_> = every_path(&code, &data)
        .iter()
        .map(|path| (stop(path).clone(), causes(stop_trace(path))))
        .collect();
    stops.sort();

    assert_eq!(
        stops,
        [
            (branch_value(0x108, 1), vec![cause(CauseKind::Call, 0x104)]),
            (
                branch_value(0x11c, 1),
                vec![cause(CauseKind::UnknownStore, 0x114)]
            ),
        ]
    );
}

#[test]
fn a_split_on_unknown_flags_gives_each_side_the_cause_of_the_value_it_selects() {
    let bytes = arm64!(at 0x100;
        bl extern 0x900; // leaves the flags and x1 unknown
        mov x2, #7;
        csel x0, x1, x2, eq; // runs again on each side of the split
        ret
    );
    let code = authored(&bytes);
    let data = ReadOnlyData::default();
    let mut selected: Vec<_> = every_path(&code, &data)
        .iter()
        .map(|path| (path.machine.register(0), path.machine.register_trace(0)))
        .map(|(value, trace)| (value, trace.map(causes)))
        .collect();
    selected.sort();

    assert_eq!(
        selected,
        [
            (None, Some(vec![cause(CauseKind::Call, 0x100)])),
            (Some(7), None),
        ]
    );
}

#[test]
fn a_value_lost_where_paths_join_names_the_join_and_every_earlier_cause() {
    let bytes = arm64!(at 0x100;
        mov x1, #0;
        cbz x4, extern 0x10c; // the loop head
        bl extern 0x900; // loses x1 on some passes
        cbnz x3, extern 0x104;
        br x1
    );
    let code = authored(&bytes);
    let data = ReadOnlyData::default();
    let traces: Vec<_> = joining_paths(&code, &data)
        .iter()
        .filter(|path| {
            path.end
                .as_ref()
                .is_err_and(|stop| stop.reason == "branch-value")
        })
        .map(|path| causes(stop_trace(path)))
        .collect();

    let call = cause(CauseKind::Call, 0x108);
    let join = cause(CauseKind::Join, 0x104);
    assert!(
        traces.contains(&vec![call]),
        "lost after the path left the head"
    );
    assert!(
        traces.contains(&vec![join, call]),
        "lost where the paths joined"
    );
    for trace in traces.iter().filter(|trace| trace.contains(&join)) {
        assert!(trace.len() > 1);
    }
}

#[test]
fn an_arrival_that_the_joined_facts_cover_keeps_its_causes_for_a_later_widening() {
    let bytes = arm64!(at 0x100;
        mov x19, #0;
        str xzr, [sp];
        str xzr, [x7]; // x7 is unknown: the first cause
        nop; // the loop head
        cbz x5, extern 0x124; // the covered pass runs first
        add x19, x19, #1; // widens the joined facts
        cbnz x6, extern 0x10c;
        ldr x1, [sp];
        br x1;
        str xzr, [x8]; // x8 is unknown: the second cause
        b extern 0x10c
    );
    let code = authored(&bytes);
    let data = ReadOnlyData::default();
    let traces: Vec<_> = joining_paths(&code, &data)
        .iter()
        .filter(|path| {
            path.end
                .as_ref()
                .is_err_and(|stop| stop.reason == "branch-value")
        })
        .map(|path| causes(stop_trace(path)))
        .collect();

    let join = cause(CauseKind::Join, 0x10c);
    let covered = cause(CauseKind::UnknownStore, 0x124);
    assert!(!traces.is_empty());
    assert!(traces.iter().all(|trace| trace.contains(&join)));
    assert!(traces.iter().any(|trace| trace.contains(&covered)));
}

#[test]
fn flags_and_memory_that_the_joined_paths_disagree_on_name_the_join() {
    let bytes = arm64!(at 0x100;
        mov x1, #0;
        cmp x1, #0;
        str x1, [sp];
        nop; // the loop head
        add x1, x1, #1;
        cmp x1, #0; // other flags than on the first arrival
        str x1, [sp]; // another value than on the first arrival
        cbnz x3, extern 0x10c;
        ret
    );
    let code = authored(&bytes);
    let data = ReadOnlyData::default();
    let paths = joining_paths(&code, &data);
    let join = cause(CauseKind::Join, 0x10c);

    let unknown_flags: Vec<_> = paths
        .iter()
        .filter(|path| path.machine.flags.is_none())
        .map(|path| causes(path.machine.traces.as_ref().unwrap().flags))
        .collect();
    assert!(!unknown_flags.is_empty());
    assert!(unknown_flags.iter().all(|trace| trace.contains(&join)));

    let unknown_memory: Vec<_> = paths
        .iter()
        .filter_map(|path| path.machine.memory_trace(STACK_TOP, 8))
        .map(causes)
        .collect();
    assert!(!unknown_memory.is_empty());
    assert!(unknown_memory.iter().all(|trace| trace.contains(&join)));
}

#[test]
fn a_trace_keeps_a_bounded_number_of_causes_and_says_what_it_dropped() {
    let bytes = arm64!(at 0x100;
        bl extern 0x900;
        mov x19, x0;
        bl extern 0x900;
        mov x20, x0;
        bl extern 0x900;
        mov x21, x0;
        bl extern 0x900;
        mov x22, x0;
        bl extern 0x900;
        mov x23, x0;
        add x9, x19, x20;
        add x9, x9, x21;
        add x9, x9, x22;
        add x9, x9, x23;
        add x9, x9, x24; // x24 was never known
        br x9
    );
    let code = authored(&bytes);
    let data = ReadOnlyData::default();
    let paths = every_path(&code, &data);
    let trace = stop_trace(&paths[0]);

    assert_eq!(
        causes(trace),
        [0x100, 0x108, 0x110, 0x118].map(|call| cause(CauseKind::Call, call))
    );
    assert!(trace.truncated);
    assert!(trace.unrecorded);
}
