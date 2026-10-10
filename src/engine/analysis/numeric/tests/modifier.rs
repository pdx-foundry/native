use super::super::*;
use crate::engine::analysis::{assembler::arm64, decode::decode_arm64};

fn member_rows() -> (Vec<Instruction>, BTreeMap<u64, String>) {
    let bytes = arm64!(at 0x4000;
        sub sp,sp,#0xb0;
        stp x28,x27,[sp,#0x50];
        stp x26,x25,[sp,#0x60];
        stp x24,x23,[sp,#0x70];
        stp x22,x21,[sp,#0x80];
        stp x20,x19,[sp,#0x90];
        stp x29,x30,[sp,#0xa0];
        add x29,sp,#0xa0;
        mov x21,x1;
        mov x19,x0;
        cmp w2,#0x1b;
        b.eq extern 0x404c;
        cmp w2,#0xf0;
        b.ne extern 0x4068;
        add x1,x19,#0xa8;
        mov x0,x21;
        bl extern 0x9060;
        mov w20,#1;
        b extern 0x42b4;
        ldr x0,[x19,#0xa0];
        ldr x8,[x0];
        ldr x8,[x8,#0x10];
        mov x1,x21;
        blr x8;
        mov w20,#1;
        b extern 0x42b4;
        adrp x24,extern 0x8000;
        ldr x24,[x24,#0x120];
        ldr w8,[x24,#0x14];
        cmp w8,#1;
        b.lt extern 0x4098;
        ldr x9,[x24,#8];
        ldr w10,[x9,#0x78];
        cmp w10,w2;
        b.ne extern 0x40a0;
        mov x22,#0;
        mov w20,#1;
        b extern 0x40d0;
        mov w20,#0;
        b extern 0x42b4;
        add x9,x9,#0x110;
        mov w11,#1;
        mov x10,x11;
        cmp x8,x11;
        b.eq extern 0x42ac;
        ldr w12,[x9],#0x98;
        add x11,x10,#1;
        cmp w12,w2;
        b.ne extern 0x40a8;
        cmp x10,x8;
        cset w20,lo;
        and x22,x10,#0xffffffff;
        str xzr,[sp,#0x48];
        add x1,sp,#0x48;
        mov x0,x21;
        bl extern 0x9040;
        ldr x8,[x24,#8];
        mov w9,#0x98;
        mul x25,x22,x9;
        add x9,x8,x25;
        ldr w23,[x9,#0x7c];
        ldp w9,w26,[x19,#0x18];
        sxtw x26,w26;
        cmp w26,w9;
        b.ne extern 0x4164;
        add w27,w26,#1;
        scvtf s0,w26;
        fmov s1,#1.50000000;
        fmul s0,s0,s1;
        fcvtzs w8,s0;
        cmp w8,w27;
        csinc w28,w8,w26,gt;
        ubfiz x0,x28,#2,#0x20;
        bl extern 0x9080;
        mov x22,x0;
        lsl x2,x26,#2;
        str w23,[x0,x2];
        ldr x1,[x19,#0x10];
        bl extern 0x90a0;
        mov x23,x19;
        ldr x8,[x23,#8]!;
        str wzr,[x23,#0x14];
        ldr x8,[x8,#0x20];
        mov x0,x23;
        blr x8;
        str x22,[x19,#0x10];
        stp w28,w27,[x23,#0x10];
        ldr x8,[x24,#8];
        b extern 0x41e4;
        ldr x10,[x19,#0x10];
        lsl x11,x26,#2;
        add x9,x10,x11;
        cmp x9,x9;
        str w23,[x9];
        add w12,w26,#1;
        str w12,[x19,#0x1c];
        b.eq extern 0x41e4;
        add x10,x11,x10;
        sub x10,x10,#4;
        mov x11,x9;
        cmp x11,x10;
        b.eq extern 0x41b8;
        ldr w12,[x11];
        ldr w13,[x10];
        mov x14,x11;
        str w13,[x14],#4;
        str w12,[x10],#-4;
        cmp x11,x10;
        mov x11,x14;
        b.ne extern 0x4190;
        mov x10,x9;
        cmp x10,x9;
        b.eq extern 0x41e4;
        ldr w11,[x10];
        ldr w12,[x9];
        mov x13,x10;
        str w12,[x13],#4;
        str w11,[x9],#-4;
        cmp x10,x9;
        mov x10,x13;
        b.ne extern 0x41bc;
        add x0,x19,#0x30;
        add x8,x8,x25;
        ldr w8,[x8,#0x7c];
        str w8,[sp,#0x10];
        ldr w1,[x19,#0x44];
        add x22,sp,#0x10;
        add x2,sp,#0x10;
        add x3,sp,#0x48;
        bl extern 0x9020;
        ldr x8,[x24,#8];
        add x9,x8,x25;
        ldr w9,[x9,#0x84];
        ldr w10,[x19,#0xac];
        tst w10,w9;
        b.ne extern 0x428c;
        adrp x1,extern 0x8000;
        add x1,x1,#0x1c0;
        add x0,sp,#0x38;
        mov w2,#0x1e1;
        mov w3,#0x1000;
        bl extern 0x90c0;
        add x8,sp,#0x10;
        mov x0,x21;
        bl extern 0x90e0;
        ldrsb w8,[sp,#0x27];
        ldr x9,[sp,#0x10];
        cmp w8,#0;
        csel x8,x9,x22,lt;
        str x8,[sp];
        adrp x1,extern 0x8000;
        add x1,x1,#0x220;
        add x0,sp,#0x38;
        bl extern 0x9100;
        ldrsb w8,[sp,#0x27];
        tbz w8,#0x1f,extern 0x4288;
        add x8,sp,#0x10;
        add x0,x8,#0x18;
        ldr x1,[sp,#0x10];
        ldr x8,[sp,#0x20];
        and x2,x8,#0x7fffffffffffffff;
        bl extern 0x9120;
        ldr x8,[x24,#8];
        strb wzr,[x19,#0xb0];
        add x8,x8,x25;
        ldr w9,[x8,#0x90];
        mov w10,#0x100;
        movk w10,#0x2000,lsl #16;
        orr w9,w9,w10;
        str w9,[x8,#0x90];
        b extern 0x42b4;
        cmp x10,x8;
        cset w20,lo;
        mov x0,x20;
        ldp x29,x30,[sp,#0xa0];
        ldp x20,x19,[sp,#0x90];
        ldp x22,x21,[sp,#0x80];
        ldp x24,x23,[sp,#0x70];
        ldp x26,x25,[sp,#0x60];
        ldp x28,x27,[sp,#0x50];
        add sp,sp,#0xb0;
        ret;
        mov x19,x0;
        ldrsb w8,[sp,#0x27];
        tbz w8,#0x1f,extern 0x42ec;
        add x0,sp,#0x10;
        bl extern 0x9000;
        mov x0,x19;
        bl extern 0x9140
    );
    (
        decode_arm64(&bytes, 0x4000).unwrap(),
        BTreeMap::from([
            (0x9060, "other_integer_reader".into()),
            (0x8120, "definitions".into()),
            (0x9040, "numeric_reader".into()),
            (0x9080, "allocate_array".into()),
            (0x90a0, "copy_bytes".into()),
            (0x9020, "insert_entry".into()),
            (0x81c0, "\"diagnostic6\"".into()),
            (0x90c0, "diagnostic_location".into()),
            (0x90e0, "reader_location".into()),
            (0x8220, "\"diagnostic9\"".into()),
            (0x9100, "report_category".into()),
            (0x9120, "deallocate".into()),
            (0x9000, "cold_cleanup".into()),
            (0x9140, "unwind".into()),
        ]),
    )
}

fn insert_rows() -> (Vec<Instruction>, BTreeMap<u64, String>) {
    let bytes = arm64!(at 0x6000;
        sub sp,sp,#0x80;
        stp x28,x27,[sp,#0x20];
        stp x26,x25,[sp,#0x30];
        stp x24,x23,[sp,#0x40];
        stp x22,x21,[sp,#0x50];
        stp x20,x19,[sp,#0x60];
        stp x29,x30,[sp,#0x70];
        add x29,sp,#0x70;
        mov x21,x3;
        mov x22,x2;
        mov x20,x1;
        mov x19,x0;
        ldp w8,w25,[x0,#0x10];
        sxtw x25,w25;
        cmp w25,w8;
        b.ne extern 0x60c8;
        add w26,w25,#1;
        scvtf s0,w25;
        fmov s1,#1.50000000;
        fmul s0,s0,s1;
        fcvtzs w8,s0;
        cmp w8,w26;
        csinc w27,w8,w25,gt;
        ubfiz x0,x27,#4,#0x20;
        bl extern 0x9080;
        mov x23,x0;
        sxtw x24,w20;
        sbfiz x2,x20,#4,#0x20;
        add x28,x0,x2;
        ldr w8,[x22];
        ldr x9,[x21];
        str x9,[x28];
        stp w8,wzr,[x28,#8];
        ldr x20,[x19,#8];
        add x21,x20,x2;
        mov x1,x20;
        bl extern 0x90a0;
        add x8,x20,x25,lsl #4;
        add x0,x28,#0x10;
        sub x2,x8,x21;
        mov x1,x21;
        bl extern 0x90a0;
        str wzr,[x19,#0x14];
        ldr x8,[x19];
        ldr x8,[x8,#0x20];
        mov x0,x19;
        blr x8;
        str x23,[x19,#8];
        stp w27,w26,[x19,#0x10];
        b extern 0x6170;
        ldr x8,[x19,#8];
        add x8,x8,x25,lsl #4;
        ldr w9,[x22];
        ldr x10,[x21];
        str x10,[x8];
        stp w9,wzr,[x8,#8];
        ldrsw x9,[x19,#0x14];
        add w10,w9,#1;
        str w10,[x19,#0x14];
        sxtw x24,w20;
        sbfiz x8,x20,#4,#0x20;
        lsl x11,x9,#4;
        cmp x8,x11;
        b.eq extern 0x6170;
        ldr x12,[x19,#8];
        add x9,x12,x8;
        add x8,x12,x11;
        add x11,x11,x12;
        sub x11,x11,#0x10;
        mov x12,x9;
        cmp x12,x11;
        b.eq extern 0x6140;
        ldr q0,[x12];
        ldr q1,[x11];
        str q1,[x12];
        add x13,x12,#0x10;
        str q0,[x11],#-16;
        cmp x12,x11;
        mov x12,x13;
        b.ne extern 0x6118;
        cmp w10,w20;
        b.eq extern 0x6170;
        cmp x9,x8;
        b.eq extern 0x6170;
        ldr q0,[x9];
        ldr q1,[x8];
        str q1,[x9];
        add x10,x9,#0x10;
        str q0,[x8],#-16;
        cmp x9,x8;
        mov x9,x10;
        b.ne extern 0x6148;
        ldr x8,[x19,#8];
        add x0,x8,x24,lsl #4;
        ldp x29,x30,[sp,#0x70];
        ldp x20,x19,[sp,#0x60];
        ldp x22,x21,[sp,#0x50];
        ldp x24,x23,[sp,#0x40];
        ldp x26,x25,[sp,#0x30];
        ldp x28,x27,[sp,#0x20];
        add sp,sp,#0x80;
        ret
    );
    (
        decode_arm64(&bytes, 0x6000).unwrap(),
        BTreeMap::from([
            (0x9080, "allocate_array".into()),
            (0x90a0, "copy_bytes".into()),
        ]),
    )
}

fn input() -> ModifierInput {
    let (member, mut names) = member_rows();
    let (insert, insertion_names) = insert_rows();
    names.extend(insertion_names);
    ModifierInput {
        member,
        insert,
        names,
        shared_callee: "shared-conversion".into(),
    }
}

fn readers() -> BTreeMap<String, NumericReader> {
    [(
        "shared-conversion".into(),
        NumericReader {
            boundary: Vec::new(),
            conversion: GrammarProperty::Partial(Some(NumericConversion::default())),
            gaps: Vec::new(),
        },
    )]
    .into()
}

#[test]
fn numeric_entry_uses_the_shared_reader_result_on_both_capacity_paths() {
    let proof = super::super::modifier::analyze(&input(), &readers()).unwrap();
    assert_eq!(proof.shared_callee, "shared-conversion");
    assert_eq!(
        proof.reader_id,
        crate::ReaderId::from_callee("shared-conversion")
    );
    assert_eq!(proof.storage_width_bits, 64);
}

#[test]
fn a_wrong_read_receiver_or_destination_breaks_the_modifier_join() {
    for (operation, operands, replacement) in [
        ("mov", "x0,x21", "x0,x19"),
        ("add", "x1,sp,#0x48", "x1,sp,#0x40"),
        ("add", "x3,sp,#0x48", "x3,sp,#0x40"),
    ] {
        let mut input = input();
        for row in &mut input.member {
            if row.operation == operation && row.operands == operands {
                row.operands = replacement.into();
            }
        }
        assert!(super::super::modifier::analyze(&input, &readers()).is_err());
    }
}

#[test]
fn unknown_callee_and_changed_append_index_remain_typed_gaps() {
    let mut input = input();
    let address = *input
        .names
        .iter()
        .find(|(_, name)| *name == "numeric_reader")
        .unwrap()
        .0;
    input.names.insert(address, "other-conversion".into());
    assert_eq!(
        super::super::modifier::analyze(&input, &readers())
            .unwrap_err()
            .reason,
        "modifier-numeric-member-flow"
    );

    let mut input = self::input();
    let index = input
        .member
        .iter_mut()
        .find(|row| row.operands == "w1,[x19,#0x44]")
        .unwrap();
    index.operands = "w1,[x19,#0x48]".into();
    assert_eq!(
        super::super::modifier::analyze(&input, &readers())
            .unwrap_err()
            .reason,
        "modifier-numeric-append-index"
    );
}

#[test]
fn changing_either_capacity_paths_numeric_store_fails_the_boundary_proof() {
    for operands in ["x9,[x28]", "x10,[x8]"] {
        let mut input = input();
        let store = input
            .insert
            .iter_mut()
            .find(|row| row.operation == "str" && row.operands == operands)
            .unwrap();
        store.operands = "xzr,[x8]".into();
        assert_eq!(
            super::super::modifier::analyze(&input, &readers())
                .unwrap_err()
                .reason,
            "modifier-numeric-entry-store"
        );
    }
    assert_eq!(
        super::super::modifier::analyze(&input(), &BTreeMap::new())
            .unwrap_err()
            .reason,
        "modifier-numeric-shared-reader"
    );
}

#[test]
fn inherited_tokens_come_from_the_proven_member_shape() {
    assert_eq!(
        super::super::modifier::member_tokens(&input()).unwrap(),
        super::super::modifier::MemberTokens {
            name: 27,
            data: 240
        }
    );
    let mut input = input();
    let comparison = input
        .member
        .iter_mut()
        .find(|row| row.operation == "cmp" && row.operands == "w2,#0x1b")
        .unwrap();
    comparison.operands = "w2,#0x1d".into();
    assert_eq!(
        super::super::modifier::member_tokens(&input).unwrap(),
        super::super::modifier::MemberTokens {
            name: 29,
            data: 240
        }
    );
    let comparison = input
        .member
        .iter_mut()
        .find(|row| row.operation == "cmp" && row.operands == "w2,#0x1d")
        .unwrap();
    comparison.operands = "w1,#0x1d".into();
    assert!(super::super::modifier::member_tokens(&input).is_err());
}
