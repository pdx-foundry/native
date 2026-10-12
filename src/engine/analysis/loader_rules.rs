//! File selection, duplicate rule and definition names of a registry's loader.
//!
//! A loader lists its files with
//! `VFSGetEnumeratedFiles(directory, files, suffix, substring, depth)`. A file name must end with
//! `suffix` and contain `substring`; a null pointer is no filter. `depth` 0 reads only the
//! directory, `n > 0` also reads `n` levels of subdirectories, and a negative depth reads every
//! level. The method reads each argument that the call's own basic block sets
//! ([`directories::enumeration_calls`]): a literal text, a null pointer, or a constant depth. Any
//! other value is not established.
//!
//! A template loader that reads a key that it already holds passes the existing item to its
//! database's `ReadExistingEntry`. At the first load the last argument is `false`: `Init` passes
//! `false` to `LoadFile`, which passes it on to `LoadFromReader`, which passes it or `false` on.
//! The method assumes that chain, which the knowledge page checks over every template registry,
//! and reads the body on the `false` path ([`existing_entry_rule`]). A custom loader has no such
//! body, so its rule is not established.
//!
//! The scan runs the entry block to the test of the last argument, then follows the `false` path:
//! it steps over each conditional branch and follows each unconditional branch inside the body. A
//! virtual call of the item's destructor, or a direct call that receives the item or a subobject
//! (an inline destructor calls each member's destructor), destroys the item. After that, a store
//! at the item's start or a direct call that receives the whole item builds it again. A return
//! after the item is built again also gives `LastWins`: a tag registry's item holds only its key,
//! and its handler reads nothing. Another call through a register, or a branch out of the body
//! that is not the read, ends the scan with no rule. A direct call clears the caller-saved
//! registers.
//!
//! A definition's name is a stated rule of the loader's code, not read from it: each stated form
//! holds when the build has the code that the rule names. `docs/native/loader-rules.md` has the
//! engine facts and the stated rules.
use std::collections::{BTreeMap, BTreeSet};

use super::decode::{
    Instruction, decode_arm64, general_register, is_control_transfer, number, wide_register,
    written_registers,
};
use super::directories::{self, Anchors, Constructor, Value};
use crate::{DefinitionForm, DuplicateRule, Recursion};

/// Method revision recorded in each answer's source.
pub const METHOD: &str = "loader-rules/v3";

/// The register of `ReadExistingEntry`'s existing item, its third argument after `this`.
const EXISTING_ITEM: usize = 3;
/// The register of `ReadExistingEntry`'s reader, its first argument after `this`.
const READER: usize = 1;
/// The register of `ReadExistingEntry`'s last argument, a `bool`.
const LAST_ARGUMENT: &str = "w4";

/// A definition form that a stated rule of one of the registry's loaders gives, or `None` when
/// the build lacks the code that the rule names.
pub type StatedForm = Option<DefinitionForm>;

/// What a registry's loaders show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoaderEvidence {
    /// The arguments of every file enumeration that the loaders make.
    pub enumerations: Vec<Enumeration>,
    /// The rule of each template database's existing-entry handler, `None` for a handler that
    /// [`existing_entry_rule`] does not read. A custom loader has none.
    pub existing_entries: Vec<Option<DuplicateRule>>,
    /// The definition forms that the loaders' stated rules give.
    pub stated: Vec<StatedForm>,
}

/// The definition forms of a registry's loaders that the build holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Naming {
    /// Each distinct form, in the order of the stated rules.
    pub forms: Vec<DefinitionForm>,
    /// How many stated rules name code that the build lacks.
    pub missing: usize,
}

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

/// Why a registry's duplicate rule is not established.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DuplicateObstacle {
    /// No template database loads the registry: its loader's own insert code is not followed.
    NotFollowed,
    /// A template database's existing-entry body does not match a shape that the method reads.
    UnreadBody,
    /// The registry's template databases give this many different rules.
    Several(usize),
}

