use crate::AnalysisError;
use crate::binding::targets::PersistentRecipe;
use crate::engine::analysis::{
    discovery::{CandidateRecord, Symbol},
    fields::{DataSection, FieldInput, Function, ObjectReader, PointReader, has_owner_receiver},
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
    persistent_recipe: Option<&PersistentRecipe>,
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
    let objects = collect_objects(
        bytes,
        symbols,
        pointers,
        bound_slots,
        persistent_recipe,
        &mut functions,
    )?;
    if !objects.is_empty() {
        collect_owner_methods(
            bytes,
            symbols,
            &selection.owner_candidate,
            &mut functions,
            &mut gaps,
        )?;
    }
    let persistent = persistent_recipe
        .map(|recipe| {
            super::receivers::persistent(
                bytes,
                symbols,
                pointers,
                bound_slots,
                &selection.owner_candidate,
                recipe,
            )
        })
        .transpose()?;
    Ok(FieldInput {
        key_readers: persistent_recipe
            .map(|recipe| key_readers(symbols, recipe))
            .transpose()?
            .unwrap_or_default(),
        persistent,
        objects,
        selection,
        symbols: symbols.to_vec(),
        functions,
        strings: strings.clone(),
        read_only_data: read_only_data(bytes)?,
        gaps,
    })
}

