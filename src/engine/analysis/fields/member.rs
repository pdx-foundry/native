//! Shared bounds, token domains and receiver translation for member delegates.
use super::{Condition, DispatchInput, FieldGap, PathOutcome, ReaderJoin, TokenPath, Value};
use crate::engine::analysis::decode::{
    Instruction, add_immediate, adrp, call_target, is_branch, number, sub_immediate, wide_register,
    written_registers,
};
use crate::engine::analysis::stop::Unresolved;
use std::collections::{BTreeMap, BTreeSet};

/// Follow one member edge, preserving its token domain and owner-relative provenance.
pub(crate) fn follow_member(
    input: &DispatchInput<'_>,
    parent: &TokenPath,
    chain: &[String],
) -> Result<(Vec<TokenPath>, Vec<FieldGap>), Unresolved> {
    let PathOutcome::Reader(ReaderJoin::Joined {
        callee, arguments, ..
    }) = &parent.outcome
    else {
        return Err(Unresolved::new("delegate-receiver"));
    };
    let Some(Value::Owner(offset)) = arguments.get("x0") else {
        return Err(Unresolved::new("delegate-receiver"));
    };
    if chain.len() >= 8 || chain.contains(callee) {
        return Err(Unresolved::new("grammar-delegation-limit"));
    }
    // The known address point belongs to the root object, not an embedded delegate.
    let owner_vtable = input.owner_vtable.filter(|_| *offset == 0);
    let mut dispatch = input.with_owner_vtable(owner_vtable);
    dispatch.scope = crate::engine::analysis::readers::scope_argument(callee, arguments).cloned();
    let (children, gaps) = super::explore_member(&dispatch, callee);
    let children = children
        .into_iter()
        .filter_map(|mut child| {
            translate(&mut child, *offset);
            inherit_path(
                child,
                parent.domain,
                &parent.conditions,
                &parent.instructions,
            )
        })
        .collect();
    Ok((children, gaps))
}

/// Intersect a delegated path with its caller and preserve caller provenance.
pub(super) fn inherit_path(
    mut child: TokenPath,
    domain: [i64; 2],
    conditions: &[Condition],
    instructions: &[u64],
) -> Option<TokenPath> {
    child.domain = [
        child.domain[0].max(domain[0]),
        child.domain[1].min(domain[1]),
    ];
    if child.domain[0] > child.domain[1] {
        return None;
    }
    child.conditions.splice(0..0, conditions.iter().cloned());
    child
        .instructions
        .splice(0..0, instructions.iter().copied());
    Some(child)
}

fn translate(path: &mut TokenPath, offset: i64) {
    for Condition { value: tested, .. } in &mut path.conditions {
        if let Some(tested) = tested {
            translate_value(tested, offset);
        }
    }
    if let PathOutcome::Reader(ReaderJoin::Stored {
        destination: Some(destination),
        ..
    }) = &mut path.outcome
    {
        *destination += offset;
    }
    if let PathOutcome::Reader(ReaderJoin::Joined { arguments, .. }) = &mut path.outcome {
        for argument in arguments.values_mut() {
            translate_value(argument, offset);
        }
    }
}

fn translate_value(value: &mut Value, offset: i64) {
    match value {
        Value::Owner(at) => *at += offset,
        Value::Load(base, _) | Value::Offset(base, _) | Value::EqualsAny(base, _) => {
            translate_value(base, offset)
        }
        Value::Indexed(base, index, _) | Value::SumProduct(base, index, _) => {
            translate_value(base, offset);
            translate_value(index, offset);
        }
        _ => {}
    }
}
/// A constructor that installs a vtable and saves its wrapped owner in the next pointer slot.
pub(crate) fn serializer_owner_slot(
    rows: &[crate::engine::analysis::decode::Instruction],
) -> Option<i64> {
    use crate::engine::analysis::references::shapes::{Shape, canonical};
    let lines = canonical(rows, &Default::default());
    Shape::parse("adrp xr0,PAGE\nadd xr0,xr0,G = {vtable}\nstp xr0,x1,[x0]\nret")
        .matches(&lines)
        .map(|_| 8)
}

/// The two calls that read a persistent object from a reader: `CPersistent::Read(CReader&)`, with
/// the object in `x0` and the reader in `x1`, and `CReader::Read(CPersistent&)`, with them swapped.
pub(crate) struct PersistentReads {
    pub persistent: u64,
    pub reader: u64,
}

