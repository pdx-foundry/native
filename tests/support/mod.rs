// Authored headers contain no game bytes or executable code.
pub fn macho(cpu: u32) -> Vec<u8> {
    [0xfeedfacfu32, cpu, 0, 2, 0, 0, 0, 0]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect()
}

pub fn fat(slices: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend(0xcafebabeu32.to_be_bytes());
    bytes.extend((slices.len() as u32).to_be_bytes());
    let mut offset = 8 + slices.len() as u32 * 20;
    for (cpu, slice) in slices {
        for value in [*cpu, 0, offset, slice.len() as u32, 0] {
            bytes.extend(value.to_be_bytes());
        }
        offset += slice.len() as u32;
    }
    for (_, slice) in slices {
        bytes.extend(slice);
    }
    bytes
}

pub fn pe() -> Vec<u8> {
    let mut bytes = vec![0; 512];
    bytes[..2].copy_from_slice(b"MZ");
    bytes[60..64].copy_from_slice(&128u32.to_le_bytes());
    bytes[128..132].copy_from_slice(b"PE\0\0");
    bytes[132..134].copy_from_slice(&0x8664u16.to_le_bytes());
    bytes[148..150].copy_from_slice(&240u16.to_le_bytes());
    bytes[150..152].copy_from_slice(&0x22u16.to_le_bytes());
    bytes[152..154].copy_from_slice(&0x20bu16.to_le_bytes());
    bytes[212..216].copy_from_slice(&512u32.to_le_bytes());
    bytes[260..264].copy_from_slice(&16u32.to_le_bytes());
    bytes
}
