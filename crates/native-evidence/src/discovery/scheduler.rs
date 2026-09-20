use super::{CandidateRecord, SchedulerRow, StaticInput, Symbol};
use crate::analysis::{Instruction, decode_arm64};
use std::collections::{BTreeMap, BTreeSet};

/// Enumerate exact template LoadFile candidates independently of named member readers.
pub fn candidates(symbols: &[Symbol]) -> Vec<CandidateRecord> {
    let names: BTreeSet<_> = symbols.iter().map(|s| s.name.as_str()).collect();
    let mut result = Vec::new();
    for symbol in symbols {
        let Some(args) = symbol
            .name
            .strip_prefix("TSingleObjectGameDatabase<")
            .and_then(|s| s.strip_suffix(">::LoadFile(char const*, bool)"))
        else {
            continue;
        };
        let args: Vec<_> = args.split(',').map(str::trim).collect();
        if args.len() != 3
            || !matches!(args[2], "true" | "false")
            || args[..2]
                .iter()
                .any(|s| s.is_empty() || s.contains(['<', '>']))
        {
            continue;
        }
        result.push(CandidateRecord {
            database: args[0].into(),
            owner_candidate: args[1].into(),
            loader: symbol.name.clone(),
            address: format!("{:#x}", symbol.address),
            has_named_member_reader: names
                .contains(format!("{}::ReadMember(CReader&, int)", args[1]).as_str()),
        });
    }
    result.sort_by(|a, b| a.database.cmp(&b.database).then(a.address.cmp(&b.address)));
    result
}
fn number(s: &str) -> Option<u64> {
    let s = s.trim_start_matches('#');
    if let Some(s) = s.strip_prefix("0x") {
        u64::from_str_radix(s, 16).ok()
    } else {
        s.parse().ok()
    }
}
fn register(s: &str) -> bool {
    s.strip_prefix('x')
        .is_some_and(|s| s.parse::<u8>().is_ok_and(|n| n < 31))
}
fn memory(s: &str) -> Option<(&str, u64)> {
    let inner = s.strip_prefix('[')?.strip_suffix(']')?;
    let (base, offset) = inner.split_once(',').unwrap_or((inner, "0"));
    Some((base, number(offset)?))
}
/// Recovered rows and instruction-level unknown obligations.
pub type SchedulerDerivation = (Vec<SchedulerRow>, Vec<(u64, String)>);

