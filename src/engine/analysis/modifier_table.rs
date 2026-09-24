//! Derive the loaded modifier array and lexer lookup from their readers.
//!
//! Instruction shapes supply candidates, not defaults. The evaluator checks the complete
//! documentation loop with zero, one and two labelled entries, and the lookup's warm path
//! with distinct tokens and equal/unequal counts. Unknown instructions or calls refuse the
//! layout. Cold lexer initialization and the logger after the loop are outside this method.
use std::collections::BTreeMap;

use super::{
    decode::Instruction,
    evaluate::{Call, Code, Exit, Machine, ReadOnlyData, Unresolved},
    families::{Arena, Effect, Model, StringFunctions, StringLayout},
};

/// Code and named anchors for the two readers, independent of any build layout.
pub struct Input {
    /// Complete body of `LogDefinitions`, starting at its named entry.
    pub documentation: Vec<Instruction>,
    /// Complete body of `GetString`, starting at its named entry.
    pub get_string: Vec<Instruction>,
    /// Absolute address of the `_Definitions` array header.
    pub definitions: u64,
    /// Entry of the function that names the category mask passed in `w0`.
    pub category_name: u64,
    /// Entry of the function that refreshes the lexer's token lookup.
    pub rebuild_lookup: u64,
    /// Entry of logger access, which marks the end of the documentation loop.
    pub logger: u64,
    /// Absolute pointer slots and their resolved values, including executable rebases.
    pub pointers: BTreeMap<u64, u64>,
    /// Read-only executable data used by the readers and string model.
    pub data: ReadOnlyData,
    /// Named string functions whose effects the evaluator can follow.
    pub strings: StringFunctions,
    /// The bound CString representation used by the documentation code.
    pub string_layout: StringLayout,
}

/// Array fields established by the readers. Both arrays must use the same header layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    /// Byte offset of the data pointer in both array headers.
    pub array_data_offset: u64,
    /// Byte offset of the 32-bit element count in both array headers.
    pub array_count_offset: u64,
    /// Bytes between consecutive modifier definitions.
    pub definition_stride: u64,
    /// Byte offset of the lexer token within a definition.
    pub token_offset: u64,
    /// Byte offset of the category mask within a definition.
    pub mask_offset: u64,
    /// Absolute address of the lexer lookup's array header.
    pub lookup: u64,
    /// Absolute address of the required element count, compared with the lookup's count.
    pub lookup_size: u64,
    /// Bytes between consecutive lookup strings.
    pub lookup_stride: u64,
}

/// Establish every table field, or refuse the operation at the first unsupported shape.
pub fn derive(input: &Input) -> Result<Layout, Unresolved> {
    let mut layout = definition_header(input)?;
    let entries = definition_reads(input, layout, 2, false)?;
    if definition_reads(input, layout, 2, true)? != entries {
        return Err(Unresolved("modifier-field-transformation"));
    }
    let [(token, mask), (next_token, next_mask)] = entries.as_slice() else {
        return Err(Unresolved("modifier-entry-count"));
    };
    layout.token_offset = *token;
    layout.mask_offset = *mask;
    if token == mask
        || token + 4 > layout.definition_stride
        || mask + 4 > layout.definition_stride
        || next_token.checked_sub(*token) != Some(layout.definition_stride)
        || next_mask.checked_sub(*mask) != Some(layout.definition_stride)
    {
        return Err(Unresolved("modifier-entry-layout"));
    }
    for count in [0, 1] {
        if definition_reads(input, layout, count, false)? != entries[..count as usize] {
            return Err(Unresolved("modifier-array-count"));
        }
    }
    let lookup = lookup_fields(input)?;
    layout.lookup = lookup
        .data
        .checked_sub(layout.array_data_offset)
        .ok_or(Unresolved("lexer-array-header"))?;
    if layout.lookup.checked_add(layout.array_count_offset) != Some(lookup.count) {
        return Err(Unresolved("lexer-array-header"));
    }
    layout.lookup_size = lookup.required;
    layout.lookup_stride = lookup.stride;
    verify_lookup(input, &lookup)?;
    Ok(layout)
}

