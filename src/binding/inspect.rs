//! A developer's view of any ARM64 executable: identity, symbols, bounded disassembly, direct
//! callers, string references and fixed-up slots. It needs no target record, so it reads an
//! uncatalogued build. It is not a consumer API; addresses appear here and nowhere public.
//!
//! ```no_run
//! use pdx_native::internals::inspect::{Image, read_image};
//!
//! let bytes = read_image("/path/to/Stellaris".as_ref())?;
//! let image = Image::read(&bytes)?;
//! let start = image.address("CMegaStructureType::ReadMember")?;
//! println!("{}", image.disassemble(start, 64 * 1024)?);
//! # Ok::<(), pdx_native::internals::inspect::InspectError>(())
//! ```
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

use object::{Architecture, Object};

use super::binary::declarations::Text;
use super::binary::families::{register, written_registers};
use super::binary::fixups::{self, FixupDiagnostic, Fixups};
use super::binary::inventory::{self, Inventory};
use crate::engine::analysis::decode::{Instruction, add_immediate, adrp, decode_arm64};
use crate::engine::analysis::stop::Stop;

/// Why an inspection could not be made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectError(String);

impl fmt::Display for InspectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for InspectError {}

fn error(message: impl Into<String>) -> InspectError {
    InspectError(message.into())
}

/// Read the executable at `path`. A directory is resolved to its executable the way
/// `Native::open` resolves an installation.
pub fn read_image(path: &Path) -> Result<Vec<u8>, InspectError> {
    let metadata =
        std::fs::metadata(path).map_err(|cause| error(format!("{}: {cause}", path.display())))?;
    let executable = if metadata.is_dir() {
        super::installation::resolve_directory(path).map_err(|cause| error(cause.to_string()))?
    } else {
        path.to_owned()
    };

    std::fs::read(&executable).map_err(|cause| error(format!("{}: {cause}", executable.display())))
}

/// One image, read for inspection. Fixups are optional: without them, every answer that needs
/// the value of a data slot says that it is unavailable.
pub struct Image<'a> {
    inventory: Inventory<'a>,
    text: Text<'a>,
    fixups: Result<Fixups, FixupDiagnostic>,
}

/// The hashes, architecture and format of an image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// SHA-256 of the whole file.
    pub executable: String,
    /// SHA-256 of the selected slice.
    pub slice: String,
    /// The architecture of the selected slice, such as `Aarch64`.
    pub architecture: String,
    /// The format of the selected slice, such as `MachO`.
    pub format: String,
}

/// A bounded disassembly of the code at one address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    /// The first address.
    pub start: u64,
    /// The symbol at `start`, or the symbol and offset that `start` is inside.
    pub place: String,
    /// Whether a symbol starts at `start`.
    pub at_symbol: bool,
    /// Why the listing stops.
    pub end: End,
    /// One row per instruction word.
    pub rows: Vec<Row>,
}

/// Why a listing stops. Symbols give every extent, so each end is inferred: a local label or
/// compiler-outlined code can place the real end elsewhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum End {
    /// The next symbol starts here.
    NextSymbol(String),
    /// The text section ends here.
    SectionEnd,
    /// The requested byte limit stopped the listing first.
    Limit,
}

/// One instruction word and what the inspector could say about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The word's address.
    pub address: u64,
    /// The mnemonic, or `.word` when the word does not decode.
    pub operation: String,
    /// The operands, or the word in hexadecimal when it does not decode.
    pub operands: String,
    /// Notes in a fixed order.
    pub notes: Vec<Note>,
}

/// A fact about one row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Note {
    /// A direct branch goes to `address`.
    Target {
        /// The destination.
        address: u64,
        /// The symbol or section and offset of `address`.
        place: String,
    },
    /// The row puts `address` in a register, through `adrp` and `add`, `adr`, or the slot that
    /// an `ldr` reads.
    Address {
        /// The formed address.
        address: u64,
        /// The symbol or section and offset of `address`.
        place: String,
    },
    /// The address is a read-only C string.
    String(String),
    /// The address is a fixed-up slot, and this is what it holds.
    Pointer(String),
    /// An indirect branch or call; its target is not resolved.
    IndirectUnresolved,
    /// The word does not decode.
    Undecoded,
}

/// One direct `bl` or `b` to an address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caller {
    /// The branch's address.
    pub at: u64,
    /// `bl` for a call, `b` for a tail call.
    pub operation: &'static str,
    /// The symbol and offset of `at`.
    pub place: String,
}

