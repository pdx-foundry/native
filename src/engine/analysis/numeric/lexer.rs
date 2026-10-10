//! The text lexer's token boundary. `CTextLexer::GetTok` is matched whole, and its three
//! character tables byte for byte, so every numeric reader converts token text that one
//! established rule formed. `docs/native/text-lexer.md` records the rule and the stated contract
//! of the lexer's input file.
use std::collections::BTreeMap;
use std::ops::RangeInclusive;

use super::super::decode::Instruction;
use super::super::evaluate::ReadOnlyData;
use super::super::references::shapes::{Line, Shape, canonical_local};
use super::super::stop::Unresolved;

/// `CTextLexer::GetTok` and the read-only bytes that hold its character tables.
pub(crate) struct LexerInput {
    pub body: Vec<Instruction>,
    pub names: BTreeMap<u64, String>,
    pub data: ReadOnlyData,
}

/// The bytes that each table classifies; the shape sends every other byte to a word.
const TABLE_BYTES: RangeInclusive<u8> = b'!'..=b'}';

/// Bytes that end a word. The lexer pushes each one back, so it starts the next token.
const TERMINATORS: &[u8] = b"!\"#(),;<=>{}";

/// The first table's entry for each first byte of a token. An entry is the instruction offset of
/// the byte's handler from the table's `adr` base, which `lexer.txt` fixes.
const FIRST_BYTE_ENTRIES: [(&[u8], u8); 11] = [
    (b"#;", 0), // comment to the end of the line
    (b"!", 2),  // `!` or `!=`
    (b"\"", 4), // quoted string
    (b"(", 8),
    (b")", 10),
    (b",", 12),
    (b"<", 14), // `<` or `<=`
    (b">", 16), // `>` or `>=`
    (b"{", 18),
    (b"}", 20),
    (b"=", 21),
];

/// The first table's entry for a byte that starts a word.
const WORD_START: u8 = 6;

/// The second table's entry for a terminator inside a word; other bytes continue the word.
const WORD_END: u8 = 21;

/// The third table's entry for every byte that is not a terminator. Such a byte never ends a
/// word, and the entry skips the `UnGet` call that a terminator's entry 0 makes.
const NOT_PUSHED_BACK: u8 = 2;

/// `Ok` when `GetTok` and its character tables match the established token boundary.
pub(crate) fn token_boundary(input: &LexerInput) -> Result<(), Unresolved> {
    let lines = canonical_local(&input.body, &input.names)
        .filter(|lines| {
            Shape::parse(include_str!("shapes/lexer.txt"))
                .matches(lines)
                .is_some()
        })
        .ok_or(Unresolved::new("numeric-lexer-shape"))?;

    character_tables(&input.body, &lines, &input.data)
}

/// Compare the first-byte, word-end and push-back tables that a matched body forms, in that
/// order, with the established classification.
pub(super) fn character_tables(
    rows: &[Instruction],
    lines: &[Line],
    data: &ReadOnlyData,
) -> Result<(), Unresolved> {
    let mismatch = || Unresolved::new("numeric-lexer-table");
    let addresses = table_addresses(rows, lines).ok_or_else(mismatch)?;
    for (address, expected) in addresses.into_iter().zip(expected_tables()) {
        let actual: Option<Vec<u8>> = (address..)
            .take(expected.len())
            .map(|address| data.read(address, 1).map(|byte| byte as u8))
            .collect();
        if actual != Some(expected) {
            return Err(mismatch());
        }
    }

    Ok(())
}

/// The addresses of the body's unnamed `adrp`/`add` pairs, in order; `None` unless there are
/// exactly three.
fn table_addresses(rows: &[Instruction], lines: &[Line]) -> Option<[u64; 3]> {
    let addresses: Vec<u64> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.text.starts_with("add ") && line.value.as_deref() == Some("?"))
        .map(|(index, _)| page_address(rows.get(index.checked_sub(1)?)?, &rows[index]))
        .collect::<Option<_>>()?;

    addresses.try_into().ok()
}

/// The address that `adrp xN,#page` and `add xN,xN,#offset` form together.
fn page_address(adrp: &Instruction, add: &Instruction) -> Option<u64> {
    if adrp.operation != "adrp" {
        return None;
    }

    let (register, page) = adrp.operands.split_once(',')?;
    let (operands, offset) = add.operands.rsplit_once(',')?;
    if operands != format!("{register},{register}") {
        return None;
    }

    immediate(page)?.checked_add(immediate(offset)?)
}

/// The value of `#0x…` or a decimal `#…` operand.
fn immediate(operand: &str) -> Option<u64> {
    let digits = operand.strip_prefix('#')?;
    match digits.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => digits.parse().ok(),
    }
}

fn expected_tables() -> [Vec<u8>; 3] {
    let first_byte = TABLE_BYTES
        .map(|byte| {
            FIRST_BYTE_ENTRIES
                .iter()
                .find(|(class, _)| class.contains(&byte))
                .map_or(WORD_START, |(_, entry)| *entry)
        })
        .collect();
    let word_end = TABLE_BYTES
        .map(|byte| {
            if TERMINATORS.contains(&byte) {
                WORD_END
            } else {
                0
            }
        })
        .collect();
    let push_back = TABLE_BYTES
        .map(|byte| {
            if TERMINATORS.contains(&byte) {
                0
            } else {
                NOT_PUSHED_BACK
            }
        })
        .collect();

    [first_byte, word_end, push_back]
}
