//! Read the declaration registrations and the engine tables that the declaration methods join.
//!
//! `Text::scope_names` keeps the scope-bit sentinels here: bit 2 is `country` and bit 40 is
//! `colony`. They are a per-build layout fact that stays with their reader, not a recipe field. The
//! build is in `docs/native/targets.md` and its facts are on `docs/native/engine-commands.md`.
use std::collections::{BTreeMap, BTreeSet};

use object::{Object, ObjectSection, SectionKind};

use crate::engine::analysis::{
    InputError,
    declarations::{CALLER_DEPTH, Composition, DeclarationInput, Function, ScopeSlots},
    decode::{
        Instruction, add_immediate, adrp, blr_register, call_or_jump_target, call_target,
        decode_arm64, is_branch, ldr_immediate, written_registers,
    },
    discovery::Symbol,
    evaluate::ReadOnlyData,
    fields::{DataSection, Token, recover_decoded},
    strings::StringLayout,
};
use crate::{AnalysisError, DeclarationKind};

use super::super::targets::DeclarationRecipe;

/// The most bytes of one function that the declaration and grammar readers keep. A longer body is
/// cut at this length, and the grammar reader records it as a cut body.
pub(super) const FUNCTION_BODY_LIMIT: u64 = 65_536;

/// How many words after an `adrp` [`Text::functions_forming`] looks for the instruction that
/// completes an address on its page.
const ADDRESS_WINDOW: usize = 4;

/// Read every registration call in executable text, including tail calls and calls in compiler
/// outlined regions that have no reliable symbol boundary, with the code that composes run-time
/// tokens.
pub(in crate::binding) fn read(
    bytes: &[u8],
    symbols: &[Symbol],
    strings: &BTreeMap<u64, String>,
    pointers: &BTreeMap<u64, u64>,
    kind: DeclarationKind,
    recipe: &DeclarationRecipe,
) -> Result<DeclarationInput, AnalysisError> {
    let (database, registrar_name, helper_class, scope_slot) = match kind {
        DeclarationKind::Effect => (
            "CEffectDatabase",
            "CEffectDatabase::RegisterEffectEntry(int, CEffectEntryBase*)",
            "CEffectRegistryHelper",
            recipe.effect_scope_slot,
        ),
        DeclarationKind::Trigger => (
            "CTriggerDatabase",
            "CTriggerDatabase::RegisterTriggerEntry(int, CTriggerEntryBase*)",
            "CTriggerRegistryHelper",
            recipe.trigger_scope_slot,
        ),
    };
    let register_entry = unique(symbols, registrar_name)?;
    let operator_new = operator_new(symbols)?;
    let text = Text::read(bytes, symbols)?;
    let tokens = text.token_names(symbols, strings)?;
    let scope_names = text.scope_names(symbols, strings).map(|table| table.names);
    let entry_calls: Vec<u64> = text
        .calls_into(&BTreeSet::from([register_entry]))
        .into_iter()
        .map(|(at, _)| at)
        .collect();
    let entry_helpers = entry_helpers(symbols, &text, helper_class, &entry_calls);
    let helper_calls = text.calls_into(&entry_helpers);
    let registrars: Vec<_> = entry_calls
        .iter()
        .copied()
        .chain(helper_calls.into_iter().map(|(at, _)| at))
        .map(|at| text.window_ending_at(at, 1024))
        .collect::<Result<_, _>>()?;
    if registrars.is_empty() {
        return Err(AnalysisError::InvalidRange);
    }
    let mut functions = BTreeMap::new();
    for &start in &text.starts {
        let length = text.function_length(start).min(FUNCTION_BODY_LIMIT);
        if length == 0 || !length.is_multiple_of(4) {
            continue;
        }
        functions.insert(
            start,
            Function {
                address: start,
                code: text.bytes(start, length)?.to_vec(),
            },
        );
    }
    Ok(DeclarationInput {
        tokens,
        registrars,
        register_entry: BTreeSet::from([register_entry]),
        entry_helpers,
        operator_new,
        constructors: BTreeMap::new(),
        constructor_calls: Default::default(),
        functions,
        pointers: pointers.clone(),
        writable_slots: super::language::writable_slots(bytes, pointers)?,
        pointer_data: std::sync::OnceLock::new(),
        constructor_image: std::sync::OnceLock::new(),
        strings: strings.clone(),
        slots: ScopeSlots {
            create: recipe.create_slot,
            supported_scopes: scope_slot,
        },
        parser_slots: match kind {
            DeclarationKind::Effect => recipe.effect_parser,
            DeclarationKind::Trigger => recipe.trigger_parser,
        },
        scope_names,
        composition: composition(bytes, symbols, &text, &entry_calls, database, recipe)?,
    })
}

