use crate::AnalysisError;
use crate::engine::analysis::discovery::{SchedulerLayout, StaticInput, Symbol, VtableWitness};
use object::read::macho::{MachHeader, MachOFile64};
use object::{Object, ObjectSection, ObjectSegment, ObjectSymbol};
use std::collections::BTreeMap;

fn bad() -> AnalysisError {
    AnalysisError::InvalidRange
}
fn u32_at(bytes: &[u8], at: usize) -> Result<u32, AnalysisError> {
    Ok(u32::from_le_bytes(
        bytes.get(at..at + 4).ok_or_else(bad)?.try_into().unwrap(),
    ))
}
fn u64_at(bytes: &[u8], at: usize) -> Result<u64, AnalysisError> {
    Ok(u64::from_le_bytes(
        bytes.get(at..at + 8).ok_or_else(bad)?.try_into().unwrap(),
    ))
}
fn cstring(bytes: &[u8], at: usize) -> Result<&str, AnalysisError> {
    let bytes = bytes.get(at..).ok_or_else(bad)?;
    let end = bytes
        .iter()
        .take(16384)
        .position(|b| *b == 0)
        .ok_or_else(bad)?;
    std::str::from_utf8(&bytes[..end]).map_err(|_| bad())
}
fn data_at<'a>(
    file: &object::File<'a>,
    address: u64,
    length: u64,
) -> Result<&'a [u8], AnalysisError> {
    let mut found = None;
    for section in file.sections() {
        if let Some(data) = section.data_range(address, length).map_err(|_| bad())?
            && found.replace(data).is_some()
        {
            return Err(bad());
        }
    }
    found.ok_or_else(bad)
}

// The M45 image uses DYLD_CHAINED_IMPORT_ADDEND64 and DYLD_CHAINED_PTR_64_OFFSET.
// These are the fixup forms of this target, not a general Mach-O dynamic loader.
fn fixups(
    bytes: &[u8],
    file: &object::File<'_>,
    raw_symbols: &BTreeMap<String, u64>,
) -> Result<BTreeMap<u64, u64>, AnalysisError> {
    let macho = MachOFile64::<object::Endianness>::parse(bytes).map_err(|_| bad())?;
    let endian = macho.endian();
    let mut commands = macho
        .macho_header()
        .load_commands(endian, bytes, 0)
        .map_err(|_| bad())?;
    let mut payload = None;
    while let Some(command) = commands.next().map_err(|_| bad())? {
        if command.cmd() == object::macho::LC_DYLD_CHAINED_FIXUPS {
            let data = command.raw_data();
            let offset = u32_at(data, 8)? as usize;
            let size = u32_at(data, 12)? as usize;
            if payload
                .replace(bytes.get(offset..offset + size).ok_or_else(bad)?)
                .is_some()
            {
                return Err(bad());
            }
        }
    }
    let payload = payload.ok_or_else(bad)?;
    if u32_at(payload, 0)? != 0 || u32_at(payload, 20)? != 3 || u32_at(payload, 24)? != 0 {
        return Err(bad());
    }
    let starts = u32_at(payload, 4)? as usize;
    let imports = u32_at(payload, 8)? as usize;
    let names = u32_at(payload, 12)? as usize;
    let import_count = u32_at(payload, 16)? as usize;
    let segments: Vec<_> = file.segments().collect();
    let segment_count = u32_at(payload, starts)? as usize;
    if segment_count != segments.len() || import_count > 100000 {
        return Err(bad());
    }
    let image_base = segments
        .iter()
        .filter(|s| s.file_range().1 > 0)
        .map(|s| s.address())
        .min()
        .ok_or_else(bad)?;
    let mut pointers = BTreeMap::new();
    for (i, segment) in segments.iter().enumerate() {
        let offset = u32_at(payload, starts + 4 + 4 * i)? as usize;
        if offset == 0 {
            continue;
        }
        let at = starts + offset;
        let size = u32_at(payload, at)? as usize;
        let info = payload.get(at..at + size).ok_or_else(bad)?;
        let word = u32_at(info, 4)?;
        let page_size = word & 0xffff;
        let format = word >> 16;
        if format != 6 || page_size == 0 {
            return Err(bad());
        }
        let segment_offset = u64_at(info, 8)?;
        if image_base.checked_add(segment_offset) != Some(segment.address()) {
            return Err(bad());
        }
        let pages =
            u16::from_le_bytes(info.get(20..22).ok_or_else(bad)?.try_into().unwrap()) as usize;
        for page in 0..pages {
            let start = u16::from_le_bytes(
                info.get(22 + page * 2..24 + page * 2)
                    .ok_or_else(bad)?
                    .try_into()
                    .unwrap(),
            );
            if start == 0xffff {
                continue;
            }
            if start & 0x8000 != 0 {
                return Err(bad());
            }
            let page_start = page as u64 * page_size as u64;
            let mut cursor = start as u64;
            loop {
                if cursor + 8 > page_size as u64 {
                    return Err(bad());
                }
                let relative = page_start + cursor;
                if relative + 8 > segment.file_range().1 {
                    return Err(bad());
                }
                let pointer = u64_at(bytes, (segment.file_range().0 + relative) as usize)?;
                let address = segment.address() + relative;
                let next = (pointer >> 51) & 0xfff;
                if pointer >> 63 == 1 {
                    let ordinal = (pointer & 0xffffff) as usize;
                    if ordinal >= import_count {
                        return Err(bad());
                    }
                    let import = u64_at(payload, imports + ordinal * 16)?;
                    let addend = u64_at(payload, imports + ordinal * 16 + 8)? as i64;
                    let library = (import & 0xffff) as u16 as i16;
                    let name = cstring(payload, names + (import >> 32) as usize)?;
                    // Only same-image weak coalescing has an established local resolution.
                    if library == -3
                        && addend == 0
                        && ((pointer >> 24) & 0xff) == 0
                        && let Some(value) = raw_symbols.get(name)
                    {
                        pointers.insert(address, *value);
                    }
                } else {
                    let target = pointer & 0xfffffffff;
                    let high = (pointer >> 36) & 0xff;
                    let value = image_base.checked_add(target).ok_or_else(bad)? | (high << 56);
                    pointers.insert(address, value);
                }
                if next == 0 {
                    break;
                }
                cursor = cursor.checked_add(next * 4).ok_or_else(bad)?;
            }
        }
    }
    Ok(pointers)
}

