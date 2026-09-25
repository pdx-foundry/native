//! Authored ARM64 code and a minimal Mach-O image around it, for Native's unit tests.
#![allow(dead_code)]
use crate::engine::analysis::assembler::arm64;

// An authored ARM64 sequence at 0x1000; it is not game code.
pub fn sample_arm64_code() -> Vec<u8> {
    arm64!(at 0x1000;
        stp x29, x30, [sp, #-16]!;
        mov x29, sp;
        add w0, w0, #32;
        bl extern 0x1040;
        adrp x8, extern 0x2000;
        ldr x8, [x8, #16];
        ldr x8, [x8];
        cmp x0, #0;
        csel x0, x8, x0, ne;
        ldp x29, x30, [sp], #16;
        ret
    )
}

/// The decoder's mnemonic and operand text for each instruction of [`sample_arm64_code`].
pub fn sample_arm64_disassembly() -> Vec<(&'static str, &'static str)> {
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
pub fn macho_with_text(code: &[u8]) -> Vec<u8> {
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

/// Where `macho_with_fixups` puts its chained-fixups header, so a test can change a field.
pub const IMAGE_FIXUPS_OFFSET: usize = 0x5000;

/// The halfword jump-table entries in `macho_with_fixups`'s read-only data.
pub const IMAGE_JUMP_TABLE: [u8; 6] = [2, 0, 0, 0, 6, 0];

/// An ARM64 Mach-O executable with symbols, strings and one chained pointer, whose segment uses
/// `pointer_format`. Format 6 is the one Native reads.
///
/// `Probe::Read()` at 0x100001000 forms the string `entity_offset` at 0x100002008, calls
/// `_helper` at 0x100001020 and loads the data slot at 0x100004000, which points to `_helper`.
/// `__TEXT,__const` at 0x100003000 holds [`IMAGE_JUMP_TABLE`].
pub fn macho_with_fixups(pointer_format: u16) -> Vec<u8> {
    const BASE: u64 = 0x1_0000_0000;
    // File offsets. The image maps each one at `BASE` plus the offset.
    const TEXT: usize = 0x1000;
    const HELPER: usize = TEXT + 0x20;
    const CSTRING: usize = 0x2000;
    const CONST: usize = 0x3000;
    const DATA: usize = 0x4000;
    const SYMBOLS: usize = 0x5100;
    const NAMES: usize = 0x5200;
    let address = |offset: usize| BASE + offset as u64;

    let code = arm64!(at address(TEXT);
        stp x29, x30, [sp, #-16]!;
        adrp x0, extern address(CSTRING) as usize;
        add x0, x0, #8; // "entity_offset"
        bl extern address(HELPER) as usize;
        adrp x8, extern address(DATA) as usize;
        ldr x8, [x8]; // the data slot
        ldp x29, x30, [sp], #16;
        ret;
        ret // _helper
    );
    let strings = b"other\0\0\0entity_offset\0";
    let names = b"\0__ZN5Probe4ReadEv\0_helper\0";

    let mut bytes = vec![0u8; 0x5300];
    let mut commands = Vec::new();
    commands.extend(segment(
        b"__TEXT",
        BASE,
        DATA as u64,
        0,
        &[
            (b"__text", address(TEXT), code.len(), TEXT, 0x8000_0400),
            (b"__cstring", address(CSTRING), strings.len(), CSTRING, 0x2),
            (b"__const", address(CONST), IMAGE_JUMP_TABLE.len(), CONST, 0),
        ],
    ));
    commands.extend(segment(
        b"__DATA",
        address(DATA),
        0x1000,
        DATA,
        &[(b"__data", address(DATA), 16, DATA, 0)],
    ));
    for value in [0x2, 24, SYMBOLS, 2, NAMES, names.len()] {
        commands.extend((value as u32).to_le_bytes()); // LC_SYMTAB
    }
    for value in [0x8000_0034u32, 16, IMAGE_FIXUPS_OFFSET as u32, 0x60] {
        commands.extend(value.to_le_bytes()); // LC_DYLD_CHAINED_FIXUPS
    }

    let header: Vec<u8> = [
        0xfeedfacfu32,
        0x0100000c,
        0,
        2,
        4,
        commands.len() as u32,
        0,
        0,
    ]
    .into_iter()
    .flat_map(u32::to_le_bytes)
    .collect();
    put(&mut bytes, 0, &header);
    put(&mut bytes, 32, &commands);
    put(&mut bytes, TEXT, &code);
    put(&mut bytes, CSTRING, strings);
    put(&mut bytes, CONST, &IMAGE_JUMP_TABLE);
    // A DYLD_CHAINED_PTR_64_OFFSET rebase to `_helper`, the last in its chain.
    put(&mut bytes, DATA, &(HELPER as u64).to_le_bytes());

    // dyld_chained_fixups_header: version, starts, imports, symbols, import count, formats.
    let mut fixups: Vec<u8> = [0u32, 32, 0x60, 0x60, 0, 3, 0, 0]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    // dyld_chained_starts_in_image: two segments; only __DATA has chains, at +12.
    for value in [2u32, 0, 12] {
        fixups.extend(value.to_le_bytes());
    }
    // dyld_chained_starts_in_segment: one 0x1000 page whose chain starts at offset 0.
    fixups.extend(24u32.to_le_bytes());
    fixups.extend(0x1000u16.to_le_bytes());
    fixups.extend(pointer_format.to_le_bytes());
    fixups.extend((DATA as u64).to_le_bytes());
    fixups.extend(0u32.to_le_bytes());
    fixups.extend(1u16.to_le_bytes());
    fixups.extend(0u16.to_le_bytes());
    put(&mut bytes, IMAGE_FIXUPS_OFFSET, &fixups);

    // nlist_64 entries: name offset, N_SECT | N_EXT, section 1, no description, address.
    for (index, (name, symbol_address)) in [(1u32, address(TEXT)), (19, address(HELPER))]
        .into_iter()
        .enumerate()
    {
        let mut symbol = name.to_le_bytes().to_vec();
        symbol.extend([0x0f, 1, 0, 0]);
        symbol.extend(symbol_address.to_le_bytes());
        put(&mut bytes, SYMBOLS + index * 16, &symbol);
    }
    put(&mut bytes, NAMES, names);

    bytes
}

/// An `LC_SEGMENT_64` command with its sections: name, address, size, file offset and flags.
fn segment(
    name: &[u8],
    address: u64,
    size: u64,
    offset: usize,
    sections: &[(&[u8], u64, usize, usize, u32)],
) -> Vec<u8> {
    let mut command = Vec::new();
    command.extend(0x19u32.to_le_bytes());
    command.extend((72 + 80 * sections.len() as u32).to_le_bytes());
    command.extend(fixed_name(name));
    for value in [address, size, offset as u64, size] {
        command.extend(value.to_le_bytes());
    }
    for value in [3u32, 3, sections.len() as u32, 0] {
        command.extend(value.to_le_bytes());
    }

    for (section, address, size, offset, flags) in sections {
        command.extend(fixed_name(section));
        command.extend(fixed_name(name));
        command.extend(address.to_le_bytes());
        command.extend((*size as u64).to_le_bytes());
        for value in [*offset as u32, 2, 0, 0, *flags, 0, 0, 0] {
            command.extend(value.to_le_bytes());
        }
    }

    command
}

fn fixed_name(name: &[u8]) -> [u8; 16] {
    let mut fixed = [0; 16];
    fixed[..name.len()].copy_from_slice(name);
    fixed
}

fn put(bytes: &mut [u8], at: usize, data: &[u8]) {
    bytes[at..at + data.len()].copy_from_slice(data);
}
