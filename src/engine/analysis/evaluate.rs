//! Concrete evaluation of compiled switch code for one known input.
//!
//! The engine keeps several language tables as compiled `switch` statements over one integer:
//! which tokens are scope links, the scopes that a link supports, the output scope of a link, the
//! scope type of a token, and the name of a modifier category. This module runs such code for one
//! concrete input and reports how the run ended and what it computed.
//!
//! It is not a general emulator. [`Machine::run`] follows one path with known register and memory
//! values. A branch on an unknown value, an unsupported instruction, a jump outside the decoded
//! code, or the step bound ends the run as [`Unresolved`]; nothing is guessed. Its [`Stop`](super::stop::Stop) names
//! the instruction, the function the run entered last, and the unknown value or spent bound. A
//! store to an unknown address makes every written byte unknown, because it may overwrite any of
//! them.
//! The caller decides what each call returns, stops the run there, or enters the callee: the path
//! then runs the callee's code and returns to the instruction after the call.
//!
//! [`Machine::run_paths`] is for code that checks run-time state before its answer, such as a
//! localization promotion that tests a database pointer. It follows both sides of a branch on an
//! unknown value and reports every path's end, so the caller can require that the paths agree.
//! It still chooses no path.
//!
//! [`Machine::run_paths_to`] follows only the paths that can still arrive at one instruction, and
//! ends each path when it arrives there, before that instruction runs. A caller reads the
//! arguments of a call site this way. The caller can protect memory ranges from a store to an
//! unknown address, and keep its own facts about each path in labels that follow that path.
//!
//! While [`trace_causes`] is on, a machine also records where each unknown value stopped being
//! known on its path, and a stop at an unknown value carries that trace. See the `trace` module.
use std::collections::{BTreeMap, BTreeSet};

use super::InputError;
use super::decode::{Instruction, decode_arm64, general_register};
use super::stop::{Bound, CauseKind, Obstacle, Unknown, Unresolved};

mod owner;
mod provenance;
mod trace;
use owner::OwnerTaint;
pub use owner::ReturnTaint;
pub use provenance::Decision;
use provenance::Provenance;

use trace::Traces;
pub use trace::trace_causes;

/// The most instructions that one run may execute.
const STEP_LIMIT: usize = 20_000;

/// The most paths that one [`Machine::run_paths`] follows, unless the caller sets another limit
/// with [`Machine::set_path_limit`].
pub const PATH_LIMIT: usize = 64;

/// The most times that one path of [`Machine::run_paths_to`] arrives at one loop head.
pub const LOOP_LIMIT: u32 = 4;

/// The most times that the joined facts at one loop head of [`Machine::run_paths_joining`]
/// become fewer.
const JOIN_LIMIT: u32 = 16;

/// Every NZCV state.
const ALL_FLAG_STATES: u16 = u16::MAX;

/// The stack pointer at entry. It is outside every mapped section, so stack loads never read
/// executable data.
const STACK_TOP: u64 = 0x7fff_0000_0000;

/// How far below the present stack pointer the stack moves when code sets it to an unknown
/// value, such as after a dynamic stack allocation. The new stack is memory that no path has
/// written, so a load from it is unknown until the code stores there, and addresses taken before
/// the move keep pointing at the old frame.
const DYNAMIC_STACK: u64 = 0x10_0000;

/// First address of scratch objects that a caller allocates.
const OBJECT_BASE: u64 = 0x7ffe_0000_0000;

/// First address of objects that a caller places in read-only data, such as the object that an
/// instance pointer holds. It is outside every mapped section, below the scratch objects and the
/// stack, so no run allocates there or reads it as stack.
pub const DATA_OBJECT_BASE: u64 = 0x7ffd_0000_0000;

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

    /// These bytes with each 8-byte little-endian word of `words` at its address, such as the
    /// targets of an executable's pointer slots. A word replaces the bytes that it covers in a
    /// section; consecutive words outside every section become sections of their own. A word
    /// that crosses a section boundary is left out.
    pub fn with_words(&self, words: &BTreeMap<u64, u64>) -> Self {
        self.with_words_of(8, words)
    }

    /// These bytes with each 32-bit word of `words` at its address, as [`Self::with_words`]
    /// places 8-byte words.
    pub fn with_32_bit_words(&self, words: &BTreeMap<u64, u64>) -> Self {
        self.with_words_of(4, words)
    }

    fn with_words_of(&self, width: u64, words: &BTreeMap<u64, u64>) -> Self {
        let mut sections = self.sections.clone();
        let mut outside = BTreeMap::<u64, Vec<u8>>::new();
        let mut run: Option<(u64, Vec<u8>)> = None;
        for (&address, &word) in words {
            let Some(end) = address.checked_add(width) else {
                continue;
            };
            let bytes_of_word = &word.to_le_bytes()[..width as usize];
            let within = sections
                .range_mut(..=address)
                .next_back()
                .filter(|(start, bytes)| address < **start + bytes.len() as u64);
            if let Some((start, bytes)) = within {
                let offset = (address - start) as usize;
                if let Some(slot) = bytes.get_mut(offset..offset + width as usize) {
                    slot.copy_from_slice(bytes_of_word);
                }
                continue;
            }
            if self.sections.range(address..end).next().is_some() {
                continue;
            }
            match &mut run {
                Some((start, bytes)) if *start + bytes.len() as u64 == address => {
                    bytes.extend_from_slice(bytes_of_word);
                }
                _ => {
                    if let Some((start, bytes)) = run.take() {
                        outside.insert(start, bytes);
                    }
                    run = Some((address, bytes_of_word.to_vec()));
                }
            }
        }
        outside.extend(run);
        sections.extend(outside);

        Self { sections }
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
            rows.extend(
                decode_arm64(bytes, *address).map_err(|error| InputError(error.to_string()))?,
            );
        }
        Ok(Self::from_rows(rows))
    }

    /// Every instruction from which the instruction at `site` can be reached by the direct
    /// control flow of this code, including `site`. A branch through a register may go to any
    /// instruction, such as a jump-table target on a path to `site`, so every such branch, and
    /// every instruction that reaches one, can reach `site`.
    fn reaching(&self, site: u64) -> BTreeSet<u64> {
        let mut predecessors = BTreeMap::<u64, Vec<u64>>::new();
        let mut through_register = Vec::new();
        for (&address, operation) in &self.rows {
            match operation.successors(address) {
                Some(successors) => {
                    for successor in successors {
                        predecessors.entry(successor).or_default().push(address);
                    }
                }
                None => through_register.push(address),
            }
        }

        let mut reaching = BTreeSet::from([site]);
        reaching.extend(&through_register);
        let mut pending: Vec<u64> = reaching.iter().copied().collect();
        while let Some(address) = pending.pop() {
            for &predecessor in predecessors.get(&address).into_iter().flatten() {
                if reaching.insert(predecessor) {
                    pending.push(predecessor);
                }
            }
        }
        reaching
    }

    /// The targets of backward direct branches among `addresses`.
    fn loop_heads(&self, addresses: &BTreeSet<u64>) -> BTreeSet<u64> {
        addresses
            .iter()
            .filter_map(|&address| {
                let successors = self.rows.get(&address)?.successors(address)?;
                Some(
                    successors
                        .into_iter()
                        .filter(move |&target| target <= address),
                )
            })
            .flatten()
            .collect()
    }

    /// Whether the instruction at `address` is in the prologue of the function at `entry`: every
    /// instruction from the entry up to it can be part of a prologue.
    fn in_prologue(&self, entry: u64, address: u64) -> bool {
        address >= entry
            && self
                .rows
                .range(entry..=address)
                .all(|(_, operation)| operation.is_prologue_row())
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
    /// Run the callee's code on this path, then continue after the call when it returns. The
    /// callee must be in the decoded code.
    Enter,
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
    /// The path arrived at the site given to [`Machine::run_paths_to`]. The site did not run.
    Reached,
    /// The path arrived again at a loop head in a state that its earlier arrival there covers,
    /// so the passes that follow repeat ones that the run already followed. Only
    /// [`Machine::run_paths_joining`] reports it.
    Looped,
}

/// What [`Machine::run_paths`] does at each call: it receives the target, or `None` for a call
/// through a register whose value is unknown.
pub type PathCalls<'c, 'a> =
    dyn FnMut(Option<u64>, &mut Machine<'a>) -> Result<Call, Unresolved> + 'c;

/// How one path of [`Machine::run_paths`] ended, with the machine state at its end.
#[derive(Debug, Clone)]
pub struct Path<'a> {
    pub end: Result<Exit, Unresolved>,
    pub machine: Machine<'a>,
}

/// A path that waits to be walked from `pc`, after `steps` steps.
struct PendingWalk<'a> {
    machine: Machine<'a>,
    pc: u64,
    steps: usize,
    /// The walk runs again the instruction that split on unknown flags, with the flag states of
    /// one side. The path has already joined at that instruction if it is a loop head.
    resumes_flag_split: bool,
}

enum Walk {
    End(Result<Exit, Unresolved>),
    /// The path can no longer arrive at the site of [`Machine::run_paths_to`].
    Leaves,
    /// Continue each branch at its address, with the flag states that remain possible on it
    /// when the decision was on unknown flags.
    Fork {
        /// The instruction that decided on the unknown value.
        at: u64,
        branches: Vec<(Option<u16>, u64)>,
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
    /// While `flags` is unknown, the flag states that this path has not excluded: bit `n` is
    /// the state `Flags::from_bits(n)`.
    possible_flags: u16,
    memory: BTreeMap<u64, Option<u8>>,
    next_object: u64,
    /// Ranges `(start, end)` that a store to an unknown address leaves known.
    protected: Vec<(u64, u64)>,
    /// The 8-byte stack words that hold a register save: see [`Machine::is_register_save`].
    register_saves: BTreeSet<u64>,
    read_watch: Option<ReadWatch>,
    provenance: Option<Provenance>,
    returned_values: BTreeMap<u64, Option<u64>>,
    tail_entries: BTreeSet<u64>,
    tail_aliases: Vec<(usize, u64)>,
    /// Facts that the caller keeps about this path.
    labels: BTreeMap<u64, u64>,
    /// Known values that this path stored to an unknown address.
    unknown_stores: Vec<u64>,
    /// Which values may point into a fresh owner, while the machine tracks one.
    owner: Option<Box<OwnerTaint>>,
    /// How often this path arrived at each loop head of a [`Machine::run_paths_to`] run, or at a
    /// loop head of a [`Machine::run_paths_joining`] run without joining the state there.
    loop_visits: BTreeMap<u64, u32>,
    /// The return address of each entered call, innermost last.
    frames: Vec<u64>,
    /// The entry of the present run.
    entry: u64,
    /// The most paths that one run of several paths follows.
    path_limit: usize,
    /// The instruction that the run is at, or the last one it ran.
    pc: u64,
    /// The target of each entered call, beside `frames`.
    callees: Vec<u64>,
    /// The stack pointer at each entered call, beside `frames`: the top of the callee's frame.
    frame_tops: Vec<u64>,
    /// The stack pointer where the present run started: the top of its entry's frame.
    run_top: u64,
    /// Where each unknown value stopped being known, while cause tracing is on.
    traces: Option<Box<Traces>>,
}

/// Reads of initial object state, excluding bytes stored during this chain.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ReadWatch {
    start: u64,
    end: u64,
    written: BTreeSet<u64>,
    accesses: Option<BTreeSet<(u64, u64)>>,
    reads: BTreeMap<(u64, u64), Option<u64>>,
}

/// Where the paths of a run end, besides a return or a stop.
enum Ends {
    /// [`Machine::run_paths`]: nowhere else.
    Unbounded,
    /// [`Machine::run_paths_to`].
    Site(Site),
    /// [`Machine::run_paths_joining`]: at an arrival at one of these loop heads that an earlier
    /// arrival covers.
    Joining(BTreeSet<u64>),
}

/// The facts that every path joined at a loop head knew there, with the flag states that any
/// of them allowed, and how often a path made them fewer.
#[derive(Debug, Clone)]
struct HeadState {
    registers: [Option<u64>; 31],
    vectors: [Option<u128>; 32],
    flags: Option<Flags>,
    possible_flags: u16,
    memory: BTreeMap<u64, Option<u8>>,
    register_saves: BTreeSet<u64>,
    stack_pointer: u64,
    frames: Vec<u64>,
    labels: BTreeMap<u64, u64>,
    read_watch: Option<ReadWatch>,
    provenance: Option<Provenance>,
    returned_values: BTreeMap<u64, Option<u64>>,
    tail_aliases: Vec<(usize, u64)>,
    owner: Option<Box<OwnerTaint>>,
    widened: u32,
    traces: Option<Box<Traces>>,
}

/// The instruction that [`Machine::run_paths_to`] must arrive at, every instruction from which
/// it can, and the loop heads among them: the targets of backward branches. The site is in the
/// function that the run started in, at `depth` entered calls; inside a call that the run
/// entered, a path runs on until it returns there.
struct Site {
    address: u64,
    reaching: BTreeSet<u64>,
    loop_heads: BTreeSet<u64>,
    depth: usize,
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
            possible_flags: ALL_FLAG_STATES,
            memory: BTreeMap::new(),
            next_object: OBJECT_BASE,
            protected: Vec::new(),
            register_saves: BTreeSet::new(),
            read_watch: None,
            provenance: None,
            returned_values: BTreeMap::new(),
            tail_entries: BTreeSet::new(),
            tail_aliases: Vec::new(),
            labels: BTreeMap::new(),
            unknown_stores: Vec::new(),
            owner: None,
            loop_visits: BTreeMap::new(),
            path_limit: PATH_LIMIT,
            frames: Vec::new(),
            entry: 0,
            pc: 0,
            callees: Vec::new(),
            frame_tops: Vec::new(),
            run_top: STACK_TOP,
            traces: Traces::when_tracing(),
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

    /// Reserve `length` bytes of scratch memory that no path has written, and return their
    /// address. A load from them is unknown until the code stores there.
    pub fn reserve(&mut self, length: u64) -> u64 {
        let address = self.next_object;
        self.next_object += length.next_multiple_of(16) + 16;
        address
    }

    /// Set general register `index` (`x0` is 0).
    pub fn set_register(&mut self, index: usize, value: u64) {
        self.registers[index] = Some(value);
        if let Some(provenance) = &mut self.provenance {
            provenance.registers[index].clear();
        }
    }

    /// The value of general register `index`, when it is known.
    pub fn register(&self, index: usize) -> Option<u64> {
        self.registers[index]
    }

    /// Last return value of an entered function, even if its caller later overwrites `x0`.
    pub fn returned_value(&self, function: u64) -> Option<Option<u64>> {
        self.returned_values.get(&function).copied()
    }

    /// Treat a branch into one of these complete function entries as a tail call, even when
    /// its instructions are decoded. Entered and tail-called functions retain their results.
    pub fn intercept_tail_calls(&mut self, entries: BTreeSet<u64>) {
        self.tail_entries = entries;
    }

    /// Whether the current call transfers control without a continuation in its caller.
    pub fn is_tail_call(&self) -> bool {
        matches!(self.code.rows.get(&self.pc), Some(Operation::Parsed { mnemonic, .. }) if matches!(mnemonic.as_str(), "b" | "br"))
    }

    /// The value of general register `index`; when it is unknown, a stop for `reason` at the
    /// instruction that the run is at, such as a call that a `calls` closure reads, or the last
    /// one it ran.
    pub fn known_register(&self, index: usize, reason: &'static str) -> Result<u64, Unresolved> {
        self.registers[index].ok_or_else(|| {
            let unknown = Obstacle::Unknown(Unknown::Register(index as u8));
            self.stop(self.pc, reason, unknown)
        })
    }

    /// The present stack pointer.
    pub fn stack_pointer(&self) -> u64 {
        self.stack_pointer
    }

    /// Whether `address` is in the stack, above every scratch object.
    pub fn is_stack(&self, address: u64) -> bool {
        (self.next_object..STACK_TOP).contains(&address)
    }

    /// The top of the stack frame that holds the stack address `address`: the stack pointer at
    /// the innermost entered call, or at the start of the run, above it, or the top of the stack.
    pub fn frame_top(&self, address: u64) -> u64 {
        self.frame_tops
            .iter()
            .chain([&self.run_top])
            .copied()
            .filter(|top| *top > address)
            .min()
            .unwrap_or(STACK_TOP)
    }

    /// A fresh call frame at the caller's stack position, with the call's arguments, the
    /// caller's established memory state and, while the caller tracks an owner, the owner-derived
    /// values that the callee can read. While tracing causes, the callee starts with the causes
    /// of the caller's unknown memory and arguments.
    pub fn fresh_callee(&self, code: &'a Code, data: &'a ReadOnlyData) -> Self {
        let mut callee = Self::new(code, data);
        callee.set_stack_pointer(self.stack_pointer);
        // Absence already means unknown off-image, or the immutable backing byte on-image.
        callee.memory = self
            .memory
            .iter()
            .filter(|(address, byte)| data.byte(**address) != **byte)
            .map(|(&address, &byte)| (address, byte))
            .collect();
        for index in 0..9 {
            if let Some(value) = self.registers[index] {
                callee.set_register(index, value);
            }
        }
        callee.vectors[..8].copy_from_slice(&self.vectors[..8]);
        callee.traces = self.traces.as_ref().map(|traces| traces.for_callee());
        self.copy_owner_taint_to_callee(&mut callee);

        callee
    }