/// The duplicate rule that every template database of a registry gives. `rules` holds one entry
/// for each database, `None` for a body that [`existing_entry_rule`] does not read.
pub fn duplicate_rule(rules: &[Option<DuplicateRule>]) -> Result<DuplicateRule, DuplicateObstacle> {
    if rules.is_empty() {
        return Err(DuplicateObstacle::NotFollowed);
    }
    let distinct: BTreeSet<DuplicateRule> = rules
        .iter()
        .copied()
        .collect::<Option<_>>()
        .ok_or(DuplicateObstacle::UnreadBody)?;

    match distinct.len() {
        1 => Ok(*distinct.first().expect("one rule")),
        several => Err(DuplicateObstacle::Several(several)),
    }
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

/// The duplicate rule of one `ReadExistingEntry` body, from what its path with a `false` last
/// argument does to the existing item before it reads the later definition into it: a call that
/// receives the item, or one of its subobjects, and the reader. A path that destroys the item
/// and builds it again first gives [`DuplicateRule::LastWins`]; a path that reads into the item
/// as it is gives [`DuplicateRule::Merges`]. Any other body gives `None`, including one that does
/// not test the last argument before its first call or branch.
pub fn existing_entry_rule(code: &[u8], address: u64) -> Option<DuplicateRule> {
    let rows = decode_arm64(code, address).ok()?;
    let positions: BTreeMap<u64, usize> = rows
        .iter()
        .enumerate()
        .map(|(position, row)| (row.address, position))
        .collect();
    let (test, when_false) = argument_test(&rows, &positions)?;
    let mut values = BTreeMap::from([(EXISTING_ITEM, Held::Item(0)), (READER, Held::Reader)]);
    for row in &rows[..test] {
        track(&mut values, row);
    }
    let mut destroyed = false;
    let mut rebuilt = false;
    let mut position = when_false;

    for _ in 0..rows.len() {
        let row = rows.get(position)?;
        let call = match row.operation.as_str() {
            "b" => match positions.get(&number(&row.operands)?) {
                Some(&target) => {
                    position = target;
                    continue;
                }
                None => Call::Direct,
            },
            "bl" => Call::Direct,
            "blr" | "br" => Call::Indirect(*values.get(&wide_register(&row.operands)?)?),
            "ret" => return (destroyed && rebuilt).then_some(DuplicateRule::LastWins),
            _ => {
                rebuilt |= destroyed && writes_item_start(&values, row);
                track(&mut values, row);
                position += 1;
                continue;
            }
        };

        let receives = values.get(&0).copied();
        let destructor = matches!(call, Call::Indirect(Held::Slot(slot)) if slot.is_destructor());
        if destructor && receives == Some(Held::Item(0)) && !destroyed {
            destroyed = true;
        } else if reads_into_item(&values) {
            return match (destroyed, rebuilt) {
                (false, _) => Some(DuplicateRule::Merges),
                (true, true) => Some(DuplicateRule::LastWins),
                (true, false) => None,
            };
        } else if matches!(call, Call::Indirect(_)) || row.operation == "b" {
            return None;
        } else if let Some(Held::Item(subobject)) = receives {
            rebuilt |= destroyed && subobject == 0;
            destroyed = true;
        }
        forget(&mut values, row);
        // On Apple ARM64 a constructor or destructor returns `this`, and the code uses `x0` after
        // the call without a move.
        if let Some(held @ Held::Item(_)) = receives {
            values.insert(0, held);
        }
        position += 1;
    }

    None
}

/// A call or a branch out of the body on the scanned path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Call {
    /// A `bl`, or a `b` to an address outside the body.
    Direct,
    /// A `blr` or `br` through a register that holds this value.
    Indirect(Held),
}

/// What a register holds on the scanned path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Held {
    /// The address of the existing item's subobject at this offset.
    Item(u64),
    /// The vtable pointer of the subobject at this offset.
    Vtable(u64),
    /// The function in a slot of a subobject's vtable.
    Slot(VirtualSlot),
    /// The reader of the later definition.
    Reader,
    /// A constant, such as the offset of a subobject.
    Constant(u64),
}

/// A slot of the vtable of one of the item's subobjects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VirtualSlot {
    subobject: u64,
    offset: u64,
}

impl VirtualSlot {
    /// The first slot of the primary vtable: the destructor that leaves the memory in place.
    fn is_destructor(self) -> bool {
        self.subobject == 0 && self.offset == 0
    }
}

