//! The registry field method on small authored inputs.
use crate::engine::analysis::{
    assembler::{Arm64, arm64},
    discovery::{Symbol, candidates},
    fields::{self, FieldGap, FieldGapKind, FieldInput, Function, PathOutcome, ReaderJoin},
    stop::{Bound, Obstacle, Unknown, Unresolved},
};
use std::collections::BTreeMap;

fn fixture() -> FieldInput {
    let root = "CExample::ReadMember(CReader&, int)";
    let symbols=vec![
        Symbol{name:"TSingleObjectGameDatabase<CExampleDatabase, CExample, false>::LoadFile(char const*, bool)".into(),address:0x7000},
        Symbol{name:root.into(),address:0x1000},Symbol{name:"GetTokenArray()".into(),address:0x2000},
        Symbol{name:"CToken::CToken(int, char const*)".into(),address:0x3000},Symbol{name:"CReader::Read(int&)".into(),address:0x4000},
        Symbol{name:"CPersistent::ReadMember(CReader&, int)".into(),address:0x5000},Symbol{name:"CReader::ReportUnexpected()".into(),address:0x6000},
    ];
    FieldInput {
        objects: vec![],
        selection: candidates(&symbols).remove(0),
        symbols,
        strings: BTreeMap::from([(0x8000, "new_engine_field".into())]),
        read_only_data: vec![],
        gaps: vec![],
        functions: vec![
            Function {
                name: root.into(),
                address: 0x1000,
                code: arm64!(at 0x1000;
                    cmp w2, #7; // token 7
                    b.eq extern 0x1010;
                    add x0, x0, #0x38;
                    b extern 0x5000; // CPersistent::ReadMember
                    add x8, x0, #0x40; // the member
                    mov x0, x1; // the reader
                    mov x1, x8;
                    b extern 0x4000 // CReader::Read
                ),
            },
            Function {
                name: "GetTokenArray()".into(),
                address: 0x2000,
                code: arm64!(at 0x2000;
                    mov w1, #7; // token 7
                    adrp x2, extern 0x8000; // "new_engine_field"
                    add x2, x2, #0;
                    bl extern 0x3000; // CToken::CToken
                    ret
                ),
            },
            Function {
                name: "CPersistent::ReadMember(CReader&, int)".into(),
                address: 0x5000,
                code: arm64!(at 0x5000;
                    mov x0, x1;
                    b extern 0x6000 // CReader::ReportUnexpected
                ),
            },
        ],
    }
}
fn derive(input: FieldInput) -> fields::RegistryFieldResult {
    fields::analyze(&input).unwrap()
}
/// Replaces the instruction at `address` in the function that holds it.
fn replace(input: &mut FieldInput, address: u64, instruction: Vec<u8>) {
    let function = input
        .functions
        .iter_mut()
        .find(|function| {
            (function.address..function.address + function.code.len() as u64).contains(&address)
        })
        .expect("an authored function holds the address");
    let offset = (address - function.address) as usize;

    function.code.splice(offset..offset + 4, instruction);
}
#[test]
fn discovers_unknown_name_and_excludes_pivot_and_rejection_tokens() {
    let result = derive(fixture());
    assert_eq!(result.fields.len(), 1, "{:?}", result.gaps);
    assert_eq!(result.fields[0].name, "new_engine_field");
    assert_eq!(result.fields[0].token, 7);
    assert_eq!(result.paths.len(), 3);
    assert!(matches!(
        result.fields[0].readers[0],
        ReaderJoin::Joined { .. }
    ));
    assert_eq!(
        result
            .paths
            .iter()
            .filter(|p| matches!(p.outcome, PathOutcome::Rejected))
            .count(),
        2
    );
    assert!(result.partition_accounted);
}
#[test]
fn unsupported_instruction_preserves_obligation_and_does_not_invent_fields() {
    let mut input = fixture();
    replace(&mut input, 0x1000, arm64!(at 0x1000; ret));
    let result = derive(input);
    assert!(result.fields.is_empty());
    assert!(result.partition_accounted);
    assert!(matches!(result.paths[0].outcome, PathOutcome::Gap(_)));
}
#[test]
fn clobbered_and_truncated_receivers_cannot_join() {
    for receiver in [
        arm64!(at 0x1014; mov x0, xzr),
        arm64!(at 0x1014; mov w0, w1),
        arm64!(at 0x1014; ldr x0, [x1]),
    ] {
        let mut input = fixture();
        replace(&mut input, 0x1014, receiver);
        let result = derive(input);
        assert_eq!(result.fields.len(), 1);
        assert!(matches!(
            result.fields[0].readers[0],
            ReaderJoin::Missing(_)
        ));
    }
}
#[test]
fn unresolved_callee_and_missing_token_name_remain_gaps() {
    let mut input = fixture();
    input.symbols.retain(|s| s.address != 0x4000);
    let result = derive(input);
    assert!(matches!(
        result.fields[0].readers[0],
        ReaderJoin::Missing(_)
    ));
    let mut input = fixture();
    input.strings.clear();
    let result = derive(input);
    assert!(result.fields.is_empty());
    assert!(
        result
            .gaps
            .iter()
            .any(|g| g.kind == FieldGapKind::TokenTable)
    );
}
#[test]
fn clobbered_token_constructor_arguments_do_not_reuse_stale_values() {
    let mut input = fixture();
    replace(&mut input, 0x2008, arm64!(at 0x2008; mov x2, xzr));
    let result = derive(input);
    assert!(result.fields.is_empty());
    assert!(
        result
            .gaps
            .iter()
            .any(|g| g.kind == FieldGapKind::TokenTable)
    );
}
#[test]
fn unsupported_rejection_body_is_not_a_successful_negative_result() {
    let mut input = fixture();
    replace(&mut input, 0x5000, arm64!(at 0x5000; nop));
    let result = derive(input);
    assert!(
        !result
            .paths
            .iter()
            .any(|p| matches!(p.outcome, PathOutcome::Rejected))
    );
    assert!(
        result
            .gaps
            .iter()
            .any(|g| g.kind == FieldGapKind::ReaderJoin)
    );
}
#[test]
fn altered_flags_and_external_branch_do_not_silently_drop_token_intervals() {
    for (address, instruction) in [
        (0x1000, arm64!(at 0x1000; cmp w1, #7)),
        (0x1004, arm64!(at 0x1004; b.eq extern 0x1024)), // past the function's end
    ] {
        let mut input = fixture();
        replace(&mut input, address, instruction);
        let result = derive(input);
        assert!(result.fields.is_empty());
        assert!(result.partition_accounted);
        assert!(
            result
                .paths
                .iter()
                .any(|p| matches!(p.outcome, PathOutcome::Gap(_)))
        );
    }
}
#[test]
fn completeness_is_never_an_input_and_a_foreign_selection_is_refused() {
    let mut input = serde_json::to_value(fixture()).unwrap();
    input["complete_registry"] = true.into();
    assert!(serde_json::from_value::<FieldInput>(input).is_err());
    let mut input = fixture();
    input.selection.owner_candidate = "COther".into();
    assert!(fields::analyze(&input).is_err());
}

#[test]
fn conflicting_names_for_one_token_never_select_an_arbitrary_name() {
    let mut input = fixture();
    input.strings.insert(0x8010, "conflicting_field".into());
    input.functions[1].code = arm64!(at 0x2000;
        mov w1, #7;
        adrp x2, extern 0x8000;
        add x2, x2, #0; // "new_engine_field"
        bl extern 0x3000;
        mov w1, #7;
        adrp x2, extern 0x8000;
        add x2, x2, #0x10; // "conflicting_field"
        bl extern 0x3000;
        ret
    );
    let result = derive(input);
    assert!(result.fields.is_empty());
    assert!(
        result
            .gaps
            .iter()
            .any(|g| g.reason.contains("conflicting names"))
    );
}

#[test]
fn writeback_cannot_preserve_a_stale_token_name_pointer() {
    let mut input = fixture();
    // The post-index store changes x2, even though its first operand is x3.
    input.functions[1].code = arm64!(at 0x2000;
        mov w1, #7;
        adrp x2, extern 0x8000;
        str x3, [x2], #8;
        bl extern 0x3000;
        ret
    );
    let result = derive(input);
    assert!(result.fields.is_empty());
    assert!(
        result
            .gaps
            .iter()
            .any(|g| g.kind == FieldGapKind::TokenTable)
    );
}

#[test]
fn cyclic_dispatch_is_bounded_and_visible() {
    let mut input = fixture();
    replace(&mut input, 0x1010, arm64!(at 0x1010; b extern 0x1010));
    let result = derive(input);
    assert!(result.partition_accounted);
    let cycle = Unresolved::at("cycle", 0x1010, 0x1000, Obstacle::Cycle);
    assert!(
        result
            .paths
            .iter()
            .any(|p| p.outcome == PathOutcome::Gap(cycle))
    );
    assert!(result.fields.is_empty());
}

#[test]
fn an_unresolved_state_alternative_stays_attached_to_the_field() {
    let mut input = fixture();
    input.functions[0].code = arm64!(at 0x1000;
        cmp w2, #7;
        b.eq extern 0x1010;
        add x0, x0, #0x38;
        b extern 0x5000;
        cbz w3, extern 0x1024; // w3 is unknown
        add x8, x0, #0x40;
        mov x0, x1;
        mov x1, x8;
        b extern 0x4000;
        ret
    );
    let result = derive(input);
    assert_eq!(result.fields.len(), 1);
    assert_eq!(result.fields[0].readers.len(), 2);
    assert!(
        result.fields[0]
            .readers
            .iter()
            .any(|j| matches!(j, ReaderJoin::Missing(_)))
    );
    assert!(
        result.fields[0]
            .readers
            .iter()
            .any(|j| matches!(j, ReaderJoin::Joined { .. }))
    );
}

#[test]
fn a_branch_into_the_constructor_cannot_bypass_argument_provenance() {
    let mut input = fixture();
    // This branch is outside the eight-instruction constructor window.
    input.functions[1].code = arm64!(at 0x2000;
        b extern 0x2030; // to the constructor call
        nop; nop; nop; nop; nop; nop; nop; nop;
        mov w1, #7;
        adrp x2, extern 0x8000;
        add x2, x2, #0;
        bl extern 0x3000;
        ret
    );
    let result = derive(input);
    assert!(result.fields.is_empty());
    assert!(
        result
            .gaps
            .iter()
            .any(|g| g.kind == FieldGapKind::TokenTable)
    );
}

#[test]
fn unreachable_token_constructors_are_not_discovered() {
    for skip in [arm64!(at 0x2000; b extern 0x2014), arm64!(at 0x2000; ret)] {
        let mut input = fixture();
        let constructor = arm64!(at 0x2004;
            mov w1, #7;
            adrp x2, extern 0x8000;
            add x2, x2, #0;
            bl extern 0x3000;
            ret
        );
        input.functions[1].code = [skip, constructor].concat();
        let result = derive(input);
        assert!(result.fields.is_empty());
        assert!(
            result
                .gaps
                .iter()
                .any(|g| g.kind == FieldGapKind::TokenTable)
        );
    }
}

#[test]
fn known_zero_tests_keep_only_feasible_reader_paths() {
    let zero = arm64!(at 0x1010; mov w3, #0);
    let one = arm64!(at 0x1010; mov w3, #1);
    let skip_if_zero = arm64!(at 0x1014; cbz w3, extern 0x1028);
    let skip_unless_zero = arm64!(at 0x1014; cbnz w3, extern 0x1028);
    for (constant, skip, has_field) in [
        (&zero[..], &skip_if_zero[..], false),
        (&one[..], &skip_if_zero[..], true),
        (&zero[..], &skip_unless_zero[..], true),
        (&one[..], &skip_unless_zero[..], false),
    ] {
        let mut input = fixture();
        let dispatch = arm64!(at 0x1000;
            cmp w2, #7;
            b.eq extern 0x1010;
            add x0, x0, #0x38;
            b extern 0x5000
        );
        let read = arm64!(at 0x1018;
            add x8, x0, #0x40;
            mov x0, x1;
            mov x1, x8;
            b extern 0x4000;
            ret
        );
        input.functions[0].code = [&dispatch[..], constant, skip, &read[..]].concat();
        let result = derive(input);
        assert_eq!(!result.fields.is_empty(), has_field);
        assert_eq!(result.paths.len(), 3, "constant alternatives must not fork");
    }
}

#[test]
fn constant_token_construction_branches_do_not_emit_dead_literals() {
    let mut input = fixture();
    input.functions[1].code = arm64!(at 0x2000;
        mov w3, #0;
        cbz w3, extern 0x2018; // always skips the constructor
        mov w1, #7;
        adrp x2, extern 0x8000;
        add x2, x2, #0;
        bl extern 0x3000;
        ret
    );
    assert!(derive(input).fields.is_empty());
}

#[test]
fn duplicate_constructor_symbols_use_one_address_and_conflicts_remain_unknown() {
    let mut input = fixture();
    let constructor = input
        .symbols
        .iter()
        .find(|s| s.address == 0x3000)
        .unwrap()
        .clone();
    input
        .symbols
        .extend(std::iter::repeat_n(constructor, 10000));
    assert_eq!(derive(input.clone()).fields.len(), 1);
    input.symbols.push(Symbol {
        name: "ambiguous_alias".into(),
        address: 0x3000,
    });
    assert!(derive(input).fields.is_empty());
}

#[test]
fn token_constructor_reachability_stops_at_external_tail_calls() {
    let mut input = fixture();
    input.functions[1].code = arm64!(at 0x2000;
        add x8, x19, #0x100, lsl #12;
        mov w1, #7;
        adrp x2, extern 0x8000;
        add x2, x2, #0;
        bl extern 0x3000;
        b extern 0x9000; // an external tail call
        mov w1, #8; // an unreachable constructor for another token
        adrp x2, extern 0x8000;
        add x2, x2, #0;
        bl extern 0x3000;
        ret
    );
    let result = derive(input);
    assert_eq!(result.fields.len(), 1);
    assert_eq!(result.fields[0].token, 7);
}
#[test]
fn a_path_that_stops_at_an_unknown_value_names_it_in_its_gap() {
    let mut input = fixture();
    replace(&mut input, 0x1000, arm64!(at 0x1000; cmp w1, #7)); // w1 is the reader, not the token
    let result = derive(input);

    let flags = Unresolved::at("flags", 0x1004, 0x1000, Obstacle::Unknown(Unknown::Flags));
    assert_eq!(result.paths.len(), 1);
    assert_eq!(result.paths[0].outcome, PathOutcome::Gap(flags));
    assert!(result.gaps.contains(&FieldGap {
        path: Some(0),
        ..FieldGap::unresolved(FieldGapKind::ReaderJoin, flags)
    }));
}
#[test]
fn a_path_that_spends_its_step_bound_names_the_bound_in_its_gap() {
    let mut root = Arm64::at(0x1000);
    for _ in 0..500 {
        arm64!(root; nop);
    }
    arm64!(root; b extern 0x5000); // CPersistent::ReadMember
    let mut input = fixture();
    input.functions[0].code = root.bytes();
    let result = derive(input);

    let spent = Unresolved::at(
        "step-limit",
        0x17d0,
        0x1000,
        Obstacle::Bound(Bound::Steps(500)),
    );
    assert_eq!(result.paths.len(), 1);
    assert_eq!(result.paths[0].outcome, PathOutcome::Gap(spent));
    assert!(result.gaps.contains(&FieldGap {
        path: Some(0),
        ..FieldGap::unresolved(FieldGapKind::ReaderJoin, spent)
    }));
}

/// Names tokens 100 to 102 and dispatches them through a jump table at 0x8040, whose entries are
/// read by `load` and whose targets are the rejection (`0x1028`), `first_table_field` and
/// `second_table_field`. `entries` are the raw table bytes.
fn table_fixture(load: Vec<u8>, entries: &[u8]) -> FieldInput {
    let mut input = fixture();
    input.strings = BTreeMap::from([
        (0x8000, "first_table_field".into()),
        (0x8010, "second_table_field".into()),
    ]);
    input.functions[1].code = arm64!(at 0x2000;
        mov w1, #100; // token 100
        adrp x2, extern 0x8000;
        add x2, x2, #0; // "first_table_field"
        bl extern 0x3000; // CToken::CToken
        mov w1, #102; // token 102
        adrp x2, extern 0x8000;
        add x2, x2, #0x10; // "second_table_field"
        bl extern 0x3000;
        ret
    );
    let dispatch = arm64!(at 0x1000;
        movn w8, #99; // -100
        add w8, w2, w8; // the index: token - 100
        cmp w8, #2;
        b.hi extern 0x1028; // tokens outside 100 to 102
        adrp x9, extern 0x8000;
        add x9, x9, #0x40; // the table
        adr x10, extern 0x1028 // the base of the entries
    );
    let cases = arm64!(at 0x1028;
        add x0, x0, #0x38;
        b extern 0x5000; // CPersistent::ReadMember
        add x8, x0, #0x40; // token 100: entry 2
        mov x0, x1;
        mov x1, x8;
        b extern 0x4000; // CReader::Read
        add x8, x0, #0x48; // token 102: entry 6
        mov x0, x1;
        mov x1, x8;
        b extern 0x4000
    );
    input.functions[0].code = [dispatch, load, arm64!(at 0x1024; br x10), cases].concat();
    input.read_only_data = vec![fields::DataSection {
        address: 0x8040,
        bytes: entries.to_vec(),
    }];
    input
}

fn halfword_table() -> FieldInput {
    table_fixture(
        arm64!(at 0x101c; ldrh w11, [x9, x8, lsl #1]; add x10, x10, x11, lsl #2),
        &[2, 0, 0, 0, 6, 0],
    )
}

fn jump_table_gaps(result: &fields::RegistryFieldResult) -> Vec<&FieldGap> {
    result
        .gaps
        .iter()
        .filter(|gap| gap.kind == FieldGapKind::JumpTable)
        .collect()
}

#[test]
fn halfword_and_byte_jump_tables_name_each_case_and_reject_the_default() {
    let byte_table = table_fixture(
        arm64!(at 0x101c; ldrb w11, [x9, x8]; add x10, x10, x11, lsl #2),
        &[2, 0, 6],
    );
    for input in [halfword_table(), byte_table] {
        let result = derive(input);

        let fields: Vec<_> = result.fields.iter().map(|f| (f.token, &*f.name)).collect();
        assert_eq!(
            fields,
            [(100, "first_table_field"), (102, "second_table_field")],
            "{:?}",
            result.gaps
        );
        assert!(
            result
                .fields
                .iter()
                .all(|f| matches!(f.readers[..], [ReaderJoin::Joined { .. }]))
        );
        let rejected: Vec<_> = result
            .paths
            .iter()
            .filter(|p| p.outcome == PathOutcome::Rejected)
            .map(|p| p.domain)
            .collect();
        assert_eq!(
            rejected,
            [[i32::MIN as i64, 99], [101, 101], [103, i32::MAX as i64]]
        );
        assert!(result.partition_accounted);
        assert!(result.gaps.is_empty(), "{:?}", result.gaps);
    }
}

#[test]
fn a_table_of_addresses_is_a_gap_that_names_the_reader_and_the_table() {
    let input = table_fixture(arm64!(at 0x101c; ldr x10, [x9, x8, lsl #3]; nop), &[0; 24]);
    let result = derive(input);

    assert!(result.fields.is_empty());
    let stop = Unresolved::at("jump-table", 0x1024, 0x1000, Obstacle::Unsupported);
    assert!(
        result
            .paths
            .iter()
            .any(|p| p.domain == [100, 102] && p.outcome == PathOutcome::Gap(stop))
    );
    assert_eq!(
        jump_table_gaps(&result),
        [&FieldGap {
            reason: "jump table at 0x8040 in CExample::ReadMember(CReader&, int): its entries are addresses".into(),
            ..FieldGap::unresolved(FieldGapKind::JumpTable, stop)
        }]
    );
    assert!(result.partition_accounted);
}

#[test]
fn unreadable_entries_and_cases_outside_the_reader_stop_only_their_tokens() {
    for (entries, reason, why) in [
        (
            &[2, 0][..],
            "jump-table-entry",
            "an entry could not be read",
        ),
        (
            &[2, 0, 0, 0, 0xff, 0][..],
            "jump-table-case",
            "a case is outside the reader",
        ),
    ] {
        let mut input = halfword_table();
        input.read_only_data[0].bytes = entries.to_vec();
        let result = derive(input);

        let fields: Vec<_> = result.fields.iter().map(|f| f.token).collect();
        assert_eq!(fields, [100]);
        let stopped = result
            .paths
            .iter()
            .find(|p| p.domain == [102, 102])
            .unwrap();
        assert!(matches!(
            stopped.outcome,
            PathOutcome::Gap(Unresolved { reason: r, .. }) if r == reason
        ));
        let gaps = jump_table_gaps(&result);
        assert_eq!(gaps.len(), 1);
        assert!(
            gaps[0]
                .reason
                .starts_with("jump table at 0x8040 in CExample::ReadMember")
        );
        assert!(gaps[0].reason.ends_with(why));
    }
}

#[test]
fn an_unbounded_table_index_spends_the_entry_bound() {
    let mut input = halfword_table();
    replace(&mut input, 0x100c, arm64!(at 0x100c; nop)); // no range check
    let result = derive(input);

    let bound = Obstacle::Bound(Bound::TableEntries(1024));
    let stop = Unresolved::at("jump-table", 0x1024, 0x1000, bound);
    assert_eq!(result.paths.len(), 1);
    assert_eq!(result.paths[0].outcome, PathOutcome::Gap(stop));
    assert_eq!(jump_table_gaps(&result).len(), 1);
}

#[test]
fn a_signed_condition_on_a_shifted_token_is_a_gap_not_a_dropped_interval() {
    let mut input = halfword_table();
    replace(&mut input, 0x100c, arm64!(at 0x100c; b.gt extern 0x1028));
    let result = derive(input);

    let stop = Unresolved::at("branch-condition", 0x100c, 0x1000, Obstacle::Unsupported);
    assert_eq!(result.paths.len(), 1);
    assert_eq!(result.paths[0].outcome, PathOutcome::Gap(stop));
    assert!(result.partition_accounted);
}

#[test]
fn a_bit_field_read_through_a_constant_index_reaches_its_reader_call() {
    let mut input = fixture();
    input.functions[0].code = arm64!(at 0x1000;
        cmp w2, #7; // token 7
        b.eq extern 0x1010;
        add x0, x0, #0x38;
        b extern 0x5000; // CPersistent::ReadMember
        mov w8, #0x20;
        ldrb w8, [x0, x8]; // the byte that holds the bit
        ubfx w8, w8, #3, #1;
        and w8, w8, #1;
        strb w8, [sp, #0x10];
        mov x9, x1; // the reader
        add x1, sp, #0x10; // a temporary, not the member
        mov x0, x9;
        bl extern 0x4000 // CReader::Read
    );
    let result = derive(input);

    assert_eq!(result.fields.len(), 1, "{:?}", result.gaps);
    assert!(matches!(
        result.fields[0].readers[..],
        [ReaderJoin::Missing(Unresolved {
            reason: "reader-routing",
            ..
        })]
    ));
}

#[test]
fn a_default_case_can_only_reject_and_other_shared_cases_are_aliases() {
    // Tokens outside 100 to 102 read the member at 0x1030, as an inherited reader would, and
    // so do tokens 100 and 101: that case is the default.
    let mut input = halfword_table();
    replace(&mut input, 0x100c, arm64!(at 0x100c; b.hi extern 0x1030));
    input.read_only_data[0].bytes = vec![2, 0, 2, 0, 6, 0];
    let result = derive(input);

    let fields: Vec<_> = result.fields.iter().map(|f| f.token).collect();
    assert_eq!(fields, [102]);
    let default = Unresolved::at("jump-table-default", 0x103c, 0x1000, Obstacle::Unsupported);
    for token in [100, 101] {
        let path = result
            .paths
            .iter()
            .find(|p| p.domain == [token, token])
            .unwrap();
        assert_eq!(path.outcome, PathOutcome::Gap(default));
    }
    assert_eq!(jump_table_gaps(&result).len(), 1);

    // Five entries: 100 and 102 read one member, and three tokens reject.
    let mut input = halfword_table();
    replace(&mut input, 0x1008, arm64!(at 0x1008; cmp w8, #4));
    input.read_only_data[0].bytes = vec![2, 0, 0, 0, 2, 0, 0, 0, 0, 0];
    let result = derive(input);

    let fields: Vec<_> = result.fields.iter().map(|f| f.token).collect();
    assert_eq!(fields, [100, 102], "{:?}", result.gaps);
    assert!(result.gaps.is_empty(), "{:?}", result.gaps);
}

#[test]
fn an_unscaled_wide_load_is_not_a_table_entry() {
    let input = table_fixture(
        arm64!(at 0x101c; ldrh w11, [x9, x8]; add x10, x10, x11, lsl #2),
        &[2, 0, 0, 0, 6, 0],
    );
    let result = derive(input);

    assert!(result.fields.is_empty());
    let stop = Unresolved::at("instruction", 0x101c, 0x1000, Obstacle::Unsupported);
    assert!(
        result
            .paths
            .iter()
            .any(|p| p.domain == [100, 102] && p.outcome == PathOutcome::Gap(stop))
    );
}

#[test]
fn a_case_without_a_known_reader_can_only_reject_when_the_default_may_be_unseen() {
    // Tokens outside 100 to 102 stop at an instruction that the walker does not run, so the
    // default may be past it. Token 102's case calls an unknown function.
    let mut input = halfword_table();
    input.functions[0].code.extend(arm64!(at 0x1050; ret));
    replace(&mut input, 0x100c, arm64!(at 0x100c; b.hi extern 0x1050));
    replace(&mut input, 0x104c, arm64!(at 0x104c; b extern 0x9000));
    let result = derive(input);

    let fields: Vec<_> = result.fields.iter().map(|f| f.token).collect();
    assert_eq!(fields, [100]);
    let default = Unresolved::at("jump-table-default", 0x104c, 0x1000, Obstacle::Unsupported);
    let path = result
        .paths
        .iter()
        .find(|p| p.domain == [102, 102])
        .unwrap();
    assert_eq!(path.outcome, PathOutcome::Gap(default));
    let gaps = jump_table_gaps(&result);
    assert_eq!(gaps.len(), 1);
    assert!(gaps[0].reason.ends_with("which an unresolved path hides"));
}

#[test]
fn a_word_copy_of_the_token_indexes_a_table() {
    let mut input = halfword_table();
    replace(&mut input, 0x1000, arm64!(at 0x1000; nop));
    replace(&mut input, 0x1004, arm64!(at 0x1004; mov w8, w2)); // the index: the token itself
    input.functions[1].code = arm64!(at 0x2000;
        mov w1, #0; // token 0
        adrp x2, extern 0x8000;
        add x2, x2, #0; // "first_table_field"
        bl extern 0x3000; // CToken::CToken
        mov w1, #2; // token 2
        adrp x2, extern 0x8000;
        add x2, x2, #0x10; // "second_table_field"
        bl extern 0x3000;
        ret
    );
    let result = derive(input);

    let fields: Vec<_> = result.fields.iter().map(|f| f.token).collect();
    assert_eq!(fields, [0, 2], "{:?}", result.gaps);
    assert!(result.partition_accounted);
}

#[test]
fn an_unresolved_path_outside_a_table_guard_does_not_hide_its_default() {
    // Tokens below 50 stop at an instruction that the walker does not run. The table's own
    // guard sends its other tokens to the rejection, and token 102's case calls an unknown
    // function.
    let mut input = halfword_table();
    input.functions[0].code = arm64!(at 0x1000;
        cmp w2, #50;
        b.lt extern 0x1058;
        movn w8, #99; // -100
        add w8, w2, w8;
        cmp w8, #2;
        b.hi extern 0x1030; // the table's guard
        adrp x9, extern 0x8000;
        add x9, x9, #0x40;
        adr x10, extern 0x1030;
        ldrh w11, [x9, x8, lsl #1];
        add x10, x10, x11, lsl #2;
        br x10;
        add x0, x0, #0x38;
        b extern 0x5000; // CPersistent::ReadMember
        add x8, x0, #0x40; // token 100: entry 2
        mov x0, x1;
        mov x1, x8;
        b extern 0x4000; // CReader::Read
        add x8, x0, #0x48; // token 102: entry 6
        mov x0, x1;
        mov x1, x8;
        b extern 0x9000; // an unknown function
        ret
    );
    let result = derive(input);

    let fields: Vec<_> = result.fields.iter().map(|f| f.token).collect();
    assert_eq!(fields, [100, 102]);
    assert!(matches!(
        result.fields[1].readers[..],
        [ReaderJoin::Missing(_)]
    ));
    assert!(jump_table_gaps(&result).is_empty());
}

fn nested_fixture() -> FieldInput {
    use fields::ObjectReader;
    let mut input = fixture();
    input.functions[0].code = arm64!(at 0x1000;
        cmp w2, #7;
        b.eq extern 0x1010;
        add x0, x0, #0x38;
        b extern 0x5000;
        mov x19, x0; // owner
        mov x20, x1; // reader
        bl extern 0x9000; // allocation
        mov x21, x0;
        bl extern 0x9100; // constructor
        str x21, [sp];
        ldr x8, [x21];
        ldr x8, [x8, #0x20];
        mov x0, x21;
        mov x1, x20;
        blr x8; // persistent read
        add x0, x19, #0x40; // collection
        mov x2, sp;
        bl extern 0x9300; // append same object
        ret
    );
    input.symbols.extend([
        Symbol {
            name: "operator new(unsigned long)".into(),
            address: 0x9000,
        },
        Symbol {
            name: "CChild::CChild()".into(),
            address: 0x9100,
        },
        Symbol {
            name: "CPersistent::Read(CReader&)".into(),
            address: 0x9200,
        },
    ]);
    input.symbols.push(Symbol {
        name: "CChild::ReadMember(CReader&, int)".into(),
        address: 0xb000,
    });
    input.functions.push(Function {
        name: "CChild::ReadMember(CReader&, int)".into(),
        address: 0xb000,
        code: arm64!(at 0xb000;
            cmp w2, #7;
            b.eq extern 0xb010;
            add x0, x0, #0x38;
            b extern 0x5000;
            add x8, x0, #0x10;
            mov x0, x1;
            mov x1, x8;
            b extern 0x4000
        ),
    });
    input.objects.push(ObjectReader {
        class: "CChild".into(),
        constructors: vec![0x9100],
        vtables: [(0, 0xa000)].into(),
        pointers: [(0xa020, 0x9200)].into(),
        read: 0x9200,
        insert: vec![0x9300],
        data_offset: Some(8),
    });
    input
}

#[test]
fn nested_fields_require_the_constructed_read_object_to_reach_its_collection() {
    let result = derive(nested_fixture());
    assert_eq!(result.collections.len(), 1);
    assert_eq!(result.collections[0].offset, 0x40);
    assert_eq!(
        result.collections[0].fields.fields[0].name,
        "new_engine_field"
    );
    for address in [0x1020, 0x1038, 0x1040] {
        let mut input = nested_fixture();
        replace(&mut input, address, arm64!(at address; mov x0, x3));
        assert!(derive(input).collections.is_empty(), "{address:#x}");
    }
}

#[test]
fn resetting_collection_storage_does_not_establish_accumulation() {
    for insertion in [0x1044, 0x1048] {
        let mut input = nested_fixture();
        let instruction = arm64!(at insertion; str wzr, [x19, #0x54]);
        let index = (insertion - 0x1000) as usize;
        input.functions[0].code.splice(index..index, instruction);
        assert!(derive(input).collections.is_empty());
    }
}

#[test]
fn reconstruction_invalidates_the_previous_persistent_read() {
    for (class, position) in [
        ("CChild", 0x3c),
        ("COther", 0x3c),
        ("CChild", 0x48),
        ("COther", 0x48),
    ] {
        let mut input = nested_fixture();
        let mut replacement = input.objects[0].clone();
        replacement.class = class.into();
        replacement.constructors = vec![0x9400];
        input.objects.push(replacement);
        let constructor = if class == "CChild" { 0x9100 } else { 0x9400 };
        let address = 0x1000 + position as u64;
        let reconstruct = arm64!(at address;
            mov x0, x21;
            bl extern constructor
        );
        input.functions[0]
            .code
            .splice(position..position, reconstruct);
        assert!(
            derive(input).collections.is_empty(),
            "{class} at {position:#x}"
        );
    }
}