/// One instruction that puts a string's address in a register.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StringReference {
    /// The string's address.
    pub string_address: u64,
    /// The string.
    pub text: String,
    /// The instruction's address.
    pub at: u64,
    /// The symbol and offset of `at`.
    pub place: String,
}

/// Where and why a method stopped, placed in the image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacedStop {
    /// The instruction, the entry and the obstacle.
    pub stop: Stop,
    /// The symbol and offset of the instruction.
    pub place: String,
    /// The text symbol that holds the instruction, or `None` outside every text symbol.
    pub function: Option<String>,
    /// The instruction, or `None` when the address is not an aligned text address.
    pub row: Option<Row>,
    /// The symbol and offset where the walk last entered code.
    pub entry: String,
}

/// One 8-byte data slot and what it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    /// The slot's address.
    pub address: u64,
    /// The pointer, the bound import, or the raw value when no fixup names the slot.
    pub holds: String,
}

impl<'a> Image<'a> {
    /// Read `bytes` as an ARM64 executable. Fails only when the image itself cannot be read or
    /// is not ARM64; unreadable fixups are kept as a diagnostic.
    pub fn read(bytes: &'a [u8]) -> Result<Self, InspectError> {
        let inventory = inventory::read(bytes).map_err(|cause| error(cause.to_string()))?;
        if inventory.file.architecture() != Architecture::Aarch64 {
            return Err(error("only ARM64 images are inspected"));
        }

        let text = Text::read(bytes, &inventory.symbols)
            .map_err(|_| error("the image does not have exactly one text section"))?;
        let fixups = fixups::read(&inventory);

        Ok(Self {
            inventory,
            text,
            fixups,
        })
    }

    /// The image's hashes, architecture and format. This hashes the whole file.
    pub fn identity(&self) -> Result<Identity, InspectError> {
        let identity = self
            .inventory
            .identity()
            .map_err(|cause| error(cause.to_string()))?;

        Ok(Identity {
            executable: identity.executable,
            slice: identity.slice,
            architecture: format!("{:?}", identity.architecture),
            format: format!("{:?}", identity.format),
        })
    }

    /// `Ok` when fixups were read, so data slots resolve; otherwise the fixup diagnostic.
    pub fn pointer_resolution(&self) -> Result<(), String> {
        self.fixups
            .as_ref()
            .map(|_| ())
            .map_err(ToString::to_string)
    }

    /// Every symbol whose demangled name contains `text`, in address order.
    pub fn symbols(&self, text: &str) -> Vec<(u64, &str)> {
        self.inventory
            .symbols
            .iter()
            .filter(|symbol| symbol.name.contains(text))
            .map(|symbol| (symbol.address, symbol.name.as_str()))
            .collect()
    }

    /// The address that `query` names: `0x` hexadecimal, an exact demangled name, or a name
    /// without its parameter list, such as `CMegaStructureType::ReadMember`. A query that
    /// names more than one address is an error that lists them.
    pub fn address(&self, query: &str) -> Result<u64, InspectError> {
        if let Some(hex) = query.strip_prefix("0x") {
            return u64::from_str_radix(hex, 16).map_err(|_| error(format!("bad address {query}")));
        }

        let exact = self.named(|name| name == query);
        let matches = if exact.is_empty() {
            self.named(|name| name.strip_prefix(query).is_some_and(is_parameter_list))
        } else {
            exact
        };

        let addresses: BTreeSet<u64> = matches.iter().map(|(address, _)| *address).collect();
        match addresses.len() {
            1 => Ok(addresses.into_iter().next().expect("one address")),
            0 => Err(error(format!("no symbol {query}"))),
            _ => {
                let listed: Vec<_> = matches
                    .iter()
                    .map(|(address, name)| format!("  {address:#x} {name}"))
                    .collect();
                Err(error(format!(
                    "{query} names more than one address:\n{}",
                    listed.join("\n")
                )))
            }
        }
    }

    fn named(&self, matches: impl Fn(&str) -> bool) -> Vec<(u64, &str)> {
        self.inventory
            .symbols
            .iter()
            .filter(|symbol| matches(&symbol.name))
            .map(|symbol| (symbol.address, symbol.name.as_str()))
            .collect()
    }