/// The position of the test of the last argument (`tbz` or `tbnz` of bit 0, or `cbz` or `cbnz`),
/// which must be the body's first call or branch, and the position where the path with a `false`
/// argument starts.
fn argument_test(rows: &[Instruction], positions: &BTreeMap<u64, usize>) -> Option<(usize, usize)> {
    let (position, test) = rows
        .iter()
        .enumerate()
        .find(|(_, row)| is_control_transfer(&row.operation))?;
    let mut operands = test.operands.split(',');
    if operands.next() != Some(LAST_ARGUMENT) {
        return None;
    }
    let tests_bit = matches!(test.operation.as_str(), "tbz" | "tbnz");
    if tests_bit && operands.next() != Some("#0") {
        return None;
    }
    let target = *positions.get(&number(operands.next()?)?)?;

    match test.operation.as_str() {
        "tbz" | "cbz" => Some((position, target)),
        "tbnz" | "cbnz" => Some((position, position + 1)),
        _ => None,
    }
}

/// Whether a call now receives the item, or one of its subobjects, and the reader.
fn reads_into_item(values: &BTreeMap<usize, Held>) -> bool {
    matches!(values.get(&0), Some(Held::Item(_))) && values.get(&1) == Some(&Held::Reader)
}

/// Whether `row` stores at the start of the item, where its primary vtable pointer is.
fn writes_item_start(values: &BTreeMap<usize, Held>, row: &Instruction) -> bool {
    row.operation.starts_with("st")
        && memory_operand(&row.operands).is_some_and(|(base, offset, _)| {
            values.get(&base) == Some(&Held::Item(0)) && offset == 0
        })
}

/// Update `values` for a row that is not a call or branch: a load, a store that moves its base,
/// a value that [`derived`] reads, or any other write.
fn track(values: &mut BTreeMap<usize, Held>, row: &Instruction) {
    match row.operation.as_str() {
        "ldr" => return load(values, row),
        "str" => return store(values, row),
        _ => {}
    }
    let destination = row.operands.split(',').next().unwrap_or_default();
    // A 32-bit write zero-extends: it keeps a constant, but not an address.
    let held = derived(values, row);
    let register = match held {
        Some(Held::Constant(_)) => general_register(destination),
        Some(_) => wide_register(destination),
        None => None,
    };

    match (register, held) {
        (Some(register), Some(held)) => {
            values.insert(register, held);
        }
        _ => forget(values, row),
    }
}

/// What a `mov` or an `add` writes: a constant, a copy, or the item's address plus a constant.
fn derived(values: &BTreeMap<usize, Held>, row: &Instruction) -> Option<Held> {
    let value = |operand: &str| match number(operand) {
        Some(constant) => Some(Held::Constant(constant)),
        None => values.get(&wide_register(operand)?).copied(),
    };
    let operands: Vec<&str> = row.operands.split(',').collect();

    match (row.operation.as_str(), operands.as_slice()) {
        ("mov", [_, source]) => value(source),
        ("add", [_, base, addend]) => match (value(base)?, value(addend)?) {
            (Held::Item(subobject), Held::Constant(offset)) => Some(Held::Item(subobject + offset)),
            _ => None,
        },
        _ => None,
    }
}

/// A 64-bit store. A pre-index writeback from the item or a subobject moves the base to the
/// subobject at the address.
fn store(values: &mut BTreeMap<usize, Held>, row: &Instruction) {
    let moved = memory_operand(&row.operands).and_then(|(base, offset, writeback)| {
        let Some(&Held::Item(subobject)) = values.get(&base) else {
            return None;
        };
        writeback.then_some((base, subobject + offset))
    });

    forget(values, row);
    if let Some((base, subobject)) = moved {
        values.insert(base, Held::Item(subobject));
    }
}

/// A 64-bit load. From the item or a subobject, it loads the vtable pointer of the subobject at
/// the address; from a vtable pointer, it loads a slot. A pre-index writeback moves the base.
fn load(values: &mut BTreeMap<usize, Held>, row: &Instruction) {
    let destination = row.operands.split(',').next().and_then(wide_register);
    let loaded = memory_operand(&row.operands).and_then(|(base, offset, writeback)| {
        let held = match *values.get(&base)? {
            Held::Item(subobject) => Held::Vtable(subobject + offset),
            Held::Vtable(subobject) => Held::Slot(VirtualSlot { subobject, offset }),
            Held::Slot(_) | Held::Reader | Held::Constant(_) => return None,
        };
        Some((base, offset, writeback, held))
    });

    let (Some(destination), Some((base, offset, writeback, held))) = (destination, loaded) else {
        forget(values, row);
        return;
    };
    if let (true, Some(&Held::Item(subobject))) = (writeback, values.get(&base)) {
        values.insert(base, Held::Item(subobject + offset));
    }
    values.insert(destination, held);
}

