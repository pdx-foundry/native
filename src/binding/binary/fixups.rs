//! Chained fixups: the pointer or import that each data slot holds once the loader has run.
//!
//! The M45 image uses DYLD_CHAINED_IMPORT_ADDEND64 and DYLD_CHAINED_PTR_64_OFFSET. These are the
//! fixup forms of this target, not a general Mach-O dynamic loader. Another form is a
//! [`FixupDiagnostic`]; the image's inventory stays readable.
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use object::read::macho::{MachHeader, MachOFile64};
use object::{BinaryFormat, Object, ObjectSegment};

use super::inventory::{Inventory, display_name};
use super::{u32_at, u64_at};

/// What each fixed-up slot holds.
pub(in crate::binding) struct Fixups {
    /// Slot address to the local address that the loader stores there.
    pub pointers: BTreeMap<u64, u64>,
    /// Slot address to the exact demangled name of the import bound there.
    pub bindings: BTreeMap<u64, String>,
    /// Every slot that the loader binds to an import, named or not.
    pub bound: BTreeSet<u64>,
}

/// Why the fixups of an image were not read, naming the form that was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::binding) enum FixupDiagnostic {
    /// The image is not Mach-O.
    NotMachO,
    /// The image has no `LC_DYLD_CHAINED_FIXUPS` command.
    Missing,
    /// The image has more than one `LC_DYLD_CHAINED_FIXUPS` command.
    Repeated,
    /// The fixups header has a version, import format or symbol format other than 0, 3, 0.
    Header {
        version: u32,
        imports_format: u32,
        symbols_format: u32,
    },
    /// A segment's chains use a pointer format other than 6.
    PointerFormat { segment: String, format: u32 },
    /// A segment's page has more than one chain start.
    MultipleStarts { segment: String },
    /// A count, range or chain does not fit the image.
    Malformed(&'static str),
}

impl fmt::Display for FixupDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotMachO => write!(formatter, "the image is not Mach-O"),
            Self::Missing => write!(formatter, "the image has no chained fixups"),
            Self::Repeated => write!(
                formatter,
                "the image has more than one chained-fixups command"
            ),
            Self::Header {
                version,
                imports_format,
                symbols_format,
            } => write!(
                formatter,
                "chained-fixups header version {version}, imports format {imports_format}, \
                 symbols format {symbols_format}; only 0, 3 (DYLD_CHAINED_IMPORT_ADDEND64), 0 is read"
            ),
            Self::PointerFormat { segment, format } => write!(
                formatter,
                "segment {segment} uses chained pointer format {format}; only 6 \
                 (DYLD_CHAINED_PTR_64_OFFSET) is read"
            ),
            Self::MultipleStarts { segment } => write!(
                formatter,
                "segment {segment} has a page with more than one chain start"
            ),
            Self::Malformed(what) => write!(formatter, "chained fixups are malformed: {what}"),
        }
    }
}

