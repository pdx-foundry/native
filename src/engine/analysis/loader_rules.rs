//! File selection of a registry's loader, from the arguments of its file-enumeration calls.
//!
//! A loader lists its files with
//! `VFSGetEnumeratedFiles(directory, files, suffix, substring, depth)`. A file name must end with
//! `suffix` and contain `substring`; a null pointer is no filter. `depth` 0 reads only the
//! directory, `n > 0` also reads `n` levels of subdirectories, and a negative depth reads every
//! level. The method reads each argument that the call's own basic block sets
//! ([`directories::enumeration_calls`]): a literal text, a null pointer, or a constant depth. Any
//! other value is not established. `docs/native/loader-rules.md` has the engine facts.
use std::collections::{BTreeMap, BTreeSet};

use super::directories::{self, Anchors, Constructor, Value};
use crate::Recursion;

/// Method revision recorded in each answer's source.
pub const METHOD: &str = "loader-rules/v1";

/// What one text argument of an enumeration call holds.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Text {
    /// A null pointer: no filter.
    Absent,
    /// A text literal.
    Literal(String),
    /// Not established.
    Unknown,
}

/// The file-selection arguments of one enumeration call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enumeration {
    /// The text that a file name must end with.
    pub suffix: Text,
    /// The text that a file name must contain.
    pub substring: Text,
    /// The depth argument, when it is a constant.
    pub depth: Option<i32>,
}

/// Why a file-selection property is not established.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Obstacle {
    /// The loader makes no enumeration call that the scan reads.
    NoEnumeration,
    /// An enumeration call passes a value that is not a literal.
    NotLiteral,
    /// The enumeration calls pass this many different values.
    Several(usize),
}

/// The file selection of one registry's loader. Each property is established or has its obstacle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    /// The required suffix of a file name; `None` is no filter.
    pub extension: Result<Option<String>, Obstacle>,
    /// The required substring of a file name; `None` is no filter.
    pub name_contains: Result<Option<String>, Obstacle>,
    /// The subdirectory levels that the loader reads.
    pub recursion: Result<Recursion, Obstacle>,
}

/// The arguments of every enumeration call in `functions`, the loader's functions.
pub fn enumerations(
    functions: &[Constructor],
    anchors: &Anchors,
    strings: &BTreeMap<u64, String>,
) -> Vec<Enumeration> {
    functions
        .iter()
        .flat_map(|function| directories::enumeration_calls(function, anchors))
        .map(|call| Enumeration {
            suffix: text(call.suffix, strings),
            substring: text(call.substring, strings),
            depth: depth(call.depth),
        })
        .collect()
}

/// The file selection that every enumeration call agrees on, property by property.
pub fn selection(calls: &[Enumeration]) -> Selection {
    Selection {
        extension: agreed(calls.iter().map(|call| filter(&call.suffix))),
        name_contains: agreed(calls.iter().map(|call| filter(&call.substring))),
        recursion: agreed(calls.iter().map(|call| call.depth.map(recursion))),
    }
}

fn text(value: Value, strings: &BTreeMap<u64, String>) -> Text {
    match value {
        Value::Constant(0) => Text::Absent,
        Value::Constant(address) => strings
            .get(&address)
            .map_or(Text::Unknown, |literal| Text::Literal(literal.clone())),
        Value::Unknown | Value::Stack(_) => Text::Unknown,
    }
}

/// The depth as the `int` parameter receives it: the low 32 bits of the register.
fn depth(value: Value) -> Option<i32> {
    match value {
        Value::Constant(bits) => Some(bits as u32 as i32),
        Value::Unknown | Value::Stack(_) => None,
    }
}

/// A text argument as a filter, `None` when it is not established.
fn filter(text: &Text) -> Option<Option<String>> {
    match text {
        Text::Absent => Some(None),
        Text::Literal(literal) => Some(Some(literal.clone())),
        Text::Unknown => None,
    }
}

fn recursion(depth: i32) -> Recursion {
    match depth {
        0 => Recursion::None,
        levels if levels > 0 => Recursion::Levels(levels.unsigned_abs()),
        _ => Recursion::Unlimited,
    }
}