/// The stack serializer that an owner's `Read(CReader&)` reads through: the vtable address point
/// stored with the owner, which arrives in `x0`, in the next pointer slot, and the offset of that
/// slot. The code before the function's first call must run straight through, keep both words
/// and the entry reader, and pass the object and the reader to one of `reads`.
pub(crate) fn serializer_read(rows: &[Instruction], reads: &PersistentReads) -> Option<(u64, i64)> {
    const STACK_POINTER: usize = 31;
    let mut addresses = BTreeMap::new();
    let mut stack = BTreeMap::from([(STACK_POINTER, 0)]);
    let mut owner_in_x0 = true;
    let mut readers = BTreeSet::from([1]);
    let mut serializers = BTreeMap::new();

    for row in rows {
        let word = u32::from_le_bytes(row.bytes);

        if row.operation == "bl" {
            let target = call_target(word, row.address)?;
            let (object, reader) = if target == reads.persistent {
                (0, 1)
            } else if target == reads.reader {
                (1, 0)
            } else {
                return None;
            };
            let point = serializers.get(stack.get(&object)?).copied()?;

            return readers.contains(&reader).then_some((point, 8));
        }
        // An authenticated call or return is also a transfer that the walk does not follow.
        if is_branch(&row.operation)
            || ["bl", "br", "ret"]
                .iter()
                .any(|op| row.operation.starts_with(op))
        {
            return None;
        }
        if row.operation.starts_with("st") {
            let (base, offset, width) = store(row)?;
            let at = stack.get(&base)? + offset;

            serializers.retain(|&object, _| at + width <= object || at >= object + 16);
            if row.operation == "stp"
                && owner_in_x0
                && let Some(point) = owner_store(row, &addresses)
            {
                serializers.insert(at, point);
            }
        }
        if row.operands.contains('!') || row.operands.contains("],") {
            return None;
        }

        let page = adrp(word, row.address);
        let moved = add_immediate(word)
            .map(|(destination, source, addend)| (destination, source, addend as i64))
            .or_else(|| {
                sub_immediate(word).map(|(destination, source, subtrahend)| {
                    (destination, source, -(subtrahend as i64))
                })
            });
        let moved_address = moved.and_then(|(destination, source, addend)| {
            let base = addresses.get(&source)?;
            Some((destination, base.wrapping_add_signed(addend)))
        });
        let moved_stack = moved.and_then(|(destination, source, addend)| {
            Some((destination, stack.get(&source)? + addend))
        });
        let written = written_registers(&row.operation, &row.operands);

        if written.contains(&0) {
            owner_in_x0 = false;
        }
        let copied_reader = copied_register(row).is_some_and(|source| readers.contains(&source));

        for register in &written {
            addresses.remove(register);
            stack.remove(register);
            readers.remove(register);
        }
        if copied_reader {
            readers.extend(written);
        }
        if let Some((register, page)) = page {
            addresses.insert(register, page);
        }
        if let Some((destination, address)) = moved_address {
            addresses.insert(destination, address);
        }
        if moved.is_some_and(|(destination, ..)| destination == STACK_POINTER) {
            // Offsets saved against the old stack pointer no longer hold.
            if !serializers.is_empty() {
                return None;
            }
            stack = BTreeMap::from([(STACK_POINTER, 0)]);
        } else if let Some((destination, offset)) = moved_stack {
            stack.insert(destination, offset);
        }
    }

    None
}

/// The source of a 64-bit register copy, `mov xd, xs`.
fn copied_register(row: &Instruction) -> Option<usize> {
    let (_, source) = row.operands.split_once(',')?;

    (row.operation == "mov").then(|| wide_register(source))?
}

/// The base register, offset and width of a store with an immediate offset, or `None` for
/// another store.
fn store(row: &Instruction) -> Option<(usize, i64, i64)> {
    let width = match row.operation.as_str() {
        "strb" | "sturb" => 1,
        "strh" | "sturh" => 2,
        "str" | "stur" => register_width(&row.operands)?,
        "stp" => 2 * register_width(&row.operands)?,
        _ => return None,
    };
    let (_, address) = row.operands.split_once('[')?;
    let address = address.split_once(']')?.0;
    let (base, offset) = match address.split_once(",#") {
        Some((base, offset)) => (base, number(offset)? as i64),
        None => (address, 0),
    };
    let base = if base == "sp" {
        31
    } else {
        wide_register(base)?
    };

    Some((base, offset, width))
}

/// The bytes that the first register operand of a store holds.
fn register_width(operands: &str) -> Option<i64> {
    match operands.chars().next()? {
        'x' | 'd' => Some(8),
        'w' | 's' => Some(4),
        'q' => Some(16),
        _ => None,
    }
}

