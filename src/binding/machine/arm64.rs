pub(super) const READ_ENTRY_REVISION: &str = "arm64-sdk483-read-entries/v2";

pub(super) fn read_entries() -> crate::binding::Machine {
    crate::binding::Machine {
        revision: READ_ENTRY_REVISION.into(),
        architecture: "arm64".into(),
        // mach/machine.h: CPU_TYPE_ARM | CPU_ARCH_ABI64.
        spawn_preference: 12 | 0x0100_0000,
        registers: [
            ("registration-token", "w1"),
            ("file", "x1"),
            ("reader", "x1"),
            ("owner", "x0"),
            ("field-token", "w2"),
            ("return", "lr"),
        ]
        .into_iter()
        .map(|(key, value)| (key.into(), value.into()))
        .collect(),
    }
}
