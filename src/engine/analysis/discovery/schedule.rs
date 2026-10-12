//! The database schedule: the table that the engine fills with one row for each database it
//! creates, initializes and destroys.
//!
//! The scheduling function sets a table owner from the stack pointer (`mov x19, sp`), then stores
//! literal values into 48-byte rows of six slots: the database name as a text literal, then the
//! create, destroy, optional, init and post-read-init functions. The fill ends at the first
//! instruction that is not one of its literal steps; the code after it schedules the rows. The
//! table starts at the lowest owner slot that holds a text literal.
//!
//! The scan tracks `adrp`, the 64-bit `add` and `ldr` of an immediate, and the stores. A register
//! move forgets its destination; M452 copies no slot value through one. A function address is often loaded from a `__got` slot, so it resolves only through
//! the resolved pointers. A row's database type is the class that its function symbols name; a
//! row joins the template candidate of that database. A row that joins none is a candidate for a
//! custom loader; a row whose functions name no single type is unresolved. A missing table owner, or a new value in it during the fill, is a gap, never an
//! empty table.
use std::collections::{BTreeMap, BTreeSet};

use super::Symbol;
use crate::engine::analysis::decode::{
    Instruction, add_immediate, adrp, is_control_transfer, ldr_immediate, number, written_registers,
};

/// The scheduling function, by the prefix of its demangled name.
pub const SCHEDULE_FUNCTION: &str = "NNullObjAndDatabaseInitUtil::SetupDatabases(";

/// The shared class whose functions a template database's rows hold, such as
/// `TGameDatabase<CExampleDatabase>::CreateInstance()`.
const GAME_DATABASE: &str = "TGameDatabase<";

/// Bytes in one row: six 8-byte slots.
const ROW: u64 = 48;

/// The frame pointer, which a stack-address move does not make a table owner.
const FRAME_POINTER: usize = 29;

/// The rows of the schedule, joined to the template candidates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleInventory {
    /// Rows in the table.
    pub rows: usize,
    /// The database type of each row that joins no template candidate, in table order.
    pub unjoined: Vec<String>,
    /// Rows whose functions name no single database type, so no join is established.
    pub unresolved: usize,
}

/// Why the schedule could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleGap {
    /// No table owner was set from the stack pointer before the first branch.
    NoTableOwner,
    /// The table owner took a new value during the fill.
    OwnerChanged,
    /// The fill stored no text literal into the table.
    NoRows,
    /// The scheduling function is missing, not unique, or cannot be decoded.
    Unreadable,
}

/// The facts of the executable that the schedule reads, besides the scheduling function.
pub struct ScheduleFacts<'a> {
    /// Symbols in address order.
    pub symbols: &'a [Symbol],
    /// Resolved pointer targets, keyed by the pointer's file address.
    pub pointers: &'a BTreeMap<u64, u64>,
    /// Literal strings, keyed by their file addresses.
    pub strings: &'a BTreeMap<u64, String>,
    /// The database type of every template candidate.
    pub templates: BTreeSet<&'a str>,
}

/// Read the schedule from the decoded scheduling function and join its rows to the candidates.
pub fn inventory(
    instructions: &[Instruction],
    facts: &ScheduleFacts<'_>,
) -> Result<ScheduleInventory, ScheduleGap> {
    let rows = table(instructions, facts)?;
    let databases: Vec<_> = rows
        .iter()
        .map(|slots| row_database(slots, facts.symbols))
        .collect();
    let unjoined = databases
        .iter()
        .flatten()
        .filter(|database| !facts.templates.contains(*database))
        .map(|database| (*database).to_owned())
        .collect();

    Ok(ScheduleInventory {
        rows: rows.len(),
        unjoined,
        unresolved: databases
            .iter()
            .filter(|database| database.is_none())
            .count(),
    })
}

/// The six slots of each row, from the first literal store to the end of the fill.
fn table(
    instructions: &[Instruction],
    facts: &ScheduleFacts<'_>,
) -> Result<Vec<[Option<u64>; 6]>, ScheduleGap> {
    let (owner, fill) = owner(instructions).ok_or(ScheduleGap::NoTableOwner)?;
    let slots = fill_slots(owner, fill, facts.pointers)?;
    let start = slots
        .iter()
        .find(|(_, value)| value.is_some_and(|value| facts.strings.contains_key(&value)))
        .map(|(&offset, _)| offset)
        .ok_or(ScheduleGap::NoRows)?;
    let end = slots
        .last_key_value()
        .map_or(start, |(&offset, _)| offset + 8);
    let rows = (end - start).div_ceil(ROW);

    Ok((0..rows)
        .map(|row| {
            std::array::from_fn(|slot| {
                slots
                    .get(&(start + row * ROW + slot as u64 * 8))
                    .copied()
                    .flatten()
            })
        })
        .collect())
}

