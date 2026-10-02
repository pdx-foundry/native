//! Bounded C library effects and executable bodies used by constructor walks.
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::engine::analysis::evaluate::{Call, Machine, ReturnTaint};

/// Bound imports have their C ABI semantics. Engine helpers are executed, never summarized by name.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct ConstructorCalls {
    /// Entry addresses of the imported C `strlen`.
    pub lengths: BTreeSet<u64>,
    /// Entry addresses of the imported C `memcpy`, which requires disjoint spans.
    pub copies: BTreeSet<u64>,
    /// Entry addresses of the imported C `memmove`, which permits overlapping spans.
    pub moves: BTreeSet<u64>,
    /// Accessor entry addresses and executable bodies to enter when the receiver is known.
    pub helpers: BTreeMap<u64, Vec<u8>>,
}

/// Bound work on one call, not an assumed engine capacity.
const BYTE_LIMIT: u64 = 4096;

impl ConstructorCalls {
    /// Handle a bound call, possibly changing memory and return derivation; `None` is unhandled.
    /// Helper bodies must already be decoded into the machine's code before returning `Enter`.
    pub(super) fn call(&self, target: Option<u64>, machine: &mut Machine<'_>) -> Option<Call> {
        let target = target?;
        if self.helpers.contains_key(&target) {
            return Some(if machine.register(0).is_some() {
                Call::Enter
            } else {
                machine.opaque_call()
            });
        }
        if self.lengths.contains(&target) {
            let length = machine
                .register(0)
                .and_then(|source| text_length(machine, source));
            // strlen writes no memory, even when its result cannot be established.
            let taint = machine.given_call_taint();
            return Some(machine.return_with_taint(
                length,
                ReturnTaint {
                    returned: taint.returned && length.is_none(),
                    ..taint
                },
            ));
        }
        let moves = self.moves.contains(&target);
        if !moves && !self.copies.contains(&target) {
            return None;
        }
        let copied = (|| {
            let destination = machine.register(0)?;
            let source = machine.register(1)?;
            let length = machine.register(2).filter(|length| *length <= BYTE_LIMIT)?;
            let destination_end = destination.checked_add(length)?;
            let source_end = source.checked_add(length)?;
            if !moves && length != 0 && destination < source_end && source < destination_end {
                return None;
            }
            let taint = machine.given_call_taint();
            let returned = taint.returned && machine.owner_derived(0);
            machine.copy_bytes(destination, source, length);
            Some(machine.return_with_taint(Some(destination), ReturnTaint { returned, ..taint }))
        })();
        Some(copied.unwrap_or_else(|| machine.opaque_call()))
    }
}

