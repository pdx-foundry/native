//! Concrete evaluation of compiled switch code for one known input.
//!
//! The engine keeps several language tables as compiled `switch` statements over one integer:
//! which tokens are scope links, the scopes that a link supports, the output scope of a link, the
//! scope type of a token, and the name of a modifier category. This module runs such code for one
//! concrete input and reports how the run ended and what it computed.
//!
//! It is not a general emulator. [`Machine::run`] follows one path with known register and memory
//! values. A branch on an unknown value, an unsupported instruction, a jump outside the decoded
//! code, or the step bound ends the run as [`Unresolved`]; nothing is guessed. A store to an
//! unknown address makes every written byte unknown, because it may overwrite any of them.
//! Calls are not entered. The caller decides what each call returns, or stops the run there.
//!
//! [`Machine::run_paths`] is for code that checks run-time state before its answer, such as a
//! localization promotion that tests a database pointer. It follows both sides of a branch on an
//! unknown value and reports every path's end, so the caller can require that the paths agree.
//! It still chooses no path.
use std::collections::BTreeMap;

use super::InputError;
use super::decode::{Instruction, decode_arm64};

/// The most instructions that one run may execute.
const STEP_LIMIT: usize = 20_000;

/// The most paths that one [`Machine::run_paths`] follows.
pub const PATH_LIMIT: usize = 64;

/// The stack pointer at entry. It is outside every mapped section, so stack loads never read
/// executable data.
const STACK_TOP: u64 = 0x7fff_0000_0000;

/// First address of scratch objects that a caller allocates.
const OBJECT_BASE: u64 = 0x7ffe_0000_0000;

/// A run could not be followed to its end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unresolved(pub &'static str);

/// Read-only bytes that code can load: jump tables and string literals.
#[derive(Debug, Clone, Default)]
pub struct ReadOnlyData {
    /// Sections by start address.
    sections: BTreeMap<u64, Vec<u8>>,
}

impl ReadOnlyData {
    /// Sections must not overlap.
    pub fn new(sections: Vec<(u64, Vec<u8>)>) -> Self {
        Self {
            sections: sections.into_iter().collect(),
        }
    }

    fn byte(&self, address: u64) -> Option<u8> {
        let (start, bytes) = self.sections.range(..=address).next_back()?;
        let offset = usize::try_from(address - start).ok()?;
        bytes.get(offset).copied()
    }

    /// Load `width` little-endian bytes, at most eight, when every byte is mapped.
    pub fn read(&self, address: u64, width: u64) -> Option<u64> {
        (0..width.min(8)).try_fold(0u64, |value, offset| {
            let byte = self.byte(address.checked_add(offset)?)?;
            Some(value | u64::from(byte) << (offset * 8))
        })
    }

    /// The NUL-terminated string at `address`, if it is readable UTF-8.
    pub fn string(&self, address: u64) -> Option<String> {
        let mut bytes = Vec::new();
        for offset in 0..4096 {
            match self.byte(address.checked_add(offset)?)? {
                0 => return String::from_utf8(bytes).ok(),
                byte => bytes.push(byte),
            }
        }
        None
    }
}

/// Decoded instructions indexed by address.
#[derive(Debug, Clone, Default)]
pub struct Code {
    rows: BTreeMap<u64, Operation>,
}

impl Code {
    /// Decode each `(address, bytes)` range completely.
    pub fn decode(ranges: &[(u64, &[u8])]) -> Result<Self, InputError> {
        let mut rows = Vec::new();
        for (address, bytes) in ranges {
            for (index, chunk) in bytes.chunks(4096).enumerate() {
                rows.extend(
                    decode_arm64(chunk, address + (index * 4096) as u64)
                        .map_err(|error| InputError(error.to_string()))?,
                );
            }
        }
        Ok(Self::from_rows(rows))
    }

    /// Build code from rows that are already decoded.
    pub fn from_rows(rows: impl IntoIterator<Item = Instruction>) -> Self {
        Self {
            rows: rows
                .into_iter()
                .map(|row| (row.address, Operation::parse(&row)))
                .collect(),
        }
    }
}

/// What the caller does at a call instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Call {
    /// Continue after the call. `x0` receives the value; other caller-saved registers become
    /// unknown.
    Return(Option<u64>),
    /// End the run here.
    Stop,
}

/// How a completed run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// The code returned.
    Returned,
    /// The caller stopped the run at a call to this address.
    Stopped(u64),
    /// The path reached a trap instruction, so it does not return. Only
    /// [`Machine::run_paths`] reports it.
    Trapped,
}

/// How one path of [`Machine::run_paths`] ended, with the machine state at its end.
#[derive(Debug, Clone)]
pub struct Path<'a> {
    pub end: Result<Exit, Unresolved>,
    pub machine: Machine<'a>,
}

enum Walk {
    End(Result<Exit, Unresolved>),
    /// Continue each branch at its address, with its flags when the decision was on flags.
    Fork {
        branches: Vec<(Option<Flags>, u64)>,
        steps: usize,
    },
}

/// Register and memory state of one run.
#[derive(Debug, Clone)]
pub struct Machine<'a> {
    code: &'a Code,
    data: &'a ReadOnlyData,
    registers: [Option<u64>; 31],
    vectors: [Option<u128>; 32],
    stack_pointer: u64,
    flags: Option<Flags>,
    memory: BTreeMap<u64, Option<u8>>,
    next_object: u64,
}

impl<'a> Machine<'a> {
    /// A machine with unknown registers and an empty stack.
    pub fn new(code: &'a Code, data: &'a ReadOnlyData) -> Self {
        Self {
            code,
            data,
            registers: [None; 31],
            vectors: [None; 32],
            stack_pointer: STACK_TOP,
            flags: None,
            memory: BTreeMap::new(),
            next_object: OBJECT_BASE,
        }
    }

    /// Reserve `length` zeroed bytes of scratch memory and return their address.
    pub fn allocate(&mut self, length: u64) -> u64 {
        let address = self.next_object;
        for offset in 0..length {
            self.memory.insert(address + offset, Some(0));
        }
        self.next_object += length.next_multiple_of(16) + 16;
        address
    }

    /// Set general register `index` (`x0` is 0).
    pub fn set_register(&mut self, index: usize, value: u64) {
        self.registers[index] = Some(value);
    }

    /// The value of general register `index`, when it is known.
    pub fn register(&self, index: usize) -> Option<u64> {
        self.registers[index]
    }

    /// The present stack pointer.
    pub fn stack_pointer(&self) -> u64 {
        self.stack_pointer
    }

    /// Store `value` little-endian in `width` bytes.
    pub fn write(&mut self, address: u64, width: u64, value: u64) {
        for offset in 0..width {
            self.memory
                .insert(address + offset, Some((value >> (offset * 8)) as u8));
        }
    }

    /// Make `width` bytes at `address` unknown, such as a field that a call may have written.
    pub fn forget(&mut self, address: u64, width: u64) {
        for offset in 0..width {
            self.memory.insert(address + offset, None);
        }
    }

    /// Load `width` little-endian bytes, when every byte is known.
    pub fn read(&self, address: u64, width: u64) -> Option<u64> {
        self.read_bytes(address, width.min(8))
            .map(|value| value as u64)
    }