pub(super) fn read_only_data(bytes: &[u8]) -> Result<Vec<DataSection>, AnalysisError> {
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

/// Follow direct constructor and factory calls only; a symbol alone does not attach a nested
/// class to a field.
fn collect_objects(
    bytes: &[u8],
    symbols: &[Symbol],
    pointers: &BTreeMap<u64, u64>,
    bound_slots: &BTreeSet<u64>,
    recipe: Option<&PersistentRecipe>,
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
    let mut classes = BTreeMap::<String, Vec<&Symbol>>::new();

    for symbol in symbols
        .iter()
        .filter(|symbol| calls.contains(&symbol.address))
    {
        if let Some(class) = super::receivers::constructor_class(&symbol.name) {
            classes.entry(class.to_owned()).or_default();
        } else if let Some(class) = factory_class(&symbol.name) {
            classes.entry(class.to_owned()).or_default().push(symbol);
        }
    }

    if classes.is_empty() {
        return Ok(Vec::new());
    }

    let data = super::language::constant_data(bytes, pointers, bound_slots)?;
    let unique_name = |address: u64| {
        let names: BTreeSet<_> = symbols
            .iter()
            .filter(|symbol| symbol.address == address)
            .map(|symbol| symbol.name.as_str())
            .collect();

        (names.len() == 1).then(|| names.first().unwrap().to_string())
    };
    let mut objects = Vec::new();

    for (class, factories) in classes {
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

        let reader = recipe
            .zip(vtable.address_points.get(&0))
            .and_then(|(recipe, &point)| {
                Some(PointReader {
                    point,
                    reader: super::receivers::concrete_reader(
                        recipe,
                        pointers,
                        point,
                        &unique_name,
                    )?,
                })
            });
        let member_name = format!("{class}::ReadMember(CReader&, int)");
        let mut bodies_available = match symbols.iter().find(|symbol| symbol.name == member_name) {
            Some(symbol) => push_function(bytes, symbols, functions, symbol)?,
            None => reader.is_some(),
        };

        for factory in &factories {
            bodies_available &= push_function(bytes, symbols, functions, factory)?;
        }

        if !bodies_available {
            continue;
        }

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
        let insert = addresses(
            symbols,
            &format!(
                "{class}*& CPdxArray<{class}*, int>::InsertAtEmplace<{class}* const&>(int, {class}* const&)"
            ),
        );
        let scoped = format!("CPdxScopedPtrImpl<{class}, false>");
        let moving_insert = addresses(
            symbols,
            &format!(
                "{scoped}& CPdxArray<{scoped}, int>::InsertAtEmplace<{scoped} >(int, {scoped}&&)"
            ),
        )
        .into_iter()
        .filter(|&start| clears_moved_source_at(bytes, symbols, start).unwrap_or(false))
        .collect();
        let data_offset = array_member_offset(bytes, symbols, &insert, array_pointer_offsets)?;
        let count_offset = array_member_offset(bytes, symbols, &insert, array_count_offsets)?;
        objects.push(ObjectReader {
            data_offset,
            count_offset,
            constructors: symbols
                .iter()
                .filter(|symbol| super::receivers::constructor_class(&symbol.name) == Some(&class))
                .map(|symbol| symbol.address)
                .collect(),
            factories: factories.iter().map(|symbol| symbol.address).collect(),
            insert,
            moving_insert,
            class,
            vtables: vtable.address_points,
            pointers: slots,
            read: *reads.first().unwrap(),
            reader,
        });
    }
    Ok(objects)
}

/// The class that a `PdxMakeScopedPtr<Class, …>` factory allocates and constructs.
fn factory_class(name: &str) -> Option<&str> {
    let (_, arguments) = name.split_once(" PdxMakeScopedPtr<")?;
    let mut depth = 0usize;

    for (at, character) in arguments.char_indices() {
        match character {
            '<' => depth += 1,
            '>' if depth == 0 => return Some(&arguments[..at]),
            '>' => depth -= 1,
            ',' if depth == 0 => return Some(&arguments[..at]),
            _ => {}
        }
    }

    None
}

fn addresses(symbols: &[Symbol], name: &str) -> Vec<u64> {
    symbols
        .iter()
        .filter(|symbol| symbol.name == name)
        .map(|symbol| symbol.address)
        .collect()
}

/// Add a symbol's bounded body once; whether the body is available.
fn push_function(
    bytes: &[u8],
    symbols: &[Symbol],
    functions: &mut Vec<Function>,
    symbol: &Symbol,
) -> Result<bool, AnalysisError> {
    if functions
        .iter()
        .any(|function| function.address == symbol.address)
    {
        return Ok(true);
    }

    let end = symbols
        .iter()
        .map(|other| other.address)
        .filter(|address| *address > symbol.address)
        .min();
    let Some(end) = end.filter(|end| end - symbol.address <= 1024 * 1024) else {
        return Ok(false);
    };

    functions.push(Function {
        name: symbol.name.clone(),
        address: symbol.address,
        code: super::code_range(bytes, symbol.address, end - symbol.address)?,
    });
    Ok(true)
}

/// Whether the moving insertion at `start` clears its source pointer on every return.
fn clears_moved_source_at(
    bytes: &[u8],
    symbols: &[Symbol],
    start: u64,
) -> Result<bool, AnalysisError> {
    let Some(end) = symbols
        .iter()
        .map(|symbol| symbol.address)
        .filter(|address| *address > start)
        .min()
        .filter(|end| end - start <= 65536)
    else {
        return Ok(false);
    };
    let code = super::code_range(bytes, start, end - start)?;
    let rows = crate::engine::analysis::decode::decode_arm64(&code, start)
        .map_err(|_| AnalysisError::InvalidRange)?;

    Ok(clears_moved_source(&rows))
}

/// Whether every return leaves the 8-byte slot that `x2` names on entry holding zero, as the
/// moving insertion of a scoped pointer must. The slot is cleared by `str xzr` through a register
/// that still holds the entry `x2`; any other store through such a register, or a call that may
/// receive it, makes the slot unknown again. Stores through other registers are taken not to
/// alias the caller's source slot, because the insertion writes only its array and frame.
fn clears_moved_source(rows: &[crate::engine::analysis::decode::Instruction]) -> bool {
    #[derive(Clone, PartialEq)]
    struct Source {
        registers: BTreeSet<String>,
        cleared: bool,
    }

    if rows.is_empty() {
        return false;
    }

    let indexes: BTreeMap<_, _> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| (row.address, index))
        .collect();
    let mut states: Vec<Option<Source>> = vec![None; rows.len()];
    states[0] = Some(Source {
        registers: BTreeSet::from(["x2".to_owned()]),
        cleared: false,
    });
    let mut pending = std::collections::VecDeque::from([0]);
    let mut returned = false;
    let mut steps = 0;

    while let Some(pc) = pending.pop_front() {
        steps += 1;
        if steps > rows.len() * 32 {
            return false;
        }

        let row = &rows[pc];
        let Some(mut source) = states[pc].clone() else {
            return false;
        };
        let parts: Vec<_> = row.operands.split(',').collect();

        if row.operation == "ret" {
            if !source.cleared {
                return false;
            }
            returned = true;
            continue;
        }

        if row.operation == "br" {
            return false;
        }

        if row.operation.starts_with("st")
            && memory_base(&row.operands).is_some_and(|base| source.registers.contains(base))
        {
            source.cleared = row.operation == "str" && parts.len() == 2 && parts[0] == "xzr";
        }

        if (row.operation == "bl" || row.operation == "blr")
            && (0..8).any(|index| source.registers.contains(&format!("x{index}")))
        {
            source.cleared = false;
        }

        let copied = row.operation == "mov"
            && parts.len() == 2
            && parts[0].starts_with('x')
            && source.registers.contains(parts[1]);

        for register in
            crate::engine::analysis::decode::written_registers(&row.operation, &row.operands)
        {
            source.registers.remove(&format!("x{register}"));
        }

        if copied {
            source.registers.insert(parts[0].into());
        }

        let mut next = Vec::new();

        if !matches!(row.operation.as_str(), "b" | "brk") && pc + 1 < rows.len() {
            next.push(pc + 1);
        }

        if row.operation == "b"
            || row.operation.starts_with("b.")
            || matches!(row.operation.as_str(), "cbz" | "cbnz" | "tbz" | "tbnz")
        {
            let Some(target) = branch_target(&row.operands) else {
                return false;
            };
            let Some(&index) = indexes.get(&target) else {
                return false;
            };
            next.push(index);
        }

        for next in next {
            let joined = match &states[next] {
                Some(previous) => Source {
                    registers: previous
                        .registers
                        .intersection(&source.registers)
                        .cloned()
                        .collect(),
                    cleared: previous.cleared && source.cleared,
                },
                None => source.clone(),
            };

            if states[next].as_ref() != Some(&joined) {
                states[next] = Some(joined);
                pending.push_back(next);
            }
        }
    }

    returned
}