fn text_length(machine: &Machine<'_>, source: u64) -> Option<u64> {
    for offset in 0..BYTE_LIMIT {
        if machine.read(source.checked_add(offset)?, 1)? == 0 {
            return Some(offset);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::{
        assembler::arm64,
        evaluate::{Code, Exit, ReadOnlyData},
    };

    const COPY: u64 = 0x8000;
    const MOVE: u64 = 0x8100;
    const LENGTH: u64 = 0x8200;
    const OWNER: u64 = 0x10000;
    const SOURCE: u64 = 0x20000;

    fn calls() -> ConstructorCalls {
        ConstructorCalls {
            lengths: [LENGTH].into(),
            copies: [COPY].into(),
            moves: [MOVE].into(),
            ..Default::default()
        }
    }

    #[test]
    fn bounded_copy_changes_only_its_span_even_after_escape() {
        let code = Code::default();
        let data = ReadOnlyData::new(vec![(SOURCE, b"abc\0".to_vec())]);
        let mut machine = Machine::new(&code, &data);
        machine.track_owner(OWNER, OWNER + 32);
        machine.write(OWNER, 8, 42);
        machine.write(OWNER + 8, 8, u64::MAX);
        machine.set_register(0, OWNER + 8);
        machine.set_register(1, SOURCE);
        machine.set_register(2, 3);
        assert!(matches!(
            calls().call(Some(COPY), &mut machine),
            Some(Call::Return(Some(_)))
        ));
        assert_eq!(machine.read(OWNER, 8), Some(42));
        assert_eq!(machine.read(OWNER + 8, 4), Some(0xff636261));
    }

    #[test]
    fn missing_copy_arguments_and_overflow_invalidate_the_owner_from_the_destination() {
        for (destination, source, length, kept) in [
            (None, Some(SOURCE), Some(3), false),
            (Some(OWNER + 8), None, Some(3), true),
            (Some(OWNER + 8), Some(SOURCE), None, true),
            (Some(OWNER + 8), Some(SOURCE), Some(BYTE_LIMIT + 1), true),
            (Some(u64::MAX), Some(SOURCE), Some(3), true),
            (Some(OWNER + 8), Some(u64::MAX), Some(3), true),
        ] {
            let code = Code::default();
            let data = ReadOnlyData::default();
            let mut machine = Machine::new(&code, &data);
            machine.track_owner(OWNER, OWNER + 32);
            machine.write(OWNER, 8, 42);
            machine.write(OWNER + 8, 8, 99);
            for (index, value) in [destination, source, length].into_iter().enumerate() {
                if let Some(value) = value {
                    machine.set_register(index, value);
                }
            }
            calls().call(Some(COPY), &mut machine).unwrap();
            assert_eq!(machine.read(OWNER, 8), kept.then_some(42));
            let given = destination.is_none_or(|at| (OWNER..OWNER + 32).contains(&at));
            assert_eq!(machine.read(OWNER + 8, 8), (!given).then_some(99));
        }
    }

    #[test]
    fn moves_snapshot_overlaps_but_memcpy_does_not_prove_them() {
        for target in [COPY, MOVE] {
            let code = Code::default();
            let data = ReadOnlyData::default();
            let mut machine = Machine::new(&code, &data);
            machine.track_owner(OWNER, OWNER + 32);
            machine.write(OWNER, 4, 0x04030201);
            machine.set_register(0, OWNER + 1);
            machine.set_register(1, OWNER);
            machine.set_register(2, 3);
            calls().call(Some(target), &mut machine).unwrap();
            assert_eq!(
                machine.read(OWNER, 4),
                (target == MOVE).then_some(0x03020101)
            );
        }
    }

    #[test]
    fn unknown_source_bytes_forget_only_the_destination_and_carry_their_derivation() {
        let code = Code::default();
        let data = ReadOnlyData::default();
        let mut machine = Machine::new(&code, &data);
        machine.track_owner(OWNER, OWNER + 32);
        machine.write(OWNER, 8, 42);
        machine.write(OWNER + 8, 8, 99);
        machine.set_register(0, OWNER + 8);
        machine.set_register(1, SOURCE);
        machine.set_register(2, 4);
        calls().call(Some(COPY), &mut machine).unwrap();
        assert_eq!(machine.read(OWNER, 8), Some(42));
        assert_eq!(machine.read(OWNER + 8, 4), None);
        let derived = machine.owner_derived_memory();
        assert!((OWNER + 8..OWNER + 12).all(|at| !derived.contains(&at)));
    }

    #[test]
    fn copied_pointer_bytes_keep_owner_derivation() {
        let code = Code::default();
        let data = ReadOnlyData::default();
        let mut machine = Machine::new(&code, &data);
        machine.track_owner(OWNER, OWNER + 32);
        machine.write(OWNER, 8, OWNER + 24);
        machine.install_owner_derived_memory(&(OWNER..OWNER + 8).collect());
        machine.set_register(0, OWNER + 8);
        machine.set_register(1, OWNER);
        machine.set_register(2, 8);
        calls().call(Some(COPY), &mut machine).unwrap();
        assert_eq!(machine.read(OWNER + 8, 8), Some(OWNER + 24));
        let derived = machine.owner_derived_memory();
        assert!((OWNER + 8..OWNER + 16).all(|at| derived.contains(&at)));
    }

    #[test]
    fn strlen_needs_every_byte_through_the_terminator_but_writes_nothing() {
        for text in [b"abc\0".to_vec(), b"abc".to_vec(), vec![]] {
            let code = Code::default();
            let data = ReadOnlyData::new(vec![(SOURCE, text.clone())]);
            let mut machine = Machine::new(&code, &data);
            machine.track_owner(OWNER, OWNER + 32);
            machine.write(OWNER, 8, 42);
            machine.set_register(0, SOURCE);
            assert_eq!(
                calls().call(Some(LENGTH), &mut machine),
                Some(Call::Return((text.len() == 4).then_some(3)))
            );
            assert_eq!(machine.read(OWNER, 8), Some(42));
        }
    }

    /// Run `body` at 0x1000 with the owner in x19, an unrelated buffer in x20 and `SOURCE` in
    /// x21, and every unbound call opaque. Returns the owner's word at +8, which is 7 before.
    fn owner_word_after_calls(body: &[u8]) -> Option<u64> {
        let code = Code::decode(&[(0x1000, body)]).unwrap();
        let data = ReadOnlyData::new(vec![(SOURCE, b"abc".to_vec())]);
        let mut machine = Machine::new(&code, &data);
        machine.track_private_owner(OWNER, OWNER + 32);
        machine.write(OWNER + 8, 4, 7);
        machine.set_register(19, OWNER);
        machine.derive_from_owner(19);
        machine.set_register(20, 0x30000);
        machine.set_register(21, SOURCE);
        let paths = machine.run_paths(0x1000, &mut |target, machine| {
            Ok(calls()
                .call(target, machine)
                .unwrap_or_else(|| machine.opaque_call()))
        });
        assert_eq!(paths.len(), 1);
        assert!(matches!(paths[0].end, Ok(Exit::Returned)));

        paths[0].machine.read(OWNER + 8, 4)
    }

    #[test]
    fn modeled_calls_return_owner_addresses_only_when_given_one() {
        let unrelated_length = arm64!(at 0x1000;
            mov x0, x21; // text without a known terminator, outside the owner
            bl extern LENGTH as usize;
            str wzr, [x0];
            str wzr, [x1];
            ret
        );
        let disjoint_copy = arm64!(at 0x1000;
            mov x0, x20;
            mov x1, x21;
            mov x2, #2;
            bl extern COPY as usize;
            str wzr, [x1];
            ret
        );
        for body in [unrelated_length, disjoint_copy] {
            assert_eq!(owner_word_after_calls(&body), Some(7));
        }

        let owner_length = arm64!(at 0x1000;
            add x0, x19, #16; // unknown text in the owner
            bl extern LENGTH as usize;
            str wzr, [x1];
            ret
        );
        let owner_copy = arm64!(at 0x1000;
            add x0, x19, #16;
            mov x1, x21;
            mov x2, #2;
            bl extern COPY as usize;
            str wzr, [x1];
            ret
        );
        for body in [owner_length, owner_copy] {
            assert_eq!(owner_word_after_calls(&body), None);
        }
    }

    #[test]
    fn copies_replace_destination_loss_history_with_the_source_snapshot() {
        use crate::engine::analysis::evaluate::trace_causes;

        for destination in [OWNER + 1, OWNER + 8] {
            let body = arm64!(at 0x1000;
                bl extern 0x9000;
                bl extern 0x9100;
                bl extern MOVE as usize;
                ret
            );
            let code = Code::decode(&[(0x1000, &body)]).unwrap();
            let data = ReadOnlyData::default();
            let paths = trace_causes(|| {
                let mut machine = Machine::new(&code, &data);
                machine.track_owner(OWNER, OWNER + 32);
                machine.write(OWNER, 8, 7);
                machine.write(OWNER + 8, 8, 9);
                machine.run_paths(0x1000, &mut |target, machine| {
                    match target {
                        Some(0x9000) => machine.forget(OWNER, 1),
                        Some(0x9100) => machine.forget(destination, 2),
                        _ => {
                            machine.set_register(0, destination);
                            machine.set_register(1, OWNER);
                            machine.set_register(2, 2);
                            return Ok(calls().call(target, machine).unwrap());
                        }
                    }
                    Ok(Call::Return(None))
                })
            });
            assert_eq!(paths.len(), 1);
            let machine = &paths[0].machine;
            assert!(matches!(paths[0].end, Ok(Exit::Returned)));
            let losses = |address| {
                machine.memory_trace(address, 1).map(|trace| {
                    trace
                        .causes()
                        .map(|cause| cause.instruction)
                        .collect::<Vec<_>>()
                })
            };
            assert_eq!(losses(destination), Some(vec![0x1000]));
            if destination == OWNER + 1 {
                assert_eq!(losses(destination + 1), Some(vec![0x1004]));
            } else {
                assert_eq!(machine.read(destination + 1, 1), Some(0));
                assert_eq!(losses(destination + 1), None);
            }
        }
    }

    #[test]
    fn capacity_branch_and_terminator_are_executed_not_assumed() {
        let body = arm64!(at 0x1000;
            stp x19, x20, [sp, #-16]!;
            mov x19, x0;
            mov x20, x2;
            cmp x2, #4;
            b.hs >unbounded;
            bl extern COPY as usize;
            strb wzr, [x19, x20];
            b >done;
            unbounded:;
            bl extern 0x9000;
            done:;
            ldp x19, x20, [sp], #16;
            ret
        );
        let code = Code::decode(&[(0x1000, &body)]).unwrap();
        let data = ReadOnlyData::new(vec![(SOURCE, b"abc\0".to_vec())]);
        for length in [Some(0), Some(3), Some(4), None] {
            let mut machine = Machine::new(&code, &data);
            machine.track_owner(OWNER, OWNER + 32);
            machine.write(OWNER, 8, 42);
            machine.write(OWNER + 12, 4, 99);
            machine.set_register(0, OWNER + 8);
            machine.set_register(1, SOURCE);
            if let Some(length) = length {
                machine.set_register(2, length);
            }
            let paths = machine.run_paths(0x1000, &mut |target, machine| {
                Ok(calls()
                    .call(target, machine)
                    .unwrap_or_else(|| machine.opaque_call()))
            });
            assert!(!paths.is_empty());
            for path in paths {
                assert!(matches!(path.end, Ok(Exit::Returned)));
                // Every path writes the owner from the destination on, never before it.
                let bounded = length.is_some_and(|length| length < 4);
                assert_eq!(path.machine.read(OWNER, 8), Some(42));
                assert_eq!(path.machine.read(OWNER + 12, 4), bounded.then_some(99));
                if bounded {
                    assert_eq!(path.machine.read(OWNER + 8 + length.unwrap(), 1), Some(0));
                }
            }
        }
    }
}
