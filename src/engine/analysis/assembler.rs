//! Authored ARM64 for tests, assembled by [dynasm](https://censoredusername.github.io/dynasm-rs/language/langref_aarch64.html).
//!
//! Write instructions as assembly with `arm64!`. The syntax differs from the decoder's in four
//! places: a branch or `adrp` target is an absolute address written `extern ADDRESS` (a `u64`
//! needs `as usize`); a register from a variable is `X(n)`, or `XSP(n)` where the operand may be
//! `sp`; an immediate from a variable needs `#`; a vector arrangement is `v0.d2`, not `v0.2d`.
//!
//! Write an `adrp` target as its page, such as `0x8000`, not `0x8010`: dynasm rounds an
//! unaligned target up to the next page. `Arm64::address` and `Arm64::load` take any address.
//!
//! ```ignore
//! let bytes = arm64!(at 0x1000;
//!     stp x29, x30, [sp, #-16]!;
//!     ldrh w11, [x9, x8, lsl #1];
//!     bl extern 0x1040;
//!     ret
//! );
//! ```
use dynasmrt::{DynasmApi, VecAssembler, aarch64::Aarch64Relocation};

/// Authored code from one start address. `arm64!` appends instructions; the methods append the
/// sequences that plain assembly cannot write with a named address.
pub struct Arm64 {
    start: u64,
    /// The dynasm assembler that `arm64!` writes to.
    pub ops: VecAssembler<Aarch64Relocation>,
}

// dynasm converts a register from a variable with `.into()`, even from `u8`.
#[allow(clippy::useless_conversion)]
impl Arm64 {
    /// Empty code whose first instruction is at `start`.
    pub fn at(start: u64) -> Self {
        Self {
            start,
            ops: VecAssembler::new(start as usize),
        }
    }

    /// The address of the first instruction.
    pub fn start(&self) -> u64 {
        self.start
    }

    /// The address of the next instruction.
    pub fn here(&self) -> u64 {
        self.start + self.ops.offset().0 as u64
    }

