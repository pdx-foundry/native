//! Keyword domains: the names that a field read through a pure converter accepts.
//!
//! A keyword field's paths call a converter, `TokenToEnum<E>(int const&)` or `Get…FromToken(int)`,
//! with the value token and store its result in the owner (the walker's `Keyword` join). The
//! converter returns one fallback for every token it does not know, and the caller decides what a
//! fallback means. This method runs the owner's member reader concretely for the field's key and
//! each value token of the token domain, every token from zero to the largest literal token and
//! the first value after it, which stands for every token created at run time. The converter runs
//! its own code; the weight method's diagnostic calls mark a path that reports the value.
//!
//! Every path must return through the converter. A value is accepted when some path returns with
//! no diagnostic: the engine keeps it in that owner state, and a diagnostic on another path is
//! about that other state, such as a key read out of order. A value is reported when every path
//! writes a diagnostic, whatever the caller stored. The run-time token decides the domain: when
//! it is reported, the accepted values are the domain; when it is accepted, the caller keeps the
//! fallback of an unknown name with no diagnostic, and the domain stays unknown. Each accepted
//! value must have one literal name.
//!
//! Two validated rules from the weight method hold here: the run-time token behaves as any name
//! that the engine does not know, and a call that receives neither the owner nor the reader
//! changes no script state. A call that receives either stops the path. The owner's post-read hook
//! is not run, so a fallback that a hook reports and replaces keeps the domain unknown; the facts
//! and this limit are on `docs/native/registry-fields.md`.
use std::collections::{BTreeMap, BTreeSet};

use super::{FieldInput, ReaderJoin, RootField, Token};
use crate::engine::analysis::decode::decode_arm64;
use crate::engine::analysis::discovery::SymbolIndex;
use crate::engine::analysis::evaluate::{Call, Code, Exit, Machine, ReadOnlyData};
use crate::engine::analysis::readers::{self, ConverterArgument};
use crate::engine::analysis::stop::Unresolved;

/// The bytes reserved for the owner and for the reader.
const SPAN: u64 = 0x10000;
/// The registers `x0` to `x7`, which carry the arguments of a call.
const ARGUMENT_REGISTERS: usize = 8;
/// Labels that keep one path's facts.
const EMITTED: u64 = 1;
const CONVERTED: u64 = 2;

/// Why a keyword field has no listed domain.
const UNKNOWN_KEPT: &str = "An unknown name keeps the converter's fallback with no diagnostic.";
const CALLER_UNRESOLVED: &str = "A path of the caller could not be followed for some value.";
const UNNAMED_VALUE: &str = "An accepted value has no single literal name.";
const THROUGH_SERIALIZER: &str =
    "The caller reads the owner through a serializer, which the domain walk does not follow.";

/// The domain of each keyword field that the member reader `member` reads, by the field's token:
/// its accepted names, sorted, or why they are not established.
pub(super) fn domains(
    input: &FieldInput,
    symbol_index: &SymbolIndex,
    member: &str,
    fields: &[RootField],
    tokens: &BTreeMap<i64, Token>,
) -> BTreeMap<i64, Result<Vec<String>, &'static str>> {
    let keywords: Vec<_> = fields.iter().filter(|field| is_keyword(field)).collect();
    if keywords.is_empty() {
        return BTreeMap::new();
    }

    let data = ReadOnlyData::new(
        input
            .read_only_data
            .iter()
            .map(|section| (section.address, section.bytes.clone()))
            .collect(),
    );
    let caller = Caller::new(input, symbol_index, member, &data);
    keywords
        .into_iter()
        .map(|field| {
            let domain = match &caller {
                Some(caller) => caller.domain(field.token as u64, tokens),
                None => Err(CALLER_UNRESOLVED),
            };
            (field.token, domain)
        })
        .collect()
}

/// Why no keyword field that a serializer's member reader reads has a listed domain, by token.
pub(super) fn through_serializer(
    fields: &[RootField],
) -> BTreeMap<i64, Result<Vec<String>, &'static str>> {
    fields
        .iter()
        .filter(|field| is_keyword(field))
        .map(|field| (field.token, Err(THROUGH_SERIALIZER)))
        .collect()
}

/// Whether every reader of `field` is a converter that the walker joined as a keyword.
fn is_keyword(field: &RootField) -> bool {
    !field.readers.is_empty()
        && field.readers.iter().all(|join| {
            matches!(join, ReaderJoin::Stored { callee, kind: crate::ReaderKind::Keyword, .. }
                if readers::converter(callee).is_some())
        })
}

/// The member reader that calls the converters, with the code of each converter it calls.
struct Caller<'a> {
    entry: u64,
    code: Code,
    converters: BTreeMap<u64, (ConverterArgument, Code)>,
    names: BTreeMap<u64, &'a str>,
    data: &'a ReadOnlyData,
    value_token: u64,
}