/// The base register, offset and pre-index writeback of a memory operand such as
/// `[x19,#0x38]!`. A post-index or register offset is not read.
fn memory_operand(operands: &str) -> Option<(usize, u64, bool)> {
    let (_, address) = operands.split_once('[')?;
    let (inside, after) = address.split_once(']')?;
    if !(after.is_empty() || after == "!") {
        return None;
    }
    let mut parts = inside.split(',');
    let base = wide_register(parts.next()?)?;
    let offset = parts.next().map_or(Some(0), number)?;

    Some((base, offset, after == "!"))
}

/// Forget every register that `row` may write.
fn forget(values: &mut BTreeMap<usize, Held>, row: &Instruction) {
    for written in written_registers(&row.operation, &row.operands) {
        values.remove(&written);
    }
}

/// The distinct forms of `stated`, and how many rules the build does not hold.
pub fn naming(stated: &[StatedForm]) -> Naming {
    let mut forms = Vec::new();
    for form in stated.iter().flatten() {
        if !forms.contains(form) {
            forms.push(form.clone());
        }
    }

    Naming {
        forms,
        missing: stated.iter().filter(|form| form.is_none()).count(),
    }
}

fn text(value: Value, strings: &BTreeMap<u64, String>) -> Text {
    match value {
        Value::Constant(0) => Text::Absent,
        Value::Constant(address) => strings
            .get(&address)
            .map_or(Text::Unknown, |literal| Text::Literal(literal.clone())),
        Value::Unknown | Value::Stack(_) | Value::Slot(_) => Text::Unknown,
    }
}

