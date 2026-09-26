use crate::AnalysisError;
use crate::engine::analysis::{
    discovery::{CandidateRecord, Symbol},
    fields::{DataSection, FieldInput, Function, ObjectReader},
};
use object::{Object, ObjectSection, SectionKind};
use std::collections::{BTreeMap, BTreeSet};

/// Read the selected root, the shared engine token constructor and the read-only data that holds
/// jump tables from the same verified buffer.
pub(in crate::binding) fn read(
    bytes: &[u8],
    symbols: &[Symbol],
    strings: &BTreeMap<u64, String>,
    pointers: &BTreeMap<u64, u64>,
    bound_slots: &BTreeSet<u64>,
    selection: CandidateRecord,
) -> Result<FieldInput, AnalysisError> {
    let mut functions = Vec::new();
    let mut gaps = Vec::new();
    let root = format!("{}::ReadMember(CReader&, int)", selection.owner_candidate);
    for name in [
        root.as_str(),
        "CPersistent::ReadMember(CReader&, int)",
        "GetTokenArray()",
    ] {
        let starts: std::collections::BTreeSet<_> = symbols
            .iter()
            .filter(|s| s.name == name)
            .map(|s| s.address)
            .collect();
        if starts.len() != 1 {
            gaps.push(format!("missing or ambiguous function {name}"));
            continue;
        }
        let start = *starts.first().unwrap();
        let Some(end) = symbols
            .iter()
            .filter(|s| s.address > start)
            .map(|s| s.address)
            .min()
        else {
            gaps.push(format!("unbounded function {name}"));
            continue;
        };
        let length = end - start;
        if length == 0 || length > 1024 * 1024 || !length.is_multiple_of(4) {
            gaps.push(format!("unsupported function extent {name}"));
            continue;
        }
        functions.push(Function {
            name: name.into(),
            address: start,
            code: super::code_range(bytes, start, length)?,
        });
    }
    let objects = collect_objects(bytes, symbols, pointers, bound_slots, &mut functions)?;
    if !objects.is_empty() {
        collect_owner_methods(
            bytes,
            symbols,
            &selection.owner_candidate,
            &mut functions,
            &mut gaps,
        )?;
    }
    Ok(FieldInput {
        objects,
        selection,
        symbols: symbols.to_vec(),
        functions,
        strings: strings.clone(),
        read_only_data: read_only_data(bytes)?,
        gaps,
    })
}

fn read_only_data(bytes: &[u8]) -> Result<Vec<DataSection>, AnalysisError> {
    let slice = super::selected_slice(bytes).map_err(|_| AnalysisError::InvalidRange)?;
    let file = object::File::parse(slice).map_err(|_| AnalysisError::InvalidRange)?;
    file.sections()
        .filter(|section| section.kind() == SectionKind::ReadOnlyData)
        .map(|section| {
            let bytes = section.data().map_err(|_| AnalysisError::InvalidRange)?;
            Ok(DataSection {
                address: section.address(),
                bytes: bytes.to_vec(),
            })
        })
        .collect()
}

