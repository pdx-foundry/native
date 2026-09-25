//! ARM64 instruction decoding. Every byte of a range is accounted for, or the range is an
//! error.
use capstone::prelude::*;
use serde::{Deserialize, Serialize};

/// One fully decoded ARM64 instruction. This establishes no higher-level reader semantics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Instruction {
    /// Unsigned virtual address in the selected executable slice.
    pub address: u64,
    /// Exact little-endian instruction bytes.
    pub bytes: [u8; 4],
    /// Lowercase mnemonic from the pinned decoder, including condition suffixes and aliases.
    pub operation: String,
    /// Pinned Capstone operand syntax with whitespace removed. Register widths, signs,
    /// conditions, writeback, and absolute branch destinations are preserved.
    pub operands: String,
}

/// A range cannot be decoded completely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError(pub String);

impl std::fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}
impl std::error::Error for DecodeError {}

/// The most bytes that the decoder receives at once. Capstone holds every instruction of one call
/// in memory, so a long range is decoded in parts of this size.
const PART_BYTES: usize = 4096;

thread_local! {
    /// The pinned decoder, built once for each thread because a Capstone handle cannot move
    /// between threads.
    static DECODER: Result<Capstone, DecodeError> = Capstone::new()
        .arm64()
        .mode(arch::arm64::ArchMode::Arm)
        .build()
        .map_err(|error| DecodeError(error.to_string()));
}

/// Decode a nonempty, aligned ARM64 range of any length. Unknown instructions, trailing bytes,
/// and address overflow are errors; no bytes are skipped or invented.
pub fn decode_arm64(bytes: &[u8], address: u64) -> Result<Vec<Instruction>, DecodeError> {
    if bytes.is_empty()
        || !bytes.len().is_multiple_of(4)
        || !address.is_multiple_of(4)
        || address.checked_add(bytes.len() as u64).is_none()
    {
        return Err(DecodeError("Invalid ARM64 instruction range".into()));
    }
    DECODER.with(|decoder| {
        let decoder = decoder.as_ref().map_err(Clone::clone)?;
        let mut rows = Vec::with_capacity(bytes.len() / 4);
        for (index, part) in bytes.chunks(PART_BYTES).enumerate() {
            let decoded = decoder
                .disasm_all(part, address + (index * PART_BYTES) as u64)
                .map_err(|error| DecodeError(error.to_string()))?;
            if decoded.len() * 4 != part.len() {
                return Err(DecodeError(
                    "Decoder did not consume the complete range".into(),
                ));
            }
            for instruction in decoded.iter() {
                if instruction.address() != address + rows.len() as u64 * 4 {
                    return Err(DecodeError("Noncontiguous decoded instruction".into()));
                }
                rows.push(Instruction {
                    address: instruction.address(),
                    bytes: instruction
                        .bytes()
                        .try_into()
                        .map_err(|_| DecodeError("Invalid ARM64 instruction size".into()))?,
                    operation: instruction
                        .mnemonic()
                        .ok_or_else(|| DecodeError("Missing instruction mnemonic".into()))?
                        .to_ascii_lowercase(),
                    operands: instruction
                        .op_str()
                        .unwrap_or_default()
                        .chars()
                        .filter(|character| !character.is_ascii_whitespace())
                        .collect::<String>()
                        .to_ascii_lowercase(),
                });
            }
        }
        Ok(rows)
    })
}

/// The destination register and page of an `adrp` word at `address`.
pub fn adrp(word: u32, address: u64) -> Option<(usize, u64)> {
    if word & 0x9f00_0000 != 0x9000_0000 {
        return None;
    }
    let immediate = (word >> 29 & 0b11) | (word >> 5 & 0x7_ffff) << 2;
    let pages = ((immediate << 11) as i32 >> 11) as i64;
    Some((
        (word & 0x1f) as usize,
        (address & !0xfff).wrapping_add_signed(pages << 12),
    ))
}

/// The destination register, source register and addend of a 64-bit `add` of an immediate,
/// with its optional `lsl #12`. Register 31 is the stack pointer.
pub fn add_immediate(word: u32) -> Option<(usize, usize, u64)> {
    if word & 0xff80_0000 != 0x9100_0000 {
        return None;
    }
    let shift = if word & 0x0040_0000 != 0 { 12 } else { 0 };
    Some((
        (word & 0x1f) as usize,
        (word >> 5 & 0x1f) as usize,
        u64::from(word >> 10 & 0xfff) << shift,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pinned decoder's reading of one word.
    fn decoded(word: u32, address: u64) -> (String, String) {
        let row = decode_arm64(&word.to_le_bytes(), address)
            .unwrap()
            .remove(0);
        (row.operation, row.operands)
    }

    #[test]
    fn adrp_gives_the_page_forward_and_backward() {
        assert_eq!(
            decoded(0x9000_0028, 0x1004),
            ("adrp".into(), "x8,#0x5000".into())
        );
        assert_eq!(adrp(0x9000_0028, 0x1004), Some((8, 0x5000)));
        assert_eq!(
            decoded(0x90ff_ffe1, 0x5000),
            ("adrp".into(), "x1,#0x1000".into())
        );
        assert_eq!(adrp(0x90ff_ffe1, 0x5000), Some((1, 0x1000)));
        assert_eq!(adrp(0x9100_4108, 0x1000), None);
    }

    #[test]
    fn add_immediate_reads_the_shift_and_refuses_add_with_tags() {
        assert_eq!(decoded(0x9100_4108, 0).0, "add");
        assert_eq!(add_immediate(0x9100_4108), Some((8, 8, 0x10)));
        assert_eq!(decoded(0x9140_07e0, 0).0, "add");
        assert_eq!(add_immediate(0x9140_07e0), Some((0, 31, 0x1000)));
        assert_eq!(decoded(0x9181_0020, 0).0, "addg");
        assert_eq!(add_immediate(0x9181_0020), None);
    }
}
