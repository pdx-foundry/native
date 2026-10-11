use crate::protocol::observation::{EntryRegisters, Machine};

pub(super) fn loader_entry() -> Machine {
    Machine {
        architecture: "arm64".into(),
        // mach/machine.h: CPU_TYPE_ARM | CPU_ARCH_ABI64.
        spawn_preference: 12 | 0x0100_0000,
        // At a member function's entry: the receiver object and the return address.
        registers: EntryRegisters {
            owner: "x0".into(),
            return_address: "lr".into(),
            file: "x1".into(),
            reader: "x1".into(),
            field_token: "w2".into(),
        },
    }
}
