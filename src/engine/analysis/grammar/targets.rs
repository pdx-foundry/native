//! Target scope constraints come from whole paths over each input scope bit.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use super::{AccessorNullObject, GrammarInput};
use crate::engine::analysis::evaluate::{Call, Code, Exit, Machine};
use crate::engine::analysis::stop::Unresolved;

/// Executable routes and scope-reference layout used by target probes.
#[derive(Default)]
pub struct Input {
    pub scope_type_offset: u64,
    pub scope_object_offset: u64,
    pub helpers: BTreeSet<u64>,
    pub nulls: BTreeMap<u64, AccessorNullObject>,
    pub table: OnceLock<BTreeMap<u64, Getter>>,
}

/// A per-input-bit result; an unknown return is never evidence of acceptance.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum Bit {
    /// At least one returning path proves a valid object.
    Accepted,
    /// Every returning path returns the bound null object.
    Rejected,
    /// A path or return value could not be established.
    Unresolved(&'static str),
}

/// All input bits of one getter, retained for the developer census.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Getter {
    /// Results ordered by the executable scope bit.
    pub bits: Vec<Bit>,
}

impl Getter {
    /// Accepted bits, only when every input bit finished.
    pub fn mask(&self) -> Result<u64, &'static str> {
        self.bits
            .iter()
            .enumerate()
            .try_fold(0, |mask, (bit, result)| match result {
                Bit::Accepted => Ok(mask | (1 << bit)),
                Bit::Rejected => Ok(mask),
                Bit::Unresolved(reason) => Err(*reason),
            })
    }
}

/// Evaluate each getter once for each scope bit, then share only that address-keyed table.
pub fn getter_table(input: &GrammarInput) -> &BTreeMap<u64, Getter> {
    input.targets.table.get_or_init(|| {
        let functions: BTreeSet<_> = input
            .targets
            .helpers
            .iter()
            .copied()
            .chain(input.targets.nulls.keys().copied())
            .collect();
        let ranges: Vec<_> = functions
            .iter()
            .filter_map(|at| {
                input
                    .declarations
                    .functions
                    .get(at)
                    .map(|body| (*at, body.code.as_slice()))
            })
            .collect();
        let code = Code::decode(&ranges);
        input
            .targets
            .nulls
            .keys()
            .map(|&address| {
                let count = input
                    .declarations
                    .scope_names
                    .as_ref()
                    .map_or(64, Vec::len)
                    .min(64);
                let bits = (0..count)
                    .map(|bit| match &code {
                        Ok(code) => getter_bit(input, code, address, bit),
                        Err(_) => Bit::Unresolved("getter code"),
                    })
                    .collect();
                (address, Getter { bits })
            })
            .collect()
    })
}

