//! Bounded instruction decoding and replay from recorded bytes. No installation or process access.

use capstone::prelude::*;
use serde::{Deserialize, Serialize};

use evidence::{ArtifactReference, CaptureOrigin};

/// Exact algorithm used by the initial bounded decode control.
pub const METHOD: &str = "static-decode-control/v1";
/// Pinned decoder and Native's textual operand normalization revision.
pub const DECODER: &str = "capstone-0.14.0/arm64-normalization-v1";
/// Recorded descriptor format; revisions require an explicit replay implementation.
pub const FORMAT: &str = "pdx-native-analysis/v1";

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

/// Identities attached to the original operation; replay does not grant fresh qualification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisProvenance {
    /// Exact executable SHA-256.
    pub executable: String,
    /// Exact selected slice SHA-256.
    pub slice: String,
    /// Opaque identity of the static operation composition.
    pub composition: String,
    /// Native analysis method revision.
    pub method: String,
    /// Decoder and normalization revision.
    pub decoder: String,
    /// Static implementation source/dependency fingerprint.
    pub implementation: String,
    /// Qualification records applicable when the original operation ran.
    pub qualification_records: Vec<String>,
    /// Immutable qualification evidence references, not loaded by replay.
    pub evidence: Vec<ArtifactReference>,
}

/// Relocatable descriptor of a bounded byte range. It stores inputs, never decoded answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisDescriptor {
    /// Must equal the supported descriptor format.
    pub format: String,
    /// Whether bytes originated in an executable or an authored test.
    pub capture_origin: CaptureOrigin,
    /// Identities of the original analysis operation.
    pub provenance: AnalysisProvenance,
    /// Virtual address at which decoding starts.
    pub address: u64,
    /// Exact raw instructions under the supplied artifact root.
    pub code: ArtifactReference,
}

/// How a static decode result was produced; neither alternative launches a game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum AnalysisOrigin {
    /// Decoded from a currently verified executable.
    Executable,
    /// Decoded from verified retained bytes; provenance remains historical.
    Replay,
}

/// Complete decoding of the bounded range with its original provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AnalysisResult {
    /// Current execution or recorded replay.
    pub origin: AnalysisOrigin,
    /// Verified inputs and original provenance supporting these instructions.
    pub descriptor: AnalysisDescriptor,
    /// Every instruction in the bounded range, in address order.
    pub instructions: Vec<Instruction>,
}

/// A range cannot be decoded completely under the pinned method.
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