    fn read_bytes(&self, address: u64, width: u64) -> Option<u128> {
        let mut value = 0u128;
        for offset in 0..width {
            let address = address.checked_add(offset)?;
            let byte = match self.memory.get(&address) {
                Some(byte) => (*byte)?,
                None => self.data.byte(address)?,
            };
            value |= u128::from(byte) << (offset * 8);
        }
        Some(value)
    }

    /// Execute from `entry` until the code returns or `calls` stops it.
    pub fn run(
        &mut self,
        entry: u64,
        calls: &mut dyn FnMut(u64, &mut Machine<'a>) -> Result<Call, Unresolved>,
    ) -> Result<Exit, Unresolved> {
        let mut pc = entry;
        for _ in 0..STEP_LIMIT {
            let code = self.code;
            let operation = code.rows.get(&pc).ok_or(Unresolved("outside-code"))?;
            match self.step(operation)? {
                Flow::Next => pc += 4,
                Flow::Jump(target) => pc = target,
                Flow::Unknown { reason, .. } => return Err(Unresolved(reason)),
                Flow::IndirectCall(_) | Flow::Trap => return Err(Unresolved("instruction")),
                Flow::Call(target) => match calls(target, self)? {
                    Call::Return(value) => {
                        self.returned_from_call(value);
                        pc += 4;
                    }
                    Call::Stop => return Ok(Exit::Stopped(target)),
                },
                Flow::Return => return Ok(Exit::Returned),
            }
        }
        Err(Unresolved("step-limit"))
    }

    /// Execute from `entry` along every path, and report how each path ended.
    ///
    /// Unlike [`Machine::run`], a branch or conditional select on an unknown value continues on
    /// both sides, and a `b` to an address outside the decoded code is a tail call that goes to
    /// `calls`: when it returns, the path returns. A decision on unknown flags continues with
    /// flags that make the condition hold on one side and fail on the other, so a later decision
    /// on the same flags agrees with it.
    ///
    /// `calls` receives the target of each call and tail call, or `None` for a call through a
    /// register whose value is unknown. At most [`PATH_LIMIT`] paths are followed; a path that would
    /// exceed the limit ends as `Unresolved("path-limit")`. Each path has its own step limit.
    pub fn run_paths(
        self,
        entry: u64,
        calls: &mut dyn FnMut(Option<u64>, &mut Machine<'a>) -> Result<Call, Unresolved>,
    ) -> Vec<Path<'a>> {
        let mut pending = vec![(self, entry, 0)];
        let mut ended = Vec::new();

        while let Some((mut machine, pc, steps)) = pending.pop() {
            match machine.walk(pc, steps, calls) {
                Walk::End(end) => ended.push(Path { end, machine }),
                Walk::Fork { branches, steps } => {
                    if ended.len() + pending.len() + branches.len() > PATH_LIMIT {
                        ended.push(Path {
                            end: Err(Unresolved("path-limit")),
                            machine,
                        });
                        continue;
                    }

                    for (flags, pc) in branches.into_iter().rev() {
                        let mut branch = machine.clone();
                        if flags.is_some() {
                            branch.flags = flags;
                        }
                        pending.push((branch, pc, steps));
                    }
                }
            }
        }

        ended
    }

    /// Follow one path until it ends or reaches a branch on an unknown value.
    fn walk(
        &mut self,
        mut pc: u64,
        mut steps: usize,
        calls: &mut dyn FnMut(Option<u64>, &mut Machine<'a>) -> Result<Call, Unresolved>,
    ) -> Walk {
        while steps < STEP_LIMIT {
            steps += 1;
            let code = self.code;
            let Some(operation) = code.rows.get(&pc) else {
                return Walk::End(Err(Unresolved("outside-code")));
            };
            let flow = match self.step(operation) {
                Ok(flow) => flow,
                Err(Unresolved("flags")) if let Some(condition) = operation.condition() => {
                    return Walk::Fork {
                        branches: condition
                            .outcomes()
                            .into_iter()
                            .map(|flags| (Some(flags), pc))
                            .collect(),
                        steps: steps - 1,
                    };
                }
                Err(unresolved) => return Walk::End(Err(unresolved)),
            };

            match flow {
                Flow::Next => pc += 4,
                Flow::Jump(target) if !code.rows.contains_key(&target) => {
                    return Walk::End(self.tail_call(target, calls));
                }
                Flow::Jump(target) => pc = target,
                Flow::Unknown { target, .. } => {
                    return Walk::Fork {
                        branches: vec![(None, target), (None, pc + 4)],
                        steps,
                    };
                }
                Flow::Call(target) => match calls(Some(target), self) {
                    Ok(Call::Return(value)) => {
                        self.returned_from_call(value);
                        pc += 4;
                    }
                    Ok(Call::Stop) => return Walk::End(Ok(Exit::Stopped(target))),
                    Err(unresolved) => return Walk::End(Err(unresolved)),
                },
                Flow::IndirectCall(target) => match calls(target, self) {
                    Ok(Call::Return(value)) => {
                        self.returned_from_call(value);
                        pc += 4;
                    }
                    Ok(Call::Stop) => return Walk::End(Err(Unresolved("stopped-at-unknown-call"))),
                    Err(unresolved) => return Walk::End(Err(unresolved)),
                },
                Flow::Return => return Walk::End(Ok(Exit::Returned)),
                Flow::Trap => return Walk::End(Ok(Exit::Trapped)),
            }
        }

        Walk::End(Err(Unresolved("step-limit")))
    }

    fn tail_call(
        &mut self,
        target: u64,
        calls: &mut dyn FnMut(Option<u64>, &mut Machine<'a>) -> Result<Call, Unresolved>,
    ) -> Result<Exit, Unresolved> {
        match calls(Some(target), self)? {
            Call::Return(value) => {
                self.returned_from_call(value);
                Ok(Exit::Returned)
            }
            Call::Stop => Ok(Exit::Stopped(target)),
        }
    }

    /// A called function returned `value`; caller-saved registers and flags are unknown.
    fn returned_from_call(&mut self, value: Option<u64>) {
        self.registers[0] = value;
        self.registers[1..=18].fill(None);
        self.flags = None;
    }

    fn step(&mut self, operation: &Operation) -> Result<Flow, Unresolved> {
        let Operation { mnemonic, operands } = operation;
        let operands = operands.as_slice();
        match (mnemonic.as_str(), operands) {
            ("nop", []) => {}
            (
                "movi",
                [
                    Operand::Register(Register {
                        name: Name::Vector(index),
                        bytes,
                        lane,
                        ..
                    }),
                    Operand::Immediate(value),
                    rest @ ..,
                ],
            ) => {
                let shift = match rest {
                    [] => 0,
                    [Operand::Shift(Shift::Left, amount)] => *amount,
                    _ => return Err(Unresolved("movi-shift")),
                };
                self.vectors[*index] = Some(replicate((*value as u64) << shift, *lane, *bytes));
            }
            ("mov" | "movz", [destination, source]) => {
                let value = self.operand(source)?;
                self.assign(destination, value)?;
            }
            ("movn", [destination, source]) => {
                let value = self.operand(source)?.map(|value| !value);
                self.assign(destination, value)?;
            }
            ("movk", [destination, Operand::Immediate(part), rest @ ..]) => {
                let shift = match rest {
                    [] => 0,
                    [Operand::Shift(Shift::Left, amount)] => *amount,
                    _ => return Err(Unresolved("movk-shift")),
                };
                let mask = 0xffffu64 << shift;
                let value = self
                    .operand(destination)?
                    .map(|prior| (prior & !mask) | (((*part as u64) & 0xffff) << shift));
                self.assign(destination, value)?;
            }
            (
                "add" | "sub" | "and" | "orr" | "eor" | "lsl" | "lsr" | "asr" | "mul",
                [destination, left, right, rest @ ..],
            ) => {
                let wide = destination.is_wide();
                let left = self.operand(left)?;
                let right = self.modified(right, rest)?;
                let value = left
                    .zip(right)
                    .map(|(left, right)| binary(mnemonic, left, right, wide));
                self.assign(destination, value)?;
            }
            (
                "ubfx",
                [
                    destination,
                    source,
                    Operand::Immediate(lsb),
                    Operand::Immediate(width),
                ],
            ) => {
                let value = self
                    .operand(source)?
                    .map(|value| (value >> *lsb) & low_bits(*width as u64));
                self.assign(destination, value)?;
            }
            (
                "bfi",
                [
                    destination,
                    source,
                    Operand::Immediate(lsb),
                    Operand::Immediate(width),
                ],
            ) => {
                let field = low_bits(*width as u64) << *lsb;
                let value = self
                    .operand(destination)?
                    .zip(self.operand(source)?)
                    .map(|(prior, source)| (prior & !field) | ((source << *lsb) & field));
                self.assign(destination, value)?;
            }
            ("madd" | "msub" | "smaddl" | "umaddl", [destination, left, right, addend]) => {
                let widen = |value: u64| match mnemonic.as_str() {
                    "smaddl" => extend("sxtw", value),
                    "umaddl" => extend("uxtw", value),
                    _ => value,
                };
                let product = self
                    .operand(left)?
                    .zip(self.operand(right)?)
                    .map(|(left, right)| widen(left).wrapping_mul(widen(right)));
                let value = product.zip(self.operand(addend)?).map(|(product, addend)| {
                    if mnemonic == "msub" {
                        addend.wrapping_sub(product)
                    } else {
                        addend.wrapping_add(product)
                    }
                });
                self.assign(destination, value)?;
            }
            ("sxtb" | "sxth" | "sxtw" | "uxtb" | "uxth", [destination, source]) => {
                let value = self.operand(source)?.map(|value| extend(mnemonic, value));
                self.assign(destination, value)?;
            }
            ("cmp" | "cmn" | "tst", [left, right, rest @ ..]) => {
                let wide = left.is_wide();
                let left = self.operand(left)?;
                let right = self.modified(right, rest)?;
                self.flags = left
                    .zip(right)
                    .map(|(left, right)| Flags::compare(mnemonic, left, right, wide));
            }
            (
                "ccmp" | "ccmn",
                [
                    left,
                    right,
                    Operand::Immediate(fallback),
                    Operand::Condition(condition),
                ],
            ) => {
                if self.holds(*condition)? {
                    let wide = left.is_wide();
                    let left = self.operand(left)?;
                    let right = self.operand(right)?;
                    let kind = if mnemonic == "ccmp" { "cmp" } else { "cmn" };
                    self.flags = left
                        .zip(right)
                        .map(|(left, right)| Flags::compare(kind, left, right, wide));
                } else {
                    self.flags = Some(Flags::from_bits(*fallback as u8));
                }
            }
            (
                "csel" | "csinc" | "csinv" | "csneg",
                [destination, left, right, Operand::Condition(condition)],
            ) => {
                let wide = destination.is_wide();
                let value = if self.holds(*condition)? {
                    self.operand(left)?
                } else {
                    self.operand(right)?.map(|value| match mnemonic.as_str() {
                        "csinc" => value.wrapping_add(1),
                        "csinv" => !value,
                        "csneg" => value.wrapping_neg(),
                        _ => value,
                    })
                };
                self.assign(destination, value.map(|value| truncate(value, wide)))?;
            }
            ("cset", [destination, Operand::Condition(condition)]) => {
                let value = u64::from(self.holds(*condition)?);
                self.assign(destination, Some(value))?;
            }
            ("adr" | "adrp", [destination, Operand::Immediate(address)]) => {
                self.assign(destination, Some(*address as u64))?;
            }
            (
                "ldr" | "ldur",
                [
                    Operand::Register(Register {
                        name: Name::Vector(index),
                        bytes,
                        ..
                    }),
                    Operand::Memory(memory),
                    rest @ ..,
                ],
            ) => {
                let address = self.address(memory, rest)?;
                self.vectors[*index] = address.and_then(|address| self.read_bytes(address, *bytes));
            }
            (
                "str" | "stur",
                [
                    Operand::Register(Register {
                        name: Name::Vector(index),
                        bytes,
                        ..
                    }),
                    Operand::Memory(memory),
                    rest @ ..,
                ],
            ) => match self.address(memory, rest)? {
                Some(address) => self.store_bytes(address, *bytes, self.vectors[*index]),
                None => self.forget_memory(),
            },
            (
                "ldp",
                [
                    Operand::Register(Register {
                        name: Name::Vector(first),
                        bytes,
                        ..
                    }),
                    Operand::Register(Register {
                        name: Name::Vector(second),
                        ..
                    }),
                    Operand::Memory(memory),
                    rest @ ..,
                ],
            ) => {
                let address = self.address(memory, rest)?;
                self.vectors[*first] = address.and_then(|address| self.read_bytes(address, *bytes));
                self.vectors[*second] =
                    address.and_then(|address| self.read_bytes(address + bytes, *bytes));
            }
            (
                "stp",
                [
                    Operand::Register(Register {
                        name: Name::Vector(first),
                        bytes,
                        ..
                    }),
                    Operand::Register(Register {
                        name: Name::Vector(second),
                        ..
                    }),
                    Operand::Memory(memory),
                    rest @ ..,
                ],
            ) => match self.address(memory, rest)? {
                Some(address) => {
                    self.store_bytes(address, *bytes, self.vectors[*first]);
                    self.store_bytes(address + bytes, *bytes, self.vectors[*second]);
                }
                None => self.forget_memory(),
            },
            (load, [destination, Operand::Memory(memory), rest @ ..])
                if load.starts_with("ldr") || load.starts_with("ldur") =>
            {
                let (width, signed) = load_width(load, destination)?;
                let address = self.address(memory, rest)?;
                let value = address.and_then(|address| self.read(address, width));
                let value = value.map(|value| match signed {
                    Some(to_wide) => sign_extend(value, width, to_wide),
                    None => value,
                });
                self.assign(destination, value)?;
            }
            ("ldp", [first, second, Operand::Memory(memory), rest @ ..]) => {
                let width = if first.is_wide() { 8 } else { 4 };
                let address = self.address(memory, rest)?;
                let values = address
                    .map(|address| (self.read(address, width), self.read(address + width, width)));
                let (first_value, second_value) = values.unwrap_or((None, None));
                self.assign(first, first_value)?;
                self.assign(second, second_value)?;
            }
            (store, [source, Operand::Memory(memory), rest @ ..])
                if store.starts_with("str") || store.starts_with("stur") =>
            {
                let width = store_width(store, source)?;
                let value = self.operand(source)?;
                match self.address(memory, rest)? {
                    Some(address) => self.store(address, width, value),
                    None => self.forget_memory(),
                }
            }
            ("stp", [first, second, Operand::Memory(memory), rest @ ..]) => {
                let width = if first.is_wide() { 8 } else { 4 };
                let first = self.operand(first)?;
                let second = self.operand(second)?;
                match self.address(memory, rest)? {
                    Some(address) => {
                        self.store(address, width, first);
                        self.store(address + width, width, second);
                    }
                    None => self.forget_memory(),
                }
            }
            ("b", [Operand::Immediate(target)]) => return Ok(Flow::Jump(*target as u64)),
            (branch, [Operand::Immediate(target)]) if branch.starts_with("b.") => {
                let condition =
                    Condition::parse(&branch[2..]).ok_or(Unresolved("branch-condition"))?;
                if self.holds(condition)? {
                    return Ok(Flow::Jump(*target as u64));
                }
            }
            ("cbz" | "cbnz", [register, Operand::Immediate(target)]) => {
                let Some(value) = self.operand(register)? else {
                    return Ok(Flow::Unknown {
                        target: *target as u64,
                        reason: "branch-value",
                    });
                };
                if (value == 0) == (mnemonic == "cbz") {
                    return Ok(Flow::Jump(*target as u64));
                }
            }
            (
                "tbz" | "tbnz",
                [
                    register,
                    Operand::Immediate(bit),
                    Operand::Immediate(target),
                ],
            ) => {
                let Some(value) = self.operand(register)? else {
                    return Ok(Flow::Unknown {
                        target: *target as u64,
                        reason: "branch-value",
                    });
                };
                let set = value >> bit & 1 == 1;
                if set == (mnemonic == "tbnz") {
                    return Ok(Flow::Jump(*target as u64));
                }
            }
            ("br", [register]) => {
                let target = self.operand(register)?.ok_or(Unresolved("branch-value"))?;
                return Ok(Flow::Jump(target));
            }
            ("bl", [Operand::Immediate(target)]) => return Ok(Flow::Call(*target as u64)),
            ("blr", [register]) => return Ok(Flow::IndirectCall(self.operand(register)?)),
            ("ret", []) => return Ok(Flow::Return),
            ("brk", [Operand::Immediate(_)]) => return Ok(Flow::Trap),
            _ => return Err(Unresolved("instruction")),
        }
        Ok(Flow::Next)
    }

    fn holds(&self, condition: Condition) -> Result<bool, Unresolved> {
        let flags = self.flags.ok_or(Unresolved("flags"))?;
        Ok(condition.holds(flags))
    }

    fn operand(&self, operand: &Operand) -> Result<Option<u64>, Unresolved> {
        match operand {
            Operand::Immediate(value) => Ok(Some(*value as u64)),
            Operand::Register(register) => Ok(self.read_register(*register)),
            _ => Err(Unresolved("operand")),
        }
    }

    /// A second source operand with its optional shift or extension.
    fn modified(&self, operand: &Operand, rest: &[Operand]) -> Result<Option<u64>, Unresolved> {
        let value = self.operand(operand)?;
        match rest {
            [] => Ok(value),
            [Operand::Shift(kind, amount)] => Ok(value.map(|value| kind.apply(value, *amount))),
            [Operand::Extend(kind, amount)] => Ok(value.map(|value| extend(kind, value) << amount)),
            _ => Err(Unresolved("operand")),
        }
    }

    fn read_register(&self, register: Register) -> Option<u64> {
        let value = match register.name {
            Name::Zero => Some(0),
            Name::StackPointer => Some(self.stack_pointer),
            Name::General(index) => self.registers[index],
            Name::Vector(_) => None,
        };
        value.map(|value| truncate(value, register.wide))
    }

    fn assign(&mut self, destination: &Operand, value: Option<u64>) -> Result<(), Unresolved> {
        let Operand::Register(register) = destination else {
            return Err(Unresolved("destination"));
        };
        let value = value.map(|value| truncate(value, register.wide));
        match register.name {
            Name::Zero => {}
            Name::StackPointer => {
                self.stack_pointer = value.ok_or(Unresolved("stack-pointer"))?;
            }
            Name::General(index) => self.registers[index] = value,
            Name::Vector(_) => return Err(Unresolved("destination")),
        }
        Ok(())
    }

    /// The effective address, applying pre- or post-index write-back to the base register.
    fn address(&mut self, memory: &Memory, rest: &[Operand]) -> Result<Option<u64>, Unresolved> {
        let base = self.read_register(memory.base);
        let index = match &memory.index {
            None => Some(0),
            Some((register, modifier)) => {
                self.modified(&Operand::Register(*register), modifier.as_slice())?
            }
        };
        let offset = base
            .zip(index)
            .map(|(base, index)| base.wrapping_add(index).wrapping_add(memory.offset as u64));
        match rest {
            [] if memory.write_back => {
                self.assign(&Operand::Register(memory.base), offset)?;
                Ok(offset)
            }
            [] => Ok(offset),
            [Operand::Immediate(post)] => {
                let updated = base.map(|base| base.wrapping_add(*post as u64));
                self.assign(&Operand::Register(memory.base), updated)?;
                Ok(base)
            }
            _ => Err(Unresolved("addressing")),
        }
    }

    /// A store to an unknown address may overwrite any byte, so no written byte stays known.
    fn forget_memory(&mut self) {
        for byte in self.memory.values_mut() {
            *byte = None;
        }
    }

    fn store(&mut self, address: u64, width: u64, value: Option<u64>) {
        self.store_bytes(address, width, value.map(u128::from));
    }

    fn store_bytes(&mut self, address: u64, width: u64, value: Option<u128>) {
        for offset in 0..width {
            let byte = value.map(|value| (value >> (offset * 8)) as u8);
            self.memory.insert(address + offset, byte);
        }
    }
}

enum Flow {
    Next,
    Jump(u64),
    /// A conditional branch to `target` whose condition is unknown.
    Unknown {
        target: u64,
        reason: &'static str,
    },
    Call(u64),
    /// A call through a register, whose target may be unknown.
    IndirectCall(Option<u64>),
    Return,
    Trap,
}

fn binary(mnemonic: &str, left: u64, right: u64, wide: bool) -> u64 {
    let bits = if wide { 64 } else { 32 };
    match mnemonic {
        "add" => left.wrapping_add(right),
        "sub" => left.wrapping_sub(right),
        "and" => left & right,
        "orr" => left | right,
        "eor" => left ^ right,
        "mul" => left.wrapping_mul(right),
        "lsl" => left.wrapping_shl((right % bits) as u32),
        "lsr" => truncate(left, wide) >> (right % bits),
        _ => (sign_extend(truncate(left, wide), bits / 8, true) as i64 >> (right % bits)) as u64,
    }
}

fn extend(kind: &str, value: u64) -> u64 {
    match kind {
        "sxtb" => value as u8 as i8 as i64 as u64,
        "sxth" => value as u16 as i16 as i64 as u64,
        "sxtw" => value as u32 as i32 as i64 as u64,
        "uxtb" => value as u8 as u64,
        "uxth" => value as u16 as u64,
        "uxtw" => value as u32 as u64,
        _ => value,
    }
}

/// A mask of the low `width` bits.
fn low_bits(width: u64) -> u64 {
    if width >= 64 {
        u64::MAX
    } else {
        (1 << width) - 1
    }
}

/// `value` repeated in each `lane`-byte lane of a `bytes`-byte view. Bytes above the view are
/// zero, as a write to a 64-bit vector view clears the upper half.
fn replicate(value: u64, lane: u64, bytes: u64) -> u128 {
    let lane_mask = if lane >= 8 {
        u64::MAX
    } else {
        (1 << (lane * 8)) - 1
    };
    let lane_value = u128::from(value & lane_mask);
    (0..bytes / lane).fold(0, |vector, index| vector | lane_value << (index * lane * 8))
}

fn truncate(value: u64, wide: bool) -> u64 {
    if wide { value } else { value as u32 as u64 }
}

fn sign_extend(value: u64, width: u64, to_wide: bool) -> u64 {
    let shift = 64 - width * 8;
    let extended = ((value << shift) as i64 >> shift) as u64;
    truncate(extended, to_wide)
}

/// Width in bytes and, for a signed load, whether it extends to 64 bits.
fn load_width(mnemonic: &str, destination: &Operand) -> Result<(u64, Option<bool>), Unresolved> {
    let wide = destination.is_wide();
    let register_width = if wide { 8 } else { 4 };
    let suffix = mnemonic
        .strip_prefix("ldur")
        .or_else(|| mnemonic.strip_prefix("ldr"))
        .ok_or(Unresolved("instruction"))?;
    Ok(match suffix {
        "" => (register_width, None),
        "b" => (1, None),
        "h" => (2, None),
        "sb" => (1, Some(wide)),
        "sh" => (2, Some(wide)),
        "sw" => (4, Some(true)),
        _ => return Err(Unresolved("instruction")),
    })
}

fn store_width(mnemonic: &str, source: &Operand) -> Result<u64, Unresolved> {
    let suffix = mnemonic
        .strip_prefix("stur")
        .or_else(|| mnemonic.strip_prefix("str"))
        .ok_or(Unresolved("instruction"))?;
    match suffix {
        "" if source.is_wide() => Ok(8),
        "" => Ok(4),
        "b" => Ok(1),
        "h" => Ok(2),
        _ => Err(Unresolved("instruction")),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Flags {
    negative: bool,
    zero: bool,
    carry: bool,
    overflow: bool,
}

impl Flags {
    fn compare(kind: &str, left: u64, right: u64, wide: bool) -> Self {
        let bits = if wide { 64 } else { 32 };
        let left = truncate(left, wide);
        let right = truncate(right, wide);
        let sign = |value: u64| value >> (bits - 1) & 1 == 1;
        if kind == "tst" {
            let result = left & right;
            return Self {
                negative: sign(result),
                zero: result == 0,
                carry: false,
                overflow: false,
            };
        }
        let (right, carry_in) = if kind == "cmn" {
            (right, 0)
        } else {
            (truncate(!right, wide), 1)
        };
        let sum = u128::from(left) + u128::from(right) + carry_in;
        let result = truncate(sum as u64, wide);
        Self {
            negative: sign(result),
            zero: result == 0,
            carry: sum >> bits != 0,
            overflow: sign(left) == sign(right) && sign(result) != sign(left),
        }
    }

    fn from_bits(bits: u8) -> Self {
        Self {
            negative: bits & 8 != 0,
            zero: bits & 4 != 0,
            carry: bits & 2 != 0,
            overflow: bits & 1 != 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Condition {
    Equal,
    NotEqual,
    HigherOrSame,
    Lower,
    Negative,
    Positive,
    Overflow,
    NoOverflow,
    Higher,
    LowerOrSame,
    GreaterOrEqual,
    Less,
    Greater,
    LessOrEqual,
    Always,
}

impl Condition {
    fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "eq" => Self::Equal,
            "ne" => Self::NotEqual,
            "hs" | "cs" => Self::HigherOrSame,
            "lo" | "cc" => Self::Lower,
            "mi" => Self::Negative,
            "pl" => Self::Positive,
            "vs" => Self::Overflow,
            "vc" => Self::NoOverflow,
            "hi" => Self::Higher,
            "ls" => Self::LowerOrSame,
            "ge" => Self::GreaterOrEqual,
            "lt" => Self::Less,
            "gt" => Self::Greater,
            "le" => Self::LessOrEqual,
            "al" => Self::Always,
            _ => return None,
        })
    }

    /// Flags that make the condition hold, and flags that make it fail, when each exists.
    fn outcomes(self) -> Vec<Flags> {
        let all: Vec<_> = (0..16).map(Flags::from_bits).collect();
        [true, false]
            .into_iter()
            .filter_map(|wanted| {
                all.iter()
                    .copied()
                    .find(|flags| self.holds(*flags) == wanted)
            })
            .collect()
    }

    fn holds(self, flags: Flags) -> bool {
        let Flags {
            negative,
            zero,
            carry,
            overflow,
        } = flags;
        match self {
            Self::Equal => zero,
            Self::NotEqual => !zero,
            Self::HigherOrSame => carry,
            Self::Lower => !carry,
            Self::Negative => negative,
            Self::Positive => !negative,
            Self::Overflow => overflow,
            Self::NoOverflow => !overflow,
            Self::Higher => carry && !zero,
            Self::LowerOrSame => !carry || zero,
            Self::GreaterOrEqual => negative == overflow,
            Self::Less => negative != overflow,
            Self::Greater => !zero && negative == overflow,
            Self::LessOrEqual => zero || negative != overflow,
            Self::Always => true,
        }
    }
}

#[derive(Debug, Clone)]
struct Operation {
    mnemonic: String,
    operands: Vec<Operand>,
}

impl Operation {
    /// The condition that the instruction tests, when it tests one.
    fn condition(&self) -> Option<Condition> {
        if let Some(suffix) = self.mnemonic.strip_prefix("b.") {
            return Condition::parse(suffix);
        }

        self.operands.iter().find_map(|operand| match operand {
            Operand::Condition(condition) => Some(*condition),
            _ => None,
        })
    }

    /// An operand that does not parse makes the whole row unsupported, so it can never be read as
    /// a different instruction.
    fn parse(row: &Instruction) -> Self {
        let operands: Option<Vec<_>> = split(&row.operands)
            .into_iter()
            .map(Operand::parse)
            .collect();
        match operands {
            Some(operands) => Self {
                mnemonic: row.operation.clone(),
                operands,
            },
            None => Self {
                mnemonic: format!("unsupported {}", row.operation),
                operands: Vec::new(),
            },
        }
    }
}

/// Split operands at commas outside brackets.
fn split(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (index, character) in text.char_indices() {
        match character {
            '[' => depth += 1,
            ']' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&text[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    if start < text.len() {
        parts.push(&text[start..]);
    }
    parts
}

#[derive(Debug, Clone)]
enum Operand {
    Register(Register),
    Immediate(i64),
    Shift(Shift, u64),
    Extend(&'static str, u64),
    Condition(Condition),
    Memory(Memory),
}

impl Operand {
    fn parse(text: &str) -> Option<Self> {
        if let Some(inner) = text.strip_prefix('[') {
            return Memory::parse(inner).map(Self::Memory);
        }
        if let Some(value) = text.strip_prefix('#') {
            return immediate(value).map(Self::Immediate);
        }
        for (prefix, kind) in [
            ("lsl", Shift::Left),
            ("lsr", Shift::Right),
            ("asr", Shift::Arithmetic),
        ] {
            if let Some(amount) = text.strip_prefix(prefix)
                && let Some(amount) = amount.strip_prefix('#')
            {
                return Some(Self::Shift(kind, immediate(amount)? as u64));
            }
        }
        for kind in [
            "uxtb", "uxth", "uxtw", "uxtx", "sxtb", "sxth", "sxtw", "sxtx",
        ] {
            if let Some(amount) = text.strip_prefix(kind) {
                let amount = match amount.strip_prefix('#') {
                    Some(amount) => immediate(amount)? as u64,
                    None if amount.is_empty() => 0,
                    None => return None,
                };
                return Some(Self::Extend(kind, amount));
            }
        }
        if let Some(condition) = Condition::parse(text) {
            return Some(Self::Condition(condition));
        }
        Register::parse(text).map(Self::Register)
    }

    fn is_wide(&self) -> bool {
        matches!(self, Self::Register(register) if register.wide)
    }
}

fn immediate(text: &str) -> Option<i64> {
    let (negative, text) = match text.strip_prefix('-') {
        Some(text) => (true, text),
        None => (false, text),
    };
    let value = match text.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).ok()?,
        None => text.parse().ok()?,
    } as i64;
    Some(if negative {
        value.wrapping_neg()
    } else {
        value
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shift {
    Left,
    Right,
    Arithmetic,
}

impl Shift {
    fn apply(self, value: u64, amount: u64) -> u64 {
        match self {
            Self::Left => value.wrapping_shl(amount as u32),
            Self::Right => value.wrapping_shr(amount as u32),
            Self::Arithmetic => ((value as i64) >> amount) as u64,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Register {
    name: Name,
    wide: bool,
    /// Width in bytes of a vector register view. Loads, stores and `movi` use vector registers.
    bytes: u64,
    /// Width in bytes of one lane of an arranged vector view such as `v0.4s`, else `bytes`.
    lane: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Name {
    General(usize),
    StackPointer,
    Zero,
    Vector(usize),
}

impl Register {
    fn parse(text: &str) -> Option<Self> {
        let (name, wide) = match text {
            "sp" => (Name::StackPointer, true),
            "wsp" => (Name::StackPointer, false),
            "xzr" => (Name::Zero, true),
            "wzr" => (Name::Zero, false),
            "fp" => (Name::General(29), true),
            "lr" => (Name::General(30), true),
            _ if text.starts_with('v') => return Self::arranged_vector(&text[1..]),
            _ if text.starts_with(['q', 'd', 's', 'h', 'b']) => {
                let bytes = match text.as_bytes()[0] {
                    b'q' => 16,
                    b'd' => 8,
                    b's' => 4,
                    b'h' => 2,
                    _ => 1,
                };
                let number = text[1..].parse::<usize>().ok()?;
                (number <= 31).then_some(())?;
                return Some(Self {
                    name: Name::Vector(number),
                    wide: false,
                    bytes,
                    lane: bytes,
                });
            }
            _ => {
                let wide = text.starts_with('x');
                let number = text.strip_prefix(['x', 'w'])?.parse::<usize>().ok()?;
                (number <= 30).then_some(())?;
                (Name::General(number), wide)
            }
        };
        let bytes = if wide { 8 } else { 4 };
        Some(Self {
            name,
            wide,
            bytes,
            lane: bytes,
        })
    }

    /// `text` is the part after `v`, such as `0.2d` or `31.16b`.
    fn arranged_vector(text: &str) -> Option<Self> {
        let (number, arrangement) = text.split_once('.')?;
        let number = number.parse::<usize>().ok()?;
        (number <= 31).then_some(())?;
        let (bytes, lane) = match arrangement {
            "2d" => (16, 8),
            "4s" => (16, 4),
            "8h" => (16, 2),
            "16b" => (16, 1),
            "1d" => (8, 8),
            "2s" => (8, 4),
            "4h" => (8, 2),
            "8b" => (8, 1),
            _ => return None,
        };
        Some(Self {
            name: Name::Vector(number),
            wide: false,
            bytes,
            lane,
        })
    }
}

#[derive(Debug, Clone)]
struct Memory {
    base: Register,
    index: Option<(Register, Vec<Operand>)>,
    offset: i64,
    write_back: bool,
}

impl Memory {
    /// `inner` is the text after `[`: `x1,#0x17]`, `x9,x8,lsl#1]` or `sp,#-0x10]!`.
    fn parse(inner: &str) -> Option<Self> {
        let (inner, write_back) = match inner.strip_suffix("]!") {
            Some(inner) => (inner, true),
            None => (inner.strip_suffix(']')?, false),
        };
        let mut parts = inner.split(',');
        let base = Register::parse(parts.next()?)?;
        let rest: Vec<_> = parts.map(Operand::parse).collect::<Option<_>>()?;
        let (index, offset) = match rest.as_slice() {
            [] => (None, 0),
            [Operand::Immediate(offset)] => (None, *offset),
            [Operand::Register(register), modifier @ ..] => {
                (Some((*register, modifier.to_vec())), 0)
            }
            _ => return None,
        };
        Some(Self {
            base,
            index,
            offset,
            write_back,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(lines: &[(u64, &str, &str)]) -> Code {
        Code::from_rows(
            lines
                .iter()
                .map(|(address, operation, operands)| Instruction {
                    address: *address,
                    bytes: [0; 4],
                    operation: (*operation).into(),
                    operands: (*operands).into(),
                }),
        )
    }

    fn returned(code: &Code, data: &ReadOnlyData, input: u64) -> Result<Option<u64>, Unresolved> {
        let mut machine = Machine::new(code, data);
        machine.set_register(0, input);
        let exit = machine.run(0x100, &mut |_, _| Ok(Call::Return(None)))?;
        assert_eq!(exit, Exit::Returned);
        Ok(machine.register(0))
    }

    #[test]
    fn range_compares_follow_signed_and_unsigned_conditions() {
        let code = rows(&[
            (0x100, "mov", "w9,#0x2a17"),
            (0x104, "cmp", "w0,w9"),
            (0x108, "b.le", "#0x118"),
            (0x10c, "sub", "w8,w0,#0x2a18"),
            (0x110, "cmp", "w8,#0x12"),
            (0x114, "b.hi", "#0x120"),
            (0x118, "mov", "x0,#1"),
            (0x11c, "ret", ""),
            (0x120, "mov", "x0,#2"),
            (0x124, "ret", ""),
        ]);
        let data = ReadOnlyData::default();
        assert_eq!(returned(&code, &data, 0x2a17), Ok(Some(1)));
        assert_eq!(returned(&code, &data, 0x2a18), Ok(Some(1)));
        assert_eq!(returned(&code, &data, 0x2a18 + 0x13), Ok(Some(2)));
        assert_eq!(returned(&code, &data, u32::MAX as u64), Ok(Some(1)));
    }

    #[test]
    fn bit_test_membership_uses_a_wide_mask() {
        let code = rows(&[
            (0x100, "sub", "w8,w0,#0x10"),
            (0x104, "mov", "w9,#1"),
            (0x108, "lsl", "x8,x9,x8"),
            (0x10c, "mov", "x9,#0x3"),
            (0x110, "movk", "x9,#0x20,lsl#32"),
            (0x114, "tst", "x8,x9"),
            (0x118, "cset", "w0,ne"),
            (0x11c, "ret", ""),
        ]);
        let data = ReadOnlyData::default();
        assert_eq!(returned(&code, &data, 0x10), Ok(Some(1)));
        assert_eq!(returned(&code, &data, 0x12), Ok(Some(0)));
        assert_eq!(returned(&code, &data, 0x10 + 37), Ok(Some(1)));
    }

    #[test]
    fn halfword_jump_table_selects_its_case() {
        let code = rows(&[
            (0x100, "adrp", "x9,#0x1000"),
            (0x104, "add", "x9,x9,#0x10"),
            (0x108, "adr", "x10,#0x118"),
            (0x10c, "ldrh", "w11,[x9,x0,lsl#1]"),
            (0x110, "add", "x10,x10,x11,lsl#2"),
            (0x114, "br", "x10"),
            (0x118, "mov", "x0,#7"),
            (0x11c, "ret", ""),
            (0x120, "mov", "x0,#9"),
            (0x124, "ret", ""),
        ]);
        let data = ReadOnlyData::new(vec![(0x1010, vec![0, 0, 2, 0])]);
        assert_eq!(returned(&code, &data, 0), Ok(Some(7)));
        assert_eq!(returned(&code, &data, 1), Ok(Some(9)));
        assert_eq!(returned(&code, &data, 2), Err(Unresolved("branch-value")));
    }

    #[test]
    fn stores_to_scratch_objects_can_be_read_back() {
        let code = rows(&[
            (0x100, "mov", "w8,#8"),
            (0x104, "strb", "w8,[x1,#0x17]"),
            (0x108, "mov", "x8,#0x6544"),
            (0x10c, "movk", "x8,#0x6f70,lsl#16"),
            (0x110, "str", "x8,[x1]"),
            (0x114, "stp", "x29,x30,[sp,#-0x10]!"),
            (0x118, "ldp", "x29,x30,[sp],#0x10"),
            (0x11c, "ret", ""),
        ]);
        let data = ReadOnlyData::default();
        let mut machine = Machine::new(&code, &data);
        let object = machine.allocate(24);
        machine.set_register(1, object);
        assert_eq!(
            machine.run(0x100, &mut |_, _| Ok(Call::Return(None))),
            Ok(Exit::Returned)
        );
        assert_eq!(machine.read(object + 0x17, 1), Some(8));
        assert_eq!(machine.read(object, 4), Some(0x6f70_6544));
    }

    #[test]
    fn vector_registers_copy_sixteen_bytes() {
        let code = rows(&[
            (0x100, "adrp", "x8,#0x1000"),
            (0x104, "ldr", "q0,[x8]"),
            (0x108, "str", "q0,[x1]"),
            (0x10c, "ret", ""),
        ]);
        let data = ReadOnlyData::new(vec![(0x1000, b"Galactic Communi".to_vec())]);
        let mut machine = Machine::new(&code, &data);
        let object = machine.allocate(24);
        machine.set_register(1, object);
        machine
            .run(0x100, &mut |_, _| Ok(Call::Return(None)))
            .unwrap();
        assert_eq!(
            machine.read(object, 8),
            Some(u64::from_le_bytes(*b"Galactic"))
        );
        assert_eq!(
            machine.read(object + 8, 8),
            Some(u64::from_le_bytes(*b" Communi"))
        );
    }

    #[test]
    fn calls_return_or_stop_as_the_caller_decides() {
        let code = rows(&[
            (0x100, "bl", "#0x900"),
            (0x104, "add", "x0,x0,#1"),
            (0x108, "bl", "#0x904"),
            (0x10c, "ret", ""),
        ]);
        let data = ReadOnlyData::default();
        let mut machine = Machine::new(&code, &data);
        let exit = machine.run(0x100, &mut |target, _| {
            Ok(if target == 0x900 {
                Call::Return(Some(41))
            } else {
                Call::Stop
            })
        });
        assert_eq!(exit, Ok(Exit::Stopped(0x904)));
        assert_eq!(machine.register(0), Some(42));
    }

    #[test]
    fn unknown_values_and_instructions_are_unresolved() {
        let data = ReadOnlyData::default();
        let branch = rows(&[(0x100, "cbz", "x3,#0x100")]);
        assert_eq!(returned(&branch, &data, 0), Err(Unresolved("branch-value")));
        let unknown = rows(&[(0x100, "fmov", "s0,#1.5")]);
        assert_eq!(returned(&unknown, &data, 0), Err(Unresolved("instruction")));
        let store = rows(&[
            (0x100, "str", "x0,[sp]"),
            (0x104, "str", "x0,[x5]"),
            (0x108, "ldr", "x0,[sp]"),
            (0x10c, "ret", ""),
        ]);
        assert_eq!(returned(&store, &data, 7), Ok(None));
        let outside = rows(&[(0x100, "b", "#0x200")]);
        assert_eq!(
            returned(&outside, &data, 0),
            Err(Unresolved("outside-code"))
        );
        let spin = rows(&[(0x100, "b", "#0x100")]);
        assert_eq!(returned(&spin, &data, 0), Err(Unresolved("step-limit")));
    }

    #[test]
    fn decoded_movi_fills_every_lane() {
        let words: [u32; 5] = [
            0x6f07e7e0, // movi v0.2d,#0xffffffffffffffff
            0x3d800020, // str q0,[x1]
            0x6f00e400, // movi v0.2d,#0
            0x3d800420, // str q0,[x1,#0x10]
            0xd65f03c0, // ret
        ];
        let bytes: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
        let code = Code::decode(&[(0x100, &bytes)]).unwrap();
        let data = ReadOnlyData::default();
        let mut machine = Machine::new(&code, &data);
        let object = machine.allocate(32);
        machine.set_register(1, object);

        assert_eq!(
            machine.run(0x100, &mut |_, _| Ok(Call::Return(None))),
            Ok(Exit::Returned)
        );
        assert_eq!(machine.read(object, 8), Some(u64::MAX));
        assert_eq!(machine.read(object + 8, 8), Some(u64::MAX));
        assert_eq!(machine.read(object + 16, 8), Some(0));
        assert_eq!(machine.read(object + 24, 8), Some(0));
    }

    #[test]
    fn movi_replicates_a_shifted_lane_and_clears_above_a_half_view() {
        assert_eq!(
            replicate(0x2a << 8, 4, 16),
            0x2a00_0000_2a00_0000_2a00_0000_2a00
        );
        assert_eq!(replicate(0xff, 1, 8), 0xffff_ffff_ffff_ffff);
    }

    fn returned_values(paths: &[Path<'_>]) -> Vec<Option<u64>> {
        let mut values: Vec<_> = paths
            .iter()
            .map(|path| {
                assert_eq!(path.end, Ok(Exit::Returned));
                path.machine.register(0)
            })
            .collect();
        values.sort();
        values
    }

    #[test]
    fn paths_follow_both_sides_of_an_unknown_branch() {
        let code = rows(&[
            (0x100, "cbz", "x3,#0x10c"),
            (0x104, "mov", "x0,#1"),
            (0x108, "ret", ""),
            (0x10c, "cmp", "x4,#7"),
            (0x110, "b.eq", "#0x11c"),
            (0x114, "mov", "x0,#2"),
            (0x118, "ret", ""),
            (0x11c, "mov", "x0,#3"),
            (0x120, "ret", ""),
        ]);
        let data = ReadOnlyData::default();
        let paths = Machine::new(&code, &data).run_paths(0x100, &mut |_, _| Ok(Call::Return(None)));

        assert_eq!(returned_values(&paths), [Some(1), Some(2), Some(3)]);
    }

    #[test]
    fn paths_split_a_select_on_unknown_flags_and_keep_later_decisions_consistent() {
        let code = rows(&[
            (0x100, "cmp", "x3,#0"),
            (0x104, "cset", "w0,eq"),
            (0x108, "b.eq", "#0x114"),
            (0x10c, "add", "x0,x0,#0x10"),
            (0x110, "ret", ""),
            (0x114, "add", "x0,x0,#0x20"),
            (0x118, "ret", ""),
        ]);
        let data = ReadOnlyData::default();
        let paths = Machine::new(&code, &data).run_paths(0x100, &mut |_, _| Ok(Call::Return(None)));

        assert_eq!(returned_values(&paths), [Some(0x10), Some(0x21)]);
        assert_eq!(
            returned(&code, &data, 0),
            Err(Unresolved("flags")),
            "a single run still refuses the unknown flags"
        );
    }

    #[test]
    fn bitfield_and_multiply_add_instructions_compute_their_values() {
        let code = rows(&[
            (0x100, "mov", "w8,#0x1234"),
            (0x104, "ubfx", "w9,w8,#8,#8"),
            (0x108, "mov", "w10,#0xffff"),
            (0x10c, "bfi", "w10,w9,#4,#8"),
            (0x110, "mov", "x11,#100"),
            (0x114, "mov", "w12,#-2"),
            (0x118, "mov", "w13,#3"),
            (0x11c, "smaddl", "x14,w12,w13,x11"),
            (0x120, "msub", "x15,x13,x13,x11"),
            (0x124, "ret", ""),
        ]);
        let data = ReadOnlyData::default();
        let mut machine = Machine::new(&code, &data);
        machine
            .run(0x100, &mut |_, _| Ok(Call::Return(None)))
            .unwrap();

        assert_eq!(machine.register(9), Some(0x12));
        assert_eq!(machine.register(10), Some(0xf12f));
        assert_eq!(machine.register(14), Some(94));
        assert_eq!(machine.register(15), Some(91));
    }

    #[test]
    fn a_trap_ends_a_path_but_not_a_single_run() {
        let code = rows(&[
            (0x100, "mov", "w9,#6"),
            (0x104, "mul", "w0,w9,w9"),
            (0x108, "cbz", "x3,#0x110"),
            (0x10c, "ret", ""),
            (0x110, "brk", "#0x1"),
        ]);
        let data = ReadOnlyData::default();
        let paths = Machine::new(&code, &data).run_paths(0x100, &mut |_, _| Ok(Call::Return(None)));
        let mut ends: Vec<_> = paths
            .iter()
            .map(|path| (path.end, path.machine.register(0)))
            .collect();
        ends.sort_by_key(|(end, _)| format!("{end:?}"));

        assert_eq!(
            ends,
            [
                (Ok(Exit::Returned), Some(36)),
                (Ok(Exit::Trapped), Some(36))
            ]
        );
        let trap = rows(&[(0x100, "brk", "#0x1")]);
        assert_eq!(returned(&trap, &data, 0), Err(Unresolved("instruction")));
    }

    #[test]
    fn paths_keep_known_branches_on_one_side() {
        let code = rows(&[
            (0x100, "cbz", "x0,#0x10c"),
            (0x104, "mov", "x0,#1"),
            (0x108, "ret", ""),
            (0x10c, "mov", "x0,#2"),
            (0x110, "ret", ""),
        ]);
        let data = ReadOnlyData::default();
        let mut machine = Machine::new(&code, &data);
        machine.set_register(0, 0);
        let paths = machine.run_paths(0x100, &mut |_, _| Ok(Call::Return(None)));

        assert_eq!(returned_values(&paths), [Some(2)]);
    }

    #[test]
    fn paths_stop_at_the_path_limit() {
        let code = rows(&[(0x100, "cbz", "x3,#0x100"), (0x104, "b", "#0x100")]);
        let data = ReadOnlyData::default();
        let paths = Machine::new(&code, &data).run_paths(0x100, &mut |_, _| Ok(Call::Return(None)));

        assert!(paths.len() <= PATH_LIMIT);
        assert!(
            paths
                .iter()
                .any(|path| path.end == Err(Unresolved("path-limit")))
        );
    }

    #[test]
    fn a_jump_outside_the_code_is_a_tail_call_for_paths_only() {
        let code = rows(&[(0x100, "b", "#0x900")]);
        let data = ReadOnlyData::default();

        let stopped = Machine::new(&code, &data).run_paths(0x100, &mut |_, _| Ok(Call::Stop));
        assert_eq!(stopped.len(), 1);
        assert_eq!(stopped[0].end, Ok(Exit::Stopped(0x900)));

        let returned =
            Machine::new(&code, &data).run_paths(0x100, &mut |_, _| Ok(Call::Return(Some(5))));
        assert_eq!(returned_values(&returned), [Some(5)]);

        let refused = Machine::new(&code, &data)
            .run_paths(0x100, &mut |_, _| Err(Unresolved("unknown-callee")));
        assert_eq!(refused[0].end, Err(Unresolved("unknown-callee")));

        assert_eq!(
            Machine::new(&code, &data).run(0x100, &mut |_, _| Ok(Call::Stop)),
            Err(Unresolved("outside-code"))
        );
    }

    #[test]
    fn paths_give_an_indirect_call_to_the_caller_with_its_target_when_known() {
        let code = rows(&[
            (0x100, "blr", "x8"),
            (0x104, "adrp", "x9,#0x900"),
            (0x108, "blr", "x9"),
            (0x10c, "ret", ""),
        ]);
        let data = ReadOnlyData::default();
        let mut targets = Vec::new();
        let paths = Machine::new(&code, &data).run_paths(0x100, &mut |target, _| {
            targets.push(target);
            Ok(Call::Return(Some(7)))
        });

        assert_eq!(targets, [None, Some(0x900)]);
        assert_eq!(returned_values(&paths), [Some(7)]);
        assert_eq!(
            returned(&code, &data, 0),
            Err(Unresolved("instruction")),
            "a single run still refuses an indirect call"
        );
    }

    #[test]
    fn a_single_run_still_refuses_an_unknown_condition() {
        let code = rows(&[(0x100, "b.eq", "#0x100")]);
        let data = ReadOnlyData::default();

        assert_eq!(returned(&code, &data, 0), Err(Unresolved("flags")));
    }

    #[test]
    fn conditional_compare_uses_fallback_flags() {
        let code = rows(&[
            (0x100, "cmp", "w0,#1"),
            (0x104, "ccmp", "w0,#2,#4,ne"),
            (0x108, "cset", "w0,eq"),
            (0x10c, "ret", ""),
        ]);
        let data = ReadOnlyData::default();
        assert_eq!(returned(&code, &data, 1), Ok(Some(1)));
        assert_eq!(returned(&code, &data, 2), Ok(Some(1)));
        assert_eq!(returned(&code, &data, 3), Ok(Some(0)));
    }
}
