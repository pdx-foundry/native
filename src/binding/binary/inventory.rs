//! What one executable image states without a target record: the selected slice, its symbols
//! and its read-only strings. Fixups are a separate layer that can fail on its own.
use std::collections::BTreeMap;

use object::read::macho::{MachHeader, MachOFile64};
use object::{BinaryFormat, Object, ObjectSection, ObjectSymbol, SectionKind, SymbolIndex};

use super::{ImageIdentity, u32_at, u64_at};
use crate::engine::analysis::discovery::Symbol;
use crate::{AnalysisError, OpenError};

/// One supported image, read without a layout, a recipe or fixups.
pub(in crate::binding) struct Inventory<'a> {
    /// The whole file, fat or thin.
    pub bytes: &'a [u8],
    /// The selected slice.
    pub slice: &'a [u8],
    pub file: object::File<'a>,
    /// Demangled definitions and indirect stubs, in address and then name order.
    pub symbols: Vec<Symbol>,
    /// Read-only C strings by address.
    pub strings: BTreeMap<u64, String>,
    /// Defined addresses by raw symbol name, which chained-fixup imports refer to.
    pub raw_symbols: BTreeMap<String, u64>,
}

pub(in crate::binding) fn read(bytes: &[u8]) -> Result<Inventory<'_>, OpenError> {
    let slice = super::selected_slice(bytes)?;
    let file = object::File::parse(slice).map_err(|_| OpenError::MalformedExecutable)?;
    super::check_supported(&file)?;

    let (mut symbols, raw_symbols) = definitions(&file);
    if file.format() == BinaryFormat::MachO {
        let stubs = indirect_stubs(slice, &file).ok_or(OpenError::MalformedExecutable)?;
        symbols.extend(
            stubs
                .into_iter()
                .map(|(address, name)| Symbol { name, address }),
        );
    }
    symbols.sort_by(|a, b| a.address.cmp(&b.address).then(a.name.cmp(&b.name)));

    let strings = strings(&file).ok_or(OpenError::MalformedExecutable)?;

    Ok(Inventory {
        bytes,
        slice,
        file,
        symbols,
        strings,
        raw_symbols,
    })
}

impl Inventory<'_> {
    /// The hashes, architecture and format of the image. Hashing reads every byte, so it is
    /// done only when asked.
    pub fn identity(&self) -> Result<ImageIdentity, OpenError> {
        super::identify(self.bytes, &super::hash(self.bytes))
    }

    /// `length` bytes at `address` from the one section that holds them.
    pub fn data_at(&self, address: u64, length: u64) -> Result<&[u8], AnalysisError> {
        let mut found = None;

        for section in self.file.sections() {
            let data = section
                .data_range(address, length)
                .map_err(|_| AnalysisError::InvalidRange)?;

            if let Some(data) = data
                && found.replace(data).is_some()
            {
                return Err(AnalysisError::InvalidRange);
            }
        }

        found.ok_or(AnalysisError::InvalidRange)
    }

    /// The start address and `segment,section` name of the section that contains `address`.
    pub fn section_of(&self, address: u64) -> Option<(u64, String)> {
        let section = self.file.sections().find(|section| {
            address >= section.address() && address - section.address() < section.size()
        })?;
        let name = section.name().ok()?;
        let name = match section.segment_name().ok().flatten() {
            Some(segment) => format!("{segment},{name}"),
            None => name.into(),
        };

        Some((section.address(), name))
    }
}

/// The deepest nesting that a symbol may reach while it is parsed and while it is rendered. The
/// defaults (96 and 128) reject the deepest modifier node constructors, which need 120 and 144 on
/// M451-hotfix. Twice the defaults reads every symbol that 1,024 and 2,048 read there, and keeps
/// the recursion well inside the stack.
const DEMANGLE_PARSE_LIMIT: u32 = 192;
const DEMANGLE_RENDER_LIMIT: u32 = 256;

/// A raw symbol name as a demangled C++ name, or unchanged when it does not demangle.
pub(super) fn display_name(raw: &str) -> String {
    let mangled = raw.strip_prefix('_').unwrap_or(raw);
    let parse = cpp_demangle::ParseOptions::default().recursion_limit(DEMANGLE_PARSE_LIMIT);
    let render = cpp_demangle::DemangleOptions::default().recursion_limit(DEMANGLE_RENDER_LIMIT);

    cpp_demangle::Symbol::new_with_options(mangled, &parse)
        .ok()
        .and_then(|symbol| symbol.demangle_with_options(&render).ok())
        .unwrap_or_else(|| raw.into())
}