/// Registry helper constructors that take the token and the documentation text and insert the
/// entry themselves, with no call to the register function: the compiler inlined it.
fn entry_helpers(
    symbols: &[Symbol],
    text: &Text,
    class: &str,
    entry_calls: &[u64],
) -> BTreeSet<u64> {
    let prefix = format!("{class}<");
    let suffix = format!(">::{class}(int, char const*)");
    symbols
        .iter()
        .filter(|symbol| symbol.name.starts_with(&prefix) && symbol.name.ends_with(&suffix))
        .map(|symbol| symbol.address)
        .filter(|&start| {
            let end = start + text.function_length(start);
            !entry_calls.iter().any(|call| (start..end).contains(call))
        })
        .collect()
}

/// The functions that contain a call to the register function, their callers up to
/// `CALLER_DEPTH`, and the functions that compose names and documentation.
fn composition(
    bytes: &[u8],
    symbols: &[Symbol],
    text: &Text,
    entry_calls: &[u64],
    database: &str,
    recipe: &DeclarationRecipe,
) -> Result<Composition, AnalysisError> {
    let enclosing = |address: u64| text.starts.range(..=address).next_back().copied();
    let mut starts: BTreeSet<u64> = entry_calls.iter().filter_map(|&at| enclosing(at)).collect();
    let mut callers = BTreeMap::<u64, Vec<(u64, u64)>>::new();
    let mut level = starts.clone();
    for _ in 0..CALLER_DEPTH {
        let mut next = BTreeSet::new();
        for (at, callee) in text.calls_into(&level) {
            let Some(caller) = enclosing(at) else {
                continue;
            };
            callers.entry(callee).or_default().push((at, caller));
            if starts.insert(caller) {
                next.insert(caller);
            }
        }
        level = next;
    }
    let composers: BTreeSet<u64> = symbols
        .iter()
        .filter(|symbol| {
            symbol.name.ends_with("::GenerateTokenName(char const*)")
                || symbol
                    .name
                    .ends_with("::GenerateDocumentation(char const*, char const*)")
        })
        .map(|symbol| symbol.address)
        .collect();
    starts.extend(&composers);
    let bodies = starts
        .into_iter()
        .map(|start| {
            let (address, code) = text.function(start)?;
            Ok((
                start,
                Function {
                    address,
                    code: code.to_vec(),
                },
            ))
        })
        .collect::<Result<_, AnalysisError>>()?;
    Ok(Composition {
        bodies,
        callers,
        composers,
        dynamic_token: addresses(
            symbols,
            "CStaticLexer::AddDynamicToken(CString const&, bool)",
        ),
        create_database: addresses(symbols, &format!("{database}::CreateInstance()")),
        strings: super::families::string_functions(symbols),
        layout: StringLayout {
            flag_byte: recipe.short_string_length_offset,
        },
        data: read_only_data(bytes)?,
    })
}

fn parse_number(text: &str) -> Option<u64> {
    let text = text.strip_prefix('#').unwrap_or(text);
    if let Some(hex) = text.strip_prefix("0x") {
        u64::from_str_radix(hex, 16).ok()
    } else {
        text.parse().ok()
    }
}

/// The one address of the symbol `name`. A name with no address or with several is an input
/// error that says which.
pub(super) fn unique(symbols: &[Symbol], name: &str) -> Result<u64, AnalysisError> {
    let addresses = addresses(symbols, name);

    match addresses.len() {
        0 => Err(InputError(format!("no symbol for {name}")).into()),
        1 => Ok(addresses.into_iter().next().unwrap()),
        count => Err(InputError(format!("{count} addresses for {name}")).into()),
    }
}