    /// Disassemble from `start` to the next symbol, the end of the text section, or `limit`
    /// bytes, whichever is first. `start` must be an aligned text address, and `limit` a
    /// nonzero multiple of 4.
    pub fn disassemble(&self, start: u64, limit: u64) -> Result<Listing, InspectError> {
        if limit == 0 || !limit.is_multiple_of(4) {
            return Err(error(format!(
                "limit {limit} is not a nonzero multiple of 4"
            )));
        }

        let text_end = self.text.address + self.text.code.len() as u64;
        if !start.is_multiple_of(4) || start < self.text.address || start >= text_end {
            return Err(error(format!("{start:#x} is not an aligned text address")));
        }

        let extent = self.text.function_length(start);
        let end = if extent > limit {
            End::Limit
        } else {
            let next = start + extent;
            match self
                .inventory
                .symbols
                .iter()
                .find(|symbol| symbol.address == next)
            {
                Some(symbol) => End::NextSymbol(symbol.name.clone()),
                None => End::SectionEnd,
            }
        };

        let code = self
            .text
            .bytes(start, extent.min(limit))
            .map_err(|_| error("the extent is outside the text section"))?;
        let words = decoded_words(code, start);

        Ok(Listing {
            start,
            place: self.place(start),
            at_symbol: self.text.starts.contains(&start),
            end,
            rows: self.annotate(words),
        })
    }

    /// Every direct `bl` or `b` to `target`, in address order. Calls through a register or a
    /// vtable are not found.
    pub fn callers(&self, target: u64) -> Vec<Caller> {
        self.text
            .calls_into(&BTreeSet::from([target]))
            .into_iter()
            .map(|(at, _)| {
                let word = self.word(at);
                let operation = if word >> 26 == 0b100101 { "bl" } else { "b" };

                Caller {
                    at,
                    operation,
                    place: self.place(at),
                }
            })
            .collect()
    }

    /// Every instruction that puts the address of a string containing `text` in a register:
    /// `adr`, or `adrp` then `add` in the same function with no branch or other write between
    /// them. Other forms are not found.
    pub fn string_references(&self, text: &str) -> Vec<StringReference> {
        if text.is_empty() {
            return Vec::new();
        }

        let strings: BTreeMap<u64, &str> = self
            .inventory
            .strings
            .iter()
            .filter(|(_, string)| string.contains(text))
            .map(|(address, string)| (*address, string.as_str()))
            .collect();

        let mut references = Vec::new();
        for function in self.string_candidates(&strings) {
            let length = self.text.function_length(function).next_multiple_of(4);
            let Ok(listing) = self.disassemble(function, length) else {
                continue;
            };

            for row in &listing.rows {
                for note in &row.notes {
                    if let Note::Address { address, .. } = note
                        && let Some(string) = strings.get(address)
                    {
                        references.push(StringReference {
                            string_address: *address,
                            text: (*string).into(),
                            at: row.address,
                            place: self.place(row.address),
                        });
                    }
                }
            }
        }

        references
    }

    /// Each function with a word that can form one of `strings`: an `adrp` of its page or an
    /// `adr` of it. Each is read whole, once, so the tracker sees every branch before a reference.
    fn string_candidates(&self, strings: &BTreeMap<u64, &str>) -> BTreeSet<u64> {
        let pages: BTreeSet<u64> = strings.keys().map(|address| address & !0xfff).collect();
        let mut functions = BTreeSet::new();

        for (index, word) in self.text.code.as_chunks::<4>().0.iter().enumerate() {
            let at = self.text.address + index as u64 * 4;
            let word = u32::from_le_bytes(*word);
            let forms_page = adrp(word, at).is_some_and(|(_, page)| pages.contains(&page));
            let forms_string =
                adr(word, at).is_some_and(|(_, target)| strings.contains_key(&target));

            if forms_page || forms_string {
                let function = self.text.starts.range(..=at).next_back().copied();
                functions.insert(function.unwrap_or(self.text.address));
            }
        }

        functions
    }

    /// `count` consecutive 8-byte slots from `address`, such as a vtable, and what each holds.
    /// Without fixups nothing is resolved, and the error is the fixup diagnostic.
    pub fn slots(&self, address: u64, count: usize) -> Result<Vec<Slot>, String> {
        let fixups = self.fixups.as_ref().map_err(ToString::to_string)?;
        let last_fits = match (count as u64).checked_sub(1) {
            None => true,
            Some(last) => last
                .checked_mul(8)
                .and_then(|offset| address.checked_add(offset))
                .is_some(),
        };
        if !last_fits {
            return Err(format!(
                "{count} slots from {address:#x} pass the end of the address space"
            ));
        }

        Ok((0..count as u64)
            .map(|index| address + index * 8)
            .map(|slot| Slot {
                address: slot,
                holds: self.slot_value(fixups, slot).unwrap_or_else(|| {
                    match self.inventory.data_at(slot, 8) {
                        Ok(raw) => format!(
                            "no fixup; raw {:#x}",
                            u64::from_le_bytes(raw.try_into().expect("8 bytes"))
                        ),
                        Err(_) => "no fixup; not in one section".into(),
                    }
                }),
            })
            .collect())
    }