/// The base register of an instruction's memory operand.
fn memory_base(operands: &str) -> Option<&str> {
    let (_, memory) = operands.split_once('[')?;

    memory.split([',', ']']).next()
}

fn branch_target(operands: &str) -> Option<u64> {
    let operand = operands.rsplit(',').next()?.trim_start_matches('#');

    match operand.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => operand.parse().ok(),
    }
}

fn collect_owner_methods(
    bytes: &[u8],
    symbols: &[Symbol],
    owner: &str,
    functions: &mut Vec<Function>,
    gaps: &mut Vec<String>,
) -> Result<(), AnalysisError> {
    for symbol in symbols
        .iter()
        .filter(|symbol| has_owner_receiver(&symbol.name, owner))
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

/// The one offset of an array member that `find` reads from every insertion specialization, such
/// as the pointer member that the specialization loads through its preserved receiver. Refuse
/// ambiguous offsets, unfamiliar receiver copies, or an absent specialization.
fn array_member_offset(
    bytes: &[u8],
    symbols: &[Symbol],
    inserts: &[u64],
    find: fn(&[crate::engine::analysis::decode::Instruction]) -> Option<BTreeSet<u64>>,
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
        let Some(found) = find(&rows) else {
            return Ok(None);
        };
        offsets.extend(found);
    }
    Ok((offsets.len() == 1).then(|| *offsets.first().unwrap()))
}

