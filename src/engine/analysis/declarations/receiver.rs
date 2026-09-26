//! Bound factory evaluation. A reader is joined only when every returned receiver agrees.
use super::{DeclarationInput, decode};
use crate::engine::analysis::{
    evaluate::{Call, Code, Exit, Machine},
    stop::Unresolved,
};
use std::collections::BTreeSet;

const ALLOCATION: u64 = 0x10000;

pub(super) fn factory_vtable(input: &DeclarationInput, factory: u64) -> Result<u64, Unresolved> {
    let entry = *input
        .pointers
        .get(&(factory + input.slots.create))
        .ok_or(Unresolved::new("factory-create"))?;
    let body = input
        .functions
        .get(&entry)
        .ok_or(Unresolved::new("create-body"))?;
    let rows = decode(body).map_err(|_| Unresolved::new("factory-code"))?;
    let code = Code::from_rows(rows);
    let mut machine = Machine::new(&code, &input.composition.data);
    for (&slot, &value) in &input.pointers {
        machine.write(slot, 8, value);
    }
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
            let receiver = machine.known_register(0, "constructor-receiver")?;
            let end = machine
                .labels()
                .iter()
                .find_map(|(&at, &size)| (at..at + size).contains(&receiver).then_some(at + size))
                .ok_or(Unresolved::new("constructor-outside-allocation"))?;
            crate::engine::analysis::receivers::install_vtables(machine, receiver, end, vtables)
                .ok_or(Unresolved::new("constructor-vtable-bound"))?;
            return Ok(Call::Return(None));
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
        let object = path
            .machine
            .register(0)
            .ok_or(Unresolved::new("factory-return"))?;
        if path.machine.labelled(object).is_none() {
            return Err(Unresolved::new("factory-receiver"));
        }
        let vtable = path
            .machine
            .read(object, 8)
            .ok_or(Unresolved::new("command-vtable"))?;
        vtables.insert(vtable);
    }
    if vtables.len() != 1 {
        return Err(Unresolved::new("ambiguous-command-vtable"));
    }
    Ok(*vtables.first().unwrap())
}