pub(in crate::binding) fn read(
    bytes: &[u8],
    layout: &SchedulerLayout,
) -> Result<StaticInput, AnalysisError> {
    let slice = super::selected_slice(bytes).map_err(|_| bad())?;
    let file = object::File::parse(slice).map_err(|_| bad())?;
    let mut raw_symbols = BTreeMap::new();
    let mut symbols = Vec::new();
    for symbol in file
        .symbols()
        .filter(|s| s.is_definition() && s.address() != 0)
    {
        let Ok(raw) = symbol.name() else {
            continue;
        };
        raw_symbols.insert(raw.into(), symbol.address());
        let mangled = raw.strip_prefix('_').unwrap_or(raw);
        let name = cpp_demangle::Symbol::new(mangled)
            .ok()
            .and_then(|s| s.demangle().ok())
            .unwrap_or_else(|| raw.into());
        let name = name
            .strip_prefix("{vtable(")
            .and_then(|s| s.strip_suffix(")}"))
            .map(|s| format!("vtable for {s}"))
            .unwrap_or(name);
        symbols.push(Symbol {
            name,
            address: symbol.address(),
        });
    }
    symbols.sort_by(|a, b| a.address.cmp(&b.address).then(a.name.cmp(&b.name)));
    let pointers = fixups(slice, &file, &raw_symbols)?;
    let mut strings = BTreeMap::new();
    for section in file.sections() {
        if section.kind() != object::SectionKind::ReadOnlyString {
            continue;
        }
        let bytes = section.data().map_err(|_| bad())?;
        let mut offset = 0;
        for string in bytes.split(|b| *b == 0) {
            if let Ok(s) = std::str::from_utf8(string) {
                strings.insert(section.address() + offset as u64, s.into());
            }
            offset += string.len() + 1;
        }
    }
    let mut code = Vec::new();
    let length = layout.end.checked_sub(layout.start).ok_or_else(bad)?;
    if length > 65536 {
        return Err(bad());
    }
    for offset in (0..length).step_by(4096) {
        code.extend(super::code_range(
            bytes,
            layout.start + offset,
            (length - offset).min(4096),
        )?);
    }
    let mut vtables = BTreeMap::new();
    for (i, symbol) in symbols
        .iter()
        .enumerate()
        .filter(|(_, s)| s.name.starts_with("vtable for "))
    {
        let end = symbols[i + 1..]
            .iter()
            .find(|s| s.address > symbol.address)
            .map(|s| s.address)
            .unwrap_or(symbol.address);
        for address in (symbol.address..end.min(symbol.address + 65536)).step_by(8) {
            let Ok(raw) = data_at(&file, address, 16) else {
                continue;
            };
            let offset = u64_at(raw, 0)? as i64;
            if !(-4096..=0).contains(&offset) || !pointers.contains_key(&(address + 8)) {
                continue;
            }
            if let Some(member) = pointers.get(&(address + 56)) {
                vtables.insert(
                    address + 16,
                    VtableWitness {
                        owner: symbol
                            .name
                            .strip_prefix("vtable for ")
                            .expect("selected vtable")
                            .into(),
                        offset_to_top: offset,
                        member: *member,
                    },
                );
            }
        }
    }
    Ok(StaticInput {
        symbols,
        code,
        layout: layout.clone(),
        pointers,
        strings,
        vtables,
    })
}
