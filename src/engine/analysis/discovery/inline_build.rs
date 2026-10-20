//! The address points that code stores into one object that it builds inline, and the functions
//! that it calls on the object's parts.
//!
//! The walk follows each branch from a start position with the object in one register, and keeps
//! the registers that hold the object plus an offset and the registers that hold an address that
//! `adrp` and `add` form. Where paths meet, a register keeps every object offset that it holds on
//! some path and a formed address only when every path holds it, and the walk visits the position
//! again until no path brings a new state, so the stores are those of any path. A walk that does
//! not settle within a bound is open. A call
//! writes the caller-saved registers. A branch out of the walked code or through a register, or
//! a store through the object that the walk cannot read, leaves stores unseen, so the build is
//! marked open; code that ends in a call ends in one that
//! does not return. Stores to memory that is not the object are
//! not followed, so a part built on the stack and copied in is not seen.
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
    /// The 8-byte words of the object that a store writes with a value that is not a formed
    /// address, by object offset.
    pub other_stores: BTreeSet<u64>,
    /// The targets of the direct calls that receive the object plus an offset in `x0`, by that
    /// offset.
    pub member_calls: BTreeMap<u64, BTreeSet<u64>>,
    /// Whether a path left the walked code other than by a return or the stop, so its later
    /// stores are not seen.
    pub open: bool,
}

