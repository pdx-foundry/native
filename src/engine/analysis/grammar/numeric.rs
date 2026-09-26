//! Join integer-key decoding to a constructed child's virtual reader.
use super::GrammarInput;
use crate::engine::analysis::{
    declarations::{self, CommandReader},
    decode::decode_arm64,
    evaluate::{Call, Code, Exit, Machine},
    fields::TokenPath,
    stop::Unresolved,
};

const SPAN: u64 = 0x10000;
const DECODED: u64 = 0;
const VTABLE: u64 = 1;

/// A numeric path must decode the original reader's token, then read an allocated receiver.
/// Other paths are not numeric candidates. Every alternative of a candidate must agree.
pub(super) fn reader(
    input: &GrammarInput,
    entry: u64,
    path: &TokenPath,
) -> Result<Option<CommandReader>, Unresolved> {
    if path.domain[0] != path.domain[1] {
        return Ok(None);
    }
    let body = input
        .declarations
        .functions
        .get(&entry)
        .ok_or(Unresolved::new("numeric-member-body"))?;
    let rows =
        decode_arm64(&body.code, entry).map_err(|_| Unresolved::new("numeric-member-code"))?;
    let candidate = rows.iter().any(|row| {
        row.address == path.terminal
            && row.operation == "bl"
            && declarations::number(&row.operands) == Some(input.numeric_decoder)
    });
    if !candidate {
        return Ok(None);
    }
    let code = Code::from_rows(rows);
    let mut machine = Machine::new(&code, &input.declarations.composition.data);
    for (&slot, &pointer) in &input.declarations.pointers {
        machine.write(slot, 8, pointer);
    }
    let owner = machine.reserve(SPAN);
    let reader = machine.reserve(SPAN);
    let stack = machine.stack_pointer();
    machine.set_register(0, owner);
    machine.set_register(1, reader);
    machine.set_register(2, path.domain[0] as u64);
    let paths = machine.run_paths(entry, &mut |target, machine| {
        let target = target.ok_or(Unresolved::new("numeric-indirect-call"))?;
        if target == input.numeric_decoder {
            let destination = machine.known_register(1, "numeric-output")?;
            if machine.register(0) != Some(reader + input.reader_token_offset)
                || !(stack - SPAN..stack).contains(&destination)
                || destination + 4 > stack
                || machine.labelled(DECODED).is_some()
            {
                return Err(Unresolved::new("numeric-key-routing"));
            }
            machine.forget(destination, 4);
            machine.label(DECODED, 1);
            return Ok(Call::Return(None));
        }
        if input.declarations.operator_new.contains(&target) {
            let size = machine.known_register(0, "numeric-allocation-size")?;
            if size == 0 || size > SPAN {
                return Err(Unresolved::new("numeric-allocation-bound"));
            }
            let object = machine.reserve(size);
            machine.label(object, size);
            return Ok(Call::Return(Some(object)));
        }
        if let Some(vtables) = input.declarations.constructors.get(&target) {
            let receiver = machine.known_register(0, "numeric-constructor-receiver")?;
            let end = machine
                .labels()
                .iter()
                .find_map(|(&at, &size)| {
                    (at > VTABLE && (at..at + size).contains(&receiver)).then_some(at + size)
                })
                .ok_or(Unresolved::new("numeric-constructor-allocation"))?;
            machine.forget(receiver, end - receiver);
            for (&offset, &point) in vtables {
                let at = receiver
                    .checked_add(offset)
                    .filter(|at| *at <= end.saturating_sub(8))
                    .ok_or(Unresolved::new("numeric-constructor-bound"))?;
                machine.write(at, 8, point);
            }
            return Ok(Call::Return(None));
        }
        let object = machine.known_register(0, "numeric-child-receiver")?;
        let vtable = machine
            .read(object, 8)
            .ok_or(Unresolved::new("numeric-child-vtable"))?;
        let child = declarations::reader_at_vtable(&input.declarations, vtable)?;
        if target != child.read
            || machine.register(1) != Some(reader)
            || machine.labelled(object).is_none()
            || machine.labelled(DECODED) != Some(1)
        {
            return Err(Unresolved::new("numeric-child-routing"));
        }
        machine.label(VTABLE, vtable);
        Ok(Call::Stop)
    });
    let mut result = None;
    for path in paths {
        let Exit::Stopped(target) = path.end? else {
            return Err(Unresolved::new("numeric-child-terminal"));
        };
        let vtable = path
            .machine
            .labelled(VTABLE)
            .ok_or(Unresolved::new("numeric-child-vtable"))?;
        let child = declarations::reader_at_vtable(&input.declarations, vtable)?;
        if target != child.read || result.is_some_and(|known| known != child) {
            return Err(Unresolved::new("ambiguous-numeric-child"));
        }
        result = Some(child);
    }
    result
        .map(Some)
        .ok_or(Unresolved::new("numeric-child-paths"))
}