/// Every address of one demangled symbol name. The same name can have several addresses, such as
/// the complete and base-object constructors.
pub(super) fn addresses(symbols: &[Symbol], name: &str) -> BTreeSet<u64> {
    symbols
        .iter()
        .filter(|symbol| symbol.name == name)
        .map(|symbol| symbol.address)
        .collect()
}

fn operator_new(symbols: &[Symbol]) -> Result<BTreeSet<u64>, AnalysisError> {
    let operator_new = addresses(symbols, "operator new(unsigned long)");
    if operator_new.is_empty() {
        return Err(AnalysisError::InvalidRange);
    }
    Ok(operator_new)
}

/// The one executable text section and the function boundaries that symbols give it.
pub(in crate::binding) struct Text<'a> {
    pub address: u64,
    pub code: &'a [u8],
    pub starts: BTreeSet<u64>,
}

impl<'a> Text<'a> {
    pub fn read(bytes: &'a [u8], symbols: &[Symbol]) -> Result<Self, AnalysisError> {
        let slice = super::selected_slice(bytes).map_err(|_| AnalysisError::InvalidRange)?;
        let file = object::File::parse(slice).map_err(|_| AnalysisError::InvalidRange)?;
        let mut text = None;
        for section in file
            .sections()
            .filter(|section| section.kind() == SectionKind::Text)
        {
            let data = section.data().map_err(|_| AnalysisError::InvalidRange)?;
            if text.replace((section.address(), data)).is_some() {
                return Err(AnalysisError::InvalidRange);
            }
        }
        let (address, code) = text.ok_or(AnalysisError::InvalidRange)?;
        let end = address
            .checked_add(code.len() as u64)
            .ok_or(AnalysisError::InvalidRange)?;
        let starts = symbols
            .iter()
            .map(|symbol| symbol.address)
            .filter(|start| *start >= address && *start < end)
            .collect();
        Ok(Self {
            address,
            code,
            starts,
        })
    }

    fn end(&self) -> u64 {
        self.address + self.code.len() as u64
    }

    /// Bytes from `start` to the next symbol, or to the end of the section.
    pub fn function_length(&self, start: u64) -> u64 {
        let end = self
            .starts
            .range((start + 1)..)
            .next()
            .copied()
            .unwrap_or_else(|| self.end());
        end - start
    }

    /// The whole function that starts at `start`. An address that is not a function start is an
    /// error.
    pub fn function(&self, start: u64) -> Result<(u64, &'a [u8]), AnalysisError> {
        if !self.starts.contains(&start) {
            return Err(AnalysisError::InvalidRange);
        }
        Ok((start, self.bytes(start, self.function_length(start))?))
    }

    /// The code from `start` to the next function start, or to the end of the text. Unlike
    /// [`Self::function`], `start` need not be a symbol: a static initializer can start where no
    /// symbol does. `None` when `start` is outside the text.
    pub fn code_from(&self, start: u64) -> Option<&'a [u8]> {
        if !(self.address..self.end()).contains(&start) {
            return None;
        }