/// The offsets of the pointer members that the specialization loads through its receiver.
fn array_pointer_offsets(
    rows: &[crate::engine::analysis::decode::Instruction],
) -> Option<BTreeSet<u64>> {
    let states = receiver_states(rows)?;
    Some(
        rows.iter()
            .zip(states)
            .filter_map(|(row, state)| pointer_member_offset(row, &state?))
            .collect(),
    )
}

/// The offsets of the 32-bit members that the specialization increments: it loads the word
/// through its receiver, adds one, and stores the sum back at the same offset. The array's count
/// is such a member. The scan reads the instructions in address order.
fn array_count_offsets(
    rows: &[crate::engine::analysis::decode::Instruction],
) -> Option<BTreeSet<u64>> {
    let states = receiver_states(rows)?;
    let mut loaded: BTreeMap<String, u64> = BTreeMap::new();
    let mut incremented: BTreeMap<String, u64> = BTreeMap::new();
    let mut counts = BTreeSet::new();
    for (row, receivers) in rows.iter().zip(states) {
        let receivers = receivers.unwrap_or_default();
        let parts: Vec<&str> = row.operands.split(',').collect();
        let member = |base: &str, offset: Option<&&str>| {
            let base = base.strip_prefix('[')?;
            let offset =
                offset.map_or(Some(0), |offset| parse_offset(offset.trim_end_matches(']')))?;
            receivers
                .contains(base.trim_end_matches(']'))
                .then_some(offset)
        };
        let mut words: Vec<(String, u64)> = Vec::new();
        let mut sum = None;
        match (row.operation.as_str(), parts.as_slice()) {
            ("ldr", [destination, base, offset @ ..]) if destination.starts_with('w') => {
                words.extend(member(base, offset.first()).map(|at| (destination.to_string(), at)));
            }
            ("ldp", [first, second, base, offset @ ..]) if first.starts_with('w') => {
                if let Some(at) = member(base, offset.first()) {
                    words.push((first.to_string(), at));
                    words.push((second.to_string(), at + 4));
                }
            }
            ("sxtw" | "mov", [destination, source]) => {
                let source = format!("w{}", &source[1..]);
                if let Some(&at) = loaded.get(&source) {
                    words.push((format!("w{}", &destination[1..]), at));
                }
            }
            ("add", [destination, source, "#1"]) if destination.starts_with('w') => {
                sum = loaded.get(*source).map(|&at| (destination.to_string(), at));
            }
            ("str", [value, base, offset @ ..]) => {
                let at = member(base, offset.first());
                if at.is_some() && incremented.get(*value) == at.as_ref() {
                    counts.extend(at);
                }
            }
            ("stp", [first, second, base, offset @ ..]) => {
                if let Some(at) = member(base, offset.first()) {
                    if incremented.get(*first) == Some(&at) {
                        counts.insert(at);
                    }
                    if incremented.get(*second) == Some(&(at + 4)) {
                        counts.insert(at + 4);
                    }
                }
            }
            _ => {}
        }
        for register in
            crate::engine::analysis::decode::written_registers(&row.operation, &row.operands)
        {
            loaded.remove(&format!("w{register}"));
            incremented.remove(&format!("w{register}"));
        }
        loaded.extend(words);
        incremented.extend(sum);
    }
    Some(counts)
}

/// An immediate offset such as `#0x14`.
fn parse_offset(operand: &str) -> Option<u64> {
    let digits = operand.strip_prefix('#')?;
    match digits.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => digits.parse().ok(),
    }
}