/// Follow direct constructor calls only; a symbol alone does not attach a nested class to a field.
fn collect_objects(
    bytes: &[u8],
    symbols: &[Symbol],
    pointers: &BTreeMap<u64, u64>,
    bound_slots: &BTreeSet<u64>,
    functions: &mut Vec<Function>,
) -> Result<Vec<ObjectReader>, AnalysisError> {
    let Some(root) = functions.first() else {
        return Ok(Vec::new());
    };
    let rows = crate::engine::analysis::decode::decode_arm64(&root.code, root.address)
        .map_err(|_| AnalysisError::InvalidRange)?;
    let calls: BTreeSet<_> = rows
        .iter()
        .filter(|row| row.operation == "bl")
        .filter_map(|row| u64::from_str_radix(row.operands.trim_start_matches("#0x"), 16).ok())
        .collect();
    let classes: BTreeSet<_> = symbols
        .iter()
        .filter(|symbol| calls.contains(&symbol.address))
        .filter_map(|symbol| {
            let (class, method) = symbol.name.split_once("::")?;
            method
                .starts_with(&format!("{class}("))
                .then_some(class.to_owned())
        })
        .collect();
    if classes.is_empty() {
        return Ok(Vec::new());
    }
    let data = super::language::constant_data(bytes, pointers, bound_slots)?;
    let mut objects = Vec::new();
    for class in classes {
        let member = format!("{class}::ReadMember(CReader&, int)");
        let Some(symbol) = symbols.iter().find(|symbol| symbol.name == member) else {
            continue;
        };
        let Some(vtable) = super::families::vtable_group(symbols, &data, &class) else {
            continue;
        };
        let reads: BTreeSet<_> = symbols
            .iter()
            .filter(|symbol| {
                symbol.name == "CPersistent::Read(CReader&)"
                    && vtable.slots.contains_key(&symbol.address)
            })
            .map(|symbol| symbol.address)
            .collect();
        if reads.len() != 1 {
            continue;
        }
        let end = symbols
            .iter()
            .map(|other| other.address)
            .filter(|address| *address > symbol.address)
            .min();
        let Some(end) = end.filter(|end| end - symbol.address <= 1024 * 1024) else {
            continue;
        };
        functions.push(Function {
            name: member,
            address: symbol.address,
            code: super::code_range(bytes, symbol.address, end - symbol.address)?,
        });
        let mut slots = BTreeMap::new();
        for point in vtable.address_points.values() {
            let end = symbols
                .iter()
                .map(|symbol| symbol.address)
                .filter(|address| address > point)
                .min()
                .unwrap_or(*point);
            for at in (*point..end).step_by(8) {
                if let Some(value) = data.read(at, 8) {
                    slots.insert(at, value);
                }
            }
        }
        let insert: Vec<_> = symbols.iter().filter(|symbol| symbol.name == format!("{class}*& CPdxArray<{class}*, int>::InsertAtEmplace<{class}* const&>(int, {class}* const&)"))
            .map(|symbol| symbol.address).collect();
        let data_offset = array_data_offset(bytes, symbols, &insert)?;
        objects.push(ObjectReader {
            data_offset,
            constructors: symbols
                .iter()
                .filter(|symbol| symbol.name.starts_with(&format!("{class}::{class}(")))
                .map(|symbol| symbol.address)
                .collect(),
            insert,
            class,
            vtables: vtable.address_points,
            pointers: slots,
            read: *reads.first().unwrap(),
        });
    }
    Ok(objects)
}

fn collect_owner_methods(
    bytes: &[u8],
    symbols: &[Symbol],
    owner: &str,
    functions: &mut Vec<Function>,
    gaps: &mut Vec<String>,
) -> Result<(), AnalysisError> {
    let prefix = format!("{owner}::");
    for symbol in symbols
        .iter()
        .filter(|symbol| symbol.name.starts_with(&prefix) && symbol.name.contains('('))
    {
        if functions
            .iter()
            .any(|function| function.address == symbol.address)
        {
            continue;
        }
        if functions.len() >= 128 {
            gaps.push("owner method collection reached 128 functions".into());
            break;
        }
        let Some(end) = symbols
            .iter()
            .map(|other| other.address)
            .filter(|address| *address > symbol.address)
            .min()
        else {
            continue;
        };
        if end - symbol.address > 1024 * 1024 {
            gaps.push(format!("owner method extent: {}", symbol.name));
            continue;
        }
        functions.push(Function {
            name: symbol.name.clone(),
            address: symbol.address,
            code: super::code_range(bytes, symbol.address, end - symbol.address)?,
        });
    }
    Ok(())
}

/// The insertion specialization loads one pointer member through its preserved receiver.
/// Refuse ambiguous offsets, unfamiliar receiver copies, or an absent specialization.
fn array_data_offset(
    bytes: &[u8],
    symbols: &[Symbol],
    inserts: &[u64],
) -> Result<Option<u64>, AnalysisError> {
    let mut offsets = BTreeSet::new();
    for &start in inserts {
        let Some(end) = symbols
            .iter()
            .map(|symbol| symbol.address)
            .filter(|address| *address > start)
            .min()
        else {
            return Ok(None);
        };
        if end - start > 65536 {
            return Ok(None);
        }
        let code = super::code_range(bytes, start, end - start)?;
        let rows = crate::engine::analysis::decode::decode_arm64(&code, start)
            .map_err(|_| AnalysisError::InvalidRange)?;
        let Some(found) = array_pointer_offsets(&rows) else {
            return Ok(None);
        };
        offsets.extend(found);
    }
    Ok((offsets.len() == 1).then(|| *offsets.first().unwrap()))
}

