//! ARM64 instruction decoding. Every byte of a range is accounted for, or the range is an
//! error.
use std::collections::BTreeMap;

use capstone::prelude::*;
use serde::{Deserialize, Serialize};

use super::declarations::number;

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

/// Decode an aligned ARM64 range of any length; an empty range has no instructions. Unknown
/// instructions, trailing bytes, and address overflow are errors; no bytes are skipped or
/// invented.
pub fn decode_arm64(bytes: &[u8], address: u64) -> Result<Vec<Instruction>, DecodeError> {
    if !bytes.len().is_multiple_of(4)
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

/// Whether an instruction with this mnemonic can leave straight-line flow: a branch, call or
/// return.
pub fn is_control_transfer(operation: &str) -> bool {
    operation.starts_with("b.")
        || matches!(
            operation,
            "b" | "bl" | "blr" | "br" | "ret" | "cbz" | "cbnz" | "tbz" | "tbnz"
        )
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

/// The destination register, base register and offset of a 64-bit `ldr` with an unsigned
/// immediate offset, such as `ldr x8,[x8,#0x248]`.
pub fn ldr_immediate(word: u32) -> Option<(usize, usize, u64)> {
    if word & 0xffc0_0000 != 0xf940_0000 {
        return None;
    }
    Some((
        (word & 0x1f) as usize,
        (word >> 5 & 0x1f) as usize,
        u64::from(word >> 10 & 0xfff) * 8,
    ))
}

/// The destination register, source register and subtrahend of a 64-bit `sub` of an
/// immediate, with its optional `lsl #12`. Register 31 is the stack pointer.
pub fn sub_immediate(word: u32) -> Option<(usize, usize, u64)> {
    if word & 0xff80_0000 != 0xd100_0000 {
        return None;
    }
    add_immediate(word & !0x4000_0000)
}

/// The general registers that an instruction may write. A branch writes none and a call writes
/// the caller-saved registers and the link register. Any other instruction writes its
/// destination operands and the base of a pre- or post-index access; a store or a comparison
/// has no destination operand, and an atomic operation's destination follows its source.
pub fn written_registers(operation: &str, operands: &str) -> Vec<usize> {
    if operation == "bl" || operation.starts_with("blr") {
        return (0..=18).chain([30]).collect();
    }
    if is_branch(operation) {
        return Vec::new();
    }

    let mut written: Vec<usize> = operands
        .split(',')
        .skip(usize::from(is_atomic(operation)))
        .take(destination_count(operation))
        .filter_map(general_register)
        .collect();
    written.extend(writeback_base(operands));
    written
}

/// Whether some path from the start of `rows`, one whole function, may read `register` before
/// it writes it. A call, a branch through a register, a branch out of the function and the end of
/// `rows` count as a read, because the code that runs next is not in `rows`.
pub fn reads_before_writing(rows: &[Instruction], register: usize) -> bool {
    let index: BTreeMap<u64, usize> = rows
        .iter()
        .enumerate()
        .map(|(position, row)| (row.address, position))
        .collect();
    let mut visited = vec![false; rows.len()];
    let mut pending = vec![0];

    while let Some(mut position) = pending.pop() {
        loop {
            let Some(row) = rows.get(position) else {
                return true;
            };

            if std::mem::replace(&mut visited[position], true) {
                break;
            }

            let operation = row.operation.as_str();
            if operation == "bl" || operation.starts_with("blr") || operation.starts_with("br") {
                return true;
            }
            if reads_register(operation, &row.operands, register) {
                return true;
            }
            if operation.starts_with("ret")
                || written_registers(operation, &row.operands).contains(&register)
            {
                break;
            }

            if is_branch(operation) {
                let target = row.operands.rsplit(',').next().and_then(number);
                let Some(&target) = target.and_then(|target| index.get(&target)) else {
                    return true;
                };
                pending.push(target);
                if operation == "b" {
                    break;
                }
            }
            position += 1;
        }
    }
    false
}

/// Whether an instruction reads general register `register`: any operand after the ones that
/// it only writes. An insertion (`movk`, `bfi`), a compare-and-swap and an atomic operation also
/// read their first operand.
fn reads_register(operation: &str, operands: &str, register: usize) -> bool {
    let reads_first = matches!(operation, "movk" | "bfi" | "bfxil" | "bfm")
        || operation.starts_with("cas")
        || is_atomic(operation);
    let written = if reads_first || is_branch(operation) {
        0
    } else {
        destination_count(operation)
    };

    operands
        .split(',')
        .skip(written)
        .map(|part| part.trim_start_matches('[').trim_end_matches(['!', ']']))
        .any(|part| general_register(part) == Some(register))
}

/// `b`, `b.cond`, `br`, `cbz`, `cbnz`, `tbz`, `tbnz`, `ret` and their pointer-authenticated
/// forms. `bl` and `blr` are calls.
fn is_branch(operation: &str) -> bool {
    operation == "b"
        || operation.starts_with("b.")
        || operation.starts_with("br")
        || operation.starts_with("cb")
        || matches!(operation, "tbz" | "tbnz")
        || operation.starts_with("ret")
}

/// How many leading operands an instruction that is not a branch writes.
fn destination_count(operation: &str) -> usize {
    let exclusive_store = ["stxr", "stlxr", "stxp", "stlxp"]
        .iter()
        .any(|prefix| operation.starts_with(prefix));
    let writes_nothing = (operation.starts_with("st") && !exclusive_store)
        || ["cmp", "cmn", "tst", "ccmp", "ccmn", "fcmp", "nop", "prfm"].contains(&operation);
    let pair = ["ldp", "ldnp", "ldxp", "ldaxp", "casp"]
        .iter()
        .any(|prefix| operation.starts_with(prefix));

    if writes_nothing {
        0
    } else if pair {
        2
    } else {
        1
    }
}

/// An atomic load-and-operate or swap, which reads its first operand and writes the loaded value
/// to its second. Its store-only form (`stadd`) writes nothing.
fn is_atomic(operation: &str) -> bool {
    [
        "ldadd", "ldclr", "ldeor", "ldset", "ldsmax", "ldsmin", "ldumax", "ldumin", "swp",
    ]
    .iter()
    .any(|prefix| operation.starts_with(prefix))
}

/// The base register of a pre-index (`[x8,#8]!`) or post-index (`[x8],#8`) operand.
fn writeback_base(operands: &str) -> Option<usize> {
    let (_, address) = operands.split_once('[')?;
    let (inside, after) = address.split_once(']')?;
    let writes_back = after.starts_with('!') || after.starts_with(',');
    if !writes_back {
        return None;
    }

    general_register(inside.split(',').next()?)
}

/// The number of general register `name`, from `x0` or `w0` to `x30` or `w30`. Register 31 is
/// the stack pointer or the zero register by name (`sp`, `xzr`), so it is not a general register.
/// The pinned decoder writes `x29` and `x30`, never `fp` or `lr`, so no alias is read.
pub fn general_register(name: &str) -> Option<usize> {
    let number = name.strip_prefix(['x', 'w'])?.parse().ok()?;
    (number <= 30).then_some(number)
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

    #[test]
    fn ldr_immediate_reads_a_scaled_offset_and_refuses_other_loads() {
        assert_eq!(
            decoded(0xf941_2508, 0),
            ("ldr".into(), "x8,[x8,#0x248]".into())
        );
        assert_eq!(ldr_immediate(0xf941_2508), Some((8, 8, 0x248)));
        assert_eq!(decoded(0xb940_0108, 0).0, "ldr");
        assert_eq!(ldr_immediate(0xb940_0108), None);
    }

    #[test]
    fn the_frame_and_link_registers_are_general_registers_by_number() {
        assert_eq!(
            decoded(0xa9bf_7bfd, 0),
            ("stp".into(), "x29,x30,[sp,#-0x10]!".into())
        );
        assert_eq!(decoded(0xaa1e_03e0, 0), ("mov".into(), "x0,x30".into()));
        assert_eq!(general_register("x29"), Some(29));
        assert_eq!(general_register("w30"), Some(30));
        for name in ["fp", "lr", "sp", "wsp", "xzr", "wzr", "x31"] {
            assert_eq!(general_register(name), None, "{name}");
        }
    }

    fn function(lines: &[(&str, &str)]) -> Vec<Instruction> {
        lines
            .iter()
            .zip((0x100..).step_by(4))
            .map(|((operation, operands), address)| Instruction {
                address,
                bytes: [0; 4],
                operation: (*operation).into(),
                operands: (*operands).into(),
            })
            .collect()
    }

    #[test]
    fn a_register_written_first_on_every_path_is_not_read() {
        let returns_a_field = function(&[("ldr", "w0,[x0,#0x438]"), ("ret", "")]);
        let writes_on_both_branches = function(&[
            ("cbz", "x0,#0x10c"),
            ("mov", "x8,#1"),
            ("b", "#0x110"),
            ("add", "x8,sp,#0x10"),
            ("str", "x8,[x0]"),
            ("ret", ""),
        ]);

        assert!(!reads_before_writing(&returns_a_field, 8));
        assert!(!reads_before_writing(&writes_on_both_branches, 8));
    }

    #[test]
    fn a_read_call_or_exit_before_a_write_counts_as_a_read() {
        let stores_through_it = function(&[("str", "xzr,[x8]"), ("ret", "")]);
        let keeps_it = function(&[("mov", "x19,x8"), ("ret", "")]);
        let calls_first = function(&[("bl", "#0x2000"), ("mov", "x8,#1"), ("ret", "")]);
        let tail_branches = function(&[("b", "#0x2000")]);
        let writes_on_one_branch = function(&[
            ("cbz", "x0,#0x10c"),
            ("mov", "x8,#1"),
            ("ret", ""),
            ("ldr", "x9,[x8]"),
            ("ret", ""),
        ]);
        let inserts = function(&[("movk", "x8,#1,lsl#16"), ("ret", "")]);

        for rows in [
            stores_through_it,
            keeps_it,
            calls_first,
            tail_branches,
            writes_on_one_branch,
            inserts,
        ] {
            assert!(reads_before_writing(&rows, 8), "{rows:?}");
        }
    }

    #[test]
    fn an_instruction_that_is_not_a_branch_writes_its_destination() {
        assert_eq!(written_registers("bfi", "x19,x8,#0,#8"), [19]);
        assert_eq!(written_registers("bfxil", "w19,w8,#0,#8"), [19]);
        assert_eq!(written_registers("bic", "x19,x19,x8"), [19]);
        assert_eq!(written_registers("ldp", "x8,x9,[x0]"), [8, 9]);
        assert_eq!(written_registers("stlxr", "w9,x8,[x0]"), [9]);
    }

    #[test]
    fn an_atomic_writes_only_the_register_that_receives_the_old_value() {
        assert_eq!(
            decoded(0xf8a8_0009, 0),
            ("ldadda".into(), "x8,x9,[x0]".into())
        );
        assert_eq!(written_registers("ldadda", "x8,x9,[x0]"), [9]);
        assert_eq!(written_registers("swpal", "x8,x9,[x0]"), [9]);
        assert_eq!(written_registers("cas", "x8,x9,[x0]"), [8]);
        assert!(written_registers("stadd", "w8,[x0]").is_empty());

        let adds_its_value = function(&[("ldaddal", "x8,x9,[x0]"), ("ret", "")]);
        assert!(reads_before_writing(&adds_its_value, 8));
    }

    #[test]
    fn a_store_or_comparison_writes_no_operand() {
        assert!(written_registers("str", "x0,[x8,#8]").is_empty());
        assert!(written_registers("stp", "x0,x1,[x8]").is_empty());
        assert!(written_registers("cmp", "x0,x1").is_empty());
        assert!(written_registers("ccmp", "x0,#0,#4,ne").is_empty());
    }

    #[test]
    fn a_pre_or_post_index_access_writes_its_base() {
        assert_eq!(written_registers("ldr", "x0,[x8,#8]"), [0]);
        assert_eq!(written_registers("ldr", "x0,[x8,#8]!"), [0, 8]);
        assert_eq!(written_registers("ldr", "x0,[x9],#16"), [0, 9]);
        assert_eq!(written_registers("str", "x0,[x8,#8]!"), [8]);
        assert_eq!(written_registers("ldp", "x0,x1,[sp],#16"), [0, 1]);
    }

    #[test]
    fn a_branch_writes_nothing_and_a_call_writes_the_caller_saved_registers() {
        let branches = [
            ("b", "#0x2000"),
            ("b.ne", "#0x2000"),
            ("br", "x8"),
            ("cbz", "x0,#0x2000"),
            ("tbnz", "w0,#3,#0x2000"),
            ("ret", ""),
        ];
        for (operation, operands) in branches {
            assert!(written_registers(operation, operands).is_empty());
        }

        let caller_saved: Vec<usize> = (0..=18).chain([30]).collect();
        assert_eq!(written_registers("bl", "#0x2000"), caller_saved);
        assert_eq!(written_registers("blraa", "x8,x9"), caller_saved);
    }
}
