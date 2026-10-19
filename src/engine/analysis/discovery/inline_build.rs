//! The address points that code stores into one object that it builds inline, and the functions
//! that it calls on the object's parts.
//!
//! The walk follows each branch from a start position with the object in one register, and keeps
//! the registers that hold the object plus an offset and the registers that hold an address that
//! `adrp` and `add` form. It visits each position once, with the state of the first path that
//! reaches it. A call writes the caller-saved registers. Stores to memory that is not the object
//! are not followed, so a part built on the stack and copied in is not seen.
use std::collections::{BTreeMap, BTreeSet};

use crate::engine::analysis::decode::{
    Instruction, adrp, call_target, number, wide_register, written_registers,
};

/// What code stores into one object and calls on its parts.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct InlineBuild {
    /// Each formed address stored into the object, by object offset. An offset that two paths
    /// store different values to keeps both.
    pub points: BTreeMap<u64, BTreeSet<u64>>,
    /// The targets of the direct calls that receive the object plus an offset in `x0`, by that
    /// offset.
    pub member_calls: BTreeMap<u64, BTreeSet<u64>>,
}

impl InlineBuild {
    /// The offsets that one value is stored to, with that value.
    pub fn single_points(&self) -> BTreeMap<u64, u64> {
        self.points
            .iter()
            .filter_map(
                |(&offset, values)| match values.iter().collect::<Vec<_>>()[..] {
                    [&value] => Some((offset, value)),
                    _ => None,
                },
            )
            .collect()
    }
}

/// The address points of a part of an object that `loader` builds inline, as object offsets:
/// the points that the part's own constructor build `part` stores, shifted by the part's
/// `offset`. `None` unless the loader stores at least one of those points itself and stores no
/// other value at any of their offsets, or when the part's constructor stores two values at one
/// offset.
pub fn part_points(
    loader: &InlineBuild,
    offset: u64,
    part: &InlineBuild,
) -> Option<BTreeMap<u64, u64>> {
    let points = part.single_points();
    if points.len() != part.points.len() {
        return None;
    }

    let shifted: BTreeMap<u64, u64> = points
        .into_iter()
        .map(|(at, point)| (at + offset, point))
        .collect();
    let mut agreed = 0;
    for (at, point) in &shifted {
        match loader.points.get(at) {
            None => {}
            Some(stored) if stored.len() == 1 && stored.contains(point) => agreed += 1,
            Some(_) => return None,
        }
    }

    (agreed > 0).then_some(shifted)
}

/// The register state of one path.
#[derive(Clone, Default)]
struct Registers {
    /// The object offset that each register holds the object's address plus.
    object: BTreeMap<usize, u64>,
    /// The address that each register holds from `adrp` and `add`.
    formed: BTreeMap<usize, u64>,
}

/// What `rows` stores into the object that `register` holds at position `start`, on every path
/// up to a return, a branch out of `rows`, or the instruction at `stop`.
pub fn inline_build(
    rows: &[Instruction],
    start: usize,
    register: usize,
    stop: Option<u64>,
) -> InlineBuild {
    let positions: BTreeMap<u64, usize> = rows
        .iter()
        .enumerate()
        .map(|(position, row)| (row.address, position))
        .collect();
    let mut build = InlineBuild::default();
    let mut visited = BTreeSet::new();
    let mut entry = Registers::default();
    entry.object.insert(register, 0);
    let mut pending = vec![(start, entry)];

    while let Some((position, mut registers)) = pending.pop() {
        let Some(row) = rows.get(position) else {
            continue;
        };
        if Some(row.address) == stop || !visited.insert(position) {
            continue;
        }

        record(row, &registers, &mut build);
        step(row, &mut registers);

        let operation = row.operation.as_str();
        let target = row
            .operands
            .rsplit(',')
            .next()
            .and_then(number)
            .and_then(|target| positions.get(&target).copied());
        let conditional =
            operation.starts_with("b.") || matches!(operation, "cbz" | "cbnz" | "tbz" | "tbnz");
        if (conditional || operation == "b")
            && let Some(target) = target
        {
            pending.push((target, registers.clone()));
        }
        if !matches!(operation, "b" | "br" | "ret") {
            pending.push((position + 1, registers));
        }
    }

    build
}

/// Record the stores of formed addresses into the object and the calls on its parts.
fn record(row: &Instruction, registers: &Registers, build: &mut InlineBuild) {
    if row.operation == "bl"
        && let Some(target) = call_target(u32::from_le_bytes(row.bytes), row.address)
        && let Some(&offset) = registers.object.get(&0)
    {
        build.member_calls.entry(offset).or_default().insert(target);
    }

    let Some(store) = Store::of(row) else {
        return;
    };
    let Some(&base) = registers.object.get(&store.base) else {
        return;
    };
    let at = base.wrapping_add(store.offset);

    for (index, value) in store.values.iter().enumerate() {
        if let Some(&formed) = value.and_then(|value| registers.formed.get(&value)) {
            let offset = at.wrapping_add(8 * index as u64);
            build.points.entry(offset).or_default().insert(formed);
        }
    }
}