fn definition_header(input: &Input) -> Result<Layout, Unresolved> {
    let code = Code::from_rows(input.documentation.clone());
    let entry = input
        .documentation
        .first()
        .ok_or(Unresolved("modifier-code"))?
        .address;
    let mut headers = Vec::new();
    for rows in input.documentation.windows(5) {
        let [count, branch, data, stride, multiply] = rows else {
            unreachable!()
        };
        if count.operation != "ldrsw"
            || branch.operation != "cbz"
            || data.operation != "ldr"
            || stride.operation != "mov"
            || multiply.operation != "madd"
        {
            continue;
        }
        let Some((count_register, base, count_offset)) = load(count) else {
            continue;
        };
        let Some((data_register, data_base, data_offset)) = load(data) else {
            continue;
        };
        let Some((_, amount)) = stride.operands.split_once(',') else {
            continue;
        };
        let Some(amount) = immediate(amount) else {
            continue;
        };
        if base != data_base
            || !count_register.starts_with('x')
            || !data_register.starts_with('x')
            || amount == 0
            || amount > 4096
            || !amount.is_multiple_of(4)
            || data_offset > 4096
            || count_offset > 4096
            || data_offset + 8 > count_offset && count_offset + 4 > data_offset
        {
            continue;
        }
        let mut machine = Machine::new(&code, &input.data);
        for (&address, &pointer) in &input.pointers {
            machine.write(address, 8, pointer);
        }
        let paths = machine.run_paths_to(entry, count.address, &mut |_, _| Ok(Call::Return(None)));
        if paths.is_empty()
            || paths.iter().any(|path| {
                path.end != Ok(Exit::Reached)
                    || path.machine.register(base) != Some(input.definitions)
            })
        {
            continue;
        }
        headers.push(Layout {
            array_data_offset: data_offset,
            array_count_offset: count_offset,
            definition_stride: amount,
            token_offset: 0,
            mask_offset: 0,
            lookup: 0,
            lookup_size: 0,
            lookup_stride: 0,
        });
    }
    one(headers, "modifier-array-header")
}

/// Each aligned word labels its byte offset. The call arguments must retain those labels.
fn definition_reads(
    input: &Input,
    layout: Layout,
    count: u64,
    reverse_labels: bool,
) -> Result<Vec<(u64, u64)>, Unresolved> {
    const LABEL: u64 = 0x4100_0000;
    let code = Code::from_rows(input.documentation.clone());
    let mut machine = Machine::new(&code, &input.data);
    for (&address, &pointer) in &input.pointers {
        machine.write(address, 8, pointer);
    }
    let span = layout.definition_stride * 2;
    let entries = machine.reserve(span);
    // Opposite label directions must identify the same fields. Arithmetic on a field value
    // must not be mistaken for a load from a neighbouring offset.
    let labels: BTreeMap<u64, u64> = (0..span)
        .step_by(4)
        .map(|offset| {
            let label = if reverse_labels {
                LABEL - offset
            } else {
                LABEL + offset
            };
            machine.write(entries + offset, 4, label);
            (label, offset)
        })
        .collect();
    machine.write(input.definitions + layout.array_data_offset, 8, entries);
    machine.write(input.definitions + layout.array_count_offset, 4, count);
    let string = machine.allocate(input.string_layout.flag_byte + 1);
    let model = Model {
        functions: &input.strings,
        layout: input.string_layout,
        data: &input.data,
        key: "",
    };
    let mut arena = Arena::default();
    let mut found = Vec::new();
    let mut token = None;
    let get_string = input
        .get_string
        .first()
        .ok_or(Unresolved("lexer-code"))?
        .address;
    let end = machine.run(input.documentation[0].address, &mut |target, machine| {
        if target == input.logger {
            return Ok(Call::Stop);
        }
        if target == get_string {
            if token.is_some() || found.len() >= count as usize {
                return Err(Unresolved("modifier-token-call"));
            }
            token = Some(
                machine
                    .register(0)
                    .and_then(|value| labels.get(&value).copied())
                    .ok_or(Unresolved("modifier-token-offset"))?,
            );
            return Ok(Call::Return(Some(string)));
        }
        if target == input.category_name {
            let token = token.take().ok_or(Unresolved("modifier-category-call"))?;
            let mask = machine
                .register(0)
                .and_then(|value| labels.get(&value).copied())
                .ok_or(Unresolved("modifier-mask-offset"))?;
            found.push((token, mask));
            // A successful category lookup bypasses the fallback loop over individual bits.
            return Ok(Call::Return(Some(1)));
        }
        match model.call(Some(target), machine, &mut arena)? {
            Effect::Followed(call) => Ok(call),
            Effect::Other => Err(Unresolved("modifier-call")),
        }
    })?;
    if end != Exit::Stopped(input.logger) || token.is_some() || found.len() != count as usize {
        return Err(Unresolved("modifier-loop"));
    }
    Ok(found)
}

struct Lookup {
    guard: u64,
    data: u64,
    count: u64,
    required: u64,
    stride: u64,
}

fn lookup_guard(rows: &[Instruction]) -> Result<u64, Unresolved> {
    let mut guards = Vec::new();
    for rows in rows.windows(3) {
        if matches!(rows[2].operation.as_str(), "ldaprb" | "ldarb")
            && let Some((_, base, offset)) = load(&rows[2])
        {
            guards.push(
                constant_register(&rows[..2], base)?
                    .checked_add(offset)
                    .ok_or(Unresolved("lexer-guard"))?,
            );
        }
    }
    one(guards, "lexer-guard")
}