fn getter_bit(input: &GrammarInput, code: &Code, address: u64, bit: usize) -> Bit {
    if input.forms.cut_bodies.contains(&address) {
        return Bit::Unresolved("cut getter body");
    }
    let Some(AccessorNullObject::Global(null_slot)) = input.targets.nulls.get(&address) else {
        return Bit::Unresolved("no null object");
    };
    let mut machine = Machine::new(code, input.declarations.pointer_data());
    machine.intercept_tail_calls(
        input
            .targets
            .helpers
            .iter()
            .copied()
            .chain(input.targets.nulls.keys().copied())
            .collect(),
    );
    let object = machine.reserve(0x1000);
    let scope = machine.reserve(0x1000);
    let target = machine.reserve(input.forms.target_size.max(1));
    write_scope(input, &mut machine, scope, bit, object);
    let mut nulls = BTreeMap::new();
    for null in input.targets.nulls.values() {
        if let AccessorNullObject::Global(slot) = null {
            let pointer = *nulls.entry(*slot).or_insert_with(|| {
                input
                    .declarations
                    .pointer_data()
                    .read(*slot, 8)
                    .filter(|pointer| *pointer != 0)
                    .unwrap_or_else(|| machine.reserve(0x1000))
            });
            machine.write(*slot, 8, pointer);
        }
    }
    let null = nulls[null_slot];
    if input
        .command_bindings
        .scope_accessors
        .contains_key(&address)
    {
        machine.set_register(0, scope);
    } else {
        machine.set_register(0, target);
        let context = machine.reserve(0x1000);
        machine.set_register(1, context);
    }
    machine.watch_reads(scope, 0x1000);
    let paths = machine.run_paths_joining(address, &mut |callee, machine| {
        let Some(callee) = callee else {
            return Err(Unresolved::new("unresolved indirect call"));
        };
        if callee == input.command_bindings.target_resolver {
            if machine.register(0) != Some(target) {
                return Err(Unresolved::new("resolver received another target"));
            }
            let destination = machine.known_register(8, "resolver destination")?;
            write_scope(input, machine, destination, bit, object);
            return Ok(Call::Return(None));
        }
        if matches!(
            input.targets.nulls.get(&callee),
            Some(AccessorNullObject::NoNullObject)
        ) {
            return Err(Unresolved::new("no null object"));
        }
        if input.targets.helpers.contains(&callee) || input.targets.nulls.contains_key(&callee) {
            if input.forms.cut_bodies.contains(&callee) {
                return Err(Unresolved::new("cut getter body"));
            }
            return Ok(Call::Enter);
        }
        Err(Unresolved::new("unclassified getter call"))
    });
    let mut accepted = false;
    let mut returned = false;
    for path in paths {
        match path.end {
            Ok(Exit::Trapped) => continue,
            Ok(Exit::Returned) => returned = true,
            Err(stop) => return Bit::Unresolved(stop.reason),
            _ => return Bit::Unresolved("unfinished getter path"),
        }
        if path.machine.receiver_reads().values().any(Option::is_none) {
            return Bit::Unresolved("unknown scope state");
        }
        match path.machine.register(0) {
            Some(value) if value == object => accepted = true,
            Some(value) if value == null => {}
            _ => return Bit::Unresolved("unknown result"),
        }
    }
    if accepted {
        Bit::Accepted
    } else if returned {
        Bit::Rejected
    } else {
        Bit::Unresolved("no returning path")
    }
}

fn write_scope(
    input: &GrammarInput,
    machine: &mut Machine<'_>,
    scope: u64,
    bit: usize,
    object: u64,
) {
    machine.write(scope + input.targets.scope_type_offset, 8, 1 << bit);
    machine.write(scope + input.targets.scope_object_offset, 8, object);
}

/// One stage either proves absence, proves a set, or cannot establish its check.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum Check {
    /// No path reads or passes the target.
    Absent,
    /// One accepted input mask, established by every touching path.
    Established(u64),
    /// A touched stage has no complete scope proof.
    Unresolved(&'static str),
}

/// Facts about one stored target, before scope names are normalized.
#[derive(Debug, Clone)]
pub struct Argument {
    pub path: crate::ArgumentPath,
    pub scopes: crate::engine::analysis::declarations::ScopeOutcome,
    pub stage: crate::TargetCheckStage,
    pub checks: [Check; 3],
    pub cause: Option<&'static str>,
}

fn combine(checks: &[Check; 3]) -> Result<(u64, crate::TargetCheckStage), &'static str> {
    use crate::TargetCheckStage::{Execution, Validation, WhileReading};
    if let Some(reason) = checks.iter().find_map(|check| match check {
        Check::Unresolved(reason) => Some(*reason),
        _ => None,
    }) {
        return Err(reason);
    }
    let established: Vec<_> = checks
        .iter()
        .zip([WhileReading, Validation, Execution])
        .filter_map(|(check, stage)| match check {
            Check::Established(mask) => Some((*mask, stage)),
            _ => None,
        })
        .collect();
    let Some(&(mask, stage)) = established.first() else {
        return Err("target not checked");
    };
    if mask == 0 {
        return Err("zero type set");
    }
    if established.iter().any(|(later, _)| later & mask != mask) {
        return Err("stage type sets differ");
    }
    Ok((mask, stage))
}

const CHECKED: u64 = 40;
const RESOLVED: u64 = 41;
const OTHER_RESOLVED: u64 = 42;