        self.bytes(start, self.function_length(start)).ok()
    }

    /// The whole function that starts at `start`, or `None` when `start` is not a function start
    /// or the function is longer than `limit` bytes.
    pub fn function_within(&self, start: u64, limit: u64) -> Option<&'a [u8]> {
        let (_, code) = self.function(start).ok()?;

        (code.len() as u64 <= limit).then_some(code)
    }

    pub fn bytes(&self, start: u64, length: u64) -> Result<&'a [u8], AnalysisError> {
        let offset = usize::try_from(
            start
                .checked_sub(self.address)
                .ok_or(AnalysisError::InvalidRange)?,
        )
        .map_err(|_| AnalysisError::InvalidRange)?;
        let length = usize::try_from(length).map_err(|_| AnalysisError::InvalidRange)?;
        let end = offset
            .checked_add(length)
            .ok_or(AnalysisError::InvalidRange)?;
        self.code
            .get(offset..end)
            .ok_or(AnalysisError::InvalidRange)
    }

    /// The decoded function named `name`, or `None` when the build has no single function of
    /// that name: the name has no address or several.
    pub fn unique_body(
        &self,
        symbols: &[Symbol],
        name: &str,
    ) -> Result<Option<Vec<Instruction>>, AnalysisError> {
        let addresses = addresses(symbols, name);
        let Some(&start) = addresses.first().filter(|_| addresses.len() == 1) else {
            return Ok(None);
        };
        let (address, code) = self.function(start)?;

        decode_arm64(code, address)
            .map(Some)
            .map_err(|_| AnalysisError::InvalidRange)
    }

    /// The address of every direct `bl` to `target`, in address order. This includes calls in
    /// compiler-outlined regions that have no reliable symbol boundary.
    pub fn direct_calls(&self, target: u64) -> Vec<u64> {
        self.code
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .map(|(index, word)| (self.address + (index * 4) as u64, u32::from_le_bytes(*word)))
            .filter(|(at, word)| call_target(*word, *at) == Some(target))
            .map(|(at, _)| at)
            .collect()
    }

    /// The address of every direct `bl` or `b` to `target`, in address order: calls and tail
    /// calls.
    pub fn branches_to(&self, target: u64) -> Vec<u64> {
        self.code
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .map(|(index, word)| (self.address + (index * 4) as u64, u32::from_le_bytes(*word)))
            .filter(|(at, word)| call_or_jump_target(*word, *at) == Some(target))
            .map(|(at, _)| at)
            .collect()
    }

    /// Every direct `bl` or `b` to one of `targets`, with its target, in address order.
    pub fn calls_into(&self, targets: &BTreeSet<u64>) -> Vec<(u64, u64)> {
        self.code
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .filter_map(|(index, word)| {
                let at = self.address + (index * 4) as u64;
                let target = call_or_jump_target(u32::from_le_bytes(*word), at)?;
                targets.contains(&target).then_some((at, target))
            })
            .collect()
    }

    /// The functions that form one of `addresses`, each with those that it forms: an `adrp` of the
    /// address's page, then, within the next [`ADDRESS_WINDOW`] instructions of the same function,
    /// an `add` or a 64-bit `ldr` with an immediate on that `adrp`'s register that gives the
    /// address. The window ends at a branch and at another write of the register.
    pub fn functions_forming(&self, addresses: &BTreeSet<u64>) -> BTreeMap<u64, BTreeSet<u64>> {
        let pages: BTreeSet<u64> = addresses.iter().map(|address| address & !0xfff).collect();
        let words: Vec<u32> = self
            .code
            .as_chunks::<4>()
            .0
            .iter()
            .map(|word| u32::from_le_bytes(*word))
            .collect();
        let mut forming: BTreeMap<u64, BTreeSet<u64>> = BTreeMap::new();
        for (index, &word) in words.iter().enumerate() {
            let at = self.address + index as u64 * 4;
            let Some((page_register, page)) =
                adrp(word, at).filter(|(_, page)| pages.contains(page))
            else {
                continue;
            };
            let Some(&function) = self.starts.range(..=at).next_back() else {
                continue;
            };

            let formed: BTreeSet<u64> = self
                .page_uses(at, page_register)
                .into_iter()
                .map(|offset| page + offset)
                .filter(|address| addresses.contains(address))
                .collect();
            if !formed.is_empty() {
                forming.entry(function).or_default().extend(formed);
            }
        }
        forming
    }

    /// The immediates that the `add` and 64-bit `ldr` instructions after the `adrp` at `at` add
    /// to its `register`, up to [`ADDRESS_WINDOW`] instructions, the end of the function, a
    /// branch, or another write of the register.
    fn page_uses(&self, at: u64, register: usize) -> Vec<u64> {
        let end = self
            .starts
            .range(at + 1..)
            .next()
            .copied()
            .unwrap_or_else(|| self.end());
        let length = (ADDRESS_WINDOW as u64 * 4).min(end - at - 4);
        let Ok(code) = self.bytes(at + 4, length) else {
            return Vec::new();
        };
        let Ok(rows) = decode_arm64(code, at + 4) else {
            return Vec::new();
        };

        let mut uses = Vec::new();
        for (row, word) in rows.iter().zip(code.as_chunks::<4>().0) {
            let word = u32::from_le_bytes(*word);
            let used = add_immediate(word)
                .or_else(|| ldr_immediate(word))
                .filter(|&(_, base, _)| base == register);
            if let Some((_, _, offset)) = used {
                uses.push(offset);
            }
            if is_branch(&row.operation)
                || written_registers(&row.operation, &row.operands).contains(&register)
            {
                break;
            }
        }
        uses
    }

    /// Every `blr` in address order, with the register that it calls through.
    pub fn register_calls(&self) -> Vec<(u64, usize)> {
        self.code
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .filter_map(|(index, word)| {
                let at = self.address + (index * 4) as u64;
                Some((at, blr_register(u32::from_le_bytes(*word))?))
            })
            .collect()
    }

    /// Up to `length` bytes of code that end with the instruction at `at`.
    pub fn window_ending_at(&self, at: u64, length: u64) -> Result<Function, AnalysisError> {
        let start = at.saturating_sub(length).max(self.address);
        Ok(Function {
            address: start,
            code: self.bytes(start, at + 4 - start)?.to_vec(),
        })
    }

    /// The engine's token table: every token that `GetTokenArray` constructs, by identity. A
    /// constructor that cannot be read leaves its token out, and the readable tokens stay: each
    /// reader reports a token it cannot find as its own gap, and the registry-field method reports
    /// the table's gaps. A table with gaps and no readable token is an error, because a table that
    /// could not be read is not an empty one.
    pub fn token_table(
        &self,
        symbols: &[Symbol],
        strings: &BTreeMap<u64, String>,
    ) -> Result<BTreeMap<i64, Token>, AnalysisError> {
        let (start, code) = self.function(unique(symbols, "GetTokenArray()")?)?;
        let rows = decode_arm64(code, start).map_err(|_| AnalysisError::InvalidRange)?;
        let (tokens, gaps) = recover_decoded(&rows, symbols, strings);

        match gaps.first() {
            Some(gap) if tokens.is_empty() => {
                Err(InputError(format!("The token table has a gap: {}", gap.reason)).into())
            }
            _ => Ok(tokens),
        }
    }

    /// The literal name of each token in the engine's token table, by 32-bit identity. A token
    /// with no single literal name has none.
    pub fn token_names(
        &self,
        symbols: &[Symbol],
        strings: &BTreeMap<u64, String>,
    ) -> Result<BTreeMap<u64, String>, AnalysisError> {
        Ok(self
            .token_table(symbols, strings)?
            .into_iter()
            .filter_map(|(number, token)| Some((number as u32 as u64, token.name?)))
            .collect())
    }

    /// The engine's scope-name table, from its scope-name function. `None` when the function
    /// is missing or the country and colony sentinels are not where the build puts them.
    pub fn scope_names(
        &self,
        symbols: &[Symbol],
        strings: &BTreeMap<u64, String>,
    ) -> Option<ScopeNameTable> {
        let start = unique(symbols, "NEventScope::GetScopeName(EScopeType, bool)").ok()?;
        let code = self.bytes(start, self.function_length(start)).ok()?;
        let rows = decode_arm64(code, start).ok()?;
        let table = scope_type_rows(&rows)?;
        let mut names = BTreeMap::new();
        let mut unread = BTreeSet::new();
        for (index, test) in table.iter().enumerate() {
            if test.operation != "tbz"
                || !(test.operands.starts_with("w21,#") || test.operands.starts_with("x21,#"))
            {
                continue;
            }

            let bit = test.operands.split(',').nth(1).and_then(parse_number)? as usize;
            if let Some(name) = tested_name(&table[index + 1..], strings) {
                names.insert(bit, name.clone());
            } else {
                unread.insert(bit);
            }
        }

        if names.get(&2).map(String::as_str) != Some("country")
            || names.get(&40).map(String::as_str) != Some("colony")
        {
            return None;
        }
        let last = *names.keys().max()?;
        Some(ScopeNameTable {
            names: (0..=last)
                .map(|bit| names.get(&bit).cloned().unwrap_or_default())
                .collect(),
            unread,
        })
    }
}

