//! Effects shared by bounded constructor-summary consumers.
use super::evaluate::Machine;
use std::collections::BTreeMap;

/// Replace the receiver's remaining span with only the constructor's proven vtable points.
/// The caller establishes ownership and the allocation end; summaries do not retain embedded state.
pub(super) fn install_vtables(
    machine: &mut Machine<'_>,
    receiver: u64,
    end: u64,
    points: &BTreeMap<u64, u64>,
) -> Option<()> {
    let span = end.checked_sub(receiver)?;
    for &offset in points.keys() {
        if offset.checked_add(8)? > span {
            return None;
        }
    }
    machine.forget(receiver, span);
    for (&offset, &point) in points {
        machine.write(receiver + offset, 8, point);
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::evaluate::{Code, ReadOnlyData};

    #[test]
    fn constructor_summary_forgets_embedded_state_and_rejects_overflowing_points() {
        let code = Code::from_rows(vec![]);
        let data = ReadOnlyData::default();
        let mut machine = Machine::new(&code, &data);
        let owner = machine.reserve(64);
        machine.write(owner, 8, 1);
        machine.write(owner + 24, 8, 2);
        machine.write(owner + 56, 8, 3);
        assert_eq!(
            install_vtables(
                &mut machine,
                owner + 16,
                owner + 64,
                &BTreeMap::from([(0, 4), (32, 5)])
            ),
            Some(())
        );
        assert_eq!(machine.read(owner, 8), Some(1));
        assert_eq!(machine.read(owner + 16, 8), Some(4));
        assert_eq!(machine.read(owner + 48, 8), Some(5));
        assert_eq!(machine.read(owner + 24, 8), None);
        assert_eq!(machine.read(owner + 56, 8), None);
        for offset in [41, u64::MAX] {
            assert_eq!(
                install_vtables(
                    &mut machine,
                    owner + 16,
                    owner + 64,
                    &BTreeMap::from([(offset, 6)])
                ),
                None
            );
        }
    }
}