fn array_pointer_offsets(
    rows: &[crate::engine::analysis::decode::Instruction],
) -> Option<BTreeSet<u64>> {
    if rows.is_empty() {
        return None;
    }
    let indexes: BTreeMap<_, _> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| (row.address, index))
        .collect();
    let mut states = vec![None; rows.len()];
    states[0] = Some(BTreeSet::from(["x0".to_owned()]));
    let mut pending = std::collections::VecDeque::from([0]);
    let mut steps = 0;
    while let Some(pc) = pending.pop_front() {
        steps += 1;
        if steps > rows.len() * 32 {
            return None;
        }
        let row = &rows[pc];
        if row.operation == "br" {
            return None;
        }
        let mut receiver = states[pc].clone()?;
        let parts: Vec<_> = row.operands.split(',').collect();
        let copied = row.operation == "mov"
            && parts.len() == 2
            && parts[0].starts_with('x')
            && receiver.contains(parts[1]);
        for register in super::families::written_registers(&row.operation, &parts) {
            receiver.remove(&format!("x{register}"));
        }
        if copied {
            receiver.insert(parts[0].into());
        }
        if matches!(row.operation.as_str(), "bl" | "blr") {
            for register in 0..19 {
                receiver.remove(&format!("x{register}"));
            }
        }
        if (row.operands.contains("]!") || row.operands.contains("],#"))
            && let Some(memory) = row.operands.split('[').nth(1)
            && let Some(base) = memory.split([',', ']']).next()
        {
            receiver.remove(base);
        }
        let mut next = Vec::new();
        if !matches!(row.operation.as_str(), "ret" | "brk" | "b") && pc + 1 < rows.len() {
            next.push(pc + 1);
        }
        if row.operation == "b"
            || row.operation.starts_with("b.")
            || matches!(row.operation.as_str(), "cbz" | "cbnz" | "tbz" | "tbnz")
        {
            let operand = row.operands.rsplit(',').next()?.trim_start_matches('#');
            let target = if let Some(hex) = operand.strip_prefix("0x") {
                u64::from_str_radix(hex, 16).ok()?
            } else {
                operand.parse().ok()?
            };
            if let Some(index) = indexes.get(&target) {
                next.push(*index);
            }
        }
        for next in next {
            match &mut states[next] {
                Some(previous) => {
                    let joined = previous.intersection(&receiver).cloned().collect();
                    if *previous != joined {
                        *previous = joined;
                        pending.push_back(next);
                    }
                }
                empty @ None => {
                    *empty = Some(receiver.clone());
                    pending.push_back(next);
                }
            }
        }
    }
    Some(
        rows.iter()
            .zip(states)
            .filter_map(|(row, state)| pointer_member_offset(row, &state?))
            .collect(),
    )
}

fn pointer_member_offset(
    row: &crate::engine::analysis::decode::Instruction,
    receivers: &BTreeSet<String>,
) -> Option<u64> {
    if row.operation != "ldr" {
        return None;
    }
    let (destination, memory) = row.operands.split_once(',')?;
    if !destination.starts_with('x') {
        return None;
    }
    let memory = memory.strip_prefix('[')?.strip_suffix(']')?;
    let (base, offset) = memory.split_once(',')?;
    if !receivers.contains(base) {
        return None;
    }
    let digits = offset.strip_prefix('#')?;
    if let Some(hex) = digits.strip_prefix("0x") {
        u64::from_str_radix(hex, 16).ok()
    } else {
        digits.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::analysis_support::{IMAGE_JUMP_TABLE, macho_with_fixups};

    #[test]
    fn the_field_input_holds_the_jump_tables_in_read_only_data() {
        let sections = read_only_data(&macho_with_fixups(6)).unwrap();

        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].address, 0x1_0000_3000);
        assert_eq!(sections[0].bytes, IMAGE_JUMP_TABLE);
    }
    #[test]
    fn collection_buffer_requires_a_receiver_on_every_incoming_path() {
        use crate::engine::analysis::assembler::arm64;
        use crate::engine::analysis::decode::decode_arm64;
        let bypass = arm64!(at 0x1000;
            cbz w2, extern 0x1008;
            mov x19, x0;
            ldr x8, [x19, #8];
            ret
        );
        let proven = arm64!(at 0x1000;
            mov x19, x0;
            cbz w2, extern 0x100c;
            mov x19, x0;
            ldr x8, [x19, #8];
            ret
        );
        assert_eq!(
            array_pointer_offsets(&decode_arm64(&bypass, 0x1000).unwrap()),
            Some(BTreeSet::new())
        );
        assert_eq!(
            array_pointer_offsets(&decode_arm64(&proven, 0x1000).unwrap()),
            Some(BTreeSet::from([8]))
        );
    }
}