/// Execution runs on the parsed command. The factory's bytes do not describe it, because readers
/// overwrite members that the method cannot always list, so only the vtable is installed. A
/// branch on any receiver byte keeps both sides, and differing facts give `receiver-state`.
fn execution(
    input: &GrammarInput,
    reader: crate::engine::analysis::declarations::CommandReader,
    offset: u64,
    targets: &BTreeSet<u64>,
) -> Check {
    use crate::engine::analysis::commands::{OBJECT_SPAN, stand_in_command};
    let Some(&function) = input
        .declarations
        .pointers
        .get(&(reader.vtable + input.forms.role_slot))
    else {
        return Check::Unresolved("missing execution slot");
    };
    let Some(body) = input.declarations.functions.get(&function) else {
        return Check::Unresolved("missing execution body");
    };
    if input.forms.cut_bodies.contains(&function) {
        return Check::Unresolved("cut execution body");
    }
    let Ok(code) = Code::decode(&[(function, &body.code)]) else {
        return Check::Unresolved("execution code");
    };
    let mut machine = Machine::new(&code, input.declarations.pointer_data());
    let owner = stand_in_command(&mut machine, reader.vtable);
    machine.watch_reads(owner, OBJECT_SPAN);
    machine.watch_accesses();
    let context = machine.reserve(OBJECT_SPAN);
    machine.set_register(0, owner);
    machine.set_register(1, context);
    let target_start = owner + offset;
    let table = getter_table(input);
    let paths = machine.run_paths_joining(function, &mut |callee, machine| {
        let receiver = machine.register(0);
        let other_target = receiver.is_some_and(|address| {
            targets.iter().any(|&other| {
                address == owner + other
                    && (other + input.forms.target_size <= offset
                        || offset + input.forms.target_size <= other)
            })
        });
        let typed_getter =
            callee.is_some_and(|at| input.command_bindings.target_getters.contains(&at));
        let accessor =
            callee.is_some_and(|at| input.command_bindings.scope_accessors.contains_key(&at));
        if other_target && callee == Some(input.command_bindings.target_resolver) {
            let destination = machine.known_register(8, "resolver destination")?;
            if machine.labelled(destination) == Some(RESOLVED) {
                return Err(Unresolved::new("overlapping resolved scopes"));
            }
            machine.label(destination, OTHER_RESOLVED);
            return Ok(Call::Return(None));
        }
        if (other_target && typed_getter)
            || (accessor
                && receiver
                    .is_some_and(|address| machine.labelled(address) == Some(OTHER_RESOLVED)))
        {
            return Ok(Call::Return(None));
        }
        if callee == Some(input.command_bindings.target_resolver) && receiver == Some(target_start)
        {
            let destination = machine.known_register(8, "resolver destination")?;
            machine.label(RESOLVED, destination);
            machine.label(destination, RESOLVED);
            return Ok(Call::Return(None));
        }
        if let Some(getter) = callee.and_then(|at| table.get(&at)) {
            let typed_target =
                callee.is_some_and(|at| input.command_bindings.target_getters.contains(&at));
            let resolves_target = if typed_target {
                receiver == Some(target_start)
            } else {
                receiver.is_some_and(|address| machine.labelled(address) == Some(RESOLVED))
            };
            if resolves_target {
                let mask = getter.mask().map_err(|reason| {
                    Unresolved::new(if reason == "unresolved indirect call" {
                        "getter unresolved indirect call"
                    } else {
                        reason
                    })
                })?;
                if machine.labelled(CHECKED).is_some_and(|prior| prior != mask) {
                    return Err(Unresolved::new("different type sets"));
                }
                machine.label(CHECKED, mask);
                return Ok(Call::Return(Some(machine.reserve(OBJECT_SPAN))));
            }
        }
        Err(Unresolved::new(if callee.is_none() {
            "unresolved indirect call"
        } else {
            "unclassified execution call"
        }))
    });
    let mut sets = BTreeSet::new();
    let mut facts = Vec::new();
    for path in paths {
        match path.end {
            Ok(Exit::Trapped) => continue,
            Ok(Exit::Returned) => {}
            Err(stop) => return Check::Unresolved(stop.reason),
            _ => return Check::Unresolved("unfinished execution path"),
        }
        if !path.machine.unknown_stores().is_empty() {
            return Check::Unresolved("unknown execution store");
        }
        let scope_span = input
            .targets
            .scope_object_offset
            .max(input.targets.scope_type_offset)
            + 8;
        if path.machine.labels().iter().any(|(&scope, &label)| {
            scope > RESOLVED
                && label == RESOLVED
                && path.machine.accessed_address(scope, scope_span)
        }) {
            return Check::Unresolved("unclassified resolved scope load");
        }
        if path.machine.accessed(offset, input.forms.target_size) {
            return Check::Unresolved("unclassified target load");
        }
        facts.push((
            path.machine.decisions(),
            Some((
                path.machine.labelled(CHECKED),
                path.machine.labelled(RESOLVED).is_some(),
            )),
        ));
        if let Some(mask) = path.machine.labelled(CHECKED) {
            sets.insert(mask);
        } else if path.machine.labelled(RESOLVED).is_some() {
            sets.insert(u64::MAX);
        }
    }
    if crate::engine::analysis::commands::receiver_dependent(&facts) {
        return Check::Unresolved("receiver-state");
    }
    match sets.len() {
        0 => Check::Absent,
        1 => Check::Established(*sets.first().unwrap()),
        _ => Check::Unresolved("different type sets"),
    }
}

