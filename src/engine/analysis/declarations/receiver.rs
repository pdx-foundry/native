//! Bound factory evaluation. A reader is joined only when every returned receiver agrees.
//!
//! A create method may tail-call an out-of-line factory that allocates and returns the command.
//! The walk runs that callee's code in place of the tail call, since the callee's return is the
//! create method's return.
use super::{DeclarationInput, Function, decode, number};
use crate::engine::analysis::{
    evaluate::{Call, Code, Exit, Machine, ReturnTaint},
    receivers::{Constructors, accept_entered_path, install_vtables, join_byte_traces},
    stop::{CauseKind, Trace, Unresolved},
};
use std::collections::{BTreeMap, BTreeSet};

const ALLOCATION: u64 = 0x10000;

pub(crate) fn factory_vtable(input: &DeclarationInput, factory: u64) -> Result<u64, Unresolved> {
    factory_state(input, factory).map(|state| state.vtable)
}

/// The primary vtable and the allocation bytes agreed by every factory return.
pub(crate) struct FactoryState {
    pub vtable: u64,
    pub bytes: BTreeMap<u64, u8>,
    /// Why each other allocation byte is unknown, by offset, while tracing causes.
    pub traces: BTreeMap<u64, Trace>,
}

pub(crate) fn factory_state(
    input: &DeclarationInput,
    factory: u64,
) -> Result<FactoryState, Unresolved> {
    let baseline = evaluate_factory(input, factory, false);
    let entered = evaluate_factory(input, factory, true);
    match (baseline, entered) {
        (Ok(baseline), Ok(mut entered))
            if baseline.vtable == entered.vtable
                && baseline.bytes.iter().all(|(offset, byte)| {
                    entered.bytes.get(offset).is_none_or(|value| value == byte)
                }) =>
        {
            entered.bytes.extend(baseline.bytes);
            entered
                .traces
                .retain(|offset, _| !entered.bytes.contains_key(offset));
            Ok(entered)
        }
        (Ok(baseline), _) => Ok(baseline),
        (Err(baseline), Err(_)) => Err(baseline),
        (Err(_), entered) => entered,
    }
}

