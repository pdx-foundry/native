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

    let payload = payload(inventory.slice)?;
    let header = Header::read(payload)?;
    let segments: Vec<_> = inventory.file.segments().collect();
    let segment_count = u32_at(payload, header.starts).ok_or(Malformed("segment count"))? as usize;
    if segment_count != segments.len() || header.imports.count > 100000 {
        return Err(Malformed("segment or import count"));
    }

    let image = Image {
        bytes: inventory.slice,
        imports: header.imports,
        base: image_base(&segments)?,
        raw_symbols: &inventory.raw_symbols,
    };
    let mut fixups = Fixups {
        pointers: BTreeMap::new(),
        bindings: BTreeMap::new(),
        bound: BTreeSet::new(),
    };

    for (index, segment) in segments.iter().enumerate() {
        let Some(chains) = SegmentChains::read(payload, header.starts, index, segment, image.base)?
        else {
            continue;
        };

        for page in 0..chains.page_count {
            let Some(start) = chains.page_start(page)? else {
                continue;
            };

            read_chain(&image, &chains, page, start, &mut fixups)?;
        }
    }

    Ok(fixups)
}

/// The `dyld_chained_fixups_header` of the one form this reader decodes.
struct Header<'a> {
    /// Payload offset of `dyld_chained_starts_in_image`.
    starts: usize,
    imports: Imports<'a>,
}

impl<'a> Header<'a> {
    /// Read the header; another version, import format or symbol format is a diagnostic.
    fn read(payload: &'a [u8]) -> Result<Self, FixupDiagnostic> {
        let field = |at| u32_at(payload, at).ok_or(FixupDiagnostic::Malformed("header"));
        let (version, imports_format, symbols_format) = (field(0)?, field(20)?, field(24)?);
        if (version, imports_format, symbols_format) != (0, 3, 0) {
            return Err(FixupDiagnostic::Header {
                version,
                imports_format,
                symbols_format,
            });
        }

        Ok(Self {
            starts: field(4)? as usize,
            imports: Imports {
                payload,
                records: field(8)? as usize,
                names: field(12)? as usize,
                count: field(16)? as usize,
            },
        })
    }
}

/// The DYLD_CHAINED_IMPORT_ADDEND64 table and the names that its records refer to.
struct Imports<'a> {
    payload: &'a [u8],
    /// Payload offset of the first 16-byte record.
    records: usize,
    /// Payload offset of the name strings.
    names: usize,
    count: usize,
}

/// One DYLD_CHAINED_IMPORT_ADDEND64 record.
struct Import<'a> {
    /// The library ordinal; negative values are the special lookups.
    library: i16,
    /// The raw symbol name.
    name: &'a str,
    addend: i64,
}

impl<'a> Imports<'a> {
    fn get(&self, ordinal: usize) -> Result<Import<'a>, FixupDiagnostic> {
        use FixupDiagnostic::Malformed;

        if ordinal >= self.count {
            return Err(Malformed("import ordinal"));
        }

        let record = self.records + ordinal * 16;
        let word = u64_at(self.payload, record).ok_or(Malformed("import"))?;
        let addend = u64_at(self.payload, record + 8).ok_or(Malformed("import"))? as i64;
        let library = (word & 0xffff) as u16 as i16;
        let name = cstring(self.payload, self.names + (word >> 32) as usize)
            .ok_or(Malformed("import name"))?;

        Ok(Import {
            library,
            name,
            addend,
        })
    }
}

/// What every chain of the image resolves against.
struct Image<'a> {
    /// The selected slice, which the chain file offsets index.
    bytes: &'a [u8],
    imports: Imports<'a>,
    /// The lowest address of a segment with file contents; rebase targets are offsets from it.
    base: u64,
    /// Defined addresses by raw symbol name.
    raw_symbols: &'a BTreeMap<String, u64>,
}

/// The lowest address of a segment with file contents.
fn image_base<'a>(segments: &[impl ObjectSegment<'a>]) -> Result<u64, FixupDiagnostic> {
    segments
        .iter()
        .filter(|s| s.file_range().1 > 0)
        .map(|s| s.address())
        .min()
        .ok_or(FixupDiagnostic::Malformed("no segment with file contents"))
}

/// One segment's `dyld_chained_starts_in_segment` and where its pages lie.
struct SegmentChains<'a> {
    /// The segment name, for diagnostics.
    name: &'a str,
    /// The `dyld_chained_starts_in_segment` record.
    starts: &'a [u8],
    page_size: u64,
    page_count: usize,
    address: u64,
    file_offset: u64,
    file_size: u64,
}

