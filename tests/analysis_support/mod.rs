//! Authored ARM64 code and a minimal Mach-O image around it. Native's unit tests and its
//! integration tests share this file.
#![allow(dead_code)]

// An authored ARM64 sequence at 0x1000; it is not game code.
pub fn code() -> Vec<u8> {
    [
        0xa9bf7bfdu32, // stp x29, x30, [sp, #-16]!
        0x910003fd,    // mov x29, sp
        0x11008000,    // add w0, w0, #32
        0x9400000d,    // bl 0x1040
        0xb0000008,    // adrp x8, 0x2000
        0xf9400908,    // ldr x8, [x8, #16]
        0xf9400108,    // ldr x8, [x8]
        0xf100001f,    // cmp x0, #0
        0x9a801100,    // csel x0, x8, x0, ne
        0xa8c17bfd,    // ldp x29, x30, [sp], #16
        0xd65f03c0,    // ret
    ]
    .into_iter()
    .flat_map(u32::to_le_bytes)
    .collect()
}

pub fn expected() -> Vec<(&'static str, &'static str)> {
    vec![
        ("stp", "x29,x30,[sp,#-0x10]!"),
        ("mov", "x29,sp"),
        ("add", "w0,w0,#0x20"),
        ("bl", "#0x1040"),
        ("adrp", "x8,#0x2000"),
        ("ldr", "x8,[x8,#0x10]"),
        ("ldr", "x8,[x8]"),
        ("cmp", "x0,#0"),
        ("csel", "x0,x8,x0,ne"),
        ("ldp", "x29,x30,[sp],#0x10"),
        ("ret", ""),
    ]
}

/// A 64-bit ARM64 Mach-O executable with one `__text` section at 0x1000 that holds `code`.
pub fn macho(code: &[u8]) -> Vec<u8> {
    let length = code.len() as u64;
    let mut bytes: Vec<u8> = [0xfeedfacfu32, 0x0100000c, 0, 2, 1, 152, 0, 0]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    bytes.extend(0x19u32.to_le_bytes()); // LC_SEGMENT_64
    bytes.extend(152u32.to_le_bytes());
    bytes.extend(b"__TEXT\0\0\0\0\0\0\0\0\0\0");
    for value in [0x1000u64, length, 184, length] {
        bytes.extend(value.to_le_bytes());
    }
    for value in [5u32, 5, 1, 0] {
        bytes.extend(value.to_le_bytes());
    }
    bytes.extend(b"__text\0\0\0\0\0\0\0\0\0\0");
    bytes.extend(b"__TEXT\0\0\0\0\0\0\0\0\0\0");
    bytes.extend(0x1000u64.to_le_bytes());
    bytes.extend(length.to_le_bytes());
    for value in [184u32, 2, 0, 0, 0x80000400, 0, 0, 0] {
        bytes.extend(value.to_le_bytes());
    }
    assert_eq!(bytes.len(), 184);
    bytes.extend(code);
    bytes
}
