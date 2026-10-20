//! The registry field method on small authored inputs.
use crate::engine::analysis::{
    assembler::{Arm64, arm64},
    discovery::Symbol,
    fields::{self, FieldGap, FieldGapKind, FieldInput, Function, PathOutcome, ReaderJoin},
    stop::{Bound, Obstacle, Unknown, Unresolved},
};
use std::collections::{BTreeMap, BTreeSet};

fn fixture() -> FieldInput {
    let root = "CExample::ReadMember(CReader&, int)";
    let symbols=vec![
        Symbol{name:"TSingleObjectGameDatabase<CExampleDatabase, CExample, false>::LoadFile(char const*, bool)".into(),address:0x7000},
        Symbol{name:root.into(),address:0x1000},Symbol{name:"GetTokenArray()".into(),address:0x2000},
        Symbol{name:"CToken::CToken(int, char const*)".into(),address:0x3000},Symbol{name:"CReader::Read(int&)".into(),address:0x4000},
        Symbol{name:"CPersistent::ReadMember(CReader&, int)".into(),address:0x5000},Symbol{name:"CReader::ReportUnexpected()".into(),address:0x6000},
    ];
    FieldInput {
        loader_root: None,
        serializer: None,
        key_readers: Default::default(),
        persistent: None,
        objects: vec![],
        owner: "CExample".into(),
        symbols,
        strings: BTreeMap::from([(0x8000, "new_engine_field".into())]),
        read_only_data: vec![],
        gaps: vec![],
        stubs: BTreeMap::new(),
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
fn an_immediate_reference_reader_joins_when_it_receives_the_reader() {
    use crate::ReaderKind;
    use crate::engine::analysis::readers::{classify, destination};
    let mut input = fixture();
    input
        .symbols
        .iter_mut()
        .find(|symbol| symbol.address == 0x4000)
        .unwrap()
        .name = "CExampleDatabase::ValueType const* NParserUtil::ReadKeyReference<CExampleDatabase>(CReader&, CExampleDatabase const&, bool)".into();
    let immediate = |passes_reader: bool| {
        let mut code = Arm64::at(0x1010);
        if passes_reader {
            arm64!(code; mov x0, x1); // the reader
        } else {
            arm64!(code; mov x0, x2); // the token, not the reader
        }
        arm64!(code;
            mov w2, #0; // log a missing key
            b extern 0x4000; // ReadKeyReference
            nop
        );
        code.bytes()
    };
    let mut joined = input.clone();
    let function = &mut joined.functions[0];
    function.code.splice(16..32, immediate(true));

    let readers = &derive(joined).fields[0].readers;
    assert!(
        matches!(readers[0], ReaderJoin::Joined { .. }),
        "{readers:?}"
    );
    assert_eq!(classify(readers).kind, ReaderKind::Reference);
    assert_eq!(
        destination(&readers[0]),
        None,
        "an immediate reader returns its item"
    );

    let mut wrong_reader = input.clone();
    wrong_reader.functions[0]
        .code
        .splice(16..32, immediate(false));
    let readers = &derive(wrong_reader).fields[0].readers;
    assert!(
        matches!(&readers[0], ReaderJoin::Missing(stop) if stop.reason == "reader-routing"),
        "{readers:?}"
    );

    input
        .symbols
        .iter_mut()
        .find(|symbol| symbol.address == 0x4000)
        .unwrap()
        .name = "void NParserUtil::ReadKeyReferenceUniform<CExampleDatabase, CPdxArray<CExample const*, int> >(CReader&, CExampleDatabase const&, CPdxArray<CExample const*, int>&)".into();
    let list = |output: bool| {
        let mut code = Arm64::at(0x1010);
        if output {
            arm64!(code; add x2, x0, #0x40); // the owner's list
        } else {
            arm64!(code; mov x2, x3); // not owner storage
        }
        arm64!(code;
            mov x0, x1; // the reader
            b extern 0x4000; // ReadKeyReferenceUniform
            nop
        );
        code.bytes()
    };
    let mut owned = input.clone();
    owned.functions[0].code.splice(16..32, list(true));
    let readers = &derive(owned).fields[0].readers;
    assert!(
        matches!(readers[0], ReaderJoin::Joined { .. }),
        "{readers:?}"
    );
    assert_eq!(destination(&readers[0]), Some(0x40));

    input.functions[0].code.splice(16..32, list(false));
    let readers = &derive(input).fields[0].readers;
    assert!(
        matches!(&readers[0], ReaderJoin::Missing(stop) if stop.reason == "reader-routing"),
        "{readers:?}"
    );
}

#[test]
fn block_family_follows_proven_arguments_and_preserves_conditional_conflicts() {
    use crate::BlockFamily;
    use crate::engine::analysis::readers::classify;
    let mut input = fixture();
    input
        .symbols
        .iter_mut()
        .find(|symbol| symbol.address == 0x4000)
        .unwrap()
        .name =
        "void NParserUtil::ReadTrigger<CRootTrigger>(CReader&, CRootTrigger&, EScopeType)".into();
    assert_eq!(
        classify(&derive(input.clone()).fields[0].readers).family,
        BlockFamily::Trigger
    );
    input.symbols.push(Symbol {
        address: 0x4100,
        name: "void NParserUtil::ReadEffect<CEffect>(CReader&, CEffect&, EScopeType)".into(),
    });
    input.functions[0].code = arm64!(at 0x1000;
        cmp w2, #7;
        b.eq extern 0x1010;
        add x0, x0, #0x38;
        b extern 0x5000;
        ldrb w8, [x0, #0x30]; // conditional owner state
        add x9, x0, #0x40;
        mov x0, x1;
        mov x1, x9;
        cbz w8, extern 0x1028;
        b extern 0x4000;
        b extern 0x4100
    );
    let result = derive(input.clone());
    assert_eq!(result.fields[0].readers.len(), 2);
    assert_eq!(
        classify(&result.fields[0].readers).family,
        BlockFamily::Unknown
    );
    for path in result.paths.iter().filter(|path| path.domain == [7, 7]) {
        assert_eq!(path.conditions.len(), 1);
        let PathOutcome::Reader(join) = &path.outcome else {
            panic!("{path:?}")
        };
        assert!(matches!(
            classify(std::slice::from_ref(join)).family,
            BlockFamily::Trigger | BlockFamily::Effect
        ));
    }
    replace(&mut input, 0x1028, arm64!(at 0x1028; ret));
    let result = derive(input);
    assert_eq!(
        classify(&result.fields[0].readers).family,
        BlockFamily::Unknown
    );
}
/// The readers of field 7 when its path runs `member` (at `0x1010`) and ends at a string reader.
fn string_reader_joins(member: Arm64) -> Vec<ReaderJoin> {
    let mut input = fixture();
    input
        .symbols
        .iter_mut()
        .find(|symbol| symbol.address == 0x4000)
        .unwrap()
        .name = "CReader::Read(CString&, bool)".into();
    input.functions[0].code.splice(16..32, member.bytes());

    derive(input).fields[0].readers.clone()
}

/// `member` followed by the tail call to the string reader.
fn string_read(member: impl FnOnce(&mut Arm64)) -> Arm64 {
    let mut code = Arm64::at(0x1010);
    member(&mut code);
    arm64!(code; b extern 0x4000); // CReader::Read(CString&, bool)

    code
}

#[test]
fn control_21_a_string_reader_stores_at_its_owner_offset() {
    use crate::engine::analysis::readers::destination;
    let readers = string_reader_joins(string_read(|code| {
        arm64!(code;
            add x8, x0, #0x40; // the member
            mov x0, x1; // the reader
            mov x1, x8
        );
    }));

    assert_eq!(destination(&readers[0]), Some(0x40), "{readers:?}");
}

#[test]
fn control_22_a_destination_from_another_owner_is_no_destination() {
    use crate::engine::analysis::readers::destination;
    let readers = string_reader_joins(string_read(|code| {
        arm64!(code;
            add x8, x2, #0x40; // an offset from the token, not the owner
            mov x0, x1;
            mov x1, x8
        );
    }));

    assert_eq!(destination(&readers[0]), None, "{readers:?}");
}

#[test]
fn control_23_a_truncated_destination_is_no_destination() {
    use crate::engine::analysis::readers::destination;
    let readers = string_reader_joins(string_read(|code| {
        arm64!(code;
            add x8, x0, #0x40;
            mov x0, x1;
            mov w1, w8 // the pointer truncated to 32 bits
        );
    }));

    assert_eq!(destination(&readers[0]), None, "{readers:?}");
}

#[test]
fn control_24_a_clobbered_destination_is_no_destination() {
    use crate::engine::analysis::readers::destination;
    let readers = string_reader_joins(string_read(|code| {
        arm64!(code;
            add x8, x0, #0x40;
            mov x0, x1;
            mov x8, x3; // the member address overwritten
            mov x1, x8
        );
    }));

    assert_eq!(destination(&readers[0]), None, "{readers:?}");
}

#[test]
fn control_25_an_unknown_call_before_the_reader_stops_the_path() {
    use crate::engine::analysis::readers::destination;
    let readers = string_reader_joins(string_read(|code| {
        arm64!(code;
            add x8, x0, #0x40;
            bl extern 0x6800; // a function the method does not know
            mov x0, x1;
            mov x1, x8
        );
    }));

    assert!(matches!(readers[0], ReaderJoin::Missing(_)), "{readers:?}");
    assert_eq!(destination(&readers[0]), None);
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
        mov w1, #7;
        adrp x2, extern 0x8000;
        add x2, x2, #0; // the first name again
        bl extern 0x3000;
        ret
    );
    let result = derive(input);
    assert!(result.fields.is_empty());
    let conflicts = result
        .gaps
        .iter()
        .filter(|g| g.reason.contains("conflicting names"))
        .count();
    assert_eq!(conflicts, 1);
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
            .any(|p| p.outcome == PathOutcome::Gap(cycle.clone()))
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
fn a_bit_test_of_an_owner_byte_keeps_both_paths_under_an_unstated_condition() {
    let mut input = fixture();
    input.functions[0].code = arm64!(at 0x1000;
        ldrsb w8, [x0, #0x1f]; // the sign bit of a string's last byte
        tbnz w8, #31, extern 0x1008;
        cmp w2, #7; // token 7
        b.eq extern 0x1018;
        b extern 0x5000; // CPersistent::ReadMember
        ret;
        add x8, x0, #0x40; // the member
        mov x0, x1;
        mov x1, x8;
        b extern 0x4000; // CReader::Read
        ret
    );

    let result = derive(input);

    let reads: Vec<_> = result
        .paths
        .iter()
        .filter(|path| path.domain == [7, 7])
        .collect();
    assert_eq!(result.fields.len(), 1);
    assert_eq!(reads.len(), 2);
    assert!(
        reads.iter().all(|path| path
            .conditions
            .iter()
            .any(|condition| condition.at == 0x1004 && condition.value.is_none())),
        "{reads:?}"
    );
}

#[test]
fn a_shifted_immediate_compare_of_an_owner_word_keeps_both_paths_under_its_value() {
    use crate::engine::analysis::fields::Value;

    let mut input = fixture();
    input.functions[0].code = arm64!(at 0x1000;
        ldr x8, [x0, #0x48]; // a word that an earlier key stores
        cmp x8, #0x200, lsl #12;
        b.eq extern 0x100c;
        cmp w2, #7; // token 7
        b.eq extern 0x101c;
        b extern 0x5000; // CPersistent::ReadMember
        ret;
        add x8, x0, #0x40; // the member
        mov x0, x1;
        mov x1, x8;
        b extern 0x4000; // CReader::Read
        ret
    );

    let result = derive(input);

    let reads: Vec<_> = result
        .paths
        .iter()
        .filter(|path| path.domain == [7, 7])
        .collect();
    let compared_word = Value::Load(Box::new(Value::Owner(0x48)), 8);
    let compared = Value::EqualsAny(Box::new(compared_word), vec![0x20_0000]);
    assert_eq!(result.fields.len(), 1, "{:?}", result.gaps);
    assert_eq!(reads.len(), 2);
    assert!(
        reads
            .iter()
            .all(|path| path.conditions.iter().any(
                |condition| condition.at == 0x1008 && condition.value == Some(compared.clone())
            )),
        "{reads:?}"
    );
}

/// The fixture's reader behind an order check: the owner word at `+0x48` must not hold
/// `0x200000`, or the reader runs `rejection` and returns. A second test of the same word
/// follows the check.
fn order_check_fixture(rejection: Vec<u8>) -> FieldInput {
    let mut input = fixture();
    let mut code = arm64!(at 0x1000;
        ldr x8, [x0, #0x48]; // a word that an earlier key stores
        cmp x8, #0x200, lsl #12;
        b.ne extern 0x1014
    );
    code.extend(rejection);
    code.extend(arm64!(at 0x1014;
        cmp x8, #0x200, lsl #12; // the same test again
        b.eq extern 0x1024;
        cmp w2, #7; // token 7
        b.eq extern 0x1028;
        b extern 0x5000; // CPersistent::ReadMember
        ret;
        add x8, x0, #0x40; // the member
        mov x0, x1;
        mov x1, x8;
        b extern 0x4000 // CReader::Read
    ));
    input.functions[0].code = code;
    input.symbols.push(Symbol {
        name: "CPdxLogFileAndLine::operator()(char const*, ...)".into(),
        address: 0xb000,
    });
    input.symbols.push(Symbol {
        name: "CReader::GetLineNumber() const".into(),
        address: 0xb100,
    });

    input
}

/// The read of token 7 in [`order_check_fixture`] keeps the check as a condition, and the
/// registry states no key order.
fn assert_no_order_check(rejection: Vec<u8>) {
    let result = derive(order_check_fixture(rejection));

    let reads: Vec<_> = result
        .paths
        .iter()
        .filter(|path| path.domain == [7, 7])
        .collect();
    assert!(
        reads.iter().all(|path| path
            .conditions
            .iter()
            .any(|condition| condition.at == 0x1008)),
        "{reads:?}"
    );
    assert!(result.key_order.is_empty());
}

#[test]
fn an_order_check_that_only_logs_leaves_the_key_read_always() {
    let logs = arm64!(at 0x100c;
        bl extern 0xb000; // the diagnostic
        ret
    );

    let result = derive(order_check_fixture(logs));

    let reads: Vec<_> = result
        .paths
        .iter()
        .filter(|path| path.domain == [7, 7])
        .collect();
    assert_eq!(reads.len(), 1, "{reads:?}");
    assert!(reads[0].conditions.is_empty(), "{reads:?}");
    assert!(result.partition_accounted);
    assert_eq!(result.key_order.len(), 1);
    assert!(
        result.key_order[0].allowed.is_empty(),
        "the read of token 7 passes the check"
    );
}

#[test]
fn a_rejecting_side_that_writes_the_owner_is_no_order_check() {
    assert_no_order_check(arm64!(at 0x100c;
        str wzr, [x0, #0x40]; // an owner write
        ret
    ));
}

#[test]
fn a_side_that_only_locates_a_diagnostic_is_no_order_check() {
    assert_no_order_check(arm64!(at 0x100c;
        bl extern 0xb100; // CReader::GetLineNumber, no diagnostic
        ret
    ));
}

#[test]
fn a_diagnostic_side_that_loops_is_no_order_check() {
    assert_no_order_check(arm64!(at 0x100c;
        bl extern 0xb000; // the diagnostic
        b extern 0x100c // never returns
    ));
}

#[test]
fn a_string_emplace_call_unsettles_an_owner_word_test() {
    let mut code = Arm64::at(0x1000);
    arm64!(code;
        cmp w2, #7; b.eq extern 0x1010;
        mov x0, x1; b extern 0x6000;
        mov x19, x0; mov x20, x1;
        ldr x8, [x19, #0x48]; cmp x8, #5
    );
    let joined = code.here() + 4;
    arm64!(code;
        b.ne extern joined as usize; // both sides join
        add x0, x19, #0x80;
        ldr w8, [x19, #0x94]; add w1, w8, #1;
        bl extern 0xd000; // emplace, which writes the owner
        ldr x8, [x19, #0x48]; cmp x8, #5
    );
    let after = code.here() + 4;
    arm64!(code; b.ne extern after as usize; ret);

    let result = derive(compound_fixture(code.bytes()));

    let reads = result
        .paths
        .iter()
        .filter(|path| path.domain == [7, 7])
        .count();
    assert_eq!(reads, 4, "the second test forks after the call");
}

#[test]
fn a_bit_test_of_the_token_is_a_gap() {
    let mut input = fixture();
    input.functions[0].code = arm64!(at 0x1000;
        tbnz w2, #3, extern 0x1014; // a bit of the token
        cmp w2, #7; // token 7
        b.eq extern 0x1014;
        b extern 0x5000; // CPersistent::ReadMember
        ret;
        add x8, x0, #0x40; // the member
        mov x0, x1;
        mov x1, x8;
        b extern 0x4000; // CReader::Read
        ret
    );

    let result = derive(input);

    assert!(result.fields.is_empty());
    assert!(result.paths.iter().any(
        |path| matches!(&path.outcome, PathOutcome::Gap(gap) if gap.reason == "token-bit-test")
    ));
}

/// The fixture's owner calls an inherited reader through a stub of the same name at `0x9000`.
/// The inherited reader at `0xa000` reads token 8 and returns false for every other token; the
/// owner then rejects the token through its base.
fn inherited_fixture() -> FieldInput {
    let inherited = "CBase::ReadMember(CReader&, int)";
    let mut input = fixture();
    input.functions[0].code = arm64!(at 0x1000;
        mov x19, x0; // the owner
        mov x20, x1; // the reader
        mov x21, x2; // the token
        cmp w2, #7; // token 7
        b.eq extern 0x1034;
        bl extern 0x9000; // the stub of CBase::ReadMember
        tbnz w0, #0, extern 0x1030; // handled
        mov x0, x19;
        mov x1, x20;
        mov x2, x21;
        b extern 0x5000; // CPersistent::ReadMember
        ret;
        ret;
        add x8, x0, #0x40; // the member
        mov x0, x1;
        mov x1, x8;
        b extern 0x4000 // CReader::Read
    );
    input.functions.push(Function {
        name: inherited.into(),
        address: 0xa000,
        code: arm64!(at 0xa000;
            cmp w2, #8; // token 8
            b.eq extern 0xa010;
            mov w0, #0; // not handled
            ret;
            add x8, x0, #0x48; // the inherited member
            mov x0, x1;
            mov x1, x8;
            b extern 0x4000 // CReader::Read
        ),
    });
    input.functions[1].code = arm64!(at 0x2000;
        mov w1, #7; // token 7
        adrp x2, extern 0x8000; // "new_engine_field"
        add x2, x2, #0;
        bl extern 0x3000; // CToken::CToken
        mov w1, #8; // token 8
        adrp x2, extern 0x8000; // "inherited_field"
        add x2, x2, #0x10;
        bl extern 0x3000; // CToken::CToken
        ret
    );
    input.strings.insert(0x8010, "inherited_field".into());
    input.stubs.insert(0x9000, 0xa000);
    input.symbols.extend([
        Symbol {
            name: inherited.into(),
            address: 0x9000,
        },
        Symbol {
            name: inherited.into(),
            address: 0xa000,
        },
    ]);

    input
}

/// An owner with no member reader of its own, read through a serializer that holds the owner at
/// offset 8: token 7 reads an owner member, token 8 a list into the serializer's own member.
fn serializer_fixture(owner_load: Vec<u8>) -> FieldInput {
    let reader = "CExample::CSerializer::ReadMember(CReader&, int)";
    let list = "std::__1::enable_if<!(std::is_enum<CString>::value), void>::type CReader::ReadUniform<CString>(CPdxArray<CString, int>&)";
    let mut input = fixture();
    input.functions[0].name = reader.into();
    input.functions[0].code = arm64!(at 0x1000;
        cmp w2, #7; // token 7
        b.eq extern 0x1014;
        cmp w2, #8; // token 8
        b.eq extern 0x1028;
        b extern 0x5000; // CPersistent::ReadMember
        nop; // the owner load
        add x8, x8, #0x40; // the owner member
        mov x0, x1;
        mov x1, x8;
        b extern 0x4000; // CReader::Read
        add x8, x0, #0x18; // the serializer's member
        mov x0, x1;
        mov x1, x8;
        b extern 0x7100 // CReader::ReadUniform<CString>
    );
    replace(&mut input, 0x1014, owner_load);
    input.functions[1].code = arm64!(at 0x2000;
        mov w1, #7; // token 7
        adrp x2, extern 0x8000; // "new_engine_field"
        add x2, x2, #0;
        bl extern 0x3000; // CToken::CToken
        mov w1, #8; // token 8
        adrp x2, extern 0x8000; // "serializer_list"
        add x2, x2, #0x10;
        bl extern 0x3000; // CToken::CToken
        ret
    );
    input.strings.insert(0x8010, "serializer_list".into());
    input.symbols[1].name = reader.into();
    input.symbols.push(Symbol {
        name: list.into(),
        address: 0x7100,
    });
    input.serializer = Some(fields::SerializerRoot {
        reader: reader.into(),
        owner_slot: 8,
    });

    input
}

#[test]
fn a_serializer_root_reads_the_owner_through_its_owner_slot() {
    use crate::engine::analysis::readers::destination;

    for (owner_load, owner_member) in [
        (arm64!(at 0x1014; ldr x8, [x0, #8]), Some(0x40)),
        (arm64!(at 0x1014; ldr x8, [x0, #16]), None),
    ] {
        let result = derive(serializer_fixture(owner_load));
        let names: Vec<_> = result
            .fields
            .iter()
            .map(|field| field.name.as_str())
            .collect();

        assert_eq!(
            names,
            ["new_engine_field", "serializer_list"],
            "{:?}",
            result.gaps
        );
        assert_eq!(
            result.fields[0]
                .readers
                .iter()
                .map(destination)
                .collect::<Vec<_>>(),
            [owner_member]
        );
        assert!(matches!(
            result.fields[1].readers[..],
            [ReaderJoin::Joined { .. }]
        ));
        assert_eq!(destination(&result.fields[1].readers[0]), None);
        assert!(result.partition_accounted);
        assert!(
            result
                .paths
                .iter()
                .filter(|path| ![[7, 7], [8, 8]].contains(&path.domain))
                .all(|path| path.outcome == PathOutcome::Rejected),
            "the serializer's base rejection rejects the other tokens: {:?}",
            result.paths
        );
    }
}

#[test]
fn an_inherited_reader_reads_the_tokens_that_its_caller_passes_on() {
    use crate::engine::analysis::readers::destination;

    let result = derive(inherited_fixture());

    let names: Vec<_> = result
        .fields
        .iter()
        .map(|field| field.name.as_str())
        .collect();
    assert_eq!(
        names,
        ["new_engine_field", "inherited_field"],
        "{:?}",
        result.gaps
    );
    let inherited = &result.fields[1].readers;
    assert_eq!(inherited.len(), 1);
    assert_eq!(destination(&inherited[0]), Some(0x48));
    assert!(matches!(
        inherited[0],
        ReaderJoin::Joined { tail: false, .. }
    ));
    assert!(result.partition_accounted);
    assert!(
        result
            .paths
            .iter()
            .filter(|path| ![[7, 7], [8, 8]].contains(&path.domain))
            .all(|path| path.outcome == PathOutcome::Rejected),
        "a token that the inherited reader declines reaches the owner's base rejection: {:?}",
        result.paths
    );
}

#[test]
fn a_value_reader_reads_the_tokens_of_a_member_that_its_caller_passes_on() {
    use crate::engine::analysis::readers::destination;

    let value_reader = "ReadExampleValue(SExampleValue&, CReader&)";
    let mut input = inherited_fixture();
    input.functions[0].code = arm64!(at 0x1000;
        mov x19, x0; // the owner
        mov x20, x1; // the reader
        mov x21, x2; // the token
        cmp w2, #7; // token 7
        b.eq extern 0x1038;
        add x0, x19, #0x40; // the value member
        mov x1, x20;
        bl extern 0xa000; // ReadExampleValue
        tbnz w0, #0, extern 0x1034; // handled
        mov x0, x19;
        mov x1, x20;
        mov x2, x21;
        b extern 0x5000; // CPersistent::ReadMember
        ret;
        add x8, x19, #0x80; // the owner's member
        mov x0, x20;
        mov x1, x8;
        b extern 0x4000 // CReader::Read
    );
    input
        .functions
        .retain(|function| function.address != 0xa000);
    input.functions.push(Function {
        name: value_reader.into(),
        address: 0xa000,
        code: arm64!(at 0xa000;
            ldr w8, [x1, #0x38]; // the reader's token
            cmp w8, #8; // token 8
            b.eq extern 0xa014;
            mov w0, #0; // not handled
            ret;
            add x8, x0, #8; // a word of the value
            mov x0, x1;
            mov x1, x8;
            bl extern 0x4000; // CReader::Read
            mov w0, #1; // handled
            ret
        ),
    });
    input.stubs.clear();
    input
        .symbols
        .retain(|symbol| symbol.address != 0x9000 && symbol.address != 0xa000);
    input.symbols.push(Symbol {
        name: value_reader.into(),
        address: 0xa000,
    });
    input.key_readers.reader_token = Some(0x38);

    let result = derive(input);

    let names: Vec<_> = result
        .fields
        .iter()
        .map(|field| field.name.as_str())
        .collect();
    assert_eq!(
        names,
        ["new_engine_field", "inherited_field"],
        "{:?}",
        result.gaps
    );
    assert_eq!(destination(&result.fields[1].readers[0]), Some(0x48));
    // The false return reaches the base rejection under a test of a known constant.
    let declined: Vec<_> = result
        .paths
        .iter()
        .filter(|path| ![[7, 7], [8, 8]].contains(&path.domain))
        .collect();
    assert!(declined.iter().all(|path| {
        path.outcome == PathOutcome::Rejected
            && path
                .conditions
                .iter()
                .all(|condition| condition.value == Some(fields::Value::Constant(0)))
    }));
}

#[test]
fn a_false_return_through_a_tail_called_ancestor_reaches_the_owner() {
    let ancestor = "CAncestor::ReadMember(CReader&, int)";
    let mut input = inherited_fixture();
    replace(&mut input, 0xa008, arm64!(at 0xa008; b extern 0xb000)); // CAncestor::ReadMember
    input.functions.push(Function {
        name: ancestor.into(),
        address: 0xb000,
        code: arm64!(at 0xb000; mov w0, #0; ret),
    });
    input.symbols.push(Symbol {
        name: ancestor.into(),
        address: 0xb000,
    });

    let result = derive(input);

    assert_eq!(result.fields.len(), 2, "{:?}", result.gaps);
    assert!(
        result
            .paths
            .iter()
            .filter(|path| ![[7, 7], [8, 8]].contains(&path.domain))
            .all(|path| path.outcome == PathOutcome::Rejected),
        "{:?}",
        result.paths
    );
}

#[test]
fn a_stub_that_the_image_does_not_bind_is_no_inherited_read() {
    let mut input = inherited_fixture();
    input.stubs.clear();

    let result = derive(input);

    assert_eq!(result.fields.len(), 1);
    assert!(result.paths.iter().any(|path| matches!(
        &path.outcome,
        PathOutcome::Reader(ReaderJoin::Missing(stop)) if stop.reason == "reader-routing"
    )));
}

#[test]
fn a_call_that_changes_the_token_is_no_inherited_read() {
    let mut input = inherited_fixture();
    replace(&mut input, 0x100c, arm64!(at 0x100c; add w2, w2, #1));
    replace(&mut input, 0x1010, arm64!(at 0x1010; nop));

    let result = derive(input);

    assert!(result.fields.is_empty());
    assert!(result.paths.iter().any(|path| matches!(
        &path.outcome,
        PathOutcome::Reader(ReaderJoin::Missing(stop)) if stop.reason == "reader-routing"
    )));
}

#[test]
fn an_unscaled_store_to_the_frame_keeps_the_path_to_its_reader() {
    let mut input = fixture();
    input.functions[0].code = arm64!(at 0x1000;
        cmp w2, #7; // token 7
        b.eq extern 0x100c;
        b extern 0x5000; // CPersistent::ReadMember
        mov x29, sp;
        stur w2, [x29, #-0x10];
        add x8, x0, #0x40; // the member
        mov x0, x1;
        mov x1, x8;
        b extern 0x4000 // CReader::Read
    );

    let result = derive(input);

    assert_eq!(result.fields.len(), 1, "{:?}", result.paths);
    assert!(matches!(
        result.fields[0].readers[0],
        ReaderJoin::Joined { .. }
    ));
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
    assert_eq!(result.paths[0].outcome, PathOutcome::Gap(flags.clone()));
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
    assert_eq!(result.paths[0].outcome, PathOutcome::Gap(spent.clone()));
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
            .any(|p| p.domain == [100, 102] && p.outcome == PathOutcome::Gap(stop.clone()))
    );
    assert_eq!(
        jump_table_gaps(&result),
        [&FieldGap {
            reason: "jump table at 0x8040 in CExample::ReadMember(CReader&, int): its entries are addresses".into(),
            ..FieldGap::unresolved(FieldGapKind::JumpTable, stop.clone())
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
        &result.fields[0].readers[..],
        [ReaderJoin::Joined { arguments, .. }]
            if arguments.get("x1") == Some(&fields::Value::Stack(0x10))
    ));
    assert_eq!(
        super::readers::destination(&result.fields[0].readers[0]),
        None
    );
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
        assert_eq!(path.outcome, PathOutcome::Gap(default.clone()));
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
            .any(|p| p.domain == [100, 102] && p.outcome == PathOutcome::Gap(stop.clone()))
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
    assert_eq!(path.outcome, PathOutcome::Gap(default.clone()));
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
        factories: vec![],
        initializers: vec![],
        insert: vec![0x9300],
        moving_insert: vec![],
        data_offset: Some(8),
        count_offset: Some(0x14),
        reader: None,
        persistent: None,
        objects: vec![],
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

/// The nested fixture with `CChild` built inline: `_bzero` on the object, a member constructor on
/// `+0x10`, then the primary address point stored at `+0`, as `CTechnology::ReadMember` builds
/// `CTechnologySwap` on M452. The read's vtable slot is a constant pointer of the image.
fn inline_nested_fixture() -> FieldInput {
    let mut input = nested_fixture();
    input.functions[0].code = arm64!(at 0x1000;
        cmp w2, #7;
        b.eq extern 0x1010;
        add x0, x0, #0x38;
        b extern 0x5000;
        mov x19, x0; // owner
        mov x20, x1; // reader
        bl extern 0x9000; // allocation
        mov x21, x0;
        mov x0, x21;
        mov x1, #0x40; // the object's size
        bl extern 0x9500; // _bzero
        add x0, x21, #0x10; // member at +0x10
        bl extern 0x9400; // member constructor
        mov x8, #0xa000; // CChild's primary address point
        str x8, [x21];
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
            name: "CMember::CMember()".into(),
            address: 0x9400,
        },
        Symbol {
            name: "_bzero".into(),
            address: 0x9500,
        },
    ]);
    let mut image = modifier_binding(0);
    image.constructors.clear();
    image.pointers = std::mem::take(&mut input.objects[0].pointers);
    input.persistent = Some(image);
    input.objects[0].constructors = vec![];
    input.objects[0].initializers = vec![0x9400];
    input
}

#[test]
fn an_object_built_inline_joins_when_its_primary_address_point_is_in_place() {
    let result = derive(inline_nested_fixture());
    assert_eq!(result.collections.len(), 1);
    assert_eq!(result.collections[0].offset, 0x40);
    assert_eq!(
        result.collections[0].fields.fields[0].name,
        "new_engine_field"
    );

    // The read is called directly, so only the address point tells the class.
    let read_directly = |point: Option<&[u8]>| {
        let mut input = inline_nested_fixture();
        replace(&mut input, 0x1050, arm64!(at 0x1050; bl extern 0x9200));
        match point {
            Some(store) => replace(&mut input, 0x1034, store.to_vec()),
            None => replace(&mut input, 0x1038, arm64!(at 0x1038; nop)),
        }
        input
    };
    let own_point = read_directly(Some(&arm64!(at 0x1034; mov x8, #0xa000)));
    assert_eq!(derive(own_point).collections.len(), 1);
    let other_point = read_directly(Some(&arm64!(at 0x1034; mov x8, #0xb000)));
    let no_point = read_directly(None);
    let mut member_of_the_owner = inline_nested_fixture();
    replace(
        &mut member_of_the_owner,
        0x102c,
        arm64!(at 0x102c; add x0, x19, #8),
    );
    let mut zero_fill_past_the_object = inline_nested_fixture();
    replace(
        &mut zero_fill_past_the_object,
        0x1024,
        arm64!(at 0x1024; mov x1, #0x20000),
    );
    let mut not_an_initializer = inline_nested_fixture();
    not_an_initializer.objects[0].initializers.clear();

    // The member's class is itself an object that the root may construct and insert.
    let mut member_object = inline_nested_fixture();
    let mut member = member_object.objects[0].clone();
    member.class = "CMember".into();
    member.constructors = vec![0x9400];
    member_object.objects.push(member);
    assert_eq!(derive(member_object).collections.len(), 1);

    for (case, input) in [
        ("another address point", other_point),
        ("no address point", no_point),
        ("an initializer on the owner", member_of_the_owner),
        ("a zero fill past the allocation", zero_fill_past_the_object),
        ("a call that initializes nothing", not_an_initializer),
    ] {
        assert!(derive(input).collections.is_empty(), "{case}");
    }
}

/// The nested fixture with a duplicate check between the read and the insertion, as
/// `CPolicyGroup::ReadMember` reads `option` on M452: a loop over the collection's earlier objects
/// that compares names with `_memcmp`, then a logged diagnostic, then the insertion.
fn duplicate_check_fixture(diagnostic: &str) -> FieldInput {
    let mut input = nested_fixture();
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
        ldr w22, [x19, #0x54]; // the collection's count
        mov x23, #0;
        cmp x23, x22;
        b.ge extern 0x1064;
        mov x0, x21;
        mov x1, x19;
        mov x2, #4;
        bl extern 0x9600; // _memcmp
        add x23, x23, #1;
        b extern 0x1044;
        mov x0, x19;
        bl extern 0x9700; // the diagnostic
        add x0, x19, #0x40; // collection
        mov x2, sp;
        bl extern 0x9300; // append same object
        ret
    );
    input.symbols.extend([
        Symbol {
            name: "_memcmp".into(),
            address: 0x9600,
        },
        Symbol {
            name: diagnostic.into(),
            address: 0x9700,
        },
    ]);
    input
}

#[test]
fn a_duplicate_check_with_a_diagnostic_does_not_stop_the_join() {
    let result = derive(duplicate_check_fixture(
        "CPdxLogFileAndLine::operator()(char const*, ...)",
    ));
    assert_eq!(result.collections.len(), 1);
    assert_eq!(result.collections[0].offset, 0x40);
    assert!(!result.collections[0].owned_pointer);

    let owner_call = derive(duplicate_check_fixture("COwner::Update()"));
    assert!(owner_call.collections.is_empty());
}

/// A root that keeps one `CChild` in the owner pointer at `+0x48`, as `CShipSize::ReadMember`
/// keeps `ai_ship_data` on M452. A non-null pointer is the object that an earlier read stored:
/// the root calls `release_slot` of its vtable, then constructs a new object with
/// `replacement`. A null pointer gets a `CChild`. The root stores the new object, then reads it.
fn owned_pointer_fixture(release_slot: u32, replacement: usize) -> FieldInput {
    let mut input = nested_fixture();
    input.functions[0].code = arm64!(at 0x1000;
        cmp w2, #7;
        b.eq extern 0x1010;
        add x0, x0, #0x38;
        b extern 0x5000;
        mov x19, x0; // owner
        mov x20, x1; // reader
        ldr x0, [x19, #0x48];
        cbz x0, extern 0x1040;
        ldr x8, [x0];
        ldr x8, [x8, #release_slot];
        blr x8; // release the earlier object
        bl extern 0x9000; // allocation
        mov x21, x0;
        bl extern replacement; // constructor
        b extern 0x104c;
        nop;
        bl extern 0x9000; // allocation
        mov x21, x0;
        bl extern 0x9100; // constructor
        str x21, [x19, #0x48];
        ldr x8, [x21];
        ldr x8, [x8, #0x20];
        mov x0, x21;
        mov x1, x20;
        blr x8; // persistent read
        ret
    );
    let mut other = input.objects[0].clone();
    other.class = "COther".into();
    other.constructors = vec![0x9110];
    other.vtables = [(0, 0xa100)].into();
    other.pointers = [(0xa120, 0x9200)].into();
    input.objects.push(other);
    input.symbols.push(Symbol {
        name: "COther::COther()".into(),
        address: 0x9110,
    });
    input
}

#[test]
fn an_object_that_the_owner_keeps_in_one_pointer_joins_with_its_release() {
    let result = derive(owned_pointer_fixture(8, 0x9100));
    assert_eq!(result.collections.len(), 1);
    let collection = &result.collections[0];
    assert_eq!(collection.offset, 0x48);
    assert!(collection.owned_pointer);
    assert_eq!(collection.data_offset, None);
    assert_eq!(collection.fields.fields[0].name, "new_engine_field");

    for (case, input) in [
        (
            "a call through another vtable slot",
            owned_pointer_fixture(0x10, 0x9100),
        ),
        (
            "another class after a release",
            owned_pointer_fixture(8, 0x9110),
        ),
    ] {
        assert!(derive(input).collections.is_empty(), "{case}");
    }
}

#[test]
fn an_inline_owner_joins_its_members_only_from_the_construction_that_every_path_runs() {
    // The constructor evidence of `CChild`: the root's code from `start` to `end`, then a return.
    // The member constructor installs a modifier reader's address point at `+0x10`.
    let points = |start: u64, end: u64| {
        let mut input = inline_nested_fixture();
        input
            .symbols
            .iter_mut()
            .find(|symbol| symbol.address == 0x4000)
            .unwrap()
            .name = "CReader::Read(CPersistent&)".into();
        let root = &input.functions[0];
        let mut code =
            root.code[(start - root.address) as usize..(end - root.address) as usize].to_vec();
        code.extend(arm64!(at end; ret));
        let mut evidence = modifier_binding(0);
        evidence.constructors = vec![Function {
            name: String::new(),
            address: start,
            code,
        }];
        evidence.summaries = BTreeMap::from([(0x9400, BTreeMap::from([(0, 0xb000)]))]);
        input.objects[0].persistent = Some(evidence);
        let result = derive(input);
        assert_eq!(result.collections.len(), 1);

        result.collections[0].fields.persistent_points.clone()
    };

    // The allocation returns to 0x101c, and the virtual read is at 0x1050.
    assert_eq!(points(0x101c, 0x1050), BTreeMap::from([(0x10, 0xb000)]));
    assert!(points(0x1020, 0x1050).is_empty(), "another start");
    assert!(points(0x101c, 0x104c).is_empty(), "another read");
}

#[test]
fn a_nested_owner_joins_its_members_from_its_own_constructors() {
    let nested = |root: Option<u32>, child: Option<u32>| {
        let mut input = nested_fixture();
        input
            .symbols
            .iter_mut()
            .find(|symbol| symbol.address == 0x4000)
            .unwrap()
            .name = "CReader::Read(CPersistent&)".into();
        input.persistent = root.map(modifier_binding);
        input.objects[0].persistent = child.map(modifier_binding);
        let result = derive(input);
        assert_eq!(result.collections.len(), 1);

        result.collections[0].fields.persistent_points.clone()
    };

    // The child reads its token-7 object at +0x10.
    assert_eq!(nested(None, Some(0x10)), BTreeMap::from([(0x10, 0xb000)]));
    // Negative controls: another offset in the child's own constructor, and the root's
    // constructor, which builds the root owner, not the child.
    assert!(nested(None, Some(0x40)).is_empty());
    assert!(nested(Some(0x10), None).is_empty());
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

    // A loop after the insertion that clears the count arrives at its head in a state that the
    // first arrival covers.
    let mut input = nested_fixture();
    replace(&mut input, 0x1048, arm64!(at 0x1048; ldr w3, [x19, #0x54]));
    input.functions[0].code.extend(arm64!(at 0x104c;
        cbz w3, extern 0x1058;
        str wzr, [x19, #0x54];
        b extern 0x1048;
        ret
    ));
    assert!(derive(input).collections.is_empty(), "a reset in a loop");
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

/// The nested fixture with a clause object that a factory builds into a scoped pointer, a
/// moving insertion, and the destructor of the moved-from pointer.
fn factory_fixture() -> FieldInput {
    use crate::engine::analysis::fields::{ConcreteReader, PointReader};
    let mut input = nested_fixture();
    input.functions[0].code = arm64!(at 0x1000;
        cmp w2, #7;
        b.eq extern 0x1010;
        add x0, x0, #0x38;
        b extern 0x5000;
        mov x19, x0; // owner
        mov x20, x1; // reader
        mov x8, sp; // scoped pointer
        bl extern 0x9500; // factory
        ldr x0, [sp];
        ldr x8, [x0];
        ldr x8, [x8, #0x20];
        mov x1, x20;
        blr x8; // persistent read
        add x0, x19, #0x40; // collection
        mov x2, sp;
        bl extern 0x9600; // moving insertion
        ldr x0, [sp];
        cbz x0, extern 0x1054;
        ldr x8, [x0];
        ldr x8, [x8, #8];
        blr x8; // destructor of the moved-from pointer
        ret
    );
    let mut factory = Arm64::at(0x9500);
    arm64!(factory; stp x19, x20, [sp, #-16]!);
    factory.prologue();
    arm64!(factory;
        mov x19, x8;
        bl extern 0x9000; // allocation
        mov x20, x0;
        bl extern 0x9100; // constructor
        str x20, [x19]
    );
    factory.epilogue();
    arm64!(factory; ldp x19, x20, [sp], #16; ret);
    input.functions.push(Function {
        name: "CPdxScopedPtrImpl<CChild, false> PdxMakeScopedPtr<CChild>()".into(),
        address: 0x9500,
        code: factory.bytes(),
    });
    let object = &mut input.objects[0];
    object.factories = vec![0x9500];
    object.insert = vec![];
    object.moving_insert = vec![0x9600];
    object.reader = Some(PointReader {
        point: 0xa000,
        reader: ConcreteReader {
            read: "CPersistent::Read(CReader&)".into(),
            member: "CChild::ReadMember(CReader&, int)".into(),
            family: crate::BlockFamily::TriggeredModifier,
            delegate: Some("CChild::ReadModifier(CReader&, int)".into()),
            post_read: [None, None],
        },
    });
    input
}

#[test]
fn a_factory_built_object_joins_through_a_proven_moving_insertion() {
    let result = derive(factory_fixture());

    assert_eq!(result.collections.len(), 1);
    assert_eq!(result.collections[0].offset, 0x40);
    assert_eq!(result.collections[0].reader.as_ref().unwrap().point, 0xa000);
    assert!(result.collections[0].fields.fields.is_empty());
}

#[test]
fn an_unproven_move_or_an_unconstructed_factory_object_is_not_joined() {
    let mut unproven = factory_fixture();
    unproven.objects[0].moving_insert.clear();
    assert!(derive(unproven).collections.is_empty());

    // Taken as a pointer insertion, the moved-from pointer still holds the object, so its
    // destructor is an unmodelled call.
    let mut not_cleared = factory_fixture();
    not_cleared.objects[0].insert = std::mem::take(&mut not_cleared.objects[0].moving_insert);
    assert!(derive(not_cleared).collections.is_empty());

    let mut unconstructed = factory_fixture();
    replace(&mut unconstructed, 0x9518, arm64!(at 0x9518; nop)); // constructor call
    assert!(derive(unconstructed).collections.is_empty());
}

#[test]
fn an_object_without_a_block_family_keeps_its_loader_fields() {
    let mut input = factory_fixture();
    input.objects[0].reader.as_mut().unwrap().reader.family = crate::BlockFamily::Unknown;
    let result = derive(input);

    assert!(result.collections[0].reader.is_none());
    assert_eq!(
        result.collections[0].fields.fields[0].name,
        "new_engine_field"
    );
}

fn persistent_fixture() -> FieldInput {
    let mut input = fixture();
    input
        .symbols
        .iter_mut()
        .find(|symbol| symbol.address == 0x4000)
        .unwrap()
        .name = "CReader::Read(CPersistent&)".into();
    input.persistent = Some(modifier_binding(0x40));
    input
}

/// Constructor evidence whose one constructor builds a modifier object at `offset`.
fn modifier_binding(offset: u32) -> fields::PersistentInput {
    use crate::engine::analysis::fields::{ConcreteReader, PersistentInput};
    let mut constructor = Arm64::at(0x9000);
    constructor.prologue();
    arm64!(constructor; mov x19, x0; add x0, x0, #offset; bl extern 0xa000; mov x0, x19);
    constructor.epilogue();
    arm64!(constructor; ret);
    PersistentInput {
        constructor_calls: Default::default(),
        constructor_bodies: BTreeMap::new(),
        constructors: vec![Function {
            name: "CExample::CExample()".into(),
            address: 0x9000,
            code: constructor.bytes(),
        }],
        summaries: BTreeMap::from([(0xa000, BTreeMap::from([(0, 0xb000)]))]),
        pointers: BTreeMap::new(),
        writable_slots: Default::default(),
        never_return: vec![],
        requested_words: BTreeMap::new(),
        containers: fields::ModifierContainers {
            mask_offset: 0xac,
            ..Default::default()
        },
        initialized_words: BTreeMap::new(),
        readers: BTreeMap::from([(
            0xb000,
            ConcreteReader {
                read: "shared persistent read".into(),
                member: "modifier member".into(),
                family: crate::BlockFamily::Modifier,
                delegate: None,
                post_read: [None, None],
            },
        )]),
    }
}

/// The persistent fixture whose embedded object at `0x40` has a member reader with no block
/// family. The member reader reads token 7 as a Boolean.
fn embedded_fixture() -> FieldInput {
    let member = "CSettings::ReadMember(CReader&, int)";
    let mut input = persistent_fixture();
    let concrete = input
        .persistent
        .as_mut()
        .unwrap()
        .readers
        .get_mut(&0xb000)
        .unwrap();
    concrete.member = member.into();
    concrete.family = crate::BlockFamily::Unknown;
    input.symbols.extend([
        Symbol {
            name: member.into(),
            address: 0xd000,
        },
        Symbol {
            name: "CReader::Read(bool&)".into(),
            address: 0xe000,
        },
    ]);
    input.functions.push(Function {
        name: member.into(),
        address: 0xd000,
        code: arm64!(at 0xd000;
            cmp w2, #7; // token 7
            b.eq extern 0xd00c;
            b extern 0x5000; // CPersistent::ReadMember
            add x8, x0, #0x10; // the Boolean member
            mov x0, x1; // the reader
            mov x1, x8;
            b extern 0xe000 // CReader::Read(bool&)
        ),
    });
    input
}

#[test]
fn an_embedded_object_without_a_block_family_lists_its_keys() {
    let result = derive(embedded_fixture());
    let fields = crate::session::questions::normalized_fields(&result, &Default::default());
    let crate::FieldMembers::Fields(members) = &fields[0].members else {
        panic!("members: {:?}", fields[0].members);
    };

    assert_eq!(members.len(), 1);
    assert_eq!(members[0].name, "new_engine_field");
    assert_eq!(members[0].reader.kind, crate::ReaderKind::Boolean);

    let mut family = embedded_fixture();
    family
        .persistent
        .as_mut()
        .unwrap()
        .readers
        .get_mut(&0xb000)
        .unwrap()
        .family = crate::BlockFamily::Modifier;
    assert!(derive(family).embedded.is_empty());

    let mut unnamed = embedded_fixture();
    replace(&mut unnamed, 0xd000, arm64!(at 0xd000; cmp w2, #8)); // a token with no name
    assert!(derive(unnamed).embedded.is_empty());

    let mut wider = embedded_fixture();
    wider.functions[0].code = arm64!(at 0x1000;
        cbz x3, extern 0x1024; // a state branch before the token test
        cmp w2, #7; // token 7
        b.eq extern 0x1014;
        add x0, x0, #0x38;
        b extern 0x5000; // CPersistent::ReadMember
        add x8, x0, #0x40; // the embedded object
        mov x0, x1; // the reader
        mov x1, x8;
        b extern 0x4000; // CReader::Read(CPersistent&)
        b extern 0x7100 // an unknown call that every token reaches
    );
    let result = derive(wider);
    assert_eq!(result.fields[0].name, "new_engine_field");
    assert!(result.embedded.is_empty());
}

#[test]
fn an_embedded_object_reads_no_container_only_when_the_generic_reader_hands_on_named_keys() {
    let categories = |read: &str, member: Option<Vec<u8>>| {
        let mut input = embedded_fixture();
        input
            .persistent
            .as_mut()
            .unwrap()
            .readers
            .get_mut(&0xb000)
            .unwrap()
            .read = read.into();
        if let Some(code) = member {
            input.functions.last_mut().unwrap().code = code;
        }
        let result = derive(input);
        assert_eq!(result.embedded.len(), 1, "{read}");
        let fields = crate::session::questions::normalized_fields(&result, &Default::default());

        fields[0].accepted_categories.clone()
    };
    let generic = "CPersistent::Read(CReader&)";

    assert_eq!(
        categories(generic, None),
        crate::AcceptedCategories::NotApplicable
    );
    // Negative controls: an object with its own reader, and a member reader that hands a token
    // with no name to an unknown call.
    assert_eq!(
        categories("CSettings::Read(CReader&)", None),
        crate::AcceptedCategories::Unresolved
    );
    let unnamed = arm64!(at 0xd000;
        cmp w2, #7; // token 7
        b.eq extern 0xd014;
        cmp w2, #8; // token 8, which has no name
        b.eq extern 0x7100; // an unknown call
        b extern 0x5000; // CPersistent::ReadMember
        add x8, x0, #0x10; // the Boolean member
        mov x0, x1; // the reader
        mov x1, x8;
        b extern 0xe000 // CReader::Read(bool&)
    );
    assert_eq!(
        categories(generic, Some(unnamed)),
        crate::AcceptedCategories::Unresolved
    );
}

#[test]
fn scoped_destination_requires_ctor_agreement_and_owner_reader_join() {
    let mut input = persistent_fixture();
    input
        .symbols
        .iter_mut()
        .find(|symbol| symbol.address == 0x4000)
        .unwrap()
        .name = "CVariableValue::Read(CReader&, EScopeType)".into();
    input.functions[0].code = arm64!(at 0x1000;
        cmp w2, #7;
        b.eq extern 0x1010;
        add x0, x0, #0x38;
        b extern 0x5000;
        add x8, x0, #0x40; // scoped member
        mov x0, x8;
        mov w2, #4; // scope 4
        b extern 0x4000
    );
    let result = derive(input.clone());
    assert_eq!(result.scoped_destinations.get(&0x40), Some(&0xb000));

    let mut conflict = input.clone();
    conflict
        .persistent
        .as_mut()
        .unwrap()
        .constructors
        .push(Function {
            name: "CExample::CExample(other)".into(),
            address: 0xc000,
            code: arm64!(at 0xc000; str xzr, [x0, #0x40]; ret),
        });
    assert!(derive(conflict).scoped_destinations.is_empty());

    input.functions[0].code = arm64!(at 0x1000;
        cmp w2, #7;
        b.eq extern 0x1010;
        add x0, x0, #0x38;
        b extern 0x5000;
        add x8, x0, #0x40;
        mov x0, x8;
        mov x1, xzr; // wrong reader
        mov w2, #4;
        b extern 0x4000
    );
    assert!(derive(input).scoped_destinations.is_empty());
}

/// The persistent fixture with a constructor body that stores `scope` in the object's word at
/// `0x30`, and a request for that word.
fn stored_scope_fixture(scope: u32) -> FieldInput {
    let mut input = persistent_fixture();
    let binding = input.persistent.as_mut().unwrap();
    binding.constructor_bodies.insert(
        0xa000,
        Function {
            name: "CWeight::CWeight(EScopeType)".into(),
            address: 0xa000,
            code: arm64!(at 0xa000; mov w8, #scope; str x8, [x0, #0x30]; ret),
        },
    );
    binding.requested_words = BTreeMap::from([(0xb000, BTreeSet::from([0x30]))]);
    input
}

#[test]
fn a_requested_word_needs_an_entered_constructor() {
    let input = stored_scope_fixture(4);
    assert_eq!(
        derive(input.clone()).stored_words.get(&(0x40, 0x30)),
        Some(&4)
    );

    let mut baseline_only = input;
    baseline_only
        .persistent
        .as_mut()
        .unwrap()
        .constructor_bodies
        .clear();
    assert!(derive(baseline_only).stored_words.is_empty());
}

#[test]
fn a_requested_word_needs_every_constructor_to_agree() {
    let mut input = stored_scope_fixture(4);
    let mut other = Arm64::at(0xc000);
    other.prologue();
    arm64!(other; mov x19, x0; add x0, x0, #0x40; bl extern 0xa000; mov w8, #8; str x8, [x19, #0x70]);
    other.epilogue();
    arm64!(other; ret);
    input
        .persistent
        .as_mut()
        .unwrap()
        .constructors
        .push(Function {
            name: "CExample::CExample(other)".into(),
            address: 0xc000,
            code: other.bytes(),
        });
    let result = derive(input);
    assert_eq!(result.persistent_points.get(&0x40), Some(&0xb000));
    assert!(result.stored_words.is_empty());
}

#[test]
fn persistent_family_and_identity_follow_the_constructed_destination() {
    assert_eq!(
        derive(persistent_fixture()).persistent_points.get(&0x40),
        Some(&0xb000)
    );
    let first = crate::session::questions::normalized_fields(
        &derive(persistent_fixture()),
        &Default::default(),
    );
    assert_eq!(first[0].reader.family, crate::BlockFamily::Modifier);
    let mut input = persistent_fixture();
    let concrete = input
        .persistent
        .as_mut()
        .unwrap()
        .readers
        .get_mut(&0xb000)
        .unwrap();
    concrete.member = "another member".into();
    concrete.family = crate::BlockFamily::Trigger;
    let second = crate::session::questions::normalized_fields(&derive(input), &Default::default());
    assert_eq!(second[0].reader.family, crate::BlockFamily::Trigger);
    assert_ne!(first[0].reader.id, second[0].reader.id);
    for alternative in &first[0].read {
        if let crate::FieldReadOutcome::Read { reader, .. } = &alternative.outcome {
            assert_eq!(reader.family, crate::BlockFamily::Modifier);
        }
    }
}

#[test]
fn persistent_family_requires_constructor_agreement_and_no_later_invalidation() {
    for missing in [
        "constructor",
        "summary",
        "destination",
        "conflict",
        "unknown_call",
    ] {
        let mut input = persistent_fixture();
        let binding = input.persistent.as_mut().unwrap();
        match missing {
            "constructor" => binding.constructors.clear(),
            "summary" => binding.summaries.clear(),
            "destination" => {
                binding
                    .summaries
                    .insert(0xa000, BTreeMap::from([(8, 0xb000)]));
            }
            "conflict" => {
                let mut body = Arm64::at(0xc000);
                arm64!(body; str xzr, [x0, #0x40]; ret);
                binding.constructors.push(Function {
                    name: "alternative".into(),
                    address: 0xc000,
                    code: body.bytes(),
                });
            }
            "unknown_call" => {
                let mut body = Arm64::at(0x9000);
                body.prologue();
                arm64!(body; mov x19, x0; add x0, x0, #0x40; bl extern 0xa000; bl extern 0xc000; mov x0, x19);
                body.epilogue();
                arm64!(body; ret);
                binding.constructors[0].code = body.bytes();
            }
            _ => unreachable!(),
        }
        let fields =
            crate::session::questions::normalized_fields(&derive(input), &Default::default());
        assert!(fields[0].reader.id.is_none(), "{missing}");
        assert_eq!(fields[0].reader.kind, crate::ReaderKind::Block);
        assert_eq!(
            fields[0].reader.family,
            crate::BlockFamily::Unknown,
            "{missing}"
        );
    }
}

#[test]
fn a_constructor_of_an_object_outside_the_owner_is_an_ordinary_call() {
    let points = |delegates: bool| {
        let mut input = persistent_fixture();
        let binding = input.persistent.as_mut().unwrap();
        let mut body = Arm64::at(0xc000);
        body.prologue();
        arm64!(body;
            mov x19, x0;
            add x0, sp, #0x10; // a stack object
            bl extern 0xa000 // its constructor
        );
        if delegates {
            arm64!(body; mov x0, x19; bl extern 0x9000); // the full constructor
        }
        arm64!(body; mov x0, x19);
        body.epilogue();
        arm64!(body; ret);
        binding.constructors.push(Function {
            name: "CExample::CExample()".into(),
            address: 0xc000,
            code: body.bytes(),
        });

        derive(input).persistent_points
    };

    assert_eq!(points(true), BTreeMap::from([(0x40, 0xb000)]));
    // Negative control: building the stack object does not build the owner's member.
    assert!(points(false).is_empty());
}

#[test]
fn persistent_family_follows_owner_aliases_and_inline_vtable_installation() {
    for inline in [false, true] {
        let mut input = persistent_fixture();
        let binding = input.persistent.as_mut().unwrap();
        if inline {
            binding.constructors[0].code = arm64!(at 0x9000;
                mov x8, #0xb000;
                str x8, [x0, #0x40];
                ret
            );
            binding.summaries.clear();
        }
        binding.constructors.push(Function {
            name: "CExample::CExample(alias)".into(),
            address: 0xc000,
            code: arm64!(at 0xc000; b extern 0x9000),
        });
        let fields =
            crate::session::questions::normalized_fields(&derive(input), &Default::default());
        assert_eq!(
            fields[0].reader.family,
            crate::BlockFamily::Modifier,
            "inline={inline}"
        );
    }
}

#[test]
fn member_delegates_are_not_terminal_registry_field_readers() {
    for name in [
        "CBase::ReadMember(CReader&, int)",
        "CBase::ReadMember(CReader&, int, EScopeType)",
    ] {
        let mut input = fixture();
        input.symbols.push(Symbol {
            name: name.into(),
            address: 0x9100,
        });
        input.functions[0].code = arm64!(at 0x1000;
            cmp w2, #7;
            b.eq extern 0x1010;
            add x0, x0, #0x38;
            b extern 0x5000;
            b extern 0x9100
        );
        let result = derive(input);
        assert!(
            matches!(result.fields[0].readers[0], ReaderJoin::Missing(_)),
            "{name}"
        );
        let fields = crate::session::questions::normalized_fields(&result, &Default::default());
        assert!(fields[0].reader.id.is_none());
        assert_eq!(fields[0].reader.kind, crate::ReaderKind::Unknown);
    }
}

#[test]
fn known_comparisons_do_not_invent_unreachable_readers() {
    for value in [0, 1, 2] {
        for conditional_compare in [false, true] {
            for equal_branch in [false, true] {
                let mut input = fixture();
                input.symbols.push(Symbol {
                    name: "CReader::Read(CString&, bool)".into(),
                    address: 0x4100,
                });
                let mut code = Arm64::at(0x1000);
                arm64!(code;
                    cmp w2, #7;
                    b.eq extern 0x1010;
                    add x0, x0, #0x38;
                    b extern 0x5000;
                    mov w8, #value;
                    cmp w8, #0
                );
                if conditional_compare {
                    arm64!(code; ccmp w8, #1, #4, ne);
                }
                arm64!(code; add x9, x0, #0x40; mov x0, x1; mov x1, x9);
                let taken = code.here() + 8;
                if equal_branch {
                    arm64!(code; b.eq extern taken as usize);
                } else {
                    arm64!(code; b.ne extern taken as usize);
                }
                arm64!(code; b extern 0x4000; b extern 0x4100);
                input.functions[0].code = code.bytes();
                let result = derive(input);
                let fields =
                    crate::session::questions::normalized_fields(&result, &Default::default());
                assert_eq!(fields[0].read.len(), 1);
                assert_eq!(fields[0].read[0].condition, crate::FieldCondition::Always);
                let equal = value == 0 || (conditional_compare && value == 1);
                let kind = if equal == equal_branch {
                    crate::ReaderKind::String
                } else {
                    crate::ReaderKind::Integer
                };
                assert_eq!(
                    fields[0].reader.kind, kind,
                    "{value} {conditional_compare} {equal_branch}"
                );
            }
        }
    }
}

fn compound_fixture(code: Vec<u8>) -> FieldInput {
    let mut input = fixture();
    input.functions[0].code = code;
    input.key_readers = fields::KeyReaders {
        compound_sizes: [0x190, 0x30, 0x18],
        value_token: 0x278,
        token_text: 0x10,
        true_token: None,
        reader_token: None,
        token_copy: vec![0xa000],
        target_construct: vec![0xb000],
        target_move: Some(0xc000),
        string_emplace: Some(0xd000),
        optional_string: Some(0xe000),
        string_read: Some(0xf000),
        persistent_read: None,
        string_from_text: vec![0x9000],
        array_data: 8,
        array_count: 0x14,
        string_stride: 0x28,
    };
    for (address, name) in [
        (0xa000, "copy"),
        (0xb000, "construct"),
        (0xc000, "move"),
        (0xd000, "emplace"),
        (0xe000, "optional"),
        (0xf000, "CReader::Read(CString&, bool)"),
        (0x9000, "CString::CString(char const*)"),
        (0x9800, "Owner::SetKey(CString const&)"),
    ] {
        input.symbols.push(Symbol {
            address,
            name: name.into(),
        });
    }
    input
}

#[test]
fn compound_target_requires_original_value_and_owner_destination() {
    for (token_offset, stack_destination) in [(0x278, false), (0x270, false), (0x278, true)] {
        let mut code = Arm64::at(0x1000);
        arm64!(code;
            cmp w2, #7; b.eq extern 0x1010;
            mov x0, x1; b extern 0x6000;
            mov x19, x0; mov x20, x1;
            sub sp, sp, #0x200;
            add x1, x20, #token_offset;
            mov x0, sp; bl extern 0xa000;
            add x0, sp, #0x120; mov x1, sp; bl extern 0xb000;
            add x0, x19, #0x80
        );
        if stack_destination {
            arm64!(code; mov x0, sp);
        }
        arm64!(code; add x1, sp, #0x120; bl extern 0xc000; ret);
        let result = derive(compound_fixture(code.bytes()));
        let kind = crate::engine::analysis::readers::classify(&result.fields[0].readers).kind;
        assert_eq!(
            kind == crate::ReaderKind::Target,
            token_offset == 0x278 && !stack_destination
        );
        if kind == crate::ReaderKind::Target {
            assert_eq!(
                crate::engine::analysis::readers::destination(&result.fields[0].readers[0]),
                Some(0x80)
            );
        }
    }
}

#[test]
fn compound_array_requires_emplace_before_reading_last_string() {
    for callee in [0xd000, 0xd004] {
        let code = arm64!(at 0x1000;
            cmp w2, #7; b.eq extern 0x1010;
            mov x0, x1; b extern 0x6000;
            mov x19, x0; mov x20, x1;
            add x0, x19, #0x80;
            ldr w8, [x19, #0x94]; add w1, w8, #1;
            bl extern callee;
            ldr x8, [x19, #0x88]; ldrsw x9, [x19, #0x94];
            mov w10, #0x28; madd x8, x9, x10, x8;
            sub x1, x8, #0x28; mov x0, x20; b extern 0xf000
        );
        let result = derive(compound_fixture(code));
        let join = &result.fields[0].readers[0];
        assert_eq!(
            matches!(
                join,
                ReaderJoin::Stored {
                    kind: crate::ReaderKind::String,
                    repeat: crate::RepeatBehavior::Accumulate,
                    destination: Some(0x80),
                    ..
                }
            ),
            callee == 0xd000
        );
    }
}

#[test]
fn a_stored_read_of_an_inherited_reader_that_returns_has_no_repeat() {
    let inherited = "CBase::ReadMember(CReader&, int)";
    let mut input = compound_fixture(arm64!(at 0x1000;
        mov x19, x0; // the owner
        mov x20, x1; // the reader
        mov x21, x2; // the token
        bl extern 0x7800; // CBase::ReadMember
        tbnz w0, #0, extern 0x1024; // handled
        mov x0, x19;
        mov x1, x20;
        mov x2, x21;
        b extern 0x5000; // CPersistent::ReadMember
        ret
    ));
    input.functions.push(Function {
        name: inherited.into(),
        address: 0x7800,
        code: arm64!(at 0x7800;
            cmp w2, #7; b.eq extern 0x7810;
            mov w0, #0; ret;
            mov x19, x0; mov x20, x1;
            add x0, x19, #0x80;
            ldr w8, [x19, #0x94]; add w1, w8, #1;
            bl extern 0xd000; // emplace
            ldr x8, [x19, #0x88]; ldrsw x9, [x19, #0x94];
            mov w10, #0x28; madd x8, x9, x10, x8;
            sub x1, x8, #0x28; mov x0, x20; b extern 0xf000 // CReader::Read(CString&, bool)
        ),
    });
    input.symbols.push(Symbol {
        name: inherited.into(),
        address: 0x7800,
    });

    let result = derive(input);

    assert!(
        matches!(
            result.fields[0].readers[..],
            [ReaderJoin::Stored {
                kind: crate::ReaderKind::String,
                repeat: crate::RepeatBehavior::Unknown,
                destination: Some(0x80),
                ..
            }]
        ),
        "{:?}",
        result.fields
    );
}

#[test]
fn an_inherited_reader_after_a_pending_stored_read_is_not_followed() {
    let inherited = "CBase::ReadMember(CReader&, int)";
    let mut input = compound_fixture(arm64!(at 0x1000;
        cmp w2, #7; b.eq extern 0x1010;
        mov x0, x1; b extern 0x6000;
        mov x19, x0; mov x20, x1; mov x21, x2;
        add x0, x19, #0x80;
        ldr w8, [x19, #0x94]; add w1, w8, #1;
        bl extern 0xd000; // emplace
        ldr x8, [x19, #0x88]; ldrsw x9, [x19, #0x94];
        mov w10, #0x28; madd x8, x9, x10, x8;
        sub x1, x8, #0x28; mov x0, x20;
        bl extern 0xf000; // CReader::Read(CString&, bool), not a tail call
        mov x0, x19; mov x1, x20; mov x2, x21;
        bl extern 0x7800; // CBase::ReadMember
        ret
    ));
    input.functions.push(Function {
        name: inherited.into(),
        address: 0x7800,
        code: arm64!(at 0x7800;
            cmp w2, #7; b.eq extern 0x7810;
            mov w0, #0; ret;
            add x8, x0, #0x48; // a second member
            mov x0, x1;
            mov x1, x8;
            b extern 0x4000 // CReader::Read
        ),
    });
    input.symbols.push(Symbol {
        name: inherited.into(),
        address: 0x7800,
    });

    let result = derive(input);

    let read: Vec<_> = result
        .paths
        .iter()
        .filter(|path| path.domain == [7, 7])
        .map(|path| &path.outcome)
        .collect();
    assert!(
        matches!(
            read[..],
            [PathOutcome::Gap(stop)] if stop.reason == "compound-reader-overwrite"
        ),
        "{read:?}"
    );
}

#[test]
fn only_a_plain_string_read_gives_a_derived_name_its_storage() {
    use crate::engine::analysis::names::{StoredField, storage};
    let array = arm64!(at 0x1000;
        cmp w2, #7; b.eq extern 0x1010;
        mov x0, x1; b extern 0x6000;
        mov x19, x0; mov x20, x1;
        add x0, x19, #0x80;
        ldr w8, [x19, #0x94]; add w1, w8, #1;
        bl extern 0xd000;
        ldr x8, [x19, #0x88]; ldrsw x9, [x19, #0x94];
        mov w10, #0x28; madd x8, x9, x10, x8;
        sub x1, x8, #0x28; mov x0, x20; b extern 0xf000
    );
    let scalar = arm64!(at 0x1000;
        cmp w2, #7; b.eq extern 0x1010;
        mov x0, x1; b extern 0x6000;
        add x8, x0, #0x80; mov x0, x1; mov x1, x8; b extern 0xf000
    );

    let array = derive(compound_fixture(array));
    assert!(matches!(
        array.fields[0].readers[..],
        [ReaderJoin::Stored {
            kind: crate::ReaderKind::String,
            destination: Some(0x80),
            ..
        }]
    ));
    assert_eq!(storage(&array).strings, []);

    let scalar = derive(compound_fixture(scalar));
    assert!(matches!(
        scalar.fields[0].readers[..],
        [ReaderJoin::Joined { .. }]
    ));
    assert_eq!(
        storage(&scalar).strings,
        [StoredField {
            path: vec!["new_engine_field".into()],
            offset: 0x80,
            element: None,
        }]
    );
}

#[test]
fn a_string_read_that_returns_through_an_empty_call_is_final() {
    for (consumer, body_returns, last) in [
        (0x9800usize, true, true),
        (0x9800, false, false),
        (0xa000, true, false),
    ] {
        let code = arm64!(at 0x1000;
            cmp w2, #7; b.eq extern 0x1010;
            mov x0, x1; b extern 0x6000;
            add x19, x0, #0x80;
            mov x0, x1; mov x1, x19; mov w2, #0;
            bl extern 0xf000;
            mov x0, x19;
            ldp x29, x30, [sp, #0x10];
            add sp, sp, #0x20;
            b extern consumer
        );
        let mut input = compound_fixture(code);
        let body = if body_returns {
            arm64!(at 0x9800; ret)
        } else {
            arm64!(at 0x9800; str xzr, [x0]; ret)
        };
        input.functions.push(Function {
            name: "Owner::SetKey(CString const&)".into(),
            address: 0x9800,
            code: body,
        });
        let result = derive(input);

        assert!(
            matches!(
                result.fields[0].readers[..],
                [ReaderJoin::Joined { tail, .. }] if tail == last
            ),
            "{consumer:#x} {body_returns}: {:?}",
            result.fields[0].readers
        );
    }
}

#[test]
fn compound_tail_call_does_not_return_to_the_next_instruction() {
    let code = arm64!(at 0x1000;
        cmp w2, #7; b.eq extern 0x1010;
        mov x0, x1; b extern 0x6000;
        mov x19, x0; mov x20, x1;
        add x0, x19, #0x80;
        ldr w8, [x19, #0x94]; add w1, w8, #1;
        b extern 0xd000; // the emplace helper returns to this reader's caller
        ldr x8, [x19, #0x88]; ldrsw x9, [x19, #0x94];
        mov w10, #0x28; madd x8, x9, x10, x8;
        sub x1, x8, #0x28; mov x0, x20; b extern 0xf000
    );
    let result = derive(compound_fixture(code));
    assert!(result.fields.iter().all(|field| {
        field
            .readers
            .iter()
            .all(|join| !matches!(join, ReaderJoin::Stored { .. }))
    }));
    assert!(
        format!("{result:?}").contains("compound-reader-return"),
        "{result:?}"
    );
}

#[test]
fn compound_optional_string_requires_value_token_text() {
    for offset in [0x288, 0x280] {
        let code = arm64!(at 0x1000;
            cmp w2, #7; b.eq extern 0x1010;
            mov x0, x1; b extern 0x6000;
            sub sp, sp, #16;
            ldr x8, [x1, #offset]; str x8, [sp];
            add x0, x0, #0x80; mov x1, sp; bl extern 0xe000; ret
        );
        let result = derive(compound_fixture(code));
        assert_eq!(
            matches!(
                result.fields[0].readers[0],
                ReaderJoin::Stored {
                    kind: crate::ReaderKind::String,
                    ..
                }
            ),
            offset == 0x288
        );
    }
}

/// A stated rule: a value-text string passed to an owner call is a string read, whatever the
/// callee stores. A path that reaches no such call still names its field.
#[test]
fn a_value_text_string_passed_to_the_owner_reads_a_string() {
    let read = |text: &str, receiver: &str, overwrite: bool| {
        let mut code = Arm64::at(0x1000);
        arm64!(code;
            cmp w2, #7; b.eq extern 0x1010;
            mov x0, x1; b extern 0x6000;
            mov x19, x0; mov x20, x1; sub sp, sp, #0x40
        );
        match text {
            "value" => arm64!(code; ldr x1, [x20, #0x288]),
            _ => arm64!(code; adr x1, #0x40),
        }
        arm64!(code; add x0, sp, #8; bl extern 0x9000);
        if overwrite {
            arm64!(code; str xzr, [sp, #8]);
        }
        arm64!(code; add x1, sp, #8);
        match receiver {
            "owner" => arm64!(code; mov x0, x19),
            "after a branch" => arm64!(code; mov x0, x19; b.eq extern 0x1010),
            _ => arm64!(code; mov x0, x20),
        }
        arm64!(code; bl extern 0x9800; ret);
        derive(compound_fixture(code.bytes())).fields[0].readers[0].clone()
    };

    assert!(matches!(
        read("value", "owner", false),
        ReaderJoin::Stored {
            kind: crate::ReaderKind::String,
            destination: None,
            ..
        }
    ));
    for join in [
        read("literal", "owner", false),
        read("value", "reader", false),
        read("value", "owner", true),
        read("value", "after a branch", false),
    ] {
        assert!(matches!(join, ReaderJoin::Missing(_)), "{join:?}");
    }
}

#[test]
fn compound_readers_forget_overwritten_stack_evidence() {
    let target = arm64!(at 0x1000;
        cmp w2, #7; b.eq extern 0x1010;
        mov x0, x1; b extern 0x6000;
        mov x19, x0; mov x20, x1; sub sp, sp, #0x200;
        add x1, x20, #0x278; mov x0, sp; bl extern 0xa000;
        str xzr, [sp];
        add x0, sp, #0x120; mov x1, sp; bl extern 0xb000;
        add x0, x19, #0x80; add x1, sp, #0x120; bl extern 0xc000; ret
    );
    let optional = arm64!(at 0x1000;
        cmp w2, #7; b.eq extern 0x1010;
        mov x0, x1; b extern 0x6000;
        sub sp, sp, #16; ldr x8, [x1, #0x288]; strb w8, [sp];
        add x0, x0, #0x80; mov x1, sp; bl extern 0xe000; ret
    );
    for code in [target, optional] {
        let result = derive(compound_fixture(code));
        assert!(!matches!(
            result.fields[0].readers[0],
            ReaderJoin::Stored { .. }
        ));
    }
}

#[test]
fn compound_target_overwrite_cannot_establish_a_reader() {
    for overwrite in [false, true] {
        let mut code = Arm64::at(0x1000);
        arm64!(code; cmp w2, #7; b.eq extern 0x1010;
            mov x0, x1; b extern 0x6000;
            mov x19, x0; mov x20, x1; sub sp, sp, #0x200;
            add x1, x20, #0x278; mov x0, sp; bl extern 0xa000;
            add x0, sp, #0x120; mov x1, sp; bl extern 0xb000;
            add x0, x19, #0x80; add x1, sp, #0x120; bl extern 0xc000);
        if overwrite {
            arm64!(code; str xzr, [x19, #0x88]);
        }
        arm64!(code; ret);
        let result = derive(compound_fixture(code.bytes()));
        assert_eq!(
            result
                .fields
                .first()
                .is_some_and(
                    |field| crate::engine::analysis::readers::classify(&field.readers).kind
                        == crate::ReaderKind::Target
                ),
            !overwrite
        );
        if overwrite {
            assert!(result.paths.iter().any(|path| matches!(&path.outcome, PathOutcome::Gap(stop) if stop.reason == "compound-reader-overwrite")));
        }
    }
}

#[test]
fn compound_string_array_clear_cannot_establish_accumulation() {
    for clear in [false, true] {
        let mut code = Arm64::at(0x1000);
        arm64!(code; cmp w2, #7; b.eq extern 0x1010;
            mov x0, x1; b extern 0x6000;
            mov x19, x0; mov x20, x1; add x0, x19, #0x80;
            ldr w8, [x19, #0x94]; add w1, w8, #1; bl extern 0xd000;
            ldr x8, [x19, #0x88]; ldrsw x9, [x19, #0x94];
            mov w10, #0x28; madd x8, x9, x10, x8;
            sub x1, x8, #0x28; mov x0, x20; bl extern 0xf000);
        if clear {
            arm64!(code; add x0, x19, #0x80; bl extern 0x11000);
        }
        arm64!(code; ret);
        let result = derive(compound_fixture(code.bytes()));
        assert_eq!(
            result.fields.first().is_some_and(|field| matches!(
                field.readers[0],
                ReaderJoin::Stored {
                    repeat: crate::RepeatBehavior::Accumulate,
                    ..
                }
            )),
            !clear
        );
        if clear {
            assert!(result.paths.iter().any(|path| matches!(&path.outcome, PathOutcome::Gap(stop) if stop.reason == "compound-reader-overwrite")));
        }
    }
}

#[test]
fn compound_cleanup_checks_both_pointer_comparison_paths() {
    for overwrite in [false, true] {
        let mut code = Arm64::at(0x1000);
        arm64!(code; cmp w2, #7; b.eq extern 0x1010;
            mov x0, x1; b extern 0x6000;
            mov x19, x0; sub sp, sp, #16;
            ldr x8, [x1, #0x288]; str x8, [sp];
            add x0, x19, #0x80; mov x1, sp; bl extern 0xe000;
            ldr x8, [sp]; mov x9, sp; cmp x8, x9; b.eq extern 0x1040);
        if overwrite {
            arm64!(code; str xzr, [x19, #0x88]);
        } else {
            arm64!(code; nop);
        }
        arm64!(code; ret);
        let result = derive(compound_fixture(code.bytes()));
        assert_eq!(result.paths.iter().any(|path| matches!(&path.outcome, PathOutcome::Gap(stop) if stop.reason == "compound-reader-overwrite")), overwrite);
        if !overwrite {
            assert!(
                result
                    .paths
                    .iter()
                    .filter(|path| path.domain == [7, 7])
                    .all(|path| matches!(
                        path.outcome,
                        PathOutcome::Reader(ReaderJoin::Stored { .. })
                    ))
            );
        }
    }
}

#[test]
fn compound_destination_saved_on_stack_is_still_protected() {
    let code = arm64!(at 0x1000;
        cmp w2, #7; b.eq extern 0x1010;
        mov x0, x1; b extern 0x6000;
        mov x19, x0; sub sp, sp, #32;
        add x8, x19, #0x80; str x8, [sp, #24];
        ldr x8, [x1, #0x288]; str x8, [sp];
        add x0, x19, #0x80; mov x1, sp; bl extern 0xe000;
        str xzr, [sp, #8]; ldr x0, [sp, #24]; bl extern 0x11000; ret);
    let result = derive(compound_fixture(code));
    assert!(result.paths.iter().any(|path| matches!(&path.outcome, PathOutcome::Gap(stop) if stop.reason == "compound-reader-overwrite")));
}

#[test]
fn compound_reader_rejects_helpers_with_any_owner_address() {
    for owner_offset in [None, Some(0), Some(0x20), Some(0x300)] {
        let mut code = Arm64::at(0x1000);
        arm64!(code; cmp w2, #7; b.eq extern 0x1010;
            mov x0, x1; b extern 0x6000;
            mov x19, x0; mov x20, x1; sub sp, sp, #0x200;
            add x1, x20, #0x278; mov x0, sp; bl extern 0xa000;
            add x0, sp, #0x120; mov x1, sp; bl extern 0xb000;
            add x0, x19, #0x80; add x1, sp, #0x120; bl extern 0xc000);
        if let Some(offset) = owner_offset {
            arm64!(code; add x0, x19, #offset);
        } else {
            arm64!(code; mov x0, sp);
        }
        arm64!(code; bl extern 0x11000; ret);
        let mut input = compound_fixture(code.bytes());
        input.symbols.push(Symbol {
            address: 0x11000,
            name: "helper".into(),
        });
        input.functions.push(Function {
            address: 0x11000,
            name: "helper".into(),
            code: arm64!(at 0x11000; str xzr, [x0, #0x80]; ret),
        });
        let result = derive(input);
        assert_eq!(
            result
                .fields
                .first()
                .is_some_and(
                    |field| crate::engine::analysis::readers::classify(&field.readers).kind
                        == crate::ReaderKind::Target
                ),
            owner_offset.is_none()
        );
        assert_eq!(result.paths.iter().any(|path| matches!(&path.outcome, PathOutcome::Gap(stop) if stop.reason == "compound-reader-overwrite")), owner_offset.is_some());
    }
}

#[test]
fn compound_reader_rejects_owner_pointers_reachable_through_the_stack() {
    for case in 0..4 {
        let mut code = Arm64::at(0x1000);
        arm64!(code; cmp w2, #7; b.eq extern 0x1010;
            mov x0, x1; b extern 0x6000;
            mov x19, x0; mov x20, x1; sub sp, sp, #0x200;
            add x1, x20, #0x278; mov x0, sp; bl extern 0xa000;
            add x0, sp, #0x120; mov x1, sp; bl extern 0xb000;
            add x0, x19, #0x80; add x1, sp, #0x120; bl extern 0xc000);
        match case {
            0 => {
                arm64!(code; str x19, [sp, #16]; add x0, sp, #16);
            }
            1 => {
                arm64!(code; str x19, [sp, #64]; mov x0, sp);
            }
            2 => {
                arm64!(code; str x19, [sp, #64]; add x8, sp, #64;
                str x8, [sp, #16]; add x0, sp, #16);
            }
            _ => {
                arm64!(code; str x9, [sp, #64]; mov x0, sp);
            }
        }
        arm64!(code; bl extern 0x11000; ret);
        let mut input = compound_fixture(code.bytes());
        input.symbols.push(Symbol {
            address: 0x11000,
            name: "helper".into(),
        });
        input.functions.push(Function {
            address: 0x11000,
            name: "helper".into(),
            code: arm64!(at 0x11000; ldr x8, [x0]; str xzr, [x8, #0x80]; ret),
        });
        let result = derive(input);
        assert!(result.fields.is_empty(), "case {case}: {:?}", result.fields);
        assert!(
            result.paths.iter().any(|path| matches!(&path.outcome,
            PathOutcome::Gap(stop) if stop.reason == "compound-reader-overwrite")),
            "case {case}"
        );
    }
}

#[test]
fn scoped_retained_owner_routing_controls() {
    // The retained root Read, timed Assign, and held-out Assign owner shapes.
    for shape in 0..3 {
        for fault in 0..4 {
            let mut input = fixture();
            let callee = if shape == 0 {
                "CVariableValue::Read(CReader&, EScopeType)"
            } else {
                "CVariableValue::Assign(CToken const&, EScopeType, CString const&)"
            };
            input
                .symbols
                .iter_mut()
                .find(|symbol| symbol.address == 0x4000)
                .unwrap()
                .name = callee.into();
            input.key_readers.value_token = 0x278;
            let mut code = Arm64::at(0x1000);
            arm64!(code; cmp w2, #7; b.eq extern 0x1010; mov x0, x1; b extern 0x6000);
            if fault == 3 {
                arm64!(code; mov x1, x9);
            }
            if shape > 0 {
                arm64!(code; add x3, x0, #0x28; add x1, x1, #0x278);
            }
            arm64!(code; add x0, x0, #0x80; mov x2, #4);
            if shape == 2 {
                arm64!(code; bl extern 0x4000; ret);
            } else {
                arm64!(code; b extern 0x4000);
            }
            input.functions[0].code = code.bytes();
            match fault {
                1 => {
                    input.functions.remove(0);
                }
                2 => {
                    input.symbols.retain(|symbol| symbol.address != 0x4000);
                }
                _ => {}
            }
            let result = fields::analyze(&input);
            let joined = result.as_ref().is_ok_and(|result| {
                result.fields.iter().any(|field| {
                    crate::engine::analysis::readers::classify(&field.readers).kind
                        == crate::ReaderKind::ScopedNumeric
                })
            });
            assert_eq!(
                joined,
                fault == 0,
                "shape {shape}, fault {fault}: {result:?}"
            );
        }
    }
}

#[test]
fn scoped_destination_follows_called_constructor_code_instead_of_only_its_primary_vtable() {
    let mut input = persistent_fixture();
    input
        .symbols
        .iter_mut()
        .find(|symbol| symbol.address == 0x4000)
        .unwrap()
        .name = "CVariableValue::Read(CReader&, EScopeType)".into();
    input.functions[0].code = arm64!(at 0x1000;
        cmp w2, #7;
        b.eq extern 0x1010;
        add x0, x0, #0x38;
        b extern 0x5000;
        add x8, x0, #0x48; // embedded operand, beyond the constructor's primary vtable
        mov x0, x8;
        mov w2, #4;
        b extern 0x4000
    );
    let binding = input.persistent.as_mut().unwrap();
    let mut body = Arm64::at(0xa000);
    body.address(8, 0xb000);
    arm64!(body; str x8, [x0]);
    body.address(8, 0xd000);
    arm64!(body; str x8, [x0, #8]; ret);
    binding.constructor_bodies.insert(
        0xa000,
        Function {
            address: 0xa000,
            name: "CMember::CMember()".into(),
            code: body.bytes(),
        },
    );
    assert_eq!(
        derive(input.clone()).scoped_destinations.get(&0x48),
        Some(&0xd000)
    );

    // An unknown stack pointer may be the owner, so a store relative to it may reach the owner.
    let mut lost = input.clone();
    let mut constructor = Arm64::at(0x9000);
    constructor.prologue();
    arm64!(constructor; mov x19, x0; add x0, x0, #0x40; bl extern 0xa000);
    arm64!(constructor;
        mov x20, sp;
        mov sp, x21;
        str xzr, [sp, #0x48];
        mov sp, x20;
        mov x0, x19
    );
    constructor.epilogue();
    arm64!(constructor; ret);
    lost.persistent.as_mut().unwrap().constructors[0].code = constructor.bytes();
    assert!(!derive(lost).scoped_destinations.contains_key(&0x48));

    // An incoming argument may hold the owner, which its caller may have published.
    let mut aliased = input.clone();
    let mut constructor = Arm64::at(0x9000);
    constructor.prologue();
    arm64!(constructor;
        mov x19, x0;
        mov x20, x1;
        add x0, x0, #0x40;
        bl extern 0xa000;
        str xzr, [x20, #0x48];
        mov x0, x19
    );
    constructor.epilogue();
    arm64!(constructor; ret);
    aliased.persistent.as_mut().unwrap().constructors[0].code = constructor.bytes();
    assert!(!derive(aliased).scoped_destinations.contains_key(&0x48));

    // Changed code leaves the embedded point unresolved; only the primary metadata survives.
    input
        .persistent
        .as_mut()
        .unwrap()
        .constructor_bodies
        .get_mut(&0xa000)
        .unwrap()
        .code = arm64!(at 0xa000; brk #0);
    assert!(!derive(input).scoped_destinations.contains_key(&0x48));
}

#[test]
fn registry_member_tail_call_keeps_the_direct_call_fallback_for_unsupported_code() {
    let mut input = persistent_fixture();
    input
        .symbols
        .iter_mut()
        .find(|symbol| symbol.address == 0x4000)
        .unwrap()
        .name = "CVariableValue::Read(CReader&, EScopeType)".into();
    input.functions[0].code = arm64!(at 0x1000;
        cmp w2, #7;
        b.eq extern 0x1010;
        add x0, x0, #0x38;
        b extern 0x5000;
        add x8, x0, #0x40; // scoped destination supplied by the fallback
        mov x0, x8;
        mov w2, #4;
        b extern 0x4000
    );
    let binding = input.persistent.as_mut().unwrap();
    binding.constructor_bodies.insert(
        0xa000,
        Function {
            address: 0xa000,
            name: "CMember::CMember()".into(),
            code: arm64!(at 0xa000; brk #0),
        },
    );
    let direct = derive(input.clone());
    let mut tail = Arm64::at(0x9000);
    arm64!(tail; add x0, x0, #0x40; b extern 0xa000);
    input.persistent.as_mut().unwrap().constructors[0].code = tail.bytes();
    let tail = derive(input);
    assert_eq!(direct.scoped_destinations.get(&0x40), Some(&0xb000));
    assert_eq!(tail.fields, direct.fields);
    assert_eq!(tail.scoped_destinations, direct.scoped_destinations);
}

#[test]
fn a_registry_writable_slot_has_no_established_target() {
    const SLOT: u64 = 0x88000;
    for (shape, writable) in [
        ("store", false),
        ("store", true),
        ("copy", false),
        ("copy", true),
    ] {
        let mut input = persistent_fixture();
        input
            .symbols
            .iter_mut()
            .find(|symbol| symbol.address == 0x4000)
            .unwrap()
            .name = "CVariableValue::Read(CReader&, EScopeType)".into();
        input.functions[0].code = arm64!(at 0x1000;
            cmp w2, #7;
            b.eq extern 0x1010;
            add x0, x0, #0x38;
            b extern 0x5000;
            add x8, x0, #0x48; // embedded operand
            mov x0, x8;
            mov w2, #4;
            b extern 0x4000
        );
        let binding = input.persistent.as_mut().unwrap();
        let mut body = Arm64::at(0xa000);
        body.address(8, 0xb000);
        arm64!(body; str x8, [x0]);
        if shape == "store" {
            body.address(8, 0xd000);
            arm64!(body; str x8, [x0, #8]);
            body.load(3, SLOT); // an unknown target, but no owner address was stored there
            arm64!(body; str xzr, [x3]; ret);
            binding.pointers.insert(SLOT, 0x90000);
        } else {
            body.load(8, SLOT); // earlier code may have replaced the slot's target
            arm64!(body; str x8, [x0, #8]; ret);
            binding.pointers.insert(SLOT, 0xd000);
        }
        binding.constructor_bodies.insert(
            0xa000,
            Function {
                address: 0xa000,
                name: "CMember::CMember()".into(),
                code: body.bytes(),
            },
        );
        if writable {
            binding.writable_slots.insert(SLOT);
        }
        let established = shape == "store" || !writable;
        assert_eq!(
            derive(input).scoped_destinations.get(&0x48),
            established.then_some(&0xd000),
            "{shape}"
        );
    }
}

/// The pointer slot, in constant data, of a category mask constant.
const MASK_SLOT: u64 = 0x2_0000;
/// A category mask constant.
const MASK_CONSTANT: u64 = 0x2_1010;
/// A category mask word in zero-fill memory.
const ZERO_FILL_MASK: u64 = 0x2_2054;

/// The persistent fixture whose constructor builds its 0x40 member with the container
/// constructor 0xa000, after `argument` sets `w2`.
fn member_container_fixture(argument: impl Fn(&mut Arm64)) -> FieldInput {
    let mut input = persistent_fixture();
    let mut constructor = Arm64::at(0x9000);
    constructor.prologue();
    arm64!(constructor; mov x19, x0; add x0, x0, #0x40);
    argument(&mut constructor);
    arm64!(constructor; bl extern 0xa000; mov x0, x19);
    constructor.epilogue();
    arm64!(constructor; ret);
    let binding = input.persistent.as_mut().unwrap();
    binding.constructors[0].code = constructor.bytes();
    binding.containers.category = BTreeSet::from([0xa000]);
    input
}

fn member_mask(input: FieldInput) -> Result<u64, &'static str> {
    derive(input).container_masks[&0x40]
        .clone()
        .map_err(|unresolved| unresolved.reason)
}

#[test]
fn a_member_container_has_the_mask_of_its_category_argument() {
    let immediate = member_container_fixture(|code| arm64!(code; mov w2, #0x407c));
    let mut default = persistent_fixture();
    default.persistent.as_mut().unwrap().containers.default = BTreeSet::from([0xa000]);

    assert_eq!(member_mask(immediate), Ok(0x407c));
    assert_eq!(
        member_mask(default),
        Ok(crate::engine::analysis::modifiers::EVERY_CATEGORY)
    );
}

#[test]
fn a_member_container_built_inline_has_the_mask_that_the_owner_stores() {
    let mut inline = persistent_fixture();
    let mut constructor = Arm64::at(0x9000);
    constructor.prologue();
    arm64!(constructor;
        mov x19, x0;
        add x0, x0, #0x40;
        bl extern 0xa000;
        movz x8, #1;
        movk x8, #0xffff, lsl #32;
        movk x8, #0xffff, lsl #48;
        str x8, [x19, #0xe8]; // the default (1, every category) at member+0xa8
        mov x0, x19
    );
    constructor.epilogue();
    arm64!(constructor; ret);
    inline.persistent.as_mut().unwrap().constructors[0].code = constructor.bytes();

    assert_eq!(
        member_mask(inline),
        Ok(crate::engine::analysis::modifiers::EVERY_CATEGORY)
    );
}

#[test]
fn an_ordinary_call_after_an_inline_container_leaves_its_mask_to_the_run_that_keeps_the_owner() {
    let mut inline = persistent_fixture();
    let mut constructor = Arm64::at(0x9000);
    constructor.prologue();
    arm64!(constructor;
        mov x19, x0;
        add x0, x0, #0x40;
        bl extern 0xa000;
        movz x8, #1;
        movk x8, #0xffff, lsl #32;
        movk x8, #0xffff, lsl #48;
        str x8, [x19, #0xe8]; // the default (1, every category) at member+0xa8
        add x0, x19, #0x200;
        bl extern 0xd000; // an ordinary member constructor that the baseline does not enter
        mov x0, x19
    );
    constructor.epilogue();
    arm64!(constructor; ret);
    inline.persistent.as_mut().unwrap().constructors[0].code = constructor.bytes();

    assert_eq!(
        member_mask(inline),
        Ok(crate::engine::analysis::modifiers::EVERY_CATEGORY)
    );
}

#[test]
fn a_constructor_that_keeps_the_owner_and_stores_no_mask_leaves_the_container_unreached() {
    let mut missing = persistent_fixture();
    let mut inline = Arm64::at(0xc000);
    inline.prologue();
    arm64!(inline;
        mov x19, x0;
        add x0, x0, #0x40;
        bl extern 0xa000;
        movz x8, #1;
        movk x8, #0xffff, lsl #32;
        movk x8, #0xffff, lsl #48;
        str x8, [x19, #0xe8]; // the default (1, every category) at member+0xa8
        mov x0, x19
    );
    inline.epilogue();
    arm64!(inline; ret);
    missing
        .persistent
        .as_mut()
        .unwrap()
        .constructors
        .push(Function {
            name: "CExample::CExample(inline)".into(),
            address: 0xc000,
            code: inline.bytes(),
        });

    assert_eq!(member_mask(missing), Err("container-unreached"));
}

#[test]
fn an_unknown_category_argument_or_a_disagreement_is_unresolved_not_every_category() {
    let unknown = member_container_fixture(|code| arm64!(code; ldr w2, [x1]));
    let mut disagreeing = member_container_fixture(|code| arm64!(code; mov w2, #0x407c));
    let mut other = Arm64::at(0xc000);
    other.prologue();
    arm64!(other; mov x19, x0; add x0, x0, #0x40; mov w2, #0x80; bl extern 0xa000; mov x0, x19);
    other.epilogue();
    arm64!(other; ret);
    disagreeing
        .persistent
        .as_mut()
        .unwrap()
        .constructors
        .push(Function {
            name: "CExample::CExample(other)".into(),
            address: 0xc000,
            code: other.bytes(),
        });

    assert_eq!(member_mask(unknown), Err("category-argument"));
    assert_eq!(member_mask(disagreeing), Err("container-paths-disagree"));
}

/// The factory fixture whose object constructor 0x9100 is a container constructor, after
/// `argument` sets `w2` in the factory, with a mask constant behind a constant pointer slot and
/// an initialized zero-fill mask.
fn clause_container_fixture(argument: impl Fn(&mut Arm64)) -> FieldInput {
    let mut input = factory_fixture();
    let mut factory = Arm64::at(0x9500);
    arm64!(factory; stp x19, x20, [sp, #-16]!);
    factory.prologue();
    arm64!(factory; mov x19, x8; bl extern 0x9000; mov x20, x0);
    argument(&mut factory);
    arm64!(factory; mov x0, x20; bl extern 0x9100; str x20, [x19]);
    factory.epilogue();
    arm64!(factory; ldp x19, x20, [sp], #16; ret);
    let function = input
        .functions
        .iter_mut()
        .find(|function| function.address == 0x9500)
        .unwrap();
    function.code = factory.bytes();
    input.read_only_data.push(fields::DataSection {
        address: MASK_CONSTANT,
        bytes: 0x400a_ca7eu32.to_le_bytes().to_vec(),
    });
    let mut binding = persistent_fixture().persistent.unwrap();
    binding.pointers = BTreeMap::from([(MASK_SLOT, MASK_CONSTANT)]);
    binding.containers.category = BTreeSet::from([0x9100]);
    binding.initialized_words = BTreeMap::from([(ZERO_FILL_MASK, 0x403f_dafe)]);
    input.persistent = Some(binding);
    input
}

fn clause_mask(input: FieldInput) -> Option<Result<u64, &'static str>> {
    let result = derive(input);
    let mask = result.collections[0].container_mask.clone();
    mask.map(|mask| mask.map_err(|unresolved| unresolved.reason))
}

#[test]
fn a_collected_container_has_the_mask_that_its_factory_passes() {
    let immediate = clause_container_fixture(|code| arm64!(code; mov w2, #0x80));
    let constant = clause_container_fixture(|code| {
        code.load(9, MASK_SLOT);
        arm64!(code; ldr w2, [x9]);
    });
    let zero_fill = clause_container_fixture(|code| {
        code.address(9, ZERO_FILL_MASK);
        arm64!(code; ldr w2, [x9]);
    });

    assert_eq!(clause_mask(immediate), Some(Ok(0x80)));
    assert_eq!(clause_mask(constant), Some(Ok(0x400a_ca7e)));
    assert_eq!(clause_mask(zero_fill), Some(Ok(0x403f_dafe)));
    assert_eq!(clause_mask(factory_fixture()), None);
}

/// The scopes that the factory fixture's constructor receives, after `argument` sets `x3` in the
/// factory, when the constructor is named `constructor`.
fn construction_scopes(constructor: &str, argument: impl Fn(&mut Arm64)) -> Vec<Option<i64>> {
    let mut input = clause_container_fixture(argument);
    for symbol in input
        .symbols
        .iter_mut()
        .filter(|symbol| symbol.address == 0x9100)
    {
        symbol.name = constructor.into();
    }

    derive(input).collections[0].construction_scopes.clone()
}

#[test]
fn a_collected_object_has_the_scope_that_its_constructor_receives() {
    const SCOPED: &str = "CChild::CChild(int, ModifierCategory, EScopeType)";
    let constant = |code: &mut Arm64| arm64!(code; mov w3, #0x40);
    let unknown = |code: &mut Arm64| arm64!(code; ldr x3, [x20, #0x10]);

    assert_eq!(construction_scopes(SCOPED, constant), [Some(0x40)]);
    assert_eq!(construction_scopes(SCOPED, unknown), [None]);
    assert_eq!(
        construction_scopes("CChild::CChild(int, ModifierCategory)", constant),
        []
    );
}

/// The mask of the container that the collection inserts. The root builds `inserted` with
/// `build`, and also a container with another mask before it and another after it that it never
/// inserts. Constructor `0x9100` gives its category argument as the mask; `0x9400` gives none.
fn inserted_container_mask(build: impl Fn(&mut Arm64)) -> Option<Result<u64, &'static str>> {
    let mut input = nested_fixture();
    let mut root = Arm64::at(0x1000);
    arm64!(root;
        cmp w2, #7;
        b.eq extern 0x1010;
        add x0, x0, #0x38;
        b extern 0x5000;
        mov x19, x0; // owner
        mov x20, x1; // reader
        bl extern 0x9000;
        mov w2, #0x100;
        bl extern 0x9100; // a container that is never inserted
        bl extern 0x9000;
        mov x21, x0
    );
    build(&mut root);
    arm64!(root;
        str x21, [sp];
        ldr x8, [x21];
        ldr x8, [x8, #0x20];
        mov x0, x21;
        mov x1, x20;
        blr x8; // persistent read
        add x0, x19, #0x40;
        mov x2, sp;
        bl extern 0x9300; // insertion
        bl extern 0x9000;
        mov w2, #0x200;
        bl extern 0x9100; // another container that is never inserted
        ret
    );
    input.functions[0].code = root.bytes();
    input.objects[0].constructors.push(0x9400);
    let mut binding = persistent_fixture().persistent.unwrap();
    binding.containers.category = BTreeSet::from([0x9100]);
    input.persistent = Some(binding);

    clause_mask(input)
}

#[test]
fn an_inserted_container_keeps_its_own_mask_beside_other_containers() {
    let masked = |code: &mut Arm64| arm64!(code; mov w2, #0x80; bl extern 0x9100);
    let unmasked = |code: &mut Arm64| arm64!(code; bl extern 0x9400);
    let masked_then_unmasked =
        |code: &mut Arm64| arm64!(code; mov w2, #0x80; bl extern 0x9100; bl extern 0x9400);

    assert_eq!(inserted_container_mask(masked), Some(Ok(0x80)));
    assert_eq!(inserted_container_mask(unmasked), None);
    assert_eq!(inserted_container_mask(masked_then_unmasked), None);
}

#[test]
fn a_mask_behind_a_writable_pointer_slot_is_unresolved() {
    let mut writable = clause_container_fixture(|code| {
        code.load(9, MASK_SLOT);
        arm64!(code; ldr w2, [x9]);
    });
    let binding = writable.persistent.as_mut().unwrap();
    binding.writable_slots = BTreeSet::from([MASK_SLOT]);

    assert_eq!(clause_mask(writable), Some(Err("category-argument")));
}

/// The example owner with a loader that reads each root statement and builds the owner in the
/// scope that its key selects: key 7 in scope 4 and key 8 in scope 0. The loader passes the
/// scope in `w21` unless `scope_register` names another.
fn loader_fixture(scope_register: &str) -> FieldInput {
    let mut input = fixture();
    let loader = "CExampleDatabase::ReadFile(CReader&)";
    input.symbols.extend([
        Symbol {
            name: loader.into(),
            address: 0x9000,
        },
        Symbol {
            name: "CReader::ReadSimpleStatement()".into(),
            address: 0xa000,
        },
        Symbol {
            name: "CExample::CExample(EScopeType)".into(),
            address: 0xc000,
        },
    ]);
    input.strings.insert(0x8020, "example_kind".into());
    input.functions[1].code = arm64!(at 0x2000;
        mov w1, #7; // token 7
        adrp x2, extern 0x8000; // "new_engine_field"
        add x2, x2, #0;
        bl extern 0x3000; // CToken::CToken
        mov w1, #8; // token 8
        adrp x2, extern 0x8000;
        add x2, x2, #0x20; // "example_kind"
        bl extern 0x3000;
        ret
    );
    let mut code = Arm64::at(0x9000);
    arm64!(code;
        mov x23, x1;
        mov x0, x23; // 0x9004: the statement loop
        bl extern 0xa000; // CReader::ReadSimpleStatement
        ldr w28, [x23, #0x38]; // 0x900c: the statement's token
        cmp w28, #7;
        b.eq extern 0x9024;
        cmp w28, #8;
        b.eq extern 0x902c;
        b extern 0x9004 // any other key: the next statement
    );
    match scope_register {
        "w21" => arm64!(code; mov w21, #4; b extern 0x9030),
        _ => arm64!(code; mov w9, #4; b extern 0x9030), // a caller-saved register
    }
    arm64!(code;
        mov w21, #0; // 0x902c
        mov x0, x23; // 0x9030
        bl extern 0xb000; // an unrelated call
        mov w0, #0x630;
        bl extern 0xb100; // the allocation
        mov x1, x21;
        bl extern 0xc000; // CExample::CExample(EScopeType)
        ret
    );
    input.functions.push(Function {
        name: loader.into(),
        address: 0x9000,
        code: code.bytes(),
    });
    input.loader_root = Some(fields::LoaderRoot {
        function: loader.into(),
        start: 0x900c,
        reader: "x23".into(),
        token_offset: 0x38,
        constructors: BTreeSet::from([0xc000]),
        item_reader: None,
    });
    input
}

/// The scope argument of each root key's constructor calls.
fn loader_scopes(
    result: &fields::RegistryFieldResult,
) -> BTreeMap<String, Vec<Option<fields::Value>>> {
    result
        .loader
        .as_ref()
        .unwrap()
        .keys
        .iter()
        .map(|key| {
            let scopes = key
                .readers
                .iter()
                .map(|join| match join {
                    ReaderJoin::Joined { arguments, .. } => arguments.get("x1").cloned(),
                    _ => panic!("{join:?}"),
                })
                .collect();
            (key.name.clone(), scopes)
        })
        .collect()
}

#[test]
fn a_loader_root_key_joins_the_owner_constructor_with_the_scope_it_selects() {
    use fields::Value;
    let result = derive(loader_fixture("w21"));
    let loader = result.loader.as_ref().unwrap();

    assert_eq!(
        loader_scopes(&result),
        BTreeMap::from([
            ("example_kind".into(), vec![Some(Value::Constant(0))]),
            ("new_engine_field".into(), vec![Some(Value::Constant(4))]),
        ])
    );
    let others: Vec<_> = loader
        .paths
        .iter()
        .filter(|path| path.domain[0] != path.domain[1])
        .collect();
    assert_eq!(others.len(), 2, "{:?}", loader.paths);
    assert!(
        others
            .iter()
            .all(|path| matches!(path.outcome, PathOutcome::Rejected))
    );
    assert!(loader.gaps.is_empty(), "{:?}", loader.gaps);
    assert_eq!(
        result.fields.len(),
        1,
        "the owner's own fields are unchanged"
    );
}

#[test]
fn a_scope_in_a_caller_saved_register_does_not_survive_a_call() {
    let result = derive(loader_fixture("w9"));

    assert_eq!(loader_scopes(&result)["new_engine_field"], vec![None]);
}

#[test]
fn an_owner_without_a_loader_root_has_no_loader_keys() {
    assert!(derive(fixture()).loader.is_none());
}

#[test]
fn a_tail_call_out_of_the_loader_does_not_build_the_owner() {
    let mut input = loader_fixture("w21");
    replace(&mut input, 0x9028, arm64!(at 0x9028; b extern 0xb000));
    let result = derive(input);
    let tail = result
        .loader
        .as_ref()
        .unwrap()
        .paths
        .iter()
        .find(|path| path.domain == [7, 7])
        .unwrap();

    assert!(!loader_scopes(&result).contains_key("new_engine_field"));
    assert!(matches!(&tail.outcome, PathOutcome::Gap(stop) if stop.reason == "loader-tail-call"));
}

#[test]
fn a_uniform_list_read_into_an_owner_array_joins_as_a_block_reader() {
    use crate::ReaderKind;
    use crate::engine::analysis::readers::{classify, destination};

    let list = |array: Vec<u8>| {
        let mut input = fixture();
        input
            .symbols
            .iter_mut()
            .find(|symbol| symbol.address == 0x4000)
            .unwrap()
            .name = "std::__1::enable_if<!(std::is_enum<CString>::value), void>::type CReader::ReadUniform<CString>(CPdxArray<CString, int>&)".into();
        input.functions[0].code.splice(16..32, array);
        derive(input)
    };

    let owner_array = arm64!(at 0x1010;
        add x8, x0, #0x40; // the array member
        mov x0, x1; // the reader
        mov x1, x8;
        b extern 0x4000 // CReader::ReadUniform<CString>
    );
    let result = list(owner_array);
    let readers = &result.fields[0].readers;
    assert!(
        matches!(readers[0], ReaderJoin::Joined { .. }),
        "{readers:?}"
    );
    assert_eq!(classify(readers).kind, ReaderKind::Block);
    assert_eq!(classify(readers).family, crate::BlockFamily::NotApplicable);
    assert_eq!(destination(&readers[0]), Some(0x40));

    let stack_array = arm64!(at 0x1010;
        add x8, sp, #0x10; // a temporary array that the loader converts later
        mov x0, x1; // the reader
        mov x1, x8;
        b extern 0x4000 // CReader::ReadUniform<CString>
    );
    let result = list(stack_array);
    let readers = &result.fields[0].readers;
    assert_eq!(classify(readers).kind, ReaderKind::Block);
    assert_eq!(destination(&readers[0]), None);

    let loaded_array = arm64!(at 0x1010;
        ldr x8, [x0, #0x40]; // an array that the owner points to
        mov x0, x1; // the reader
        mov x1, x8;
        b extern 0x4000 // CReader::ReadUniform<CString>
    );
    let result = list(loaded_array);
    assert!(
        matches!(result.fields[0].readers[0], ReaderJoin::Missing(_)),
        "{:?}",
        result.fields[0].readers
    );
}

/// A field input whose token 7 reads a Boolean into the stack temporary at `sp + 0x10`, then runs
/// `merge` from `0x1030`, with the owner in `x19`.
fn boolean_temporary(merge: Vec<u8>) -> fields::RegistryFieldResult {
    let mut input = fixture();
    input
        .symbols
        .iter_mut()
        .find(|symbol| symbol.address == 0x4000)
        .unwrap()
        .name = "CReader::Read(bool&)".into();
    let mut code = arm64!(at 0x1000;
        cmp w2, #7; // token 7
        b.eq extern 0x1010;
        add x0, x0, #0x38;
        b extern 0x5000; // CPersistent::ReadMember
        mov x20, x1; // the reader
        mov x19, x0; // the owner
        ubfx w8, w8, #3, #1;
        strb w8, [sp, #0x10];
        add x1, sp, #0x10; // a temporary, not the member
        ldrb w21, [sp, #0x10]; // the old value, kept across the call
        mov x0, x20;
        bl extern 0x4000 // CReader::Read(bool&)
    );
    code.extend(merge);
    input.functions[0].code = code;
    derive(input)
}

#[test]
fn a_boolean_merged_into_one_bit_of_an_owner_word_replaces_that_bit() {
    use crate::{ReaderKind, RepeatBehavior};

    let merged = boolean_temporary(arm64!(at 0x1030;
        ldrb w8, [sp, #0x10]; // the temporary
        ldr w9, [x19, #0x40];
        and w9, w9, #0xfffffff7; // clear bit 3
        orr w8, w9, w8, lsl #3;
        b extern 0x1044;
        str w8, [x19, #0x40];
        ret
    ));
    assert_eq!(
        merged.fields[0].readers,
        [ReaderJoin::Stored {
            callee: "CReader::Read(bool&)".into(),
            kind: ReaderKind::Boolean,
            destination: None,
            repeat: RepeatBehavior::Replace,
        }],
        "{:?}",
        merged.gaps
    );

    let copied = boolean_temporary(arm64!(at 0x1030;
        ldrb w8, [sp, #0x10]; // the temporary
        strb w8, [x19, #0x48];
        ldp d9, d8, [sp], #0x70; // a restore after the merge may move the stack pointer
        ret
    ));
    assert!(matches!(
        copied.fields[0].readers[..],
        [ReaderJoin::Stored {
            destination: Some(0x48),
            repeat: RepeatBehavior::Replace,
            ..
        }]
    ));
}

#[test]
fn a_merge_that_is_not_one_bit_of_the_stored_word_keeps_the_reader_call() {
    for merge in [
        arm64!(at 0x1030;
            ldrb w8, [sp, #0x10];
            ldr w9, [x19, #0x40];
            and w9, w9, #0xfffffff3; // clears bits 2 and 3
            orr w8, w9, w8, lsl #3;
            str w8, [x19, #0x40];
            ret
        ),
        arm64!(at 0x1030;
            ldrb w8, [sp, #0x10];
            ldr w9, [x19, #0x40];
            and w9, w9, #0xfffffff7;
            orr w8, w9, w8, lsl #2; // the byte lands on another bit
            str w8, [x19, #0x40];
            ret
        ),
        arm64!(at 0x1030;
            ldrb w8, [sp, #0x10];
            ldr w9, [x19, #0x40];
            and w9, w9, #0xfffffff7;
            orr w8, w9, w8, lsl #3;
            str w8, [x19, #0x44]; // another word
            ret
        ),
        arm64!(at 0x1030;
            ldrb w8, [sp, #0x10];
            ldr w9, [x19, #0x40];
            and w9, w9, #0xfffffff7;
            orr w8, w9, w8, lsl #3;
            bl extern 0x4000; // a call before the store
            str w8, [x19, #0x40];
            ret
        ),
    ] {
        let result = boolean_temporary(merge);
        assert!(
            matches!(
                &result.fields[0].readers[..],
                [ReaderJoin::Joined { arguments, tail: false, .. }]
                    if arguments.get("x1") == Some(&fields::Value::Stack(0x10))
            ),
            "{:?}",
            result.fields[0].readers
        );
    }
}

#[test]
fn a_temporary_loaded_before_the_reader_call_is_not_the_value_read() {
    let result = boolean_temporary(arm64!(at 0x1030;
        ldr w9, [x19, #0x40];
        and w9, w9, #0xfffffff7;
        orr w8, w9, w21, lsl #3; // the old value, not the one read
        str w8, [x19, #0x40];
        ret
    ));
    assert!(
        matches!(
            result.fields[0].readers[..],
            [ReaderJoin::Joined { tail: false, .. }]
        ),
        "{:?}",
        result.fields[0].readers
    );
}

#[test]
fn a_continuation_that_ends_without_a_return_keeps_the_reader_call() {
    for continuation in [
        arm64!(at 0x1030;
            ldrb w8, [sp, #0x10] // the function ends here
        ),
        arm64!(at 0x1030;
            ldrb w8, [sp, #0x10];
            b extern 0x1030 // a cycle
        ),
        arm64!(at 0x1030;
            ldrb w8, [sp, #0x10];
            ldp x29, x30, [sp], #0x10; // moves the stack pointer
            strb w8, [x19, #0x48];
            ret
        ),
    ] {
        let result = boolean_temporary(continuation);
        assert!(
            matches!(
                result.fields[0].readers[..],
                [ReaderJoin::Joined { tail: false, .. }]
            ),
            "{:?}",
            result.fields[0].readers
        );
    }
}

#[test]
fn a_merged_boolean_temporary_is_a_known_reader_of_its_table_case() {
    // As above, the default may be past the unresolved wide path. Token 102's case reads a
    // Boolean into a temporary and merges it into bit 3 of the owner word at 0x40.
    let mut input = halfword_table();
    input
        .symbols
        .iter_mut()
        .find(|symbol| symbol.address == 0x4000)
        .unwrap()
        .name = "CReader::Read(bool&)".into();
    input.functions[0].code.extend(arm64!(at 0x1050;
        ret;
        mov x19, x0; // the owner
        mov x20, x1; // the reader
        add x1, sp, #0x10; // the temporary
        mov x0, x20;
        bl extern 0x4000; // CReader::Read(bool&)
        ldrb w8, [sp, #0x10];
        ldr w9, [x19, #0x40];
        and w9, w9, #0xfffffff7;
        orr w8, w9, w8, lsl #3;
        str w8, [x19, #0x40];
        ret
    ));
    replace(&mut input, 0x100c, arm64!(at 0x100c; b.hi extern 0x1050));
    replace(&mut input, 0x1040, arm64!(at 0x1040; b extern 0x1054));
    let result = derive(input);

    let fields: Vec<_> = result.fields.iter().map(|f| f.token).collect();
    assert_eq!(fields, [100, 102], "{:?}", result.gaps);
    assert!(matches!(
        result.fields[1].readers[..],
        [ReaderJoin::Stored {
            destination: None,
            ..
        }]
    ));
    assert!(jump_table_gaps(&result).is_empty(), "{:?}", result.gaps);
}

/// The reader's offset of the value token in the keyword fixtures.
const VALUE_TOKEN: i64 = 0x278;

/// Field 7 converts the value token with `GetExampleFromToken(int)`, which returns 1 for token 8
/// (`alpha`) and the fallback 0 for any other token; `rest` follows the call, from `0x1024`, and
/// ends with the epilogue at `0x1100`. `alpha` may get a second literal name.
fn keyword_fixture(rest: &[u8], second_alpha: bool) -> FieldInput {
    let mut input = fixture();
    input.key_readers.value_token = VALUE_TOKEN;
    input.symbols.extend([
        Symbol {
            name: "GetExampleFromToken(int)".into(),
            address: 0x9000,
        },
        Symbol {
            name: "CReader::ReportMalformed()".into(),
            address: 0xa000,
        },
        Symbol {
            name: "CExample::Touch(CReader&)".into(),
            address: 0xb000,
        },
    ]);
    input
        .strings
        .extend([(0x8100, "alpha".into()), (0x8200, "beta".into())]);
    let root = &mut input.functions[0];
    root.code.truncate(16);
    root.code.extend(arm64!(at 0x1010;
        stp x19, x20, [sp, #-16]!;
        mov x19, x0; // the owner
        mov x20, x1; // the reader
        ldr w0, [x20, #0x278]; // the value token
        bl extern 0x9000 // GetExampleFromToken
    ));
    root.code.extend(rest);
    while root.code.len() < 0x100 {
        root.code.extend(arm64!(at 0; nop));
    }
    root.code.extend(arm64!(at 0x1100;
        ldp x19, x20, [sp], #16;
        ret
    ));
    let names = &mut input.functions[1];
    names.code.truncate(names.code.len() - 4);
    names.code.extend(arm64!(at 0x2010;
        mov w1, #8; // token 8
        adrp x2, extern 0x8000; // "alpha"
        add x2, x2, #0x100;
        bl extern 0x3000 // CToken::CToken
    ));
    if second_alpha {
        names.code.extend(arm64!(at 0x2020;
            mov w1, #8; // token 8
            adrp x2, extern 0x8000; // "beta"
            add x2, x2, #0x200;
            bl extern 0x3000 // CToken::CToken
        ));
    }
    names.code.extend(arm64!(at 0; ret));
    input.functions.push(Function {
        name: "GetExampleFromToken(int)".into(),
        address: 0x9000,
        code: arm64!(at 0x9000;
            cmp w0, #8; // token 8
            b.ne extern 0x9010;
            mov w0, #1; // alpha
            ret;
            mov w0, #0; // the fallback
            ret
        ),
    });
    input
}

/// Store the converter's result in the owner, and report it when it is the fallback.
fn reported() -> Vec<u8> {
    arm64!(at 0x1024;
        str w0, [x19, #0x40];
        cbnz w0, extern 0x1100;
        bl extern 0xa000; // CReader::ReportMalformed
        b extern 0x1100
    )
}

#[test]
fn a_converted_value_stored_in_the_owner_is_a_keyword() {
    let silent = arm64!(at 0x1024; str w0, [x19, #0x40]; b extern 0x1100);
    let result = derive(keyword_fixture(&silent, false));

    assert_eq!(result.fields.len(), 1, "{:?}", result.gaps);
    assert_eq!(
        result.fields[0].readers,
        [ReaderJoin::Stored {
            callee: "GetExampleFromToken(int)".into(),
            kind: crate::ReaderKind::Keyword,
            destination: Some(0x40),
            repeat: crate::RepeatBehavior::Replace,
        }]
    );
    // Every unknown name keeps the fallback 0, with no diagnostic.
    assert_eq!(
        result.keyword_domains[&7],
        Err("An unknown name keeps the converter's fallback with no diagnostic.")
    );
}

#[test]
fn a_reported_fallback_leaves_the_accepted_names() {
    let result = derive(keyword_fixture(&reported(), false));

    // Both sides of the test of the result read the field.
    let readers = &result.fields[0].readers;
    assert_eq!(readers.len(), 2);
    assert!(
        readers.iter().all(|join| *join == readers[0]),
        "{readers:?}"
    );
    assert_eq!(result.keyword_domains[&7], Ok(vec!["alpha".to_owned()]));
}

#[test]
fn a_keyword_domain_needs_one_literal_name_and_a_followed_caller() {
    let ambiguous = derive(keyword_fixture(&reported(), true));
    assert_eq!(
        ambiguous.keyword_domains[&7],
        Err("An accepted value has no single literal name.")
    );

    // After the store, the caller passes the reader to a call that the method does not follow.
    let touches_reader = arm64!(at 0x1024;
        str w0, [x19, #0x40];
        mov x0, x20; // the reader
        bl extern 0xb000; // CExample::Touch
        b extern 0x1100
    );
    let unresolved = derive(keyword_fixture(&touches_reader, false));
    assert!(matches!(
        unresolved.fields[0].readers[..],
        [ReaderJoin::Stored { .. }]
    ));
    assert_eq!(
        unresolved.keyword_domains[&7],
        Err("A path of the caller could not be followed for some value.")
    );
}

#[test]
fn a_converter_keeps_the_field_when_it_is_no_keyword() {
    let reason = |result: &fields::RegistryFieldResult| match &result.fields[..] {
        [field] => match &field.readers[..] {
            [ReaderJoin::Missing(stop)] => stop.reason,
            readers => panic!("{readers:?}"),
        },
        fields => panic!("{fields:?}"),
    };

    // The converter receives the key token, not the value token.
    let mut key = keyword_fixture(&reported(), false);
    replace(&mut key, 0x101c, arm64!(at 0x101c; mov w0, w2));
    let key = derive(key);
    assert_eq!(reason(&key), "reader-routing");
    assert!(key.keyword_domains.is_empty());

    // The caller returns without storing the result.
    let unstored = derive(keyword_fixture(&arm64!(at 0x1024; b extern 0x1100), false));
    assert_eq!(reason(&unstored), "keyword-store");
}

#[test]
fn a_pointer_converter_reads_the_stack_word_that_holds_the_value_token() {
    let converter = "EExample TokenToEnum<EExample>(int const&)";
    let pointer = |overwritten: bool| {
        let mut input = keyword_fixture(&[], false);
        let symbol = input.symbols.iter_mut().find(|s| s.address == 0x9000);
        symbol.unwrap().name = converter.into();
        let function = input.functions.iter_mut().find(|f| f.address == 0x9000);
        let function = function.unwrap();
        function.name = converter.into();
        function.code = arm64!(at 0x9000;
            ldr w0, [x0]; // the token
            cmp w0, #8; // token 8
            b.ne extern 0x9014;
            mov w0, #1; // alpha
            ret;
            mov w0, #0; // the fallback
            ret
        );
        let mut code = Arm64::at(0x1010);
        arm64!(code;
            stp x19, x20, [sp, #-32]!;
            mov x19, x0; // the owner
            mov x20, x1; // the reader
            ldr w8, [x20, #0x278]; // the value token
            str w8, [sp, #16]
        );
        if overwritten {
            arm64!(code; stp wzr, wzr, [sp, #16]);
        }
        arm64!(code;
            add x0, sp, #16;
            bl extern 0x9000; // TokenToEnum
            str w0, [x19, #0x40];
            ldp x19, x20, [sp], #32;
            ret
        );
        let root = &mut input.functions[0];
        root.code.truncate(16);
        root.code.extend(code.bytes());
        derive(input)
    };

    let read = pointer(false);
    assert!(
        matches!(&read.fields[0].readers[..], [ReaderJoin::Stored { callee, .. }] if callee == converter),
        "{:?}",
        read.fields[0].readers
    );
    // A pair store replaced the word, so the converter does not read the value.
    let overwritten = pointer(true);
    assert!(
        matches!(overwritten.fields[0].readers[..], [ReaderJoin::Missing(_)]),
        "{:?}",
        overwritten.fields[0].readers
    );
}

#[test]
fn a_boolean_temporary_at_the_stack_pointer_merges_into_its_bit() {
    use crate::{ReaderKind, RepeatBehavior};

    let mut input = fixture();
    input
        .symbols
        .iter_mut()
        .find(|symbol| symbol.address == 0x4000)
        .unwrap()
        .name = "CReader::Read(bool&)".into();
    // CPlanetClass::ReadMember on M452 keeps the temporary at `sp + 0`.
    input.functions[0].code = arm64!(at 0x1000;
        cmp w2, #7; // token 7
        b.eq extern 0x1010;
        add x0, x0, #0x38;
        b extern 0x5000; // CPersistent::ReadMember
        mov x20, x1; // the reader
        mov x19, x0; // the owner
        strb w8, [sp];
        mov x1, sp;
        mov x0, x20;
        bl extern 0x4000; // CReader::Read(bool&)
        ldrb w8, [sp];
        ldr x9, [x19, #0x4a0];
        and x9, x9, #0xfffffffffffffbff; // clear bit 10
        orr x8, x9, x8, lsl #10;
        str x8, [x19, #0x4a0];
        ret
    );
    assert_eq!(
        derive(input).fields[0].readers,
        [ReaderJoin::Stored {
            callee: "CReader::Read(bool&)".into(),
            kind: ReaderKind::Boolean,
            destination: None,
            repeat: RepeatBehavior::Replace,
        }]
    );
}

#[test]
fn an_immediate_reference_read_ends_the_path_only_when_its_item_is_stored() {
    let mut input = fixture();
    input
        .symbols
        .iter_mut()
        .find(|symbol| symbol.address == 0x4000)
        .unwrap()
        .name = "CExampleDatabase::ValueType const* NParserUtil::ReadKeyReference<CExampleDatabase>(CReader&, CExampleDatabase const&, bool)".into();
    let tail_of = |continuation: Vec<u8>| {
        let mut input = input.clone();
        let mut code = arm64!(at 0x1000;
            cmp w2, #7; // token 7
            b.eq extern 0x1010;
            add x0, x0, #0x38;
            b extern 0x5000; // CPersistent::ReadMember
            mov x19, x0; // the owner
            mov x8, x0; // the owner, in a register that the call may overwrite
            mov x0, x1; // the reader
            mov w2, #0;
            bl extern 0x4000 // ReadKeyReference
        );
        code.extend(continuation);
        input.functions[0].code = code;

        match &derive(input).fields[0].readers[..] {
            [ReaderJoin::Joined { tail, .. }] => *tail,
            readers => panic!("{readers:?}"),
        }
    };

    assert!(tail_of(arm64!(at 0x1024;
        str x0, [x19, #0x50];
        b extern 0x102c;
        ldp x29, x30, [sp, #0x10];
        add sp, sp, #0x20;
        ret
    )));
    assert!(!tail_of(arm64!(at 0x1024;
        str x0, [x19, #0x50];
        bl extern 0x4000; // a call after the store
        ret
    )));
    assert!(!tail_of(arm64!(at 0x1024;
        str x0, [sp, #0x50]; // a stack slot, not a member
        ret
    )));
    assert!(!tail_of(arm64!(at 0x1024;
        str x0, [x8, #0x50]; // x8 held the owner before the call, which may overwrite it
        ret
    )));
}