    /// Follow at most `limit` paths in a run of several paths, in place of [`PATH_LIMIT`].
    pub fn set_path_limit(&mut self, limit: usize) {
        self.path_limit = limit;
    }

    /// Set the stack position for an authored caller or a separately evaluated call frame.
    pub fn set_stack_pointer(&mut self, value: u64) {
        self.stack_pointer = value;
    }

    /// Invalidate every tracked byte after a call whose stores may escape its receiver.
    pub fn forget_memory(&mut self) {
        for byte in self.memory.values_mut() {
            *byte = None;
        }
    }

    /// Store `value` little-endian in `width` bytes.
    pub fn write(&mut self, address: u64, width: u64, value: u64) {
        self.note_store(address, width, false);
        for offset in 0..width {
            self.memory
                .insert(address + offset, Some((value >> (offset * 8)) as u8));
        }
        self.record_stored_inputs(address, width, true);
    }

    /// Copy a checked byte span, taking a snapshot so overlapping moves preserve their source.
    /// Callers bound the length and check both end addresses before calling.
    pub(crate) fn copy_bytes(&mut self, destination: u64, source: u64, length: u64) {
        let traces = self.unknown_byte_traces(source, length);
        let bytes: Vec<_> = (source..source + length)
            .map(|at| (self.byte(at), self.loaded_owner(false, Some(at), 1)))
            .collect();
        for (offset, (byte, derived)) in bytes.into_iter().enumerate() {
            let at = destination + offset as u64;
            self.note_store(at, 1, derived);
            self.memory.insert(at, byte);
            if byte.is_some() {
                self.record_stored_inputs(at, 1, true);
            }
        }
        self.set_byte_traces(destination, &traces);
    }

    /// Keep `length` bytes at `address` known when this path stores to an unknown address. The
    /// caller protects only memory that it has shown no unknown address can reach.
    pub fn protect(&mut self, address: u64, length: u64) {
        self.protected.push((address, address + length));
    }

    /// Track instruction loads of bytes not yet stored by this chain. Install initial state
    /// before starting the watch; keep the watch when moving to the next stage.
    pub fn watch_reads(&mut self, address: u64, length: u64) {
        self.provenance = Some(Provenance::default());
        self.read_watch = Some(ReadWatch {
            start: address,
            end: address + length,
            written: BTreeSet::new(),
            accesses: None,
            reads: BTreeMap::new(),
        });
    }

    /// Initial-state reads, by object-relative offset and width, with their initial values.
    pub fn receiver_reads(&self) -> BTreeMap<(u64, u64), Option<u64>> {
        self.read_watch
            .as_ref()
            .map(|watch| watch.reads.clone())
            .unwrap_or_default()
    }

    /// Include instruction loads of script-written bytes in the current watch.
    pub fn watch_accesses(&mut self) {
        if let Some(watch) = &mut self.read_watch {
            watch.accesses = Some(BTreeSet::new());
        }
    }

    /// Whether an instruction loaded any byte in this object-relative range, including script stores.
    pub fn accessed(&self, offset: u64, width: u64) -> bool {
        self.read_watch
            .as_ref()
            .is_some_and(|watch| self.accessed_address(watch.start + offset, width))
    }

    /// Whether the active access watch saw a load in this absolute range.
    pub fn accessed_address(&self, address: u64, width: u64) -> bool {
        self.read_watch.as_ref().is_some_and(|watch| {
            watch.accesses.iter().flatten().any(|&(at, size)| {
                at < address.saturating_add(width) && at.saturating_add(size) > address
            })
        })
    }

    /// Store bytes from classified script input whose values are unknown.
    pub fn write_unknown(&mut self, address: u64, width: u64) {
        self.note_store(address, width, false);
        self.forget(address, width);
        if let Some(provenance) = &mut self.provenance {
            for at in address..address + width {
                provenance.memory.remove(&at);
            }
        }
    }

    /// Each known byte in a reserved object, relative to its start.
    pub fn known_bytes(&self, address: u64, length: u64) -> BTreeMap<u64, u8> {
        self.memory
            .range(address..address + length)
            .filter_map(|(&at, &value)| Some((at - address, value?)))
            .collect()
    }

    /// A definite store of `width` bytes at `address`, of an owner-derived value or not.
    fn note_store(&mut self, address: u64, width: u64, derived: bool) {
        self.store_owner_bytes(address, width, derived);
        let end = address.saturating_add(width);
        self.register_saves
            .retain(|word| *word >= end || word.saturating_add(8) <= address);

        if let Some(watch) = &mut self.read_watch {
            watch.written.extend(
                (address..address.saturating_add(width))
                    .filter(|at| (watch.start..watch.end).contains(at)),
            );
        }
    }

    fn load_bytes(&mut self, address: u64, width: u64) -> Option<u128> {
        let value = self.read_bytes(address, width);
        if let Some(watch) = &mut self.read_watch
            && let Some(accesses) = &mut watch.accesses
        {
            accesses.insert((address, width));
        }
        if value.is_none()
            && let Some(provenance) = &self.provenance
        {
            provenance
                .inputs
                .borrow_mut()
                .extend(self.receiver_sources(address, width));
        }
        let Some(watch) = &self.read_watch else {
            return value;
        };
        let mut initial = Vec::new();
        let end = address.saturating_add(width).min(watch.end);
        let mut at = address.max(watch.start);
        while at < end {
            if watch.written.contains(&at) {
                at += 1;
                continue;
            }
            let start = at;
            while at < end && at - start < 8 && !watch.written.contains(&at) {
                at += 1;
            }
            initial.push((
                (start - watch.start, at - start),
                self.read(start, at - start),
            ));
        }
        self.read_watch.as_mut().unwrap().reads.extend(initial);
        value
    }

    fn load(&mut self, address: u64, width: u64) -> Option<u64> {
        self.load_bytes(address, width).map(|value| value as u64)
    }

    /// Stop protecting the range that starts at `address`.
    pub fn release(&mut self, address: u64) {
        self.protected.retain(|(start, _)| *start != address);
    }

    /// Record the caller's fact `value` under `key` for this path.
    pub fn label(&mut self, key: u64, value: u64) {
        self.labels.insert(key, value);
    }

    /// The caller's fact under `key` on this path.
    pub fn labelled(&self, key: u64) -> Option<u64> {
        self.labels.get(&key).copied()
    }

    /// Remove the caller's fact under `key` on this path.
    pub fn unlabel(&mut self, key: u64) {
        self.labels.remove(&key);
    }

    /// Every fact that the caller keeps on this path.
    pub fn labels(&self) -> &BTreeMap<u64, u64> {
        &self.labels
    }

    /// Known values that this path stored to an unknown address.
    pub fn unknown_stores(&self) -> &[u64] {
        &self.unknown_stores
    }

    /// The instruction that the run is at. During a call of [`PathCalls`], the call instruction.
    pub fn pc(&self) -> u64 {
        self.pc
    }

    /// This machine's state outside every call that its path entered, so that a run from it
    /// returns where its own entry returns.
    pub fn without_entered_calls(&self) -> Self {
        let mut machine = self.clone();
        machine.frames.clear();
        machine.callees.clear();
        machine.frame_tops.clear();
        machine.tail_aliases.clear();
        machine
    }

