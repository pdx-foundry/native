//! Bound factory evaluation. A reader is joined only when every returned receiver agrees.
//!
//! A create method may tail-call an out-of-line factory that allocates and returns the command.
//! The walk runs that callee's code in place of the tail call, since the callee's return is the
//! create method's return.
use super::{DeclarationInput, Function, decode, number};
use crate::engine::analysis::{
    evaluate::{Call, Code, Exit, Machine},
    stop::Unresolved,
};
use std::collections::{BTreeMap, BTreeSet};

const ALLOCATION: u64 = 0x10000;

pub(crate) fn factory_vtable(input: &DeclarationInput, factory: u64) -> Result<u64, Unresolved> {
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
    let code = Code::from_rows(rows);
    let machine = Machine::new(&code, input.pointer_data());
    let paths = machine.run_paths(entry, &mut |target, machine| {
        if target.is_some_and(|target| input.operator_new.contains(&target)) {
            let size = machine.known_register(0, "allocation-size")?;
            if size == 0 || size > ALLOCATION {
                return Err(Unresolved::new("allocation-bound"));
            }
            let object = machine.reserve(size);
            if machine.labels().is_empty() {
                machine.label(object, size);
            }
            return Ok(Call::Return(Some(object)));
        }
        if let Some(vtables) = target.and_then(|target| input.constructors.get(&target)) {
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
                let (_, end) =
                    allocation.ok_or(Unresolved::new("constructor-outside-allocation"))?;
                crate::engine::analysis::receivers::install_vtables(
                    machine, receiver, end, vtables,
                )
                .ok_or(Unresolved::new("constructor-vtable-bound"))?;
                return Ok(Call::Return(None));
            }
        }
        if target.is_some_and(|target| wrappers.contains(&target)) {
            return Ok(Call::Enter);
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
    for path in paths {
        match path.end? {
            Exit::Returned => {}
            _ => return Err(Unresolved::new("factory-terminal")),
        }
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
    }
    if vtables.len() != 1 {
        return Err(Unresolved::new("ambiguous-command-vtable"));
    }
    Ok(*vtables.first().unwrap())
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
