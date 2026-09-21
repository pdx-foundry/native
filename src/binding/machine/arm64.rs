pub(super) fn loader_entry() -> crate::binding::Machine {
    crate::binding::Machine {
        architecture: "arm64".into(),
        // mach/machine.h: CPU_TYPE_ARM | CPU_ARCH_ABI64.
        spawn_preference: 12 | 0x0100_0000,
        // At a member function's entry: the receiver object and the return address.
        registers: [
            ("owner", "x0"),
            ("return", "lr"),
            ("file", "x1"),
            ("reader", "x1"),
            ("field-token", "w2"),
        ]
        .into_iter()
        .map(|(key, value)| (key.into(), value.into()))
        .collect(),
    }
}