    /// What the loader stores in `slot`, when a fixup names it.
    fn slot_value(&self, fixups: &Fixups, slot: u64) -> Option<String> {
        if let Some(target) = fixups.pointers.get(&slot) {
            return Some(format!("{target:#x} {}", self.place(*target)));
        }
        if let Some(name) = fixups.bindings.get(&slot) {
            return Some(format!("import {name}"));
        }

        fixups
            .bound
            .contains(&slot)
            .then(|| "an import without an exact name".into())
    }

    /// Place a method's stop: the instruction, the function that holds it and where the walk
    /// entered code.
    pub fn place_stop(&self, stop: Stop) -> PlacedStop {
        PlacedStop {
            stop,
            place: self.place(stop.instruction),
            function: self.function_at(stop.instruction).map(str::to_owned),
            row: self
                .disassemble(stop.instruction, 4)
                .ok()
                .and_then(|listing| listing.rows.into_iter().next()),
            entry: self.place(stop.entry),
        }
    }

    /// The text symbol that holds `address`.
    pub fn function_at(&self, address: u64) -> Option<&str> {
        let text_end = self.text.address + self.text.code.len() as u64;
        if !(self.text.address..text_end).contains(&address) {
            return None;
        }

        let symbols = &self.inventory.symbols;
        symbols[..symbols.partition_point(|symbol| symbol.address <= address)]
            .last()
            .filter(|symbol| symbol.address >= self.text.address)
            .map(|symbol| symbol.name.as_str())
    }

    /// Where `address` is. In code, the preceding symbol and offset. In data, symbols have no
    /// size, so a data address that is not a symbol is named by its section and offset, after
    /// the preceding symbol.
    pub fn place(&self, address: u64) -> String {
        let Some((section_start, section)) = self.inventory.section_of(address) else {
            return "outside every section".into();
        };

        let symbols = &self.inventory.symbols;
        let preceding = symbols[..symbols.partition_point(|symbol| symbol.address <= address)]
            .last()
            .filter(|symbol| symbol.address >= section_start);
        let in_text = address >= self.text.address
            && address - self.text.address < self.text.code.len() as u64;

        match preceding {
            Some(symbol) if in_text || symbol.address == address => {
                offset_name(&symbol.name, address - symbol.address)
            }
            Some(symbol) => format!(
                "{}; after {}",
                offset_name(&section, address - section_start),
                offset_name(&symbol.name, address - symbol.address)
            ),
            None => offset_name(&section, address - section_start),
        }
    }

    fn word(&self, at: u64) -> u32 {
        let offset = (at - self.text.address) as usize;
        u32::from_le_bytes(
            self.text.code[offset..offset + 4]
                .try_into()
                .expect("a text word"),
        )
    }

    /// Notes for each word, from one pass that tracks the addresses registers hold. A tracked
    /// value is forgotten at every branch, at every branch target inside the listing and at an
    /// undecoded word, since another path can reach the next row with other values.
    fn annotate(&self, words: Vec<Word>) -> Vec<Row> {
        let targets: BTreeSet<u64> = words
            .iter()
            .filter_map(|word| word.instruction.as_ref())
            .filter_map(branch_target)
            .collect();
        let mut values: BTreeMap<usize, u64> = BTreeMap::new();
        let mut rows = Vec::new();

        for word in words {
            if targets.contains(&word.address) {
                values.clear();
            }

            let Some(instruction) = word.instruction else {
                values.clear();
                rows.push(Row {
                    address: word.address,
                    operation: ".word".into(),
                    operands: format!("{:#010x}", word.value),
                    notes: vec![Note::Undecoded],
                });
                continue;
            };

            let notes = self.row_notes(&instruction, word.value, &mut values);
            rows.push(Row {
                address: instruction.address,
                operation: instruction.operation,
                operands: instruction.operands,
                notes,
            });
        }

        rows
    }