/// The engine's scope-name table.
pub(in crate::binding) struct ScopeNameTable {
    /// Names indexed by scope-type bit. A bit that the table does not test has `""`.
    pub names: Vec<String>,
    /// Bits that the table tests but whose name could not be read.
    pub unread: BTreeSet<usize>,
}

/// The rows in which `x21` holds the scope type: after the function copies its argument there
/// and before the code writes `x21` again. A later test of `x21` tests another value.
fn scope_type_rows(rows: &[Instruction]) -> Option<&[Instruction]> {
    let copy = rows
        .iter()
        .position(|row| row.operation == "mov" && row.operands == "x21,x0")?;
    let rows = &rows[copy + 1..];
    let end = rows.iter().position(writes_x21).unwrap_or(rows.len());
    Some(&rows[..end])
}

/// Whether `row` writes `x21`: as its first operand, or as the second register of a pair load.
/// Stores, tests and compares only read it.
fn writes_x21(row: &Instruction) -> bool {
    let operands: Vec<_> = row.operands.split(',').collect();
    let reads_only = row.operation.starts_with("st")
        || matches!(
            row.operation.as_str(),
            "tbz" | "tbnz" | "cbz" | "cbnz" | "cmp" | "cmn" | "tst" | "ccmp" | "ccmn"
        );
    let first = !reads_only && matches!(operands.first(), Some(&("x21" | "w21")));
    let pair = row.operation == "ldp" && matches!(operands.get(1), Some(&("x21" | "w21")));

    first || pair
}

