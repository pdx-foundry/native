//! The registry field method on small authored inputs.
use crate::engine::analysis::{
    assembler::arm64,
    discovery::{Symbol, candidates},
    fields::{self, FieldInput, Function, PathOutcome, ReaderJoin},
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
        selection: candidates(&symbols).remove(0),
        symbols,
        strings: BTreeMap::from([(0x8000, "new_engine_field".into())]),
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
            ReaderJoin::Missing { .. }
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
        ReaderJoin::Missing { .. }
    ));
    let mut input = fixture();
    input.strings.clear();
    let result = derive(input);
    assert!(result.fields.is_empty());
    assert!(result.gaps.iter().any(|g| g.kind == "token-table"));
}
#[test]
fn clobbered_token_constructor_arguments_do_not_reuse_stale_values() {
    let mut input = fixture();
    replace(&mut input, 0x2008, arm64!(at 0x2008; mov x2, xzr));
    let result = derive(input);
    assert!(result.fields.is_empty());
    assert!(result.gaps.iter().any(|g| g.kind == "token-table"));
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
    assert!(result.gaps.iter().any(|g| g.kind == "reader-join"));
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
    assert!(result.gaps.iter().any(|g| g.kind == "token-table"));
}

#[test]
fn cyclic_dispatch_is_bounded_and_visible() {
    let mut input = fixture();
    replace(&mut input, 0x1010, arm64!(at 0x1010; b extern 0x1010));
    let result = derive(input);
    assert!(result.partition_accounted);
    assert!(
        result
            .paths
            .iter()
            .any(|p| matches!(&p.outcome,PathOutcome::Gap(reason) if reason.contains("cycle")))
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
            .any(|j| matches!(j, ReaderJoin::Missing { .. }))
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
    assert!(result.gaps.iter().any(|g| g.kind == "token-table"));
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
        assert!(result.gaps.iter().any(|g| g.kind == "token-table"));
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