// The summary-only evaluation is independent of entered bodies, which add facts only above it.
fn evaluate_factory(
    input: &DeclarationInput,
    factory: u64,
    enter_constructors: bool,
) -> Result<FactoryState, Unresolved> {
    let entry = *input
        .pointers
        .get(&(factory + input.slots.create))
        .ok_or(Unresolved::new("factory-create"))?;
    let body = input
        .functions
        .get(&entry)
        .ok_or(Unresolved::new("create-body"))?;
    let mut rows = decode(body).map_err(|_| Unresolved::new("factory-code"))?;
    for callee in tail_callees(&input.functions, body) {
        rows.extend(decode(callee).map_err(|_| Unresolved::new("factory-code"))?);
    }
    let wrappers: BTreeSet<_> = rows
        .iter()
        .filter(|row| row.operation == "bl")
        .filter_map(|row| number(&row.operands))
        .filter_map(|address| input.functions.get(&address))
        .filter(|function| {
            register_move_tail_target(function)
                .is_some_and(|target| input.constructors.contains_key(&target))
        })
        .map(|function| function.address)
        .collect();
    for address in &wrappers {
        rows.extend(
            decode(&input.functions[address]).map_err(|_| Unresolved::new("factory-code"))?,
        );
    }
    let mut constructor_targets: Vec<_> = rows
        .iter()
        .filter(|row| matches!(row.operation.as_str(), "bl" | "b"))
        .filter_map(|row| number(&row.operands))
        .filter(|target| input.constructors.contains_key(target))
        .collect();
    let mut decoded = BTreeSet::new();
    if !enter_constructors {
        constructor_targets.clear();
    }
    while let Some(target) = constructor_targets.pop() {
        if !decoded.insert(target) {
            continue;
        }
        let Some(body) = input.functions.get(&target) else {
            continue;
        };
        let Ok(body_rows) = decode(body) else {
            continue;
        };
        constructor_targets.extend(
            body_rows
                .iter()
                .filter(|row| matches!(row.operation.as_str(), "bl" | "b"))
                .filter_map(|row| number(&row.operands))
                .filter(|target| input.constructors.contains_key(target)),
        );
        rows.extend(body_rows);
    }
    let code = Code::from_rows(rows);
    let image = input.constructor_image();
    let machine = if enter_constructors {
        let mut machine = image.entered(&code);
        machine.intercept_tail_calls(input.constructors.keys().copied().collect());
        machine
    } else {
        Machine::new(&code, input.pointer_data())
    };
    let paths = machine.run_paths(entry, &mut |target, machine| {
        if target.is_some_and(|target| input.operator_new.contains(&target)) {
            let size = machine.known_register(0, "allocation-size")?;
            if size == 0 || size > ALLOCATION {
                return Err(Unresolved::new("allocation-bound"));
            }
            if enter_constructors {
                // The allocator runs code that may reach memory, including an escaped owner.
                machine.opaque_call_effects();
            }
            let object = machine.reserve(size);
            let owner = machine.labels().is_empty();
            if owner {
                machine.label(object, size);
            }
            if owner && enter_constructors {
                return Ok(machine.return_allocated_owner(object, object + size));
            }
            // The result is not a tracked owner, but the allocator may leave one in any register
            // that it does not preserve.
            let taint = ReturnTaint {
                returned: false,
                clobbered: true,
            };
            return Ok(machine.return_with_taint(Some(object), taint));
        }
        if let Some((target, vtables)) =
            target.and_then(|target| Some((target, input.constructors.get(&target)?)))
        {
            let receiver = machine.register(0);
            let allocation = receiver.and_then(|receiver| {
                machine.labels().iter().find_map(|(&at, &size)| {
                    (at..at + size)
                        .contains(&receiver)
                        .then_some((at, at + size))
                })
            });
            let member = allocation
                .zip(receiver)
                .is_some_and(|((start, _), receiver)| receiver > start);
            if !vtables.is_empty() || member {
                let receiver = machine.known_register(0, "constructor-receiver")?;
                let (owner, end) =
                    allocation.ok_or(Unresolved::new("constructor-outside-allocation"))?;
                let vtable_bound = || Unresolved::new("constructor-vtable-bound");
                if !enter_constructors {
                    install_vtables(machine, receiver, end, vtables, CauseKind::Invalidated)
                        .ok_or_else(vtable_bound)?;
                    return Ok(Call::Return(None));
                }
                let constructors = Constructors {
                    code: &code,
                    image,
                    summaries: &input.constructors,
                    owner,
                    end,
                };
                let body_available = input.functions.contains_key(&target);
                return constructors
                    .call(machine, target, receiver, body_available)
                    .ok_or_else(vtable_bound);
            }
        }
        if target.is_some_and(|target| wrappers.contains(&target)) {
            return Ok(Call::Enter);
        }
        if enter_constructors {
            return Ok(machine.opaque_call());
        }
        let objects: Vec<_> = machine
            .labels()
            .iter()
            .map(|(&at, &size)| (at, size))
            .collect();
        for (at, size) in objects {
            machine.forget(at, size);
        }
        Ok(Call::Return(None))
    });
    let mut vtables = BTreeSet::new();
    let mut agreed: Option<(BTreeMap<u64, u8>, BTreeMap<u64, Trace>)> = None;
    for path in paths {
        match path.end? {
            Exit::Returned => {}
            _ => return Err(Unresolved::new("factory-terminal")),
        }
        accept_entered_path(&path.machine)?;
        let object = path.machine.register(0).ok_or_else(|| {
            Unresolved::new("factory-return").traced(path.machine.register_trace(0))
        })?;
        if path.machine.labelled(object).is_none() {
            return Err(Unresolved::new("factory-receiver"));
        }
        let vtable = path.machine.read(object, 8).ok_or_else(|| {
            Unresolved::new("command-vtable").traced(path.machine.memory_trace(object, 8))
        })?;
        vtables.insert(vtable);
        let size = path.machine.labelled(object).unwrap();
        let returned = path.machine.known_bytes(object, size);
        let traces = path.machine.unknown_byte_traces(object, size);
        agreed = Some(match agreed {
            Some((mut bytes, earlier)) => {
                let traces = join_byte_traces(
                    (&bytes, &earlier),
                    (&returned, &traces),
                    path.machine.join_cause(),
                );
                bytes.retain(|offset, byte| returned.get(offset) == Some(byte));
                (bytes, traces)
            }
            None => (returned, traces),
        });
    }
    if vtables.len() != 1 {
        return Err(Unresolved::new("ambiguous-command-vtable"));
    }
    let (bytes, traces) = agreed.unwrap_or_default();
    Ok(FactoryState {
        vtable: *vtables.first().unwrap(),
        bytes,
        traces,
    })
}

/// The known functions that `body` tail-calls: an unconditional branch to a function start
/// outside `body`. A body that cannot be decoded tail-calls none.
pub(crate) fn tail_callees<'a>(
    functions: &'a BTreeMap<u64, Function>,
    body: &Function,
) -> Vec<&'a Function> {
    let end = body.address + body.code.len() as u64;
    let rows = decode(body).unwrap_or_default();
    let targets: BTreeSet<u64> = rows
        .iter()
        .filter(|row| row.operation == "b")
        .filter_map(|row| number(&row.operands))
        .filter(|target| !(body.address..end).contains(target))
        .collect();

    targets
        .iter()
        .filter_map(|target| functions.get(target))
        .collect()
}

/// A whole function consisting only of general-register moves and one direct tail call.
/// The caller must separately establish that the tail target is a constructor.
pub(crate) fn register_move_tail_target(body: &Function) -> Option<u64> {
    let rows = decode(body).ok()?;
    let (tail, moves) = rows.split_last()?;
    if tail.operation != "b" || moves.is_empty() {
        return None;
    }
    for row in moves {
        let (destination, source) = row.operands.split_once(',')?;
        if row.operation != "mov" || !general_register(destination) || !general_register(source) {
            return None;
        }
    }
    let target = number(&tail.operands)?;
    (!(body.address..body.address + body.code.len() as u64).contains(&target)).then_some(target)
}

fn general_register(operand: &str) -> bool {
    let operand = operand.trim();
    let Some(number) = operand
        .strip_prefix('x')
        .or_else(|| operand.strip_prefix('w'))
    else {
        return false;
    };
    number.parse::<u8>().is_ok_and(|index| index <= 30)
}