/// Read every chain of the image's fixups.
pub(in crate::binding) fn read(inventory: &Inventory<'_>) -> Result<Fixups, FixupDiagnostic> {
    use FixupDiagnostic::Malformed;

    if inventory.file.format() != BinaryFormat::MachO {
        return Err(FixupDiagnostic::NotMachO);
    }

    let bytes = inventory.slice;
    let payload = payload(bytes)?;
    let header = |at| u32_at(payload, at).ok_or(Malformed("header"));
    let (version, imports_format, symbols_format) = (header(0)?, header(20)?, header(24)?);
    if (version, imports_format, symbols_format) != (0, 3, 0) {
        return Err(FixupDiagnostic::Header {
            version,
            imports_format,
            symbols_format,
        });
    }

    let starts = header(4)? as usize;
    let imports = header(8)? as usize;
    let names = header(12)? as usize;
    let import_count = header(16)? as usize;
    let segments: Vec<_> = inventory.file.segments().collect();
    let segment_count = u32_at(payload, starts).ok_or(Malformed("segment count"))? as usize;
    if segment_count != segments.len() || import_count > 100000 {
        return Err(Malformed("segment or import count"));
    }

    let image_base = segments
        .iter()
        .filter(|s| s.file_range().1 > 0)
        .map(|s| s.address())
        .min()
        .ok_or(Malformed("no segment with file contents"))?;
    let mut pointers = BTreeMap::new();
    let mut bindings = BTreeMap::new();
    let mut bound = BTreeSet::new();

    for (i, segment) in segments.iter().enumerate() {
        let segment_name = || segment.name().ok().flatten().unwrap_or("?").to_owned();
        let offset = u32_at(payload, starts + 4 + 4 * i).ok_or(Malformed("segment start"))?;
        if offset == 0 {
            continue;
        }

        let at = starts + offset as usize;
        let size = u32_at(payload, at).ok_or(Malformed("segment starts"))? as usize;
        let info = payload
            .get(at..at + size)
            .ok_or(Malformed("segment starts"))?;
        let word = u32_at(info, 4).ok_or(Malformed("segment starts"))?;
        let page_size = word & 0xffff;
        let format = word >> 16;
        if format != 6 {
            return Err(FixupDiagnostic::PointerFormat {
                segment: segment_name(),
                format,
            });
        }
        if page_size == 0 {
            return Err(Malformed("page size 0"));
        }

        let segment_offset = u64_at(info, 8).ok_or(Malformed("segment starts"))?;
        if image_base.checked_add(segment_offset) != Some(segment.address()) {
            return Err(Malformed("segment offset"));
        }

        let pages = u16_at(info, 20).ok_or(Malformed("page count"))? as usize;
        for page in 0..pages {
            let start = u16_at(info, 22 + page * 2).ok_or(Malformed("page start"))?;
            if start == 0xffff {
                continue;
            }
            if start & 0x8000 != 0 {
                return Err(FixupDiagnostic::MultipleStarts {
                    segment: segment_name(),
                });
            }

            let page_start = page as u64 * page_size as u64;
            let mut cursor = start as u64;
            loop {
                if cursor + 8 > page_size as u64 {
                    return Err(Malformed("chain leaves its page"));
                }

                let relative = page_start + cursor;
                if relative + 8 > segment.file_range().1 {
                    return Err(Malformed("chain leaves its segment"));
                }

                let pointer = u64_at(bytes, (segment.file_range().0 + relative) as usize)
                    .ok_or(Malformed("chain leaves the image"))?;
                let address = segment.address() + relative;
                let next = (pointer >> 51) & 0xfff;

                if pointer >> 63 == 1 {
                    bound.insert(address);

                    let ordinal = (pointer & 0xffffff) as usize;
                    if ordinal >= import_count {
                        return Err(Malformed("import ordinal"));
                    }

                    let import =
                        u64_at(payload, imports + ordinal * 16).ok_or(Malformed("import"))?;
                    let addend = u64_at(payload, imports + ordinal * 16 + 8)
                        .ok_or(Malformed("import"))? as i64;
                    let library = (import & 0xffff) as u16 as i16;
                    let name = cstring(payload, names + (import >> 32) as usize)
                        .ok_or(Malformed("import name"))?;

                    if let Some(name) =
                        exact_binding_name(name, addend, ((pointer >> 24) & 0xff) as u8)
                    {
                        bindings.insert(address, name);
                    }

                    // Only same-image weak coalescing has an established local resolution.
                    if library == -3
                        && addend == 0
                        && ((pointer >> 24) & 0xff) == 0
                        && let Some(value) = inventory.raw_symbols.get(name)
                    {
                        pointers.insert(address, *value);
                    }
                } else {
                    let target = pointer & 0xfffffffff;
                    let high = (pointer >> 36) & 0xff;
                    let value = image_base
                        .checked_add(target)
                        .ok_or(Malformed("rebase target"))?
                        | (high << 56);
                    pointers.insert(address, value);
                }

                if next == 0 {
                    break;
                }
                cursor = cursor
                    .checked_add(next * 4)
                    .ok_or(Malformed("chain stride"))?;
            }
        }
    }

    Ok(Fixups {
        pointers,
        bindings,
        bound,
    })
}

/// The one `LC_DYLD_CHAINED_FIXUPS` payload of a Mach-O slice.
fn payload(bytes: &[u8]) -> Result<&[u8], FixupDiagnostic> {
    use FixupDiagnostic::Malformed;

    let macho =
        MachOFile64::<object::Endianness>::parse(bytes).map_err(|_| Malformed("Mach-O header"))?;
    let mut commands = macho
        .macho_header()
        .load_commands(macho.endian(), bytes, 0)
        .map_err(|_| Malformed("load commands"))?;
    let mut payload = None;

    while let Some(command) = commands.next().map_err(|_| Malformed("load commands"))? {
        if command.cmd() != object::macho::LC_DYLD_CHAINED_FIXUPS {
            continue;
        }

        let data = command.raw_data();
        let offset = u32_at(data, 8).ok_or(Malformed("fixups command"))? as usize;
        let size = u32_at(data, 12).ok_or(Malformed("fixups command"))? as usize;
        let range = bytes
            .get(offset..offset.saturating_add(size))
            .ok_or(Malformed("fixups range"))?;
        if payload.replace(range).is_some() {
            return Err(FixupDiagnostic::Repeated);
        }
    }

    payload.ok_or(FixupDiagnostic::Missing)
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(at..at.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn cstring(bytes: &[u8], at: usize) -> Option<&str> {
    let bytes = bytes.get(at..)?;
    let end = bytes.iter().take(16384).position(|b| *b == 0)?;

    std::str::from_utf8(&bytes[..end]).ok()
}

fn exact_binding_name(raw: &str, import_addend: i64, pointer_addend: u8) -> Option<String> {
    (import_addend == 0 && pointer_addend == 0).then(|| display_name(raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_bindings_require_zero_import_and_pointer_addends() {
        assert_eq!(
            exact_binding_name("_known", 0, 0).as_deref(),
            Some("_known")
        );
        assert_eq!(exact_binding_name("_known", 1, 0), None);
        assert_eq!(exact_binding_name("_known", 0, 1), None);
        assert_eq!(exact_binding_name("_known", -1, 0), None);
    }
}