fn lookup_fields(input: &Input) -> Result<Lookup, Unresolved> {
    let guard = lookup_guard(&input.get_string)?;
    let mut candidates = Vec::new();
    for rows in input.get_string.windows(10) {
        let [
            _,
            _,
            pair,
            compare,
            branch,
            rebuild,
            page,
            data,
            stride,
            multiply,
        ] = rows
        else {
            unreachable!()
        };
        if pair.operation != "ldp"
            || compare.operation != "cmp"
            || branch.operation != "b.eq"
            || rebuild.operation != "bl"
            || immediate(&rebuild.operands) != Some(input.rebuild_lookup)
            || immediate(&branch.operands) != Some(page.address)
            || page.operation != "adrp"
            || data.operation != "ldr"
            || stride.operation != "mov"
            || multiply.operation != "smaddl"
        {
            continue;
        }
        let Some((registers, memory)) = pair.operands.split_once(",[") else {
            continue;
        };
        let Some((left, right)) = registers.split_once(',') else {
            continue;
        };
        if !left.starts_with('w') || !right.starts_with('w') || compare.operands != registers {
            continue;
        }
        let Some((base, offset)) = memory_operand(memory) else {
            continue;
        };
        let count = constant_register(&rows[..2], base)?
            .checked_add(offset)
            .ok_or(Unresolved("lexer-count"))?;
        let Some((destination, base, offset)) = load(data) else {
            continue;
        };
        if !destination.starts_with('x') {
            continue;
        }
        let data = constant_register(std::slice::from_ref(page), base)?
            .checked_add(offset)
            .ok_or(Unresolved("lexer-data"))?;
        let Some((_, stride)) = stride.operands.split_once(',') else {
            continue;
        };
        let Some(stride) = immediate(stride).filter(|stride| *stride > 0 && *stride <= 4096) else {
            continue;
        };
        candidates.push(Lookup {
            guard,
            data,
            count,
            required: count + 4,
            stride,
        });
    }
    one(candidates, "lexer-lookup-shape")
}

fn verify_lookup(input: &Input, lookup: &Lookup) -> Result<(), Unresolved> {
    let code = Code::from_rows(input.get_string.clone());
    for (token, count, required) in [
        (0, 11, 11),
        (1, 11, 11),
        (7, 11, 11),
        (1, 10, 11),
        (1, 12, 11),
    ] {
        let mut machine = Machine::new(&code, &input.data);
        let data = machine.reserve(lookup.stride * 12);
        machine.write(lookup.guard, 1, 1);
        machine.write(lookup.data, 8, data);
        machine.write(lookup.count, 4, count);
        machine.write(lookup.required, 4, required);
        machine.set_register(0, token);
        let end = machine.run(input.get_string[0].address, &mut |target, _| {
            if target == input.rebuild_lookup {
                Ok(Call::Stop)
            } else {
                Err(Unresolved("lexer-call"))
            }
        })?;
        if count == required {
            if end != Exit::Returned || machine.register(0) != Some(data + token * lookup.stride) {
                return Err(Unresolved("lexer-index"));
            }
        } else if end != Exit::Stopped(input.rebuild_lookup) {
            return Err(Unresolved("lexer-rebuild-check"));
        }
    }
    Ok(())
}

fn one<T>(mut values: Vec<T>, reason: &'static str) -> Result<T, Unresolved> {
    if values.len() == 1 {
        Ok(values.remove(0))
    } else {
        Err(Unresolved(reason))
    }
}

fn immediate(value: &str) -> Option<u64> {
    let value = value.strip_prefix('#')?;
    match value.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => value.parse().ok(),
    }
}

fn memory_operand(memory: &str) -> Option<(usize, u64)> {
    let memory = memory.strip_suffix(']')?;
    let (base, offset) = memory
        .split_once(',')
        .map_or((memory, Some(0)), |(base, offset)| {
            (base, immediate(offset))
        });
    let base = base
        .strip_prefix('x')?
        .parse::<usize>()
        .ok()
        .filter(|base| *base < 31)?;
    Some((base, offset?))
}

fn load(row: &Instruction) -> Option<(&str, usize, u64)> {
    let (register, memory) = row.operands.split_once(",[")?;
    let (base, offset) = memory_operand(memory)?;
    Some((register, base, offset))
}

/// Evaluate only an adjacent address-forming sequence; no incoming register is known.
fn constant_register(rows: &[Instruction], register: usize) -> Result<u64, Unresolved> {
    let mut rows = rows.to_vec();
    if rows.is_empty()
        || rows
            .iter()
            .any(|row| !matches!(row.operation.as_str(), "adrp" | "add"))
    {
        return Err(Unresolved("global-address"));
    }
    rows.push(Instruction {
        address: rows.last().unwrap().address + 4,
        bytes: [0; 4],
        operation: "ret".into(),
        operands: "".into(),
    });
    let entry = rows[0].address;
    let code = Code::from_rows(rows);
    let data = ReadOnlyData::default();
    let mut machine = Machine::new(&code, &data);
    if machine.run(entry, &mut |_, _| Err(Unresolved("global-call")))? != Exit::Returned {
        return Err(Unresolved("global-address"));
    }
    machine
        .register(register)
        .ok_or(Unresolved("global-address"))
}

#[cfg(test)]
mod tests;