/// The depth as the `int` parameter receives it: the low 32 bits of the register.
fn depth(value: Value) -> Option<i32> {
    match value {
        Value::Constant(bits) => Some(bits as u32 as i32),
        Value::Unknown | Value::Stack(_) | Value::Slot(_) => None,
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
    use crate::{DefinitionName, DefinitionStatement};

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

    const LOG: u64 = 0x1400;
    const ITEM_CONSTRUCTOR: u64 = 0x1500;
    const PERSISTENT_READ: u64 = 0x1600;

    fn rule(code: Arm64) -> Option<DuplicateRule> {
        let start = code.start();

        existing_entry_rule(&code.bytes(), start)
    }

    /// The template shape that replaces an item: with a `false` last argument it logs, then
    /// destroys the item, builds it again and reads the later definition into it.
    #[test]
    fn a_body_that_rebuilds_the_existing_item_on_the_false_path_gives_last_wins() {
        let mut code = Arm64::at(0x1000);
        arm64!(code
            ; mov x19, x3
            ; mov x20, x1
            ; tbnz w4, #0, >rebuild
            ; bl extern LOG as usize // "Object with key: %s already exists"
            ; tbz w8, #31, >rebuild
            ; nop
            ; rebuild:
            ; ldr w24, [x19, #8]
            ; ldr x8, [x19]
            ; ldr x8, [x8]
            ; mov x0, x19
            ; blr x8 // the item's destructor
            ; mov x0, x19
            ; bl extern ITEM_CONSTRUCTOR as usize
            ; mov x0, x19
            ; ldr x8, [x0, #0x38]!
            ; ldr x8, [x8, #0x20]
            ; mov x1, x20
            ; blr x8 // CPersistent::Read
            ; ret
        );

        assert_eq!(rule(code), Some(DuplicateRule::LastWins));
    }

    /// An inline constructor: a store of the primary vtable builds the destroyed item again,
    /// and the read is a direct call.
    #[test]
    fn an_item_built_again_by_inline_stores_gives_last_wins() {
        let mut code = Arm64::at(0x1000);
        arm64!(code
            ; mov x20, x3
            ; mov x19, x1
            ; tbnz w4, #0, >rebuild
            ; rebuild:
            ; ldr x8, [x20]
            ; ldr x8, [x8]
            ; mov x0, x20
            ; blr x8 // the item's destructor
            ; stp xzr, xzr, [x20, #0x10]
            ; mov x0, x20
            ; str x9, [x0, #0x38]! // the vtable of the subobject at 0x38
            ; str x8, [x20] // the primary vtable
            ; mov x1, x19
            ; bl extern PERSISTENT_READ as usize
            ; ret
        );

        assert_eq!(rule(code), Some(DuplicateRule::LastWins));
    }

    /// The extendable shape: with a `false` last argument it reads the later definition into
    /// the item as it is.
    #[test]
    fn a_body_that_reads_into_the_existing_item_on_the_false_path_gives_merges() {
        let mut code = Arm64::at(0x1000);
        arm64!(code
            ; mov x19, x3
            ; mov x20, x1
            ; tbz w4, #0, >read
            ; ldr x8, [x19]
            ; ldr x8, [x8]
            ; mov x0, x19
            ; blr x8 // the destructor, only when the last argument is true
            ; ret
            ; read:
            ; ldr x8, [x19, #0x38]!
            ; ldr x2, [x8, #0x20]
            ; mov x0, x19
            ; mov x1, x20
            ; br x2 // CPersistent::Read
        );

        assert_eq!(rule(code), Some(DuplicateRule::Merges));
    }

    #[test]
    fn a_read_into_a_destroyed_item_that_is_not_built_again_establishes_no_rule() {
        let mut code = Arm64::at(0x1000);
        arm64!(code
            ; mov x19, x3
            ; mov x20, x1
            ; tbnz w4, #0, >destroy
            ; destroy:
            ; ldr x8, [x19]
            ; ldr x8, [x8]
            ; mov x0, x19
            ; blr x8
            ; mov x0, x19
            ; mov x1, x20
            ; bl extern PERSISTENT_READ as usize
            ; ret
        );

        assert_eq!(rule(code), None);
    }

    #[test]
    fn a_virtual_call_on_the_item_without_the_reader_establishes_no_rule() {
        let mut code = Arm64::at(0x1000);
        arm64!(code
            ; mov x19, x3
            ; tbz w4, #0, >query
            ; ret
            ; query:
            ; ldr x8, [x19]
            ; ldr x2, [x8, #0x10] // another virtual function of the item
            ; mov x0, x19
            ; mov w1, #3
            ; br x2
        );

        assert_eq!(rule(code), None);
    }

    #[test]
    fn a_body_that_does_not_test_its_last_argument_first_establishes_no_rule() {
        let mut code = Arm64::at(0x1000);
        arm64!(code; ldr x8, [x3]; ldr x8, [x8]; mov x0, x3; blr x8; ret);

        assert_eq!(rule(code), None);
    }

    #[test]
    fn an_item_held_only_in_a_caller_saved_register_is_lost_at_a_call() {
        let mut code = Arm64::at(0x1000);
        arm64!(code
            ; mov x9, x3
            ; tbnz w4, #0, >destroy
            ; destroy:
            ; bl extern LOG as usize
            ; ldr x8, [x9]
            ; ldr x8, [x8]
            ; mov x0, x9
            ; blr x8
            ; ret
        );

        assert_eq!(rule(code), None);
    }

    #[test]
    fn a_registry_with_no_template_database_has_no_followed_duplicate_rule() {
        // A custom loader, such as the ship behaviors' find-log-and-delete insert, has no
        // existing-entry handler for the method to read.
        assert_eq!(duplicate_rule(&[]), Err(DuplicateObstacle::NotFollowed));
    }

    #[test]
    fn template_databases_must_agree_on_a_read_rule() {
        assert_eq!(
            duplicate_rule(&[Some(DuplicateRule::Merges)]),
            Ok(DuplicateRule::Merges)
        );
        assert_eq!(
            duplicate_rule(&[Some(DuplicateRule::LastWins), None]),
            Err(DuplicateObstacle::UnreadBody)
        );
        assert_eq!(
            duplicate_rule(&[Some(DuplicateRule::LastWins), Some(DuplicateRule::Merges)]),
            Err(DuplicateObstacle::Several(2))
        );
    }

    fn form(name: DefinitionName) -> DefinitionForm {
        DefinitionForm {
            statement: DefinitionStatement::Root,
            name,
        }
    }

    #[test]
    fn naming_keeps_each_distinct_form_in_rule_order_and_counts_the_missing_rules() {
        let stated = [
            Some(form(DefinitionName::Key)),
            None,
            Some(form(DefinitionName::Field("id".into()))),
            Some(form(DefinitionName::Key)),
        ];

        assert_eq!(
            naming(&stated),
            Naming {
                forms: vec![
                    form(DefinitionName::Key),
                    form(DefinitionName::Field("id".into()))
                ],
                missing: 1,
            }
        );
    }
}
