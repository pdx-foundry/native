//! Value forms are read facts followed by the complete initialization and validation chain.
//! A diagnostic proves rejection. A false or unknown result without a diagnostic proves neither
//! acceptance nor rejection. Receiver state not established by the factory cannot prove a form.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use super::GrammarInput;
use crate::ReaderKind;
use crate::engine::analysis::{
    commands::{OBJECT_SPAN, forget_if_passed, stand_in_command},
    declarations::CommandReader,
    evaluate::{Call, Code, Decision, Exit, Machine, Path},
    fields::{ReaderJoin, Value},
    readers,
    references::{Missing, initialization::InitializationLookup},
    stop::Unresolved,
};

const BLOCK: u64 = 1;
const ASSIGN: u64 = 2;
const DIAGNOSED: u64 = 3;
const ALTERNATIVE: u64 = 4;
const TOKEN_COPY: u64 = 5;
const TARGET_TEMP: u64 = 6;
const STRING_TEMP: u64 = 7;
const TOKEN_TEXT: u64 = FOUND + 0x30000;
const FOUND: u64 = 0x7000_0000_0000;
const MISSING: u64 = FOUND + 0x10000;

/// Executable-bound details needed only by the forms chain.
#[derive(Default)]
pub struct Input {
    pub reader_value_token_offset: Option<u64>,
    pub token_text_offset: u64,
    pub target_size: u64,
    pub string_size: u64,
    pub role_slot: u64,
    pub shared: BTreeMap<u64, String>,
    pub qualified_references: BTreeMap<u64, u64>,
    pub assignments: BTreeSet<u64>,
    pub string_copies: BTreeSet<u64>,
    pub strings_from_text: BTreeSet<u64>,
    pub harmless: BTreeSet<u64>,
    pub dynamic_name: u64,
    pub interner: u64,
    pub initializers: BTreeMap<u64, Initializer>,
    pub cut_bodies: BTreeSet<u64>,
    pub cache: Mutex<BTreeMap<CacheKey, Arc<Result>>>,
}

/// The reference method's whole-body proof, translated to executable addresses.
#[derive(Clone)]
pub enum Initializer {
    /// Run the body; the reference method found no lookup to summarize.
    NoLookup,
    Lookup {
        lookup: InitializationLookup,
        execution: LookupExecution,
    },
    Unresolved,
}

#[derive(Clone)]
pub enum LookupExecution {
    /// A whole-body inline lookup. Conditional lookups cannot be summarized.
    Inline { always: bool, null: u64 },
    /// Run the body and intercept only the established getter on this database.
    Getter {
        database: u64,
        getter: u64,
        null: u64,
    },
}

/// Selected slots, reached callees and initial receiver bytes watched by every probe.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CacheKey {
    /// Read, member, assign, initializer, validation and role slots, in that order.
    pub slots: [Option<u64>; 6],
    /// Every entered or classified target, including virtual helpers and tail calls.
    pub functions: BTreeSet<u64>,
    /// Factory-established byte values, or an explicit unknown input.
    pub receiver: BTreeMap<u64, Option<u8>>,
}

/// The engine stage responsible for a value-path fact or obstruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub enum Stage {
    /// The outer reader, before deferred reference resolution.
    Read,
    /// Assignment of the reader's value token.
    Assign,
    /// Initialization after deferred references have resolved.
    PostInit,
    /// Validation of the initialized command.
    PostValidate,
}