    /// The address of each entered call that this path is inside, outermost first.
    pub fn entered_calls(&self) -> impl Iterator<Item = u64> + '_ {
        self.frames.iter().map(|address| address - 4)
    }

    /// Every known eight-byte value at an eight-byte aligned address, by address.
    pub fn known_words(&self) -> Vec<(u64, u64)> {
        self.memory
            .keys()
            .filter(|address| address.is_multiple_of(8))
            .filter_map(|&address| Some((address, self.read(address, 8)?)))
            .collect()
    }

    /// The 4-byte-aligned words in `start..end` that this path wrote or invalidated.
    pub fn written_words(&self, start: u64, end: u64) -> BTreeSet<u64> {
        self.memory
            .range(start..end)
            .map(|(&address, _)| address & !3)
            .collect()
    }

    /// Whether the 8-byte word at `address` holds a register save: a store of a callee-saved
    /// register (`x19` to `x30`) to a stack address in the prologue of the function that the
    /// path is in, which no later store has overwritten. A callee that receives a lower stack
    /// address does not reach a register save.
    pub fn is_register_save(&self, address: u64) -> bool {
        self.register_saves.contains(&address)
    }

    /// Whether this path wrote or explicitly invalidated any byte of reserved memory.
    /// A reservation starts without entries; even an unknown stored byte counts as a write.
    pub fn has_written(&self, address: u64, length: u64) -> bool {
        self.memory
            .range(address..address.saturating_add(length))
            .next()
            .is_some()
    }

    /// Make each known byte of `length` bytes at `address` unknown. Scratch objects have no
    /// read-only backing, so a byte without an entry is already unknown; filling the rest of a
    /// large object with unknown entries would only enlarge path clones.
    pub fn forget_known_bytes(&mut self, address: u64, length: u64) {
        self.forget_known_bytes_for(address, length, CauseKind::Invalidated);
    }

    /// [`Machine::forget_known_bytes`], for the reason `kind` while tracing causes.
    pub(super) fn forget_known_bytes_for(&mut self, address: u64, length: u64, kind: CauseKind) {
        self.trace_forgetting_unknown(address, length, kind);
        let known: Vec<u64> = self
            .memory
            .range(address..address.saturating_add(length))
            .filter(|(_, byte)| byte.is_some())
            .map(|(&at, _)| at)
            .collect();
        for at in known {
            self.forget_for(at, 1, kind);
        }
    }

    /// Make `width` bytes at `address` unknown, such as a field that a call may have written.
    pub fn forget(&mut self, address: u64, width: u64) {
        self.forget_for(address, width, CauseKind::Invalidated);
    }

    /// [`Machine::forget`], for the reason `kind` while tracing causes.
    pub(super) fn forget_for(&mut self, address: u64, width: u64, kind: CauseKind) {
        self.trace_forgetting(address, width, kind);
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
            value |= u128::from(self.byte(address)?) << (offset * 8);
        }
        Some(value)
    }

    /// Execute from `entry` until the code returns or `calls` stops it.
    pub fn run(
        &mut self,
        entry: u64,
        calls: &mut dyn FnMut(u64, &mut Machine<'a>) -> Result<Call, Unresolved>,
    ) -> Result<Exit, Unresolved> {
        self.entry = entry;
        let mut pc = entry;
        for _ in 0..STEP_LIMIT {
            self.pc = pc;
            let code = self.code;
            let operation = code
                .rows
                .get(&pc)
                .ok_or_else(|| self.stop(pc, "outside-code", Obstacle::OutsideCode))?;
            let flow = self
                .step(pc, operation)
                .map_err(|halt| self.stop(pc, halt.reason, halt.obstacle))?;
            match flow {
                Flow::Next => pc += 4,
                Flow::Jump(target) => pc = target,
                Flow::Unknown { halt, .. } => {
                    return Err(self.stop(pc, halt.reason, halt.obstacle));
                }
                Flow::IndirectCall(_) | Flow::Trap => {
                    return Err(self.stop(pc, "instruction", Obstacle::Unsupported));
                }
                Flow::Call(target) => match self.hand_over(pc, |machine| calls(target, machine))? {
                    Call::Return(value) => {
                        self.returned_from_call(value);
                        pc += 4;
                    }
                    Call::Stop => return Ok(Exit::Stopped(target)),
                    Call::Enter => pc = self.enter(pc, target),
                },
                Flow::Return => match self.leave() {
                    Some(caller) => pc = caller,
                    None => return Ok(Exit::Returned),
                },
            }
        }
        Err(self.stop(pc, "step-limit", Obstacle::Bound(Bound::Steps(STEP_LIMIT))))
    }

    /// Execute from `entry` along every path, and report how each path ended.
    ///
    /// Unlike [`Machine::run`], a branch or conditional select on an unknown value continues on
    /// both sides, and a `b` to an address outside the decoded code is a tail call that goes to
    /// `calls`: when it returns, the path returns. A decision on unknown flags splits the flag
    /// states that remain possible into those where the condition holds and those where it
    /// fails, so a later decision on the same flags follows every state that is still possible.
    ///
    /// `calls` receives the target of each call and tail call; see [`PathCalls`]. At most [`PATH_LIMIT`] paths are followed; a path that would
    /// exceed the limit ends as `path-limit`. Each path has its own step limit.
    pub fn run_paths(self, entry: u64, calls: &mut PathCalls<'_, 'a>) -> Vec<Path<'a>> {
        self.follow(entry, &Ends::Unbounded, calls)
    }

    /// Execute from `entry` along every path, as [`Machine::run_paths`] does, and join the
    /// states of the paths at each loop head.
    ///
    /// A loop head is a target of a backward branch of the decoded code. The run keeps, for each
    /// loop head, the facts that every path that arrived there knew with the same values:
    /// registers, flags and memory. A path that arrives in a state that knows each of them ends
    /// as [`Exit::Looped`], since the run already follows a path from a state that covers its
    /// own. Otherwise the kept facts become fewer, and the path goes on from the kept facts alone,
    /// which cover every path joined there. A path joins only paths with the same stack, entered
    /// calls and labels; the caller's labels are facts that the run cannot make fewer. A path
    /// that goes on without a join more than [`LOOP_LIMIT`] times at one head, or a loop head
    /// whose facts become fewer more than [`JOIN_LIMIT`] times, ends as `Unresolved`. Paths that
    /// end as [`Exit::Looped`] do not count toward [`PATH_LIMIT`].
    pub fn run_paths_joining(self, entry: u64, calls: &mut PathCalls<'_, 'a>) -> Vec<Path<'a>> {
        let addresses = self.code.rows.keys().copied().collect();
        let heads = self.code.loop_heads(&addresses);
        self.follow(entry, &Ends::Joining(heads), calls)
    }

    /// Execute from `entry` along every path that can arrive at the instruction at `site`, as
    /// [`Machine::run_paths`] does.
    ///
    /// A path ends as [`Exit::Reached`] when it arrives at `site`, before `site` runs, so the
    /// registers hold the arguments of a call there. A path that can no longer arrive, by the
    /// direct branches of the decoded code, is dropped and counts toward no limit. A branch
    /// through a register may go anywhere, so every instruction before one can arrive. Inside a
    /// call that the run enters, a path is neither dropped nor at the site until it returns.
    pub fn run_paths_to(
        self,
        entry: u64,
        site: u64,
        calls: &mut PathCalls<'_, 'a>,
    ) -> Vec<Path<'a>> {
        let reaching = self.code.reaching(site);
        let site = Site {
            address: site,
            loop_heads: self.code.loop_heads(&reaching),
            reaching,
            depth: self.frames.len(),
        };
        self.follow(entry, &Ends::Site(site), calls)
    }

    fn follow(mut self, entry: u64, ends: &Ends, calls: &mut PathCalls<'_, 'a>) -> Vec<Path<'a>> {
        self.entry = entry;
        self.run_top = self.stack_pointer;
        let mut pending = vec![PendingWalk {
            machine: self,
            pc: entry,
            steps: 0,
            resumes_flag_split: false,
        }];
        let mut ended = Vec::new();
        let mut joined = BTreeMap::new();
        // Paths that a joined state covers end at once and add no work, so they do not count.
        let mut covered = 0;

        while let Some(PendingWalk {
            mut machine,
            pc,
            steps,
            resumes_flag_split,
        }) = pending.pop()
        {
            match machine.walk(pc, steps, resumes_flag_split, ends, &mut joined, calls) {
                Walk::End(end) => {
                    covered += usize::from(end == Ok(Exit::Looped));
                    ended.push(Path { end, machine });
                }
                Walk::Leaves => {}
                Walk::Fork {
                    at,
                    branches,
                    steps,
                } => {
                    let path_limit = machine.path_limit;
                    if ended.len() - covered + pending.len() + branches.len() > path_limit {
                        let limit = Obstacle::Bound(Bound::Paths(path_limit));
                        ended.push(Path {
                            end: Err(machine.stop(at, "path-limit", limit)),
                            machine,
                        });
                        continue;
                    }

                    for (side, (states, pc)) in branches.into_iter().enumerate().rev() {
                        let mut branch = machine.clone();
                        branch.record_decision(at, side, states.is_some());
                        if let Some(states) = states {
                            branch.possible_flags = states;
                        }
                        pending.push(PendingWalk {
                            machine: branch,
                            pc,
                            steps,
                            resumes_flag_split: states.is_some(),
                        });
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
        mut resumes_flag_split: bool,
        ends: &Ends,
        joined: &mut BTreeMap<u64, HeadState>,
        calls: &mut PathCalls<'_, 'a>,
    ) -> Walk {
        while steps < STEP_LIMIT {
            self.pc = pc;
            match ends {
                Ends::Unbounded => {}
                Ends::Site(site) if self.frames.len() == site.depth => {
                    if pc == site.address {
                        return Walk::End(Ok(Exit::Reached));
                    }
                    if !site.reaching.contains(&pc) {
                        return Walk::Leaves;
                    }
                    if site.loop_heads.contains(&pc) && self.record_loop_arrival(pc) > LOOP_LIMIT {
                        let limit = Obstacle::Bound(Bound::LoopArrivals(LOOP_LIMIT));
                        return Walk::End(Err(self.stop(pc, "loop-limit", limit)));
                    }
                }
                Ends::Site(_) => {}
                // A walk that resumes a split on flags has joined at its instruction already.
                Ends::Joining(heads) if heads.contains(&pc) && !resumes_flag_split => {
                    match self.join(pc, joined) {
                        Ok(true) => {}
                        Ok(false) => return Walk::End(Ok(Exit::Looped)),
                        Err(unresolved) => return Walk::End(Err(unresolved)),
                    }
                }
                Ends::Joining(_) => {}
            }
            resumes_flag_split = false;

            steps += 1;
            let code = self.code;
            let Some(operation) = code.rows.get(&pc) else {
                return Walk::End(Err(self.stop(pc, "outside-code", Obstacle::OutsideCode)));
            };
            let flow = match self.step(pc, operation) {
                Ok(flow) => flow,
                Err(halt)
                    if halt == Halt::unknown_flags()
                        && let Some(condition) = operation.condition() =>
                {
                    let (holding, failing) = condition.split(self.possible_flags);
                    return Walk::Fork {
                        at: pc,
                        branches: vec![(Some(holding), pc), (Some(failing), pc)],
                        steps: steps - 1,
                    };
                }
                Err(halt) => return Walk::End(Err(self.stop(pc, halt.reason, halt.obstacle))),
            };

            match flow {
                Flow::Next => pc += 4,
                Flow::Jump(target)
                    if !code.rows.contains_key(&target)
                        || (self.tail_entries.contains(&target) && target != self.entered()) =>
                {
                    match self.tail_call(pc, target, calls) {
                        Ok(Some(caller)) => pc = caller,
                        Ok(None) => return Walk::End(Ok(Exit::Returned)),
                        Err(end) => return Walk::End(end),
                    }
                }
                Flow::Jump(target) => pc = target,
                Flow::Unknown { target, .. } => {
                    return Walk::Fork {
                        at: pc,
                        branches: vec![(None, target), (None, pc + 4)],
                        steps,
                    };
                }
                Flow::Call(target) => {
                    match self.hand_over(pc, |machine| calls(Some(target), machine)) {
                        Ok(Call::Return(value)) => {
                            self.returned_from_call(value);
                            pc += 4;
                        }
                        Ok(Call::Stop) => return Walk::End(Ok(Exit::Stopped(target))),
                        Ok(Call::Enter) => pc = self.enter(pc, target),
                        Err(unresolved) => return Walk::End(Err(unresolved)),
                    }
                }
                Flow::IndirectCall(target) => {
                    match (self.hand_over(pc, |machine| calls(target, machine)), target) {
                        (Ok(Call::Return(value)), _) => {
                            self.returned_from_call(value);
                            pc += 4;
                        }
                        (Ok(Call::Stop), Some(target)) => {
                            return Walk::End(Ok(Exit::Stopped(target)));
                        }
                        (Ok(Call::Enter), Some(target)) => pc = self.enter(pc, target),
                        (Ok(Call::Stop | Call::Enter), None) => {
                            let unknown = self.stop(pc, "stopped-at-unknown-call", Obstacle::Call);
                            return Walk::End(Err(unknown));
                        }
                        (Err(unresolved), _) => return Walk::End(Err(unresolved)),
                    }
                }
                Flow::Return => match self.leave() {
                    Some(caller) => pc = caller,
                    None => return Walk::End(Ok(Exit::Returned)),
                },
                Flow::Trap => return Walk::End(Ok(Exit::Trapped)),
            }
        }

        let limit = Obstacle::Bound(Bound::Steps(STEP_LIMIT));
        Walk::End(Err(self.stop(pc, "step-limit", limit)))
    }

    /// Join this path's state with the facts kept at the loop head `pc`. Whether the path goes
    /// on: `false` when the kept facts cover its state.
    fn join(&mut self, pc: u64, joined: &mut BTreeMap<u64, HeadState>) -> Result<bool, Unresolved> {
        let Some(kept) = joined.get_mut(&pc) else {
            joined.insert(pc, self.head_state(0));
            return Ok(true);
        };
        if kept.stack_pointer != self.stack_pointer
            || kept.frames != self.frames
            || kept.labels != self.labels
            || kept.read_watch != self.read_watch
            || kept.provenance != self.provenance
            || kept.returned_values != self.returned_values
            || kept.tail_aliases != self.tail_aliases
            || kept.owner.is_some() != self.owner.is_some()
        {
            return match self.record_loop_arrival(pc) {
                visits if visits > LOOP_LIMIT => {
                    let limit = Obstacle::Bound(Bound::LoopArrivals(LOOP_LIMIT));
                    Err(self.stop(pc, "loop-limit", limit))
                }
                _ => Ok(true),
            };
        }

        let arrival = self.arrival();
        let mut lost = false;
        for index in 0..self.registers.len() {
            let before = kept.registers[index];
            lost |= before.is_some() && before != self.registers[index];
            if before != self.registers[index] {
                self.registers[index] = None;
            }
        }
        for index in 0..self.vectors.len() {
            let before = kept.vectors[index];
            lost |= before.is_some() && before != self.vectors[index];
            if before != self.vectors[index] {
                self.vectors[index] = None;
            }
        }
        let kept_states = kept.flags.map_or(kept.possible_flags, Flags::state);
        let states = self.flags.map_or(self.possible_flags, Flags::state);
        lost |= states & !kept_states != 0;
        if kept.flags.is_none() || kept.flags != self.flags {
            self.set_flags(None);
            self.possible_flags = kept_states | states;
        }
        let addresses: BTreeSet<u64> = kept
            .memory
            .keys()
            .chain(self.memory.keys())
            .copied()
            .collect();
        for address in addresses {
            let before = kept
                .memory
                .get(&address)
                .copied()
                .unwrap_or_else(|| self.data.byte(address));
            let now = self.byte(address);
            lost |= before.is_some() && before != now;
            if before != now && now.is_some() {
                self.memory.insert(address, None);
            }
        }
        lost |= !kept.register_saves.is_subset(&self.register_saves);
        self.register_saves
            .retain(|word| kept.register_saves.contains(word));

        lost |= self.widen_owner(kept.owner.as_deref());

        if !lost {
            if let Some(arrival) = &arrival {
                self.trace_covered(kept, arrival);
            }
            return Ok(false);
        }
        if let Some(arrival) = &arrival {
            self.trace_join(kept, arrival);
        }
        let widened = kept.widened + 1;
        if widened > JOIN_LIMIT {
            let limit = Obstacle::Bound(Bound::Joins(JOIN_LIMIT));
            return Err(self.stop(pc, "join-limit", limit));
        }
        *kept = self.head_state(widened);
        Ok(true)
    }

    fn head_state(&self, widened: u32) -> HeadState {
        HeadState {
            registers: self.registers,
            vectors: self.vectors,
            flags: self.flags,
            possible_flags: self.possible_flags,
            memory: self.memory.clone(),
            register_saves: self.register_saves.clone(),
            stack_pointer: self.stack_pointer,
            frames: self.frames.clone(),
            labels: self.labels.clone(),
            read_watch: self.read_watch.clone(),
            provenance: self.provenance.clone(),
            returned_values: self.returned_values.clone(),
            tail_aliases: self.tail_aliases.clone(),
            owner: self.owner.clone(),
            widened,
            traces: self.traces.clone(),
        }
    }

    /// The byte at `address`, when it is known.
    fn byte(&self, address: u64) -> Option<u8> {
        match self.memory.get(&address) {
            Some(byte) => *byte,
            None => self.data.byte(address),
        }
    }

    /// Count an arrival at the loop head `pc`, and return how often the path arrived there.
    fn record_loop_arrival(&mut self, pc: u64) -> u32 {
        let visits = self.loop_visits.entry(pc).or_default();
        *visits += 1;
        *visits
    }

    /// A branch out of the decoded code: the call, then a return from the present function. The
    /// address where the path continues, or `None` when the outermost function returned.
    fn tail_call(
        &mut self,
        pc: u64,
        target: u64,
        calls: &mut PathCalls<'_, 'a>,
    ) -> Result<Option<u64>, Result<Exit, Unresolved>> {
        let call = self.hand_over(pc, |machine| calls(Some(target), machine));
        match call.map_err(Err)? {
            Call::Return(value) => {
                self.returned_from_call(value);
                Ok(self.leave())
            }
            Call::Stop => Err(Ok(Exit::Stopped(target))),
            Call::Enter if self.code.rows.contains_key(&target) => {
                self.tail_aliases.push((self.frames.len(), self.entered()));
                if let Some(callee) = self.callees.last_mut() {
                    *callee = target;
                } else {
                    self.entry = target;
                }
                Ok(Some(target))
            }
            Call::Enter => Err(Err(self.stop(pc, "outside-code", Obstacle::OutsideCode))),
        }
    }

    /// Enter the call at `pc` to `target`, and return where the path continues.
    fn enter(&mut self, pc: u64, target: u64) -> u64 {
        self.frames.push(pc + 4);
        self.callees.push(target);
        self.frame_tops.push(self.stack_pointer);
        target
    }

    /// Return from the innermost entered call, and return where the path continues: `None` when
    /// the outermost function returned.
    fn leave(&mut self) -> Option<u64> {
        if !self.tail_entries.is_empty() {
            let value = self.register(0);
            self.returned_values.insert(self.entered(), value);
            let depth = self.frames.len();
            for &(_, function) in self.tail_aliases.iter().filter(|(at, _)| *at == depth) {
                self.returned_values.insert(function, value);
            }
            self.tail_aliases.retain(|(at, _)| *at != depth);
        }
        self.callees.pop();
        self.frame_tops.pop();
        self.frames.pop()
    }

    /// Where the run last entered code: the innermost entered call, or its entry.
    fn entered(&self) -> u64 {
        self.callees.last().copied().unwrap_or(self.entry)
    }

    /// The run stopped at `pc` for `reason`.
    fn stop(&self, pc: u64, reason: &'static str, obstacle: Obstacle) -> Unresolved {
        let unresolved = Unresolved::at(reason, pc, self.entered(), obstacle);
        self.with_stop_trace(unresolved, obstacle)
    }

    /// Give the call at `pc` to `call`, which may run other code on this machine. The run is
    /// then again at `pc`, in the code it entered. A refusal that `call` did not locate is
    /// located at `pc`; one that it located, such as a stop of its own run, stays.
    fn hand_over(
        &mut self,
        pc: u64,
        call: impl FnOnce(&mut Self) -> Result<Call, Unresolved>,
    ) -> Result<Call, Unresolved> {
        let entry = self.entry;
        let result = call(self);
        self.entry = entry;
        self.pc = pc;

        result.map_err(|unresolved| match unresolved.stop {
            Some(_) => unresolved,
            None => Unresolved {
                trace: unresolved.trace,
                ..self.stop(pc, unresolved.reason, Obstacle::Call)
            },
        })
    }

    /// A called function returned `value`; caller-saved registers and flags are unknown.
    fn returned_from_call(&mut self, value: Option<u64>) {
        let lost = if value.is_some() { 1..=18 } else { 0..=18 };
        self.registers[0] = value;
        self.registers[lost.clone()].fill(None);
        self.set_flags(None);
        if let Some(provenance) = &mut self.provenance {
            for index in 0..=18 {
                provenance.registers[index].clear();
            }
            provenance.flags.clear();
        }
        self.owner_call_returned();
        self.trace_call(lost);
    }

    /// Run the instruction at `pc`.
    fn step(&mut self, pc: u64, operation: &Operation) -> Result<Flow, Halt> {
        let Operation::Parsed { mnemonic, operands } = operation else {
            return Err(Halt::unsupported("instruction"));
        };
        self.clear_inputs();
        let mnemonic = ordered_access(mnemonic);
        let operands = operands.as_slice();
        if self.step_move(mnemonic, operands)?
            || self.step_integer(mnemonic, operands)?
            || self.step_condition(mnemonic, operands)?
            || self.step_vector(mnemonic, operands)?
            || self.step_atomic(mnemonic, operands)?
            || self.step_memory(mnemonic, operands)?
        {
            if !matches!(mnemonic, "str" | "stur" | "stp" | "ldr" | "ldur" | "ldp") {
                let destinations = if mnemonic == "ldp" { 2 } else { 1 };
                for operand in operands.iter().take(destinations) {
                    let Operand::Register(Register {
                        name: Name::Vector(index),
                        ..
                    }) = operand
                    else {
                        continue;
                    };
                    let derived = self.owner.as_ref().is_some_and(|owner| owner.inputs.get());
                    self.set_owner_vector(*index, derived);
                    if let Some(provenance) = &mut self.provenance {
                        provenance.vectors[*index] = if self.vectors[*index].is_none() {
                            provenance.inputs.borrow().clone()
                        } else {
                            BTreeSet::new()
                        };
                    }
                }
            }
            return Ok(Flow::Next);
        }
        self.step_control(pc, mnemonic, operands)
    }

    /// Run a move of a value or an address into a register. `false` when it is not one.
    fn step_move(&mut self, mnemonic: &str, operands: &[Operand]) -> Result<bool, Halt> {
        match (mnemonic, operands) {
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
                    _ => return Err(Halt::unsupported("movk-shift")),
                };
                let mask = 0xffffu64 << shift;
                let value = self
                    .operand(destination)?
                    .map(|prior| (prior & !mask) | (((*part as u64) & 0xffff) << shift));
                self.assign(destination, value)?;
            }
            ("adr" | "adrp", [destination, Operand::Immediate(address)]) => {
                self.assign(destination, Some(*address as u64))?;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// Run integer arithmetic, bit-field or extension. `false` when the instruction is not one.
    fn step_integer(&mut self, mnemonic: &str, operands: &[Operand]) -> Result<bool, Halt> {
        match (mnemonic, operands) {
            (
                "add" | "sub" | "and" | "bic" | "orr" | "eor" | "lsl" | "lsr" | "asr" | "ror"
                | "mul",
                [destination, left, right, rest @ ..],
            ) => {
                let stack_offset = matches!(mnemonic, "add" | "sub")
                    && matches!(
                        left,
                        Operand::Register(Register {
                            name: Name::StackPointer,
                            wide: true,
                            ..
                        })
                    )
                    && matches!(right, Operand::Immediate(_));
                let wide = destination.is_wide();
                let left = self.operand(left)?;
                let right = self.modified(right, rest)?;
                let value = left
                    .zip(right)
                    .map(|(left, right)| binary(mnemonic, left, right, wide));
                if stack_offset {
                    self.assign_stack_offset(destination, value)?;
                } else {
                    self.assign(destination, value)?;
                }
            }
            ("adds" | "subs", [destination, left, right, rest @ ..]) => {
                let wide = destination.is_wide();
                let left = self.operand(left)?;
                let right = self.modified(right, rest)?;
                let (kind, operation) = if mnemonic == "adds" {
                    ("cmn", "add")
                } else {
                    ("cmp", "sub")
                };
                self.set_flags(
                    left.zip(right)
                        .map(|(left, right)| Flags::compare(kind, left, right, wide)),
                );
                let value = left
                    .zip(right)
                    .map(|(left, right)| binary(operation, left, right, wide));
                self.assign(destination, value)?;
            }
            ("ands" | "bics", [destination, left, right, rest @ ..]) => {
                let wide = destination.is_wide();
                let left = self.operand(left)?;
                let right = self.modified(right, rest)?;
                let mask = right.map(|right| if mnemonic == "bics" { !right } else { right });
                self.set_flags(
                    left.zip(mask)
                        .map(|(left, mask)| Flags::compare("tst", left, mask, wide)),
                );
                self.assign(destination, left.zip(mask).map(|(left, mask)| left & mask))?;
            }
            ("udiv" | "sdiv", [destination, left, right]) => {
                let wide = destination.is_wide();
                let value = self
                    .operand(left)?
                    .zip(self.operand(right)?)
                    .map(|(left, right)| divide(mnemonic == "sdiv", left, right, wide));
                self.assign(destination, value)?;
            }
            ("smulh" | "umulh", [destination, left, right]) => {
                let value = self
                    .operand(left)?
                    .zip(self.operand(right)?)
                    .map(|(left, right)| {
                        if mnemonic == "smulh" {
                            ((i128::from(left as i64) * i128::from(right as i64)) >> 64) as u64
                        } else {
                            ((u128::from(left) * u128::from(right)) >> 64) as u64
                        }
                    });
                self.assign(destination, value)?;
            }
            ("smull" | "umull", [destination, left, right]) => {
                let kind = if mnemonic == "smull" { "sxtw" } else { "uxtw" };
                let value = self
                    .operand(left)?
                    .zip(self.operand(right)?)
                    .map(|(left, right)| extend(kind, left).wrapping_mul(extend(kind, right)));
                self.assign(destination, value)?;
            }
            (
                "sbfiz" | "ubfiz",
                [
                    destination,
                    source,
                    Operand::Immediate(lsb),
                    Operand::Immediate(width),
                ],
            ) => {
                let wide = destination.is_wide();
                let value = self.operand(source)?.map(|value| {
                    let field = value & low_bits(*width as u64);
                    let unused = 64 - *width as u32;
                    let field = if mnemonic == "sbfiz" {
                        (((field << unused) as i64) >> unused) as u64
                    } else {
                        field
                    };
                    truncate(field << lsb, wide)
                });
                self.assign(destination, value)?;
            }
            ("mvn" | "neg", [destination, source, rest @ ..]) => {
                let value = self.modified(source, rest)?.map(|value| {
                    if mnemonic == "mvn" {
                        !value
                    } else {
                        value.wrapping_neg()
                    }
                });
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
                let widen = |value: u64| match mnemonic {
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
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// Run a comparison or a conditional select. `false` when the instruction is not one.
    fn step_condition(&mut self, mnemonic: &str, operands: &[Operand]) -> Result<bool, Halt> {
        match (mnemonic, operands) {
            ("cinc" | "cneg", [destination, source, Operand::Condition(condition)]) => {
                let value = self.operand(source)?;
                let value = if self.holds(*condition)? {
                    value.map(|value| {
                        if mnemonic == "cinc" {
                            value.wrapping_add(1)
                        } else {
                            value.wrapping_neg()
                        }
                    })
                } else {
                    value
                };
                self.assign(destination, value)?;
            }
            ("cmp" | "cmn" | "tst", [left, right, rest @ ..]) => {
                let wide = left.is_wide();
                let left = self.operand(left)?;
                let right = self.modified(right, rest)?;
                self.set_flags(
                    left.zip(right)
                        .map(|(left, right)| Flags::compare(mnemonic, left, right, wide)),
                );
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
                    self.set_flags(
                        left.zip(right)
                            .map(|(left, right)| Flags::compare(kind, left, right, wide)),
                    );
                } else {
                    self.set_flags(Some(Flags::from_bits(*fallback as u8)));
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
                    self.operand(right)?.map(|value| match mnemonic {
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
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// Run a vector or floating-point instruction. `false` when the instruction is not one.
    fn step_vector(&mut self, mnemonic: &str, operands: &[Operand]) -> Result<bool, Halt> {
        match (mnemonic, operands) {
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
                    _ => return Err(Halt::unsupported("movi-shift")),
                };
                self.vectors[*index] = Some(replicate((*value as u64) << shift, *lane, *bytes));
            }
            ("fmov", [Operand::Register(destination), Operand::Register(source)]) => {
                let value = match source.name {
                    Name::Vector(index) => self
                        .vector(index)
                        .map(|value| value & u128::from(low_bits(source.bytes * 8))),
                    _ => self.read_register(*source).map(u128::from),
                };
                match destination.name {
                    Name::Vector(index) => self.vectors[index] = value,
                    _ => {
                        let value = value.map(|value| value as u64);
                        self.assign(&Operand::Register(*destination), value)?;
                    }
                }
            }
            (
                "fmov",
                [
                    Operand::Register(Register {
                        name: Name::Vector(index),
                        bytes,
                        ..
                    }),
                    Operand::Float(value),
                ],
            ) => {
                self.vectors[*index] = Some(float_bits(*value, *bytes)?);
            }
            (
                "scvtf",
                [
                    Operand::Register(Register {
                        name: Name::Vector(index),
                        bytes,
                        ..
                    }),
                    source @ Operand::Register(Register {
                        name: Name::General(_) | Name::Zero,
                        wide,
                        ..
                    }),
                ],
            ) => {
                let bytes = *bytes;
                let value = self.operand(source)?.map(|value| {
                    let integer = sign_extend(value, if *wide { 8 } else { 4 }, true) as i64;
                    float_bits(integer as f64, bytes)
                });
                self.vectors[*index] = value.transpose()?;
            }
            (
                "fmul",
                [
                    Operand::Register(Register {
                        name: Name::Vector(index),
                        bytes,
                        ..
                    }),
                    Operand::Register(Register {
                        name: Name::Vector(left),
                        ..
                    }),
                    Operand::Register(Register {
                        name: Name::Vector(right),
                        ..
                    }),
                ],
            ) => {
                let bytes = *bytes;
                let value = self
                    .vector(*left)
                    .zip(self.vector(*right))
                    .map(|(left, right)| match bytes {
                        4 => Ok(u128::from(
                            (f32::from_bits(left as u32) * f32::from_bits(right as u32)).to_bits(),
                        )),
                        8 => Ok(u128::from(
                            (f64::from_bits(left as u64) * f64::from_bits(right as u64)).to_bits(),
                        )),
                        _ => Err(Halt::unsupported("float-width")),
                    });
                self.vectors[*index] = value.transpose()?;
            }
            (
                "fcvtzs",
                [
                    destination,
                    Operand::Register(Register {
                        name: Name::Vector(index),
                        bytes,
                        ..
                    }),
                ],
            ) => {
                let wide = destination.is_wide();
                let value = self
                    .vector(*index)
                    .map(|bits| truncated_integer(bits, *bytes, wide))
                    .transpose()?;
                self.assign(destination, value)?;
            }
            (
                "dup",
                [
                    Operand::Register(Register {
                        name: Name::Vector(index),
                        bytes,
                        lane,
                        ..
                    }),
                    source @ Operand::Register(Register {
                        name: Name::General(_) | Name::Zero,
                        ..
                    }),
                ],
            ) => {
                let value = self.operand(source)?;
                self.vectors[*index] = value.map(|value| replicate(value, *lane, *bytes));
            }
            (
                "ext",
                [
                    Operand::Register(Register {
                        name: Name::Vector(index),
                        bytes,
                        ..
                    }),
                    Operand::Register(Register {
                        name: Name::Vector(low),
                        ..
                    }),
                    Operand::Register(Register {
                        name: Name::Vector(high),
                        ..
                    }),
                    Operand::Immediate(start),
                ],
            ) => {
                let bits = *bytes as u32 * 8;
                let start = *start as u32 * 8;
                let mask = u128::MAX >> (128 - bits);
                self.vectors[*index] =
                    self.vector(*low)
                        .zip(self.vector(*high))
                        .map(|(low, high)| {
                            let low = (low & mask) >> start;
                            let high = if start == 0 {
                                0
                            } else {
                                (high & mask) << (bits - start)
                            };
                            (low | high) & mask
                        });
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// Run a load or a store. `false` when the instruction is not one.
    fn step_memory(&mut self, mnemonic: &str, operands: &[Operand]) -> Result<bool, Halt> {
        match (mnemonic, operands) {
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
                let address_inputs = self.take_inputs();
                self.vectors[*index] = address.and_then(|address| self.load_bytes(address, *bytes));
                let sources = self.loaded_inputs(address_inputs, address, *bytes);
                self.set_owner_vector(*index, sources.owner);
                if let Some(provenance) = &mut self.provenance {
                    provenance.vectors[*index] = sources.receiver;
                }
            }
            (
                "ld1r",
                [
                    Operand::Register(Register {
                        name: Name::Vector(index),
                        bytes,
                        lane,
                        ..
                    }),
                    Operand::Memory(memory),
                    rest @ ..,
                ],
            ) => {
                let address = self.address(memory, rest)?;
                let address_inputs = self.take_inputs();
                let element = address.and_then(|address| self.load_bytes(address, *lane));
                self.vectors[*index] =
                    element.map(|element| replicate(element as u64, *lane, *bytes));
                let sources = self.loaded_inputs(address_inputs, address, *lane);
                self.set_owner_vector(*index, sources.owner);
                if let Some(provenance) = &mut self.provenance {
                    provenance.vectors[*index] = sources.receiver;
                }
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
            ) => {
                let value = self.vector(*index);
                let value_inputs = self.take_inputs();
                match self.address(memory, rest)? {
                    Some(address) => {
                        self.restore_inputs(value_inputs);
                        self.store_bytes(address, *bytes, value);
                    }
                    None => self.store_to_unknown(&halves(value), memory),
                }
            }
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
                let address_inputs = self.take_inputs();
                self.vectors[*first] = address.and_then(|address| self.load_bytes(address, *bytes));
                self.vectors[*second] =
                    address.and_then(|address| self.load_bytes(address + bytes, *bytes));
                let first_sources = self.loaded_inputs(address_inputs.clone(), address, *bytes);
                let second_sources =
                    self.loaded_inputs(address_inputs, address.map(|at| at + bytes), *bytes);
                self.set_owner_vector(*first, first_sources.owner);
                self.set_owner_vector(*second, second_sources.owner);
                if let Some(provenance) = &mut self.provenance {
                    provenance.vectors[*first] = first_sources.receiver;
                    provenance.vectors[*second] = second_sources.receiver;
                }
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
            ) => {
                let first = self.vector(*first);
                let first_inputs = self.take_inputs();
                let second = self.vector(*second);
                let second_inputs = self.take_inputs();
                match self.address(memory, rest)? {
                    Some(address) => {
                        self.restore_inputs(first_inputs);
                        self.store_bytes(address, *bytes, first);
                        self.restore_inputs(second_inputs);
                        self.store_bytes(address + bytes, *bytes, second);
                    }
                    None => {
                        let mut values = halves(first).to_vec();
                        values.extend(halves(second));
                        self.store_to_unknown(&values, memory);
                    }
                }
            }
            (load, [destination, Operand::Memory(memory), rest @ ..])
                if load.starts_with("ldr") || load.starts_with("ldur") =>
            {
                let (width, signed) = load_width(load, destination)?;
                let address = self.address(memory, rest)?;
                let address_inputs = self.take_inputs();
                let value = address.and_then(|address| self.load(address, width));
                let value = value.map(|value| match signed {
                    Some(to_wide) => sign_extend(value, width, to_wide),
                    None => value,
                });
                self.restore_inputs(self.loaded_inputs(address_inputs, address, width));
                self.assign(destination, value)?;
            }
            ("ldp", [first, second, Operand::Memory(memory), rest @ ..]) => {
                let width = if first.is_wide() { 8 } else { 4 };
                let address = self.address(memory, rest)?;
                let address_inputs = self.take_inputs();
                let values = address
                    .map(|address| (self.load(address, width), self.load(address + width, width)));
                let (first_value, second_value) = values.unwrap_or((None, None));
                self.restore_inputs(self.loaded_inputs(address_inputs.clone(), address, width));
                self.assign(first, first_value)?;
                let second_address = address.map(|address| address + width);
                self.restore_inputs(self.loaded_inputs(address_inputs, second_address, width));
                self.assign(second, second_value)?;
            }
            (store, [source, Operand::Memory(memory), rest @ ..])
                if store.starts_with("str") || store.starts_with("stur") =>
            {
                let width = store_width(store, source)?;
                let value = self.operand(source)?;
                let value_inputs = self.take_inputs();
                match self.address(memory, rest)? {
                    Some(address) => {
                        self.restore_inputs(value_inputs);
                        self.store(address, width, value);
                        self.mark_register_save(source, address);
                    }
                    None => self.store_to_unknown(&[value], memory),
                }
            }
            ("stp", [first, second, Operand::Memory(memory), rest @ ..]) => {
                let width = if first.is_wide() { 8 } else { 4 };
                let first_value = self.operand(first)?;
                let first_inputs = self.take_inputs();
                let second_value = self.operand(second)?;
                let second_inputs = self.take_inputs();
                match self.address(memory, rest)? {
                    Some(address) => {
                        self.restore_inputs(first_inputs);
                        self.store(address, width, first_value);
                        self.restore_inputs(second_inputs);
                        self.store(address + width, width, second_value);
                        self.mark_register_save(first, address);
                        self.mark_register_save(second, address + width);
                    }
                    None => self.store_to_unknown(&[first_value, second_value], memory),
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// Run an atomic read-modify-write as a load, an operation and a store. `false` when the
    /// instruction is not one.
    ///
    /// The stored value takes its inputs only from what it is computed from: a swap from the
    /// source, a compare-and-swap that matches from the replacement. The register receives the
    /// loaded value. A store to an unknown address keeps the address's inputs, which tell whether
    /// it can reach the owner.
    fn step_atomic(&mut self, mnemonic: &str, operands: &[Operand]) -> Result<bool, Halt> {
        let Some((atomic, width)) = Atomic::parse(mnemonic) else {
            return Ok(false);
        };
        let (first, second, memory) = match operands {
            [first, second, Operand::Memory(memory)] => (first, Some(second), memory),
            [first, Operand::Memory(memory)] if atomic.has_store_only_form() => {
                (first, None, memory)
            }
            _ => return Err(Halt::unsupported("operand")),
        };
        let width = width.unwrap_or(if first.is_wide() { 8 } else { 4 });

        let address = self.address(memory, &[])?;
        let address_inputs = self.take_inputs();
        let old = address.and_then(|address| self.load(address, width));
        let loaded = self.loaded_inputs(address_inputs.clone(), address, width);

        let stored = match (atomic, second) {
            (Atomic::Swap, _) => {
                self.clear_inputs();
                Some(self.operand(first)?)
            }
            (Atomic::CompareAndSwap, Some(replacement)) => {
                self.restore_inputs(loaded.clone());
                let expected = self.operand(first)?;
                let matched = old
                    .zip(expected)
                    .map(|(old, expected)| old == (expected & low_bits(width * 8)));
                match matched {
                    Some(false) => None,
                    Some(true) => {
                        self.clear_inputs();
                        Some(self.operand(replacement)?)
                    }
                    None => {
                        self.operand(replacement)?;
                        Some(None)
                    }
                }
            }
            _ => {
                self.restore_inputs(loaded.clone());
                let source = self.operand(first)?;
                Some(
                    old.zip(source)
                        .map(|(old, source)| atomic.combine(old, source, width)),
                )
            }
        };

        if let Some(value) = stored {
            match address {
                Some(address) => self.store(address, width, value),
                None => {
                    self.restore_inputs(address_inputs);
                    self.store_to_unknown(&[value], memory);
                }
            }
        }

        let destination = if atomic == Atomic::CompareAndSwap {
            Some(first)
        } else {
            second
        };
        if let Some(destination) = destination {
            self.restore_inputs(loaded);
            self.assign(destination, old)?;
        }

        Ok(true)
    }

    /// Run a branch, call, return or trap, or a `nop`. Every other instruction is unsupported.
    fn step_control(
        &mut self,
        pc: u64,
        mnemonic: &str,
        operands: &[Operand],
    ) -> Result<Flow, Halt> {
        if matches!((mnemonic, operands), ("nop", [])) {
            return Ok(Flow::Next);
        }

        let branch = Branch::of(mnemonic).ok_or(Halt::unsupported("instruction"))?;
        match (branch, operands) {
            (Branch::Always, [Operand::Immediate(target)]) => {
                return Ok(Flow::Jump(*target as u64));
            }
            (Branch::OnFlags(condition), [Operand::Immediate(target)]) => {
                let condition = condition.ok_or(Halt::unsupported("branch-condition"))?;
                if self.holds(condition)? {
                    return Ok(Flow::Jump(*target as u64));
                }
            }
            (Branch::OnZero { when_zero }, [register, Operand::Immediate(target)]) => {
                let Some(value) = self.operand(register)? else {
                    return Ok(Flow::Unknown {
                        target: *target as u64,
                        halt: Halt::unknown_register("branch-value", register),
                    });
                };
                if (value == 0) == when_zero {
                    return Ok(Flow::Jump(*target as u64));
                }
            }
            (
                Branch::OnBit { when_set },
                [
                    register,
                    Operand::Immediate(bit),
                    Operand::Immediate(target),
                ],
            ) => {
                let Some(value) = self.operand(register)? else {
                    return Ok(Flow::Unknown {
                        target: *target as u64,
                        halt: Halt::unknown_register("branch-value", register),
                    });
                };
                let set = value >> bit & 1 == 1;
                if set == when_set {
                    return Ok(Flow::Jump(*target as u64));
                }
            }
            (Branch::Register, [register]) => {
                let target = self
                    .operand(register)?
                    .ok_or(Halt::unknown_register("branch-value", register))?;
                return Ok(Flow::Jump(target));
            }
            // A call puts its return address in the link register, so a `calls` closure can tell
            // call sites apart: the site is `x30 - 4`.
            (Branch::Call, [Operand::Immediate(target)]) => {
                self.registers[30] = Some(pc + 4);
                return Ok(Flow::Call(*target as u64));
            }
            (Branch::RegisterCall, [register]) => {
                let target = self.operand(register)?;
                self.registers[30] = Some(pc + 4);
                return Ok(Flow::IndirectCall(target));
            }
            (Branch::Return, []) => return Ok(Flow::Return),
            (Branch::Trap, [Operand::Immediate(_)]) => return Ok(Flow::Trap),
            _ => return Err(Halt::unsupported("instruction")),
        }
        Ok(Flow::Next)
    }

    /// Set the flags. Unknown flags may again be in any state.
    fn set_flags(&mut self, flags: Option<Flags>) {
        if let Some(provenance) = &mut self.provenance {
            provenance.flags = if flags.is_none() {
                provenance.inputs.borrow().clone()
            } else {
                BTreeSet::new()
            };
        }
        self.flags = flags;
        self.possible_flags = ALL_FLAG_STATES;
        if flags.is_none() {
            self.trace_flags();
        }
    }

    fn holds(&self, condition: Condition) -> Result<bool, Halt> {
        if let Some(flags) = self.flags {
            return Ok(condition.holds(flags));
        }

        match condition.split(self.possible_flags) {
            (_, 0) => Ok(true),
            (0, _) => Ok(false),
            _ => Err(Halt::unknown_flags()),
        }
    }

    fn operand(&self, operand: &Operand) -> Result<Option<u64>, Halt> {
        match operand {
            Operand::Immediate(value) => Ok(Some(*value as u64)),
            Operand::Register(register) => Ok(self.read_register(*register)),
            _ => Err(Halt::unsupported("operand")),
        }
    }

    /// A second source operand with its optional shift or extension.
    fn modified(&self, operand: &Operand, rest: &[Operand]) -> Result<Option<u64>, Halt> {
        let value = self.operand(operand)?;
        match rest {
            [] => Ok(value),
            [Operand::Shift(kind, amount)] => {
                let wide = operand.is_wide();
                Ok(value.map(|value| kind.apply(value, *amount, wide)))
            }
            [Operand::Extend(kind, amount)] => Ok(value.map(|value| extend(kind, value) << amount)),
            _ => Err(Halt::unsupported("operand")),
        }
    }

    fn read_register(&self, register: Register) -> Option<u64> {
        if let Name::General(index) = register.name {
            self.read_owner_register(index);
            if self.registers[index].is_none() {
                self.read_unknown_register(index);
            }
        }

        self.register_value(register)
    }

    /// The value of `register`, without recording it as an input of the present instruction.
    fn register_value(&self, register: Register) -> Option<u64> {
        let value = match register.name {
            Name::Zero => Some(0),
            Name::StackPointer => Some(self.stack_pointer),
            Name::General(index) => self.registers[index],
            Name::Vector(_) => None,
        };
        value.map(|value| truncate(value, register.wide))
    }

    fn assign(&mut self, destination: &Operand, value: Option<u64>) -> Result<(), Halt> {
        let Operand::Register(register) = destination else {
            return Err(Halt::unsupported("destination"));
        };
        let value = value.map(|value| truncate(value, register.wide));
        match register.name {
            Name::Zero => {}
            Name::StackPointer => {
                self.lose_owner_stack();
                self.stack_pointer = match value {
                    Some(value) => value,
                    None => self.stack_pointer - DYNAMIC_STACK,
                };
            }
            Name::General(index) => {
                if let Some(provenance) = &mut self.provenance {
                    provenance.registers[index] = if value.is_none() {
                        provenance.inputs.borrow().clone()
                    } else {
                        BTreeSet::new()
                    };
                }
                self.registers[index] = value;
                self.assign_owner_register(index);
                if value.is_none() {
                    self.trace_register(index);
                }
            }
            Name::Vector(_) => return Err(Halt::unsupported("destination")),
        }
        Ok(())
    }

    /// Only a known immediate offset from the current stack retains the private-frame proof.
    fn assign_stack_offset(
        &mut self,
        destination: &Operand,
        value: Option<u64>,
    ) -> Result<(), Halt> {
        if let (
            Operand::Register(Register {
                name: Name::StackPointer,
                wide: true,
                ..
            }),
            Some(value),
        ) = (destination, value)
        {
            self.stack_pointer = value;
            return Ok(());
        }
        self.assign(destination, value)
    }

    /// The effective address, applying pre- or post-index write-back to the base register.
    fn address(&mut self, memory: &Memory, rest: &[Operand]) -> Result<Option<u64>, Halt> {
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
                if memory.index.is_none() {
                    self.assign_stack_offset(&Operand::Register(memory.base), offset)?;
                } else {
                    self.assign(&Operand::Register(memory.base), offset)?;
                }
                Ok(offset)
            }
            [] => Ok(offset),
            [Operand::Immediate(post)] => {
                let updated = base.map(|base| base.wrapping_add(*post as u64));
                self.assign_stack_offset(&Operand::Register(memory.base), updated)?;
                Ok(base)
            }
            _ => Err(Halt::unsupported("addressing")),
        }
    }

    /// A store to an unknown address may overwrite any byte, so no written byte stays known,
    /// except in a protected range, or in the part of a tracked owner that the address cannot
    /// reach. A known value stored there is kept in `unknown_stores`.
    ///
    /// The address is unknown only through its base or its register index. With a known base,
    /// the base register is not written back, so its value is the base of this store.
    fn store_to_unknown(&mut self, values: &[Option<u64>], memory: &Memory) {
        self.unknown_stores.extend(values.iter().flatten());
        let protected_before = self.protected.len();
        let base = self.register_value(memory.base);
        let disjoint_owner = self.store_owner_unknown(base);
        self.protected.extend(disjoint_owner);
        let protected = &self.protected;
        let tracing = self.traces.is_some();
        let mut overwritten = Vec::new();
        for (address, byte) in self.memory.iter_mut() {
            if !protected
                .iter()
                .any(|(start, end)| (*start..*end).contains(address))
            {
                if tracing {
                    overwritten.push((*address, byte.is_some()));
                }
                *byte = None;
            }
        }
        let inherited = self.inherited_unknown_bytes(|address| {
            !self
                .protected
                .iter()
                .any(|(start, end)| (*start..*end).contains(&address))
        });
        overwritten.extend(inherited.into_iter().map(|address| (address, false)));
        self.trace_unknown_store(&overwritten);
        // The store may have written any unprotected watched byte, so a later load of one is not
        // initial receiver state.
        if let Some(watch) = &mut self.read_watch {
            for at in watch.start..watch.end {
                if !self
                    .protected
                    .iter()
                    .any(|(start, end)| (*start..*end).contains(&at))
                {
                    watch.written.insert(at);
                    if let Some(provenance) = &mut self.provenance {
                        provenance.memory.remove(&at);
                    }
                }
            }
        }
        // The owner is protected only from this store.
        self.protected.truncate(protected_before);
    }

    /// Mark the word that `source` was stored to at `address` as a register save, when `source`
    /// is a callee-saved general register and the store is in the prologue of the function that
    /// the path is in.
    fn mark_register_save(&mut self, source: &Operand, address: u64) {
        if source.is_callee_saved_general() && self.code.in_prologue(self.entered(), self.pc) {
            self.register_saves.insert(address);
        }
    }

    fn store(&mut self, address: u64, width: u64, value: Option<u64>) {
        self.store_bytes(address, width, value.map(u128::from));
    }

    fn store_bytes(&mut self, address: u64, width: u64, value: Option<u128>) {
        let derived = self.owner.as_ref().is_some_and(|owner| owner.inputs.get());
        self.note_store(address, width, derived);
        for offset in 0..width {
            let byte = value.map(|value| (value >> (offset * 8)) as u8);
            self.memory.insert(address + offset, byte);
        }
        self.record_stored_inputs(address, width, value.is_some());
    }

    /// Vector register `index`, noting an unknown one as an input of the present instruction.
    fn vector(&self, index: usize) -> Option<u128> {
        self.read_owner_vector(index);
        if self.vectors[index].is_none() {
            self.read_unknown_vector();
            if let Some(provenance) = &self.provenance {
                provenance
                    .inputs
                    .borrow_mut()
                    .extend(&provenance.vectors[index]);
            }
        }
        self.vectors[index]
    }
}

enum Flow {
    Next,
    Jump(u64),
    /// A conditional branch to `target` whose condition is unknown.
    Unknown {
        target: u64,
        halt: Halt,
    },
    Call(u64),
    /// A call through a register, whose target may be unknown.
    IndirectCall(Option<u64>),
    Return,
    Trap,
}

/// Why one instruction could not run. The run adds where, to make an [`Unresolved`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Halt {
    reason: &'static str,
    obstacle: Obstacle,
}

impl Halt {
    fn unsupported(reason: &'static str) -> Self {
        Self {
            reason,
            obstacle: Obstacle::Unsupported,
        }
    }

    fn unknown_flags() -> Self {
        Self {
            reason: "flags",
            obstacle: Obstacle::Unknown(Unknown::Flags),
        }
    }

    /// `operand` is a register whose value is unknown.
    fn unknown_register(reason: &'static str, operand: &Operand) -> Self {
        let obstacle = match operand {
            Operand::Register(Register {
                name: Name::General(index),
                ..
            }) => Obstacle::Unknown(Unknown::Register(*index as u8)),
            _ => Obstacle::Unsupported,
        };

        Self { reason, obstacle }
    }
}

fn binary(mnemonic: &str, left: u64, right: u64, wide: bool) -> u64 {
    let bits = if wide { 64 } else { 32 };
    match mnemonic {
        "add" => left.wrapping_add(right),
        "sub" => left.wrapping_sub(right),
        "and" => left & right,
        "bic" => left & !right,
        "orr" => left | right,
        "eor" => left ^ right,
        "mul" => left.wrapping_mul(right),
        "lsl" => left.wrapping_shl((right % bits) as u32),
        "lsr" => truncate(left, wide) >> (right % bits),
        "asr" => Shift::Arithmetic.apply(left, right % bits, wide),
        "ror" if wide => left.rotate_right(right as u32),
        "ror" => u64::from((left as u32).rotate_right(right as u32)),
        other => unreachable!("{other} is not a two-operand integer instruction"),
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
        "uxtx" | "sxtx" => value,
        other => unreachable!("{other} is not an extension"),
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

/// The bits of `value` as a float of `bytes` bytes, rounded to nearest as the processor does.
fn float_bits(value: f64, bytes: u64) -> Result<u128, Halt> {
    match bytes {
        4 => Ok(u128::from((value as f32).to_bits())),
        8 => Ok(u128::from(value.to_bits())),
        _ => Err(Halt::unsupported("float-width")),
    }
}

/// `fcvtzs`: the float in the low `bytes` bytes of `bits`, truncated toward zero to a signed
/// integer. Out-of-range values saturate and NaN gives zero, as on the processor.
fn truncated_integer(bits: u128, bytes: u64, wide: bool) -> Result<u64, Halt> {
    let value = match bytes {
        4 => f64::from(f32::from_bits(bits as u32)),
        8 => f64::from_bits(bits as u64),
        _ => return Err(Halt::unsupported("float-width")),
    };
    Ok(if wide {
        value as i64 as u64
    } else {
        value as i32 as u32 as u64
    })
}

/// The plain load or store of an acquire or release access. Their ordering does not change the
/// value that one path reads or writes.
fn ordered_access(mnemonic: &str) -> &str {
    match mnemonic {
        "ldar" | "ldapr" => "ldr",
        "ldarb" | "ldaprb" => "ldrb",
        "ldarh" | "ldaprh" => "ldrh",
        "stlr" => "str",
        "stlrb" => "strb",
        "stlrh" => "strh",
        other => other,
    }
}

/// An atomic read-modify-write of the large system extensions. Its acquire and release ordering
/// does not change the values that one path reads or writes, as for [`ordered_access`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Atomic {
    Add,
    Clear,
    ExclusiveOr,
    Set,
    SignedMax,
    SignedMin,
    UnsignedMax,
    UnsignedMin,
    Swap,
    CompareAndSwap,
}

impl Atomic {
    /// The operations that `ld<operation>` loads and `st<operation>` only stores.
    const OPERATIONS: [(&str, Self); 8] = [
        ("add", Self::Add),
        ("clr", Self::Clear),
        ("eor", Self::ExclusiveOr),
        ("set", Self::Set),
        ("smax", Self::SignedMax),
        ("smin", Self::SignedMin),
        ("umax", Self::UnsignedMax),
        ("umin", Self::UnsignedMin),
    ];

    /// The atomic that `mnemonic` names with any ordering suffix, and its width in bytes when a
    /// `b` or `h` suffix sets it. `None` for other instructions, such as `casp`.
    fn parse(mnemonic: &str) -> Option<(Self, Option<u64>)> {
        let (atomic, suffix) = if let Some(suffix) = mnemonic.strip_prefix("swp") {
            (Self::Swap, suffix)
        } else if let Some(suffix) = mnemonic.strip_prefix("cas") {
            (Self::CompareAndSwap, suffix)
        } else {
            let operation = mnemonic
                .strip_prefix("ld")
                .or_else(|| mnemonic.strip_prefix("st"))?;
            Self::OPERATIONS.iter().find_map(|(name, atomic)| {
                operation.strip_prefix(name).map(|suffix| (*atomic, suffix))
            })?
        };
        let (ordering, width) = match suffix.strip_suffix('b') {
            Some(ordering) => (ordering, Some(1)),
            None => match suffix.strip_suffix('h') {
                Some(ordering) => (ordering, Some(2)),
                None => (suffix, None),
            },
        };

        matches!(ordering, "" | "a" | "l" | "al").then_some((atomic, width))
    }

    /// Whether the atomic combines the loaded value with its source, so it also has a
    /// store-only form.
    fn has_store_only_form(self) -> bool {
        !matches!(self, Self::Swap | Self::CompareAndSwap)
    }

    /// The value that an operation stores from the loaded value `old` and the `source` register,
    /// compared at `width` bytes.
    fn combine(self, old: u64, source: u64, width: u64) -> u64 {
        let signed = |value| sign_extend(value, width, true) as i64;
        let unsigned = |value| value & low_bits(width * 8);
        let old_if = |keep_old: bool| if keep_old { old } else { source };
        match self {
            Self::Add => old.wrapping_add(source),
            Self::Clear => old & !source,
            Self::ExclusiveOr => old ^ source,
            Self::Set => old | source,
            Self::SignedMax => old_if(signed(old) >= signed(source)),
            Self::SignedMin => old_if(signed(old) <= signed(source)),
            Self::UnsignedMax => old_if(unsigned(old) >= unsigned(source)),
            Self::UnsignedMin => old_if(unsigned(old) <= unsigned(source)),
            Self::Swap | Self::CompareAndSwap => unreachable!("{self:?} does not combine"),
        }
    }
}

/// Integer division as the processor does it: division by zero gives zero, and the signed
/// quotient is truncated toward zero.
fn divide(signed: bool, left: u64, right: u64, wide: bool) -> u64 {
    let (left, right) = (truncate(left, wide), truncate(right, wide));
    if right == 0 {
        return 0;
    }
    if !signed {
        return left / right;
    }
    let bytes = if wide { 8 } else { 4 };
    let left = sign_extend(left, bytes, true) as i64;
    let right = sign_extend(right, bytes, true) as i64;
    truncate(left.wrapping_div(right) as u64, wide)
}

/// The two 64-bit halves of a vector value.
fn halves(value: Option<u128>) -> [Option<u64>; 2] {
    [
        value.map(|value| value as u64),
        value.map(|value| (value >> 64) as u64),
    ]
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
fn load_width(mnemonic: &str, destination: &Operand) -> Result<(u64, Option<bool>), Halt> {
    let wide = destination.is_wide();
    let register_width = if wide { 8 } else { 4 };
    let suffix = mnemonic
        .strip_prefix("ldur")
        .or_else(|| mnemonic.strip_prefix("ldr"))
        .ok_or(Halt::unsupported("instruction"))?;
    Ok(match suffix {
        "" => (register_width, None),
        "b" => (1, None),
        "h" => (2, None),
        "sb" => (1, Some(wide)),
        "sh" => (2, Some(wide)),
        "sw" => (4, Some(true)),
        _ => return Err(Halt::unsupported("instruction")),
    })
}

fn store_width(mnemonic: &str, source: &Operand) -> Result<u64, Halt> {
    let suffix = mnemonic
        .strip_prefix("stur")
        .or_else(|| mnemonic.strip_prefix("str"))
        .ok_or(Halt::unsupported("instruction"))?;
    match suffix {
        "" if source.is_wide() => Ok(8),
        "" => Ok(4),
        "b" => Ok(1),
        "h" => Ok(2),
        _ => Err(Halt::unsupported("instruction")),
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

    /// The bit of this state in a set of possible flag states.
    fn state(self) -> u16 {
        let bits = u8::from(self.negative) << 3
            | u8::from(self.zero) << 2
            | u8::from(self.carry) << 1
            | u8::from(self.overflow);
        1 << bits
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

    /// Split a set of flag states into the states where the condition holds and the states
    /// where it fails.
    fn split(self, states: u16) -> (u16, u16) {
        (0..16u8).filter(|bits| states & (1 << bits) != 0).fold(
            (0, 0),
            |(holding, failing), bits| {
                if self.holds(Flags::from_bits(bits)) {
                    (holding | 1 << bits, failing)
                } else {
                    (holding, failing | 1 << bits)
                }
            },
        )
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
enum Operation {
    Parsed {
        mnemonic: String,
        operands: Vec<Operand>,
    },
    /// A row with an operand that does not parse. The machine does not run it, so it can never be
    /// read as a different instruction. Reachability takes it to continue at the next row.
    Unparsed,
}

impl Operation {
    fn parse(row: &Instruction) -> Self {
        let operands: Option<Vec<_>> = split(&row.operands)
            .into_iter()
            .map(Operand::parse)
            .collect();
        match operands {
            Some(operands) => Self::Parsed {
                mnemonic: row.operation.clone(),
                operands,
            },
            None => Self::Unparsed,
        }
    }

    /// Whether the instruction can be part of a prologue: a stack allocation, a store of
    /// callee-saved registers to the stack, or the frame pointer set from the stack pointer.
    fn is_prologue_row(&self) -> bool {
        let Self::Parsed { mnemonic, operands } = self else {
            return false;
        };
        let on_stack =
            |memory: &Memory| memory.base.name == Name::StackPointer && memory.index.is_none();

        match (mnemonic.as_str(), operands.as_slice()) {
            ("sub", [destination, source, Operand::Immediate(_)]) => {
                destination.is_register(Name::StackPointer)
                    && source.is_register(Name::StackPointer)
            }
            ("add", [destination, source, Operand::Immediate(_)])
            | ("mov", [destination, source]) => {
                destination.is_register(Name::General(29)) && source.is_register(Name::StackPointer)
            }
            ("stp", [first, second, Operand::Memory(memory), ..]) => {
                first.is_callee_saved() && second.is_callee_saved() && on_stack(memory)
            }
            ("str", [source, Operand::Memory(memory), ..]) => {
                source.is_callee_saved() && on_stack(memory)
            }
            _ => false,
        }
    }

    /// The instructions that can run next by direct control flow, or `None` for a branch through
    /// a register. A call continues after itself.
    fn successors(&self, address: u64) -> Option<Vec<u64>> {
        let next = address + 4;
        let Self::Parsed { mnemonic, operands } = self else {
            return Some(vec![next]);
        };

        let target = operands.iter().rev().find_map(|operand| match operand {
            Operand::Immediate(target) => Some(*target as u64),
            _ => None,
        });
        Some(match Branch::of(mnemonic) {
            None | Some(Branch::Call | Branch::RegisterCall) => vec![next],
            Some(Branch::Always) => target.into_iter().collect(),
            Some(Branch::OnFlags(_) | Branch::OnZero { .. } | Branch::OnBit { .. }) => {
                target.into_iter().chain([next]).collect()
            }
            Some(Branch::Register) => return None,
            Some(Branch::Return | Branch::Trap) => Vec::new(),
        })
    }

    /// The condition that the instruction tests, when it tests one.
    fn condition(&self) -> Option<Condition> {
        let Self::Parsed { mnemonic, operands } = self else {
            return None;
        };
        if let Some(Branch::OnFlags(condition)) = Branch::of(mnemonic) {
            return condition;
        }

        operands.iter().find_map(|operand| match operand {
            Operand::Condition(condition) => Some(*condition),
            _ => None,
        })
    }
}

/// How an instruction passes control other than to the next instruction. Both the machine and the
/// reachability of the decoded code read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Branch {
    /// `b`: to its target.
    Always,
    /// `b.<condition>`: to its target when the flags satisfy the condition. `None` for a condition
    /// that does not parse.
    OnFlags(Option<Condition>),
    /// `cbz` or `cbnz`: to its target when the register is zero, or when it is not.
    OnZero { when_zero: bool },
    /// `tbz` or `tbnz`: to its target when the tested bit is clear, or when it is set.
    OnBit { when_set: bool },
    /// `br`: to the address in a register.
    Register,
    /// `bl`: a call to its target that returns to the next instruction.
    Call,
    /// `blr`: a call to the address in a register that returns to the next instruction.
    RegisterCall,
    /// `ret`: out of the present function.
    Return,
    /// `brk`: a trap that does not return.
    Trap,
}

impl Branch {
    /// The branch that `mnemonic` makes, or `None` when it is not a branch, call, return or trap.
    fn of(mnemonic: &str) -> Option<Self> {
        if let Some(condition) = mnemonic.strip_prefix("b.") {
            return Some(Self::OnFlags(Condition::parse(condition)));
        }

        Some(match mnemonic {
            "b" => Self::Always,
            "cbz" => Self::OnZero { when_zero: true },
            "cbnz" => Self::OnZero { when_zero: false },
            "tbz" => Self::OnBit { when_set: false },
            "tbnz" => Self::OnBit { when_set: true },
            "br" => Self::Register,
            "bl" => Self::Call,
            "blr" => Self::RegisterCall,
            "ret" => Self::Return,
            "brk" => Self::Trap,
            _ => return None,
        })
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
    /// A floating-point immediate, such as the `#1.50000000` of `fmov s8,#1.50000000`.
    Float(f64),
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
        if let Some(single) = text
            .strip_prefix('{')
            .and_then(|rest| rest.strip_suffix('}'))
        {
            return Register::parse(single).map(Self::Register);
        }
        if let Some(value) = text.strip_prefix('#') {
            if value.contains('.') {
                return value.parse().ok().map(Self::Float);
            }
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

    /// Whether this is a callee-saved general register, `x19` to `x30`.
    fn is_callee_saved_general(&self) -> bool {
        matches!(
            self,
            Self::Register(Register {
                name: Name::General(19..=30),
                wide: true,
                ..
            })
        )
    }

    /// Whether this is a callee-saved register: `x19` to `x30`, or `d8` to `d15`.
    fn is_callee_saved(&self) -> bool {
        self.is_callee_saved_general()
            || matches!(
                self,
                Self::Register(Register {
                    name: Name::Vector(8..=15),
                    bytes: 8,
                    ..
                })
            )
    }

    fn is_register(&self, name: Name) -> bool {
        matches!(self, Self::Register(register) if register.name == name)
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
    /// `value` shifted as a register of the given width. An arithmetic shift copies the sign bit
    /// of that width.
    fn apply(self, value: u64, amount: u64, wide: bool) -> u64 {
        match self {
            Self::Left => value.wrapping_shl(amount as u32),
            Self::Right => value.wrapping_shr(amount as u32),
            Self::Arithmetic => {
                let bytes = if wide { 8 } else { 4 };
                (sign_extend(truncate(value, wide), bytes, true) as i64 >> amount) as u64
            }
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
            _ => (
                Name::General(general_register(text)?),
                text.starts_with('x'),
            ),
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
    #[test]
    fn read_watch_excludes_written_bytes_in_a_mixed_load() {
        let code = super::Code::default();
        let data = super::ReadOnlyData::default();
        let mut machine = super::Machine::new(&code, &data);
        let command = machine.reserve(16);
        machine.write(command + 4, 4, 7);
        machine.watch_reads(command, 16);
        machine.write_unknown(command, 4);
        assert_eq!(machine.load(command, 8), None);
        assert_eq!(
            machine.receiver_reads(),
            std::collections::BTreeMap::from([((4, 4), Some(7))])
        );
        machine.load(command - 4, 8);
        assert_eq!(machine.receiver_reads().len(), 1);
    }

    #[test]
    fn a_store_through_an_unknown_address_ends_initial_receiver_reads() {
        let code = super::Code::default();
        let data = super::ReadOnlyData::default();
        let mut machine = super::Machine::new(&code, &data);
        let command = machine.reserve(16);
        machine.write(command + 4, 4, 7);
        machine.watch_reads(command, 16);
        let unknown_base = super::Memory {
            base: super::Register {
                name: super::Name::General(1),
                wide: true,
                bytes: 8,
                lane: 8,
            },
            index: None,
            offset: 0,
            write_back: false,
        };
        machine.store_to_unknown(&[None], &unknown_base);
        machine.load(command + 4, 4);
        assert!(machine.receiver_reads().is_empty());
    }

    use super::*;
    use crate::engine::analysis::assembler::arm64;

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

    #[test]
    fn words_overlay_sections_and_extend_outside_them() {
        let data = ReadOnlyData::new(vec![(0x1000, vec![0xaa; 0x10])]);
        let words = BTreeMap::from([
            (0x1008, 0x1122_3344_5566_7788),
            (0x100c, 0x99), // crosses the section's end
            (0x2000, 0x2222),
            (0x2008, 0x3333),
            (0x3000, 0x4444),
        ]);
        let overlaid = data.with_words(&words);
        assert_eq!(overlaid.read(0x1000, 8), Some(0xaaaa_aaaa_aaaa_aaaa));
        assert_eq!(overlaid.read(0x1008, 8), Some(0x1122_3344_5566_7788));
        assert_eq!(overlaid.read(0x2000, 8), Some(0x2222));
        assert_eq!(overlaid.read(0x2008, 8), Some(0x3333));
        assert_eq!(overlaid.read(0x2010, 1), None);
        assert_eq!(overlaid.read(0x3000, 8), Some(0x4444));
        assert_eq!(data.read(0x1008, 8), Some(0xaaaa_aaaa_aaaa_aaaa));
    }

    /// A stop of a run that entered at 0x100.
    fn stop_at(reason: &'static str, instruction: u64, obstacle: Obstacle) -> Unresolved {
        Unresolved::at(reason, instruction, 0x100, obstacle)
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
    fn rotate_right_wraps_within_the_register_width() {
        let code = rows(&[(0x100, "ror", "w0,w0,#3"), (0x104, "ret", "")]);
        let data = ReadOnlyData::default();
        assert_eq!(returned(&code, &data, 0b1011), Ok(Some(0x6000_0001)));
        assert_eq!(returned(&code, &data, 0x1_0000_0008), Ok(Some(1)));
    }

    #[test]
    fn ands_and_bics_set_the_flags_of_their_result() {
        let code = rows(&[
            (0x100, "ands", "x8,x0,#0x80000"),
            (0x104, "cset", "w9,eq"),
            (0x108, "mov", "w10,#0x6"),
            (0x10c, "bics", "wzr,w10,w0"),
            (0x110, "cset", "w11,ne"),
            (0x114, "add", "x0,x8,x9,lsl#1"),
            (0x118, "add", "x0,x0,x11"),
            (0x11c, "ret", ""),
        ]);
        let data = ReadOnlyData::default();
        assert_eq!(returned(&code, &data, 0x80006), Ok(Some(0x80000)));
        assert_eq!(returned(&code, &data, 0x2), Ok(Some(0b11)));
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
        assert_eq!(
            returned(&code, &data, 2),
            Err(stop_at(
                "branch-value",
                0x114,
                Obstacle::Unknown(Unknown::Register(10))
            ))
        );
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
        assert_eq!(
            returned(&branch, &data, 0),
            Err(stop_at(
                "branch-value",
                0x100,
                Obstacle::Unknown(Unknown::Register(3))
            ))
        );
        let unknown = rows(&[(0x100, "fmla", "s0,s1,s2")]);
        assert_eq!(
            returned(&unknown, &data, 0),
            Err(stop_at("instruction", 0x100, Obstacle::Unsupported))
        );
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
            Err(stop_at("outside-code", 0x200, Obstacle::OutsideCode))
        );
        let spin = rows(&[(0x100, "b", "#0x100")]);
        assert_eq!(
            returned(&spin, &data, 0),
            Err(stop_at(
                "step-limit",
                0x100,
                Obstacle::Bound(Bound::Steps(STEP_LIMIT))
            ))
        );
    }

    #[test]
    fn decoded_movi_fills_every_lane() {
        let bytes = arm64!(at 0x100;
            movi v0.d2, #0xffffffffffffffff;
            str q0, [x1];
            movi v0.d2, #0;
            str q0, [x1, #0x10];
            ret
        );
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
    fn atomics_return_the_old_value_and_store_the_result() {
        let bytes = arm64!(at 0x100;
            ldaddal x8, x9, [x0];
            ldaddal w10, w11, [x1]; // carries out of 32 bits
            ldaddh w12, w13, [x2]; // carries out of 16 bits
            ldaddal x14, x14, [x3];
            stadd w15, [x4];
            swpal x16, x17, [x5];
            ret
        );
        let code = Code::decode(&[(0x100, &bytes)]).unwrap();
        let data = ReadOnlyData::default();
        let mut machine = Machine::new(&code, &data);
        let objects: Vec<_> = (0..6).map(|_| machine.allocate(16)).collect();
        for (index, &object) in objects.iter().enumerate() {
            machine.set_register(index, object);
            machine.write(object, 8, 5);
            machine.write(object + 8, 8, 7);
        }
        machine.write(objects[1], 4, 0xffff_fffe);
        machine.write(objects[2], 2, 0xfffe);
        for register in [8, 10, 12, 14, 15] {
            machine.set_register(register, 3);
        }
        machine.set_register(16, 9);

        assert_eq!(
            machine.run(0x100, &mut |_, _| Ok(Call::Return(None))),
            Ok(Exit::Returned)
        );
        assert!(matches!(
            code.rows.get(&0x110),
            Some(Operation::Parsed { mnemonic, .. }) if mnemonic == "stadd"
        ));
        assert_eq!(
            (machine.register(9), machine.read(objects[0], 8)),
            (Some(5), Some(8))
        );
        assert_eq!(machine.register(11), Some(0xffff_fffe));
        assert_eq!(machine.read(objects[1], 8), Some(1));
        assert_eq!(machine.register(13), Some(0xfffe));
        assert_eq!(machine.read(objects[2], 8), Some(1));
        assert_eq!(
            (machine.register(14), machine.read(objects[3], 8)),
            (Some(5), Some(8))
        );
        assert_eq!(machine.read(objects[4], 8), Some(8));
        assert_eq!(
            (machine.register(17), machine.read(objects[5], 8)),
            (Some(5), Some(9))
        );
        assert!(
            objects
                .iter()
                .all(|object| machine.read(object + 8, 8) == Some(7))
        );
    }

    #[test]
    fn a_compare_and_swap_stores_only_when_memory_matches() {
        let bytes = arm64!(at 0x100;
            casal x8, x9, [x0];
            cas w10, w11, [x1];
            ret
        );
        let code = Code::decode(&[(0x100, &bytes)]).unwrap();
        let data = ReadOnlyData::default();
        let mut machine = Machine::new(&code, &data);
        let matching = machine.allocate(8);
        let differing = machine.allocate(8);
        machine.write(matching, 8, 5);
        machine.write(differing, 8, 5);
        machine.set_register(0, matching);
        machine.set_register(1, differing);
        machine.set_register(8, 5);
        machine.set_register(9, 7);
        machine.set_register(10, 6);
        machine.set_register(11, 7);

        assert_eq!(
            machine.run(0x100, &mut |_, _| Ok(Call::Return(None))),
            Ok(Exit::Returned)
        );
        assert_eq!(
            (machine.register(8), machine.read(matching, 8)),
            (Some(5), Some(7))
        );
        assert_eq!(
            (machine.register(10), machine.read(differing, 8)),
            (Some(5), Some(5))
        );
    }

    #[test]
    fn an_atomic_at_an_unknown_address_loads_an_unknown_value() {
        let bytes = arm64!(at 0x100; ldaddal x8, x9, [x0]; ret);
        let code = Code::decode(&[(0x100, &bytes)]).unwrap();
        let data = ReadOnlyData::default();
        let mut machine = Machine::new(&code, &data);
        machine.set_register(8, 3);
        machine.set_register(9, 1);

        assert_eq!(
            machine.run(0x100, &mut |_, _| Ok(Call::Return(None))),
            Ok(Exit::Returned)
        );
        assert_eq!(machine.register(9), None);
    }

    #[test]
    fn atomic_mnemonics_name_their_operation_and_width() {
        let cases = [
            ("ldaddal", Some((Atomic::Add, None))),
            ("ldclrab", Some((Atomic::Clear, Some(1)))),
            ("ldsminlh", Some((Atomic::SignedMin, Some(2)))),
            ("stumaxl", Some((Atomic::UnsignedMax, None))),
            ("steorh", Some((Atomic::ExclusiveOr, Some(2)))),
            ("swpab", Some((Atomic::Swap, Some(1)))),
            ("casal", Some((Atomic::CompareAndSwap, None))),
            ("caslh", Some((Atomic::CompareAndSwap, Some(2)))),
            ("casp", None),
            ("caspal", None),
            ("ldclrp", None),
            ("ldaddx", None),
            ("ldapr", None),
            ("stlr", None),
            ("str", None),
        ];
        for (mnemonic, expected) in cases {
            assert_eq!(Atomic::parse(mnemonic), expected, "{mnemonic}");
        }
    }

    #[test]
    fn atomic_operations_compare_at_their_width_and_sign() {
        let cases = [
            (Atomic::Add, 5, 3, 8, 8),
            (Atomic::Clear, 0b1111, 0b0101, 8, 0b1010),
            (Atomic::ExclusiveOr, 0b1100, 0b1010, 8, 0b0110),
            (Atomic::Set, 0b1100, 0b0011, 8, 0b1111),
            (Atomic::SignedMax, 0x80, 1, 1, 1),
            (Atomic::UnsignedMax, 0x80, 1, 1, 0x80),
            (Atomic::SignedMin, 0x80, 1, 1, 0x80),
            (Atomic::UnsignedMin, 0x80, 1, 1, 1),
            (Atomic::SignedMax, 0xffff_ffff, 0, 4, 0),
            (Atomic::UnsignedMax, 0xffff_ffff, 0, 4, 0xffff_ffff),
            (Atomic::SignedMin, 0xffff_ffff, 0, 8, 0),
        ];
        for (atomic, old, source, width, expected) in cases {
            assert_eq!(
                atomic.combine(old, source, width),
                expected,
                "{atomic:?} {old:#x} {source:#x} at {width}"
            );
        }
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
            Err(stop_at("flags", 0x104, Obstacle::Unknown(Unknown::Flags))),
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
    fn an_arithmetic_shifted_operand_keeps_the_sign_of_its_register_width() {
        let bytes = arm64!(at 0x100;
            add w1, wzr, w0, asr #1;
            asr w2, w0, #1;
            add x3, xzr, x0, asr #1;
            cmp wzr, w0, asr #1;
            b.gt extern 0x11c;
            mov x0, #1;
            ret;
            mov x0, #2; // at 0x11c: zero is greater than a negative w0
            ret
        );
        let code = Code::decode(&[(0x100, &bytes)]).unwrap();
        let data = ReadOnlyData::default();
        let run = |input: u64| {
            let mut machine = Machine::new(&code, &data);
            machine.set_register(0, input);
            machine
                .run(0x100, &mut |_, _| Ok(Call::Return(None)))
                .unwrap();
            [1, 2, 3, 0].map(|register| machine.register(register))
        };

        assert_eq!(
            run(0x8000_0000),
            [
                Some(0xc000_0000),
                Some(0xc000_0000),
                Some(0x4000_0000),
                Some(2)
            ]
        );
        assert_eq!(
            run(0x4000_0000),
            [
                Some(0x2000_0000),
                Some(0x2000_0000),
                Some(0x2000_0000),
                Some(1)
            ]
        );
        assert_eq!(
            run(0x8000_0000_0000_0000),
            [Some(0), Some(0), Some(0xc000_0000_0000_0000), Some(1)]
        );
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
            .map(|path| (path.end.clone(), path.machine.register(0)))
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
        assert_eq!(
            returned(&trap, &data, 0),
            Err(stop_at("instruction", 0x100, Obstacle::Unsupported))
        );
    }

    #[test]
    fn a_later_decision_on_the_same_flags_follows_every_remaining_state() {
        let code = rows(&[
            (0x100, "cmp", "x3,x4"),
            (0x104, "b.ge", "#0x118"),
            (0x108, "b.eq", "#0x114"),
            (0x10c, "mov", "x0,#1"),
            (0x110, "ret", ""),
            (0x114, "mov", "x0,#2"),
            (0x118, "b.lt", "#0x124"),
            (0x11c, "mov", "x0,#3"),
            (0x120, "ret", ""),
            (0x124, "mov", "x0,#4"),
            (0x128, "ret", ""),
        ]);
        let data = ReadOnlyData::default();
        let paths = Machine::new(&code, &data).run_paths(0x100, &mut |_, _| Ok(Call::Return(None)));

        // `ge` false with `eq` true is possible (Z set, N and V differ), so 0x114 is reached,
        // and there `lt` must hold. Where `ge` held, `lt` cannot hold. So exactly three paths.
        assert_eq!(returned_values(&paths), [Some(1), Some(3), Some(4)]);
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
    fn paths_to_a_site_stop_before_it_and_drop_paths_that_cannot_reach_it() {
        let code = rows(&[
            (0x100, "cbz", "x3,#0x110"),
            (0x104, "mov", "x1,#1"),
            (0x108, "bl", "#0x900"),
            (0x10c, "ret", ""),
            (0x110, "mov", "x1,#2"),
            (0x114, "ret", ""),
        ]);
        let data = ReadOnlyData::default();
        let paths = Machine::new(&code, &data)
            .run_paths_to(0x100, 0x108, &mut |_, _| Ok(Call::Return(None)));

        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].end, Ok(Exit::Reached));
        assert_eq!(paths[0].machine.register(1), Some(1));
    }

    #[test]
    fn a_store_to_an_unknown_address_keeps_protected_bytes_and_records_its_value() {
        let code = rows(&[
            (0x100, "mov", "x9,#0x55"),
            (0x104, "str", "x9,[x3]"),
            (0x108, "ret", ""),
        ]);
        let data = ReadOnlyData::default();
        let mut machine = Machine::new(&code, &data);
        let protected = machine.allocate(8);
        let other = machine.allocate(8);
        machine.write(protected, 8, 7);
        machine.write(other, 8, 9);
        machine.protect(protected, 8);
        machine
            .run(0x100, &mut |_, _| Ok(Call::Return(None)))
            .unwrap();

        assert_eq!(machine.read(protected, 8), Some(7));
        assert_eq!(machine.read(other, 8), None);
        assert_eq!(machine.unknown_stores(), [0x55]);

        machine.release(protected);
        machine
            .run(0x100, &mut |_, _| Ok(Call::Return(None)))
            .unwrap();
        assert_eq!(machine.read(protected, 8), None);
    }

    #[test]
    fn flag_setting_division_multiply_and_bitfield_instructions_compute_their_values() {
        let code = rows(&[
            (0x100, "mov", "x1,#10"),
            (0x104, "subs", "x2,x1,#10"),
            (0x108, "cset", "w3,eq"),
            (0x10c, "mov", "w4,#-7"),
            (0x110, "mov", "w5,#2"),
            (0x114, "sdiv", "w6,w4,w5"),
            (0x118, "udiv", "x7,x1,xzr"),
            (0x11c, "smull", "x8,w4,w5"),
            (0x120, "mov", "x9,#-1"),
            (0x124, "umulh", "x10,x9,x9"),
            (0x128, "mov", "w11,#5"),
            (0x12c, "sbfiz", "x12,x11,#3,#3"),
            (0x130, "ubfiz", "x13,x11,#3,#3"),
            (0x134, "neg", "x14,x1"),
            (0x138, "cinc", "x15,x1,eq"),
            (0x13c, "ldarb", "w16,[x17]"),
            (0x140, "ret", ""),
        ]);
        let data = ReadOnlyData::default();
        let mut machine = Machine::new(&code, &data);
        let byte = machine.allocate(1);
        machine.write(byte, 1, 0x2a);
        machine.set_register(17, byte);
        machine
            .run(0x100, &mut |_, _| Ok(Call::Return(None)))
            .unwrap();

        assert_eq!(machine.register(2), Some(0));
        assert_eq!(machine.register(3), Some(1));
        assert_eq!(machine.register(6), Some((-3i32) as u32 as u64));
        assert_eq!(machine.register(7), Some(0));
        assert_eq!(machine.register(8), Some((-14i64) as u64));
        assert_eq!(machine.register(10), Some(u64::MAX - 1));
        assert_eq!(machine.register(12), Some((-24i64) as u64));
        assert_eq!(machine.register(13), Some(40));
        assert_eq!(machine.register(14), Some((-10i64) as u64));
        assert_eq!(machine.register(15), Some(11));
        assert_eq!(machine.register(16), Some(0x2a));
    }

    /// The growth rule of an engine array: capacity times 1.5, truncated toward zero.
    #[test]
    fn float_growth_is_exact() {
        let code = rows(&[
            (0x100, "fmov", "s8,#1.50000000"),
            (0x104, "scvtf", "s0,w0"),
            (0x108, "fmul", "s0,s0,s8"),
            (0x10c, "fcvtzs", "w0,s0"),
            (0x110, "ret", ""),
        ]);
        let data = ReadOnlyData::default();
        assert_eq!(returned(&code, &data, 3), Ok(Some(4)));
        assert_eq!(returned(&code, &data, 0), Ok(Some(0)));
        // -1 times 1.5 truncates toward zero to -1; a product above the range saturates.
        assert_eq!(
            returned(&code, &data, u32::MAX as u64),
            Ok(Some(u32::MAX as u64))
        );
        assert_eq!(returned(&code, &data, 0x7fff_ffff), Ok(Some(0x7fff_ffff)));
    }

    #[test]
    fn a_call_puts_its_return_address_in_the_link_register() {
        let code = rows(&[
            (0x100, "bl", "#0x900"),
            (0x104, "blr", "x8"),
            (0x108, "ret", ""),
        ]);
        let data = ReadOnlyData::default();
        let mut machine = Machine::new(&code, &data);
        machine.set_register(8, 0x800);
        let mut sites = Vec::new();
        machine
            .run(0x100, &mut |_, machine| {
                sites.push(machine.register(30).map(|link| link - 4));
                Ok(Call::Return(None))
            })
            .unwrap_err();
        assert_eq!(sites, [Some(0x100)]);

        let mut sites = Vec::new();
        let paths = Machine::new(&code, &data).run_paths(0x100, &mut |_, machine| {
            sites.push(machine.register(30).map(|link| link - 4));
            Ok(Call::Return(None))
        });
        assert_eq!(paths.len(), 1);
        assert_eq!(sites, [Some(0x100), Some(0x104)]);
    }

    #[test]
    fn reserved_memory_is_unknown_until_stored() {
        let code = rows(&[]);
        let data = ReadOnlyData::default();
        let mut machine = Machine::new(&code, &data);
        let reserved = machine.reserve(0x40);
        let allocated = machine.allocate(8);
        assert!(allocated >= reserved + 0x40);
        assert_eq!(machine.read(reserved + 0x10, 8), None);
        assert_eq!(machine.read(allocated, 8), Some(0));
        machine.write(reserved + 0x10, 8, 7);
        assert_eq!(machine.read(reserved + 0x10, 8), Some(7));
    }

    #[test]
    fn register_moves_between_general_and_vector_registers_keep_their_bits() {
        let code = rows(&[
            (0x100, "mov", "x1,#0x1234"),
            (0x104, "fmov", "d0,x1"),
            (0x108, "fmov", "x2,d0"),
            (0x10c, "dup", "v1.2d,x1"),
            (0x110, "mov", "x3,#0x55"),
            (0x114, "dup", "v2.2d,x3"),
            (0x118, "ext", "v3.16b,v1.16b,v2.16b,#8"),
            (0x11c, "str", "q3,[x4]"),
            (0x120, "ret", ""),
        ]);
        let data = ReadOnlyData::default();
        let mut machine = Machine::new(&code, &data);
        let stored = machine.allocate(16);
        machine.set_register(4, stored);
        machine
            .run(0x100, &mut |_, _| Ok(Call::Return(None)))
            .unwrap();

        assert_eq!(machine.register(2), Some(0x1234));
        assert_eq!(machine.read(stored, 8), Some(0x1234));
        assert_eq!(machine.read(stored + 8, 8), Some(0x55));
    }

    #[test]
    fn a_replicating_load_fills_every_lane_with_one_element() {
        let code = rows(&[
            (0x100, "ld1r", "{v0.2d},[x1]"),
            (0x104, "str", "q0,[x2]"),
            (0x108, "ld1r", "{v1.4s},[x1],#4"),
            (0x10c, "str", "q1,[x3]"),
            (0x110, "ret", ""),
        ]);
        let data = ReadOnlyData::new(vec![(
            0x8000,
            0x1122_3344_5566_7788u64.to_le_bytes().to_vec(),
        )]);
        let mut machine = Machine::new(&code, &data);
        let doubles = machine.allocate(16);
        let words = machine.allocate(16);
        machine.set_register(1, 0x8000);
        machine.set_register(2, doubles);
        machine.set_register(3, words);
        machine
            .run(0x100, &mut |_, _| Ok(Call::Return(None)))
            .unwrap();

        assert_eq!(machine.read(doubles, 8), Some(0x1122_3344_5566_7788));
        assert_eq!(machine.read(doubles + 8, 8), Some(0x1122_3344_5566_7788));
        assert_eq!(machine.read(words, 8), Some(0x5566_7788_5566_7788));
        assert_eq!(machine.read(words + 8, 8), Some(0x5566_7788_5566_7788));
        assert_eq!(machine.register(1), Some(0x8004));
    }

    /// A prologue that saves `d8` and `d9` first, then `x19` and `x20`, then the frame record,
    /// followed by `body` from 0x110.
    fn saves_then(body: &[(u64, &str, &str)]) -> Code {
        let mut lines = vec![
            (0x100, "stp", "d9,d8,[sp,#-0x30]!"),
            (0x104, "stp", "x20,x19,[sp,#0x10]"),
            (0x108, "stp", "x29,x30,[sp,#0x20]"),
            (0x10c, "add", "x29,sp,#0x20"),
        ];
        lines.extend_from_slice(body);
        rows(&lines)
    }

    fn run_saving<'a>(code: &'a Code, data: &'a ReadOnlyData) -> Machine<'a> {
        let mut machine = Machine::new(code, data);
        machine.set_register(9, 7);
        machine.set_register(19, 0x19);
        machine.set_register(20, 0x20);
        machine
            .run(0x100, &mut |_, _| Ok(Call::Return(None)))
            .unwrap();
        machine
    }

    #[test]
    fn a_prologue_store_of_a_callee_saved_register_is_a_register_save() {
        let code = saves_then(&[
            (0x110, "mov", "x19,x0"),
            (0x114, "str", "x19,[sp,#0x8]"), // a body store of a callee-saved register
            (0x118, "ret", ""),
        ]);
        let data = ReadOnlyData::default();
        let machine = run_saving(&code, &data);

        for word in [0x10, 0x18, 0x20, 0x28] {
            assert!(
                machine.is_register_save(STACK_TOP - 0x30 + word),
                "{word:#x}"
            );
        }
        assert!(!machine.is_register_save(STACK_TOP - 0x30));
        assert!(!machine.is_register_save(STACK_TOP - 0x28));
    }

    #[test]
    fn a_later_store_over_a_register_save_ends_it() {
        let code = saves_then(&[
            (0x110, "str", "x9,[sp,#0x10]"),
            (0x114, "strb", "w9,[sp,#0x1f]"),
            (0x118, "ret", ""),
        ]);
        let data = ReadOnlyData::default();
        let mut machine = run_saving(&code, &data);
        let frame = STACK_TOP - 0x30;

        assert!(!machine.is_register_save(frame + 0x10));
        assert!(!machine.is_register_save(frame + 0x18));
        assert!(machine.is_register_save(frame + 0x20));
        machine.copy_bytes(frame + 0x24, frame, 4);
        assert!(!machine.is_register_save(frame + 0x20));
    }

    /// A store in the loop body writes the same bytes over the save of `x20`. The head runs again
    /// for the path without the save, so a path leaves the loop without it.
    #[test]
    fn a_join_keeps_only_the_register_saves_of_both_paths() {
        let code = rows(&[
            (0x100, "stp", "x20,x19,[sp,#-0x20]!"),
            (0x104, "cbz", "x1,#0x110"),
            (0x108, "str", "x20,[sp]"),
            (0x10c, "b", "#0x104"),
            (0x110, "bl", "#0x900"),
            (0x114, "ret", ""),
        ]);
        let data = ReadOnlyData::default();
        let mut machine = Machine::new(&code, &data);
        machine.set_register(20, 0x20);
        let paths = machine.run_paths_joining(0x100, &mut |_, _| Ok(Call::Return(None)));
        let returned: Vec<&Machine<'_>> = paths
            .iter()
            .filter(|path| path.end == Ok(Exit::Returned))
            .map(|path| &path.machine)
            .collect();

        assert!(
            returned
                .iter()
                .any(|machine| !machine.is_register_save(STACK_TOP - 0x20))
        );
        assert!(
            returned
                .iter()
                .all(|machine| machine.is_register_save(STACK_TOP - 0x18))
        );
    }

    #[test]
    fn watched_stack_updates_require_immediate_offsets() {
        for (mnemonic, operands, lost) in [
            ("sub", "sp,sp,#32", false),
            ("add", "sp,sp,#32", false),
            ("stp", "x19,x20,[sp,#-16]!", false),
            ("ldp", "x19,x20,[sp],#16", false),
            ("mov", "sp,x1", true),
            ("add", "sp,sp,x1", true),
            ("sub", "sp,sp,x2", true),
        ] {
            let code = rows(&[(0x100, mnemonic, operands), (0x104, "ret", "")]);
            let data = ReadOnlyData::default();
            let mut machine = Machine::new(&code, &data);
            let owner = machine.reserve(64);
            machine.set_register(1, 16);
            machine.track_owner(owner, owner + 64);
            machine
                .run(0x100, &mut |_, _| Ok(Call::Return(None)))
                .unwrap();
            assert_eq!(machine.owner_stack_lost(), lost, "{mnemonic} {operands}");
        }
    }

    #[test]
    fn an_unknown_stack_pointer_moves_the_stack_to_memory_that_no_path_wrote() {
        let code = rows(&[
            (0x100, "mov", "x9,#7"),
            (0x104, "str", "x9,[sp,#-0x10]"),
            (0x108, "sub", "x19,sp,#0x10"),
            (0x10c, "mov", "sp,x3"),
            (0x110, "ldr", "x0,[sp,#-0x10]"),
            (0x114, "ldr", "x1,[x19]"),
            (0x118, "ret", ""),
        ]);
        let data = ReadOnlyData::default();
        let mut machine = Machine::new(&code, &data);
        machine
            .run(0x100, &mut |_, _| Ok(Call::Return(None)))
            .unwrap();

        assert_eq!(machine.register(0), None);
        assert_eq!(machine.register(1), Some(7));
    }

    /// Calls each path made to `target`, counted in the path's label `target`.
    fn count_calls(
        target: u64,
    ) -> impl FnMut(Option<u64>, &mut Machine) -> Result<Call, Unresolved> {
        move |called, machine| {
            if called == Some(target) {
                let count = machine.labelled(target).unwrap_or(0);
                machine.label(target, count + 1);
            }
            Ok(Call::Return(None))
        }
    }

    /// A loop that counts `x0` up to three and then calls `0x900`. The joined state forgets the
    /// counter, so the run follows the exit although the first pass cannot take it.
    #[test]
    fn a_joining_run_follows_the_code_after_a_counted_loop() {
        let code = rows(&[
            (0x100, "mov", "x0,#0"),
            (0x104, "add", "x0,x0,#1"),
            (0x108, "cmp", "x0,#3"),
            (0x10c, "b.ne", "#0x104"),
            (0x110, "bl", "#0x900"),
            (0x114, "ret", ""),
        ]);
        let data = ReadOnlyData::default();
        let paths = Machine::new(&code, &data).run_paths_joining(0x100, &mut count_calls(0x900));

        assert!(paths.iter().all(|path| path.end.is_ok()));
        assert!(paths.iter().any(|path| path.end == Ok(Exit::Looped)));
        assert!(
            paths
                .iter()
                .any(|path| path.end == Ok(Exit::Returned)
                    && path.machine.labelled(0x900) == Some(1))
        );
    }

    /// The first pass sets `x5`; only a later pass calls `0x904`, because of it. The run follows
    /// that later pass.
    #[test]
    fn a_joining_run_follows_a_later_pass_that_differs_from_the_first() {
        let code = rows(&[
            (0x100, "mov", "x5,#0"),
            (0x104, "cbnz", "x5,#0x110"),
            (0x108, "mov", "x5,#1"),
            (0x10c, "b", "#0x114"),
            (0x110, "bl", "#0x904"),
            (0x114, "cbnz", "x6,#0x104"),
            (0x118, "ret", ""),
        ]);
        let data = ReadOnlyData::default();
        let mut called = BTreeSet::new();
        let paths = Machine::new(&code, &data).run_paths_joining(0x100, &mut |target, _| {
            called.extend(target);
            Ok(Call::Return(None))
        });

        assert!(paths.iter().all(|path| path.end.is_ok()));
        assert!(paths.iter().any(|path| path.end == Ok(Exit::Looped)));
        assert!(called.contains(&0x904));

        let mut first_pass = BTreeSet::new();
        Machine::new(&code, &data).run_paths_to(0x100, 0x114, &mut |target, _| {
            first_pass.extend(target);
            Ok(Call::Return(None))
        });
        assert!(first_pass.is_empty(), "the first pass never calls 0x904");
    }

    /// The loop head is a branch on unknown flags. Both sides of the split resume at the head,
    /// and neither may end as covered by the arrival that split.
    #[test]
    fn a_joining_run_follows_both_sides_of_a_branch_at_a_loop_head() {
        let code = rows(&[
            (0x100, "cmp", "x7,#0"),
            (0x104, "b.eq", "#0x10c"),
            (0x108, "b", "#0x104"),
            (0x10c, "bl", "#0x900"),
            (0x110, "ret", ""),
        ]);
        let data = ReadOnlyData::default();
        let mut called = BTreeSet::new();
        let paths = Machine::new(&code, &data).run_paths_joining(0x100, &mut |target, _| {
            called.extend(target);
            Ok(Call::Return(None))
        });

        assert!(paths.iter().any(|path| path.end == Ok(Exit::Returned)));
        assert!(paths.iter().any(|path| path.end == Ok(Exit::Looped)));
        assert!(called.contains(&0x900));
    }

    /// Paths with different labels are not joined: each goes on until it passes the loop head
    /// too often.
    #[test]
    fn a_joining_run_keeps_paths_with_different_labels_apart() {
        let code = rows(&[
            (0x100, "bl", "#0x900"),
            (0x104, "cbnz", "x6,#0x100"),
            (0x108, "ret", ""),
        ]);
        let data = ReadOnlyData::default();
        let paths = Machine::new(&code, &data).run_paths_joining(0x100, &mut count_calls(0x900));

        let limit = Obstacle::Bound(Bound::LoopArrivals(LOOP_LIMIT));
        assert!(
            paths
                .iter()
                .any(|path| path.end == Err(stop_at("loop-limit", 0x100, limit)))
        );
        assert!(!paths.iter().any(|path| path.end == Ok(Exit::Looped)));
    }

    #[test]
    fn a_path_that_loops_too_often_ends_so_that_other_paths_reach_the_site() {
        let code = rows(&[
            (0x100, "cbz", "x3,#0x110"),
            (0x104, "ldr", "x3,[x3]"),
            (0x108, "cbnz", "x4,#0x100"),
            (0x10c, "b", "#0x100"),
            (0x110, "mov", "x1,#1"),
            (0x114, "bl", "#0x900"),
            (0x118, "ret", ""),
        ]);
        let data = ReadOnlyData::default();
        let paths = Machine::new(&code, &data)
            .run_paths_to(0x100, 0x114, &mut |_, _| Ok(Call::Return(None)));

        assert!(paths.iter().any(|path| path.end == Ok(Exit::Reached)));
        let limit = Obstacle::Bound(Bound::LoopArrivals(LOOP_LIMIT));
        assert!(
            paths
                .iter()
                .any(|path| path.end == Err(stop_at("loop-limit", 0x100, limit)))
        );
        assert!(paths.len() < PATH_LIMIT);
    }

    #[test]
    fn paths_stop_at_the_path_limit() {
        let code = rows(&[(0x100, "cbz", "x3,#0x100"), (0x104, "b", "#0x100")]);
        let data = ReadOnlyData::default();
        let paths = Machine::new(&code, &data).run_paths(0x100, &mut |_, _| Ok(Call::Return(None)));

        assert!(paths.len() <= PATH_LIMIT);
        let limit = Obstacle::Bound(Bound::Paths(PATH_LIMIT));
        assert!(
            paths
                .iter()
                .any(|path| path.end == Err(stop_at("path-limit", 0x100, limit)))
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
            .run_paths(0x100, &mut |_, _| Err(Unresolved::new("unknown-callee")));
        assert_eq!(
            refused[0].end,
            Err(stop_at("unknown-callee", 0x100, Obstacle::Call))
        );

        assert_eq!(
            Machine::new(&code, &data).run(0x100, &mut |_, _| Ok(Call::Stop)),
            Err(stop_at("outside-code", 0x900, Obstacle::OutsideCode))
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

        let stopped = Machine::new(&code, &data).run_paths(0x100, &mut |target, _| {
            Ok(match target {
                Some(_) => Call::Stop,
                None => Call::Return(None),
            })
        });
        assert_eq!(stopped[0].end, Ok(Exit::Stopped(0x900)));
        assert_eq!(
            returned(&code, &data, 0),
            Err(stop_at("instruction", 0x100, Obstacle::Unsupported)),
            "a single run still refuses an indirect call"
        );
    }

    #[test]
    fn a_single_run_still_refuses_an_unknown_condition() {
        let code = rows(&[(0x100, "b.eq", "#0x100")]);
        let data = ReadOnlyData::default();

        assert_eq!(
            returned(&code, &data, 0),
            Err(stop_at("flags", 0x100, Obstacle::Unknown(Unknown::Flags)))
        );
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

    /// Runs `code` from 0x100 with every call to an address in `entered` entered and every other
    /// call returning its target, and gives each path's end with its `x0`, ordered by `x0`.
    fn entered_paths(code: &Code, entered: &[u64]) -> Vec<(Result<Exit, Unresolved>, Option<u64>)> {
        let data = ReadOnlyData::default();
        let mut paths: Vec<_> = Machine::new(code, &data)
            .run_paths(0x100, &mut |target, _| {
                Ok(match target {
                    Some(target) if entered.contains(&target) => Call::Enter,
                    target => Call::Return(target),
                })
            })
            .into_iter()
            .map(|path| (path.end, path.machine.register(0)))
            .collect();
        paths.sort_by_key(|(_, value)| *value);
        paths
    }

    #[test]
    fn an_entered_call_runs_the_callee_and_returns_after_the_call() {
        let code = rows(&[
            (0x100, "bl", "#0x200"),
            (0x104, "add", "x0,x0,#1"),
            (0x108, "ret", ""),
            (0x200, "bl", "#0x300"),
            (0x204, "add", "x0,x0,#0x10"),
            (0x208, "ret", ""),
            (0x300, "mov", "x0,#0x100"),
            (0x304, "ret", ""),
        ]);

        assert_eq!(
            entered_paths(&code, &[0x200, 0x300]),
            [(Ok(Exit::Returned), Some(0x111))]
        );
        assert_eq!(
            entered_paths(&code, &[0x200]),
            [(Ok(Exit::Returned), Some(0x311))]
        );
    }

    #[test]
    fn a_fork_inside_an_entered_call_returns_to_the_caller_on_each_path() {
        let code = rows(&[
            (0x100, "bl", "#0x200"),
            (0x104, "add", "x0,x0,#1"),
            (0x108, "ret", ""),
            (0x200, "mov", "x0,#0x10"),
            (0x204, "cbz", "x5,#0x20c"),
            (0x208, "mov", "x0,#0x20"),
            (0x20c, "ret", ""),
        ]);

        assert_eq!(
            entered_paths(&code, &[0x200]),
            [
                (Ok(Exit::Returned), Some(0x11)),
                (Ok(Exit::Returned), Some(0x21))
            ]
        );
    }

    /// Runs `code` from 0x100 to the site at 0x10c with the call to 0x200 entered, and gives
    /// each path's end with the `x1` that it holds there, ordered by `x1`.
    fn entered_paths_to_site(code: &Code) -> Vec<(Result<Exit, Unresolved>, Option<u64>)> {
        let data = ReadOnlyData::default();
        let mut paths: Vec<_> = Machine::new(code, &data)
            .run_paths_to(0x100, 0x10c, &mut |target, _| {
                Ok(match target {
                    Some(0x200) => Call::Enter,
                    _ => Call::Return(None),
                })
            })
            .into_iter()
            .map(|path| (path.end, path.machine.register(1)))
            .collect();
        paths.sort_by_key(|(_, value)| *value);
        paths
    }

    #[test]
    fn a_call_entered_before_the_site_returns_to_it_with_its_writes() {
        let code = rows(&[
            (0x100, "sub", "sp,sp,#0x20"),
            (0x104, "bl", "#0x200"),
            (0x108, "ldr", "x1,[sp]"),
            (0x10c, "bl", "#0x900"),
            (0x110, "ret", ""),
            (0x200, "mov", "x9,#7"),
            (0x204, "str", "x9,[sp]"),
            (0x208, "ret", ""),
        ]);

        assert_eq!(entered_paths_to_site(&code), [(Ok(Exit::Reached), Some(7))]);
    }

    #[test]
    fn each_branch_of_a_call_entered_before_the_site_returns_to_it() {
        let code = rows(&[
            (0x100, "sub", "sp,sp,#0x20"),
            (0x104, "bl", "#0x200"),
            (0x108, "ldr", "x1,[sp]"),
            (0x10c, "bl", "#0x900"),
            (0x110, "ret", ""),
            (0x200, "cbz", "x5,#0x20c"),
            (0x204, "mov", "x9,#7"),
            (0x208, "b", "#0x210"),
            (0x20c, "mov", "x9,#8"),
            (0x210, "str", "x9,[sp]"),
            (0x214, "ret", ""),
        ]);

        assert_eq!(
            entered_paths_to_site(&code),
            [(Ok(Exit::Reached), Some(7)), (Ok(Exit::Reached), Some(8))]
        );
    }

    #[test]
    fn an_entered_call_that_never_returns_does_not_reach_the_site() {
        let code = rows(&[
            (0x100, "sub", "sp,sp,#0x20"),
            (0x104, "bl", "#0x200"),
            (0x108, "ldr", "x1,[sp]"),
            (0x10c, "bl", "#0x900"),
            (0x110, "ret", ""),
            (0x200, "b", "#0x200"),
        ]);

        assert!(
            entered_paths_to_site(&code)
                .iter()
                .all(|(end, _)| *end != Ok(Exit::Reached))
        );
    }

    /// A run of the called function from inside an entered call returns where that function
    /// returns, not into the code that entered the call.
    #[test]
    fn a_run_outside_the_entered_calls_ends_at_its_own_return() {
        let code = rows(&[
            (0x100, "bl", "#0x200"),
            (0x104, "mov", "x0,#9"),
            (0x108, "ret", ""),
            (0x200, "bl", "#0x300"),
            (0x204, "ret", ""),
            (0x300, "mov", "x0,#5"),
            (0x304, "ret", ""),
        ]);
        let data = ReadOnlyData::default();
        let run_callee = |machine: Machine<'_>| -> Vec<Option<u64>> {
            machine
                .run_paths(0x300, &mut |_, _| Ok(Call::Return(None)))
                .iter()
                .filter(|path| path.end == Ok(Exit::Returned))
                .map(|path| path.machine.register(0))
                .collect()
        };
        let mut inherited = Vec::new();
        let mut own = Vec::new();
        Machine::new(&code, &data).run_paths(0x100, &mut |target, machine| {
            Ok(match target {
                Some(0x200) => Call::Enter,
                Some(0x300) => {
                    inherited = run_callee(machine.clone());
                    own = run_callee(machine.without_entered_calls());
                    Call::Return(None)
                }
                _ => Call::Return(None),
            })
        });

        assert_eq!(inherited, [Some(9)]);
        assert_eq!(own, [Some(5)]);
    }

    #[test]
    fn a_tail_call_inside_an_entered_call_returns_from_that_call_only() {
        let code = rows(&[
            (0x100, "bl", "#0x200"),
            (0x104, "bl", "#0x900"),
            (0x108, "ret", ""),
            (0x200, "b", "#0x800"),
        ]);
        let data = ReadOnlyData::default();
        let mut calls = Vec::new();
        let paths = Machine::new(&code, &data).run_paths(0x100, &mut |target, machine| {
            calls.push((target, machine.entered_calls().collect::<Vec<_>>()));
            Ok(match target {
                Some(0x200) => Call::Enter,
                target => Call::Return(target),
            })
        });

        assert_eq!(
            calls,
            [
                (Some(0x200), vec![]),
                (Some(0x800), vec![0x100]),
                (Some(0x900), vec![])
            ]
        );
        assert_eq!(returned_values(&paths), [Some(0x900)]);
    }

    #[test]
    fn a_tail_jump_into_an_entered_function_returns_to_the_first_caller() {
        let code = rows(&[
            (0x100, "bl", "#0x200"),
            (0x104, "add", "x0,x0,#1"),
            (0x108, "ret", ""),
            (0x200, "b", "#0x300"),
            (0x300, "mov", "x0,#0x40"),
            (0x304, "ret", ""),
        ]);

        assert_eq!(
            entered_paths(&code, &[0x200]),
            [(Ok(Exit::Returned), Some(0x41))]
        );
        assert_eq!(
            entered_paths(
                &rows(&[(0x100, "bl", "#0x200"), (0x104, "ret", "")]),
                &[0x200]
            ),
            [(
                Err(Unresolved::at(
                    "outside-code",
                    0x200,
                    0x200,
                    Obstacle::OutsideCode
                )),
                None
            )]
        );
    }

    /// Runs `ranges` from 0x100 along every path. A call to 0x200 is entered; every other call
    /// returns an unknown value.
    fn authored_paths(ranges: &[(u64, &[u8])]) -> Vec<Result<Exit, Unresolved>> {
        let code = Code::decode(ranges).unwrap();
        let data = ReadOnlyData::default();

        Machine::new(&code, &data)
            .run_paths(0x100, &mut |target, _| {
                Ok(match target {
                    Some(0x200) => Call::Enter,
                    _ => Call::Return(None),
                })
            })
            .into_iter()
            .map(|path| path.end)
            .collect()
    }

    #[test]
    fn a_stop_at_an_unknown_value_names_the_register_and_the_entered_function() {
        let caller = arm64!(at 0x100;
            bl extern 0x200;
            br x9 // x9 is unknown after the call
        );
        let callee = arm64!(at 0x200; mov x0, #1; ret);
        assert_eq!(
            authored_paths(&[(0x100, &caller), (0x200, &callee)]),
            [Err(stop_at(
                "branch-value",
                0x104,
                Obstacle::Unknown(Unknown::Register(9))
            ))]
        );

        let callee = arm64!(at 0x200; br x3);
        assert_eq!(
            authored_paths(&[(0x100, &caller), (0x200, &callee)]),
            [Err(Unresolved::at(
                "branch-value",
                0x200,
                0x200,
                Obstacle::Unknown(Unknown::Register(3))
            ))]
        );
    }

    #[test]
    fn a_stop_at_a_spent_bound_names_the_bound() {
        let spin = arm64!(at 0x100; b extern 0x100);
        assert_eq!(
            authored_paths(&[(0x100, &spin)]),
            [Err(stop_at(
                "step-limit",
                0x100,
                Obstacle::Bound(Bound::Steps(STEP_LIMIT))
            ))]
        );

        let fork = arm64!(at 0x100;
            add x0, x0, #1;
            cbz x3, extern 0x100 // x3 is unknown, so every pass forks
        );
        let ends = authored_paths(&[(0x100, &fork)]);
        let limit = Obstacle::Bound(Bound::Paths(PATH_LIMIT));
        assert!(ends.contains(&Err(stop_at("path-limit", 0x104, limit))));
    }

    #[test]
    fn a_closure_names_the_unknown_register_at_its_call() {
        let code = Code::decode(&[(0x100, &arm64!(at 0x100; nop; bl extern 0x900; ret))]).unwrap();
        let data = ReadOnlyData::default();
        let paths = Machine::new(&code, &data).run_paths(0x100, &mut |_, machine| {
            machine.known_register(1, "argument")?;
            Ok(Call::Return(None))
        });

        assert_eq!(
            paths[0].end,
            Err(stop_at(
                "argument",
                0x104,
                Obstacle::Unknown(Unknown::Register(1))
            ))
        );
    }

    #[test]
    fn a_read_at_a_reached_site_names_the_site() {
        let code = Code::decode(&[(
            0x100,
            &arm64!(at 0x100;
                mov x1, #1;
                b extern 0x10c;
                nop;
                bl extern 0x900 // the site; x2 is unknown here
            ),
        )])
        .unwrap();
        let data = ReadOnlyData::default();
        let paths =
            Machine::new(&code, &data).run_paths_to(0x100, 0x10c, &mut |_, _| Ok(Call::Stop));

        assert_eq!(paths[0].end, Ok(Exit::Reached));
        assert_eq!(
            paths[0].machine.known_register(2, "argument"),
            Err(stop_at(
                "argument",
                0x10c,
                Obstacle::Unknown(Unknown::Register(2))
            ))
        );
    }

    #[test]
    fn a_run_that_a_closure_makes_does_not_change_the_outer_entry() {
        let outer = arm64!(at 0x100;
            bl extern 0x200; // the closure runs 0x200 itself
            br x9 // x9 is unknown after the call
        );
        let inner = arm64!(at 0x200; mov x0, #1; ret);
        let code = Code::decode(&[(0x100, &outer), (0x200, &inner)]).unwrap();
        let data = ReadOnlyData::default();
        let paths = Machine::new(&code, &data).run_paths(0x100, &mut |target, machine| {
            let exit = machine.run(target.unwrap(), &mut |_, _| Ok(Call::Stop))?;
            assert_eq!(exit, Exit::Returned);
            Ok(Call::Return(machine.register(0)))
        });

        assert_eq!(
            paths[0].end,
            Err(stop_at(
                "branch-value",
                0x104,
                Obstacle::Unknown(Unknown::Register(9))
            ))
        );
    }
}