/// The name that the rows after a scope bit test load into `x1`: `adrp x1` then `add x1,x1`.
fn tested_name<'a>(rows: &[Instruction], strings: &'a BTreeMap<u64, String>) -> Option<&'a String> {
    let [page, offset, ..] = rows else {
        return None;
    };
    if page.operation != "adrp"
        || offset.operation != "add"
        || !page.operands.starts_with("x1,")
        || !offset.operands.starts_with("x1,x1,")
    {
        return None;
    }

    let page = page.operands.split(',').nth(1).and_then(parse_number)?;
    let offset = offset.operands.split(',').nth(2).and_then(parse_number)?;
    strings.get(&(page + offset))
}

/// The read-only sections and string literals that code loads, as the file holds them: jump
/// tables and text. Every static reader takes its constants from these sections.
pub(super) fn read_only_sections(bytes: &[u8]) -> Result<Vec<DataSection>, AnalysisError> {
    let slice = super::selected_slice(bytes).map_err(|_| AnalysisError::InvalidRange)?;
    let file = object::File::parse(slice).map_err(|_| AnalysisError::InvalidRange)?;
    file.sections()
        .filter(|section| super::language::is_read_only(section.kind()))
        .map(|section| {
            let data = section.data().map_err(|_| AnalysisError::InvalidRange)?;
            Ok(DataSection {
                address: section.address(),
                bytes: data.to_vec(),
            })
        })
        .collect()
}

