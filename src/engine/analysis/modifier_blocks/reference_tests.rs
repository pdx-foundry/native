use super::*;
use crate::engine::analysis::{assembler::arm64, decode::decode_arm64};

fn input() -> Input {
    let bytes = arm64!(at 0x1000;
        sub sp,sp,#0xa0;
        stp x28,x27,[sp,#0x40];
        stp x26,x25,[sp,#0x50];
        stp x24,x23,[sp,#0x60];
        stp x22,x21,[sp,#0x70];
        stp x20,x19,[sp,#0x80];
        stp x29,x30,[sp,#0x90];
        add x29,sp,#0x90;
        mov x21,x2;
        mov x20,x1;
        mov x19,x0;
        adrp x23,extern 0x8000;
        ldr x8,[x23,#0x100];
        cbz x8,extern 0x10b8;
        ldr x1,[x20,#0x48];
        mov x0,sp;
        bl extern 0x9000;
        add x0,x20,#0x278;
        bl extern 0x9100;
        mov x22,x0;
        ldr x8,[x23,#0x100];
        ldr w26,[x8,#0x3dc];
        cmp w26,#0x1;
        b.lt extern 0x1154;
        ldr x27,[x8,#0x3d0];
        ldrb w8,[sp,#0x17];
        sxtb w9,w8;
        cmp w9,#0x0;
        ldp x10,x9,[sp];
        csel x24,x9,x8,lt;
        mov x8,sp;
        csel x25,x10,x8,lt;
        cbz x24,extern 0x1098;
        mov x28,#0x0;
        b extern 0x1104;
        add x27,x27,#0x8;
        subs x26,x26,#0x1;
        b.eq extern 0x1154;
        ldr x23,[x27];
        ldrb w8,[x23,#0x16f];
        sxtb w9,w8;
        ldr x10,[x23,#0x160];
        cmp w9,#0x0;
        csel x8,x10,x8,lt;
        cbnz x8,extern 0x108c;
        b extern 0x1160;
        mov x0,x19;
        mov x1,x20;
        mov x2,x21;
        ldp x29,x30,[sp,#0x90];
        ldp x20,x19,[sp,#0x80];
        ldp x22,x21,[sp,#0x70];
        ldp x24,x23,[sp,#0x60];
        ldp x26,x25,[sp,#0x50];
        ldp x28,x27,[sp,#0x40];
        add sp,sp,#0xa0;
        b extern 0x9200;
        ldr x0,[x9];
        mov x1,x25;
        mov x2,x24;
        bl extern 0x9300;
        cbz w0,extern 0x1160;
        add x28,x28,#0x1;
        cmp x28,x26;
        b.eq extern 0x1154;
        ldr x23,[x27,x28,lsl #3];
        ldrb w8,[x23,#0x16f];
        sxtb w9,w8;
        ldr x10,[x23,#0x160];
        cmp w9,#0x0;
        csel x9,x10,x8,lt;
        cmp x9,x24;
        b.ne extern 0x10f8;
        add x9,x23,#0x158;
        tbnz w8,#7,extern 0x10e4;
        mov x10,x25;
        ldrb w11,[x9];
        ldrb w12,[x10];
        cmp w11,w12;
        b.ne extern 0x10f8;
        add x9,x9,#0x1;
        add x10,x10,#0x1;
        subs x8,x8,#0x1;
        b.ne extern 0x1130;
        b extern 0x1160;
        adrp x8,extern 0x8000;
        ldr x8,[x8,#0x200];
        ldr x23,[x8];
        ldr x8,[x23];
        ldr x8,[x8,#0x98];
        mov x0,x23;
        blr x8;
        cbz w0,extern 0x1194;
        ldrb w8,[x23,#0x1a8];
        tbnz w8,#1,extern 0x11ec;
        ldr x8,[x19];
        ldr x8,[x8,#0xc0];
        mov x0,x19;
        mov x1,x20;
        blr x8;
        b extern 0x126c;
        mov x0,x19;
        mov x1,x20;
        mov x2,x21;
        bl extern 0x9200;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        nop;
        ret
    );
    Input {
        member: decode_arm64(&bytes, 0x1000).unwrap(),
        conversion: decode_arm64(
            &arm64!(at 0x9100; ldr x0, [x0, #0x10]; b extern 0x9400),
            0x9100,
        )
        .unwrap(),
        names: BTreeMap::from([
            (0x8100, "database".into()),
            (0x8200, "null_item".into()),
            (0x9000, "key_string".into()),
            (0x9100, "fixed_value".into()),
            (0x9200, "base_member".into()),
            (0x9300, "compare_bytes".into()),
            (0x9400, "fixed_conversion".into()),
        ]),
        key_text_offset: 0x48,
        value_token_offset: 0x278,
        token_text_offset: 0x10,
    }
}

#[test]
fn reference_requires_database_key_lookup_value_conversion_and_fallback() {
    assert!(analyze(&input()).is_ok());
    for address in [0x8100, 0x8200, 0x9000, 0x9100, 0x9200, 0x9300, 0x9400] {
        let mut input = input();
        input.names.remove(&address);
        assert_eq!(
            analyze(&input).unwrap_err().reason,
            "modifier-reference-flow",
            "{address:#x}"
        );
    }
}

#[test]
fn lookup_of_a_different_string_and_changed_hit_miss_routes_fail() {
    for (index, operands) in [
        (14, "x1,[x19,#0x48]"), // Owner text is not the reader key.
        (17, "x0,x20,#0x38"),   // Key token is not the value token.
        (61, "w0,#0x1154"),     // Equality must lead to the found item.
        (92, "w0,#0x1174"),     // Miss must lead to the base reader.
        (103, "x2,x19"),        // Fallback must retain the input token.
    ] {
        let mut input = input();
        input.member[index].operands = operands.into();
        assert!(analyze(&input).is_err(), "{index}");
    }
}

#[test]
fn every_required_lookup_instruction_is_checked_but_later_cleanup_is_not() {
    for index in (8..49).chain(57..105) {
        let mut input = input();
        input.member[index].operation = "nop".into();
        input.member[index].operands.clear();
        assert!(analyze(&input).is_err(), "instruction {index}");
    }
    let mut input = input();
    input.member[120].operation = "ret".into();
    assert!(analyze(&input).is_ok());
    input.conversion[0].operands = "x0,[x1,#0x10]".into();
    assert!(analyze(&input).is_err());
}