/// The address point of `stp point, x0, [...]`, where a register in `addresses` holds the point.
fn owner_store(row: &Instruction, addresses: &BTreeMap<usize, u64>) -> Option<u64> {
    let (first, rest) = row.operands.split_once(',')?;
    rest.strip_prefix("x0,[")?;

    addresses.get(&wide_register(first)?).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::{
        assembler::arm64,
        declarations::Function,
        discovery::{Symbol, SymbolIndex},
    };
    use std::collections::BTreeMap;

    #[test]
    fn member_chain_translates_owner_offsets_and_retains_parent_domain() {
        let names = [
            "A::ReadMember(CReader&, int)",
            "B::ReadMember(CReader&, int)",
            "C::ReadMember(CReader&, int)",
            "CReader::Read(int&)",
        ];
        let symbols: Vec<_> = names
            .iter()
            .enumerate()
            .map(|(index, name)| Symbol {
                name: (*name).into(),
                address: 0x1000 + index as u64 * 0x1000,
            })
            .collect();
        let functions = BTreeMap::from([
            (
                0x1000,
                Function {
                    address: 0x1000,
                    code: arm64!(at 0x1000; add x0, x0, #0x10; b extern 0x2000),
                },
            ),
            (
                0x2000,
                Function {
                    address: 0x2000,
                    code: arm64!(at 0x2000; add x0, x0, #0x20; b extern 0x3000),
                },
            ),
            (
                0x3000,
                Function {
                    address: 0x3000,
                    code: arm64!(at 0x3000; add x8, x0, #8; mov x0, x1; mov x1, x8; b extern 0x4000),
                },
            ),
        ]);
        let readers = super::super::KeyReaders::default();
        let symbol_index = SymbolIndex::new(&symbols);
        let input =
            DispatchInput::command(&functions, &symbols, &symbol_index, &[], 0x38, &readers);
        let (mut paths, _) = super::super::explore_member(&input, names[0]);
        paths[0].domain = [7, 7];
        let chain = vec![names[0].into()];
        let (children, _) = follow_member(&input, &paths[0], &chain).unwrap();
        let (leaves, _) =
            follow_member(&input, &children[0], &[names[0].into(), names[1].into()]).unwrap();
        assert_eq!(leaves[0].domain, [7, 7]);
        assert_eq!(
            crate::engine::analysis::readers::destination(match &leaves[0].outcome {
                PathOutcome::Reader(join) => join,
                _ => panic!(),
            }),
            Some(0x38)
        );
        assert_eq!(leaves[0].instructions.len(), 8);
        assert_eq!(
            follow_member(&input, &paths[0], &[names[1].into()])
                .unwrap_err()
                .reason,
            "grammar-delegation-limit"
        );
        assert_eq!(
            follow_member(&input, &paths[0], &vec![names[0].into(); 8])
                .unwrap_err()
                .reason,
            "grammar-delegation-limit"
        );
    }

    #[test]
    fn serializer_read_requires_the_saved_owner_and_reader_at_the_read() {
        const PERSISTENT: u64 = 0x5000;
        const READER: u64 = 0x6000;
        let reads = PersistentReads {
            persistent: PERSISTENT,
            reader: READER,
        };
        let read = |code: Vec<u8>| {
            let rows = crate::engine::analysis::decode::decode_arm64(&code, 0x1000).unwrap();
            serializer_read(&rows, &reads)
        };
        let prefix = |code: &mut crate::engine::analysis::assembler::Arm64| {
            arm64!(code;
                sub sp, sp, #0x40;
                adrp x8, extern 0x8000;
                add x8, x8, #0x10; // the serializer's address point
                stp x8, x0, [sp, #8]
            );
        };
        let mut persistent = crate::engine::analysis::assembler::Arm64::at(0x1000);
        prefix(&mut persistent);
        arm64!(persistent; add x0, sp, #8; bl extern 0x5000);
        let mut reader = crate::engine::analysis::assembler::Arm64::at(0x1000);
        prefix(&mut reader);
        arm64!(reader; mov x8, x1; add x1, sp, #8; mov x0, x8; bl extern 0x6000);

        assert_eq!(read(persistent.bytes()), Some((0x8010, 8)));
        assert_eq!(read(reader.bytes()), Some((0x8010, 8)));

        for (control, rest) in [
            (
                "owner slot overwritten",
                arm64!(at 0x1010; str xzr, [sp, #16]; add x0, sp, #8; bl extern 0x5000),
            ),
            (
                "vtable overwritten",
                arm64!(at 0x1010; strb wzr, [sp, #8]; add x0, sp, #8; bl extern 0x5000),
            ),
            (
                "another receiver",
                arm64!(at 0x1010; add x0, sp, #16; bl extern 0x5000),
            ),
            (
                "another reader",
                arm64!(at 0x1010; mov x1, x2; add x0, sp, #8; bl extern 0x5000),
            ),
            (
                "another call first",
                arm64!(at 0x1010; add x0, sp, #8; bl extern 0x7000; bl extern 0x5000),
            ),
            (
                "a branch first",
                arm64!(at 0x1010; cbz x2, extern 0x1020; add x0, sp, #8; bl extern 0x5000),
            ),
            (
                "an indirect call first",
                arm64!(at 0x1010; mov x19, x1; blr x9; mov x1, x19; add x0, sp, #8; bl extern 0x5000),
            ),
            (
                "a store at a register offset",
                arm64!(at 0x1010; mov x9, #16; str xzr, [sp, x9]; add x0, sp, #8; bl extern 0x5000),
            ),
        ] {
            let mut code = crate::engine::analysis::assembler::Arm64::at(0x1000);
            prefix(&mut code);
            let mut code = code.bytes();
            code.extend(rest);

            assert_eq!(read(code), None, "{control}");
        }
    }
}