    fn row_notes(
        &self,
        instruction: &Instruction,
        word: u32,
        values: &mut BTreeMap<usize, u64>,
    ) -> Vec<Note> {
        let at = instruction.address;
        let mut notes = Vec::new();

        if let Some(target) = branch_target(instruction) {
            notes.push(Note::Target {
                address: target,
                place: self.place(target),
            });
        }
        if is_indirect_branch(&instruction.operation) {
            notes.push(Note::IndirectUnresolved);
        }
        if is_control_flow(&instruction.operation) {
            values.clear();
            return notes;
        }

        let page = adrp(word, at);
        let formed = adr(word, at).or_else(|| {
            let (destination, source, addend) = add_immediate(word)?;
            let value = values.get(&source)?;
            Some((destination, value.wrapping_add(addend)))
        });
        let loaded_slot = load_unsigned_offset(word)
            .and_then(|(source, offset)| Some(values.get(&source)? + offset));

        for register in destinations(instruction) {
            values.remove(&register);
        }

        if let Some((destination, page)) = page {
            values.insert(destination, page);
        }
        if let Some((destination, address)) = formed {
            values.insert(destination, address);
            notes.extend(self.address_notes(address));
        }
        if let Some(slot) = loaded_slot {
            notes.extend(self.address_notes(slot));
        }

        notes
    }

    /// The place of `address`, the string there, and the pointer its slot holds when fixups
    /// were read.
    fn address_notes(&self, address: u64) -> Vec<Note> {
        let mut notes = vec![Note::Address {
            address,
            place: self.place(address),
        }];

        if let Some(string) = self.inventory.strings.get(&address) {
            notes.push(Note::String(string.clone()));
        }
        if let Ok(fixups) = &self.fixups
            && let Some(value) = self.slot_value(fixups, address)
        {
            notes.push(Note::Pointer(value));
        }

        notes
    }
}

/// One instruction word and, when it decodes, its instruction.
struct Word {
    address: u64,
    value: u32,
    instruction: Option<Instruction>,
}

/// Decode `code`. When it does not decode, each word is decoded on its own, so one data word
/// does not hide the rest.
fn decoded_words(code: &[u8], start: u64) -> Vec<Word> {
    let decoded = decode_arm64(code, start).ok();

    code.as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .map(|(offset, bytes)| {
            let at = start + offset as u64 * 4;
            let instruction = match &decoded {
                Some(rows) => Some(rows[offset].clone()),
                None => decode_arm64(bytes, at).ok().map(|mut rows| rows.remove(0)),
            };

            Word {
                address: at,
                value: u32::from_le_bytes(*bytes),
                instruction,
            }
        })
        .collect()
}

/// Whether `rest` is a function's parameter list and qualifiers, such as `(CReader&, int)` or
/// `() const`, and not a name nested in the function, such as `()::s_pTokenArray`.
fn is_parameter_list(rest: &str) -> bool {
    if !rest.starts_with('(') {
        return false;
    }

    let mut depth = 0;
    for (index, character) in rest.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }

        if depth == 0 {
            return !rest[index + 1..].contains("::");
        }
    }

    false
}

fn offset_name(name: &str, offset: u64) -> String {
    if offset == 0 {
        name.into()
    } else {
        format!("{name}+{offset:#x}")
    }
}

/// `b`, `bl`, `b.cond`, `cbz`, `cbnz`, `tbz` or `tbnz`.
fn is_direct_branch(operation: &str) -> bool {
    matches!(operation, "b" | "bl" | "cbz" | "cbnz" | "tbz" | "tbnz") || operation.starts_with("b.")
}

/// The destination of a direct branch.
fn branch_target(instruction: &Instruction) -> Option<u64> {
    if !is_direct_branch(&instruction.operation) {
        return None;
    }

    let last = instruction.operands.rsplit(',').next()?;
    u64::from_str_radix(last.strip_prefix("#0x")?, 16).ok()
}

/// `br`, `blr` and their pointer-authenticated forms.
fn is_indirect_branch(operation: &str) -> bool {
    operation.starts_with("blr") || (operation.starts_with("br") && operation != "brk")
}

fn is_control_flow(operation: &str) -> bool {
    is_direct_branch(operation) || is_indirect_branch(operation) || operation.starts_with("ret")
}

