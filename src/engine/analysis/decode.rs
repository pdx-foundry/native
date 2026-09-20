//! Bounded ARM64 instruction decoding. Every byte of a range is accounted for, or the range is
//! an error.
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

/// Decode a nonempty, aligned ARM64 range of at most 4096 bytes. Unknown instructions,
/// trailing bytes, and address overflow are errors; no bytes are skipped or invented.
pub fn decode_arm64(bytes: &[u8], address: u64) -> Result<Vec<Instruction>, DecodeError> {
    if bytes.is_empty()
        || bytes.len() > 4096
        || !bytes.len().is_multiple_of(4)
        || !address.is_multiple_of(4)
        || address.checked_add(bytes.len() as u64).is_none()
    {
        return Err(DecodeError(
            "Invalid bounded ARM64 instruction range".into(),
        ));
    }
    let decoder = Capstone::new()
        .arm64()
        .mode(arch::arm64::ArchMode::Arm)
        .build()
        .map_err(|error| DecodeError(error.to_string()))?;
    let decoded = decoder
        .disasm_all(bytes, address)
        .map_err(|error| DecodeError(error.to_string()))?;
    if decoded.len() * 4 != bytes.len() {
        return Err(DecodeError(
            "Decoder did not consume the complete range".into(),
        ));
    }
    decoded
        .iter()
        .enumerate()
        .map(|(index, instruction)| {
            if instruction.address() != address + index as u64 * 4 {
                return Err(DecodeError("Noncontiguous decoded instruction".into()));
            }
            Ok(Instruction {
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
            })
        })
        .collect()
}