/// For each instruction, the registers that hold the specialization's receiver before it runs:
/// `x0` at entry and its copies, joined across branches. `None` when the flow leaves through a
/// register or exceeds its bound.
fn receiver_states(
    rows: &[crate::engine::analysis::decode::Instruction],
) -> Option<Vec<Option<BTreeSet<String>>>> {
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
        for register in
            crate::engine::analysis::decode::written_registers(&row.operation, &row.operands)
        {
            receiver.remove(&format!("x{register}"));
        }
        if copied {
            receiver.insert(parts[0].into());
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
            next.push(*indexes.get(&target)?);
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
    Some(states)
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

/// Bind the compound reader boundaries used by both field and command dispatch.
pub(super) fn key_readers(
    symbols: &[Symbol],
    recipe: &super::super::targets::PersistentRecipe,
) -> Result<crate::engine::analysis::fields::KeyReaders, AnalysisError> {
    use super::declarations::{addresses, unique};
    let token_copy = addresses(symbols, "CToken::CToken(CToken const&)");
    let target_construct = addresses(
        symbols,
        "CEventTarget::CEventTarget(CToken, EScopeType, CString const&)",
    );
    if token_copy.is_empty() || target_construct.is_empty() {
        return Err(AnalysisError::InvalidRange);
    }
    Ok(crate::engine::analysis::fields::KeyReaders {
        compound_sizes: recipe.compound_sizes,
        array_data: recipe.string_array[0],
        array_count: recipe.string_array[1],
        string_stride: recipe.string_array[2],
        value_token: recipe.value_token as i64,
        token_text: recipe.token_text as i64,
        token_copy: token_copy.into_iter().collect(),
        target_construct: target_construct.into_iter().collect(),
        target_move: Some(unique(symbols, "CEventTarget::operator=(CEventTarget&&)")?),
        string_emplace: Some(unique(
            symbols,
            "void CPdxArray<CString, int>::SetSizeAndEmplace<>(int, const&)",
        )?),
        optional_string: Some(unique(
            symbols,
            "void CPdxOptional<CString>::SetEmplace<char const*>(char const*&&)",
        )?),
        string_read: Some(unique(symbols, "CReader::Read(CString&, bool)")?),
        persistent_read: Some(unique(symbols, "CReader::Read(CPersistent&)")?),
    })
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
    /// The shape of the pointer insertion specialization: the count at `+0x14` is incremented
    /// on the path that has room and on the path that grows the buffer.
    #[test]
    fn the_count_is_the_word_that_an_insertion_loads_increments_and_stores_back() {
        use crate::engine::analysis::assembler::arm64;
        use crate::engine::analysis::decode::decode_arm64;
        let counts = |code: Vec<u8>| array_count_offsets(&decode_arm64(&code, 0x1000).unwrap());
        let insertion = arm64!(at 0x1000;
            mov x19, x0;
            ldp w8, w25, [x0, #0x10];
            sxtw x25, w25;
            cmp w25, w8;
            b.ne extern 0x1020;
            add w26, w25, #1;
            stp w8, w26, [x19, #0x10];
            ret;
            ldr x8, [x19, #8]; // 0x1020: room for one more
            add w10, w25, #1;
            str w10, [x19, #0x14];
            ret
        );
        let other_word = arm64!(at 0x1000;
            ldr w8, [x0, #0x10];
            add w9, w8, #1;
            str w9, [x0, #0x14];
            ret
        );

        assert_eq!(counts(insertion), Some(BTreeSet::from([0x14])));
        assert_eq!(counts(other_word), Some(BTreeSet::new()));
    }

    #[test]
    fn a_moving_insertion_must_clear_its_source_on_every_return() {
        use crate::engine::analysis::assembler::arm64;
        use crate::engine::analysis::decode::decode_arm64;
        let clears = |code: Vec<u8>| clears_moved_source(&decode_arm64(&code, 0x1000).unwrap());
        let moved = arm64!(at 0x1000;
            mov x22, x2;
            bl extern 0x2000; // allocation
            ldr x8, [x22];
            str xzr, [x22];
            str x8, [x0];
            ret
        );
        let bypassed = arm64!(at 0x1000;
            mov x22, x2;
            cbz w1, extern 0x1010;
            ldr x8, [x22];
            str xzr, [x22];
            ret
        );
        let other_slot = arm64!(at 0x1000;
            ldr x8, [x2];
            str xzr, [x3];
            ret
        );
        let kept = arm64!(at 0x1000;
            ldr x8, [x2];
            str x8, [x0];
            ret
        );
        let passed_on = arm64!(at 0x1000;
            str xzr, [x2];
            mov x0, x2;
            bl extern 0x2000; // may store through the source
            ret
        );

        assert!(clears(moved));
        for code in [bypassed, other_slot, kept, passed_on] {
            assert!(!clears(code));
        }
    }

    #[test]
    fn an_object_whose_member_is_the_root_does_not_add_the_root_twice() {
        // A second body with the root's name would make the root ambiguous to the dispatch walk.
        let root = Symbol {
            name: "CExample::ReadMember(CReader&, int)".into(),
            address: 0x1000,
        };
        let mut functions = vec![Function {
            name: root.name.clone(),
            address: root.address,
            code: vec![0; 4],
        }];

        assert!(push_function(&[], std::slice::from_ref(&root), &mut functions, &root).unwrap());
        assert_eq!(functions.len(), 1);
    }

    #[test]
    fn a_factory_names_its_class_as_the_first_template_argument() {
        assert_eq!(
            factory_class(
                "CPdxScopedPtrImpl<A<B>, false> PdxMakeScopedPtr<A<B>, int, C const&>(int, C const&)"
            ),
            Some("A<B>")
        );
        assert_eq!(
            factory_class("CPdxScopedPtrImpl<A, false> PdxMakeScopedPtr<A>()"),
            Some("A")
        );
        assert_eq!(factory_class("A::A(int)"), None);
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
    #[test]
    fn collection_buffer_forgets_a_receiver_copy_that_an_instruction_writes() {
        use crate::engine::analysis::assembler::arm64;
        use crate::engine::analysis::decode::decode_arm64;
        let inserted = arm64!(at 0x1000;
            mov x19, x0;
            bfi x19, x8, #0, #8;
            ldr x9, [x19, #8];
            ret
        );
        let pre_index = arm64!(at 0x1000;
            mov x19, x0;
            ldr x9, [x19, #16]!;
            ldr x10, [x19, #8];
            ret
        );
        let post_index = arm64!(at 0x1000;
            mov x19, x0;
            ldr x9, [x19], #16;
            ldr x10, [x19, #8];
            ret
        );
        let kept = arm64!(at 0x1000;
            mov x19, x0;
            ldr x9, [x0, #16];
            ldr x10, [x19, #8];
            ret
        );
        let offsets = |code: Vec<u8>| array_pointer_offsets(&decode_arm64(&code, 0x1000).unwrap());

        assert_eq!(offsets(inserted), Some(BTreeSet::new()));
        assert_eq!(offsets(pre_index), Some(BTreeSet::new()));
        assert_eq!(offsets(post_index), Some(BTreeSet::new()));
        assert_eq!(offsets(kept), Some(BTreeSet::from([8, 16])));
    }
    #[test]
    fn collection_buffer_is_unresolved_when_a_branch_leaves_the_function() {
        use crate::engine::analysis::assembler::arm64;
        use crate::engine::analysis::decode::decode_arm64;
        let conditional_exit = arm64!(at 0x1000;
            ldr x8, [x0, #8];
            cbz w2, extern 0x2000; // cold block outside the bounded function
            ret
        );
        let tail_exit = arm64!(at 0x1000;
            ldr x8, [x0, #8];
            b extern 0x2000
        );
        for code in [conditional_exit, tail_exit] {
            assert_eq!(
                array_pointer_offsets(&decode_arm64(&code, 0x1000).unwrap()),
                None
            );
        }
    }
}
