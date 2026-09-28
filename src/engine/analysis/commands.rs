//! Shared stand-in objects and call handling for command analysis.
use super::declarations::number;
use super::decode::Instruction;
use super::evaluate::Machine;

/// Bytes tracked for a stand-in command object.
pub(crate) const OBJECT_SPAN: u64 = 0x1000;

/// Whether `rows` call or tail-call one of `targets`.
pub(crate) fn calls_any(rows: &[Instruction], targets: &[u64]) -> bool {
    rows.iter()
        .filter(|row| matches!(row.operation.as_str(), "bl" | "b"))
        .filter_map(|row| number(&row.operands))
        .any(|target| targets.contains(&target))
}

/// A command object whose vtable is `vtable` and whose other fields are unknown.
pub(crate) fn stand_in_command(machine: &mut Machine<'_>, vtable: u64) -> u64 {
    let command = machine.reserve(OBJECT_SPAN);
    machine.write(command, 8, vtable);

    command
}

/// An unknown call that receives a pointer into the command object may write any of it.
pub(crate) fn forget_if_passed(machine: &mut Machine<'_>, command: u64) {
    let passed = (0..8).any(|register| {
        machine
            .register(register)
            .is_some_and(|value| (command..command + OBJECT_SPAN).contains(&value))
    });
    if passed {
        machine.forget(command, OBJECT_SPAN);
    }
}