impl<'a> Caller<'a> {
    fn new(
        input: &'a FieldInput,
        symbol_index: &SymbolIndex,
        member: &str,
        data: &'a ReadOnlyData,
    ) -> Option<Self> {
        let decoded = |name: &str| {
            let function = input
                .functions
                .iter()
                .find(|function| function.name == name)?;
            let rows = decode_arm64(&function.code, function.address).ok()?;
            Some((function.address, Code::from_rows(rows)))
        };
        let (entry, code) = decoded(member)?;
        let converters = input
            .functions
            .iter()
            .filter_map(|function| {
                let argument = readers::converter(&function.name)?;
                let (address, code) = decoded(&function.name)?;
                Some((address, (argument, code)))
            })
            .collect();
        let names = input
            .symbols
            .iter()
            .filter_map(|symbol| {
                let name = symbol_index.name(&input.symbols, symbol.address)?;
                Some((symbol.address, name))
            })
            .collect();

        Some(Self {
            entry,
            code,
            converters,
            names,
            data,
            value_token: u64::try_from(input.key_readers.value_token).ok()?,
        })
    }

    /// The accepted names of the field with the key `token`.
    fn domain(
        &self,
        token: u64,
        tokens: &BTreeMap<i64, Token>,
    ) -> Result<Vec<String>, &'static str> {
        let last = tokens.keys().copied().filter(|token| *token >= 0).max();
        let runtime = last.map_or(0, |last| last as u64 + 1);
        let mut accepted = Vec::new();
        for value in 0..=runtime {
            match self.disposition(token, value) {
                Some(true) => accepted.push(value),
                Some(false) => {}
                None => return Err(CALLER_UNRESOLVED),
            }
        }
        if accepted.last() == Some(&runtime) {
            return Err(UNKNOWN_KEPT);
        }

        let names: BTreeSet<_> = accepted
            .iter()
            .map(|value| tokens.get(&(*value as i64))?.name.clone())
            .collect::<Option<_>>()
            .ok_or(UNNAMED_VALUE)?;
        if names.len() != accepted.len() || names.is_empty() {
            return Err(UNNAMED_VALUE);
        }

        Ok(names.into_iter().collect())
    }

    /// Whether the caller accepts `value` for the key `token`: `Some(true)` when some path
    /// returns with no diagnostic, `Some(false)` when every path reports it, and `None` when a
    /// path does not return through the converter.
    fn disposition(&self, token: u64, value: u64) -> Option<bool> {
        let mut machine = Machine::new(&self.code, self.data);
        let owner = machine.reserve(SPAN);
        let reader = machine.reserve(SPAN);
        machine.write(reader + self.value_token, 4, value & 0xffff_ffff);
        machine.set_register(0, owner);
        machine.set_register(1, reader);
        machine.set_register(2, token);
        let state = [owner..owner + SPAN, reader..reader + SPAN];
        let paths = machine.run_paths(self.entry, &mut |target, machine| {
            self.call(target, value, &state, machine)
        });

        let accepted_on_paths = paths
            .iter()
            .map(|path| {
                let converted = path.machine.labelled(CONVERTED).is_some();
                (path.end == Ok(Exit::Returned) && converted)
                    .then(|| path.machine.labelled(EMITTED).is_none())
            })
            .collect::<Option<Vec<_>>>()?;

        Some(accepted_on_paths.contains(&true))
    }

    /// What one path of the caller does at a call.
    fn call(
        &self,
        target: Option<u64>,
        value: u64,
        state: &[std::ops::Range<u64>; 2],
        machine: &mut Machine<'_>,
    ) -> Result<Call, Unresolved> {
        let target = target.ok_or(Unresolved::new("keyword-indirect-call"))?;
        if let Some((argument, code)) = self.converters.get(&target) {
            let result = self.convert(*argument, code, target, value, machine)?;
            machine.label(CONVERTED, result);
            return Ok(Call::Return(Some(result)));
        }

        let callee = self.names.get(&target).copied().unwrap_or("");
        if readers::DIAGNOSTICS.contains(&callee) {
            machine.label(EMITTED, 1);
            return Ok(Call::Return(None));
        }
        let receives_state = (0..ARGUMENT_REGISTERS)
            .filter_map(|index| machine.register(index))
            .any(|argument| state.iter().any(|range| range.contains(&argument)));
        if receives_state && !readers::changes_no_script_state(callee) {
            return Err(Unresolved::new("keyword-call"));
        }

        Ok(Call::Return(None))
    }

    /// What the converter `code` returns for `value`. The caller must pass the value as the
    /// converter receives its token.
    fn convert(
        &self,
        argument: ConverterArgument,
        code: &Code,
        entry: u64,
        value: u64,
        machine: &Machine<'_>,
    ) -> Result<u64, Unresolved> {
        let word = value & 0xffff_ffff;
        let mut converter = Machine::new(code, self.data);
        match argument {
            ConverterArgument::Pointer => {
                let pointer = machine.known_register(0, "keyword-argument")?;
                if machine.read(pointer, 4) != Some(word) {
                    return Err(Unresolved::new("keyword-argument"));
                }
                let copy = converter.allocate(4);
                converter.write(copy, 4, word);
                converter.set_register(0, copy);
            }
            ConverterArgument::Word => {
                if machine.known_register(0, "keyword-argument")? & 0xffff_ffff != word {
                    return Err(Unresolved::new("keyword-argument"));
                }
                converter.set_register(0, word);
            }
        }

        match converter.run(entry, &mut |_, _| {
            Err(Unresolved::new("keyword-converter-call"))
        })? {
            Exit::Returned => Ok(converter.known_register(0, "keyword-result")? & 0xffff_ffff),
            _ => Err(Unresolved::new("keyword-converter-end")),
        }
    }
}