/// Every named definition with a nonzero address, demangled, and the same by raw name.
fn definitions(file: &object::File<'_>) -> (Vec<Symbol>, BTreeMap<String, u64>) {
    let mut symbols = Vec::new();
    let mut raw_symbols = BTreeMap::new();

    for symbol in file
        .symbols()
        .filter(|s| s.is_definition() && s.address() != 0)
    {
        let Ok(raw) = symbol.name() else {
            continue;
        };
        raw_symbols.insert(raw.into(), symbol.address());

        let name = display_name(raw);
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

    (symbols, raw_symbols)
}

/// The imported symbol behind each symbol stub. An image without `LC_DYSYMTAB` has none.
fn indirect_stubs(bytes: &[u8], file: &object::File<'_>) -> Option<BTreeMap<u64, String>> {
    let macho = MachOFile64::<object::Endianness>::parse(bytes).ok()?;
    let endian = macho.endian();
    let mut commands = macho.macho_header().load_commands(endian, bytes, 0).ok()?;
    let mut indirect = None;
    let mut stub_sections = Vec::new();

    while let Some(command) = commands.next().ok()? {
        let data = command.raw_data();

        if command.cmd() == object::macho::LC_DYSYMTAB {
            let offset = u32_at(data, 56)? as usize;
            let count = u32_at(data, 60)? as usize;

            if count > 1_000_000 || indirect.replace((offset, count)).is_some() {
                return None;
            }
        }

        if command.cmd() != object::macho::LC_SEGMENT_64 {
            continue;
        }

        let section_count = u32_at(data, 64)? as usize;
        if section_count > 10_000 || data.len() < 72 + section_count * 80 {
            return None;
        }

        for index in 0..section_count {
            let section = &data[72 + index * 80..72 + (index + 1) * 80];
            if u32_at(section, 64)? & 0xff != object::macho::S_SYMBOL_STUBS {
                continue;
            }

            let address = u64_at(section, 32)?;
            let size = u64_at(section, 40)?;
            let first = u32_at(section, 68)? as usize;
            let stride = u32_at(section, 72)? as u64;
            if stride == 0 || !size.is_multiple_of(stride) || size / stride > 100_000 {
                return None;
            }

            stub_sections.push((address, size / stride, first, stride));
        }
    }

    let Some((offset, count)) = indirect else {
        return (stub_sections.is_empty()).then(BTreeMap::new);
    };
    let end = offset.checked_add(count.checked_mul(4)?)?;
    let table = bytes.get(offset..end)?;
    let mut stubs = BTreeMap::new();

    for (address, entries, first, stride) in stub_sections {
        if first
            .checked_add(entries as usize)
            .is_none_or(|end| end > count)
        {
            return None;
        }

        for index in 0..entries as usize {
            let symbol_index = u32_at(table, (first + index) * 4)?;
            if symbol_index
                & (object::macho::INDIRECT_SYMBOL_LOCAL | object::macho::INDIRECT_SYMBOL_ABS)
                != 0
            {
                continue;
            }

            let symbol = file
                .symbol_by_index(SymbolIndex(symbol_index as usize))
                .ok()?;
            let raw = symbol.name().ok()?;
            stubs.insert(address + index as u64 * stride, display_name(raw));
        }
    }

    Some(stubs)
}

/// Every UTF-8 C string in a read-only string section, by address.
fn strings(file: &object::File<'_>) -> Option<BTreeMap<u64, String>> {
    let mut strings = BTreeMap::new();

    for section in file
        .sections()
        .filter(|section| section.kind() == SectionKind::ReadOnlyString)
    {
        let bytes = section.data().ok()?;
        let mut offset = 0;

        for string in bytes.split(|b| *b == 0) {
            if let Ok(s) = std::str::from_utf8(string) {
                strings.insert(section.address() + offset as u64, s.into());
            }
            offset += string.len() + 1;
        }
    }

    Some(strings)
}

#[cfg(test)]
mod tests {
    use super::display_name;

    /// The base constructor of modifier node 23 on M451-hotfix, which nests past the default
    /// parse and render limits.
    const DEEP_NODE_CONSTRUCTOR: &str = "__ZN13NModifierNode17CModifierNodeBaseI9CModifier21EModifierNodeCategoryEC2IZNS_13CModifierNodeIS1_S2_LS2_23ENS_13SDependenciesIS2_JNS5_IS1_S2_LS2_18ENS6_IS2_JNS5_IS1_S2_LS2_17ENS6_IS2_JNS5_IS1_S2_LS2_4ENS6_IS2_JEEEEENS5_IS1_S2_LS2_13ES7_EENS5_IS1_S2_LS2_27ES7_EENS5_IS1_S2_LS2_0ES7_EENS5_IS1_S2_LS2_5ES7_EENS5_IS1_S2_LS2_6ES7_EENS5_IS1_S2_LS2_2ENS6_IS2_JNS5_IS1_S2_LS2_10ES7_EENS5_IS1_S2_LS2_26ES7_EENS5_IS1_S2_LS2_34ES7_EEEEEEENS5_IS1_S2_LS2_28ES7_EENS5_IS1_S2_LS2_22ES7_EEEEEEENS5_IS1_S2_LS2_32ENS6_IS2_JNS5_IS1_S2_LS2_24ENS6_IS2_JNS5_IS1_S2_LS2_3ES7_EES8_S9_NS5_IS1_S2_LS2_15ES7_EENS5_IS1_S2_LS2_9ES7_EESI_SD_SC_NS5_IS1_S2_LS2_30ENS6_IS2_JS8_S9_NS5_IS1_S2_LS2_25ES7_EESC_SD_SI_SB_NS5_IS1_S2_LS2_33ES7_EEEEEEESA_SK_EEEEENS5_IS1_S2_LS2_29ES7_EESJ_EEEEENS5_IS1_S2_LS2_8ES7_EENS5_IS1_S2_LS2_1ES7_EESR_EEEEESI_EEEEC1IZNS14_C1I9CPopGroupJiRK16ModifierCategoryEEERT_MS1B_FvRS14_EDpOT0_EUlS1D_E_JiS1A_EEENS_10EOwnerTypeES1B_S1I_EUlRS3_E_JiS1A_EEERKNS_17CNodeTypeInstanceIS2_EES1K_S1B_S1I_";

    #[test]
    fn deeply_nested_template_symbols_demangle() {
        let name = display_name(DEEP_NODE_CONSTRUCTOR);

        assert!(name.starts_with(
            "NModifierNode::CModifierNodeBase<CModifier, EModifierNodeCategory>::CModifierNodeBase<"
        ));
        assert!(name.contains("::CModifierNode<CPopGroup, int, ModifierCategory const&>("));
    }
}