/// The general registers an instruction writes. `written_registers` reads every mnemonic that
/// starts with `b` as a branch; branches are handled before this, so the rest, such as `bic`
/// and `bfi`, write their first operand. A pre- or post-index access also writes its base.
fn destinations(instruction: &Instruction) -> Vec<usize> {
    let operands: Vec<&str> = instruction.operands.split(',').collect();
    let mut written = if instruction.operation.starts_with('b') {
        operands
            .first()
            .and_then(|operand| register(operand))
            .into_iter()
            .collect()
    } else {
        written_registers(&instruction.operation, &operands)
    };

    written.extend(writeback_base(&instruction.operands));
    written
}

/// The base register of a pre-index (`[x8,#8]!`) or post-index (`[x8],#8`) operand.
fn writeback_base(operands: &str) -> Option<usize> {
    let (_, address) = operands.split_once('[')?;
    let (inside, after) = address.split_once(']')?;
    let writes_back = after.starts_with('!') || after.starts_with(',');
    if !writes_back {
        return None;
    }

    register(inside.split(',').next()?)
}

/// The destination register and address of an `adr` word at `address`.
fn adr(word: u32, address: u64) -> Option<(usize, u64)> {
    if word & 0x9f00_0000 != 0x1000_0000 {
        return None;
    }

    let immediate = (word >> 29 & 0b11) | (word >> 5 & 0x7_ffff) << 2;
    let offset = ((immediate << 11) as i32 >> 11) as i64;

    Some(((word & 0x1f) as usize, address.wrapping_add_signed(offset)))
}

/// The base register and byte offset of a 64-bit `ldr` with an unsigned immediate offset.
fn load_unsigned_offset(word: u32) -> Option<(usize, u64)> {
    if word & 0xffc0_0000 != 0xf940_0000 {
        return None;
    }

    Some((
        (word >> 5 & 0x1f) as usize,
        u64::from(word >> 10 & 0xfff) * 8,
    ))
}

impl fmt::Display for Identity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(formatter, "executable sha256 {}", self.executable)?;
        writeln!(formatter, "slice sha256      {}", self.slice)?;
        write!(
            formatter,
            "slice             {} {}",
            self.format, self.architecture
        )
    }
}

impl fmt::Display for Listing {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.at_symbol {
            writeln!(formatter, "{:#x} {}", self.start, self.place)?;
        } else {
            writeln!(formatter, "{:#x} inside {}", self.start, self.place)?;
        }

        let end = match &self.end {
            End::NextSymbol(name) => format!("the next symbol, {name}"),
            End::SectionEnd => "the end of the text section".into(),
            End::Limit => "the byte limit".into(),
        };
        writeln!(
            formatter,
            "inferred boundary: stops at {end}; a local label or outlined code can move the real end"
        )?;

        for row in &self.rows {
            writeln!(formatter, "{row}")?;
        }

        Ok(())
    }
}

impl fmt::Display for Row {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let line = format!(
            "{:#x}  {:<8} {}",
            self.address, self.operation, self.operands
        );
        if self.notes.is_empty() {
            return formatter.write_str(&line);
        }

        let notes: Vec<_> = self.notes.iter().map(ToString::to_string).collect();
        write!(formatter, "{line:<56} ; {}", notes.join("; "))
    }
}

impl fmt::Display for Note {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Target { address, place } => write!(formatter, "-> {address:#x} {place}"),
            Self::Address { address, place } => write!(formatter, "= {address:#x} {place}"),
            Self::String(text) => write!(formatter, "{text:?}"),
            Self::Pointer(value) => write!(formatter, "holds {value}"),
            Self::IndirectUnresolved => formatter.write_str("indirect, target unresolved"),
            Self::Undecoded => formatter.write_str("does not decode"),
        }
    }
}

impl fmt::Display for Caller {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:#x}  {:<2} in {}",
            self.at, self.operation, self.place
        )
    }
}

impl fmt::Display for StringReference {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:#x}  in {}  = {:#x} {:?}",
            self.at, self.place, self.string_address, self.text
        )
    }
}

impl fmt::Display for PlacedStop {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(formatter, "{}", self.stop.obstacle)?;
        match &self.row {
            Some(row) => writeln!(formatter, "  {row}")?,
            None => writeln!(
                formatter,
                "  {:#x}  not in the text section",
                self.stop.instruction
            )?,
        }
        write!(
            formatter,
            "  at {}; entered at {:#x} {}",
            self.place, self.stop.entry, self.entry
        )
    }
}

impl fmt::Display for Slot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:#x}  holds {}", self.address, self.holds)
    }
}

#[cfg(test)]
mod tests;