/// The table owner and the instructions after the move that sets it. The move must come before
/// the first branch; a call before it, such as a stack probe, is harmless.
fn owner(instructions: &[Instruction]) -> Option<(usize, &[Instruction])> {
    for (index, instruction) in instructions.iter().enumerate() {
        let operation = instruction.operation.as_str();
        if operation == "mov"
            && let Some(("x", register)) = instruction
                .operands
                .strip_suffix(",sp")
                .map(|register| register.split_at(1))
            && let Ok(register) = register.parse::<usize>()
            && register != FRAME_POINTER
        {
            return Some((register, &instructions[index + 1..]));
        }
        if is_control_transfer(operation) && !matches!(operation, "bl" | "blr") {
            return None;
        }
    }

    None
}

/// The value stored at each owner offset during the fill; `None` where the value is not a known
/// literal.
fn fill_slots(
    owner: usize,
    fill: &[Instruction],
    pointers: &BTreeMap<u64, u64>,
) -> Result<BTreeMap<u64, Option<u64>>, ScheduleGap> {
    let mut registers: BTreeMap<usize, u64> = BTreeMap::new();
    let mut slots = BTreeMap::new();
    for instruction in fill {
        let word = u32::from_le_bytes(instruction.bytes);
        let operation = instruction.operation.as_str();
        let written = written_registers(operation, &instruction.operands);
        if written.contains(&owner) {
            return Err(ScheduleGap::OwnerChanged);
        }
        if let Some((destination, page)) = adrp(word, instruction.address) {
            registers.insert(destination, page);
        } else if let Some((destination, source, addend)) = add_immediate(word) {
            let value = registers.get(&source).map(|value| value + addend);
            set(&mut registers, destination, value);
        } else if let Some((destination, base, offset)) = ldr_immediate(word) {
            let value = registers
                .get(&base)
                .and_then(|address| pointers.get(&(address + offset)).copied());
            set(&mut registers, destination, value);
        } else if operation == "mov" {
            for register in written {
                registers.remove(&register);
            }
        } else if matches!(operation, "str" | "stp" | "stur") {
            let Some((sources, base, offset)) = store(&instruction.operands) else {
                break;
            };
            if base == format!("x{owner}") {
                let Some(offset) = number(offset) else {
                    break;
                };
                for (index, source) in sources.iter().enumerate() {
                    let value = match *source {
                        "xzr" => Some(0),
                        source => source
                            .strip_prefix('x')
                            .and_then(|register| registers.get(&register.parse().ok()?))
                            .copied(),
                    };
                    slots.insert(offset + 8 * index as u64, value);
                }
            } else if !matches!(base, "sp" | "x29") {
                break;
            }
        } else {
            break;
        }
    }

    Ok(slots)
}

fn set(registers: &mut BTreeMap<usize, u64>, register: usize, value: Option<u64>) {
    match value {
        Some(value) => registers.insert(register, value),
        None => registers.remove(&register),
    };
}

/// The source registers, base register and offset of a store with no writeback, such as
/// `x9,x8,[x19,#0x60]`.
fn store(operands: &str) -> Option<(Vec<&str>, &str, &str)> {
    let (sources, memory) = operands.split_once('[')?;
    let memory = memory.strip_suffix(']')?;
    let (base, offset) = memory.split_once(',').unwrap_or((memory, "#0"));
    let sources = sources
        .split(',')
        .filter(|source| !source.is_empty())
        .collect();

    Some((sources, base, offset))
}

/// The one database type that the row's function symbols name.
fn row_database<'a>(slots: &[Option<u64>; 6], symbols: &'a [Symbol]) -> Option<&'a str> {
    let types: BTreeSet<&str> = slots[1..]
        .iter()
        .flatten()
        .filter_map(|&address| slot_database(address, symbols))
        .collect();

    single(types)
}

/// The one database type that the symbols at `address` name.
fn slot_database(address: u64, symbols: &[Symbol]) -> Option<&str> {
    let start = symbols.partition_point(|symbol| symbol.address < address);
    let types: BTreeSet<Option<&str>> = symbols[start..]
        .iter()
        .take_while(|symbol| symbol.address == address)
        .map(|symbol| database_type(&symbol.name))
        .collect();

    single(types).flatten()
}

/// The database type of a function such as `TGameDatabase<CExampleDatabase>::CreateInstance()`
/// or `CExampleDatabase::CreateInstance()`.
fn database_type(function: &str) -> Option<&str> {
    let (qualified, _) = function.split_once('(')?;
    let (class, _) = qualified.rsplit_once("::")?;

    Some(
        class
            .strip_prefix(GAME_DATABASE)
            .and_then(|class| class.strip_suffix('>'))
            .unwrap_or(class),
    )
}

fn single<T>(values: BTreeSet<T>) -> Option<T> {
    let mut values = values.into_iter();
    let (Some(value), None) = (values.next(), values.next()) else {
        return None;
    };

    Some(value)
}