impl<'a> SegmentChains<'a> {
    /// The chains of segment `index`, or `None` when the segment has no chains. A pointer format
    /// other than DYLD_CHAINED_PTR_64_OFFSET is a diagnostic.
    fn read<'data>(
        payload: &'a [u8],
        image_starts: usize,
        index: usize,
        segment: &'a impl ObjectSegment<'data>,
        image_base: u64,
    ) -> Result<Option<Self>, FixupDiagnostic> {
        use FixupDiagnostic::Malformed;

        let name = segment.name().ok().flatten().unwrap_or("?");
        let offset =
            u32_at(payload, image_starts + 4 + 4 * index).ok_or(Malformed("segment start"))?;
        if offset == 0 {
            return Ok(None);
        }

        let at = image_starts + offset as usize;
        let size = u32_at(payload, at).ok_or(Malformed("segment starts"))? as usize;
        let starts = payload
            .get(at..at + size)
            .ok_or(Malformed("segment starts"))?;
        let word = u32_at(starts, 4).ok_or(Malformed("segment starts"))?;
        let page_size = word & 0xffff;
        let format = word >> 16;
        if format != 6 {
            return Err(FixupDiagnostic::PointerFormat {
                segment: name.to_owned(),
                format,
            });
        }
        if page_size == 0 {
            return Err(Malformed("page size 0"));
        }

        let segment_offset = u64_at(starts, 8).ok_or(Malformed("segment starts"))?;
        if image_base.checked_add(segment_offset) != Some(segment.address()) {
            return Err(Malformed("segment offset"));
        }

        let page_count = u16_at(starts, 20).ok_or(Malformed("page count"))? as usize;
        let (file_offset, file_size) = segment.file_range();

        Ok(Some(Self {
            name,
            starts,
            page_size: page_size as u64,
            page_count,
            address: segment.address(),
            file_offset,
            file_size,
        }))
    }

    /// The offset of the one chain start in `page`, or `None` when the page has no chain.
    fn page_start(&self, page: usize) -> Result<Option<u64>, FixupDiagnostic> {
        let start =
            u16_at(self.starts, 22 + page * 2).ok_or(FixupDiagnostic::Malformed("page start"))?;
        if start == 0xffff {
            return Ok(None);
        }
        if start & 0x8000 != 0 {
            return Err(FixupDiagnostic::MultipleStarts {
                segment: self.name.to_owned(),
            });
        }

        Ok(Some(start as u64))
    }
}

/// Follow the chain that starts `start` bytes into `page` and record what each slot holds.
fn read_chain(
    image: &Image<'_>,
    segment: &SegmentChains<'_>,
    page: usize,
    start: u64,
    fixups: &mut Fixups,
) -> Result<(), FixupDiagnostic> {
    use FixupDiagnostic::Malformed;

    let page_start = page as u64 * segment.page_size;
    let mut cursor = start;
    loop {
        if cursor + 8 > segment.page_size {
            return Err(Malformed("chain leaves its page"));
        }

        let relative = page_start + cursor;
        if relative + 8 > segment.file_size {
            return Err(Malformed("chain leaves its segment"));
        }

        let word = u64_at(image.bytes, (segment.file_offset + relative) as usize)
            .ok_or(Malformed("chain leaves the image"))?;
        let address = segment.address + relative;
        let pointer = ChainedPointer::decode(word);

        match pointer.target {
            PointerTarget::Import {
                ordinal,
                addend: pointer_addend,
            } => {
                fixups.bound.insert(address);

                let import = image.imports.get(ordinal)?;
                if let Some(name) = exact_binding_name(import.name, import.addend, pointer_addend) {
                    fixups.bindings.insert(address, name);
                }

                // Only same-image weak coalescing has an established local resolution.
                if import.library == -3
                    && import.addend == 0
                    && pointer_addend == 0
                    && let Some(value) = image.raw_symbols.get(import.name)
                {
                    fixups.pointers.insert(address, *value);
                }
            }
            PointerTarget::Local { offset, high_byte } => {
                let value = image
                    .base
                    .checked_add(offset)
                    .ok_or(Malformed("rebase target"))?
                    | (high_byte << 56);
                fixups.pointers.insert(address, value);
            }
        }

        if pointer.next == 0 {
            break;
        }
        cursor = cursor
            .checked_add(pointer.next * 4)
            .ok_or(Malformed("chain stride"))?;
    }

    Ok(())
}

/// One DYLD_CHAINED_PTR_64_OFFSET word, split into its fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ChainedPointer {
    /// Bits 51-62: the distance to the next slot in 4-byte strides; 0 ends the chain.
    next: u64,
    target: PointerTarget,
}

/// What a chained pointer refers to; bit 63 selects the form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PointerTarget {
    /// `dyld_chained_ptr_64_bind`: bits 0-23 are the import ordinal, bits 24-31 the addend.
    Import { ordinal: usize, addend: u8 },
    /// `dyld_chained_ptr_64_rebase`: bits 0-35 are the offset from the image base, bits 36-43
    /// the top byte of the stored pointer.
    Local { offset: u64, high_byte: u64 },
}

impl ChainedPointer {
    fn decode(word: u64) -> Self {
        let next = (word >> 51) & 0xfff;
        let target = if word >> 63 == 1 {
            PointerTarget::Import {
                ordinal: (word & 0xffffff) as usize,
                addend: ((word >> 24) & 0xff) as u8,
            }
        } else {
            PointerTarget::Local {
                offset: word & 0xfffffffff,
                high_byte: (word >> 36) & 0xff,
            }
        };

        Self { next, target }
    }
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
    fn a_bind_pointer_names_its_ordinal_addend_and_stride() {
        let word = 1 << 63 | 5 << 51 | 0x07 << 24 | 0x123456;

        assert_eq!(
            ChainedPointer::decode(word),
            ChainedPointer {
                next: 5,
                target: PointerTarget::Import {
                    ordinal: 0x123456,
                    addend: 0x07,
                },
            }
        );
    }

    #[test]
    fn a_rebase_pointer_names_its_offset_and_high_byte() {
        let reserved = 0x7f << 44;
        let word = 0xfff << 51 | reserved | 0xab << 36 | 0x9_8765_4321;

        assert_eq!(
            ChainedPointer::decode(word),
            ChainedPointer {
                next: 0xfff,
                target: PointerTarget::Local {
                    offset: 0x9_8765_4321,
                    high_byte: 0xab,
                },
            }
        );
    }

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
