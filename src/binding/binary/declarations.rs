use std::collections::{BTreeMap, BTreeSet};

use object::{Object, ObjectSection, SectionKind};

use crate::engine::analysis::{
    declarations::{CALLER_DEPTH, Composition, DeclarationInput, Function, ScopeSlots},
    decode::decode_arm64,
    discovery::Symbol,
    evaluate::ReadOnlyData,
    families::StringLayout,
    fields::literal_token_names,
};
use crate::{AnalysisError, DeclarationKind};

use super::super::targets::DeclarationRecipe;

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
    let scope_names = text.scope_names(symbols, strings);
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
        let length = text.function_length(start).min(4096);
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
        functions,
        pointers: pointers.clone(),
        strings: strings.clone(),
        slots: ScopeSlots {
            create: recipe.create_slot,
            supported_scopes: scope_slot,
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

pub(super) fn unique(symbols: &[Symbol], name: &str) -> Result<u64, AnalysisError> {
    let addresses: BTreeSet<_> = symbols
        .iter()
        .filter(|symbol| symbol.name == name)
        .map(|symbol| symbol.address)
        .collect();
    (addresses.len() == 1)
        .then(|| *addresses.first().unwrap())
        .ok_or(AnalysisError::InvalidRange)
}

fn branch_target(word: u32, address: u64) -> Option<u64> {
    if word >> 26 != 0b100101 {
        return None;
    }
    immediate_target(word, address)
}

/// The target of a `bl` or of a `b`, which a tail call uses.
pub(super) fn call_or_jump_target(word: u32, address: u64) -> Option<u64> {
    if word >> 26 != 0b100101 && word >> 26 != 0b000101 {
        return None;
    }
    immediate_target(word, address)
}

fn immediate_target(word: u32, address: u64) -> Option<u64> {
    let signed = ((word << 6) as i32 >> 6) as i64;
    address.checked_add_signed(signed * 4)
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

    /// The whole function that starts at `start`.
    pub fn function(&self, start: u64) -> Result<(u64, &'a [u8]), AnalysisError> {
        Ok((start, self.bytes(start, self.function_length(start))?))
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

    /// The address of every direct `bl` to `target`, in address order. This includes calls in
    /// compiler-outlined regions that have no reliable symbol boundary.
    pub fn direct_calls(&self, target: u64) -> Vec<u64> {
        self.code
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .map(|(index, word)| (self.address + (index * 4) as u64, u32::from_le_bytes(*word)))
            .filter(|(at, word)| branch_target(*word, *at) == Some(target))
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

    /// Up to `length` bytes of code that end with the instruction at `at`.
    pub fn window_ending_at(&self, at: u64, length: u64) -> Result<Function, AnalysisError> {
        let start = at.saturating_sub(length).max(self.address);
        Ok(Function {
            address: start,
            code: self.bytes(start, at + 4 - start)?.to_vec(),
        })
    }

    /// Literal token names from the engine's token table.
    pub fn token_names(
        &self,
        symbols: &[Symbol],
        strings: &BTreeMap<u64, String>,
    ) -> Result<BTreeMap<u64, String>, AnalysisError> {
        let start = unique(symbols, "GetTokenArray()")?;
        let code = self.bytes(start, self.function_length(start))?;
        literal_token_names(code, start, symbols, strings).map_err(AnalysisError::Input)
    }

    /// Scope names indexed by scope-type bit, from the engine's scope-name function.
    pub fn scope_names(
        &self,
        symbols: &[Symbol],
        strings: &BTreeMap<u64, String>,
    ) -> Option<Vec<String>> {
        let start = unique(symbols, "NEventScope::GetScopeName(EScopeType, bool)").ok()?;
        let code = self.bytes(start, self.function_length(start)).ok()?;
        let rows = decode_arm64(code, start).ok()?;
        let mut names = BTreeMap::new();
        for window in rows.windows(3) {
            let [test, page, offset] = window else {
                continue;
            };
            if test.operation != "tbz"
                || !(test.operands.starts_with("w21,#") || test.operands.starts_with("x21,#"))
                || page.operation != "adrp"
                || offset.operation != "add"
                || !page.operands.starts_with("x1,")
                || !offset.operands.starts_with("x1,x1,")
            {
                continue;
            }
            let bit = test.operands.split(',').nth(1).and_then(parse_number)? as usize;
            let page = page.operands.split(',').nth(1).and_then(parse_number)?;
            let offset = offset.operands.split(',').nth(2).and_then(parse_number)?;
            if let Some(name) = strings.get(&(page + offset)) {
                names.insert(bit, name.clone());
            }
        }
        if names.get(&2).map(String::as_str) != Some("country")
            || names.get(&40).map(String::as_str) != Some("colony")
        {
            return None;
        }
        let last = *names.keys().max()?;
        Some(
            (0..=last)
                .map(|bit| names.get(&bit).cloned().unwrap_or_default())
                .collect(),
        )
    }
}

/// Read-only data that switch code loads: jump tables and string literals.
pub(super) fn read_only_data(bytes: &[u8]) -> Result<ReadOnlyData, AnalysisError> {
    let slice = super::selected_slice(bytes).map_err(|_| AnalysisError::InvalidRange)?;
    let file = object::File::parse(slice).map_err(|_| AnalysisError::InvalidRange)?;
    let mut sections = Vec::new();
    for section in file.sections().filter(|section| {
        matches!(
            section.kind(),
            SectionKind::ReadOnlyData | SectionKind::ReadOnlyString
        )
    }) {
        let data = section.data().map_err(|_| AnalysisError::InvalidRange)?;
        sections.push((section.address(), data.to_vec()));
    }
    Ok(ReadOnlyData::new(sections))
}
