//! Content directory of a registry, read from its database constructor.
//!
//! Each engine database class names its content directory with one string literal, which its
//! constructor loads with an `adrp`/`add` pair. The method takes every constructor body of one
//! class, resolves each pair against the executable's string literals, and keeps the literals that
//! have the shape of a relative directory (`common/traditions`, `map/galaxy`).
//!
//! Exactly one distinct directory names the registry. None or several is a gap; the method never
//! selects one of several. A pair is followed only when the `add` reads the register that the
//! `adrp` wrote, within a short window, with no other write to that register between them.
use std::collections::{BTreeMap, BTreeSet};

/// Method revision recorded in each answer's source.
pub const METHOD: &str = "registry-directories/v1";

/// Instructions examined after an `adrp` for its `add`.
const WINDOW: usize = 6;

/// One constructor body of a database class.
#[derive(Debug, Clone)]
pub struct Constructor {
    /// File virtual address of the first instruction.
    pub address: u64,
    /// Complete function bytes.
    pub code: Vec<u8>,
}

/// What the constructors of one class establish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Directory {
    /// Exactly one directory literal.
    Named(String),
    /// No constructor, or no directory literal in any constructor.
    Missing,
    /// Several distinct directory literals; none is selected.
    Ambiguous(Vec<String>),
}

/// Resolve the directory that the constructors of one database class load.
pub fn directory(constructors: &[Constructor], strings: &BTreeMap<u64, String>) -> Directory {
    let mut found = BTreeSet::new();
    for constructor in constructors {
        let words: Vec<u32> = constructor
            .code
            .chunks_exact(4)
            .map(|word| u32::from_le_bytes(word.try_into().expect("four bytes")))
            .collect();
        for (index, &word) in words.iter().enumerate() {
            let Some((register, page)) = adrp(word, constructor.address + index as u64 * 4) else {
                continue;
            };
            for &next in words.iter().skip(index + 1).take(WINDOW) {
                if let Some(offset) = add_immediate(next, register) {
                    if let Some(text) = strings.get(&page.wrapping_add(offset))
                        && is_directory(text)
                    {
                        found.insert(text.trim_end_matches('/').to_owned());
                    }
                    break;
                }
                if writes(next, register) {
                    break;
                }
            }
        }
    }
    let mut found: Vec<_> = found.into_iter().collect();
    match found.len() {
        0 => Directory::Missing,
        1 => Directory::Named(found.remove(0)),
        _ => Directory::Ambiguous(found),
    }
}

/// `adrp xd, page`: destination register and target page.
fn adrp(word: u32, address: u64) -> Option<(u32, u64)> {
    if word & 0x9f00_0000 != 0x9000_0000 {
        return None;
    }
    let immediate = (((word >> 5) & 0x7ffff) << 2 | ((word >> 29) & 3)) as i64;
    let immediate = if immediate >= 1 << 20 {
        immediate - (1 << 21)
    } else {
        immediate
    };
    let page = ((address & !0xfff) as i64).wrapping_add(immediate << 12);
    Some((word & 31, page as u64))
}

/// `add xd, xn, #imm` (64-bit, no shift) that reads `register`: the immediate.
fn add_immediate(word: u32, register: u32) -> Option<u64> {
    (word & 0xffc0_0000 == 0x9100_0000 && (word >> 5) & 31 == register)
        .then_some(((word >> 10) & 0xfff) as u64)
}

/// Conservative: any instruction whose destination field equals the register ends the pair.
fn writes(word: u32, register: u32) -> bool {
    word & 31 == register
}

/// A relative directory: lowercase segments joined by `/`, with no file extension.
fn is_directory(text: &str) -> bool {
    let text = text.trim_end_matches('/');
    let mut segments = text.split('/');
    text.contains('/')
        && segments.all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|word| word.to_le_bytes()).collect()
    }
    // adrp x1, +1 page from 0x1000; add x1, x1, #imm.
    const ADRP_X1: u32 = 0xb000_0001;
    fn add_x1(immediate: u32) -> u32 {
        0x9100_0021 | immediate << 10
    }
    fn strings() -> BTreeMap<u64, String> {
        BTreeMap::from([
            (0x2010, "common/examples".to_owned()),
            (0x2020, "map/other".to_owned()),
            (0x2030, "common/alerts.txt".to_owned()),
            (0x2040, "Example".to_owned()),
        ])
    }
    fn constructor(words: &[u32]) -> Vec<Constructor> {
        vec![Constructor {
            address: 0x1000,
            code: code(words),
        }]
    }

    #[test]
    fn one_directory_literal_names_the_registry() {
        let nop = 0xd503_201f;
        assert_eq!(
            directory(&constructor(&[ADRP_X1, nop, add_x1(0x10)]), &strings()),
            Directory::Named("common/examples".into())
        );
    }

    #[test]
    fn files_and_plain_words_are_not_directories() {
        assert_eq!(
            directory(
                &constructor(&[ADRP_X1, add_x1(0x30), ADRP_X1, add_x1(0x40)]),
                &strings()
            ),
            Directory::Missing
        );
        assert_eq!(directory(&[], &strings()), Directory::Missing);
    }

    #[test]
    fn several_directories_are_never_resolved_to_one() {
        assert_eq!(
            directory(
                &constructor(&[ADRP_X1, add_x1(0x10), ADRP_X1, add_x1(0x20)]),
                &strings()
            ),
            Directory::Ambiguous(vec!["common/examples".into(), "map/other".into()])
        );
    }

    #[test]
    fn a_clobbered_page_register_ends_the_pair() {
        let mov_x1_x2 = 0xaa02_03e1;
        assert_eq!(
            directory(
                &constructor(&[ADRP_X1, mov_x1_x2, add_x1(0x10)]),
                &strings()
            ),
            Directory::Missing
        );
    }

    #[test]
    fn two_constructor_bodies_with_one_literal_agree() {
        let mut both = constructor(&[ADRP_X1, add_x1(0x10)]);
        both.push(Constructor {
            address: 0x1000,
            code: code(&[ADRP_X1, add_x1(0x10)]),
        });
        assert_eq!(
            directory(&both, &strings()),
            Directory::Named("common/examples".into())
        );
    }
}