/// Decode the complete bounded initialization, preserving gaps for unsupported effects.
pub fn scheduler(input: &StaticInput) -> Result<SchedulerDerivation, String> {
    let layout = &input.layout;
    if layout.count == 0
        || layout.count > 4096
        || layout.stride != 48
        || layout.start.checked_add(input.code.len() as u64) != Some(layout.end)
        || input.code.len() > 65536
        || !layout.offset.is_multiple_of(8)
        || layout
            .offset
            .checked_add(layout.stride * layout.count as u64)
            .is_none()
    {
        return Err("invalid scheduler bounds".into());
    }
    let mut instructions = Vec::new();
    for (i, chunk) in input.code.chunks(4096).enumerate() {
        instructions.extend(
            decode_arm64(chunk, layout.start + (i * 4096) as u64).map_err(|e| e.to_string())?,
        );
    }
    extract(input, &instructions)
}
fn extract(
    input: &StaticInput,
    instructions: &[Instruction],
) -> Result<SchedulerDerivation, String> {
    let mut regs: BTreeMap<String, u64> = BTreeMap::new();
    let mut slots = BTreeMap::new();
    let mut sources = BTreeMap::new();
    let mut gaps = Vec::new();
    let mut receiver_valid = false;
    for instruction in instructions {
        let args = instruction.operands.as_str();
        let parts: Vec<_> = args.split(',').collect();
        let destination = parts.first().copied().unwrap_or("");
        let operation = instruction.operation.as_str();
        if matches!(destination, "x19" | "w19")
            && !operation.starts_with("st")
            && !(operation == "mov" && parts.get(1) == Some(&"sp") && slots.is_empty())
        {
            receiver_valid = false;
            slots.clear();
        }
        let value = |name: &str| {
            if name == "xzr" {
                Some(0)
            } else {
                regs.get(name).copied()
            }
        };
        match instruction.operation.as_str() {
            "adrp" if parts.len() == 2 && register(destination) => {
                if let Some(v) = number(parts[1]) {
                    regs.insert(destination.into(), v);
                } else {
                    regs.remove(destination);
                }
            }
            "add" if parts.len() == 3 && register(destination) && parts[2].starts_with('#') => {
                let v = value(parts[1]).and_then(|v| v.checked_add(number(parts[2])?));
                regs.remove(destination);
                if let Some(v) = v {
                    regs.insert(destination.into(), v);
                }
            }
            "ldr" if register(destination) => {
                let v = args
                    .split_once(',')
                    .and_then(|(_, a)| memory(a))
                    .and_then(|(base, off)| value(base)?.checked_add(off))
                    .and_then(|a| input.pointers.get(&a).copied());
                regs.remove(destination);
                if let Some(v) = v {
                    regs.insert(destination.into(), v);
                }
            }
            "str" | "stp" if args.contains(",[x19") => {
                let count = if instruction.operation == "stp" { 2 } else { 1 };
                let mem = parts[count..].join(",");
                if let Some(("x19", offset)) = memory(&mem) {
                    for (i, r) in parts[..count].iter().enumerate() {
                        let at = offset + 8 * i as u64;
                        slots.insert(
                            at,
                            if receiver_valid && (register(r) || *r == "xzr") {
                                value(r)
                            } else {
                                None
                            },
                        );
                        sources.insert(at, format!("{:#x}", instruction.address));
                    }
                } else {
                    gaps.push((instruction.address, "unsupported scheduler store".into()));
                    slots.clear();
                }
            }
            "mov" => {
                // The accepted literal method does not infer values through moves.
                regs.remove(destination);
                if let Some(index) = destination.strip_prefix('w') {
                    regs.remove(&format!("x{index}"));
                }
                if destination == "x19" {
                    receiver_valid = parts.get(1) == Some(&"sp") && slots.is_empty();
                }
            }
            "str" | "stp" | "stur" | "sub" if args.contains("sp") || args.contains("x29") => {}
            "blr" | "bl" => {
                slots.clear();
                regs.retain(|r, _| r[1..].parse::<u8>().is_ok_and(|n| n >= 19));
                gaps.push((
                    instruction.address,
                    "unknown call; volatile values discarded and helper effects unresolved".into(),
                ));
            }
            _ => {
                regs.clear();
                if destination == "x19" || destination == "w19" {
                    receiver_valid = false;
                    slots.clear();
                }
                if instruction.operation.starts_with('b')
                    || instruction.operation.starts_with("cb")
                    || instruction.operation.starts_with("tb")
                    || instruction.operation.starts_with("st")
                {
                    slots.clear();
                }
                gaps.push((
                    instruction.address,
                    format!("unsupported instruction {}", instruction.operation),
                ));
            }
        }
    }
    let mut rows = Vec::new();
    for index in 0..input.layout.count {
        let offsets: Vec<_> = (0..6)
            .map(|i| input.layout.offset + input.layout.stride * index as u64 + i * 8)
            .collect();
        let values: Vec<_> = offsets
            .iter()
            .map(|a| slots.get(a).copied().flatten())
            .collect();
        let name = values[0].and_then(|a| input.strings.get(&a).cloned());
        rows.push(SchedulerRow {
            index,
            status: if name.is_some() && values.iter().all(Option::is_some) {
                "recovered"
            } else {
                "gap"
            }
            .into(),
            name,
            values,
            sources: offsets.iter().map(|a| sources.get(a).cloned()).collect(),
        });
    }
    Ok((rows, gaps))
}