impl InlineBuild {
    /// The offsets that one formed address and no other value is stored to, with that address.
    pub fn single_points(&self) -> BTreeMap<u64, u64> {
        self.points
            .iter()
            .filter(|(offset, _)| !self.other_stores.contains(offset))
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
/// the vtable address points (`is_address_point`) that the part's own constructor build `part`
/// stores, shifted by the part's `offset`. Other formed addresses that the part stores are not
/// points, and an offset where the loader stores any other value has none: the loader's store
/// replaces the part's. `None` unless the loader stores at least one of the points itself, or
/// when the part's constructor stores two values at one offset or another value at a point's
/// offset.
pub fn part_points(
    loader: &InlineBuild,
    offset: u64,
    part: &InlineBuild,
    is_address_point: impl Fn(u64) -> bool,
) -> Option<BTreeMap<u64, u64>> {
    let points = part.single_points();
    if loader.open || part.open || points.len() != part.points.len() {
        return None;
    }

    let shifted: BTreeMap<u64, u64> = points
        .into_iter()
        .filter(|&(_, point)| is_address_point(point))
        .map(|(at, point)| (at + offset, point))
        .collect();
    let mut agreed = 0;
    let mut kept = BTreeMap::new();

    for (at, point) in shifted {
        if loader.other_stores.contains(&at) {
            continue;
        }

        match loader.points.get(&at) {
            None => {}
            Some(stored) if *stored == BTreeSet::from([point]) => agreed += 1,
            Some(_) => continue,
        }

        kept.insert(at, point);
    }

    (agreed > 0).then_some(kept)
}

/// The register state of one path.
#[derive(Clone, Default, PartialEq)]
struct Registers {
    /// The object offsets that each register may hold the object's address plus, on some path.
    object: BTreeMap<usize, BTreeSet<u64>>,
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
    let mut arrived: BTreeMap<usize, Registers> = BTreeMap::new();
    let mut entry = Registers::default();
    entry.object.insert(register, BTreeSet::from([0]));
    let mut pending = vec![(start, entry)];
    let mut steps = 0;

    while let Some((position, registers)) = pending.pop() {
        steps += 1;
        if steps > rows.len() * VISITS {
            build.open = true;
            break;
        }
        let Some(row) = rows.get(position) else {
            // Code that ends in a call ends in a call that does not return, such as the
            // `_Unwind_Resume` of a landing pad.
            let ends_in_call =
                position == rows.len() && rows.last().is_some_and(|last| last.operation == "bl");
            build.open |= !ends_in_call;
            continue;
        };
        if Some(row.address) == stop {
            continue;
        }
        let mut registers = match arrived.get(&position) {
            Some(earlier) => {
                let merged = earlier.merged(&registers);
                if merged == *earlier {
                    continue;
                }
                merged
            }
            None => registers,
        };
        arrived.insert(position, registers.clone());

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
        if conditional || operation == "b" {
            match target {
                Some(target) => pending.push((target, registers.clone())),
                None => build.open = true,
            }
        }
        if operation == "br" {
            build.open = true;
        }
        if !matches!(operation, "b" | "br" | "ret") {
            pending.push((position + 1, registers));
        }
    }

    build
}

/// The visits of each position after which the walk gives up and marks the build open.
const VISITS: usize = 16;

impl Registers {
    /// The state where two paths meet: every object offset that either path may hold, and the
    /// formed addresses that both hold.
    fn merged(&self, other: &Self) -> Self {
        let mut object = self.object.clone();
        for (&register, offsets) in &other.object {
            object.entry(register).or_default().extend(offsets);
        }
        let formed = self
            .formed
            .iter()
            .filter(|(register, address)| other.formed.get(register) == Some(address))
            .map(|(&register, &address)| (register, address))
            .collect();

        Self { object, formed }
    }
}

/// Record the stores of formed addresses into the object and the calls on its parts.
fn record(row: &Instruction, registers: &Registers, build: &mut InlineBuild) {
    if row.operation == "bl"
        && let Some(target) = call_target(u32::from_le_bytes(row.bytes), row.address)
    {
        for &offset in registers.object.get(&0).into_iter().flatten() {
            build.member_calls.entry(offset).or_default().insert(target);
        }
    }

    let Some(store) = Store::of(row) else {
        // A store that the walk cannot read, such as one with a register index, may write the
        // object.
        build.open |= row.operation.starts_with("st")
            && memory_base(row).is_some_and(|base| registers.object.contains_key(&base));
        return;
    };
    let Some(bases) = registers.object.get(&store.base) else {
        return;
    };

    for at in bases.iter().map(|base| base.wrapping_add(store.offset)) {
        for (index, value) in store.values.iter().enumerate() {
            let offset = at.wrapping_add(store.width * index as u64);
            let formed = value
                .and_then(|value| registers.formed.get(&value))
                .filter(|_| store.width == 8);

            match formed {
                Some(&formed) => {
                    build.points.entry(offset).or_default().insert(formed);
                }
                None => {
                    let words = (offset & !7..offset + store.width).step_by(8);
                    build.other_stores.extend(words);
                }
            }
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
            registers.object.get(&source).cloned(),
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
            registers.object.get(&source).map(|offsets| {
                offsets
                    .iter()
                    .map(|offset| offset.wrapping_add(immediate))
                    .collect()
            }),
            registers
                .formed
                .get(&source)
                .map(|address| address + immediate),
        )
    });
    let written_back = Store::of(row)
        .filter(|store| store.writes_back)
        .and_then(|store| {
            let offsets = registers.object.get(&store.base)?;
            let advanced = offsets
                .iter()
                .map(|offset| offset.wrapping_add(store.advance))
                .collect::<BTreeSet<_>>();
            Some((store.base, advanced))
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
    /// The stored general register of each slot, `None` for the zero register or a vector
    /// register.
    values: Vec<Option<usize>>,
    /// The bytes of each slot.
    width: u64,
    base: usize,
    /// The offset from the base that the store writes.
    offset: u64,
    writes_back: bool,
    /// The amount that the base register advances by when the store writes back.
    advance: u64,
}

impl Store {
    fn of(row: &Instruction) -> Option<Self> {
        let narrow = match row.operation.as_str() {
            "str" | "stur" | "stp" => None,
            "strb" | "sturb" => Some(1),
            "strh" | "sturh" => Some(2),
            _ => return None,
        };

        let (values, address) = row.operands.split_once('[')?;
        let values: Vec<&str> = values.trim_end_matches(',').split(',').collect();
        let width = match (narrow, values.first()?.chars().next()?) {
            (Some(width), 'w') => width,
            (None, 'x' | 'd') => 8,
            (None, 'w' | 's') => 4,
            (None, 'q') => 16,
            _ => return None,
        };
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
            width,
            base: wide_register(base)?,
            offset,
            writes_back,
            advance,
        })
    }
}

/// The base register of a memory operand such as `[x19,#0x10]`.
fn memory_base(row: &Instruction) -> Option<usize> {
    let (_, address) = row.operands.split_once('[')?;
    let base = address.split([',', ']']).next()?;

    wide_register(base)
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
            ..InlineBuild::default()
        }
    }

    #[test]
    fn a_part_constructor_gives_its_points_only_when_the_loader_stores_agree() {
        let part = stored(&[(0, FIRST_POINT), (0x20, SECOND_POINT)]);
        let shifted = BTreeMap::from([(0x10, FIRST_POINT), (0x30, SECOND_POINT)]);
        let part_points = |loader: &InlineBuild, offset: u64, part: &InlineBuild| {
            part_points(loader, offset, part, |_| true)
        };

        assert_eq!(
            part_points(&stored(&[(0x10, FIRST_POINT)]), 0x10, &part),
            Some(shifted)
        );
        // The loader stores another value at one of the part's offsets, which keeps no point.
        assert_eq!(
            part_points(
                &stored(&[(0x10, FIRST_POINT), (0x30, FIRST_POINT)]),
                0x10,
                &part
            ),
            Some(BTreeMap::from([(0x10, FIRST_POINT)]))
        );
        // The loader stores none of the part's points.
        assert_eq!(
            part_points(&stored(&[(0x8, FIRST_POINT)]), 0x10, &part),
            None
        );
    }

    #[test]
    fn paths_that_store_different_values_at_one_offset_give_it_no_point() {
        let mut code = Arm64::at(START);
        arm64!(code; cbz x1, >second);
        code.address(8, FIRST_POINT);
        arm64!(code; b >store; second:);
        code.address(8, SECOND_POINT);
        arm64!(code; store:; str x8, [x19, #0x10]; stp q0, q0, [x19, #0x20]; ret);
        let rows = decode_arm64(&code.bytes(), START).unwrap();

        let build = inline_build(&rows, 0, 19, None);

        assert!(build.single_points().is_empty());
        assert_eq!(
            build.other_stores,
            BTreeSet::from([0x10, 0x20, 0x28, 0x30, 0x38])
        );
        let part = stored(&[(0, FIRST_POINT)]);
        assert_eq!(part_points(&build, 0x10, &part, |_| true), None);
    }

    #[test]
    fn a_store_that_the_walk_cannot_read_opens_the_build_and_one_path_s_alias_is_kept() {
        let rows = |code: Arm64| decode_arm64(&code.bytes(), START).unwrap();
        let mut indexed = Arm64::at(START);
        arm64!(indexed; str x8, [x19, x9]; strb w8, [x19, #8]; ret);
        let mut aliased = Arm64::at(START);
        arm64!(aliased;
            cbz x1, >alias;
            b >join;
            alias:;
            mov x20, x19;
            join:;
            str x8, [x20, #0x10];
            ret
        );

        let indexed = inline_build(&rows(indexed), 0, 19, None);
        let aliased = inline_build(&rows(aliased), 0, 19, None);

        assert!(indexed.open);
        // Only one path holds the object in x20; its store through x20 still counts.
        assert!(!aliased.open);
        assert_eq!(aliased.other_stores, BTreeSet::from([0x10]));
        // A byte store is read: it writes its word with another value than a point.
        let mut byte = Arm64::at(START);
        arm64!(byte; strb w8, [x19, #9]; ret);
        let byte = inline_build(&rows(byte), 0, 19, None);
        assert!(!byte.open);
        assert_eq!(byte.other_stores, BTreeSet::from([8]));
    }

    #[test]
    fn only_a_vtable_address_point_witnesses_the_agreement() {
        // The part stores a vtable point and a shared string; the loader stores only the string.
        let part = stored(&[(0, FIRST_POINT), (0x20, SECOND_POINT)]);
        let is_vtable = |point| point == FIRST_POINT;

        assert_eq!(
            part_points(&stored(&[(0x30, SECOND_POINT)]), 0x10, &part, is_vtable),
            None
        );
        assert_eq!(
            part_points(&stored(&[(0x10, FIRST_POINT)]), 0x10, &part, is_vtable),
            Some(BTreeMap::from([(0x10, FIRST_POINT)]))
        );
    }
}