/// [`read_only_sections`] as the memory that the evaluator loads from.
pub(super) fn read_only_data(bytes: &[u8]) -> Result<ReadOnlyData, AnalysisError> {
    let sections = read_only_sections(bytes)?
        .into_iter()
        .map(|section| (section.address, section.bytes))
        .collect();

    Ok(ReadOnlyData::new(sections))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_is_formed_only_until_its_page_register_is_written() {
        use crate::engine::analysis::assembler::arm64;
        let code = arm64!(at 0x1000;
            adrp x8, #0x5000; // the page 0x6000
            ldr x9, [x8, #0x40];
            adrp x8, #0x5000;
            mov x8, x0;
            ldr x9, [x8, #0x48];
            adrp x8, #0x5000;
            b >next;
            next:;
            add x9, x8, #0x50;
            ret
        );
        let text = Text {
            address: 0x1000,
            code: &code,
            starts: BTreeSet::from([0x1000]),
        };

        assert_eq!(
            text.functions_forming(&BTreeSet::from([0x6040, 0x6048, 0x6050])),
            BTreeMap::from([(0x1000, BTreeSet::from([0x6040]))])
        );
    }

    #[test]
    fn data_symbols_are_not_function_bodies() {
        let text = Text {
            address: 0x1000,
            code: &[0; 16],
            starts: BTreeSet::from([0x1000]),
        };
        assert!(text.function(0x2000).is_err());
        assert!(text.function(0xff0).is_err());
        assert_eq!(text.function(0x1000).unwrap().1.len(), 16);
    }

    #[test]
    fn code_runs_from_an_address_that_no_symbol_names_to_the_next_function() {
        let text = Text {
            address: 0x1000,
            code: &[0; 32],
            starts: BTreeSet::from([0x1000, 0x1010]),
        };
        let length = |start| text.code_from(start).map(<[u8]>::len);

        assert_eq!(length(0x1000), Some(16));
        assert_eq!(length(0x1004), Some(12));
        assert_eq!(length(0x1010), Some(16));
        assert!(text.function(0x1004).is_err());
        assert_eq!(length(0x1020), None);
        assert_eq!(length(0xff0), None);
    }

    #[test]
    fn every_collector_takes_the_same_function_extent() {
        // The symbol of a later data object at 0x9000 is outside the text, so it is not a start.
        let text = Text {
            address: 0x1000,
            code: &[0; 32],
            starts: BTreeSet::from([0x1000, 0x1010]),
        };

        let final_length = |code: &[u8]| code.len() as u64;
        assert_eq!(text.function_length(0x1010), 16);
        assert!(
            text.function(0x1010)
                .is_ok_and(|(_, code)| code.len() == 16)
        );
        assert_eq!(text.function_within(0x1010, 16).map(final_length), Some(16));
        assert_eq!(text.function_within(0x1010, 15), None);
        assert_eq!(
            super::super::weight_blocks::body(&text, 0x1010).map(final_length),
            Some(16)
        );

        assert!(text.function(0x1004).is_err());
        assert_eq!(text.function_within(0x1004, 1024), None);
        assert_eq!(super::super::weight_blocks::body(&text, 0x1004), None);
    }

    #[test]
    fn a_symbol_that_is_missing_or_has_several_addresses_is_an_input_error_that_names_it() {
        let symbol = |name: &str, address| Symbol {
            name: name.into(),
            address,
        };
        let symbols = [
            symbol("One()", 0x10),
            symbol("Alias()", 0x20),
            symbol("Alias()", 0x20),
            symbol("Two()", 0x30),
            symbol("Two()", 0x40),
        ];

        assert_eq!(unique(&symbols, "One()"), Ok(0x10));
        assert_eq!(unique(&symbols, "Alias()"), Ok(0x20));
        assert_eq!(
            unique(&symbols, "Missing()"),
            Err(InputError("no symbol for Missing()".into()).into())
        );
        assert_eq!(
            unique(&symbols, "Two()"),
            Err(InputError("2 addresses for Two()".into()).into())
        );
    }

    #[test]
    fn a_function_with_no_single_address_has_no_body() {
        let text = Text {
            address: 0x1000,
            code: &[0xc0, 0x03, 0x5f, 0xd6, 0xc0, 0x03, 0x5f, 0xd6],
            starts: BTreeSet::from([0x1000, 0x1004]),
        };
        let symbol = |name: &str, address| Symbol {
            name: name.into(),
            address,
        };
        let symbols = [
            symbol("One()", 0x1000),
            symbol("Alias()", 0x1004),
            symbol("Alias()", 0x1004),
            symbol("Two()", 0x1000),
            symbol("Two()", 0x1004),
        ];
        let body = |name| text.unique_body(&symbols, name).unwrap();

        assert_eq!(body("One()").map(|rows| rows.len()), Some(1));
        assert_eq!(body("Alias()").map(|rows| rows[0].address), Some(0x1004));
        assert!(body("Missing()").is_none());
        assert!(body("Two()").is_none());
    }

    #[test]
    fn a_token_table_keeps_its_readable_tokens_and_is_an_error_only_when_none_is_readable() {
        use crate::engine::analysis::assembler::{Arm64, arm64};
        const START: u64 = 0x1000;
        const CONSTRUCTOR: u64 = 0x2000;
        const YES: u64 = 0x8000;
        const UNREAD: u64 = 0x8010;

        // GetTokenArray names token 7 "yes" and then runs `rest`.
        let token_array = |rest: fn(&mut Arm64)| {
            let mut code = Arm64::at(START);
            arm64!(code; mov w1, #7); // token 7
            code.address(2, YES).call(CONSTRUCTOR);
            rest(&mut code);
            code.bytes()
        };
        let symbols = [
            Symbol {
                name: "GetTokenArray()".into(),
                address: START,
            },
            Symbol {
                name: "CToken::CToken(int, char const*)".into(),
                address: CONSTRUCTOR,
            },
        ];
        let table = |code: &[u8], strings: &BTreeMap<u64, String>| {
            let text = Text {
                address: START,
                code,
                starts: BTreeSet::from([START]),
            };
            let names = text.token_names(&symbols, strings);

            (text.token_table(&symbols, strings), names)
        };
        let strings = BTreeMap::from([(YES, "yes".to_owned())]);

        let complete = token_array(|code| arm64!(code; ret));
        let (tokens, names) = table(&complete, &strings);
        assert_eq!(tokens.unwrap()[&7].name.as_deref(), Some("yes"));
        assert_eq!(names.unwrap(), BTreeMap::from([(7, "yes".to_owned())]));

        // Token 8's literal cannot be read, so token 8 is left out and token 7 stays.
        let one_unreadable = token_array(|code| {
            arm64!(code; mov w1, #8); // token 8
            code.address(2, UNREAD).call(CONSTRUCTOR);
            arm64!(code; ret);
        });
        let (tokens, names) = table(&one_unreadable, &strings);
        let tokens = tokens.unwrap();
        assert_eq!(tokens[&7].name.as_deref(), Some("yes"));
        assert!(!tokens.contains_key(&8));
        assert_eq!(names.unwrap(), BTreeMap::from([(7, "yes".to_owned())]));

        // A branch the recovery cannot follow leaves no readable token: not an empty table.
        let unresolved_branch = token_array(|code| arm64!(code; br x8));
        let (tokens, names) = table(&unresolved_branch, &strings);
        let gap =
            || AnalysisError::Input(InputError("The token table has a gap: instruction".into()));
        assert_eq!(tokens.unwrap_err(), gap());
        assert_eq!(names.unwrap_err(), gap());

        let (tokens, names) = table(&complete, &BTreeMap::new());
        let AnalysisError::Input(InputError(reason)) = tokens.unwrap_err() else {
            panic!("a literal that is not read is an input error");
        };
        assert!(reason.contains("no literal at 0x8000"), "{reason}");
        assert!(names.is_err());
    }

    #[test]
    fn a_scope_bit_whose_name_is_not_read_is_kept_as_unread() {
        use crate::engine::analysis::assembler::{Arm64, arm64};
        const START: u64 = 0x1000;
        const COUNTRY: u64 = 0x8000;
        const COLONY: u64 = 0x8010;
        const MISSING: u64 = 0x8020;

        let mut code = Arm64::at(START);
        arm64!(code; mov x21, x0); // the scope type
        for (bit, name) in [(2u32, COUNTRY), (5, MISSING), (40, COLONY)] {
            let next = code.here() + 12;
            arm64!(code; tbz x21, #bit, extern next as usize);
            code.address(1, name);
        }
        let next = code.here() + 12;
        arm64!(code; tbz x21, #41, extern next as usize); // a name that the table loads another way
        code.load(1, COUNTRY);
        let next = code.here() + 8;
        arm64!(code;
            mov x21, x0; // a returned flag, no longer the scope type
            tbz w21, #0, extern next as usize;
            ret
        );
        let code = code.bytes();

        let text = Text {
            address: START,
            code: &code,
            starts: BTreeSet::from([START]),
        };
        let symbols = [Symbol {
            name: "NEventScope::GetScopeName(EScopeType, bool)".into(),
            address: START,
        }];
        let strings = BTreeMap::from([(COUNTRY, "country".into()), (COLONY, "colony".into())]);

        let table = text.scope_names(&symbols, &strings).unwrap();
        assert_eq!(table.names.len(), 41);
        assert_eq!(table.names[2], "country");
        assert_eq!(table.names[5], "");
        assert_eq!(table.names[40], "colony");
        assert_eq!(table.unread, BTreeSet::from([5, 41]));
    }
}