/// The one value of all `values`; `None` in `values` is a value that is not established.
fn agreed<T: Ord>(values: impl Iterator<Item = Option<T>>) -> Result<T, Obstacle> {
    let mut found = BTreeSet::new();
    for value in values {
        found.insert(value.ok_or(Obstacle::NotLiteral)?);
    }

    match found.len() {
        0 => Err(Obstacle::NoEnumeration),
        1 => Ok(found.pop_first().expect("one value")),
        several => Err(Obstacle::Several(several)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::assembler::{Arm64, arm64};

    const ENUMERATE: u64 = 0x1300;
    const DIRECTORY: u64 = 0x2010;
    const TXT: u64 = 0x2020;

    fn anchors() -> Anchors {
        Anchors {
            file_enumerations: BTreeSet::from([ENUMERATE]),
            ..Anchors::default()
        }
    }
    fn strings() -> BTreeMap<u64, String> {
        BTreeMap::from([
            (DIRECTORY, "common/examples".to_owned()),
            (TXT, ".txt".to_owned()),
        ])
    }
    fn read(code: Arm64) -> Vec<Enumeration> {
        let function = Constructor {
            address: 0x1000,
            code: code.bytes(),
        };

        enumerations(&[function], &anchors(), &strings())
    }

    #[test]
    fn literal_arguments_give_the_file_selection() {
        let mut code = Arm64::at(0x1000);
        code.address(0, DIRECTORY).address(2, TXT);
        arm64!(code; mov x3, xzr; movn w4, #0);
        code.call(ENUMERATE);

        let calls = read(code);

        assert_eq!(
            calls,
            vec![Enumeration {
                suffix: Text::Literal(".txt".into()),
                substring: Text::Absent,
                depth: Some(-1),
            }]
        );
        assert_eq!(
            selection(&calls),
            Selection {
                extension: Ok(Some(".txt".into())),
                name_contains: Ok(None),
                recursion: Ok(Recursion::Unlimited),
            }
        );
    }

    #[test]
    fn null_filters_and_a_zero_depth_read_every_file_of_the_directory() {
        let mut code = Arm64::at(0x1000);
        arm64!(code; ldr x0, [x19]; movz x2, #0; movz x3, #0; movz w4, #0);
        code.call(ENUMERATE);

        assert_eq!(
            selection(&read(code)),
            Selection {
                extension: Ok(None),
                name_contains: Ok(None),
                recursion: Ok(Recursion::None),
            }
        );
    }

    #[test]
    fn arguments_set_after_two_paths_join_are_read() {
        let mut code = Arm64::at(0x1000);
        arm64!(code
            ; tbnz w8, #31, >long
            ; mov x0, x28
            ; b >join
            ; long:
            ; ldr x0, [x28]
            ; join:
            ; movz x2, #0
            ; movz x3, #0
            ; movz w4, #0
        );
        code.call(ENUMERATE);

        assert_eq!(
            selection(&read(code)),
            Selection {
                extension: Ok(None),
                name_contains: Ok(None),
                recursion: Ok(Recursion::None),
            }
        );
    }

    #[test]
    fn an_argument_set_before_a_branch_target_is_not_established() {
        let mut code = Arm64::at(0x1000);
        code.address(2, TXT);
        arm64!(code
            ; cbz w8, >join
            ; nop
            ; join:
            ; movz x3, #0
            ; movz w4, #0
        );
        code.call(ENUMERATE);

        assert_eq!(read(code)[0].suffix, Text::Unknown);
    }

    #[test]
    fn an_argument_that_is_not_a_literal_is_not_established() {
        let mut code = Arm64::at(0x1000);
        code.address(0, DIRECTORY);
        arm64!(code; ldr x2, [x19]; movz x3, #0; movz w4, #2);
        code.call(ENUMERATE);

        assert_eq!(
            selection(&read(code)),
            Selection {
                extension: Err(Obstacle::NotLiteral),
                name_contains: Ok(None),
                recursion: Ok(Recursion::Levels(2)),
            }
        );
    }

    #[test]
    fn a_loader_with_no_enumeration_call_establishes_nothing() {
        let mut code = Arm64::at(0x1000);
        arm64!(code; ret);

        assert_eq!(
            selection(&read(code)),
            Selection {
                extension: Err(Obstacle::NoEnumeration),
                name_contains: Err(Obstacle::NoEnumeration),
                recursion: Err(Obstacle::NoEnumeration),
            }
        );
    }

    #[test]
    fn calls_that_disagree_establish_no_value() {
        let calls = [
            Enumeration {
                suffix: Text::Literal(".txt".into()),
                substring: Text::Absent,
                depth: Some(0),
            },
            Enumeration {
                suffix: Text::Absent,
                substring: Text::Absent,
                depth: Some(0),
            },
        ];

        assert_eq!(
            selection(&calls),
            Selection {
                extension: Err(Obstacle::Several(2)),
                name_contains: Ok(None),
                recursion: Ok(Recursion::None),
            }
        );
    }
}