    /// `stp x29, x30, [sp, #-16]!` and `mov x29, sp`.
    pub fn prologue(&mut self) -> &mut Self {
        arm64!(self; stp x29, x30, [sp, #-16]!; mov x29, sp);
        self
    }

    /// `ldp x29, x30, [sp], #16`.
    pub fn epilogue(&mut self) -> &mut Self {
        arm64!(self; ldp x29, x30, [sp], #16);
        self
    }

    /// `adrp` and `add`: `address` in `register`.
    pub fn address(&mut self, register: u8, address: u64) -> &mut Self {
        let (page, page_offset) = split(address);
        arm64!(self;
            adrp X(register), extern page;
            add XSP(register), XSP(register), #page_offset
        );
        self
    }

    /// `adrp` and `ldr`: the pointer stored at `address` in `register`.
    pub fn load(&mut self, register: u8, address: u64) -> &mut Self {
        let (page, page_offset) = split(address);
        arm64!(self;
            adrp X(register), extern page;
            ldr X(register), [XSP(register), #page_offset]
        );
        self
    }

    /// `bl target`.
    pub fn call(&mut self, target: u64) -> &mut Self {
        arm64!(self; bl extern target as usize);
        self
    }

    /// `b target`.
    pub fn tail_call(&mut self, target: u64) -> &mut Self {
        arm64!(self; b extern target as usize);
        self
    }

    /// The little-endian code. Panics when a branch or `adrp` target is out of reach.
    pub fn bytes(self) -> Vec<u8> {
        self.ops.finalize().expect("authored ARM64 assembles")
    }
}

/// Assembles ARM64 instructions separated by `;`.
///
/// `arm64!(code; mov w1, #7; ret)` appends to the `Arm64` named `code`.
/// `arm64!(at 0x1000; mov w1, #7; ret)` returns the bytes of code that starts at 0x1000.
macro_rules! arm64 {
    (at $start:expr; $($instructions:tt)*) => {{
        let mut code = $crate::engine::analysis::assembler::Arm64::at($start);
        $crate::engine::analysis::assembler::arm64!(code; $($instructions)*);
        code.bytes()
    }};
    ($code:expr; $($instructions:tt)*) => {{
        #[allow(unused_imports)]
        use dynasmrt::{DynasmApi as _, DynasmLabelApi as _};
        dynasm::dynasm!($code.ops; .arch aarch64; $($instructions)*);
    }};
}
pub(crate) use arm64;

/// The 4 KiB page of `address`, which is the `adrp` target, and the offset in that page.
fn split(address: u64) -> (usize, u32) {
    ((address & !0xfff) as usize, (address & 0xfff) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The single instruction word in `bytes`.
    fn word(bytes: &[u8]) -> u32 {
        u32::from_le_bytes(bytes.try_into().expect("one instruction"))
    }

    /// Every instruction word the authored tests wrote by hand before this helper, at the address
    /// where they used it, and the ticket's jump-table loads. The `ldrh` and `ldrb` references
    /// are from `clang -target arm64-apple-macos` and `objdump`.
    #[test]
    fn every_authored_form_matches_its_reference_word() {
        const BASE: u64 = 0x1_0000_0000;
        let forms = [
            (arm64!(at 0x1000; stp x29, x30, [sp, #-16]!), 0xa9bf7bfd),
            (arm64!(at 0x1000; ldp x29, x30, [sp], #16), 0xa8c17bfd),
            (arm64!(at 0x1000; stp x0, x0, [x0]), 0xa9000000),
            (arm64!(at 0x1000; stp xzr, xzr, [x19, #0x70]), 0xa9077e7f),
            (arm64!(at 0x1000; stp xzr, xzr, [x19, #0x80]), 0xa9087e7f),
            (arm64!(at 0x1000; mov x29, sp), 0x910003fd),
            (arm64!(at 0x1000; mov x19, sp), 0x910003f3),
            (arm64!(at 0x1000; mov x0, x0), 0xaa0003e0),
            (arm64!(at 0x1000; mov x0, x1), 0xaa0103e0),
            (arm64!(at 0x1000; mov x1, x8), 0xaa0803e1),
            (arm64!(at 0x1000; mov x0, xzr), 0xaa1f03e0),
            (arm64!(at 0x1000; mov x2, xzr), 0xaa1f03e2),
            (arm64!(at 0x1000; mov w0, w1), 0x2a0103e0),
            (arm64!(at 0x1000; mov w0, #0), 0x52800000),
            (arm64!(at 0x1000; mov w3, #0), 0x52800003),
            (arm64!(at 0x1000; mov w8, #0), 0x52800008),
            (arm64!(at 0x1000; mov w3, #1), 0x52800023),
            (arm64!(at 0x1000; mov w1, #7), 0x528000e1),
            (arm64!(at 0x1000; mov w1, #8), 0x52800101),
            (arm64!(at 0x1000; add w0, w0, #32), 0x11008000),
            (arm64!(at 0x1000; add x0, x0, #0), 0x91000000),
            (arm64!(at 0x1000; add x2, x2, #0), 0x91000042),
            (arm64!(at 0x1000; add x0, x0, #8), 0x91002000),
            (arm64!(at 0x1000; add x19, x19, #8), 0x91002273),
            (arm64!(at 0x1000; add x2, x2, #0x10), 0x91004042),
            (arm64!(at 0x1000; add x0, x0, #0x38), 0x9100e000),
            (arm64!(at 0x1000; add x8, x0, #0x40), 0x91010008),
            (arm64!(at 0x1000; add sp, sp, #0x40), 0x910103ff),
            (arm64!(at 0x1000; sub sp, sp, #0x40), 0xd10103ff),
            (arm64!(at 0x1000; add x8, x19, #0x100, lsl #12), 0x91440268),
            (arm64!(at 0x1000; cmp x0, #0), 0xf100001f),
            (arm64!(at 0x1000; cmp w1, #7), 0x71001c3f),
            (arm64!(at 0x1000; cmp w2, #7), 0x71001c5f),
            (arm64!(at 0x1000; csel x0, x8, x0, ne), 0x9a801100),
            (arm64!(at 0x1000; ldr x0, [x0]), 0xf9400000),
            (arm64!(at 0x1000; ldr x0, [x1]), 0xf9400020),
            (arm64!(at 0x1000; ldr x8, [x8]), 0xf9400108),
            (arm64!(at 0x1000; ldr x0, [x0, #8]), 0xf9400400),
            (arm64!(at 0x1000; ldr x8, [x8, #16]), 0xf9400908),
            (arm64!(at 0x1000; ldr w1, [x0]), 0xb9400001),
            (arm64!(at 0x1000; str x0, [x0]), 0xf9000000),
            (arm64!(at 0x1000; str x10, [x8]), 0xf900010a),
            (arm64!(at 0x1000; str x8, [x19, #0x60]), 0xf9003268),
            (arm64!(at 0x1000; str x9, [x19, #0x68]), 0xf9003669),
            (arm64!(at 0x1000; str x3, [x2], #8), 0xf8008443),
            (arm64!(at 0x1000; str x9, [x8], #0x68), 0xf8068509),
            (arm64!(at 0x1000; str q0, [x1]), 0x3d800020),
            (arm64!(at 0x1000; str q0, [x1, #0x10]), 0x3d800420),
            (arm64!(at 0x1000; movi v0.d2, #0), 0x6f00e400),
            (
                arm64!(at 0x1000; movi v0.d2, #0xffffffffffffffff),
                0x6f07e7e0,
            ),
            (arm64!(at 0x1000; ldrh w11, [x9, x8, lsl #1]), 0x7868792b),
            (arm64!(at 0x1000; ldrb w11, [x9, x8]), 0x3868692b),
            (arm64!(at 0x1000; nop), 0xd503201f),
            (arm64!(at 0x1000; ret), 0xd65f03c0),
            (arm64!(at 0x100c; bl extern 0x1040), 0x9400000d),
            (arm64!(at 0x1018; bl extern 0x1018), 0x94000000),
            (arm64!(at 0x100c; b extern 0x5000), 0x14000ffd),
            (arm64!(at 0x1004; b.eq extern 0x1010), 0x54000060),
            (arm64!(at 0x1004; b.eq extern 0x1024), 0x54000100),
            (arm64!(at 0x1010; cbz w3, extern 0x1024), 0x340000a3),
            (arm64!(at 0x1010; cbnz w3, extern 0x1024), 0x350000a3),
            (arm64!(at 0x1010; adrp x8, extern 0x2000), 0xb0000008),
            (arm64!(at 0x100c; adrp x9, extern 0x3000), 0xd0000009),
            (arm64!(at 0x2004; adrp x2, extern 0x8000), 0xd0000022),
            (
                arm64!(at BASE + 0x1004; adrp x0, extern (BASE + 0x2000) as usize),
                0xb0000000,
            ),
            (
                arm64!(at BASE + 0x1010; adrp x8, extern (BASE + 0x4000) as usize),
                0xf0000008,
            ),
            (
                arm64!(at BASE + 0x100c; bl extern (BASE + 0x1020) as usize),
                0x94000005,
            ),
        ];

        for (bytes, reference) in forms {
            assert_eq!(word(&bytes), reference, "{:#010x}", reference);
        }
    }

    #[test]
    fn idioms_match_their_reference_words() {
        let mut code = Arm64::at(0x1000);
        code.prologue()
            .address(2, 0x9010)
            .load(8, 0x9018)
            .call(0x1000)
            .tail_call(0x2000)
            .epilogue();
        let words: Vec<u32> = code.bytes().chunks(4).map(word).collect();

        assert_eq!(
            words,
            [
                0xa9bf7bfd, 0x910003fd, // prologue
                0x90000042, 0x91004042, // adrp x2, 0x9000; add x2, x2, #0x10
                0x90000048, 0xf9400d08, // adrp x8, 0x9000; ldr x8, [x8, #0x18]
                0x97fffffa, // bl 0x1000
                0x140003f9, // b 0x2000
                0xa8c17bfd, // epilogue
            ]
        );
    }

    #[test]
    #[should_panic(expected = "authored ARM64 assembles")]
    fn a_branch_target_out_of_reach_is_refused() {
        arm64!(at 0x1000; b extern 0x1_0000_0000);
    }

    #[test]
    #[should_panic(expected = "out of range")]
    fn an_immediate_out_of_range_is_refused() {
        let immediate = 4096u32;
        arm64!(at 0x1000; add x0, x0, #immediate);
    }
}