/// Apply the register writes of `row` to `registers`.
fn step(row: &Instruction, registers: &mut Registers) {
    let word = u32::from_le_bytes(row.bytes);
    let operands: Vec<&str> = row.operands.split(',').collect();
    let copied = match (row.operation.as_str(), operands.as_slice()) {
        ("mov", [destination, source]) => wide_register(destination).zip(wide_register(source)),
        _ => None,
    }
    .map(|(destination, source)| {
        (
            destination,
            registers.object.get(&source).copied(),
            registers.formed.get(&source).copied(),
        )
    });
    let added = match (row.operation.as_str(), operands.as_slice()) {
        ("add", [destination, source, immediate]) => wide_register(destination)
            .zip(wide_register(source))
            .zip(number(immediate)),
        _ => None,
    }
    .map(|((destination, source), immediate)| {
        (
            destination,
            registers
                .object
                .get(&source)
                .map(|offset| offset + immediate),
            registers
                .formed
                .get(&source)
                .map(|address| address + immediate),
        )
    });
    let written_back = Store::of(row)
        .filter(|store| store.writes_back)
        .and_then(|store| {
            let offset = registers.object.get(&store.base)?;
            Some((store.base, offset.wrapping_add(store.advance)))
        });
    let page = adrp(word, row.address);

    for register in written_registers(&row.operation, &row.operands) {
        registers.object.remove(&register);
        registers.formed.remove(&register);
    }
    for (destination, object, formed) in copied.into_iter().chain(added) {
        registers
            .object
            .extend(object.map(|offset| (destination, offset)));
        registers
            .formed
            .extend(formed.map(|address| (destination, address)));
    }
    registers.object.extend(written_back);
    registers.formed.extend(page);
}

/// One `str`, `stur` or `stp` of 64-bit registers.
struct Store {
    /// The stored register of each 8-byte slot, `None` for the zero register.
    values: Vec<Option<usize>>,
    base: usize,
    /// The offset from the base that the store writes.
    offset: u64,
    writes_back: bool,
    /// The amount that the base register advances by when the store writes back.
    advance: u64,
}

impl Store {
    fn of(row: &Instruction) -> Option<Self> {
        if !matches!(row.operation.as_str(), "str" | "stur" | "stp") {
            return None;
        }

        let (values, address) = row.operands.split_once('[')?;
        let values: Vec<&str> = values.trim_end_matches(',').split(',').collect();
        if !values.iter().all(|value| value.starts_with('x')) {
            return None;
        }
        let (inside, after) = address.split_once(']')?;
        let (base, offset) = inside.split_once(',').unwrap_or((inside, "#0"));
        let offset = number(offset)?;
        let post = after.strip_prefix(',').map(number);
        let (offset, writes_back, advance) = match (after, post) {
            ("!", _) => (offset, true, offset),
            (_, Some(Some(advance))) => (offset, true, advance),
            ("", None) => (offset, false, 0),
            _ => return None,
        };

        Some(Self {
            values: values.iter().map(|value| wide_register(value)).collect(),
            base: wide_register(base)?,
            offset,
            writes_back,
            advance,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::assembler::{Arm64, arm64};
    use crate::engine::analysis::decode::decode_arm64;

    const START: u64 = 0x1000;
    const FIRST_POINT: u64 = 0x5_0010;
    const SECOND_POINT: u64 = 0x5_0110;
    const MEMBER: u64 = 0x9000;

    #[test]
    fn stores_through_copies_and_writeback_are_at_their_object_offsets() {
        let mut code = Arm64::at(START);
        code.address(8, FIRST_POINT);
        arm64!(code; str x8, [x19, #0x10]; mov x25, x19; str x8, [x25, #0xe0]!);
        code.address(9, SECOND_POINT);
        arm64!(code; stp x8, x9, [x25, #-0x38]; add x0, x19, #0x10);
        code.call(MEMBER);
        // The call writes x8 and x9, so the later stores hold no formed address.
        arm64!(code; str x8, [x19, #0x20]; ret);
        let rows = decode_arm64(&code.bytes(), START).unwrap();

        let build = inline_build(&rows, 0, 19, None);

        assert_eq!(
            build.single_points(),
            BTreeMap::from([
                (0x10, FIRST_POINT),
                (0xa8, FIRST_POINT),
                (0xb0, SECOND_POINT),
                (0xe0, FIRST_POINT),
            ])
        );
        assert_eq!(
            build.member_calls,
            BTreeMap::from([(0x10, BTreeSet::from([MEMBER]))])
        );
    }

    fn stored(points: &[(u64, u64)]) -> InlineBuild {
        InlineBuild {
            points: points
                .iter()
                .map(|&(at, point)| (at, BTreeSet::from([point])))
                .collect(),
            member_calls: BTreeMap::new(),
        }
    }

    #[test]
    fn a_part_constructor_gives_its_points_only_when_the_loader_stores_agree() {
        let part = stored(&[(0, FIRST_POINT), (0x20, SECOND_POINT)]);
        let shifted = BTreeMap::from([(0x10, FIRST_POINT), (0x30, SECOND_POINT)]);

        assert_eq!(
            part_points(&stored(&[(0x10, FIRST_POINT)]), 0x10, &part),
            Some(shifted)
        );
        // The loader stores another value at one of the part's offsets.
        assert_eq!(
            part_points(
                &stored(&[(0x10, FIRST_POINT), (0x30, FIRST_POINT)]),
                0x10,
                &part
            ),
            None
        );
        // The loader stores none of the part's points.
        assert_eq!(
            part_points(&stored(&[(0x8, FIRST_POINT)]), 0x10, &part),
            None
        );
    }
}