/// Analyze stored arguments, retaining unresolved checks alongside established ones.
pub fn analyze(
    input: &GrammarInput,
    grammar: &super::GrammarResult,
    state: &BTreeMap<u64, u8>,
) -> Vec<Argument> {
    use crate::engine::analysis::declarations::{ScopeOutcome, scope_mask};
    use crate::{ArgumentPath, ReaderKind, TargetCheckStage};
    let mut found = Vec::new();
    if let Some(forms) = &grammar.forms {
        for value in &forms.alternatives {
            if value.accepted && value.value.kind == ReaderKind::Target {
                found.push((ArgumentPath::Value, value.value.destination, None));
            }
        }
    }
    if !grammar.value_only() {
        collect_keys(grammar, &[], Some(0), &mut found);
    }
    let mut collected = Vec::new();
    for (path, mut offset, member) in found.iter().cloned() {
        if found
            .iter()
            .any(|(other_path, other_offset, other_member)| {
                other_path == &path && (*other_offset != offset || *other_member != member)
            })
        {
            offset = None;
        }
        if collected.iter().any(|(prior, _, _)| prior == &path) {
            continue;
        }
        let offset = offset.filter(|offset| {
            offset
                .checked_add(input.forms.target_size)
                .is_some_and(|end| end <= crate::engine::analysis::commands::OBJECT_SPAN)
        });
        collected.push((path, offset, member));
    }
    let targets = collected
        .iter()
        .filter_map(|(_, offset, _)| *offset)
        .collect();
    let mut arguments = Vec::new();
    for (path, offset, member) in collected {
        let checks = if let Some(offset) = offset {
            let earlier = super::forms::target_checks(input, grammar.reader, state, offset, member);
            [
                earlier[0].clone(),
                earlier[1].clone(),
                execution(input, grammar.reader, offset, &targets),
            ]
        } else {
            [
                Check::Unresolved("target destination not established"),
                Check::Absent,
                Check::Absent,
            ]
        };
        let (scopes, stage, cause) = match combine(&checks) {
            Ok((mask, stage)) => {
                let scopes = scope_mask(mask, input.declarations.scope_names.as_deref());
                if matches!(scopes, ScopeOutcome::Unresolved(_)) {
                    (
                        scopes,
                        TargetCheckStage::Unresolved,
                        Some("scope names unresolved"),
                    )
                } else {
                    (scopes, stage, None)
                }
            }
            Err(cause) => (
                ScopeOutcome::Unresolved(Unresolved::new(cause)),
                TargetCheckStage::Unresolved,
                Some(cause),
            ),
        };
        arguments.push(Argument {
            path,
            scopes,
            stage,
            checks,
            cause,
        });
    }
    arguments
}

fn collect_keys(
    grammar: &super::GrammarResult,
    path: &[String],
    base: Option<u64>,
    found: &mut Vec<(crate::ArgumentPath, Option<u64>, Option<Member>)>,
) {
    use crate::engine::analysis::readers;
    for field in &grammar.fields.fields {
        let mut path = path.to_vec();
        path.push(field.name.clone());
        let offsets: BTreeSet<_> = field.readers.iter().map(readers::destination).collect();
        let offset = if offsets.len() == 1 {
            offsets
                .first()
                .copied()
                .flatten()
                .and_then(|offset| u64::try_from(offset).ok())
                .and_then(|offset| base?.checked_add(offset))
        } else {
            None
        };
        if readers::classify(&field.readers).kind == crate::ReaderKind::Target {
            found.push((
                crate::ArgumentPath::Key(path.clone()),
                offset,
                base.map(|receiver| Member {
                    entry: grammar.reader.member,
                    receiver,
                    token: field.token,
                }),
            ));
        }
        if let Some(child) = grammar.nested.get(&field.name) {
            collect_keys(child, &path, offset, found);
        }
    }
}

/// A proved member receiver and the key token whose reader stores the argument.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Member {
    pub entry: u64,
    pub receiver: u64,
    pub token: i64,
}

#[cfg(test)]
mod tests;