/// Acceptance requires known success; rejection requires a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum PathClass {
    /// Every result-bearing stage succeeds without a diagnostic.
    Accepting,
    /// A stage emits a diagnostic.
    Rejecting,
    /// A bound, unknown fact, or false result without a diagnostic prevents a conclusion.
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct StageResult {
    pub stage: Stage,
    pub returned: Option<bool>,
    pub diagnostic: bool,
    pub cause: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ChainPath {
    pub stages: Vec<StageResult>,
    pub class: PathClass,
    pub stops: Vec<Unresolved>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueForm {
    pub kind: ReaderKind,
    pub destination: Option<u64>,
    pub reader: Option<ReaderJoin>,
    pub initialization: Option<InitializationLookup>,
    pub deferred_null: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alternative {
    pub value: ValueForm,
    pub paths: Vec<ChainPath>,
    pub missing: Vec<ChainPath>,
    pub accepted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Result {
    pub key: CacheKey,
    pub block: bool,
    pub alternatives: Vec<Alternative>,
    pub complete: bool,
    pub receiver_state: bool,
    pub stops: Vec<Unresolved>,
}

/// Analyze each receiver before sharing by its complete key. Return the independently computed
/// key beside the shared result so the population audit can check each cache reuse.
pub(super) fn analyze(
    input: &GrammarInput,
    reader: CommandReader,
    state: &BTreeMap<u64, u8>,
) -> (Arc<Result>, CacheKey) {
    let slots = input.declarations.parser_slots;
    let binding = &input.command_bindings;
    let at = |slot| {
        input
            .declarations
            .pointers
            .get(&(reader.vtable + slot))
            .copied()
    };
    let functions = [
        Some(reader.read),
        Some(reader.member),
        at(binding.assign_slot),
        at(slots.initializer),
        at(binding.validation_slot),
        at(input.forms.role_slot),
    ];
    let mut result = examine(input, reader, state, functions);
    let independent_key = result.key.clone();
    let mut cache = input.forms.cache.lock().unwrap();
    if let Some(known) = cache.get(&result.key) {
        if **known == result {
            return (known.clone(), independent_key);
        }
        result.complete = false;
        result.stops.push(Unresolved::new("form-cache-conflict"));
        return (Arc::new(result), independent_key);
    }
    let result = Arc::new(result);
    cache.insert(result.key.clone(), result.clone());
    (result, independent_key)
}

fn examine(
    input: &GrammarInput,
    reader: CommandReader,
    state: &BTreeMap<u64, u8>,
    functions: [Option<u64>; 6],
) -> Result {
    let mut result = Result {
        key: CacheKey {
            slots: functions,
            functions: functions.into_iter().flatten().collect(),
            receiver: BTreeMap::new(),
        },
        block: false,
        alternatives: vec![],
        complete: true,
        receiver_state: false,
        stops: vec![],
    };
    if input.forms.cut_bodies.contains(&reader.read) {
        result.complete = false;
        result.stops.push(Unresolved::new("form-cut-body"));
        return result;
    }
    let Ok(code) = code(input, &functions) else {
        result.complete = false;
        result.stops.push(Unresolved::new("form-code"));
        return result;
    };
    let marker = marker(input);
    let probes = [
        None,
        Some(input.command_bindings.boolean_tokens[0] as u64),
        Some(input.command_bindings.boolean_tokens[1] as u64),
        Some(marker),
    ];
    let mut runs = Vec::new();
    for token in probes {
        runs.push(run(
            input, &code, reader, state, functions, token, false, None,
        ));
    }
    for run in &runs {
        extend_key(&mut result.key, run, state);
    }
    let boolean = boolean_difference(&runs[1], &runs[2]);
    // The three concrete probes stand for every token only when they cover each outcome of the
    // unbounded run. A token that the reader or `Assign` compares specially adds outcomes there.
    if boolean && !probes_cover(&runs[0], &runs[1..]) {
        result.complete = false;
        result.stops.push(Unresolved::new("form-token-coverage"));
    }
    for (probe, run) in runs.iter_mut().enumerate() {
        if matches!(probe, 1 | 2) && boolean {
            for path in &mut run.paths {
                if path.assigned && path.chain.class == PathClass::Accepting {
                    path.value = Some(ValueForm {
                        kind: ReaderKind::Boolean,
                        destination: None,
                        reader: None,
                        initialization: None,
                        deferred_null: None,
                    });
                }
            }
        }
    }
    // The unbounded token run owns ordinary value alternatives. Concrete Boolean probes own
    // Boolean alternatives, and the non-literal probe proves whether any other value is open.
    let chosen: Vec<_> = if boolean { vec![1, 2, 3] } else { vec![0] };
    for &index in &chosen {
        let run = &runs[index];
        let block_dependent = receiver_dependent(&run.paths, |path| path.block.then_some(()));
        result.block |= run.block && !block_dependent;
        result.receiver_state |= block_dependent;
        result.stops.extend(run.stops.clone());
        result.complete &= run.stops.is_empty();
        for path in &run.paths {
            let value = path.value.clone().unwrap_or(ValueForm {
                kind: ReaderKind::Unknown,
                destination: None,
                reader: None,
                initialization: None,
                deferred_null: None,
            });
            if !path.assigned && path.value.is_none() {
                continue;
            }
            let position = result
                .alternatives
                .iter()
                .position(|a| a.value == value)
                .unwrap_or_else(|| {
                    result.alternatives.push(Alternative {
                        value,
                        paths: vec![],
                        missing: vec![],
                        accepted: false,
                    });
                    result.alternatives.len() - 1
                });
            result.alternatives[position].paths.push(path.chain.clone());
        }
    }
    if boolean
        && runs[3]
            .paths
            .iter()
            .any(|path| path.assigned && path.chain.class != PathClass::Rejecting)
    {
        result.complete = false;
    }
    let mut dependent_values = Vec::new();
    for alternative in &result.alternatives {
        if chosen.iter().any(|&index| {
            receiver_dependent(&runs[index].paths, |path| {
                (path.value.as_ref() == Some(&alternative.value)
                    || (path.value.is_none()
                        && path.assigned
                        && alternative.value.kind == ReaderKind::Unknown))
                    .then_some(path.chain.class)
            })
        }) {
            dependent_values.push(alternative.value.clone());
        }
    }
    result.receiver_state |= !dependent_values.is_empty();
    result.complete &= !result.receiver_state;
    let missing = result
        .alternatives
        .iter()
        .any(|alternative| alternative.value.kind == ReaderKind::Reference)
        .then(|| run(input, &code, reader, state, functions, None, true, None));
    if let Some(missing) = &missing {
        extend_key(&mut result.key, missing, state);
    }
    for alternative in &mut result.alternatives {
        alternative.missing = missing
            .as_ref()
            .into_iter()
            .flat_map(|run| &run.paths)
            .filter(|path| path.value.as_ref() == Some(&alternative.value))
            .map(|path| path.chain.clone())
            .collect();
        alternative.accepted = !dependent_values.contains(&alternative.value)
            && alternative.paths.len() <= 64
            && alternative.value.kind != ReaderKind::Unknown
            && !alternative.paths.is_empty()
            && alternative
                .paths
                .iter()
                .all(|path| path.class == PathClass::Accepting);
        let rejected = alternative
            .paths
            .iter()
            .all(|path| path.class == PathClass::Rejecting);
        let unknown_reader = alternative.value.kind == ReaderKind::Unknown
            && alternative.value.destination.is_some();
        if unknown_reader {
            result.stops.push(Unresolved::new("form-reader-kind"));
        }
        result.complete &=
            !unknown_reader && (alternative.accepted || rejected) && alternative.paths.len() <= 64;
    }
    crate::engine::analysis::stop::sort_and_dedup(&mut result.stops);
    result
}

fn extend_key(key: &mut CacheKey, run: &Run, state: &BTreeMap<u64, u8>) {
    key.functions.extend(&run.functions);
    for &(offset, width) in run.reads.keys() {
        if offset < 8 {
            continue;
        }
        for byte in offset..offset + width {
            key.receiver.insert(byte, state.get(&byte).copied());
        }
    }
}

fn code(
    input: &GrammarInput,
    functions: &[Option<u64>; 6],
) -> std::result::Result<Code, Unresolved> {
    let mut pending: Vec<_> = functions.iter().flatten().copied().collect();
    let mut visited = BTreeSet::new();
    let mut rows = Vec::new();
    while let Some(address) = pending.pop() {
        if !visited.insert(address) {
            continue;
        }
        let Some(body) = input.declarations.functions.get(&address) else {
            continue;
        };
        let body = crate::engine::analysis::decode::decode_arm64(&body.code, body.address)
            .map_err(|_| Unresolved::new("form-code"))?;
        pending.extend(
            body.iter()
                .filter(|row| matches!(row.operation.as_str(), "bl" | "b"))
                .filter_map(|row| crate::engine::analysis::declarations::number(&row.operands))
                .filter(|target| input.forms.assignments.contains(target)),
        );
        rows.extend(body);
    }
    Ok(Code::from_rows(rows))
}

fn marker(input: &GrammarInput) -> u64 {
    (0..=u32::MAX)
        .rev()
        .find(|id| !input.tokens.contains_key(&i64::from(*id)))
        .unwrap() as u64
}

struct Run {
    functions: BTreeSet<u64>,
    paths: Vec<ReadPath>,
    block: bool,
    reads: BTreeMap<(u64, u64), Option<u64>>,
    stops: Vec<Unresolved>,
}
struct ReadPath {
    trapped: bool,
    target_touches: [bool; 2],
    target_load: bool,
    decisions: Vec<Decision>,
    value: Option<ValueForm>,
    assigned: bool,
    block: bool,
    chain: ChainPath,
    bytes: BTreeMap<u64, u8>,
}

fn receiver_dependent<T: PartialEq>(
    paths: &[ReadPath],
    fact: impl Fn(&ReadPath) -> Option<T>,
) -> bool {
    let facts: Vec<_> = paths
        .iter()
        .map(|path| (path.decisions.clone(), fact(path)))
        .collect();
    crate::engine::analysis::commands::receiver_dependent(&facts)
}

/// Whether every outcome of the unbounded run also occurs in a concrete-token run, and the
/// unbounded run has no stop of its own. An outcome is the path class, block, assignment and
/// value form, with the stored command bytes of an accepting path. Outcomes are compared as sets:
/// without negative constraints, the unbounded run can repeat an outcome on an infeasible fork.
fn probes_cover(unbounded: &Run, probes: &[Run]) -> bool {
    let outcome = |path: &ReadPath| {
        let accepting = path.chain.class == PathClass::Accepting;
        (
            path.chain.class,
            path.block,
            path.assigned,
            path.value
                .as_ref()
                .map(|value| (value.kind, value.destination, value.reader.clone())),
            accepting.then(|| path.bytes.clone()),
        )
    };
    let probed: Vec<_> = probes
        .iter()
        .flat_map(|run| &run.paths)
        .map(outcome)
        .collect();
    unbounded.stops.is_empty()
        && unbounded
            .paths
            .iter()
            .all(|path| probed.contains(&outcome(path)))
}

fn boolean_difference(yes: &Run, no: &Run) -> bool {
    let accepted = |run: &Run| {
        !run.paths.is_empty()
            && run
                .paths
                .iter()
                .all(|path| path.assigned && path.chain.class == PathClass::Accepting)
    };
    let paired = |left: &Run, right: &Run| {
        left.paths.iter().all(|left| {
            let partners: Vec<_> = right
                .paths
                .iter()
                .filter(|right| left.decisions == right.decisions)
                .collect();
            !partners.is_empty()
                && partners.iter().all(|right| {
                    left.bytes.iter().any(|(offset, value)| {
                        *offset >= 8 && right.bytes.get(offset).is_some_and(|other| other != value)
                    })
                })
        })
    };
    accepted(yes) && accepted(no) && paired(yes, no) && paired(no, yes)
}

#[allow(clippy::too_many_arguments)] // The optional target probe reuses this exact stage chain.
fn run(
    input: &GrammarInput,
    code: &Code,
    reader: CommandReader,
    state: &BTreeMap<u64, u8>,
    functions: [Option<u64>; 6],
    token: Option<u64>,
    missing: bool,
    probe: Option<TargetProbe>,
) -> Run {
    if probe.is_none_or(|probe| probe.member.is_none())
        && input
            .forms
            .shared
            .get(&reader.read)
            .is_some_and(|name| readers::entry(name).0 == ReaderKind::Block)
    {
        return Run {
            functions: BTreeSet::from([reader.read]),
            paths: vec![],
            block: true,
            reads: BTreeMap::new(),
            stops: vec![],
        };
    }
    let mut machine = Machine::new(code, input.declarations.pointer_data());
    machine.intercept_tail_calls(
        functions
            .iter()
            .flatten()
            .copied()
            .chain(input.forms.assignments.iter().copied())
            .collect(),
    );
    let command = stand_in_command(&mut machine, reader.vtable);
    for (&offset, &value) in state {
        machine.write(command + offset, 1, value.into());
    }
    let source = machine.reserve(OBJECT_SPAN);
    machine.write(
        source + input.command_bindings.reader_value_token_offset + input.forms.token_text_offset,
        8,
        TOKEN_TEXT,
    );
    if let Some(token) = token {
        machine.write(
            source + input.command_bindings.reader_value_token_offset,
            4,
            token,
        );
    }
    machine.watch_reads(command, OBJECT_SPAN);
    if probe.is_some() {
        machine.watch_accesses();
    }
    machine.set_register(0, command);
    machine.set_register(1, source);
    if let Some(member) = probe.and_then(|probe| probe.member) {
        machine.set_register(0, command + member.receiver);
        machine.set_register(2, member.token as u64);
    }
    let mut calls = Calls {
        input,
        command,
        source,
        reader,
        functions,
        probe,
        values: vec![],
        reached: functions.into_iter().flatten().collect(),
    };
    let paths = machine.run_paths_joining(
        probe
            .and_then(|probe| probe.member)
            .map_or(reader.read, |member| member.entry),
        &mut |at, machine| calls.read(at, machine),
    );
    let mut result = Run {
        functions: BTreeSet::new(),
        paths: vec![],
        block: false,
        reads: BTreeMap::new(),
        stops: vec![],
    };
    for path in paths {
        let block = path.machine.labelled(BLOCK).is_some();
        result.block |= block;
        let assigned = path.machine.labelled(ASSIGN).is_some();
        let value = path
            .machine
            .labelled(ALTERNATIVE)
            .map(|index| calls.values[index as usize].clone());
        let stage = if assigned { Stage::Assign } else { Stage::Read };
        let mut first = stage_result(&path, stage, assigned);
        if assigned {
            first.returned = functions[2]
                .and_then(|at| path.machine.returned_value(at))
                .flatten()
                .map(|value| value & 1 == 1);
            if path.end == Ok(Exit::Returned) && !first.diagnostic {
                first.cause = match first.returned {
                    Some(true) => None,
                    Some(false) => Some("false without diagnostic"),
                    None => Some("unknown result"),
                };
            }
        }
        let mut stops = vec![];
        if let Err(stop) = &path.end {
            stops.push(stop.clone());
            result.stops.push(stop.clone());
        }
        if !assigned && value.is_none() {
            result.reads.extend(path.machine.receiver_reads());
            if path.machine.labelled(BLOCK).is_none() && path.end != Ok(Exit::Trapped) {
                result.stops.push(Unresolved::new("form-reader-call"));
            }
            result.paths.push(ReadPath {
                trapped: path.end == Ok(Exit::Trapped),
                target_touches: target_touches(&path.machine),
                target_load: probe.is_some_and(|probe| {
                    path.machine.accessed(probe.offset, input.forms.target_size)
                }),
                decisions: path.machine.decisions(),
                value,
                assigned,
                block,
                chain: chain(vec![first], stops),
                bytes: path.machine.known_bytes(command, OBJECT_SPAN),
            });
            continue;
        }
        let bytes = path.machine.known_bytes(command, OBJECT_SPAN);
        if path.end != Ok(Exit::Returned) {
            result.reads.extend(path.machine.receiver_reads());
            result.paths.push(ReadPath {
                trapped: path.end == Ok(Exit::Trapped),
                target_touches: target_touches(&path.machine),
                target_load: probe.is_some_and(|probe| {
                    path.machine.accessed(probe.offset, input.forms.target_size)
                }),
                decisions: path.machine.decisions(),
                value,
                assigned,
                block,
                chain: chain(vec![first], stops),
                bytes,
            });
            continue;
        }
        let chained = stages(
            &mut calls,
            path.machine,
            functions,
            value.as_ref(),
            missing,
            vec![first],
            stops,
        );
        for (path, chain) in chained {
            result.reads.extend(path.receiver_reads());
            result.paths.push(ReadPath {
                trapped: path.labelled(52).is_some(),
                target_touches: target_touches(&path),
                target_load: probe
                    .is_some_and(|probe| path.accessed(probe.offset, input.forms.target_size)),
                decisions: path.decisions(),
                value: value.clone(),
                assigned,
                block,
                chain,
                bytes: bytes.clone(),
            });
        }
    }
    result.functions = calls.reached;
    if result.paths.len() > 64 {
        result.stops.push(Unresolved::new("form-path-limit"));
    }
    result
}

fn stage_result(path: &Path<'_>, stage: Stage, has_result: bool) -> StageResult {
    let diagnostic = path.machine.labelled(DIAGNOSED).is_some();
    let returned = has_result
        .then(|| path.machine.register(0).map(|value| value & 1 == 1))
        .flatten();
    let cause = match &path.end {
        Err(stop) => Some(stop.reason),
        Ok(Exit::Returned) => {
            if has_result && !diagnostic {
                match returned {
                    Some(true) => None,
                    Some(false) => Some("false without diagnostic"),
                    None => Some("unknown result"),
                }
            } else {
                None
            }
        }
        _ => Some("unfinished path"),
    };
    StageResult {
        stage,
        returned,
        diagnostic,
        cause,
    }
}

fn chain(stages: Vec<StageResult>, stops: Vec<Unresolved>) -> ChainPath {
    let class = if stages.iter().any(|stage| stage.diagnostic) {
        PathClass::Rejecting
    } else if !stops.is_empty() || stages.iter().any(|stage| stage.cause.is_some()) {
        PathClass::Unresolved
    } else {
        PathClass::Accepting
    };
    ChainPath {
        stages,
        class,
        stops,
    }
}

fn stages<'a>(
    calls: &mut Calls<'_>,
    mut machine: Machine<'a>,
    functions: [Option<u64>; 6],
    value: Option<&ValueForm>,
    missing: bool,
    mut stages: Vec<StageResult>,
    stops: Vec<Unresolved>,
) -> Vec<(Machine<'a>, ChainPath)> {
    let selected = if missing { MISSING } else { FOUND };
    if let Some(value) =
        value.filter(|value| value.kind == ReaderKind::Reference && value.initialization.is_none())
    {
        if let Some(null) = value.deferred_null {
            machine.write(null, 8, MISSING);
        } else if missing {
            stages.push(StageResult {
                stage: Stage::PostInit,
                returned: None,
                diagnostic: false,
                cause: Some("missing-key null object not bound"),
            });
        }
        if let Some(offset) = value.destination {
            machine.write(calls.command + offset, 8, selected);
        }
    }
    let mut pending = vec![(machine, stages, stops)];
    for (stage, function) in [
        (Stage::PostInit, functions[3]),
        (Stage::PostValidate, functions[4]),
    ] {
        let mut next = Vec::new();
        for (mut machine, mut stages, stops) in pending {
            if calls.probe.is_some() && machine.labelled(52).is_some() {
                next.push((machine, stages, stops));
                continue;
            }
            if !stops.is_empty()
                || stages.iter().any(|stage| {
                    matches!(
                        stage.cause,
                        Some("initializer not summarized" | "missing stage" | "form-cut-body")
                    )
                })
            {
                next.push((machine, stages, stops));
                continue;
            }
            machine.unlabel(DIAGNOSED);
            let Some(function) = function else {
                stages.push(StageResult {
                    stage,
                    returned: None,
                    diagnostic: false,
                    cause: Some("missing stage"),
                });
                next.push((machine, stages, stops));
                continue;
            };
            if calls.input.forms.cut_bodies.contains(&function) {
                stages.push(StageResult {
                    stage,
                    returned: None,
                    diagnostic: false,
                    cause: Some("form-cut-body"),
                });
                next.push((machine, stages, stops));
                continue;
            }
            if stage == Stage::PostInit {
                match calls.input.forms.initializers.get(&function) {
                    Some(Initializer::Lookup {
                        lookup,
                        execution: LookupExecution::Inline { always: true, null },
                    }) if (!missing || lookup.lookup.on_missing == Some(Missing::NullObject))
                        && value.is_some_and(|value| {
                            value.destination == u64::try_from(lookup.key_offset).ok()
                        }) =>
                    {
                        machine.write(*null, 8, MISSING);
                        machine.write(
                            calls.command.wrapping_add_signed(lookup.item_offset),
                            8,
                            selected,
                        );
                        stages.push(StageResult {
                            stage,
                            returned: None,
                            diagnostic: false,
                            cause: None,
                        });
                        next.push((machine, stages, stops));
                        continue;
                    }
                    Some(
                        Initializer::NoLookup
                        | Initializer::Lookup {
                            execution: LookupExecution::Getter { .. },
                            ..
                        },
                    ) => {}
                    _ => {
                        stages.push(StageResult {
                            stage,
                            returned: None,
                            diagnostic: false,
                            cause: Some("initializer not summarized"),
                        });
                        next.push((machine, stages, stops));
                        continue;
                    }
                }
            }
            machine.set_register(0, calls.command);
            if let Some(Initializer::Lookup {
                execution: LookupExecution::Getter { database, null, .. },
                ..
            }) = calls.input.forms.initializers.get(&function)
            {
                machine.write(*database, 8, FOUND + 0x20000);
                machine.write(*null, 8, MISSING);
            }
            let paths = machine.run_paths_joining(function, &mut |at, machine| {
                calls.stage(at, machine, function, selected)
            });
            for mut path in paths {
                if calls.probe.is_some() && path.end == Ok(Exit::Trapped) {
                    path.machine.label(52, 1);
                }
                let mut stages = stages.clone();
                stages.push(stage_result(&path, stage, stage == Stage::PostValidate));
                let mut stops = stops.clone();
                if let Err(stop) = path.end {
                    stops.push(stop);
                }
                next.push((path.machine, stages, stops));
            }
        }
        if next.len() > 64 {
            for (_, stages, _) in &mut next {
                stages.push(StageResult {
                    stage,
                    returned: None,
                    diagnostic: false,
                    cause: Some("path limit"),
                });
            }
            pending = next;
            break;
        }
        pending = next;
    }
    pending
        .into_iter()
        .map(|(machine, stages, stops)| (machine, chain(stages, stops)))
        .collect()
}

struct Calls<'a> {
    probe: Option<TargetProbe>,
    input: &'a GrammarInput,
    command: u64,
    source: u64,
    reader: CommandReader,
    functions: [Option<u64>; 6],
    values: Vec<ValueForm>,
    reached: BTreeSet<u64>,
}
impl Calls<'_> {
    fn target_type(
        &self,
        callee: Option<u64>,
        machine: &mut Machine<'_>,
        label: u64,
    ) -> Option<std::result::Result<Call, Unresolved>> {
        let probe = self.probe?;
        if callee == Some(self.input.command_bindings.target_scope_type)
            && machine.register(0) == Some(self.command + probe.offset)
        {
            machine.label(label, 1);
            return Some(Ok(Call::Return(Some(probe.bit))));
        }
        None
    }

    fn owner_offset(&self, address: Option<u64>) -> Option<u64> {
        address
            .filter(|address| (self.command..self.command + OBJECT_SPAN).contains(address))
            .map(|address| address - self.command)
    }
    fn value(&mut self, machine: &mut Machine<'_>, value: ValueForm) {
        let index = self
            .values
            .iter()
            .position(|known| known == &value)
            .unwrap_or_else(|| {
                self.values.push(value);
                self.values.len() - 1
            });
        machine.label(ALTERNATIVE, index as u64);
    }
    fn read(
        &mut self,
        target: Option<u64>,
        machine: &mut Machine<'_>,
    ) -> std::result::Result<Call, Unresolved> {
        self.reached.extend(target);
        if let Some(call) = self.target_type(target, machine, 50) {
            return call;
        }
        let binding = &self.input.command_bindings;
        let token = self.source + binding.reader_value_token_offset;
        let receiver = machine.register(0);
        let argument = machine.register(1);
        if target.is_some_and(|at| binding.error_logs.contains(&at)) {
            machine.label(DIAGNOSED, 1);
            return Ok(Call::Return(None));
        }
        if target == Some(self.reader.member)
            && receiver == Some(self.command)
            && argument == Some(self.source)
        {
            machine.label(BLOCK, 1);
            return Ok(Call::Return(None));
        }
        if target == self.functions[2] && argument == Some(token) && receiver == Some(self.command)
        {
            if target.is_some_and(|at| self.input.forms.cut_bodies.contains(&at)) {
                return Err(Unresolved::new("form-cut-body"));
            }
            machine.label(ASSIGN, 1);
            return Ok(Call::Enter);
        }
        if target.is_some_and(|at| self.input.forms.assignments.contains(&at))
            && argument == Some(token)
            && receiver == Some(self.command)
        {
            if target.is_some_and(|at| self.input.forms.cut_bodies.contains(&at)) {
                return Err(Unresolved::new("form-cut-body"));
            }
            return Ok(Call::Enter);
        }
        if target.is_some_and(|at| binding.operator_readers.contains(&at))
            && argument == Some(self.source)
        {
            let destination = receiver.ok_or(Unresolved::new("form-reader-call"))?;
            machine.write_unknown(destination, 4);
            return Ok(Call::Return(None));
        }
        if target.is_some_and(|at| binding.token_copy.contains(&at)) && argument == Some(token) {
            let destination = receiver.ok_or(Unresolved::new("form-reader-call"))?;
            machine.label(TOKEN_COPY, destination);
            if let Some(id) = machine.read(token, 4) {
                machine.write(destination, 4, id);
            }
            return Ok(Call::Return(Some(destination)));
        }
        if target.is_some_and(|at| binding.target_from_token.contains(&at))
            && argument == machine.labelled(TOKEN_COPY)
            && argument.is_some()
        {
            let destination = receiver.ok_or(Unresolved::new("form-reader-call"))?;
            machine.label(TARGET_TEMP, destination);
            return Ok(Call::Return(Some(destination)));
        }
        if target == Some(binding.target_move)
            && argument == machine.labelled(TARGET_TEMP)
            && argument.is_some()
        {
            let destination = self
                .owner_offset(receiver)
                .ok_or(Unresolved::new("form-reader-call"))?;
            self.value(
                machine,
                ValueForm {
                    kind: ReaderKind::Target,
                    destination: Some(destination),
                    reader: None,
                    initialization: None,
                    deferred_null: None,
                },
            );
            machine.write_unknown(self.command + destination, self.input.forms.target_size);
            return Ok(Call::Return(receiver));
        }
        if target == Some(binding.variable_assign) && argument == Some(token) {
            let destination = self
                .owner_offset(receiver)
                .ok_or(Unresolved::new("form-reader-call"))?;
            self.value(
                machine,
                ValueForm {
                    kind: ReaderKind::Unknown,
                    destination: Some(destination),
                    reader: None,
                    initialization: None,
                    deferred_null: None,
                },
            );
            return Ok(Call::Return(None));
        }
        if target.is_some_and(|at| self.input.forms.strings_from_text.contains(&at))
            && argument == Some(TOKEN_TEXT)
        {
            let destination = receiver.ok_or(Unresolved::new("form-reader-call"))?;
            machine.label(STRING_TEMP, destination);
            machine.write_unknown(destination, self.input.forms.string_size);
            return Ok(Call::Return(receiver));
        }
        if target == Some(self.input.forms.dynamic_name)
            && receiver.is_some()
            && receiver == machine.labelled(STRING_TEMP)
        {
            let destination = self
                .owner_offset(argument)
                .ok_or(Unresolved::new("form-reader-call"))?;
            self.value(
                machine,
                ValueForm {
                    kind: ReaderKind::String,
                    destination: Some(destination),
                    reader: None,
                    initialization: None,
                    deferred_null: None,
                },
            );
            machine.write_unknown(self.command + destination, self.input.forms.string_size);
            let target = self
                .owner_offset(machine.register(2))
                .ok_or(Unresolved::new("form-reader-call"))?;
            machine.write_unknown(self.command + target, self.input.forms.target_size);
            return Ok(Call::Return(None));
        }
        if target == Some(self.input.forms.interner) {
            return Ok(Call::Return(None));
        }
        if target.is_some_and(|at| self.input.forms.string_copies.contains(&at))
            && argument == Some(token + self.input.forms.token_text_offset)
            && machine.is_tail_call()
            && let Some(Initializer::Lookup { lookup, .. }) =
                self.functions[3].and_then(|at| self.input.forms.initializers.get(&at))
            && self.owner_offset(receiver) == u64::try_from(lookup.key_offset).ok()
        {
            self.value(
                machine,
                ValueForm {
                    kind: ReaderKind::Reference,
                    destination: self.owner_offset(receiver),
                    reader: None,
                    initialization: Some(lookup.clone()),
                    deferred_null: None,
                },
            );
            machine.write_unknown(receiver.unwrap(), self.input.forms.string_size);
            return Ok(Call::Return(receiver));
        }
        if let Some(name) = target.and_then(|at| self.input.forms.shared.get(&at)) {
            let arguments =
                (0..4)
                    .filter_map(|register| {
                        let value = match machine.register(register) {
                            Some(value) if value == self.source => Value::Reader(0),
                            Some(value)
                                if self
                                    .input
                                    .forms
                                    .reader_value_token_offset
                                    .and_then(|offset| self.source.checked_add(offset))
                                    == Some(value) =>
                            {
                                Value::Reader(
                                    self.input.forms.reader_value_token_offset.unwrap() as i64
                                )
                            }
                            address if self.owner_offset(address).is_some() => {
                                Value::Owner(self.owner_offset(address).unwrap() as i64)
                            }
                            Some(value) => Value::Constant(value as i64),
                            None => return None,
                        };
                        Some((format!("x{register}"), value))
                    })
                    .collect();
            if readers::arguments_join(
                name,
                &arguments,
                true,
                self.input
                    .forms
                    .reader_value_token_offset
                    .map(|offset| offset as i64),
            ) {
                let join = ReaderJoin::Joined {
                    callee: name.clone(),
                    arguments,
                    tail: true,
                };
                let (kind, _) = readers::entry(name);
                if kind == ReaderKind::Reference
                    && !target
                        .is_some_and(|at| self.input.forms.qualified_references.contains_key(&at))
                {
                    return Err(Unresolved::new("form-reference-lookup"));
                }
                if kind == ReaderKind::Block && receiver == Some(self.command) {
                    machine.label(BLOCK, 1);
                    return Ok(Call::Return(None));
                }
                if let Some(destination) =
                    readers::destination(&join).and_then(|offset| u64::try_from(offset).ok())
                {
                    let initialization = self.functions[3]
                        .and_then(|at| self.input.forms.initializers.get(&at))
                        .and_then(|initializer| match initializer {
                            Initializer::Lookup { lookup, .. }
                                if kind == ReaderKind::String
                                    && machine.is_tail_call()
                                    && u64::try_from(lookup.key_offset).ok()
                                        == Some(destination) =>
                            {
                                Some(lookup.clone())
                            }
                            _ => None,
                        });
                    let kind = if initialization.is_some() {
                        ReaderKind::Reference
                    } else {
                        kind
                    };
                    self.value(
                        machine,
                        ValueForm {
                            kind,
                            destination: Some(destination),
                            reader: Some(join),
                            initialization,
                            deferred_null: target.and_then(|at| {
                                self.input.forms.qualified_references.get(&at).copied()
                            }),
                        },
                    );
                    let width = match kind {
                        ReaderKind::String => self.input.forms.string_size,
                        ReaderKind::Boolean => 1,
                        // A reference destination is a pointer; an unlisted scalar keeps the
                        // widest width, which only withholds facts.
                        _ => readers::scalar_width(name).unwrap_or(8),
                    };
                    machine.write_unknown(self.command + destination, width);
                    return Ok(Call::Return(None));
                }
            }
        }
        if target.is_some_and(|at| self.input.forms.harmless.contains(&at)) {
            if self.probe.is_some()
                && (0..8).any(|r| self.owner_offset(machine.register(r)).is_some())
            {
                return Err(Unresolved::new("unclassified target call"));
            }
            forget_if_passed(machine, self.command);
            return Ok(Call::Return(None));
        }
        if (0..8).any(|r| matches!(machine.register(r), Some(value) if value == self.source || value == token || value == token + self.input.forms.token_text_offset || value == TOKEN_TEXT || Some(value) == machine.labelled(STRING_TEMP))) {
            return Err(Unresolved::new("form-reader-call"));
        }
        if self.probe.is_some() {
            return Err(Unresolved::new("unclassified target call"));
        }
        // An unentered helper that can reach the command can log or change acceptance state.
        if (0..8).any(|r| self.owner_offset(machine.register(r)).is_some()) {
            return Err(Unresolved::new("form-command-call"));
        }
        Ok(Call::Return(None))
    }
    fn stage(
        &mut self,
        target: Option<u64>,
        machine: &mut Machine<'_>,
        function: u64,
        selected: u64,
    ) -> std::result::Result<Call, Unresolved> {
        self.reached.extend(target);
        if let Some(call) = self.target_type(target, machine, 51) {
            return call;
        }
        if target.is_some_and(|at| self.input.command_bindings.error_logs.contains(&at)) {
            machine.label(DIAGNOSED, 1);
            return Ok(Call::Return(None));
        }
        if let Some(Initializer::Lookup {
            lookup,
            execution: LookupExecution::Getter { getter, .. },
        }) = self.input.forms.initializers.get(&function)
            && target == Some(*getter)
            && machine.register(0) == Some(FOUND + 0x20000)
            && machine.register(1) == Some(self.command.wrapping_add_signed(lookup.key_offset))
        {
            return Ok(Call::Return(Some(selected)));
        }

        if (0..8).any(|r| self.owner_offset(machine.register(r)).is_some()) {
            return Err(Unresolved::new("form-stage-call"));
        }
        if self.probe.is_some() {
            return Err(Unresolved::new("unclassified target call"));
        }
        Ok(Call::Return(None))
    }
}

#[cfg(test)]
mod tests;

fn target_touches(machine: &Machine<'_>) -> [bool; 2] {
    [
        machine.labelled(50).is_some(),
        machine.labelled(51).is_some(),
    ]
}

#[derive(Clone, Copy)]
struct TargetProbe {
    offset: u64,
    bit: u64,
    member: Option<super::targets::Member>,
}

/// Reuse the full read/initialization/validation chain for each concrete target type.
pub(super) fn target_checks(
    input: &GrammarInput,
    reader: CommandReader,
    state: &BTreeMap<u64, u8>,
    offset: u64,
    member: Option<super::targets::Member>,
) -> [super::targets::Check; 2] {
    use super::targets::Check;
    let at = |slot| {
        input
            .declarations
            .pointers
            .get(&(reader.vtable + slot))
            .copied()
    };
    let functions = [
        Some(reader.read),
        Some(member.map_or(reader.member, |member| member.entry)),
        at(input.command_bindings.assign_slot),
        at(input.declarations.parser_slots.initializer),
        at(input.command_bindings.validation_slot),
        at(input.forms.role_slot),
    ];
    let Ok(code) = code(input, &functions) else {
        return [
            Check::Unresolved("target chain code"),
            Check::Unresolved("target chain code"),
        ];
    };
    let count = input
        .declarations
        .scope_names
        .as_ref()
        .map_or(64, Vec::len)
        .min(64);
    let probe = |bit| {
        run(
            input,
            &code,
            reader,
            state,
            functions,
            None,
            false,
            Some(TargetProbe {
                offset,
                bit: 1u64 << bit,
                member,
            }),
        )
    };
    let first = probe(0);
    // With no scope-type call on any path, changing its return cannot change this run.
    let checks_type = first
        .paths
        .iter()
        .any(|path| path.target_touches.iter().any(|touch| *touch));
    let mut runs = vec![first];
    if checks_type {
        runs.extend((1..count).map(probe));
    }
    std::array::from_fn(|stage| {
        if !runs.iter().any(|run| {
            run.paths
                .iter()
                .any(|path| !path.trapped && path.target_touches[stage])
        }) {
            if runs.iter().any(|run| {
                !run.stops.is_empty()
                    || run
                        .paths
                        .iter()
                        .any(|path| !path.trapped && path.target_load)
            }) {
                return Check::Unresolved("target chain unfinished");
            }
            return Check::Absent;
        }
        let mut mask = 0;
        for (bit, run) in runs.iter().enumerate() {
            if receiver_dependent(&run.paths, |path| {
                (!path.trapped).then_some((path.target_touches, path.chain.class))
            }) {
                return Check::Unresolved("receiver-state");
            }
            if !run.stops.is_empty() || run.paths.is_empty() || run.paths.len() > 64 {
                return Check::Unresolved("target chain unfinished");
            }
            let classes: Vec<_> = run
                .paths
                .iter()
                .filter(|path| !path.trapped)
                .map(|path| {
                    if path.target_load {
                        return PathClass::Unresolved;
                    }
                    if stage == 0 {
                        chain(
                            path.chain
                                .stages
                                .iter()
                                .filter(|s| matches!(s.stage, Stage::Read | Stage::Assign))
                                .cloned()
                                .collect(),
                            vec![],
                        )
                        .class
                    } else {
                        path.chain.class
                    }
                })
                .collect();
            if classes.is_empty() {
                return Check::Unresolved("no returning target path");
            }
            if classes.iter().all(|class| *class == PathClass::Accepting) {
                mask |= 1 << bit;
            } else if !classes.iter().all(|class| *class == PathClass::Rejecting) {
                let false_without_log = run.paths.iter().any(|path| {
                    path.chain
                        .stages
                        .iter()
                        .any(|stage| stage.cause == Some("false without diagnostic"))
                });
                return Check::Unresolved(if false_without_log {
                    "false without diagnostic"
                } else {
                    "target chain unresolved or mixed"
                });
            }
        }
        Check::Established(mask)
    })
}
